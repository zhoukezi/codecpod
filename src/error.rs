use std::ffi::{CStr, NulError};
use std::fmt;

use crate::sys;

/// Errors that [`codecpod`](crate) may produce.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The path contains an interior NUL byte and cannot be converted to a C string.
    #[error("invalid path: contains interior NUL")]
    InvalidPath(#[from] NulError),

    /// The caller passed an invalid argument.
    #[error("invalid argument: {0}")]
    InvalidArg(&'static str),

    /// The input file contains no audio stream.
    #[error("file has no audio stream")]
    NoAudioStream,

    /// No decoder matching the given codec id could be found.
    #[error("decoder not found for codec id {0}")]
    DecoderNotFound(u32),

    /// The audio stream reported an invalid channel count or sample rate.
    #[error("invalid stream parameters: channels={channels} sample_rate={sample_rate}")]
    InvalidStreamParameters {
        /// Channel count reported by the stream.
        channels: i32,
        /// Sample rate reported by the stream.
        sample_rate: i32,
    },

    /// No encoder matching the given name could be found (not enabled at build time).
    #[error("encoder not found: {0}")]
    EncoderNotFound(&'static str),

    /// The encoder does not support the target sample rate.
    #[error("encoder does not support sample rate {sample_rate} (supported: {supported:?})")]
    UnsupportedSampleRate {
        /// Sample rate requested by the caller.
        sample_rate: i32,
        /// List of sample rates supported by the encoder.
        supported: Vec<i32>,
    },

    /// The input [`SampleData`](crate::SampleData) length does not match `channels * frames`,
    /// or `channels` / `sample_rate` is 0.
    #[error("invalid input buffer: {0}")]
    InvalidInputBuffer(&'static str),

    /// An `av_opt_set` / `av_opt_set_int` / `av_opt_set_double` call failed. The common cause
    /// is that the encoder does not support the given AVOption (a misspelled key or a mismatched FFmpeg encoder version).
    #[error("av_opt_set({key}): {code}")]
    AvOptSetFailed {
        /// Key of the AVOption that failed to be set.
        key: &'static str,
        /// FFmpeg error code.
        code: FFmpegError,
    },

    /// Memory allocation failed.
    #[error("memory allocation failed")]
    OutOfMemory,

    /// The `avformat_open_input` call failed.
    #[error("avformat_open_input: {0}")]
    OpenInput(FFmpegError),

    /// The `avformat_find_stream_info` call failed.
    #[error("avformat_find_stream_info: {0}")]
    FindStreamInfo(FFmpegError),

    /// `avcodec_alloc_context3` returned NULL.
    #[error("avcodec_alloc_context3 returned NULL")]
    AllocCodecContext,

    /// The `avcodec_parameters_to_context` call failed.
    #[error("avcodec_parameters_to_context: {0}")]
    CodecParameters(FFmpegError),

    /// The `avcodec_open2` call failed.
    #[error("avcodec_open2: {0}")]
    OpenCodec(FFmpegError),

    /// The `swr_alloc_set_opts2` call failed.
    #[error("swr_alloc_set_opts2: {0}")]
    SwrAlloc(FFmpegError),

    /// The `swr_init` call failed.
    #[error("swr_init: {0}")]
    SwrInit(FFmpegError),

    /// The `swr_convert` call failed.
    #[error("swr_convert: {0}")]
    SwrConvert(FFmpegError),

    /// The `av_seek_frame` call failed.
    #[error("av_seek_frame: {0}")]
    Seek(FFmpegError),

    /// The `av_read_frame` call failed.
    #[error("av_read_frame: {0}")]
    ReadFrame(FFmpegError),

    /// The `avcodec_send_packet` call failed.
    #[error("avcodec_send_packet: {0}")]
    SendPacket(FFmpegError),

    /// The `avcodec_receive_frame` call failed.
    #[error("avcodec_receive_frame: {0}")]
    ReceiveFrame(FFmpegError),

    /// The `avformat_alloc_output_context2` call failed.
    #[error("avformat_alloc_output_context2: {0}")]
    AllocOutputContext(FFmpegError),

    /// `avformat_new_stream` returned NULL.
    #[error("avformat_new_stream returned NULL")]
    NewStream,

    /// The `avcodec_parameters_from_context` call failed.
    #[error("avcodec_parameters_from_context: {0}")]
    CodecParametersFrom(FFmpegError),

    /// The `avio_open` call failed.
    #[error("avio_open: {0}")]
    AvioOpen(FFmpegError),

    /// The `avformat_write_header` call failed.
    #[error("avformat_write_header: {0}")]
    WriteHeader(FFmpegError),

    /// The `av_write_trailer` call failed.
    #[error("av_write_trailer: {0}")]
    WriteTrailer(FFmpegError),

    /// The `av_interleaved_write_frame` call failed.
    #[error("av_interleaved_write_frame: {0}")]
    WriteFrame(FFmpegError),

    /// The `avcodec_send_frame` call failed.
    #[error("avcodec_send_frame: {0}")]
    SendFrame(FFmpegError),

    /// The `avcodec_receive_packet` call failed.
    #[error("avcodec_receive_packet: {0}")]
    ReceivePacket(FFmpegError),

    /// `av_audio_fifo_alloc` returned NULL.
    #[error("av_audio_fifo_alloc returned NULL")]
    AudioFifoAlloc,

    /// An `av_audio_fifo_realloc` / `av_audio_fifo_write` call failed.
    #[error("av_audio_fifo write: {0}")]
    AudioFifoWrite(FFmpegError),

    /// The `av_audio_fifo_read` call failed.
    #[error("av_audio_fifo_read: {0}")]
    AudioFifoRead(FFmpegError),

    /// The `av_frame_get_buffer` call failed.
    #[error("av_frame_get_buffer: {0}")]
    FrameGetBuffer(FFmpegError),
}

/// FFmpeg error code.
#[derive(Debug, Clone, Copy)]
pub struct FFmpegError(pub i32);

impl fmt::Display for FFmpegError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = [0i8; sys::AV_ERROR_MAX_STRING_SIZE as usize];
        let msg = unsafe {
            if sys::av_strerror(self.0, buf.as_mut_ptr() as *mut _, buf.len()) == 0 {
                CStr::from_ptr(buf.as_ptr() as *const _)
                    .to_string_lossy()
                    .into_owned()
            } else {
                format!("unknown FFmpeg error {}", self.0)
            }
        };
        write!(f, "{msg} ({})", self.0)
    }
}
