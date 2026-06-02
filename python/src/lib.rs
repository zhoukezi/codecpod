use codecpod::{
    AiffSampleFormat, AlacBitsPerSample, ChannelLayout, Codec, FlacBitsPerSample, LoadOptions,
    OpusApplication, OpusFrameDuration, OpusVbrMode, SampleData, SaveOptions, WavSampleFormat,
};
use numpy::ndarray::Array2;
use numpy::{IntoPyArray, PyArrayDyn, PyArrayMethods};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyBytesMethods};
use std::path::PathBuf;

pyo3::create_exception!(codecpod, CodecpodError, pyo3::exceptions::PyException);

fn map_err(e: ::codecpod::Error) -> PyErr {
    CodecpodError::new_err(e.to_string())
}

/// Metadata of an audio file's primary audio stream.
///
/// Returned by :func:`info`. All fields are read-only properties.
#[pyclass(name = "AudioInfo", frozen, module = "codecpod")]
struct PyAudioInfo {
    #[pyo3(get)]
    sample_rate: u32,
    #[pyo3(get)]
    channels: u32,
    #[pyo3(get)]
    frames: Option<u64>,
    #[pyo3(get)]
    bits_per_sample: Option<u32>,
    #[pyo3(get)]
    codec: Option<String>,
}

fn opt_int(v: Option<u64>) -> String {
    v.map_or_else(|| "None".to_owned(), |n| n.to_string())
}

#[pymethods]
impl PyAudioInfo {
    fn __repr__(&self) -> String {
        let codec = self
            .codec
            .as_deref()
            .map_or_else(|| "None".to_owned(), |c| format!("{c:?}"));
        format!(
            "AudioInfo(sample_rate={}, channels={}, frames={}, bits_per_sample={}, codec={})",
            self.sample_rate,
            self.channels,
            opt_int(self.frames),
            opt_int(self.bits_per_sample.map(u64::from)),
            codec,
        )
    }
}

/// Decoded audio samples together with their format.
///
/// Returned by :func:`load` when ``return_buffer=True``, and accepted directly by
/// :func:`save` (in which case the layout is taken from the buffer and ``channels_first``
/// must not be passed). All fields are read-only properties.
#[pyclass(name = "AudioBuffer", frozen, module = "codecpod")]
struct PyAudioBuffer {
    samples: Py<PyAny>,
    #[pyo3(get)]
    sample_rate: u32,
    #[pyo3(get)]
    channels: u32,
    #[pyo3(get)]
    frames: u64,
    #[pyo3(get)]
    layout: String,
}

#[pymethods]
impl PyAudioBuffer {
    #[getter]
    fn samples(&self, py: Python<'_>) -> Py<PyAny> {
        self.samples.clone_ref(py)
    }

    fn __repr__(&self) -> String {
        format!(
            "AudioBuffer(sample_rate={}, channels={}, frames={}, layout={:?})",
            self.sample_rate, self.channels, self.frames, self.layout
        )
    }
}

