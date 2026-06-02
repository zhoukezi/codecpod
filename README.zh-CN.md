# codecpod

[English](README.md) | **简体中文**

一个音频编解码库，静态链接 [FFmpeg](https://ffmpeg.org/) ，避免预装依赖项的复杂性。提供 Rust API 和 Python bindings。

## 特性

- 自带静态 FFmpeg，运行时零外部动态库依赖。
- 解码：AAC、AC-3、ALAC、APE、FLAC、MP1/2/3、Opus、Vorbis、WavPack、各类 PCM/ADPCM 等。
- 编码：WAV、AIFF、FLAC、ALAC、AAC、MP3(LAME)、Opus、Vorbis。
- 可选重采样、下混、区间截取、归一化等。

## 安装

```bash
pip install codecpod
```

PyPI 上仅提供 **Linux x86_64** 的预编译 wheel，目前也只支持这一个平台。

## 从源码构建

构建需要以下工具链：

- Rust 工具链，建议使用最新 stable
- `nasm`、`clang` 与 `libclang`
- `make`、`pkg-config`

构建并安装到当前环境:

```bash
uvx maturin develop --release
```

或产出 wheel:

```bash
uvx maturin build --release
```

> 首次构建会按 `build.rs` 中固定的版本与 SHA-256 从上游下载 FFmpeg 及其依赖项的源码 tarball，耗时较长，请确保网络通畅。
>
> 如果设置了环境变量 `CODECPOD_VENDOR_DIR`，构建时将跳过下载直接使用其中的源码。关于目录细节，请参考 [`build.rs`](build.rs)。

## 用法

```python
import codecpod
import numpy as np

# 读取音频元数据
info = codecpod.info("input.flac")
print(info.sample_rate, info.channels, info.frames, info.codec)

# 解码音频，默认返回 (ndarray, sample_rate)，channels_first=True
waveform, sample_rate = codecpod.load("input.flac")

# 解码时重采样到 16 kHz 并下混
waveform, sample_rate = codecpod.load(
    "input.flac",
    sample_rate=16000,
    mono=True,
)

# 指定编码器参数编码为 MP3
codecpod.save(
    "output.mp3",
    waveform,
    sample_rate,
    codec=codecpod.Mp3(bit_rate=192000),
)
```

## 许可证

Copyright (C) 2026 zhoukz \<me@zhoukz.com\>

本项目以 **LGPL-2.1-or-later** 分发，详见 [`LICENSE`](LICENSE)。

项目静态链接了多个第三方库，详见 [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md)。
