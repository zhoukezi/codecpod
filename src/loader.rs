use std::ffi::{CStr, CString};
use std::path::Path;
use std::ptr::{self, NonNull};

use crate::avio::ReadAvio;
use crate::error::{Error, FFmpegError};
use crate::sys;
use crate::util::{Frame, Packet};
use crate::{AudioBuffer, AudioInfo, ChannelLayout, LoadOptions, SampleData};

const INITIAL_CAPACITY: usize = 4096;

// INVARIANT: self.0 points to an AVFormatContext that has been successfully opened
// (via avformat_open_input) and populated by avformat_find_stream_info. self.1, when
// present, owns the custom AVIO backing an in-memory input and must outlive self.0;
// field declaration order guarantees self.0 is closed before self.1 is freed.
struct FormatCtx<'a>(
    NonNull<sys::AVFormatContext>,
    // Held only to keep the custom AVIO (and the borrowed input bytes) alive until the
    // format context is closed; never read directly.
    #[allow(dead_code)] Option<ReadAvio<'a>>,
);

impl<'a> FormatCtx<'a> {
    // Finish opening a context whose pointer has been written by avformat_open_input
    // (and whose AVIO, if any, is `avio`), then probe its streams.
    fn finish(ptr: *mut sys::AVFormatContext, avio: Option<ReadAvio<'a>>) -> Result<Self, Error> {
        // SAFETY: ptr is the non-null pointer written by avformat_open_input on success.
        unsafe {
            let ctx = FormatCtx(NonNull::new_unchecked(ptr), avio);
            let ret = sys::avformat_find_stream_info(ctx.0.as_ptr(), ptr::null_mut());
            if ret < 0 {
                return Err(Error::FindStreamInfo(FFmpegError(ret)));
            }
            Ok(ctx)
        }
    }

    fn open_path(path: &CStr) -> Result<Self, Error> {
        // SAFETY: All FFmpeg arguments are locally owned; path is a valid NUL-terminated &CStr.
        // An incompletely constructed FormatCtx on the error path is released via Drop.
        unsafe {
            let mut ptr: *mut sys::AVFormatContext = ptr::null_mut();
            let ret =
                sys::avformat_open_input(&mut ptr, path.as_ptr(), ptr::null(), ptr::null_mut());
            if ret < 0 {
                return Err(Error::OpenInput(FFmpegError(ret)));
            }
            Self::finish(ptr, None)
        }
    }

    fn open_bytes(data: &'a [u8]) -> Result<Self, Error> {
        let avio = ReadAvio::new(data)?;
        // SAFETY: avformat_alloc_context returns null or a valid context. pb is set to the
        // custom AVIO and AVFMT_FLAG_CUSTOM_IO marks it caller-owned so avformat_close_input
        // will not free it; on open failure FFmpeg frees the context and the AVIO is released
        // by `avio`'s Drop, so there is no double free.
        unsafe {
            let ctx = sys::avformat_alloc_context();
            if ctx.is_null() {
                return Err(Error::OutOfMemory);
            }
            (*ctx).pb = avio.as_ptr();
            (*ctx).flags |= sys::AVFMT_FLAG_CUSTOM_IO as i32;

            let mut ptr = ctx;
            let ret = sys::avformat_open_input(&mut ptr, ptr::null(), ptr::null(), ptr::null_mut());
            if ret < 0 {
                return Err(Error::OpenInput(FFmpegError(ret)));
            }
            Self::finish(ptr, Some(avio))
        }
    }
}

impl Drop for FormatCtx<'_> {
    fn drop(&mut self) {
        // SAFETY: The type invariant guarantees self.0 is a valid AVFormatContext.
        // avformat_close_input takes &mut *mut to null out the caller's pointer; since NonNull
        // cannot hold null, a local variable is used instead. With AVFMT_FLAG_CUSTOM_IO set it
        // leaves the custom AVIO untouched, which self.1 frees afterwards.
        unsafe {
            let mut p = self.0.as_ptr();
            sys::avformat_close_input(&mut p);
        }
    }
}