macro_rules! codec_class {
    ($(#[doc = $doc:expr])* $name:ident, $pyname:literal) => {
        $(#[doc = $doc])*
        #[pyclass(name = $pyname, frozen, from_py_object, module = "codecpod")]
        #[derive(Clone, Copy)]
        struct $name {
            inner: Codec,
        }
    };
}

codec_class!(
    /// WAV codec: PCM in a WAV container.
    ///
    /// Args:
    ///     sample_format: PCM sample format, one of ``"u8"``, ``"i16"``, ``"i24"``,
    ///         ``"i32"``, ``"f32"``, ``"f64"``. Defaults to ``"i16"``.
    Wav, "Wav"
);
codec_class!(
    /// AIFF codec: PCM in an AIFF container.
    ///
    /// Args:
    ///     sample_format: PCM sample format, one of ``"i8"``, ``"i16"``, ``"i24"``,
    ///         ``"i32"``, ``"f32"``, ``"f64"``. Defaults to ``"i16"``.
    Aiff, "Aiff"
);
codec_class!(
    /// FLAC codec: lossless FLAC in a native FLAC container.
    ///
    /// Args:
    ///     compression_level: Compression level from ``0`` (fastest) to ``12`` (highest
    ///         compression). ``None`` uses the FFmpeg default.
    ///     bits_per_sample: Encoding bit depth, ``16`` or ``24``. Defaults to ``16``.
    Flac, "Flac"
);
codec_class!(
    /// ALAC codec: Apple Lossless in an m4a container.
    ///
    /// Args:
    ///     bits_per_sample: Encoding bit depth, ``16`` or ``24``. Defaults to ``16``.
    Alac, "Alac"
);
codec_class!(
    /// AAC codec: AAC-LC in an m4a container.
    ///
    /// Args:
    ///     bit_rate: Target bit rate in bits per second. ``None`` uses the FFmpeg default.
    Aac, "Aac"
);
codec_class!(
    /// MP3 codec: MP3 (LAME) in an mp3 container.
    ///
    /// Args:
    ///     bit_rate: Target bit rate in bits per second; setting it enables ABR.
    ///         ``None`` lets LAME choose its default mode.
    ///     compression_level: LAME quality level from ``0`` (best quality, slowest) to
    ///         ``9`` (worst quality, fastest). Independent of ``bit_rate``. ``None`` uses
    ///         the LAME default.
    Mp3, "Mp3"
);
codec_class!(
    /// Opus codec: Opus (libopus) in an ogg container.
    ///
    /// libopus only supports sample rates of 8000, 12000, 16000, 24000, or 48000 Hz. If the
    /// audio has a different rate, resample it via the ``resample_to`` argument of
    /// :func:`save`, otherwise a :class:`CodecpodError` is raised.
    ///
    /// Args:
    ///     bit_rate: Target bit rate in bits per second. ``None`` uses the FFmpeg default.
    ///     application: Tuning target, one of ``"voip"``, ``"audio"``, ``"lowdelay"``.
    ///         ``None`` uses the FFmpeg default.
    ///     frame_duration: Frame duration in milliseconds, one of ``2.5``, ``5``, ``10``,
    ///         ``20``, ``40``, ``60``. ``None`` uses the FFmpeg default.
    ///     vbr: Bit-rate mode, one of ``"off"`` (CBR), ``"on"`` (VBR), ``"constrained"``.
    ///         ``None`` uses the FFmpeg default.
    Opus, "Opus"
);
codec_class!(
    /// Vorbis codec: Vorbis (libvorbis) in an ogg container.
    ///
    /// ``quality`` and ``bit_rate`` are mutually exclusive; if both are given, ``quality``
    /// takes precedence. If both are ``None``, libvorbis's default VBR quality 3 is used.
    ///
    /// Args:
    ///     quality: Quality level from ``-1.0`` (lowest) to ``10.0`` (highest).
    ///     bit_rate: Target bit rate in bits per second.
    Vorbis, "Vorbis"
);

fn wav_format(s: &str) -> PyResult<WavSampleFormat> {
    Ok(match s {
        "u8" => WavSampleFormat::U8,
        "i16" => WavSampleFormat::I16,
        "i24" => WavSampleFormat::I24,
        "i32" => WavSampleFormat::I32,
        "f32" => WavSampleFormat::F32,
        "f64" => WavSampleFormat::F64,
        other => {
            return Err(PyValueError::new_err(format!(
                "invalid sample_format {other:?}; expected one of: u8, i16, i24, i32, f32, f64"
            )));
        }
    })
}

fn aiff_format(s: &str) -> PyResult<AiffSampleFormat> {
    Ok(match s {
        "i8" => AiffSampleFormat::I8,
        "i16" => AiffSampleFormat::I16,
        "i24" => AiffSampleFormat::I24,
        "i32" => AiffSampleFormat::I32,
        "f32" => AiffSampleFormat::F32,
        "f64" => AiffSampleFormat::F64,
        other => {
            return Err(PyValueError::new_err(format!(
                "invalid sample_format {other:?}; expected one of: i8, i16, i24, i32, f32, f64"
            )));
        }
    })
}

fn flac_bits(b: u32) -> PyResult<FlacBitsPerSample> {
    Ok(match b {
        16 => FlacBitsPerSample::Bits16,
        24 => FlacBitsPerSample::Bits24,
        other => {
            return Err(PyValueError::new_err(format!(
                "invalid bits_per_sample {other}; expected 16 or 24"
            )));
        }
    })
}

fn alac_bits(b: u32) -> PyResult<AlacBitsPerSample> {
    Ok(match b {
        16 => AlacBitsPerSample::Bits16,
        24 => AlacBitsPerSample::Bits24,
        other => {
            return Err(PyValueError::new_err(format!(
                "invalid bits_per_sample {other}; expected 16 or 24"
            )));
        }
    })
}

fn opus_application(s: &str) -> PyResult<OpusApplication> {
    Ok(match s {
        "voip" => OpusApplication::Voip,
        "audio" => OpusApplication::Audio,
        "lowdelay" => OpusApplication::LowDelay,
        other => {
            return Err(PyValueError::new_err(format!(
                "invalid application {other:?}; expected one of: voip, audio, lowdelay"
            )));
        }
    })
}

fn opus_frame_duration(ms: f64) -> PyResult<OpusFrameDuration> {
    Ok(match ms {
        2.5 => OpusFrameDuration::D2_5ms,
        5.0 => OpusFrameDuration::D5ms,
        10.0 => OpusFrameDuration::D10ms,
        20.0 => OpusFrameDuration::D20ms,
        40.0 => OpusFrameDuration::D40ms,
        60.0 => OpusFrameDuration::D60ms,
        other => {
            return Err(PyValueError::new_err(format!(
                "invalid frame_duration {other}; expected one of: 2.5, 5, 10, 20, 40, 60 (ms)"
            )));
        }
    })
}

fn opus_vbr(s: &str) -> PyResult<OpusVbrMode> {
    Ok(match s {
        "off" => OpusVbrMode::Off,
        "on" => OpusVbrMode::On,
        "constrained" => OpusVbrMode::Constrained,
        other => {
            return Err(PyValueError::new_err(format!(
                "invalid vbr {other:?}; expected one of: off, on, constrained"
            )));
        }
    })
}

fn wav_format_str(f: WavSampleFormat) -> &'static str {
    match f {
        WavSampleFormat::U8 => "u8",
        WavSampleFormat::I16 => "i16",
        WavSampleFormat::I24 => "i24",
        WavSampleFormat::I32 => "i32",
        WavSampleFormat::F32 => "f32",
        WavSampleFormat::F64 => "f64",
    }
}

fn aiff_format_str(f: AiffSampleFormat) -> &'static str {
    match f {
        AiffSampleFormat::I8 => "i8",
        AiffSampleFormat::I16 => "i16",
        AiffSampleFormat::I24 => "i24",
        AiffSampleFormat::I32 => "i32",
        AiffSampleFormat::F32 => "f32",
        AiffSampleFormat::F64 => "f64",
    }
}

