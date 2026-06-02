use std::ffi::{CStr, CString, c_void};
use std::path::Path;
use std::ptr::{self, NonNull};

use crate::avio::WriteAvio;
use crate::error::{Error, FFmpegError};
use crate::sys;
use crate::util::{Frame, Packet};
use crate::{AudioBuffer, ChannelLayout, Codec, SampleData, SaveOptions};

// Maximum number of frames pushed to swr per input chunk. Also used as the
// per-frame size for PCM-like encoders (frame_size==0) and the initial capacity
// of the swr output buffer.
const CHUNK_FRAMES: i32 = 4096;

// INVARIANT: self.0 points to an AVFormatContext successfully allocated by
// avformat_alloc_output_context2. self.1 records whether pb was opened by us via
// avio_open (true, closed on Drop) or is a borrowed custom AVIO (false, detached
// on Drop so the AVIO's owner frees it).
struct OutputCtx(NonNull<sys::AVFormatContext>, bool);

impl OutputCtx {
    // Allocate an output context for file IO; pb is opened separately via avio_open
    // and owned by this context.
    fn alloc(muxer: &CStr, path: &CStr) -> Result<Self, Error> {
        Self::alloc_inner(muxer, path, ptr::null_mut(), true)
    }

    // Allocate an output context wired to a caller-owned custom AVIO. `name` is a
    // synthetic filename used only for the muxer's extension validation (e.g. the
    // ipod muxer checks for .m4a); no filesystem access occurs.
    fn alloc_custom(muxer: &CStr, name: &CStr, pb: *mut sys::AVIOContext) -> Result<Self, Error> {
        Self::alloc_inner(muxer, name, pb, false)
    }

    fn alloc_inner(
        muxer: &CStr,
        path: &CStr,
        pb: *mut sys::AVIOContext,
        owns_pb: bool,
    ) -> Result<Self, Error> {
        // SAFETY: muxer / path are valid NUL-terminated strings;
        // avformat_alloc_output_context2 does not infer the format when a muxer
        // name is provided, but still writes path into ctx->url (some muxers use
        // this to validate the extension, e.g. ipod checks for .m4a/.m4v).
        unsafe {
            let mut ptr: *mut sys::AVFormatContext = ptr::null_mut();
            let ret = sys::avformat_alloc_output_context2(
                &mut ptr,
                ptr::null(),
                muxer.as_ptr(),
                path.as_ptr(),
            );
            if ret < 0 {
                return Err(Error::AllocOutputContext(FFmpegError(ret)));
            }
            let ctx = NonNull::new(ptr).ok_or(Error::AllocOutputContext(FFmpegError(0)))?;
            if !pb.is_null() {
                (*ctx.as_ptr()).pb = pb;
            }
            Ok(OutputCtx(ctx, owns_pb))
        }
    }
}

impl Drop for OutputCtx {
    fn drop(&mut self) {
        // SAFETY: self.0 is valid by the type invariant. When we own pb (file IO) we
        // close it via avio_closep; for a borrowed custom AVIO we only detach it so
        // avformat_free_context cannot touch it and its owner frees it afterwards.
        unsafe {
            let fmt = self.0.as_ptr();
            let pb = (*fmt).pb;
            if self.1 && !pb.is_null() {
                let mut local = pb;
                sys::avio_closep(&mut local);
            }
            (*fmt).pb = ptr::null_mut();
            sys::avformat_free_context(fmt);
        }
    }
}

// INVARIANT: self.0 points to an encoder context that has been allocated with
// avcodec_alloc_context3 and successfully opened with avcodec_open2.
struct EncoderCtx(NonNull<sys::AVCodecContext>);

impl Drop for EncoderCtx {
    fn drop(&mut self) {
        // SAFETY: self.0 is valid by the type invariant; avcodec_free_context
        // receives the pointer via a local variable and nulls it out, but since
        // NonNull cannot hold null we pass a local copy.
        unsafe {
            let mut p = self.0.as_ptr();
            sys::avcodec_free_context(&mut p);
        }
    }
}

// INVARIANT: self.0 points to a SwrContext that has been initialized with swr_init.
struct SwrCtx(NonNull<sys::SwrContext>);