// INVARIANT: self.0 points to an AVCodecContext that has been successfully allocated,
// populated via avcodec_parameters_to_context, and opened by avcodec_open2.
struct CodecCtx(NonNull<sys::AVCodecContext>);

// The time base of the audio stream associated with this CodecCtx, stored as a (num, den)
// pair so that Copy can be derived without depending on AVRational.
#[derive(Clone, Copy)]
struct StreamTimeBase {
    num: i32,
    den: i32,
}

impl StreamTimeBase {
    fn as_av(self) -> sys::AVRational {
        sys::AVRational {
            num: self.num,
            den: self.den,
        }
    }
}

impl CodecCtx {
    fn open_audio(fmt: &FormatCtx<'_>) -> Result<(i32, Self, StreamTimeBase), Error> {
        // SAFETY: fmt upholds the FormatCtx invariant, so fmt.0 is valid and the streams array
        // is dereferenceable in [0, nb_streams). av_find_best_stream guarantees idx is either
        // negative or within that range. An incompletely constructed CodecCtx on the error path
        // is released via Drop.
        unsafe {
            let idx = sys::av_find_best_stream(
                fmt.0.as_ptr(),
                sys::AVMEDIA_TYPE_AUDIO,
                -1,
                -1,
                ptr::null_mut(),
                0,
            );
            if idx < 0 {
                return Err(Error::NoAudioStream);
            }
            let stream = *(*fmt.0.as_ptr()).streams.add(idx as usize);
            let codecpar = (*stream).codecpar;
            let codec = sys::avcodec_find_decoder((*codecpar).codec_id);
            if codec.is_null() {
                return Err(Error::DecoderNotFound((*codecpar).codec_id as u32));
            }

            let ptr =
                NonNull::new(sys::avcodec_alloc_context3(codec)).ok_or(Error::AllocCodecContext)?;
            let ctx = CodecCtx(ptr);

            let ret = sys::avcodec_parameters_to_context(ctx.0.as_ptr(), codecpar);
            if ret < 0 {
                return Err(Error::CodecParameters(FFmpegError(ret)));
            }

            let stb = StreamTimeBase {
                num: (*stream).time_base.num,
                den: (*stream).time_base.den,
            };
            // Inform the decoder of the packet time base so that PTS is correctly adjusted
            // across pre-skip / end-trim. The opus decoder prints a "Could not update
            // timestamps" warning when this value is absent.
            (*ctx.0.as_ptr()).pkt_timebase = stb.as_av();

            let ret = sys::avcodec_open2(ctx.0.as_ptr(), codec, ptr::null_mut());
            if ret < 0 {
                return Err(Error::OpenCodec(FFmpegError(ret)));
            }
            Ok((idx, ctx, stb))
        }
    }

    fn channels(&self) -> i32 {
        // SAFETY: The type invariant guarantees self.0 is valid; ch_layout is populated by avcodec_open2.
        unsafe { (*self.0.as_ptr()).ch_layout.nb_channels }
    }

    fn sample_rate(&self) -> i32 {
        // SAFETY: The type invariant guarantees self.0 is valid; sample_rate is populated by avcodec_open2.
        unsafe { (*self.0.as_ptr()).sample_rate }
    }

    fn sample_fmt(&self) -> i32 {
        // SAFETY: The type invariant guarantees self.0 is valid; sample_fmt is populated by avcodec_open2.
        unsafe { (*self.0.as_ptr()).sample_fmt }
    }
}

impl Drop for CodecCtx {
    fn drop(&mut self) {
        // SAFETY: The type invariant guarantees self.0 is valid. avcodec_free_context takes
        // &mut *mut to null out the caller's pointer; since NonNull cannot hold null, a local
        // variable is used instead.
        unsafe {
            let mut p = self.0.as_ptr();
            sys::avcodec_free_context(&mut p);
        }
    }
}

// INVARIANT: self.0 points to a SwrContext that has been successfully allocated via
// swr_alloc_set_opts2 and initialized by swr_init.
struct SwrCtx(NonNull<sys::SwrContext>);

