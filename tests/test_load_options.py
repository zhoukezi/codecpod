"""Coverage for the keyword options of :func:`codecpod.load`."""

import numpy as np
import pytest

import codecpod
from conftest import DEFAULT_RATE, dominant_freq, tone


def _write_wav(tmp_path, sig, rate=DEFAULT_RATE, fmt="f32"):
    path = tmp_path / "in.wav"
    codecpod.save(path, sig, rate, codec=codecpod.Wav(fmt))
    return path


def test_resample_on_load(tmp_path):
    sig = tone(freq=1000.0, dur=0.5)
    path = _write_wav(tmp_path, sig)
    data, sr = codecpod.load(path, sample_rate=16000)
    assert sr == 16000
    assert data.shape[-1] == pytest.approx(16000 * 0.5, rel=0.05)
    # Resampling preserves the tone frequency; the decoded 16 kHz signal stays at 1 kHz.
    assert abs(dominant_freq(data, 16000) - 1000.0) <= 30.0


def test_mono_downmix_on_load(tmp_path):
    sig = tone(channels=2)
    path = _write_wav(tmp_path, sig)
    data, sr = codecpod.load(path, mono=True)
    assert data.ndim == 1
    assert sr == DEFAULT_RATE


def test_frame_offset_and_num_frames(tmp_path):
    sig = tone(dur=0.5)  # 24000 frames mono f32
    path = _write_wav(tmp_path, sig)
    data, sr = codecpod.load(path, frame_offset=1000, num_frames=2000)
    assert data.shape[-1] == 2000
    np.testing.assert_allclose(data, sig[1000:3000], atol=1e-5)


def test_num_frames_clamped_to_available(tmp_path):
    sig = tone(dur=0.1)
    path = _write_wav(tmp_path, sig)
    data, _ = codecpod.load(path, num_frames=10**9)
    assert data.shape[-1] == len(sig)


def test_normalize_false_keeps_native_dtype(tmp_path):
    sig = tone()
    path = _write_wav(tmp_path, sig, fmt="i16")
    data, _ = codecpod.load(path, normalize=False)
    assert data.dtype == np.int16
    norm, _ = codecpod.load(path, normalize=True)
    assert norm.dtype == np.float32


def test_channels_first_orientation(tmp_path):
    sig = tone(channels=2)
    path = _write_wav(tmp_path, sig)
    planar, _ = codecpod.load(path, channels_first=True)
    interleaved, _ = codecpod.load(path, channels_first=False)
    assert planar.shape == (2, sig.shape[1])
    assert interleaved.shape == (sig.shape[1], 2)
    np.testing.assert_allclose(planar.T, interleaved, atol=1e-5)


def test_return_buffer(tmp_path):
    sig = tone(channels=2)
    path = _write_wav(tmp_path, sig)
    buf = codecpod.load(path, return_buffer=True)
    assert isinstance(buf, codecpod.AudioBuffer)
    assert buf.sample_rate == DEFAULT_RATE
    assert buf.channels == 2
    assert buf.frames == sig.shape[1]
    assert buf.layout in ("planar", "interleaved")
    assert isinstance(buf.samples, np.ndarray)