fn flac_bits_num(b: FlacBitsPerSample) -> u32 {
    match b {
        FlacBitsPerSample::Bits16 => 16,
        FlacBitsPerSample::Bits24 => 24,
    }
}

fn alac_bits_num(b: AlacBitsPerSample) -> u32 {
    match b {
        AlacBitsPerSample::Bits16 => 16,
        AlacBitsPerSample::Bits24 => 24,
    }
}

fn opus_application_str(a: OpusApplication) -> &'static str {
    match a {
        OpusApplication::Voip => "voip",
        OpusApplication::Audio => "audio",
        OpusApplication::LowDelay => "lowdelay",
    }
}

fn opus_frame_duration_ms(d: OpusFrameDuration) -> f64 {
    match d {
        OpusFrameDuration::D2_5ms => 2.5,
        OpusFrameDuration::D5ms => 5.0,
        OpusFrameDuration::D10ms => 10.0,
        OpusFrameDuration::D20ms => 20.0,
        OpusFrameDuration::D40ms => 40.0,
        OpusFrameDuration::D60ms => 60.0,
    }
}

fn opus_vbr_str(v: OpusVbrMode) -> &'static str {
    match v {
        OpusVbrMode::Off => "off",
        OpusVbrMode::On => "on",
        OpusVbrMode::Constrained => "constrained",
    }
}