impl SwrCtx {
    fn new(
        input: &CodecCtx,
        out_channels: i32,
        out_rate: i32,
        out_fmt: i32,
    ) -> Result<Self, Error> {
        // SAFETY: input upholds the CodecCtx invariant, so input.0 is valid and ch_layout /
        // sample_fmt / sample_rate are all populated. out_layout lives on the stack, zero-
        // initialized and then filled by av_channel_layout_default; av_channel_layout_uninit is
        // called before returning regardless of swr_alloc_set_opts2's result. An incompletely
        // constructed SwrCtx on the error path is released via Drop.
        unsafe {
            let mut out_layout = std::mem::zeroed::<sys::AVChannelLayout>();
            sys::av_channel_layout_default(&mut out_layout, out_channels);

            let mut ptr: *mut sys::SwrContext = ptr::null_mut();
            let ret = sys::swr_alloc_set_opts2(
                &mut ptr,
                &out_layout,
                out_fmt as _,
                out_rate,
                &(*input.0.as_ptr()).ch_layout,
                (*input.0.as_ptr()).sample_fmt,
                (*input.0.as_ptr()).sample_rate,
                0,
                ptr::null_mut(),
            );
            sys::av_channel_layout_uninit(&mut out_layout);
            if ret < 0 {
                return Err(Error::SwrAlloc(FFmpegError(ret)));
            }
            // SAFETY: swr_alloc_set_opts2 returns >= 0 on success and writes a non-null pointer.
            let ctx = SwrCtx(NonNull::new_unchecked(ptr));

            let ret = sys::swr_init(ctx.0.as_ptr());
            if ret < 0 {
                return Err(Error::SwrInit(FFmpegError(ret)));
            }
            Ok(ctx)
        }
    }
}

impl Drop for SwrCtx {
    fn drop(&mut self) {
        // SAFETY: The type invariant guarantees self.0 is valid. swr_free takes &mut *mut to
        // null out the caller's pointer; since NonNull cannot hold null, a local variable is
        // used instead.
        unsafe {
            let mut p = self.0.as_ptr();
            sys::swr_free(&mut p);
        }
    }
}

// ===== info =====

pub fn info(path: &Path) -> Result<AudioInfo, Error> {
    let cpath = CString::new(path.as_os_str().to_string_lossy().as_bytes())?;
    info_from_ctx(&FormatCtx::open_path(&cpath)?)
}

pub fn info_bytes(data: &[u8]) -> Result<AudioInfo, Error> {
    info_from_ctx(&FormatCtx::open_bytes(data)?)
}

