#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(dead_code)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(unnecessary_transmutes)]
#![allow(unsafe_op_in_unsafe_fn)]

include!(concat!(env!("OUT_DIR"), "/ffmpeg.rs"));

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