impl SwrCtx {
    fn alloc_init(
        in_layout: *const sys::AVChannelLayout,
        in_fmt: i32,
        in_rate: i32,
        out_layout: *const sys::AVChannelLayout,
        out_fmt: i32,
        out_rate: i32,
    ) -> Result<Self, Error> {
        // SAFETY: the caller is responsible for keeping in_/out_layout valid for
        // the duration of the call. A partially constructed SwrCtx on the error
        // path is freed by Drop.
        unsafe {
            let mut ptr: *mut sys::SwrContext = ptr::null_mut();
            let ret = sys::swr_alloc_set_opts2(
                &mut ptr,
                out_layout,
                out_fmt as _,
                out_rate,
                in_layout,
                in_fmt as _,
                in_rate,
                0,
                ptr::null_mut(),
            );
            if ret < 0 {
                return Err(Error::SwrAlloc(FFmpegError(ret)));
            }
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
        // SAFETY: self.0 is valid by the type invariant.
        unsafe {
            let mut p = self.0.as_ptr();
            sys::swr_free(&mut p);
        }
    }
}

// INVARIANT: self.0 points to an AVAudioFifo returned by av_audio_fifo_alloc.
struct AudioFifo(NonNull<sys::AVAudioFifo>);

impl AudioFifo {
    fn new(sample_fmt: i32, channels: i32, initial: i32) -> Result<Self, Error> {
        // SAFETY: av_audio_fifo_alloc accepts any AVSampleFormat and a positive channel count.
        unsafe {
            let ptr = sys::av_audio_fifo_alloc(sample_fmt as _, channels, initial.max(1));
            NonNull::new(ptr)
                .map(AudioFifo)
                .ok_or(Error::AudioFifoAlloc)
        }
    }

    fn size(&self) -> i32 {
        // SAFETY: self.0 is valid by the type invariant.
        unsafe { sys::av_audio_fifo_size(self.0.as_ptr()) }
    }
}

impl Drop for AudioFifo {
    fn drop(&mut self) {
        // SAFETY: self.0 is valid by the type invariant.
        unsafe { sys::av_audio_fifo_free(self.0.as_ptr()) }
    }
}

// Validate that buf's declared shape matches its sample buffer length.
fn validate_buffer(buf: &AudioBuffer) -> Result<(), Error> {
    if buf.channels == 0 || buf.sample_rate == 0 {
        return Err(Error::InvalidInputBuffer(
            "channels and sample_rate must be > 0",
        ));
    }
    let expected: u64 = (buf.channels as u64)
        .checked_mul(buf.frames)
        .ok_or(Error::InvalidInputBuffer("channels * frames overflows u64"))?;
    if sample_data_len(&buf.samples) as u64 != expected {
        return Err(Error::InvalidInputBuffer(
            "samples length does not match channels * frames",
        ));
    }
    Ok(())
}

// Muxer and encoder names for a codec.
fn codec_io(codec: Codec) -> (&'static CStr, &'static CStr) {
    match codec {
        Codec::Wav { sample_format } => (c"wav", sample_format.encoder_name()),
        Codec::Aiff { sample_format } => (c"aiff", sample_format.encoder_name()),
        Codec::Flac { .. } => (c"flac", c"flac"),
        Codec::Alac { .. } => (c"ipod", c"alac"),
        Codec::Aac { .. } => (c"ipod", c"aac"),
        Codec::Mp3 { .. } => (c"mp3", c"libmp3lame"),
        Codec::Opus { .. } => (c"ogg", c"libopus"),
        Codec::Vorbis { .. } => (c"ogg", c"libvorbis"),
    }
}

// A synthetic filename carrying the container extension, used so muxers that
// validate the extension (e.g. ipod) accept a custom-IO context that has no path.
fn synthetic_name(codec: Codec) -> &'static CStr {
    match codec {
        Codec::Wav { .. } => c"out.wav",
        Codec::Aiff { .. } => c"out.aiff",
        Codec::Flac { .. } => c"out.flac",
        Codec::Alac { .. } | Codec::Aac { .. } => c"out.m4a",
        Codec::Mp3 { .. } => c"out.mp3",
        Codec::Opus { .. } | Codec::Vorbis { .. } => c"out.ogg",
    }
}

pub(crate) fn save(path: &Path, buf: &AudioBuffer, opts: &SaveOptions) -> Result<(), Error> {
    validate_buffer(buf)?;
    let (muxer, _) = codec_io(opts.codec);
    let cpath = CString::new(path.as_os_str().to_string_lossy().as_bytes())?;
    let out_ctx = OutputCtx::alloc(muxer, &cpath)?;

    // SAFETY: cpath is a valid NUL-terminated path; out_ctx.pb is NULL after alloc,
    // and avio_open writes the opened handle into it.
    unsafe {
        let pb_ptr = &mut (*out_ctx.0.as_ptr()).pb;
        let ret = sys::avio_open(pb_ptr, cpath.as_ptr(), sys::AVIO_FLAG_WRITE as i32);
        if ret < 0 {
            return Err(Error::AvioOpen(FFmpegError(ret)));
        }
    }

    run_encode(&out_ctx, buf, opts)
}