fn info_from_ctx(fmt: &FormatCtx<'_>) -> Result<AudioInfo, Error> {
    // SAFETY: fmt upholds the FormatCtx invariant; idx returned by av_find_best_stream is
    // either negative or within [0, nb_streams). codecpar is populated by
    // avformat_find_stream_info.
    unsafe {
        let idx = sys::av_find_best_stream(
            fmt.0.as_ptr(),
            sys::AVMEDIA_TYPE_AUDIO,
            -1,
            -1,
            ptr::null_mut(),
            0,
        );
        if idx < 0 {
            return Err(Error::NoAudioStream);
        }
        let stream = *(*fmt.0.as_ptr()).streams.add(idx as usize);
        let codecpar = (*stream).codecpar;

        let sample_rate = (*codecpar).sample_rate;
        let channels = (*codecpar).ch_layout.nb_channels;
        if sample_rate <= 0 || channels <= 0 {
            return Err(Error::InvalidStreamParameters {
                channels,
                sample_rate,
            });
        }

        // Frame count: rescale duration from the stream time_base to 1/sample_rate.
        //
        // stream.nb_frames is intentionally avoided — in most audio containers its semantics
        // are packet/access-unit count rather than PCM sample count (especially apparent in
        // m4a/mov: AAC has 1024 samples per packet, so treating it as sample count would be
        // off by three orders of magnitude). Returns None when duration is unavailable.
        let dur = (*stream).duration;
        let frames = if dur > 0 && dur != sys::AV_NOPTS_VALUE {
            let rescaled = sys::av_rescale_q(
                dur,
                sys::AVRational {
                    num: (*stream).time_base.num,
                    den: (*stream).time_base.den,
                },
                sys::AVRational {
                    num: 1,
                    den: sample_rate,
                },
            );
            if rescaled > 0 {
                Some(rescaled as u64)
            } else {
                None
            }
        } else {
            None
        };

        let bps_raw = (*codecpar).bits_per_raw_sample;
        let bps_coded = (*codecpar).bits_per_coded_sample;
        let bits_per_sample = if bps_raw > 0 {
            Some(bps_raw as u32)
        } else if bps_coded > 0 {
            Some(bps_coded as u32)
        } else {
            let bytes = sys::av_get_bytes_per_sample((*codecpar).format as _);
            if bytes > 0 {
                Some((bytes * 8) as u32)
            } else {
                None
            }
        };

        let name_ptr = sys::avcodec_get_name((*codecpar).codec_id);
        let codec = if name_ptr.is_null() {
            None
        } else {
            Some(CStr::from_ptr(name_ptr).to_string_lossy().into_owned())
        };

        // sample_rate and channels have already been validated as > 0 above; casting to u32 is safe.
        Ok(AudioInfo {
            sample_rate: sample_rate as u32,
            channels: channels as u32,
            frames,
            bits_per_sample,
            codec,
        })
    }
}

// ===== load =====

// Select the swr output sample format based on (input fmt, normalize, channels_first).
// When `normalize` is false the output is kept in its source packed form; all 6 native packed
// formats defined by FFmpeg (U8/S16/S32/S64/FLT/DBL) have a corresponding SampleData variant.
// The rare unrecognized formats fall back to FLT — silently truncating to integer would be worse.
fn pick_out_fmt(in_fmt: i32, normalize: bool, channels_first: bool) -> i32 {
    let packed = if normalize {
        sys::AV_SAMPLE_FMT_FLT
    } else {
        // SAFETY: av_get_packed_sample_fmt accepts any AVSampleFormat (returns NONE for unknown).
        let p = unsafe { sys::av_get_packed_sample_fmt(in_fmt as _) };
        if p == sys::AV_SAMPLE_FMT_U8
            || p == sys::AV_SAMPLE_FMT_S16
            || p == sys::AV_SAMPLE_FMT_S32
            || p == sys::AV_SAMPLE_FMT_S64
            || p == sys::AV_SAMPLE_FMT_FLT
            || p == sys::AV_SAMPLE_FMT_DBL
        {
            p
        } else {
            sys::AV_SAMPLE_FMT_FLT
        }
    };

    if channels_first {
        // SAFETY: `packed` is one of the valid packed formats enumerated above.
        unsafe { sys::av_get_planar_sample_fmt(packed) as i32 }
    } else {
        packed as i32
    }
}

trait Sample: Copy + Default + 'static {
    fn into_planar(buf: Vec<Vec<Self>>) -> SampleData;
    fn into_packed(buf: Vec<Self>) -> SampleData;
}

impl Sample for f64 {
    fn into_planar(buf: Vec<Vec<f64>>) -> SampleData {
        SampleData::F64(flatten_planes(buf))
    }
    fn into_packed(buf: Vec<f64>) -> SampleData {
        SampleData::F64(buf)
    }
}

impl Sample for f32 {
    fn into_planar(buf: Vec<Vec<f32>>) -> SampleData {
        SampleData::F32(flatten_planes(buf))
    }
    fn into_packed(buf: Vec<f32>) -> SampleData {
        SampleData::F32(buf)
    }
}

impl Sample for i64 {
    fn into_planar(buf: Vec<Vec<i64>>) -> SampleData {
        SampleData::I64(flatten_planes(buf))
    }
    fn into_packed(buf: Vec<i64>) -> SampleData {
        SampleData::I64(buf)
    }
}

