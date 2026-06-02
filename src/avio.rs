//! Custom AVIO contexts that back in-memory byte sources and sinks.

use std::ffi::c_void;
use std::marker::PhantomData;
use std::os::raw::c_int;
use std::ptr::{self, NonNull};

use crate::error::Error;
use crate::sys;

// Size of the bounce buffer FFmpeg uses between its internal state and our
// callbacks. 4 KiB matches FFmpeg's own default for avio_open.
const AVIO_BUFFER_SIZE: usize = 4096;

// libc whence values and FFmpeg's seek extensions. These are plain C macros not
// captured by the bindgen allowlist, so they are restated here.
const SEEK_SET: c_int = 0;
const SEEK_CUR: c_int = 1;
const SEEK_END: c_int = 2;
const AVSEEK_SIZE: c_int = 0x10000;
const AVSEEK_FORCE: c_int = 0x20000;

// Resolve a (whence, offset) pair against a cursor of length `len` currently at
// `pos`, returning the new absolute position, or None when the request is
// invalid. `allow_past_end` permits seeking beyond the current length (writers
// extend on the next write); readers reject it.
fn resolve_seek(
    pos: usize,
    len: usize,
    offset: i64,
    whence: c_int,
    allow_past_end: bool,
) -> Option<i64> {
    let base = match whence & !AVSEEK_FORCE {
        SEEK_SET => 0,
        SEEK_CUR => pos as i64,
        SEEK_END => len as i64,
        _ => return None,
    };
    let target = base.checked_add(offset)?;
    if target < 0 || (!allow_past_end && target as u64 > len as u64) {
        return None;
    }
    Some(target)
}

// ===== read =====

// A read cursor over a borrowed input slice. The raw pointer / length are kept
// instead of a slice so the type can cross the C callback boundary; the borrow
// is tracked by ReadAvio's lifetime parameter.
struct ReadCursor {
    data: *const u8,
    len: usize,
    pos: usize,
}

unsafe extern "C" fn read_packet(opaque: *mut c_void, buf: *mut u8, buf_size: c_int) -> c_int {
    // SAFETY: opaque is the Box<ReadCursor> pointer installed in ReadAvio::new and
    // stays valid for the lifetime of the AVIOContext. buf points to at least
    // buf_size bytes provided by FFmpeg.
    unsafe {
        let cur = &mut *(opaque as *mut ReadCursor);
        if buf_size <= 0 || cur.pos >= cur.len {
            return sys::AVERROR_EOF;
        }
        let n = (cur.len - cur.pos).min(buf_size as usize);
        ptr::copy_nonoverlapping(cur.data.add(cur.pos), buf, n);
        cur.pos += n;
        n as c_int
    }
}

unsafe extern "C" fn read_seek(opaque: *mut c_void, offset: i64, whence: c_int) -> i64 {
    // SAFETY: opaque is the Box<ReadCursor> pointer installed in ReadAvio::new.
    unsafe {
        let cur = &mut *(opaque as *mut ReadCursor);
        if whence & !AVSEEK_FORCE == AVSEEK_SIZE {
            return cur.len as i64;
        }
        match resolve_seek(cur.pos, cur.len, offset, whence, false) {
            Some(target) => {
                cur.pos = target as usize;
                target
            }
            None => sys::AVERROR(sys::EINVAL) as i64,
        }
    }
}

/// A read-only [`sys::AVIOContext`] streaming from an in-memory byte slice. The
/// borrow of the input is tracked so the context cannot outlive its data.
pub(crate) struct ReadAvio<'a> {
    ctx: NonNull<sys::AVIOContext>,
    cursor: *mut ReadCursor,
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> ReadAvio<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Result<Self, Error> {
        // SAFETY: av_malloc returns either null or a buffer of the requested size,
        // which avio_alloc_context takes ownership of (freed via ctx->buffer on Drop).
        // The cursor box outlives the context and is reclaimed on Drop.
        unsafe {
            let buffer = sys::av_malloc(AVIO_BUFFER_SIZE) as *mut u8;
            if buffer.is_null() {
                return Err(Error::OutOfMemory);
            }
            let cursor = Box::into_raw(Box::new(ReadCursor {
                data: data.as_ptr(),
                len: data.len(),
                pos: 0,
            }));
            let ctx = sys::avio_alloc_context(
                buffer,
                AVIO_BUFFER_SIZE as c_int,
                0,
                cursor as *mut c_void,
                Some(read_packet),
                None,
                Some(read_seek),
            );
            if ctx.is_null() {
                let mut b = buffer as *mut c_void;
                sys::av_freep(&mut b as *mut *mut c_void as *mut c_void);
                drop(Box::from_raw(cursor));
                return Err(Error::OutOfMemory);
            }
            Ok(ReadAvio {
                ctx: NonNull::new_unchecked(ctx),
                cursor,
                _marker: PhantomData,
            })
        }
    }

    pub(crate) fn as_ptr(&self) -> *mut sys::AVIOContext {
        self.ctx.as_ptr()
    }
}