pub(crate) fn save_bytes(buf: &AudioBuffer, opts: &SaveOptions) -> Result<Vec<u8>, Error> {
    validate_buffer(buf)?;
    let (muxer, _) = codec_io(opts.codec);
    let avio = WriteAvio::new()?;
    let out_ctx = OutputCtx::alloc_custom(muxer, synthetic_name(opts.codec), avio.as_ptr())?;

    run_encode(&out_ctx, buf, opts)?;

    // SAFETY: out_ctx upholds its type invariant; flush the muxer's buffered output
    // into the sink, then drop the format context (which detaches the borrowed pb)
    // before reclaiming the accumulated bytes from the AVIO.
    unsafe { sys::avio_flush((*out_ctx.0.as_ptr()).pb) };
    drop(out_ctx);
    Ok(avio.into_bytes())
}

fn run_encode(out_ctx: &OutputCtx, buf: &AudioBuffer, opts: &SaveOptions) -> Result<(), Error> {
    let (_, encoder_name) = codec_io(opts.codec);

    // Look up the encoder.
    // SAFETY: encoder_name is a valid NUL-terminated string.
    let encoder = unsafe { sys::avcodec_find_encoder_by_name(encoder_name.as_ptr()) };
    if encoder.is_null() {
        return Err(Error::EncoderNotFound(
            encoder_name.to_str().unwrap_or("<encoder>"),
        ));
    }

    let out_channels = if opts.mono { 1 } else { buf.channels as i32 };
    let req_rate = opts.sample_rate.unwrap_or(buf.sample_rate) as i32;
    let out_rate = pick_supported_sample_rate(encoder, req_rate)?;
    // Preferred bit depth / sample format explicitly specified by the caller per
    // codec; non-PCM-like codecs (aac/mp3/opus/vorbis) have no adjustable bit
    // depth, so None is passed to let pick_encoder_sample_fmt fall back to
    // encoder.sample_fmts[0].
    let preferred_fmt: Option<i32> = match opts.codec {
        Codec::Wav { sample_format } => Some(sample_format.sample_fmt()),
        Codec::Aiff { sample_format } => Some(sample_format.sample_fmt()),
        Codec::Flac {
            bits_per_sample, ..
        } => Some(bits_per_sample.sample_fmt()),
        Codec::Alac { bits_per_sample } => Some(bits_per_sample.sample_fmt()),
        Codec::Aac { .. } | Codec::Mp3 { .. } | Codec::Opus { .. } | Codec::Vorbis { .. } => None,
    };
    // SAFETY: encoder is the non-null return value of avcodec_find_encoder_by_name.
    let out_fmt = unsafe { pick_encoder_sample_fmt(encoder, preferred_fmt) };

    let in_packed = sample_data_packed_fmt(&buf.samples);
    // SAFETY: in_packed comes from sample_data_packed_fmt and is always one of
    // the six valid packed sample formats.
    let in_fmt = if buf.layout == ChannelLayout::Planar {
        unsafe { sys::av_get_planar_sample_fmt(in_packed as _) as i32 }
    } else {
        in_packed
    };
    let in_bps = sample_data_bytes_per_sample(&buf.samples);

    // ---- stream + encoder ----
    // SAFETY: out_ctx upholds its type invariant; the second argument of
    // avformat_new_stream (deprecated codec handle) is passed as NULL.
    let stream = unsafe { sys::avformat_new_stream(out_ctx.0.as_ptr(), ptr::null()) };
    if stream.is_null() {
        return Err(Error::NewStream);
    }

    // SAFETY: encoder is non-null; all codec_ctx field writes are plain POD
    // assignments; ch_layout is zeroed after alloc so av_channel_layout_default
    // will not leak a prior allocation. On avcodec_open2 failure the EncoderCtx
    // frees the context via Drop.
    let codec_ctx = unsafe {
        let p =
            NonNull::new(sys::avcodec_alloc_context3(encoder)).ok_or(Error::AllocCodecContext)?;
        let ctx = EncoderCtx(p);
        let cc = ctx.0.as_ptr();

        (*cc).sample_fmt = out_fmt as _;
        (*cc).sample_rate = out_rate;
        (*cc).time_base.num = 1;
        (*cc).time_base.den = out_rate;
        sys::av_channel_layout_default(&mut (*cc).ch_layout, out_channels);

        // FLAC / ALAC: sample_fmt only specifies the container width (s16/s32);
        // the effective bit depth is bounded by bits_per_raw_sample.
        // Both flacenc and alacenc read this field.
        match opts.codec {
            Codec::Flac {
                bits_per_sample, ..
            } => {
                (*cc).bits_per_raw_sample = bits_per_sample.bits_per_raw_sample();
            }
            Codec::Alac { bits_per_sample } => {
                (*cc).bits_per_raw_sample = bits_per_sample.bits_per_raw_sample();
            }
            _ => {}
        }

        // Codec-specific parameters: bit_rate / compression_level are written
        // directly to AVCodecContext fields; application / frame_duration / vbr /
        // vorbis quality go through the AVOption path (av_opt_set_* on priv_data).
        match opts.codec {
            Codec::Aac { bit_rate } => {
                if let Some(br) = bit_rate {
                    (*cc).bit_rate = br as i64;
                }
            }
            Codec::Mp3 {
                bit_rate,
                compression_level,
            } => {
                if let Some(br) = bit_rate {
                    (*cc).bit_rate = br as i64;
                }
                if let Some(level) = compression_level {
                    (*cc).compression_level = level as i32;
                }
            }
            Codec::Flac {
                compression_level, ..
            } => {
                if let Some(level) = compression_level {
                    (*cc).compression_level = level as i32;
                }
            }
            Codec::Opus {
                bit_rate,
                application,
                frame_duration,
                vbr,
            } => {
                if let Some(br) = bit_rate {
                    (*cc).bit_rate = br as i64;
                }
                let priv_data = (*cc).priv_data;
                if let Some(app) = application {
                    set_opt_str(priv_data, c"application", app.av_opt_value())?;
                }
                if let Some(fd) = frame_duration {
                    set_opt_double(priv_data, c"frame_duration", fd.millis())?;
                }
                if let Some(v) = vbr {
                    set_opt_str(priv_data, c"vbr", v.av_opt_value())?;
                }
            }
            Codec::Vorbis { quality, bit_rate } => {
                // libvorbis: quality takes priority (VBR), expressed via
                // global_quality + AV_CODEC_FLAG_QSCALE; otherwise bit_rate
                // selects ABR; if neither is provided libvorbis uses its default
                // VBR quality.
                if let Some(q) = quality {
                    (*cc).flags |= sys::AV_CODEC_FLAG_QSCALE as i32;
                    (*cc).global_quality = (sys::FF_QP2LAMBDA as f32 * q) as i32;
                } else if let Some(br) = bit_rate {
                    (*cc).bit_rate = br as i64;
                }
            }
            // PCM-like codecs (Wav / Aiff / Alac) and Flac bits_per_sample were
            // handled in the previous match; bit_rate and similar fields are
            // not meaningful here.
            Codec::Wav { .. } | Codec::Aiff { .. } | Codec::Alac { .. } => {}
        }

        if (*(*out_ctx.0.as_ptr()).oformat).flags & sys::AVFMT_GLOBALHEADER as i32 != 0 {
            (*cc).flags |= sys::AV_CODEC_FLAG_GLOBAL_HEADER as i32;
        }

        let ret = sys::avcodec_open2(cc, encoder, ptr::null_mut());
        if ret < 0 {
            return Err(Error::OpenCodec(FFmpegError(ret)));
        }
        ctx
    };

    // SAFETY: stream is non-null; codec_ctx upholds its type invariant;
    // codecpar was allocated by avformat_new_stream.
    unsafe {
        let ret = sys::avcodec_parameters_from_context((*stream).codecpar, codec_ctx.0.as_ptr());
        if ret < 0 {
            return Err(Error::CodecParametersFrom(FFmpegError(ret)));
        }
        (*stream).time_base = clone_rational(&(*codec_ctx.0.as_ptr()).time_base);
    }

    // SAFETY: out_ctx.pb is ready (opened by save / set to a custom AVIO by save_bytes).
    unsafe {
        let ret = sys::avformat_write_header(out_ctx.0.as_ptr(), ptr::null_mut());
        if ret < 0 {
            return Err(Error::WriteHeader(FFmpegError(ret)));
        }
    }

    // ---- swr ----
    // SAFETY: in/out_layout are zero-initialized on the stack; av_channel_layout_default
    // fills in the mask field; SwrCtx::alloc_init only reads these layouts for
    // the duration of the call, after which we immediately uninit them.
    let swr = unsafe {
        let mut in_layout = std::mem::zeroed::<sys::AVChannelLayout>();
        sys::av_channel_layout_default(&mut in_layout, buf.channels as i32);
        let mut out_layout = std::mem::zeroed::<sys::AVChannelLayout>();
        sys::av_channel_layout_default(&mut out_layout, out_channels);
        let r = SwrCtx::alloc_init(
            &in_layout,
            in_fmt,
            buf.sample_rate as i32,
            &out_layout,
            out_fmt,
            out_rate,
        );
        sys::av_channel_layout_uninit(&mut in_layout);
        sys::av_channel_layout_uninit(&mut out_layout);
        r?
    };

    // ---- fifo ----
    let fifo = AudioFifo::new(out_fmt, out_channels, CHUNK_FRAMES * 2)?;

    // SAFETY: codec_ctx has been opened.
    let frame_size_codec = unsafe { (*codec_ctx.0.as_ptr()).frame_size };
    let frame_size = if frame_size_codec > 0 {
        frame_size_codec
    } else {
        CHUNK_FRAMES
    };

    encode_loop(
        out_ctx,
        &codec_ctx,
        &swr,
        &fifo,
        stream,
        buf,
        in_fmt,
        in_bps,
        out_channels,
        out_fmt,
        frame_size,
    )?;

    // SAFETY: out_ctx upholds its type invariant; the trailer is written once
    // after all packets have been flushed.
    unsafe {
        let ret = sys::av_write_trailer(out_ctx.0.as_ptr());
        if ret < 0 {
            return Err(Error::WriteTrailer(FFmpegError(ret)));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn encode_loop(
    out_ctx: &OutputCtx,
    codec_ctx: &EncoderCtx,
    swr: &SwrCtx,
    fifo: &AudioFifo,
    stream: *mut sys::AVStream,
    buf: &AudioBuffer,
    in_fmt: i32,
    in_bps: usize,
    out_channels: i32,
    out_fmt: i32,
    frame_size: i32,
) -> Result<(), Error> {
    let pkt = Packet::new()?;
    let frame = Frame::new()?;

    let total_frames = buf.frames as i64;
    let in_channels = buf.channels as i32;
    // SAFETY: in_fmt / out_fmt are valid values obtained from av_get_planar_sample_fmt
    // and encoder->sample_fmts[0] respectively.
    let in_planar = unsafe { sys::av_sample_fmt_is_planar(in_fmt as _) != 0 };
    let out_planar = unsafe { sys::av_sample_fmt_is_planar(out_fmt as _) != 0 };
    let out_bps = unsafe { sys::av_get_bytes_per_sample(out_fmt as _) } as usize;

    let n_in_planes = if in_planar { in_channels as usize } else { 1 };
    let in_stride_bytes = if in_planar {
        in_bps
    } else {
        in_bps * in_channels as usize
    };
    let n_out_planes = if out_planar { out_channels as usize } else { 1 };

    // Per-plane byte budget: CHUNK_FRAMES plus some headroom for swr's internal buffer.
    let initial_plane_bytes = if out_planar {
        (CHUNK_FRAMES as usize + 256) * out_bps
    } else {
        (CHUNK_FRAMES as usize + 256) * out_channels as usize * out_bps
    };
    let mut swr_out_buf: Vec<Vec<u8>> = (0..n_out_planes)
        .map(|_| vec![0u8; initial_plane_bytes])
        .collect();

    let in_base = sample_data_ptr(&buf.samples);
    // Base address plus start offset (along the channel dimension) for the c-th
    // plane in the input chunk. Only meaningful for planar input; packed input
    // keeps all data in a single contiguous buffer.
    let plane_offset_bytes = |c: usize| -> usize {
        if in_planar {
            c * (total_frames as usize) * in_bps
        } else {
            0
        }
    };

    let mut chunk_start: i64 = 0;
    let mut pts_counter: i64 = 0;

    while chunk_start < total_frames {
        let chunk = ((total_frames - chunk_start) as i32).min(CHUNK_FRAMES);

        let mut in_planes: [*const u8; sys::AV_NUM_DATA_POINTERS as usize] =
            [ptr::null(); sys::AV_NUM_DATA_POINTERS as usize];
        // SAFETY: the offset is within the valid range of the buf.samples vec
        // (total samples = channels * frames).
        unsafe {
            for (c, plane) in in_planes.iter_mut().take(n_in_planes).enumerate() {
                *plane = in_base
                    .add(plane_offset_bytes(c))
                    .add(chunk_start as usize * in_stride_bytes);
            }
        }

        // Push this input chunk into swr and write any available output into the fifo.
        push_through_swr(
            swr,
            fifo,
            &mut swr_out_buf,
            in_planes.as_ptr(),
            chunk,
            n_out_planes,
            out_planar,
            out_channels,
            out_bps,
        )?;

        // Encode one frame each time enough samples have accumulated.
        while fifo.size() >= frame_size {
            encode_one_frame(
                out_ctx,
                codec_ctx,
                fifo,
                stream,
                &pkt,
                &frame,
                frame_size,
                out_fmt,
                out_channels,
                &mut pts_counter,
            )?;
        }

        chunk_start += chunk as i64;
    }

    // Drain any remaining samples held internally by swr.
    push_through_swr(
        swr,
        fifo,
        &mut swr_out_buf,
        ptr::null(),
        0,
        n_out_planes,
        out_planar,
        out_channels,
        out_bps,
    )?;

    // Drain fifo remainder: the final frame may be smaller than frame_size.
    while fifo.size() > 0 {
        let n = fifo.size().min(frame_size);
        encode_one_frame(
            out_ctx,
            codec_ctx,
            fifo,
            stream,
            &pkt,
            &frame,
            n,
            out_fmt,
            out_channels,
            &mut pts_counter,
        )?;
    }

    // Flush the encoder by sending a NULL frame and draining the remaining output.
    // SAFETY: codec_ctx / pkt / stream / out_ctx each uphold their type invariants.
    unsafe { send_frame_and_drain(out_ctx, codec_ctx, stream, &pkt, ptr::null_mut())? };
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn push_through_swr(
    swr: &SwrCtx,
    fifo: &AudioFifo,
    swr_out_buf: &mut [Vec<u8>],
    in_planes: *const *const u8,
    in_samples: i32,
    n_out_planes: usize,
    out_planar: bool,
    out_channels: i32,
    out_bps: usize,
) -> Result<(), Error> {
    loop {
        // SAFETY: swr upholds its type invariant.
        let max_out = unsafe { sys::swr_get_out_samples(swr.0.as_ptr(), in_samples) };
        if max_out < 0 {
            return Err(Error::SwrConvert(FFmpegError(max_out)));
        }

        // In push mode (in_samples > 0) at least one push is required; in pure
        // drain mode exit immediately when nothing more can be pulled.
        let pure_drain = in_samples == 0;
        if pure_drain && max_out == 0 {
            return Ok(());
        }
        if max_out == 0 {
            // Input is available but swr cannot produce output yet (rare; deferred
            // to the next push). Feed the input through (converted=0) and return.
        }

        // Grow per-plane byte budget if necessary.
        let needed = if out_planar {
            (max_out.max(1) as usize) * out_bps
        } else {
            (max_out.max(1) as usize) * out_channels as usize * out_bps
        };
        for b in swr_out_buf.iter_mut() {
            if b.len() < needed {
                b.resize(needed, 0);
            }
        }

        let mut out_planes: [*mut u8; sys::AV_NUM_DATA_POINTERS as usize] =
            [ptr::null_mut(); sys::AV_NUM_DATA_POINTERS as usize];
        for (c, p) in out_planes.iter_mut().take(n_out_planes).enumerate() {
            *p = swr_out_buf[c].as_mut_ptr();
        }

        // SAFETY: in_planes is guaranteed valid by the caller in push mode and is
        // null in pure drain mode (accepted by swr_convert). out_planes points to
        // the buffer we just ensured is large enough.
        let converted = unsafe {
            sys::swr_convert(
                swr.0.as_ptr(),
                out_planes.as_mut_ptr(),
                max_out.max(1),
                in_planes as *mut *const u8,
                in_samples,
            )
        };
        if converted < 0 {
            return Err(Error::SwrConvert(FFmpegError(converted)));
        }

        if converted > 0 {
            let mut fifo_in: [*mut c_void; sys::AV_NUM_DATA_POINTERS as usize] =
                [ptr::null_mut(); sys::AV_NUM_DATA_POINTERS as usize];
            for (c, slot) in fifo_in.iter_mut().take(n_out_planes).enumerate() {
                *slot = out_planes[c] as *mut c_void;
            }
            // SAFETY: fifo upholds its type invariant; fifo_in points to the
            // valid buffer just written by swr.
            let written =
                unsafe { sys::av_audio_fifo_write(fifo.0.as_ptr(), fifo_in.as_ptr(), converted) };
            if written < 0 {
                return Err(Error::AudioFifoWrite(FFmpegError(written)));
            }
            if written != converted {
                return Err(Error::AudioFifoWrite(FFmpegError(written)));
            }
        }

        // In push mode swr_convert has consumed all input; exit this iteration.
        // In drain mode keep looping as long as swr still produces output.
        if !pure_drain {
            return Ok(());
        }
        if converted <= 0 {
            return Ok(());
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn encode_one_frame(
    out_ctx: &OutputCtx,
    codec_ctx: &EncoderCtx,
    fifo: &AudioFifo,
    stream: *mut sys::AVStream,
    pkt: &Packet,
    frame: &Frame,
    nb_samples: i32,
    out_fmt: i32,
    out_channels: i32,
    pts_counter: &mut i64,
) -> Result<(), Error> {
    // SAFETY: frame / codec_ctx / fifo each uphold their type invariants. After
    // av_frame_unref we re-populate the required fields and allocate a new
    // buffer with av_frame_get_buffer; nb_samples <= fifo.size() is guaranteed
    // by the caller.
    unsafe {
        sys::av_frame_unref(frame.0.as_ptr());
        let f = frame.0.as_ptr();
        (*f).format = out_fmt;
        (*f).nb_samples = nb_samples;
        (*f).sample_rate = (*codec_ctx.0.as_ptr()).sample_rate;
        sys::av_channel_layout_uninit(&mut (*f).ch_layout);
        sys::av_channel_layout_default(&mut (*f).ch_layout, out_channels);

        let ret = sys::av_frame_get_buffer(f, 0);
        if ret < 0 {
            return Err(Error::FrameGetBuffer(FFmpegError(ret)));
        }

        let n_planes = if sys::av_sample_fmt_is_planar(out_fmt as _) != 0 {
            out_channels as usize
        } else {
            1
        };
        let mut targets: [*mut c_void; sys::AV_NUM_DATA_POINTERS as usize] =
            [ptr::null_mut(); sys::AV_NUM_DATA_POINTERS as usize];
        for (c, slot) in targets.iter_mut().take(n_planes).enumerate() {
            *slot = *(*f).extended_data.add(c) as *mut c_void;
        }
        let read = sys::av_audio_fifo_read(fifo.0.as_ptr(), targets.as_ptr(), nb_samples);
        if read < 0 {
            return Err(Error::AudioFifoRead(FFmpegError(read)));
        }

        (*f).pts = *pts_counter;
        *pts_counter += nb_samples as i64;

        send_frame_and_drain(out_ctx, codec_ctx, stream, pkt, f)?;
        sys::av_frame_unref(f);
    }
    Ok(())
}

unsafe fn send_frame_and_drain(
    out_ctx: &OutputCtx,
    codec_ctx: &EncoderCtx,
    stream: *mut sys::AVStream,
    pkt: &Packet,
    frame: *mut sys::AVFrame,
) -> Result<(), Error> {
    // SAFETY: the caller guarantees that codec_ctx / pkt / stream / out_ctx each
    // uphold their type invariants. frame may be NULL (flush mode).
    unsafe {
        let send = sys::avcodec_send_frame(codec_ctx.0.as_ptr(), frame);
        if send < 0 && send != sys::AVERROR(sys::EAGAIN) {
            return Err(Error::SendFrame(FFmpegError(send)));
        }
        loop {
            let r = sys::avcodec_receive_packet(codec_ctx.0.as_ptr(), pkt.0.as_ptr());
            if r == sys::AVERROR(sys::EAGAIN) || r == sys::AVERROR_EOF {
                break;
            }
            if r < 0 {
                return Err(Error::ReceivePacket(FFmpegError(r)));
            }
            sys::av_packet_rescale_ts(
                pkt.0.as_ptr(),
                clone_rational(&(*codec_ctx.0.as_ptr()).time_base),
                clone_rational(&(*stream).time_base),
            );
            (*pkt.0.as_ptr()).stream_index = (*stream).index;
            let w = sys::av_interleaved_write_frame(out_ctx.0.as_ptr(), pkt.0.as_ptr());
            sys::av_packet_unref(pkt.0.as_ptr());
            if w < 0 {
                return Err(Error::WriteFrame(FFmpegError(w)));
            }
        }
    }
    Ok(())
}

// bindgen has derive_copy disabled in this project, so AVRational cannot be
// assigned by value directly; this helper collapses the field-copy boilerplate
// into a single line.
fn clone_rational(r: &sys::AVRational) -> sys::AVRational {
    sys::AVRational {
        num: r.num,
        den: r.den,
    }
}

fn sample_data_len(s: &SampleData) -> usize {
    match s {
        SampleData::F64(v) => v.len(),
        SampleData::F32(v) => v.len(),
        SampleData::I64(v) => v.len(),
        SampleData::I32(v) => v.len(),
        SampleData::I16(v) => v.len(),
        SampleData::U8(v) => v.len(),
    }
}

fn sample_data_packed_fmt(s: &SampleData) -> i32 {
    match s {
        SampleData::F64(_) => sys::AV_SAMPLE_FMT_DBL,
        SampleData::F32(_) => sys::AV_SAMPLE_FMT_FLT,
        SampleData::I64(_) => sys::AV_SAMPLE_FMT_S64,
        SampleData::I32(_) => sys::AV_SAMPLE_FMT_S32,
        SampleData::I16(_) => sys::AV_SAMPLE_FMT_S16,
        SampleData::U8(_) => sys::AV_SAMPLE_FMT_U8,
    }
}

fn sample_data_bytes_per_sample(s: &SampleData) -> usize {
    match s {
        SampleData::F64(_) | SampleData::I64(_) => 8,
        SampleData::F32(_) | SampleData::I32(_) => 4,
        SampleData::I16(_) => 2,
        SampleData::U8(_) => 1,
    }
}

fn sample_data_ptr(s: &SampleData) -> *const u8 {
    match s {
        SampleData::F64(v) => v.as_ptr() as *const u8,
        SampleData::F32(v) => v.as_ptr() as *const u8,
        SampleData::I64(v) => v.as_ptr() as *const u8,
        SampleData::I32(v) => v.as_ptr() as *const u8,
        SampleData::I16(v) => v.as_ptr() as *const u8,
        SampleData::U8(v) => v.as_ptr(),
    }
}

unsafe fn pick_encoder_sample_fmt(encoder: *const sys::AVCodec, preferred: Option<i32>) -> i32 {
    // SAFETY: the caller guarantees encoder is non-null. sample_fmts is a
    // constant array defined by the FFmpeg encoder itself, terminated by
    // AV_SAMPLE_FMT_NONE = -1; the first entry is the encoder's most preferred
    // format.
    unsafe {
        let p = (*encoder).sample_fmts;
        if p.is_null() {
            // A NULL sample_fmts means the encoder accepts any format; use
            // preferred if given, otherwise fall back to FLT (consistent with
            // the pre-refactor behavior).
            return preferred.unwrap_or(sys::AV_SAMPLE_FMT_FLT);
        }
        if let Some(want) = preferred {
            let mut q = p;
            while *q != sys::AV_SAMPLE_FMT_NONE {
                if *q == want {
                    return want;
                }
                q = q.add(1);
            }
        }
        // preferred did not match or was not given; use the first entry (the
        // encoder's most preferred format).
        *p
    }
}

unsafe fn set_opt_str(priv_data: *mut c_void, key: &'static CStr, val: &CStr) -> Result<(), Error> {
    // SAFETY: the caller guarantees priv_data comes from (*AVCodecContext).priv_data,
    // which may be NULL (when the codec has no private options). av_opt_set
    // returns an error when priv_data is null; we wrap that as AvOptSetFailed.
    unsafe {
        let r = sys::av_opt_set(
            priv_data,
            key.as_ptr(),
            val.as_ptr(),
            sys::AV_OPT_SEARCH_CHILDREN as i32,
        );
        if r < 0 {
            return Err(Error::AvOptSetFailed {
                key: key.to_str().unwrap_or("<key>"),
                code: FFmpegError(r),
            });
        }
        Ok(())
    }
}

unsafe fn set_opt_double(
    priv_data: *mut c_void,
    key: &'static CStr,
    val: f64,
) -> Result<(), Error> {
    // SAFETY: same as set_opt_str.
    unsafe {
        let r = sys::av_opt_set_double(
            priv_data,
            key.as_ptr(),
            val,
            sys::AV_OPT_SEARCH_CHILDREN as i32,
        );
        if r < 0 {
            return Err(Error::AvOptSetFailed {
                key: key.to_str().unwrap_or("<key>"),
                code: FFmpegError(r),
            });
        }
        Ok(())
    }
}

fn pick_supported_sample_rate(encoder: *const sys::AVCodec, requested: i32) -> Result<i32, Error> {
    if requested <= 0 {
        return Err(Error::InvalidArg("sample_rate must be > 0"));
    }
    // SAFETY: the caller guarantees encoder is non-null. supported_samplerates
    // may be NULL (any sample rate is accepted) or a zero-terminated i32 array.
    unsafe {
        let mut p = (*encoder).supported_samplerates;
        if p.is_null() {
            return Ok(requested);
        }
        let mut supported = Vec::new();
        while *p != 0 {
            if *p == requested {
                return Ok(requested);
            }
            supported.push(*p);
            p = p.add(1);
        }
        Err(Error::UnsupportedSampleRate {
            sample_rate: requested,
            supported,
        })
    }
}