impl Sample for i32 {
    fn into_planar(buf: Vec<Vec<i32>>) -> SampleData {
        SampleData::I32(flatten_planes(buf))
    }
    fn into_packed(buf: Vec<i32>) -> SampleData {
        SampleData::I32(buf)
    }
}

impl Sample for i16 {
    fn into_planar(buf: Vec<Vec<i16>>) -> SampleData {
        SampleData::I16(flatten_planes(buf))
    }
    fn into_packed(buf: Vec<i16>) -> SampleData {
        SampleData::I16(buf)
    }
}

impl Sample for u8 {
    fn into_planar(buf: Vec<Vec<u8>>) -> SampleData {
        SampleData::U8(flatten_planes(buf))
    }
    fn into_packed(buf: Vec<u8>) -> SampleData {
        SampleData::U8(buf)
    }
}

fn flatten_planes<T: Copy>(planes: Vec<Vec<T>>) -> Vec<T> {
    let total: usize = planes.iter().map(|p| p.len()).sum();
    let mut out = Vec::with_capacity(total);
    for plane in &planes {
        out.extend_from_slice(plane);
    }
    out
}

pub fn load(path: &Path, opts: &LoadOptions) -> Result<AudioBuffer, Error> {
    let cpath = CString::new(path.as_os_str().to_string_lossy().as_bytes())?;
    load_from_ctx(FormatCtx::open_path(&cpath)?, opts)
}

pub fn load_bytes(data: &[u8], opts: &LoadOptions) -> Result<AudioBuffer, Error> {
    load_from_ctx(FormatCtx::open_bytes(data)?, opts)
}

fn load_from_ctx(fmt: FormatCtx<'_>, opts: &LoadOptions) -> Result<AudioBuffer, Error> {
    let (audio_idx, codec_ctx, stream_tb) = CodecCtx::open_audio(&fmt)?;

    let in_channels = codec_ctx.channels();
    let in_rate = codec_ctx.sample_rate();
    let in_fmt = codec_ctx.sample_fmt();
    if in_channels <= 0 || in_rate <= 0 {
        return Err(Error::InvalidStreamParameters {
            channels: in_channels,
            sample_rate: in_rate,
        });
    }

    let out_channels = if opts.mono { 1 } else { in_channels };
    let out_rate = opts.sample_rate.map(|r| r as i32).unwrap_or(in_rate);
    let out_fmt = pick_out_fmt(in_fmt, opts.normalize, opts.channels_first);

    let swr = SwrCtx::new(&codec_ctx, out_channels, out_rate, out_fmt)?;
    let pkt = Packet::new()?;
    let frame = Frame::new()?;

    // A seek with the BACKWARD flag may land before the requested position at an earlier
    // keyframe; the decode loop uses each frame's PTS to narrow the window.
    if opts.frame_offset > 0 {
        // SAFETY: fmt and codec_ctx each uphold their respective type invariants.
        unsafe {
            let target_pts = sys::av_rescale_q(
                opts.frame_offset as i64,
                sys::AVRational {
                    num: 1,
                    den: in_rate,
                },
                stream_tb.as_av(),
            );
            let ret = sys::av_seek_frame(
                fmt.0.as_ptr(),
                audio_idx,
                target_pts,
                sys::AVSEEK_FLAG_BACKWARD,
            );
            if ret < 0 {
                return Err(Error::Seek(FFmpegError(ret)));
            }
            sys::avcodec_flush_buffers(codec_ctx.0.as_ptr());
        }
    }

    // SAFETY: in_fmt comes from a successfully opened decoder, so it is a valid AVSampleFormat.
    let in_planar = unsafe { sys::av_sample_fmt_is_planar(in_fmt as _) != 0 };
    let in_bps = unsafe { sys::av_get_bytes_per_sample(in_fmt as _) } as usize;
    if in_bps == 0 {
        return Err(Error::InvalidArg(
            "decoder produced an unknown sample format",
        ));
    }

    let packed_out = unsafe { sys::av_get_packed_sample_fmt(out_fmt as _) };

    let ctx = DecodeCtx {
        fmt: fmt.0,
        codec_ctx: &codec_ctx,
        swr: &swr,
        pkt: &pkt,
        frame: &frame,
        audio_idx,
        in_channels,
        in_rate,
        out_channels,
        out_rate,
        in_planar,
        in_bps,
        stream_tb,
        opts,
    };

    if packed_out == sys::AV_SAMPLE_FMT_U8 {
        decode::<u8>(ctx)
    } else if packed_out == sys::AV_SAMPLE_FMT_S16 {
        decode::<i16>(ctx)
    } else if packed_out == sys::AV_SAMPLE_FMT_S32 {
        decode::<i32>(ctx)
    } else if packed_out == sys::AV_SAMPLE_FMT_S64 {
        decode::<i64>(ctx)
    } else if packed_out == sys::AV_SAMPLE_FMT_FLT {
        decode::<f32>(ctx)
    } else if packed_out == sys::AV_SAMPLE_FMT_DBL {
        decode::<f64>(ctx)
    } else {
        Err(Error::InvalidArg("unsupported output sample format"))
    }
}

