//! Audio codec backed by FFmpeg: compact and free of external dependencies.
//!
//! ```no_run
//! let info = codecpod::info("input.mp3")?;
//! assert!(info.sample_rate > 0);
//!
//! let buf = codecpod::load("input.mp3", &codecpod::LoadOptions {
//!     sample_rate: Some(16_000),
//!     mono: true,
//!     ..Default::default()
//! })?;
//! assert_eq!(buf.channels, 1);
//! assert_eq!(buf.sample_rate, 16_000);
//!
//! codecpod::save("output.flac", &buf, &codecpod::SaveOptions {
//!     codec: codecpod::Codec::Flac {
//!         compression_level: None,
//!         bits_per_sample: codecpod::FlacBitsPerSample::Bits16,
//!     },
//!     ..Default::default()
//! })?;
//! # Ok::<_, codecpod::Error>(())
//! ```

#![warn(clippy::default_trait_access)]
#![warn(clippy::elidable_lifetime_names)]
#![warn(clippy::uninlined_format_args)]
#![warn(missing_debug_implementations)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(rust_2024_compatibility)]

use std::ffi::CStr;
use std::path::Path;

pub use error::{Error, FFmpegError};

mod error;
mod sys;

#[cfg(not(docsrs))]
mod avio;
#[cfg(not(docsrs))]
mod util;

#[cfg_attr(docsrs, path = "encoder_stub.rs")]
mod encoder;
#[cfg_attr(docsrs, path = "loader_stub.rs")]
mod loader;

/// Decoded audio samples.
#[derive(Debug, Clone)]
pub enum SampleData {
    /// 64-bit floating-point PCM. In the range `[-1, 1]` when normalized.
    F64(Vec<f64>),
    /// 32-bit floating-point PCM. In the range `[-1, 1]` when normalized.
    F32(Vec<f32>),
    /// 64-bit signed integer PCM.
    I64(Vec<i64>),
    /// 32-bit signed integer PCM.
    I32(Vec<i32>),
    /// 16-bit signed integer PCM.
    I16(Vec<i16>),
    /// 8-bit unsigned integer PCM, with a zero offset of `128`.
    U8(Vec<u8>),
}

/// Memory layout of multi-channel samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelLayout {
    /// Planar layout `[C, F]`, where samples of the same channel are contiguous.
    Planar,
    /// Interleaved layout `[F, C]`, where samples of the same frame are contiguous across channels.
    Interleaved,
}

/// Decoded audio.
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    /// Decoded audio samples; the type and range depend on the options in [`LoadOptions`].
    pub samples: SampleData,
    /// Number of channels.
    pub channels: u32,
    /// Number of frames, i.e. samples per channel.
    pub frames: u64,
    /// Sample rate.
    pub sample_rate: u32,
    /// Memory layout of multi-channel samples.
    pub layout: ChannelLayout,
}

/// Metadata of the primary audio stream of an audio file.
#[derive(Debug, Clone)]
pub struct AudioInfo {
    /// Sample rate.
    pub sample_rate: u32,
    /// Number of channels.
    pub channels: u32,
    /// Number of frames, i.e. samples per channel; `None` when neither container nor duration information is available.
    pub frames: Option<u64>,
    /// Effective bits per sample; `None` when it cannot be determined.
    pub bits_per_sample: Option<u32>,
    /// Short codec name (e.g. `"mp3"`, `"flac"`); `None` when it cannot be identified.
    pub codec: Option<String>,
}

/// Options for [`load`].
#[derive(Debug, Clone)]
pub struct LoadOptions {
    /// Target sample rate. Defaults to `None`, which keeps the source sample rate.
    pub sample_rate: Option<u32>,
    /// Whether to downmix to mono. Defaults to `false`.
    pub mono: bool,
    /// Number of frames to skip from the start, in source sample-rate units. Defaults to `0`.
    pub frame_offset: u64,
    /// Maximum number of frames to decode, in source sample-rate units. Defaults to `None`, which decodes all remaining frames.
    pub num_frames: Option<u64>,
    /// Whether to normalize integer PCM.
    ///
    /// When `true` (the default), the output is uniformly `f32` in the range `[-1, 1]`
    /// regardless of the source format. Integer PCM is normalized, floating-point PCM is
    /// returned as `f32`, and `f64` sources are converted to `f32`. When `false`, samples are
    /// returned in the source's native integer or floating-point type. A few rare source
    /// formats may fall back to `f32`.
    pub normalize: bool,
    /// When `true` (the default), the output uses planar layout `[C, F]`; when `false`,
    /// the output uses interleaved layout `[F, C]`.
    pub channels_first: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            sample_rate: None,
            mono: false,
            frame_offset: 0,
            num_frames: None,
            normalize: true,
            channels_first: true,
        }
    }
}

