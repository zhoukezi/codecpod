"""Coverage for the keyword options of :func:`codecpod.save`."""

import numpy as np
import pytest

import codecpod
from conftest import DEFAULT_RATE, assert_lossless, assert_tone_preserved, tone


def test_default_codec_is_lossless_wav(tmp_path):
    sig = tone()
    path = tmp_path / "out.wav"
    codecpod.save(path, sig, DEFAULT_RATE)  # no codec -> 16-bit WAV
    data, sr = codecpod.load(path)
    assert sr == DEFAULT_RATE
    assert_lossless(sig, data, 1e-3)


def test_save_resample_to(tmp_path):
    sig = tone(dur=0.5)
    path = tmp_path / "out.wav"
    codecpod.save(path, sig, DEFAULT_RATE, codec=codecpod.Wav("f32"), resample_to=24000)
    data, sr = codecpod.load(path)
    assert sr == 24000
    assert data.shape[-1] == pytest.approx(24000 * 0.5, rel=0.05)


def test_save_mono_downmix(tmp_path):
    sig = tone(channels=2)
    path = tmp_path / "out.wav"
    codecpod.save(path, sig, DEFAULT_RATE, codec=codecpod.Wav("f32"), mono=True)
    data, _ = codecpod.load(path)
    assert data.ndim == 1


def test_save_channels_first_false(tmp_path):
    sig = tone(channels=2)  # (2, frames)
    interleaved = np.ascontiguousarray(sig.T)  # (frames, 2)
    path = tmp_path / "out.wav"
    codecpod.save(
        path, interleaved, DEFAULT_RATE, codec=codecpod.Wav("f32"), channels_first=False
    )
    data, _ = codecpod.load(path, channels_first=True)
    assert_lossless(sig, data, 1e-5)


def test_save_from_audiobuffer(tmp_path):
    sig = tone(channels=2)
    src = tmp_path / "src.wav"
    codecpod.save(src, sig, DEFAULT_RATE, codec=codecpod.Wav("f32"))
    buf = codecpod.load(src, return_buffer=True)

    dst = tmp_path / "dst.flac"
    codecpod.save(dst, buf, codec=codecpod.Flac())  # sample_rate taken from buffer
    data, sr = codecpod.load(dst)
    assert sr == DEFAULT_RATE
    assert_tone_preserved(sig, data, DEFAULT_RATE)


def test_save_buffer_interleaved_layout(tmp_path):
    sig = tone(channels=2)
    src = tmp_path / "src.wav"
    codecpod.save(src, sig, DEFAULT_RATE, codec=codecpod.Wav("f32"))
    buf = codecpod.load(src, return_buffer=True)  # planar buffer
    dst = tmp_path / "dst.wav"
    codecpod.save(dst, buf, codec=codecpod.Wav("f32"))
    data, _ = codecpod.load(dst)
    assert_lossless(sig, data, 1e-5)
