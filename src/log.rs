//! Global configuration of FFmpeg's internal logging.
//!
//! FFmpeg keeps a single process-global logger. This module bridges it to the crate's public
//! [`LogSetting`] API: either adjust the level of FFmpeg's own stderr handler, or install a
//! Rust callback. The callback is invoked from whatever thread FFmpeg happens to log on
//! (including internal codec worker threads), so all shared state here is synchronized.

use std::ffi::{c_char, c_int, c_void};
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Mutex, PoisonError, RwLock};

use crate::sys;
use crate::{LogCallback, LogLevel, LogMessage, LogSetting};

/// Matches FFmpeg's own `LINE_SZ`; long messages are formatted (and, if necessary, truncated)
/// into a buffer of this size before being reassembled into complete lines.
const LINE_SIZE: usize = 1024;

/// The user-provided callback, if one is installed. `None` means FFmpeg's default handler is
/// active. Read on every log call, written only from [`set`].
static CALLBACK: RwLock<Option<LogCallback>> = RwLock::new(None);

/// FFmpeg logs a single message as one or more fragments, and only the last fragment of a
/// logical line ends in `'\n'`. This accumulates fragments until a line is complete, mirroring
/// the buffering FFmpeg's default handler performs internally.
struct LineState {
    /// The line assembled so far, across fragments.
    buf: String,
    /// Level of the fragment that started the current line; reported for the whole line.
    level: LogLevel,
    /// FFmpeg's in/out prefix flag, persisted across fragments (`1` before a fresh line, set to
    /// `0` by `av_log_format_line2` while a line is mid-assembly).
    print_prefix: c_int,
}

impl LineState {
    const fn new() -> Self {
        Self {
            buf: String::new(),
            level: LogLevel::Info,
            print_prefix: 1,
        }
    }
}

static LINE: Mutex<LineState> = Mutex::new(LineState::new());

/// Applies a [`LogSetting`] to FFmpeg's global logger.
pub(crate) fn set(setting: LogSetting) {
    // Discard any half-assembled line so a switch never fuses leftover fragments onto the next
    // message. Reset the prefix flag to FFmpeg's fresh-line default.
    {
        let mut state = LINE.lock().unwrap_or_else(PoisonError::into_inner);
        state.buf.clear();
        state.print_prefix = 1;
    }
    match setting {
        LogSetting::Level(level) => {
            // Drop any custom callback and restore FFmpeg's default stderr handler, which is the
            // component that actually enforces the level threshold.
            *CALLBACK.write().unwrap_or_else(PoisonError::into_inner) = None;
            // SAFETY: both are plain FFI setters with no aliasing or lifetime requirements;
            // av_log_default_callback is a valid FFmpeg-provided callback pointer.
            unsafe {
                sys::av_log_set_level(level.to_av());
                sys::av_log_set_callback(Some(sys::av_log_default_callback));
            }
        }
        LogSetting::Callback(cb) => {
            *CALLBACK.write().unwrap_or_else(PoisonError::into_inner) = Some(cb);
            // SAFETY: `trampoline` has the exact signature FFmpeg expects for a log callback.
            unsafe {
                sys::av_log_set_callback(Some(trampoline));
            }
        }
    }
}

/// The C callback handed to `av_log_set_callback`. FFmpeg calls it with a `va_list`; rather than
/// interpret the varargs ourselves (which is not portable), we hand them straight to
/// `av_log_format_line2`, which renders the message exactly as the default handler would.
///
/// # Safety
///
/// Called only by FFmpeg with the contract of `av_log_set_callback`: `fmt` is a valid
/// printf-style format string and `vl` its matching `va_list`. Must be thread-safe.
unsafe extern "C" fn trampoline(
    avcl: *mut c_void,
    level: c_int,
    fmt: *const c_char,
    vl: *mut sys::__va_list_tag,
) {
    // Reassemble fragments into whole lines under a single lock. The user callback is invoked
    // only after the lock is released, so a callback that itself logs cannot deadlock here.
    let flushed = {
        let mut state = LINE.lock().unwrap_or_else(PoisonError::into_inner);

        let mut line = [0 as c_char; LINE_SIZE];
        // SAFETY: `line` is a writable buffer of LINE_SIZE, `print_prefix` a valid int pointer,
        // and `fmt`/`vl` are forwarded verbatim from FFmpeg.
        let written = unsafe {
            sys::av_log_format_line2(
                avcl,
                level,
                fmt,
                vl,
                line.as_mut_ptr(),
                LINE_SIZE as c_int,
                &mut state.print_prefix,
            )
        };
        if written < 0 {
            return;
        }

        // `written` is the length that would have been produced; clamp to what fit in the buffer.
        let truncated = written as usize >= LINE_SIZE;
        let len = (written as usize).min(LINE_SIZE - 1);
        // SAFETY: av_log_format_line2 wrote `len` bytes plus a NUL; the bytes are a valid C
        // string body. Interpreting them as UTF-8 lossily never reads out of bounds.
        let fragment = {
            let bytes = unsafe { std::slice::from_raw_parts(line.as_ptr() as *const u8, len) };
            String::from_utf8_lossy(bytes)
        };

        if state.buf.is_empty() {
            state.level = LogLevel::from_av(level);
        }
        state.buf.push_str(&fragment);

        // A line is complete once FFmpeg emits its trailing newline; a truncated fragment is
        // treated as a complete line since its own newline was cut off.
        if fragment.ends_with('\n') || truncated {
            let text = state.buf.trim_end_matches(['\r', '\n']).to_owned();
            let msg = LogMessage {
                level: state.level,
                text,
            };
            state.buf.clear();
            Some(msg)
        } else {
            None
        }
    };

    let Some(msg) = flushed else { return };

    let guard = CALLBACK.read().unwrap_or_else(PoisonError::into_inner);
    if let Some(cb) = guard.as_ref() {
        // A panic must not unwind across the FFI boundary back into FFmpeg.
        let _ = panic::catch_unwind(AssertUnwindSafe(|| cb(&msg)));
    }
}