/// PCM bit depth in a WAV container.
#[derive(Debug, Clone, Copy)]
pub enum WavSampleFormat {
    /// 8-bit unsigned integer.
    U8,
    /// 16-bit signed integer.
    I16,
    /// 24-bit signed integer.
    I24,
    /// 32-bit signed integer.
    I32,
    /// 32-bit floating point.
    F32,
    /// 64-bit floating point.
    F64,
}

/// PCM bit depth in an AIFF container.
#[derive(Debug, Clone, Copy)]
pub enum AiffSampleFormat {
    /// 8-bit signed integer.
    I8,
    /// 16-bit signed integer.
    I16,
    /// 24-bit signed integer.
    I24,
    /// 32-bit signed integer.
    I32,
    /// 32-bit floating point.
    F32,
    /// 64-bit floating point.
    F64,
}

/// FLAC output bit depth.
#[derive(Debug, Clone, Copy)]
pub enum FlacBitsPerSample {
    /// 16-bit integer samples.
    Bits16,
    /// 24-bit integer samples.
    Bits24,
}

/// ALAC output bit depth.
#[derive(Debug, Clone, Copy)]
pub enum AlacBitsPerSample {
    /// 16-bit integer samples.
    Bits16,
    /// 24-bit integer samples.
    Bits24,
}

/// Target application type for the Opus encoder, which influences internal mode selection.
#[derive(Debug, Clone, Copy, Default)]
pub enum OpusApplication {
    /// Optimize for speech intelligibility (VoIP).
    Voip,
    /// Optimize for fidelity to the original audio.
    #[default]
    Audio,
    /// Restrict to the lowest-latency mode, disabling speech-optimized modes.
    LowDelay,
}

/// Opus frame duration.
#[derive(Debug, Clone, Copy, Default)]
pub enum OpusFrameDuration {
    /// 2.5 ms.
    D2_5ms,
    /// 5 ms.
    D5ms,
    /// 10 ms.
    D10ms,
    /// 20 ms.
    #[default]
    D20ms,
    /// 40 ms.
    D40ms,
    /// 60 ms.
    D60ms,
}

/// Opus bit-rate mode.
#[derive(Debug, Clone, Copy, Default)]
pub enum OpusVbrMode {
    /// Constant bit rate.
    Off,
    /// Variable bit rate.
    #[default]
    On,
    /// Constrained variable bit rate.
    Constrained,
}

/// Output encoder and its corresponding container.
#[derive(Debug, Clone, Copy)]
pub enum Codec {
    /// PCM in a WAV container.
    Wav {
        /// PCM bit depth.
        sample_format: WavSampleFormat,
    },
    /// PCM in an AIFF container.
    Aiff {
        /// PCM bit depth.
        sample_format: AiffSampleFormat,
    },
    /// FLAC in a native FLAC container.
    Flac {
        /// Compression level, from `0` (fastest) to `12` (highest compression); FFmpeg's default is used when `None`.
        compression_level: Option<u32>,
        /// Encoding bit depth.
        bits_per_sample: FlacBitsPerSample,
    },
    /// ALAC (Apple Lossless) in an m4a (ipod muxer) container.
    Alac {
        /// Encoding bit depth.
        bits_per_sample: AlacBitsPerSample,
    },
    /// FFmpeg's built-in AAC-LC in an m4a (ipod muxer) container.
    Aac {
        /// Target bit rate (bits/s); FFmpeg's default is used when `None`.
        bit_rate: Option<u32>,
    },
    /// MP3 (libmp3lame) in an mp3 container.
    Mp3 {
        /// Target bit rate (bits/s); setting it enables ABR. When `None`, LAME decides according to its default mode.
        bit_rate: Option<u32>,
        /// LAME's internal quality level, from `0` (highest quality, slowest) to `9` (lowest quality, fastest).
        ///
        /// Controls the encoder's quality/speed trade-off, independent of `bit_rate` and settable
        /// alongside it. LAME's default is used when both are `None`.
        compression_level: Option<u32>,
    },
    /// Opus (libopus) in an ogg container.
    ///
    /// libopus only supports 8000 / 12000 / 16000 / 24000 / 48000 Hz. When the input [`AudioBuffer`]'s
    /// sample rate is not one of these, set `sample_rate` in [`SaveOptions`] to one of them;
    /// otherwise [`Error::UnsupportedSampleRate`] is returned.
    Opus {
        /// Target bit rate (bits/s); FFmpeg's default is used when `None`.
        bit_rate: Option<u32>,
        /// Application type; FFmpeg's default is used when `None`.
        application: Option<OpusApplication>,
        /// Frame duration; FFmpeg's default is used when `None`.
        frame_duration: Option<OpusFrameDuration>,
        /// Bit-rate mode; FFmpeg's default is used when `None`.
        vbr: Option<OpusVbrMode>,
    },
    /// Vorbis (libvorbis) in an ogg container.
    Vorbis {
        /// Quality level, from `-1.0` (lowest) to `10.0` (highest).
        ///
        /// Mutually exclusive with `bit_rate`; `quality` takes precedence when both are given. When `None`
        /// and `bit_rate` is also `None`, libvorbis's default VBR quality 3 is used.
        quality: Option<f32>,
        /// Target bit rate (bits/s).
        ///
        /// Mutually exclusive with `quality`; `quality` takes precedence when both are given. When `None`
        /// and `quality` is also `None`, libvorbis's default VBR quality 3 is used.
        bit_rate: Option<u32>,
    },
}

