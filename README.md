# codecpod

**English** | [简体中文](README.zh-CN.md)

An audio codec library that statically links [FFmpeg](https://ffmpeg.org/), avoiding the
hassle of pre-installed dependencies. Provides both a Rust API and Python bindings.

## Features

- Bundles a static FFmpeg; zero external shared-library dependencies at runtime.
- Decoding: AAC, AC-3, ALAC, APE, FLAC, MP1/2/3, Opus, Vorbis, WavPack, various PCM/ADPCM, and more.
- Encoding: WAV, AIFF, FLAC, ALAC, AAC, MP3 (LAME), Opus, Vorbis.
- Optional resampling, downmixing, range trimming, normalization, and more.

## Installation

```bash
pip install codecpod
```

Pre-built wheels are published to PyPI for **Linux x86_64 only**, which is currently the
only supported platform.

## Building from source

Building requires the following toolchain:

- A Rust toolchain; the latest stable is recommended.
- `nasm`, `clang`, and `libclang`.
- `make`, `pkg-config`.

Build and install into the current environment:

```bash
uvx maturin develop --release
```

Or produce a wheel:

```bash
uvx maturin build --release
```

> The first build downloads the FFmpeg and dependency source tarballs from upstream,
> pinned to the exact versions and SHA-256 checksums in `build.rs`. This takes a while, so
> make sure your network connection is stable.
>
> If the `CODECPOD_VENDOR_DIR` environment variable is set, the build skips downloading and
> uses the sources there directly. See [`build.rs`](build.rs) for the directory layout.

## Usage

```python
import codecpod
import numpy as np

# Read audio metadata
info = codecpod.info("input.flac")
print(info.sample_rate, info.channels, info.frames, info.codec)

# Decode audio; returns (ndarray, sample_rate) by default, with channels_first=True
waveform, sample_rate = codecpod.load("input.flac")

# Resample to 16 kHz and downmix to mono while decoding
waveform, sample_rate = codecpod.load(
    "input.flac",
    sample_rate=16000,
    mono=True,
)

# Encode to MP3 with explicit encoder parameters
codecpod.save(
    "output.mp3",
    waveform,
    sample_rate,
    codec=codecpod.Mp3(bit_rate=192000),
)
```

## License

Copyright (C) 2026 zhoukz \<me@zhoukz.com\>

This project is distributed under **LGPL-2.1-or-later**; see [`LICENSE`](LICENSE).

It statically links several third-party libraries; see
[`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md) for details.
