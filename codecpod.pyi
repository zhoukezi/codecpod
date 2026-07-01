import os
from typing import Callable, Literal

import numpy as np
import numpy.typing as npt

WavSampleFormat = Literal["u8", "i16", "i24", "i32", "f32", "f64"]
AiffSampleFormat = Literal["i8", "i16", "i24", "i32", "f32", "f64"]
OpusApplication = Literal["voip", "audio", "lowdelay"]
OpusVbrMode = Literal["off", "on", "constrained"]
LogLevel = Literal[
    "quiet", "panic", "fatal", "error", "warning", "info", "verbose", "debug", "trace"
]
LogCallback = Callable[[LogLevel, str], object]
StrPath = str | os.PathLike[str]

class CodecpodError(Exception):
    """Raised when an underlying FFmpeg operation fails."""

class AudioInfo:
    """Metadata of an audio file's primary audio stream.

    Returned by :func:`info`. All fields are read-only properties.
    """

    @property
    def sample_rate(self) -> int:
        """Sample rate in Hz."""

    @property
    def channels(self) -> int:
        """Number of channels."""

    @property
    def frames(self) -> int | None:
        """Number of frames (samples per channel), or ``None`` if unknown."""

    @property
    def bits_per_sample(self) -> int | None:
        """Effective bits per sample, or ``None`` if it cannot be determined."""

    @property
    def codec(self) -> str | None:
        """Short codec name (e.g. ``"mp3"``), or ``None`` if unidentified."""

class AudioBuffer:
    """Decoded audio samples together with their format.

    Returned by :func:`load` when ``return_buffer=True``, and accepted directly by
    :func:`save` (in which case the layout is taken from the buffer and ``channels_first``
    must not be passed). All fields are read-only properties.
    """

    @property
    def samples(self) -> npt.NDArray[np.generic]:
        """The decoded samples as a NumPy array."""

    @property
    def sample_rate(self) -> int:
        """Sample rate in Hz."""

    @property
    def channels(self) -> int:
        """Number of channels."""

    @property
    def frames(self) -> int:
        """Number of frames (samples per channel)."""

    @property
    def layout(self) -> Literal["planar", "interleaved"]:
        """Memory layout of ``samples``: ``"planar"`` ``(C, F)`` or ``"interleaved"`` ``(F, C)``."""

class Wav:
    """WAV codec: PCM in a WAV container."""

    def __init__(self, sample_format: WavSampleFormat = "i16") -> None:
        """Args:
        sample_format: PCM sample format. Defaults to ``"i16"``.
        """

class Aiff:
    """AIFF codec: PCM in an AIFF container."""

    def __init__(self, sample_format: AiffSampleFormat = "i16") -> None:
        """Args:
        sample_format: PCM sample format. Defaults to ``"i16"``.
        """

class Flac:
    """FLAC codec: lossless FLAC in a native FLAC container."""

    def __init__(
        self,
        compression_level: int | None = None,
        bits_per_sample: Literal[16, 24] = 16,
    ) -> None:
        """Args:
        compression_level: ``0`` (fastest) to ``12`` (highest). ``None`` uses the FFmpeg default.
        bits_per_sample: Encoding bit depth, ``16`` or ``24``. Defaults to ``16``.
        """

class Alac:
    """ALAC codec: Apple Lossless in an m4a container."""

    def __init__(self, bits_per_sample: Literal[16, 24] = 16) -> None:
        """Args:
        bits_per_sample: Encoding bit depth, ``16`` or ``24``. Defaults to ``16``.
        """

class Aac:
    """AAC codec: AAC-LC in an m4a container."""

    def __init__(self, bit_rate: int | None = None) -> None:
        """Args:
        bit_rate: Target bit rate (bits/s). ``None`` uses the FFmpeg default.
        """

class Mp3:
    """MP3 codec: MP3 (LAME) in an mp3 container."""

    def __init__(
        self,
        bit_rate: int | None = None,
        compression_level: int | None = None,
    ) -> None:
        """Args:
        bit_rate: Target bit rate (bits/s); setting it enables ABR. ``None`` lets LAME pick.
        compression_level: LAME quality, ``0`` (best/slowest) to ``9`` (worst/fastest).
        """

