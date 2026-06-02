"""Encode/decode throughput benchmarks for the Python public API.

These exercise the shipped surface (``codecpod.save_bytes`` / ``codecpod.load``) fully in
memory, so timings include the pyo3 boundary but not filesystem latency. They live outside
``tests/`` so the normal ``pytest tests/`` run stays fast; run them with ``pytest benchmarks/``.
"""

import numpy as np
import pytest

import codecpod

RATE = 48_000
CHANNELS = 2
DUR_SECS = 1.0

# One representative config per codec, mirroring the Rust criterion benchmark.
CODECS = {
    "wav_i16": codecpod.Wav("i16"),
    "flac": codecpod.Flac(bits_per_sample=16),
    "alac": codecpod.Alac(bits_per_sample=16),
    "aac": codecpod.Aac(),
    "mp3": codecpod.Mp3(bit_rate=192_000),
    "opus": codecpod.Opus(bit_rate=128_000),
    "vorbis": codecpod.Vorbis(quality=6.0),
}


def tone() -> np.ndarray:
    """Planar ``(channels, frames)`` f32 sine tone, matching the test/Rust-bench signal."""
    n = int(round(RATE * DUR_SECS))
    t = np.arange(n) / RATE
    chans = [
        0.4 * np.sin(2 * np.pi * (1000.0 * (1.0 + 0.05 * i)) * t)
        for i in range(CHANNELS)
    ]
    return np.asarray(chans, dtype=np.float32)


@pytest.fixture(scope="module")
def signal() -> np.ndarray:
    return tone()


@pytest.mark.parametrize("name", sorted(CODECS))
def test_encode(benchmark, signal, name):
    codec = CODECS[name]
    benchmark(codecpod.save_bytes, signal, RATE, codec)


@pytest.mark.parametrize("name", sorted(CODECS))
def test_decode(benchmark, signal, name):
    codec = CODECS[name]
    encoded = codecpod.save_bytes(signal, RATE, codec)
    benchmark(codecpod.load, encoded)
