"""In-memory (byte) API: load/info from bytes and save_bytes."""

import numpy as np
import pytest

import codecpod
from conftest import DEFAULT_RATE, assert_lossless, assert_tone_preserved, tone


def test_save_bytes_returns_bytes():
    sig = tone()
    data = codecpod.save_bytes(sig, DEFAULT_RATE)  # default 16-bit WAV
    assert isinstance(data, bytes)
    assert data[:4] == b"RIFF"


@pytest.mark.parametrize("channels", [1, 2])
@pytest.mark.parametrize(
    "codec",
    [
        codecpod.Wav("i16"),
        codecpod.Wav("f32"),
        codecpod.Aiff("i24"),
        codecpod.Flac(bits_per_sample=16),
        codecpod.Mp3(bit_rate=192000),
    ],
)
def test_save_bytes_matches_file(tmp_path, codec, channels):
    """Custom-IO output must be byte-identical to the file path."""
    sig = tone(channels=channels)
    path = tmp_path / "out"
    codecpod.save(path, sig, DEFAULT_RATE, codec=codec)
    from_file = path.read_bytes()
    from_bytes = codecpod.save_bytes(sig, DEFAULT_RATE, codec=codec)
    assert from_bytes == from_file


@pytest.mark.parametrize("channels", [1, 2])
@pytest.mark.parametrize(
    "codec, tol",
    [
        (codecpod.Wav("i16"), 1e-3),
        (codecpod.Wav("f32"), 1e-5),
        (codecpod.Aiff("i24"), 1e-4),
        (codecpod.Flac(bits_per_sample=24), 1e-4),
    ],
)
def test_bytes_roundtrip_lossless(codec, tol, channels):
    sig = tone(channels=channels)
    encoded = codecpod.save_bytes(sig, DEFAULT_RATE, codec=codec)
    data, sr = codecpod.load(encoded)
    assert sr == DEFAULT_RATE
    assert_lossless(sig, data, tol)


@pytest.mark.parametrize(
    "codec",
    [codecpod.Mp3(bit_rate=192000), codecpod.Opus(bit_rate=128000)],
)
def test_bytes_roundtrip_lossy(codec):
    sig = tone(dur=1.0)
    encoded = codecpod.save_bytes(sig, DEFAULT_RATE, codec=codec)
    data, sr = codecpod.load(encoded)
    assert sr == DEFAULT_RATE
    assert_tone_preserved(sig, data, DEFAULT_RATE)


def test_load_bytes_matches_file(tmp_path):
    sig = tone(channels=2)
    path = tmp_path / "out.flac"
    codecpod.save(path, sig, DEFAULT_RATE, codec=codecpod.Flac())
    from_path, sr_path = codecpod.load(path)
    from_bytes, sr_bytes = codecpod.load(path.read_bytes())
    assert sr_path == sr_bytes
    np.testing.assert_array_equal(from_path, from_bytes)


def test_info_bytes_matches_file(tmp_path):
    sig = tone(channels=2)
    path = tmp_path / "out.flac"
    codecpod.save(path, sig, DEFAULT_RATE, codec=codecpod.Flac())
    blob = path.read_bytes()
    a = codecpod.info(path)
    b = codecpod.info(blob)
    assert (a.sample_rate, a.channels, a.codec) == (b.sample_rate, b.channels, b.codec)
    assert b.sample_rate == DEFAULT_RATE
    assert b.channels == 2


def test_load_bytes_honors_options():
    sig = tone(channels=2)
    encoded = codecpod.save_bytes(sig, DEFAULT_RATE, codec=codecpod.Wav("f32"))
    data, sr = codecpod.load(encoded, mono=True, num_frames=1000)
    assert sr == DEFAULT_RATE
    assert data.ndim == 1
    assert data.shape[0] == 1000


def test_save_bytes_from_buffer():
    sig = tone(channels=2)
    encoded_path_style = codecpod.save_bytes(sig, DEFAULT_RATE, codec=codecpod.Flac())
    buf = codecpod.load(encoded_path_style, return_buffer=True)
    # An AudioBuffer carries its own sample rate and layout.
    re_encoded = codecpod.save_bytes(buf, codec=codecpod.Flac())
    data, sr = codecpod.load(re_encoded)
    assert sr == DEFAULT_RATE
    assert_lossless(sig, data, 1e-3)


def test_save_bytes_resample_and_mono():
    sig = tone(channels=2)
    encoded = codecpod.save_bytes(
        sig, DEFAULT_RATE, codec=codecpod.Wav("f32"), resample_to=24000, mono=True
    )
    data, sr = codecpod.load(encoded)
    assert sr == 24000
    assert data.ndim == 1


def test_load_bytes_invalid_raises():
    with pytest.raises(codecpod.CodecpodError):
        codecpod.load(b"not an audio file")