fn opt_num<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "None".to_owned(), |n| n.to_string())
}

fn opt_quoted(v: Option<&str>) -> String {
    v.map_or_else(|| "None".to_owned(), |s| format!("'{s}'"))
}

fn codec_repr(c: &Codec) -> String {
    match *c {
        Codec::Wav { sample_format } => {
            format!("Wav(sample_format='{}')", wav_format_str(sample_format))
        }
        Codec::Aiff { sample_format } => {
            format!("Aiff(sample_format='{}')", aiff_format_str(sample_format))
        }
        Codec::Flac {
            compression_level,
            bits_per_sample,
        } => format!(
            "Flac(compression_level={}, bits_per_sample={})",
            opt_num(compression_level),
            flac_bits_num(bits_per_sample)
        ),
        Codec::Alac { bits_per_sample } => {
            format!("Alac(bits_per_sample={})", alac_bits_num(bits_per_sample))
        }
        Codec::Aac { bit_rate } => format!("Aac(bit_rate={})", opt_num(bit_rate)),
        Codec::Mp3 {
            bit_rate,
            compression_level,
        } => format!(
            "Mp3(bit_rate={}, compression_level={})",
            opt_num(bit_rate),
            opt_num(compression_level)
        ),
        Codec::Opus {
            bit_rate,
            application,
            frame_duration,
            vbr,
        } => format!(
            "Opus(bit_rate={}, application={}, frame_duration={}, vbr={})",
            opt_num(bit_rate),
            opt_quoted(application.map(opus_application_str)),
            opt_num(frame_duration.map(opus_frame_duration_ms)),
            opt_quoted(vbr.map(opus_vbr_str))
        ),
        Codec::Vorbis { quality, bit_rate } => format!(
            "Vorbis(quality={}, bit_rate={})",
            opt_num(quality),
            opt_num(bit_rate)
        ),
    }
}

#[pymethods]
impl Wav {
    #[new]
    #[pyo3(signature = (sample_format="i16"))]
    fn new(sample_format: &str) -> PyResult<Self> {
        Ok(Self {
            inner: Codec::Wav {
                sample_format: wav_format(sample_format)?,
            },
        })
    }

    fn __repr__(&self) -> String {
        codec_repr(&self.inner)
    }
}

#[pymethods]
impl Aiff {
    #[new]
    #[pyo3(signature = (sample_format="i16"))]
    fn new(sample_format: &str) -> PyResult<Self> {
        Ok(Self {
            inner: Codec::Aiff {
                sample_format: aiff_format(sample_format)?,
            },
        })
    }

    fn __repr__(&self) -> String {
        codec_repr(&self.inner)
    }
}

#[pymethods]
impl Flac {
    #[new]
    #[pyo3(signature = (compression_level=None, bits_per_sample=16))]
    fn new(compression_level: Option<u32>, bits_per_sample: u32) -> PyResult<Self> {
        Ok(Self {
            inner: Codec::Flac {
                compression_level,
                bits_per_sample: flac_bits(bits_per_sample)?,
            },
        })
    }

    fn __repr__(&self) -> String {
        codec_repr(&self.inner)
    }
}

#[pymethods]
impl Alac {
    #[new]
    #[pyo3(signature = (bits_per_sample=16))]
    fn new(bits_per_sample: u32) -> PyResult<Self> {
        Ok(Self {
            inner: Codec::Alac {
                bits_per_sample: alac_bits(bits_per_sample)?,
            },
        })
    }

    fn __repr__(&self) -> String {
        codec_repr(&self.inner)
    }
}