class Opus:
    """Opus codec: Opus (libopus) in an ogg container.

    libopus only supports 8000, 12000, 16000, 24000, or 48000 Hz. Use the ``resample_to``
    argument of :func:`save` for other rates, otherwise a :class:`CodecpodError` is raised.
    """

    def __init__(
        self,
        bit_rate: int | None = None,
        application: OpusApplication | None = None,
        frame_duration: float | None = None,
        vbr: OpusVbrMode | None = None,
    ) -> None:
        """Args:
        bit_rate: Target bit rate (bits/s). ``None`` uses the FFmpeg default.
        application: Tuning target. ``None`` uses the FFmpeg default.
        frame_duration: Frame duration in ms: 2.5, 5, 10, 20, 40, or 60.
        vbr: Bit-rate mode: ``"off"`` (CBR), ``"on"`` (VBR), or ``"constrained"``.
        """

class Vorbis:
    """Vorbis codec: Vorbis (libvorbis) in an ogg container.

    ``quality`` and ``bit_rate`` are mutually exclusive; ``quality`` wins if both are given.
    If both are ``None``, libvorbis's default VBR quality 3 is used.
    """

    def __init__(
        self,
        quality: float | None = None,
        bit_rate: int | None = None,
    ) -> None:
        """Args:
        quality: Quality level from ``-1.0`` (lowest) to ``10.0`` (highest).
        bit_rate: Target bit rate (bits/s).
        """

Codec = Wav | Aiff | Flac | Alac | Aac | Mp3 | Opus | Vorbis

def info(path: bytes | StrPath) -> AudioInfo:
    """Read the metadata of an audio source without decoding its samples.

    Args:
        path: Either a path to an audio file, or a ``bytes`` object holding the full
            contents of an encoded audio file (read in memory, without touching disk).

    Returns:
        An :class:`AudioInfo` describing the primary audio stream.

    Raises:
        CodecpodError: If the source cannot be opened or contains no audio stream.
    """

def load(
    path: bytes | StrPath,
    *,
    sample_rate: int | None = None,
    mono: bool = False,
    frame_offset: int = 0,
    num_frames: int | None = None,
    normalize: bool = True,
    channels_first: bool = True,
    return_buffer: bool = False,
) -> tuple[npt.NDArray[np.generic], int] | AudioBuffer:
    """Decode an audio source into a NumPy array.

    Args:
        path: Either a path to an audio file, or a ``bytes`` object holding the full
            contents of an encoded audio file (read in memory, without touching disk).
        sample_rate: Resample the output to this rate (Hz). ``None`` keeps the source rate.
        mono: If ``True``, downmix to a single channel. Defaults to ``False``.
        frame_offset: Frames to skip from the start, in source-rate units. Defaults to ``0``.
        num_frames: Maximum frames to decode, in source-rate units. ``None`` decodes to the end.
        normalize: If ``True`` (default), return ``float32`` samples in ``[-1, 1]``; if
            ``False``, return samples in the source's native dtype.
        channels_first: If ``True`` (default), multi-channel output is ``(channels, frames)``
            (planar); if ``False``, ``(frames, channels)`` (interleaved). Mono is always 1-D.
        return_buffer: If ``True``, return an :class:`AudioBuffer` instead of a
            ``(samples, sample_rate)`` tuple. Defaults to ``False``.

    Returns:
        A ``(samples, sample_rate)`` tuple by default, or an :class:`AudioBuffer` if
        ``return_buffer=True``.

    Raises:
        CodecpodError: If the source cannot be opened or decoded.
    """

