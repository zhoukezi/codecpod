"""Coverage for :func:`codecpod.info`, repr output, and AudioBuffer attributes."""

import codecpod
from conftest import DEFAULT_RATE, tone


def test_info_fields(tmp_path):
    sig = tone(channels=2, dur=0.5)
    path = tmp_path / "in.flac"
    codecpod.save(path, sig, DEFAULT_RATE, codec=codecpod.Flac())
    info = codecpod.info(path)
    assert info.sample_rate == DEFAULT_RATE
    assert info.channels == 2
    assert info.codec == "flac"
    assert info.frames is None or info.frames > 0
    assert info.bits_per_sample is None or info.bits_per_sample > 0


def test_audioinfo_repr(tmp_path):
    sig = tone()
    path = tmp_path / "in.wav"
    codecpod.save(path, sig, DEFAULT_RATE, codec=codecpod.Wav("i16"))
    assert repr(codecpod.info(path)).startswith("AudioInfo(")


def test_audiobuffer_repr(tmp_path):
    sig = tone(channels=2)
    path = tmp_path / "in.wav"
    codecpod.save(path, sig, DEFAULT_RATE, codec=codecpod.Wav("f32"))
    buf = codecpod.load(path, return_buffer=True)
    text = repr(buf)
    assert text.startswith("AudioBuffer(")
    assert "channels=2" in text


def test_codec_repr():
    assert repr(codecpod.Wav()) == "Wav(sample_format='i16')"
    assert repr(codecpod.Mp3(bit_rate=192000)) == (
        "Mp3(bit_rate=192000, compression_level=None)"
    )
    assert repr(codecpod.Opus(application="audio", vbr="on")) == (
        "Opus(bit_rate=None, application='audio', frame_duration=None, vbr='on')"
    )
    assert repr(codecpod.Flac(compression_level=5, bits_per_sample=24)) == (
        "Flac(compression_level=5, bits_per_sample=24)"
    )