#[pymethods]
impl Aac {
    #[new]
    #[pyo3(signature = (bit_rate=None))]
    fn new(bit_rate: Option<u32>) -> Self {
        Self {
            inner: Codec::Aac { bit_rate },
        }
    }

    fn __repr__(&self) -> String {
        codec_repr(&self.inner)
    }
}

#[pymethods]
impl Mp3 {
    #[new]
    #[pyo3(signature = (bit_rate=None, compression_level=None))]
    fn new(bit_rate: Option<u32>, compression_level: Option<u32>) -> Self {
        Self {
            inner: Codec::Mp3 {
                bit_rate,
                compression_level,
            },
        }
    }

    fn __repr__(&self) -> String {
        codec_repr(&self.inner)
    }
}

#[pymethods]
impl Opus {
    #[new]
    #[pyo3(signature = (bit_rate=None, application=None, frame_duration=None, vbr=None))]
    fn new(
        bit_rate: Option<u32>,
        application: Option<&str>,
        frame_duration: Option<f64>,
        vbr: Option<&str>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: Codec::Opus {
                bit_rate,
                application: application.map(opus_application).transpose()?,
                frame_duration: frame_duration.map(opus_frame_duration).transpose()?,
                vbr: vbr.map(opus_vbr).transpose()?,
            },
        })
    }

    fn __repr__(&self) -> String {
        codec_repr(&self.inner)
    }
}

#[pymethods]
impl Vorbis {
    #[new]
    #[pyo3(signature = (quality=None, bit_rate=None))]
    fn new(quality: Option<f32>, bit_rate: Option<u32>) -> Self {
        Self {
            inner: Codec::Vorbis { quality, bit_rate },
        }
    }

    fn __repr__(&self) -> String {
        codec_repr(&self.inner)
    }
}

fn resolve_codec(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Codec> {
    let Some(obj) = obj else {
        return Ok(SaveOptions::default().codec);
    };
    macro_rules! try_codec {
        ($ty:ty) => {
            if let Ok(c) = obj.extract::<$ty>() {
                return Ok(c.inner);
            }
        };
    }
    try_codec!(Wav);
    try_codec!(Aiff);
    try_codec!(Flac);
    try_codec!(Alac);
    try_codec!(Aac);
    try_codec!(Mp3);
    try_codec!(Opus);
    try_codec!(Vorbis);
    Err(PyTypeError::new_err(
        "codec must be one of: Wav, Aiff, Flac, Alac, Aac, Mp3, Opus, Vorbis",
    ))
}

fn samples_to_pyarray<'py>(
    py: Python<'py>,
    buf: ::codecpod::AudioBuffer,
) -> PyResult<Bound<'py, PyAny>> {
    let channels = buf.channels as usize;
    let frames = buf.frames as usize;
    let mono = channels == 1;
    let (rows, cols) = match buf.layout {
        ChannelLayout::Planar => (channels, frames),
        ChannelLayout::Interleaved => (frames, channels),
    };

    macro_rules! build {
        ($vec:expr) => {{
            if mono {
                $vec.into_pyarray(py).into_any()
            } else {
                let arr = Array2::from_shape_vec((rows, cols), $vec)
                    .map_err(|e| PyValueError::new_err(format!("internal shape error: {e}")))?;
                arr.into_pyarray(py).into_any()
            }
        }};
    }

    Ok(match buf.samples {
        SampleData::F64(v) => build!(v),
        SampleData::F32(v) => build!(v),
        SampleData::I64(v) => build!(v),
        SampleData::I32(v) => build!(v),
        SampleData::I16(v) => build!(v),
        SampleData::U8(v) => build!(v),
    })
}

