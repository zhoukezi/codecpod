"""Shared fixtures and signal helpers for the codecpod test suite.

All test audio is synthesized with numpy so the repository ships no third-party
sample files.
"""

import numpy as np
import pytest

DEFAULT_RATE = 48000


def tone(
    freq: float = 1000.0,
    rate: int = DEFAULT_RATE,
    dur: float = 0.5,
    channels: int = 1,
    amplitude: float = 0.4,
    dtype=np.float32,
) -> np.ndarray:
    """Generate a sine tone in channels-first layout ``(channels, frames)``.

    Each channel gets a slightly different frequency so multi-channel handling and
    channel ordering can be checked. A mono signal is returned as a 1-D array.
    """
    n = int(round(rate * dur))
    t = np.arange(n) / rate
    if channels == 1:
        return (amplitude * np.sin(2 * np.pi * freq * t)).astype(dtype)
    chans = [
        amplitude * np.sin(2 * np.pi * (freq * (1.0 + 0.05 * i)) * t)
        for i in range(channels)
    ]
    return np.asarray(chans, dtype=dtype)


def dominant_freq(signal: np.ndarray, rate: int) -> float:
    """Return the frequency (Hz) of the strongest spectral component."""
    signal = np.asarray(signal, dtype=np.float64)
    signal = signal - signal.mean()
    spectrum = np.abs(np.fft.rfft(signal))
    freqs = np.fft.rfftfreq(signal.shape[0], d=1.0 / rate)
    return float(freqs[int(np.argmax(spectrum))])


def as_2d(arr: np.ndarray) -> np.ndarray:
    """Normalize a loaded array to channels-first 2-D for per-channel inspection."""
    arr = np.asarray(arr, dtype=np.float64)
    if arr.ndim == 1:
        return arr[np.newaxis, :]
    return arr


def assert_lossless(original: np.ndarray, decoded: np.ndarray, atol: float) -> None:
    """Assert two channels-first signals match sample-for-sample within ``atol``.

    Codecs without encoder delay may still emit a few trailing padding samples, so
    the comparison is done over the shared leading region.
    """
    orig = as_2d(original)
    dec = as_2d(decoded)
    assert dec.shape[0] == orig.shape[0], "channel count changed"
    n = min(orig.shape[1], dec.shape[1])
    assert n >= int(orig.shape[1] * 0.95), "decoded signal is unexpectedly short"
    np.testing.assert_allclose(dec[:, :n], orig[:, :n], atol=atol)


def assert_tone_preserved(
    original: np.ndarray, decoded: np.ndarray, rate: int, tol_hz: float = 25.0
) -> None:
    """Assert each decoded channel keeps the dominant frequency of the original.

    Robust to encoder delay, padding, and lossy quantization, so it suits the lossy
    codecs whose waveforms are not sample-aligned with the input.
    """
    orig = as_2d(original)
    dec = as_2d(decoded)
    assert dec.shape[0] == orig.shape[0], "channel count changed"
    for ch in range(orig.shape[0]):
        expected = dominant_freq(orig[ch], rate)
        actual = dominant_freq(dec[ch], rate)
        assert abs(actual - expected) <= tol_hz, (
            f"channel {ch}: dominant freq {actual:.1f} Hz != {expected:.1f} Hz"
        )


@pytest.fixture
def rate() -> int:
    return DEFAULT_RATE