impl Drop for ReadAvio<'_> {
    fn drop(&mut self) {
        // SAFETY: ctx is valid by construction. The internal buffer (possibly
        // reallocated by FFmpeg) is freed before the context, then the cursor box
        // is reclaimed. The owner guarantees any AVFormatContext referencing this
        // AVIO has already been closed.
        unsafe {
            let mut buf = (*self.ctx.as_ptr()).buffer as *mut c_void;
            sys::av_freep(&mut buf as *mut *mut c_void as *mut c_void);
            let mut p = self.ctx.as_ptr();
            sys::avio_context_free(&mut p);
            drop(Box::from_raw(self.cursor));
        }
    }
}

// ===== write =====

// A growable sink that the write callback appends to. `pos` tracks the muxer's
// current write position so header back-patching (seek + overwrite) works.
struct WriteSink {
    data: Vec<u8>,
    pos: usize,
}

unsafe extern "C" fn write_packet(opaque: *mut c_void, buf: *const u8, buf_size: c_int) -> c_int {
    // SAFETY: opaque is the Box<WriteSink> pointer installed in WriteAvio::new.
    // buf points to at least buf_size readable bytes.
    unsafe {
        let sink = &mut *(opaque as *mut WriteSink);
        if buf_size <= 0 {
            return 0;
        }
        let n = buf_size as usize;
        let end = sink.pos + n;
        if end > sink.data.len() {
            sink.data.resize(end, 0);
        }
        ptr::copy_nonoverlapping(buf, sink.data.as_mut_ptr().add(sink.pos), n);
        sink.pos = end;
        n as c_int
    }
}

unsafe extern "C" fn write_seek(opaque: *mut c_void, offset: i64, whence: c_int) -> i64 {
    // SAFETY: opaque is the Box<WriteSink> pointer installed in WriteAvio::new.
    unsafe {
        let sink = &mut *(opaque as *mut WriteSink);
        if whence & !AVSEEK_FORCE == AVSEEK_SIZE {
            return sink.data.len() as i64;
        }
        match resolve_seek(sink.pos, sink.data.len(), offset, whence, true) {
            Some(target) => {
                sink.pos = target as usize;
                target
            }
            None => sys::AVERROR(sys::EINVAL) as i64,
        }
    }
}

/// A writable [`sys::AVIOContext`] that accumulates muxer output in a `Vec<u8>`,
/// supporting the seeks muxers issue to back-patch container headers.
pub(crate) struct WriteAvio {
    ctx: NonNull<sys::AVIOContext>,
    sink: *mut WriteSink,
}

impl WriteAvio {
    pub(crate) fn new() -> Result<Self, Error> {
        // SAFETY: mirrors ReadAvio::new; write_flag is 1 and a write/seek callback
        // pair is installed. The sink box outlives the context.
        unsafe {
            let buffer = sys::av_malloc(AVIO_BUFFER_SIZE) as *mut u8;
            if buffer.is_null() {
                return Err(Error::OutOfMemory);
            }
            let sink = Box::into_raw(Box::new(WriteSink {
                data: Vec::new(),
                pos: 0,
            }));
            let ctx = sys::avio_alloc_context(
                buffer,
                AVIO_BUFFER_SIZE as c_int,
                1,
                sink as *mut c_void,
                None,
                Some(write_packet),
                Some(write_seek),
            );
            if ctx.is_null() {
                let mut b = buffer as *mut c_void;
                sys::av_freep(&mut b as *mut *mut c_void as *mut c_void);
                drop(Box::from_raw(sink));
                return Err(Error::OutOfMemory);
            }
            Ok(WriteAvio {
                ctx: NonNull::new_unchecked(ctx),
                sink,
            })
        }
    }

    pub(crate) fn as_ptr(&self) -> *mut sys::AVIOContext {
        self.ctx.as_ptr()
    }

    /// Takes the accumulated bytes out of the sink. The caller must have flushed
    /// the context (e.g. via [`sys::avio_flush`] after the muxer trailer) first.
    pub(crate) fn into_bytes(self) -> Vec<u8> {
        // SAFETY: sink is valid by construction; taking the Vec leaves an empty one
        // behind for Drop to reclaim with the box.
        unsafe { std::mem::take(&mut (*self.sink).data) }
    }
}

impl Drop for WriteAvio {
    fn drop(&mut self) {
        // SAFETY: same cleanup contract as ReadAvio::drop.
        unsafe {
            let mut buf = (*self.ctx.as_ptr()).buffer as *mut c_void;
            sys::av_freep(&mut buf as *mut *mut c_void as *mut c_void);
            let mut p = self.ctx.as_ptr();
            sys::avio_context_free(&mut p);
            drop(Box::from_raw(self.sink));
        }
    }
}
