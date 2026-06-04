#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(dead_code)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(unnecessary_transmutes)]
#![allow(unsafe_op_in_unsafe_fn)]

#[cfg(not(docsrs))]
include!(concat!(env!("OUT_DIR"), "/ffmpeg.rs"));

// docs.rs stub
#[cfg(docsrs)]
mod docsrs {
    pub const AV_SAMPLE_FMT_U8: i32 = 0;
    pub const AV_SAMPLE_FMT_S16: i32 = 1;
    pub const AV_SAMPLE_FMT_S32: i32 = 2;
    pub const AV_SAMPLE_FMT_FLT: i32 = 3;
    pub const AV_SAMPLE_FMT_DBL: i32 = 4;
    pub const AV_SAMPLE_FMT_S16P: i32 = 6;
    pub const AV_SAMPLE_FMT_S32P: i32 = 7;

    pub const AV_ERROR_MAX_STRING_SIZE: i32 = 64;

    unsafe extern "C" {
        pub fn av_strerror(errnum: i32, errbuf: *mut core::ffi::c_char, errbuf_size: usize) -> i32;
    }
}

#[cfg(docsrs)]
pub use docsrs::*;

pub const fn AVERROR(e: i32) -> i32 {
    -e
}

const fn fferrtag(a: u8, b: u8, c: u8, d: u8) -> i32 {
    let raw = (a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24);
    -(raw as i32)
}

pub const AVERROR_EOF: i32 = fferrtag(b'E', b'O', b'F', b' ');
pub const AVERROR_INVALIDDATA: i32 = fferrtag(b'I', b'N', b'D', b'A');

pub const AV_NOPTS_VALUE: i64 = i64::MIN;
pub const AVSEEK_FLAG_BACKWARD: i32 = 1;
