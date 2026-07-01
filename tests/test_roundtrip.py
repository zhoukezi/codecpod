"""Encode -> decode round-trip checks for every supported encoder."""

import pytest
from conftest import DEFAULT_RATE, assert_lossless, assert_tone_preserved, tone

import codecpod

WAV_FORMATS = ["u8", "i16", "i24", "i32", "f32", "f64"]
AIFF_FORMATS = ["i8", "i16", "i24", "i32", "f32", "f64"]

# Tolerance after a normalize=True (-> float32 in [-1, 1]) round trip, per bit depth.
FMT_TOL = {
    "u8": 2e-2,
    "i8": 2e-2,
    "i16": 1e-3,
    "i24": 1e-4,
    "i32": 1e-4,
    "f32": 1e-5,
    "f64": 1e-5,
}


@pytest.mark.parametrize("channels", [1, 2])
@pytest.mark.parametrize("fmt", WAV_FORMATS)
def test_wav_roundtrip(tmp_path, fmt, channels):
    sig = tone(channels=channels)
    path = tmp_path / "out.wav"
    codecpod.save(path, sig, DEFAULT_RATE, codec=codecpod.Wav(fmt))
    data, sr = codecpod.load(path)
    assert sr == DEFAULT_RATE
    assert_lossless(sig, data, FMT_TOL[fmt])


@pytest.mark.parametrize("channels", [1, 2])
@pytest.mark.parametrize("fmt", AIFF_FORMATS)
def test_aiff_roundtrip(tmp_path, fmt, channels):
    sig = tone(channels=channels)
    path = tmp_path / "out.aiff"
    codecpod.save(path, sig, DEFAULT_RATE, codec=codecpod.Aiff(fmt))
    data, sr = codecpod.load(path)
    assert sr == DEFAULT_RATE
    assert_lossless(sig, data, FMT_TOL[fmt])


@pytest.mark.parametrize("channels", [1, 2])
@pytest.mark.parametrize("bits", [16, 24])
def test_flac_roundtrip(tmp_path, bits, channels):
    sig = tone(channels=channels)
    path = tmp_path / "out.flac"
    codecpod.save(path, sig, DEFAULT_RATE, codec=codecpod.Flac(bits_per_sample=bits))
    data, sr = codecpod.load(path)
    assert sr == DEFAULT_RATE
    assert_lossless(sig, data, 1e-3 if bits == 16 else 1e-4)


@pytest.mark.parametrize("channels", [1, 2])
@pytest.mark.parametrize("bits", [16, 24])
def test_alac_roundtrip(tmp_path, bits, channels):
    sig = tone(channels=channels)
    path = tmp_path / "out.m4a"
    codecpod.save(path, sig, DEFAULT_RATE, codec=codecpod.Alac(bits_per_sample=bits))
    data, sr = codecpod.load(path)
    assert sr == DEFAULT_RATE
    # ALAC is lossless but the m4a container may introduce priming delay, so verify
    # the tone rather than sample alignment.
    assert_tone_preserved(sig, data, DEFAULT_RATE)


LOSSY_CODECS = {
    "aac": (codecpod.Aac(), "m4a"),
    "mp3": (codecpod.Mp3(bit_rate=192000), "mp3"),
    "opus": (codecpod.Opus(bit_rate=128000), "ogg"),
    "vorbis": (codecpod.Vorbis(quality=6.0), "ogg"),
}


@pytest.mark.parametrize("channels", [1, 2])
@pytest.mark.parametrize("name", sorted(LOSSY_CODECS))
def test_lossy_roundtrip(tmp_path, name, channels):
    codec, ext = LOSSY_CODECS[name]
    sig = tone(channels=channels, dur=1.0)
    path = tmp_path / f"out.{ext}"
    codecpod.save(path, sig, DEFAULT_RATE, codec=codec)
    data, sr = codecpod.load(path)
    assert sr == DEFAULT_RATE
    assert_tone_preserved(sig, data, DEFAULT_RATE)
    # Duration should survive within encoder delay/padding margins.
    frames = data.shape[-1]
    assert sig.shape[-1] * 0.8 <= frames <= sig.shape[-1] * 1.3