/// Options for [`save`].
#[derive(Debug, Clone)]
pub struct SaveOptions {
    /// Output encoder and container.
    pub codec: Codec,
    /// Target sample rate. Defaults to `None`, which reuses the input [`AudioBuffer`]'s sample rate.
    pub sample_rate: Option<u32>,
    /// Whether to downmix to mono.
    pub mono: bool,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self {
            codec: Codec::Wav {
                sample_format: WavSampleFormat::I16,
            },
            sample_rate: None,
            mono: false,
        }
    }
}

impl WavSampleFormat {
    pub(crate) fn encoder_name(self) -> &'static CStr {
        match self {
            Self::U8 => c"pcm_u8",
            Self::I16 => c"pcm_s16le",
            Self::I24 => c"pcm_s24le",
            Self::I32 => c"pcm_s32le",
            Self::F32 => c"pcm_f32le",
            Self::F64 => c"pcm_f64le",
        }
    }

    pub(crate) fn sample_fmt(self) -> i32 {
        match self {
            Self::U8 => sys::AV_SAMPLE_FMT_U8,
            Self::I16 => sys::AV_SAMPLE_FMT_S16,
            Self::I24 | Self::I32 => sys::AV_SAMPLE_FMT_S32,
            Self::F32 => sys::AV_SAMPLE_FMT_FLT,
            Self::F64 => sys::AV_SAMPLE_FMT_DBL,
        }
    }
}

impl AiffSampleFormat {
    pub(crate) fn encoder_name(self) -> &'static CStr {
        match self {
            Self::I8 => c"pcm_s8",
            Self::I16 => c"pcm_s16be",
            Self::I24 => c"pcm_s24be",
            Self::I32 => c"pcm_s32be",
            Self::F32 => c"pcm_f32be",
            Self::F64 => c"pcm_f64be",
        }
    }

    pub(crate) fn sample_fmt(self) -> i32 {
        match self {
            Self::I8 => sys::AV_SAMPLE_FMT_U8,
            Self::I16 => sys::AV_SAMPLE_FMT_S16,
            Self::I24 | Self::I32 => sys::AV_SAMPLE_FMT_S32,
            Self::F32 => sys::AV_SAMPLE_FMT_FLT,
            Self::F64 => sys::AV_SAMPLE_FMT_DBL,
        }
    }
}

impl FlacBitsPerSample {
    pub(crate) fn sample_fmt(self) -> i32 {
        match self {
            Self::Bits16 => sys::AV_SAMPLE_FMT_S16,
            Self::Bits24 => sys::AV_SAMPLE_FMT_S32,
        }
    }

    pub(crate) fn bits_per_raw_sample(self) -> i32 {
        match self {
            Self::Bits16 => 16,
            Self::Bits24 => 24,
        }
    }
}

