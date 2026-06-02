"""Error paths and argument validation."""

import numpy as np
import pytest

import codecpod
from conftest import DEFAULT_RATE, tone


def test_info_missing_file(tmp_path):
    with pytest.raises(codecpod.CodecpodError):
        codecpod.info(tmp_path / "nope.wav")


def test_load_missing_file(tmp_path):
    with pytest.raises(codecpod.CodecpodError):
        codecpod.load(tmp_path / "nope.wav")


def test_load_zero_sample_rate(tmp_path):
    sig = tone()
    path = tmp_path / "in.wav"
    codecpod.save(path, sig, DEFAULT_RATE, codec=codecpod.Wav("f32"))
    with pytest.raises(codecpod.CodecpodError):
        codecpod.load(path, sample_rate=0)


def test_load_zero_num_frames(tmp_path):
    sig = tone()
    path = tmp_path / "in.wav"
    codecpod.save(path, sig, DEFAULT_RATE, codec=codecpod.Wav("f32"))
    with pytest.raises(codecpod.CodecpodError):
        codecpod.load(path, num_frames=0)


def test_save_numpy_without_sample_rate(tmp_path):
    sig = tone()
    with pytest.raises(ValueError):
        codecpod.save(tmp_path / "out.wav", sig)


def test_save_buffer_with_channels_first(tmp_path):
    sig = tone(channels=2)
    src = tmp_path / "src.wav"
    codecpod.save(src, sig, DEFAULT_RATE, codec=codecpod.Wav("f32"))
    buf = codecpod.load(src, return_buffer=True)
    with pytest.raises(ValueError):
        codecpod.save(tmp_path / "out.wav", buf, channels_first=True)


def test_save_rejects_non_codec(tmp_path):
    sig = tone()
    with pytest.raises(TypeError):
        codecpod.save(tmp_path / "out.wav", sig, DEFAULT_RATE, codec="flac")


def test_save_rejects_unsupported_dtype(tmp_path):
    sig = (tone() * 100).astype(np.int8)
    with pytest.raises(TypeError):
        codecpod.save(tmp_path / "out.wav", sig, DEFAULT_RATE, codec=codecpod.Wav("i16"))


def test_save_rejects_3d_array(tmp_path):
    sig = np.zeros((2, 2, 10), dtype=np.float32)
    with pytest.raises(ValueError):
        codecpod.save(tmp_path / "out.wav", sig, DEFAULT_RATE, codec=codecpod.Wav("f32"))


def test_opus_unsupported_sample_rate(tmp_path):
    sig = tone(rate=44100, dur=0.3)
    with pytest.raises(codecpod.CodecpodError):
        codecpod.save(tmp_path / "out.ogg", sig, 44100, codec=codecpod.Opus())


@pytest.mark.parametrize(
    "factory",
    [
        lambda: codecpod.Wav("bogus"),
        lambda: codecpod.Aiff("bogus"),
        lambda: codecpod.Flac(bits_per_sample=20),
        lambda: codecpod.Alac(bits_per_sample=20),
        lambda: codecpod.Opus(application="bad"),
        lambda: codecpod.Opus(frame_duration=7.0),
        lambda: codecpod.Opus(vbr="bad"),
    ],
)
def test_codec_argument_validation(factory):
    with pytest.raises(ValueError):
        factory()