struct DecodeCtx<'a> {
    // Raw pointer rather than &FormatCtx so the decode loop does not carry the input's
    // borrow lifetime; the owning FormatCtx is kept alive by the caller across decode().
    fmt: NonNull<sys::AVFormatContext>,
    codec_ctx: &'a CodecCtx,
    swr: &'a SwrCtx,
    pkt: &'a Packet,
    frame: &'a Frame,
    audio_idx: i32,
    in_channels: i32,
    in_rate: i32,
    out_channels: i32,
    out_rate: i32,
    in_planar: bool,
    in_bps: usize,
    stream_tb: StreamTimeBase,
    opts: &'a LoadOptions,
}

fn decode<T: Sample>(ctx: DecodeCtx<'_>) -> Result<AudioBuffer, Error> {
    let DecodeCtx {
        fmt,
        codec_ctx,
        swr,
        pkt,
        frame,
        audio_idx,
        in_channels,
        in_rate,
        out_channels,
        out_rate,
        in_planar,
        in_bps,
        stream_tb,
        opts,
    } = ctx;

    let n_out = out_channels as usize;
    let channels_first = opts.channels_first;
    let n_in_planes = if in_planar { in_channels as usize } else { 1 };

    let mut chbuf_planar: Vec<Vec<T>> = if channels_first {
        (0..n_out)
            .map(|_| Vec::with_capacity(INITIAL_CAPACITY))
            .collect()
    } else {
        Vec::new()
    };
    let mut chbuf_packed: Vec<T> = if channels_first {
        Vec::new()
    } else {
        Vec::with_capacity(INITIAL_CAPACITY * n_out)
    };

    let mut total_frames: usize = 0;
    // Fallback source-rate frame counter used when frame->pts is absent. After a seek we
    // rely on PTS — codecs that omit PTS cannot support accurate seeking in any mode.
    let mut decoded_src_pos: i64 = 0;
    let mut consumed_in: u64 = 0;

    let target_start = opts.frame_offset as i64;
    let target_end = match opts.num_frames {
        Some(n) => target_start.saturating_add(n as i64),
        None => i64::MAX,
    };

    let mut eof_sent = false;
    let mut eof_seen = false;

    // SAFETY: fmt / codec_ctx / swr / pkt / frame each uphold their type invariants, so their
    // internal pointers are valid and live throughout this block. pkt and frame are unref'd
    // after every send/receive, so no reference to their internal buffers extends beyond a
    // single iteration. chbuf_* is grown to the requested number of output samples before each
    // swr_convert call and then truncated to the actual number written, keeping plane pointers
    // within the allocated capacity at all times.
    unsafe {
        'outer: while !eof_seen {
            if !eof_sent {
                let ret = sys::av_read_frame(fmt.as_ptr(), pkt.0.as_ptr());
                if ret == sys::AVERROR_EOF {
                    sys::avcodec_send_packet(codec_ctx.0.as_ptr(), ptr::null());
                    eof_sent = true;
                } else if ret < 0 {
                    return Err(Error::ReadFrame(FFmpegError(ret)));
                } else if (*pkt.0.as_ptr()).stream_index != audio_idx {
                    sys::av_packet_unref(pkt.0.as_ptr());
                    continue;
                } else {
                    let send = sys::avcodec_send_packet(codec_ctx.0.as_ptr(), pkt.0.as_ptr());
                    sys::av_packet_unref(pkt.0.as_ptr());
                    if send < 0 && send != sys::AVERROR(libc::EAGAIN) {
                        return Err(Error::SendPacket(FFmpegError(send)));
                    }
                }
            }

            loop {
                let recv = sys::avcodec_receive_frame(codec_ctx.0.as_ptr(), frame.0.as_ptr());
                if recv == sys::AVERROR(libc::EAGAIN) {
                    break;
                }
                if recv == sys::AVERROR_EOF {
                    eof_seen = true;
                    break;
                }
                if recv < 0 {
                    return Err(Error::ReceiveFrame(FFmpegError(recv)));
                }

                let frame_samples = (*frame.0.as_ptr()).nb_samples as i64;

                let pts = (*frame.0.as_ptr()).pts;
                let pts_offset = if pts == sys::AV_NOPTS_VALUE {
                    decoded_src_pos
                } else {
                    sys::av_rescale_q(
                        pts,
                        stream_tb.as_av(),
                        sys::AVRational {
                            num: 1,
                            den: in_rate,
                        },
                    )
                };
                decoded_src_pos = pts_offset + frame_samples;

                // Compute the intersection of [pts_offset, pts_offset + frame_samples) and [target_start, target_end).
                let frame_end = pts_offset + frame_samples;
                let in_start = pts_offset.max(target_start);
                let in_end = frame_end.min(target_end);

                if in_start >= in_end {
                    sys::av_frame_unref(frame.0.as_ptr());
                    if pts_offset >= target_end {
                        break 'outer;
                    }
                    continue;
                }

                let skip_in_frame = (in_start - pts_offset) as i32;
                let take_samples = (in_end - in_start) as i32;

                // Build adjusted input plane pointers. extended_data remains valid until
                // av_frame_unref; the offset arithmetic stays within the frame's sample count.
                let stride_bytes = if in_planar {
                    in_bps
                } else {
                    in_bps * in_channels as usize
                };
                let mut in_planes: [*const u8; sys::AV_NUM_DATA_POINTERS as usize] =
                    [ptr::null(); sys::AV_NUM_DATA_POINTERS as usize];
                for (c, plane) in in_planes.iter_mut().take(n_in_planes).enumerate() {
                    let base = *(*frame.0.as_ptr()).extended_data.add(c);
                    *plane = base.add(skip_in_frame as usize * stride_bytes);
                }

                let max_out = sys::swr_get_out_samples(swr.0.as_ptr(), take_samples);
                if max_out < 0 {
                    sys::av_frame_unref(frame.0.as_ptr());
                    return Err(Error::SwrConvert(FFmpegError(max_out)));
                }
                let max_out = max_out as usize;

                grow_storage(
                    channels_first,
                    n_out,
                    total_frames,
                    max_out,
                    &mut chbuf_planar,
                    &mut chbuf_packed,
                );

                let mut out_planes: [*mut u8; sys::AV_NUM_DATA_POINTERS as usize] =
                    [ptr::null_mut(); sys::AV_NUM_DATA_POINTERS as usize];
                fill_out_planes(
                    channels_first,
                    n_out,
                    total_frames,
                    &mut chbuf_planar,
                    &mut chbuf_packed,
                    &mut out_planes,
                );

                let converted = sys::swr_convert(
                    swr.0.as_ptr(),
                    out_planes.as_mut_ptr(),
                    max_out as i32,
                    in_planes.as_mut_ptr(),
                    take_samples,
                );
                sys::av_frame_unref(frame.0.as_ptr());
                if converted < 0 {
                    return Err(Error::SwrConvert(FFmpegError(converted)));
                }
                total_frames += converted as usize;
                consumed_in += take_samples as u64;

                truncate_storage(
                    channels_first,
                    n_out,
                    total_frames,
                    &mut chbuf_planar,
                    &mut chbuf_packed,
                );

                if matches!(opts.num_frames, Some(n) if consumed_in >= n) {
                    break 'outer;
                }
            }
        }

        // Flush swr's internal buffer.
        loop {
            let remaining = sys::swr_get_out_samples(swr.0.as_ptr(), 0);
            if remaining <= 0 {
                break;
            }
            let remaining = remaining as usize;
            grow_storage(
                channels_first,
                n_out,
                total_frames,
                remaining,
                &mut chbuf_planar,
                &mut chbuf_packed,
            );

            let mut out_planes: [*mut u8; sys::AV_NUM_DATA_POINTERS as usize] =
                [ptr::null_mut(); sys::AV_NUM_DATA_POINTERS as usize];
            fill_out_planes(
                channels_first,
                n_out,
                total_frames,
                &mut chbuf_planar,
                &mut chbuf_packed,
                &mut out_planes,
            );

            let converted = sys::swr_convert(
                swr.0.as_ptr(),
                out_planes.as_mut_ptr(),
                remaining as i32,
                ptr::null_mut(),
                0,
            );
            if converted <= 0 {
                truncate_storage(
                    channels_first,
                    n_out,
                    total_frames,
                    &mut chbuf_planar,
                    &mut chbuf_packed,
                );
                break;
            }
            total_frames += converted as usize;
            truncate_storage(
                channels_first,
                n_out,
                total_frames,
                &mut chbuf_planar,
                &mut chbuf_packed,
            );
        }
    }

    let samples = if channels_first {
        T::into_planar(chbuf_planar)
    } else {
        T::into_packed(chbuf_packed)
    };

    // out_channels / out_rate were validated as > 0 at the load() entry point. total_frames
    // is usize which equals u64 on 64-bit platforms; on 32-bit platforms u64 is strictly
    // wider, so the as u64 cast is always lossless.
    Ok(AudioBuffer {
        samples,
        channels: out_channels as u32,
        frames: total_frames as u64,
        sample_rate: out_rate as u32,
        layout: if channels_first {
            ChannelLayout::Planar
        } else {
            ChannelLayout::Interleaved
        },
    })
}