fn pyarray_to_samples(
    data: &Bound<'_, PyAny>,
    channels_first: bool,
) -> PyResult<(SampleData, u32, u64, ChannelLayout)> {
    macro_rules! try_dtype {
        ($t:ty, $variant:ident) => {
            if let Ok(arr) = data.cast::<PyArrayDyn<$t>>() {
                let ro = arr.readonly();
                let view = ro.as_array();
                let shape = view.shape();
                let (channels, frames, layout) = match shape.len() {
                    1 => (1u32, shape[0] as u64, ChannelLayout::Planar),
                    2 => {
                        if channels_first {
                            (shape[0] as u32, shape[1] as u64, ChannelLayout::Planar)
                        } else {
                            (shape[1] as u32, shape[0] as u64, ChannelLayout::Interleaved)
                        }
                    }
                    n => {
                        return Err(PyValueError::new_err(format!(
                            "samples must be a 1-D or 2-D array, got {n}-D"
                        )));
                    }
                };
                let vec: Vec<$t> = view.iter().copied().collect();
                return Ok((SampleData::$variant(vec), channels, frames, layout));
            }
        };
    }
    try_dtype!(f32, F32);
    try_dtype!(f64, F64);
    try_dtype!(i64, I64);
    try_dtype!(i32, I32);
    try_dtype!(i16, I16);
    try_dtype!(u8, U8);
    Err(PyTypeError::new_err(
        "samples must be a numpy array of dtype float32, float64, int16, int32, int64, or uint8",
    ))
}

/// Read the metadata of an audio source without decoding its samples.
///
/// Args:
///     path: Either a path to an audio file, or a ``bytes`` object holding the full
///         contents of an encoded audio file (read in memory, without touching disk).
///
/// Returns:
///     An :class:`AudioInfo` describing the primary audio stream.
///
/// Raises:
///     CodecpodError: If the source cannot be opened or contains no audio stream.
#[pyfunction]
fn info(path: &Bound<'_, PyAny>) -> PyResult<PyAudioInfo> {
    let i = if let Ok(bytes) = path.cast::<PyBytes>() {
        ::codecpod::info_bytes(bytes.as_bytes()).map_err(map_err)?
    } else {
        ::codecpod::info(path.extract::<PathBuf>()?).map_err(map_err)?
    };
    Ok(PyAudioInfo {
        sample_rate: i.sample_rate,
        channels: i.channels,
        frames: i.frames,
        bits_per_sample: i.bits_per_sample,
        codec: i.codec,
    })
}

/// Decode an audio source into a NumPy array.
///
/// Args:
///     path: Either a path to an audio file, or a ``bytes`` object holding the full
///         contents of an encoded audio file (read in memory, without touching disk).
///     sample_rate: Resample the output to this rate (Hz). ``None`` keeps the source rate.
///     mono: If ``True``, downmix to a single channel. Defaults to ``False``.
///     frame_offset: Number of frames to skip from the start, in source-rate units.
///         Defaults to ``0``.
///     num_frames: Maximum number of frames to decode, in source-rate units. ``None``
///         decodes to the end.
///     normalize: If ``True`` (default), return ``float32`` samples in ``[-1, 1]``
///         regardless of source format. If ``False``, return samples in the source's
///         native dtype.
///     channels_first: If ``True`` (default), shape multi-channel output as ``(channels,
///         frames)`` (planar); if ``False``, as ``(frames, channels)`` (interleaved).
///         Mono output is always 1-D.
///     return_buffer: If ``True``, return an :class:`AudioBuffer` instead of a
///         ``(samples, sample_rate)`` tuple. Defaults to ``False``.
///
/// Returns:
///     By default, a ``(samples, sample_rate)`` tuple where ``samples`` is a NumPy array.
///     If ``return_buffer=True``, an :class:`AudioBuffer`.
///
/// Raises:
///     CodecpodError: If the source cannot be opened or decoded.
#[pyfunction]
#[pyo3(signature = (
    path,
    *,
    sample_rate=None,
    mono=false,
    frame_offset=0,
    num_frames=None,
    normalize=true,
    channels_first=true,
    return_buffer=false,
))]
#[allow(clippy::too_many_arguments)]
fn load<'py>(
    py: Python<'py>,
    path: &Bound<'_, PyAny>,
    sample_rate: Option<u32>,
    mono: bool,
    frame_offset: u64,
    num_frames: Option<u64>,
    normalize: bool,
    channels_first: bool,
    return_buffer: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let opts = LoadOptions {
        sample_rate,
        mono,
        frame_offset,
        num_frames,
        normalize,
        channels_first,
    };
    let buf = if let Ok(bytes) = path.cast::<PyBytes>() {
        ::codecpod::load_bytes(bytes.as_bytes(), &opts).map_err(map_err)?
    } else {
        ::codecpod::load(path.extract::<PathBuf>()?, &opts).map_err(map_err)?
    };

    let out_rate = buf.sample_rate;
    let channels = buf.channels;
    let frames = buf.frames;
    let layout = match buf.layout {
        ChannelLayout::Planar => "planar",
        ChannelLayout::Interleaved => "interleaved",
    };
    let arr = samples_to_pyarray(py, buf)?;

    if return_buffer {
        let buffer = PyAudioBuffer {
            samples: arr.unbind(),
            sample_rate: out_rate,
            channels,
            frames,
            layout: layout.to_owned(),
        };
        Ok(Bound::new(py, buffer)?.into_any())
    } else {
        Ok((arr, out_rate).into_pyobject(py)?.into_any())
    }
}

