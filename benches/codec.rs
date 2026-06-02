//! Encode/decode throughput benchmarks for the core (non-Python) API.
//!
//! Everything runs in memory via `save_bytes` / `load_bytes`, so the timings reflect
//! codec work rather than filesystem latency. Run with `cargo bench`.

use std::hint::black_box;

use codecpod::{
    AlacBitsPerSample, AudioBuffer, ChannelLayout, Codec, FlacBitsPerSample, LoadOptions,
    SampleData, SaveOptions, WavSampleFormat, load_bytes, save_bytes,
};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};

const RATE: u32 = 48_000;
const CHANNELS: u32 = 2;
const DUR_SECS: f64 = 1.0;

/// Planar `[C, F]` sine tone in `f32`, mirroring the Python test fixture: amplitude 0.4 and
/// a slightly different frequency per channel so multi-channel paths get exercised.
fn tone() -> AudioBuffer {
    let frames = (RATE as f64 * DUR_SECS).round() as u64;
    let mut samples = Vec::with_capacity((frames * CHANNELS as u64) as usize);
    for ch in 0..CHANNELS {
        let freq = 1000.0 * (1.0 + 0.05 * ch as f64);
        for n in 0..frames {
            let t = n as f64 / RATE as f64;
            samples.push((0.4 * (2.0 * std::f64::consts::PI * freq * t).sin()) as f32);
        }
    }
    AudioBuffer {
        samples: SampleData::F32(samples),
        channels: CHANNELS,
        frames,
        sample_rate: RATE,
        layout: ChannelLayout::Planar,
    }
}

/// Codecs to benchmark, with a representative config each (mirrors the round-trip test matrix).
fn codecs() -> Vec<(&'static str, Codec)> {
    vec![
        (
            "wav_i16",
            Codec::Wav {
                sample_format: WavSampleFormat::I16,
            },
        ),
        (
            "flac",
            Codec::Flac {
                compression_level: None,
                bits_per_sample: FlacBitsPerSample::Bits16,
            },
        ),
        (
            "alac",
            Codec::Alac {
                bits_per_sample: AlacBitsPerSample::Bits16,
            },
        ),
        ("aac", Codec::Aac { bit_rate: None }),
        (
            "mp3",
            Codec::Mp3 {
                bit_rate: Some(192_000),
                compression_level: None,
            },
        ),
        (
            "opus",
            Codec::Opus {
                bit_rate: Some(128_000),
                application: None,
                frame_duration: None,
                vbr: None,
            },
        ),
        (
            "vorbis",
            Codec::Vorbis {
                quality: Some(6.0),
                bit_rate: None,
            },
        ),
    ]
}

fn bench_encode(c: &mut Criterion) {
    let buf = tone();
    let mut group = c.benchmark_group("encode");
    group.throughput(Throughput::Elements(buf.frames));
    for (name, codec) in codecs() {
        let opts = SaveOptions {
            codec,
            sample_rate: None,
            mono: false,
        };
        group.bench_function(name, |b| {
            b.iter(|| save_bytes(black_box(&buf), black_box(&opts)).unwrap());
        });
    }
    group.finish();
}

fn bench_decode(c: &mut Criterion) {
    let buf = tone();
    let mut group = c.benchmark_group("decode");
    group.throughput(Throughput::Elements(buf.frames));
    for (name, codec) in codecs() {
        let opts = SaveOptions {
            codec,
            sample_rate: None,
            mono: false,
        };
        let encoded = save_bytes(&buf, &opts).unwrap();
        let load_opts = LoadOptions::default();
        group.bench_function(name, |b| {
            b.iter(|| load_bytes(black_box(&encoded), black_box(&load_opts)).unwrap());
        });
    }
    group.finish();
}

criterion_group!(benches, bench_encode, bench_decode);
criterion_main!(benches);