def save(
    path: StrPath,
    data: npt.NDArray[np.generic] | AudioBuffer,
    sample_rate: int | None = None,
    codec: Codec | None = None,
    *,
    resample_to: int | None = None,
    mono: bool = False,
    channels_first: bool | None = None,
) -> None:
    """Encode audio samples and write them to a file.

    The output container is determined by ``codec``, not the file extension.

    Args:
        path: Output file path.
        data: Samples to encode: a NumPy array (1-D mono, or 2-D per ``channels_first``) or
            an :class:`AudioBuffer`.
        sample_rate: Sample rate of ``data`` (Hz). Required for a NumPy array; ignored for an
            :class:`AudioBuffer` (taken from the buffer).
        codec: A codec instance (e.g. :class:`Flac`). Defaults to 16-bit WAV when ``None``.
        resample_to: Resample to this rate (Hz) before encoding. ``None`` keeps the input rate.
        mono: If ``True``, downmix to a single channel before encoding. Defaults to ``False``.
        channels_first: For a 2-D array, ``True`` (default) is ``(channels, frames)``, ``False``
            is ``(frames, channels)``. Must be ``None`` when ``data`` is an :class:`AudioBuffer`.

    Raises:
        CodecpodError: If encoding fails (e.g. an unsupported sample rate for the codec).
        ValueError: If ``sample_rate`` is missing for a NumPy array, or ``channels_first`` is
            set together with an :class:`AudioBuffer`.
    """

def save_bytes(
    data: npt.NDArray[np.generic] | AudioBuffer,
    sample_rate: int | None = None,
    codec: Codec | None = None,
    *,
    resample_to: int | None = None,
    mono: bool = False,
    channels_first: bool | None = None,
) -> bytes:
    """Encode audio samples and return them as ``bytes``.

    The in-memory counterpart of :func:`save`; the output container is determined by
    ``codec``, and the encoded bytes are returned instead of being written to a file.

    Args:
        data: Samples to encode: a NumPy array (1-D mono, or 2-D per ``channels_first``) or
            an :class:`AudioBuffer`.
        sample_rate: Sample rate of ``data`` (Hz). Required for a NumPy array; ignored for an
            :class:`AudioBuffer` (taken from the buffer).
        codec: A codec instance (e.g. :class:`Flac`). Defaults to 16-bit WAV when ``None``.
        resample_to: Resample to this rate (Hz) before encoding. ``None`` keeps the input rate.
        mono: If ``True``, downmix to a single channel before encoding. Defaults to ``False``.
        channels_first: For a 2-D array, ``True`` (default) is ``(channels, frames)``, ``False``
            is ``(frames, channels)``. Must be ``None`` when ``data`` is an :class:`AudioBuffer`.

    Returns:
        The encoded audio as a ``bytes`` object.

    Raises:
        CodecpodError: If encoding fails (e.g. an unsupported sample rate for the codec).
        ValueError: If ``sample_rate`` is missing for a NumPy array, or ``channels_first`` is
            set together with an :class:`AudioBuffer`.
    """

def set_log(handler: LogLevel | LogCallback) -> None:
    """Configure how FFmpeg's internal log output is handled.

    FFmpeg emits diagnostics (e.g. ``Estimating duration from bitrate, this may be
    inaccurate``) through a process-global logger. This crate defaults to ``"quiet"``, so
    FFmpeg is silent unless the caller raises the threshold. This function overrides that
    global state; the last call wins across the whole process.

    Args:
        handler: Either a log level string, or a callable.

            A level string is one of ``"quiet"``, ``"panic"``, ``"fatal"``, ``"error"``,
            ``"warning"``, ``"info"``, ``"verbose"``, ``"debug"``, ``"trace"`` (ordered from
            least to most verbose). Messages at or below that level keep going to stderr; more
            verbose ones are dropped. Use ``"info"`` to restore FFmpeg's own default.

            A callable ``handler(level, message)`` diverts all output away from stderr: it is
            invoked once per complete log line with the level string and the formatted message
            text (the same text FFmpeg's default handler would print, including any
            ``[component @ 0x…]`` prefix, without the trailing newline). It receives messages of
            every level regardless; filter inside the callable if needed. It may be called from
            any thread, including internal codec worker threads.

    Raises:
        ValueError: If ``handler`` is a string that is not a valid level.
        TypeError: If ``handler`` is neither a string nor callable.
    """