/// Encode audio samples and write them to a file.
///
/// The output container is determined by ``codec``, not the file extension.
///
/// Args:
///     path: Output file path.
///     data: Samples to encode, either a NumPy array (1-D mono, or 2-D per
///         ``channels_first``) or an :class:`AudioBuffer`.
///     sample_rate: Sample rate of ``data`` in Hz. Required when ``data`` is a NumPy
///         array; ignored (taken from the buffer) when ``data`` is an :class:`AudioBuffer`.
///     codec: A codec instance (e.g. :class:`Flac`, :class:`Mp3`). Defaults to
///         16-bit WAV when ``None``.
///     resample_to: Resample to this rate (Hz) before encoding. ``None`` keeps the input rate.
///     mono: If ``True``, downmix to a single channel before encoding. Defaults to ``False``.
///     channels_first: For a 2-D NumPy array, ``True`` (default) means ``(channels,
///         frames)``, ``False`` means ``(frames, channels)``. Must be ``None`` when
///         ``data`` is an :class:`AudioBuffer`, whose layout is used instead.
///
/// Raises:
///     CodecpodError: If encoding fails (e.g. an unsupported sample rate for the codec).
///     ValueError: If ``sample_rate`` is missing for a NumPy array, or ``channels_first``
///         is set together with an :class:`AudioBuffer`.
#[pyfunction]
#[pyo3(signature = (
    path,
    data,
    sample_rate=None,
    codec=None,
    *,
    resample_to=None,
    mono=false,
    channels_first=None,
))]
fn save(
    path: PathBuf,
    data: &Bound<'_, PyAny>,
    sample_rate: Option<u32>,
    codec: Option<&Bound<'_, PyAny>>,
    resample_to: Option<u32>,
    mono: bool,
    channels_first: Option<bool>,
) -> PyResult<()> {
    let buf = build_audio_buffer(data, sample_rate, channels_first)?;
    let opts = SaveOptions {
        codec: resolve_codec(codec)?,
        sample_rate: resample_to,
        mono,
    };
    ::codecpod::save(path, &buf, &opts).map_err(map_err)
}