fn grow_storage<T: Default + Clone>(
    channels_first: bool,
    n_out: usize,
    total_frames: usize,
    extra: usize,
    planar: &mut [Vec<T>],
    packed: &mut Vec<T>,
) {
    if channels_first {
        for buf in planar.iter_mut() {
            buf.resize(total_frames + extra, T::default());
        }
    } else {
        packed.resize((total_frames + extra) * n_out, T::default());
    }
}

fn truncate_storage<T>(
    channels_first: bool,
    n_out: usize,
    total_frames: usize,
    planar: &mut [Vec<T>],
    packed: &mut Vec<T>,
) {
    if channels_first {
        for buf in planar.iter_mut() {
            buf.truncate(total_frames);
        }
    } else {
        packed.truncate(total_frames * n_out);
    }
}

// SAFETY: The caller must ensure that every plane / packed buf holds capacity for at least
// `total_frames + extra` samples for the subsequent swr_convert call; this function only
// writes the base+offset pointers it derives from the current buffer addresses.
unsafe fn fill_out_planes<T>(
    channels_first: bool,
    n_out: usize,
    total_frames: usize,
    planar: &mut [Vec<T>],
    packed: &mut [T],
    out_planes: &mut [*mut u8; sys::AV_NUM_DATA_POINTERS as usize],
) {
    if channels_first {
        for c in 0..n_out {
            out_planes[c] = unsafe { planar[c].as_mut_ptr().add(total_frames) } as *mut u8;
        }
    } else {
        out_planes[0] = unsafe { packed.as_mut_ptr().add(total_frames * n_out) } as *mut u8;
    }
}