impl AlacBitsPerSample {
    pub(crate) fn sample_fmt(self) -> i32 {
        match self {
            Self::Bits16 => sys::AV_SAMPLE_FMT_S16P,
            Self::Bits24 => sys::AV_SAMPLE_FMT_S32P,
        }
    }

    pub(crate) fn bits_per_raw_sample(self) -> i32 {
        match self {
            Self::Bits16 => 16,
            Self::Bits24 => 24,
        }
    }
}

impl OpusApplication {
    pub(crate) fn av_opt_value(self) -> &'static CStr {
        match self {
            Self::Voip => c"voip",
            Self::Audio => c"audio",
            Self::LowDelay => c"lowdelay",
        }
    }
}

impl OpusFrameDuration {
    pub(crate) fn millis(self) -> f64 {
        match self {
            Self::D2_5ms => 2.5,
            Self::D5ms => 5.0,
            Self::D10ms => 10.0,
            Self::D20ms => 20.0,
            Self::D40ms => 40.0,
            Self::D60ms => 60.0,
        }
    }
}

impl OpusVbrMode {
    pub(crate) fn av_opt_value(self) -> &'static CStr {
        match self {
            Self::Off => c"off",
            Self::On => c"on",
            Self::Constrained => c"constrained",
        }
    }
}

/// Retrieves the metadata of an audio file's primary audio stream.
pub fn info<P: AsRef<Path>>(path: P) -> Result<AudioInfo, Error> {
    loader::info(path.as_ref())
}

/// Loads an audio file and decodes it into an [`AudioBuffer`].
pub fn load<P: AsRef<Path>>(path: P, opts: &LoadOptions) -> Result<AudioBuffer, Error> {
    if let Some(0) = opts.sample_rate {
        return Err(Error::InvalidArg("sample_rate must be > 0 when set"));
    }
    if let Some(0) = opts.num_frames {
        return Err(Error::InvalidArg("num_frames must be > 0 when set"));
    }
    loader::load(path.as_ref(), opts)
}

/// Encodes an [`AudioBuffer`] and writes it to a file.
///
/// The container is determined by [`SaveOptions::codec`], not the file extension; the extension only
/// determines the output path. When `opts.sample_rate` differs from the input sample rate, or `opts.mono`
/// is `true` while the input is not mono, swresample is invoked internally to resample / downmix channels.
pub fn save<P: AsRef<Path>>(path: P, buf: &AudioBuffer, opts: &SaveOptions) -> Result<(), Error> {
    if let Some(0) = opts.sample_rate {
        return Err(Error::InvalidArg("sample_rate must be > 0 when set"));
    }
    encoder::save(path.as_ref(), buf, opts)
}

/// Retrieves the metadata of the primary audio stream from in-memory encoded bytes.
///
/// The in-memory equivalent of [`info`]; `data` is the full contents of an encoded audio
/// file (e.g. an MP3 or FLAC byte buffer) and is read without being copied.
pub fn info_bytes(data: &[u8]) -> Result<AudioInfo, Error> {
    loader::info_bytes(data)
}

/// Decodes in-memory encoded bytes into an [`AudioBuffer`].
///
/// The in-memory equivalent of [`load`]; `data` is the full contents of an encoded audio
/// file and is read without being copied. The [`LoadOptions`] behave exactly as for [`load`].
pub fn load_bytes(data: &[u8], opts: &LoadOptions) -> Result<AudioBuffer, Error> {
    if let Some(0) = opts.sample_rate {
        return Err(Error::InvalidArg("sample_rate must be > 0 when set"));
    }
    if let Some(0) = opts.num_frames {
        return Err(Error::InvalidArg("num_frames must be > 0 when set"));
    }
    loader::load_bytes(data, opts)
}

/// Encodes an [`AudioBuffer`] into a freshly allocated byte buffer.
///
/// The in-memory equivalent of [`save`]: instead of writing to a file, the encoded bytes are
/// returned. The container is determined by [`SaveOptions::codec`]; [`SaveOptions::sample_rate`]
/// and [`SaveOptions::mono`] trigger the same internal resample / downmix as [`save`].
pub fn save_bytes(buf: &AudioBuffer, opts: &SaveOptions) -> Result<Vec<u8>, Error> {
    if let Some(0) = opts.sample_rate {
        return Err(Error::InvalidArg("sample_rate must be > 0 when set"));
    }
    encoder::save_bytes(buf, opts)
}
