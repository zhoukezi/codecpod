//! RAII wrappers for FFmpeg resources shared across modules.

use std::ptr::NonNull;

use crate::error::Error;
use crate::sys;

// INVARIANT: self.0 points to the AVPacket returned by av_packet_alloc.
pub(crate) struct Packet(pub(crate) NonNull<sys::AVPacket>);

impl Packet {
    pub(crate) fn new() -> Result<Self, Error> {
        // SAFETY: av_packet_alloc has no preconditions.
        let ptr = unsafe { sys::av_packet_alloc() };
        NonNull::new(ptr).map(Packet).ok_or(Error::OutOfMemory)
    }
}

impl Drop for Packet {
    fn drop(&mut self) {
        // SAFETY: The type invariant guarantees self.0 is valid. av_packet_free takes &mut *mut
        // to null out the caller's pointer; since NonNull cannot hold null, a local variable is
        // used instead.
        unsafe {
            let mut p = self.0.as_ptr();
            sys::av_packet_free(&mut p);
        }
    }
}

// INVARIANT: self.0 points to the AVFrame returned by av_frame_alloc.
pub(crate) struct Frame(pub(crate) NonNull<sys::AVFrame>);

impl Frame {
    pub(crate) fn new() -> Result<Self, Error> {
        // SAFETY: av_frame_alloc has no preconditions.
        let ptr = unsafe { sys::av_frame_alloc() };
        NonNull::new(ptr).map(Frame).ok_or(Error::OutOfMemory)
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        // SAFETY: The type invariant guarantees self.0 is valid. av_frame_free takes &mut *mut
        // to null out the caller's pointer; since NonNull cannot hold null, a local variable is
        // used instead.
        unsafe {
            let mut p = self.0.as_ptr();
            sys::av_frame_free(&mut p);
        }
    }
}