/// Encode audio samples and return them as ``bytes``.
///
/// The in-memory counterpart of :func:`save`; the output container is determined by
/// ``codec``, and the encoded bytes are returned instead of being written to a file.
///
/// Args:
///     data: Samples to encode, either a NumPy array (1-D mono, or 2-D per
///         ``channels_first``) or an :class:`AudioBuffer`.
///     sample_rate: Sample rate of ``data`` in Hz. Required when ``data`` is a NumPy
///         array; ignored (taken from the buffer) when ``data`` is an :class:`AudioBuffer`.
///     codec: A codec instance (e.g. :class:`Flac`, :class:`Mp3`). Defaults to
///         16-bit WAV when ``None``.
///     resample_to: Resample to this rate (Hz) before encoding. ``None`` keeps the input rate.
///     mono: If ``True``, downmix to a single channel before encoding. Defaults to ``False``.
///     channels_first: For a 2-D NumPy array, ``True`` (default) means ``(channels,
///         frames)``, ``False`` means ``(frames, channels)``. Must be ``None`` when
///         ``data`` is an :class:`AudioBuffer`, whose layout is used instead.
///
/// Returns:
///     The encoded audio as a ``bytes`` object.
///
/// Raises:
///     CodecpodError: If encoding fails (e.g. an unsupported sample rate for the codec).
///     ValueError: If ``sample_rate`` is missing for a NumPy array, or ``channels_first``
///         is set together with an :class:`AudioBuffer`.
#[pyfunction]
#[pyo3(signature = (
    data,
    sample_rate=None,
    codec=None,
    *,
    resample_to=None,
    mono=false,
    channels_first=None,
))]
fn save_bytes<'py>(
    py: Python<'py>,
    data: &Bound<'_, PyAny>,
    sample_rate: Option<u32>,
    codec: Option<&Bound<'_, PyAny>>,
    resample_to: Option<u32>,
    mono: bool,
    channels_first: Option<bool>,
) -> PyResult<Bound<'py, PyBytes>> {
    let buf = build_audio_buffer(data, sample_rate, channels_first)?;
    let opts = SaveOptions {
        codec: resolve_codec(codec)?,
        sample_rate: resample_to,
        mono,
    };
    let bytes = ::codecpod::save_bytes(&buf, &opts).map_err(map_err)?;
    Ok(PyBytes::new(py, &bytes))
}

// Resolve the `data` argument (NumPy array or AudioBuffer) plus the sample-rate /
// channels_first options into a codecpod::AudioBuffer, shared by save / save_bytes.
fn build_audio_buffer(
    data: &Bound<'_, PyAny>,
    sample_rate: Option<u32>,
    channels_first: Option<bool>,
) -> PyResult<::codecpod::AudioBuffer> {
    let (array, src_rate, channels_first) =
        if let Ok(buffer) = data.extract::<PyRef<'_, PyAudioBuffer>>() {
            if channels_first.is_some() {
                return Err(PyValueError::new_err(
                    "channels_first must not be set when data is an AudioBuffer; \
                     the layout is determined by the buffer",
                ));
            }
            let array = buffer.samples.clone_ref(data.py()).into_bound(data.py());
            let rate = sample_rate.or(Some(buffer.sample_rate));
            let cf = buffer.layout != "interleaved";
            (array, rate, cf)
        } else {
            (data.clone(), sample_rate, channels_first.unwrap_or(true))
        };

    let Some(src_rate) = src_rate else {
        return Err(PyValueError::new_err(
            "sample_rate is required when data is a numpy array",
        ));
    };

    let (samples, channels, frames, layout) = pyarray_to_samples(&array, channels_first)?;
    Ok(::codecpod::AudioBuffer {
        samples,
        channels,
        frames,
        sample_rate: src_rate,
        layout,
    })
}

#[pymodule(gil_used = false)]
#[pyo3(name = "codecpod")]
fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("CodecpodError", m.py().get_type::<CodecpodError>())?;
    m.add_class::<PyAudioInfo>()?;
    m.add_class::<PyAudioBuffer>()?;
    m.add_class::<Wav>()?;
    m.add_class::<Aiff>()?;
    m.add_class::<Flac>()?;
    m.add_class::<Alac>()?;
    m.add_class::<Aac>()?;
    m.add_class::<Mp3>()?;
    m.add_class::<Opus>()?;
    m.add_class::<Vorbis>()?;
    m.add_function(wrap_pyfunction!(info, m)?)?;
    m.add_function(wrap_pyfunction!(load, m)?)?;
    m.add_function(wrap_pyfunction!(save, m)?)?;
    m.add_function(wrap_pyfunction!(save_bytes, m)?)?;
    Ok(())
}
