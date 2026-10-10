use super::VideoFrame;
use crate::{LoopCount, MediaError, MediaInfo, MemoryBudget, PixelLease, pixels::pixel_bytes};
use gstreamer as gst;
use gstreamer_app::AppSink;
use gstreamer_video as video;
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use video::prelude::*;

pub struct NativeVideoFrame {
    pub fd: std::os::fd::OwnedFd,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub offset: u64,
    pub timestamp: Duration,
    pub duration: Duration,
    bytes: usize,
    // The decoder pool cannot reuse the DMA allocation while the renderer still reads it.
    _sample: gst::Sample,
}

impl NativeVideoFrame {
    pub fn byte_len(&self) -> usize {
        self.bytes
    }
}

fn sample_info(sample: &gst::Sample) -> Result<video::VideoInfo, MediaError> {
    let mut caps = sample.caps().ok_or(MediaError::Unsupported)?.to_owned();
    let structure = caps
        .make_mut()
        .structure_mut(0)
        .ok_or(MediaError::Unsupported)?;
    if structure.get::<&str>("format") == Ok("DMA_DRM") {
        // Only linear ARGB8888 is imported; tiled/compressed modifiers require a different importer.
        if structure.get::<&str>("drm-format") != Ok("AR24") {
            return Err(MediaError::Unsupported);
        }
        structure.set("format", "BGRA");
        structure.remove_field("drm-format");
    }
    video::VideoInfo::from_caps(&caps).map_err(decode_error)
}

pub(crate) struct VideoDecoder {
    pub info: MediaInfo,
    pipeline: gst::Pipeline,
    sink: AppSink,
    first: Option<gst::Sample>,
    first_pts: Option<gst::ClockTime>,
    error: Arc<Mutex<Option<MediaError>>>,
    last_pts: Option<gst::ClockTime>,
    playing: bool,
    _lease: PixelLease,
}

fn decode_error(error: impl std::fmt::Display) -> MediaError {
    MediaError::Decode(error.to_string())
}

impl VideoDecoder {
    pub fn open(
        path: &Path,
        budget: &MemoryBudget,
        cancelled: &std::sync::atomic::AtomicBool,
        software: bool,
        native_device: Option<&crate::NativeVideoDevice>,
    ) -> Result<Self, MediaError> {
        gst::init().map_err(decode_error)?;
        let pipeline = gst::Pipeline::new();
        let source = gst::ElementFactory::make("filesrc")
            .property("location", path.to_str().ok_or(MediaError::Unsupported)?)
            .build()
            .map_err(decode_error)?;
        let demux = gst::ElementFactory::make("qtdemux")
            .build()
            .map_err(decode_error)?;
        let decode = gst::ElementFactory::make("decodebin")
            .property("force-sw-decoders", software)
            .build()
            .map_err(decode_error)?;
        let native = native_device.is_some();
        let convert = if native {
            gst::ElementFactory::make("vapostproc")
                .property_from_str("video-direction", "auto")
                .build()
                .map_err(decode_error)?
        } else {
            gst::ElementFactory::make("videoconvert")
                .build()
                .map_err(decode_error)?
        };
        let flip = if native {
            gst::ElementFactory::make("identity")
                .build()
                .map_err(decode_error)?
        } else {
            gst::ElementFactory::make("videoflip")
                .property_from_str("method", "automatic")
                .build()
                .map_err(decode_error)?
        };
        let caps: gst::Caps = if native {
            "video/x-raw(memory:DMABuf),format=DMA_DRM,drm-format=AR24;video/x-raw(memory:DMABuf),format=BGRA;video/x-raw,format=BGRA"
                .parse().map_err(decode_error)?
        } else {
            gst::Caps::builder("video/x-raw")
                .field("format", "BGRA")
                .build()
        };
        let sink = AppSink::builder()
            .caps(&caps)
            .max_buffers(1)
            .drop(false)
            .sync(false)
            .wait_on_eos(false)
            .build();
        pipeline
            .add_many([&source, &demux, &decode, &convert, &flip, sink.upcast_ref()])
            .map_err(decode_error)?;
        source.link(&demux).map_err(decode_error)?;
        gst::Element::link_many([&convert, &flip, sink.upcast_ref()]).map_err(decode_error)?;
        let error = Arc::new(Mutex::new(None));
        let canvas = Arc::new(Mutex::new(None));
        let canvas_admission = canvas.clone();
        let decode_budget = budget.clone();
        let decode_sink = decode.static_pad("sink").ok_or(MediaError::Unsupported)?;
        let rejected = error.clone();
        demux.connect_pad_added(move |_, pad| {
            let Some(caps) = pad.current_caps() else {
                return;
            };
            let Some(structure) = caps.structure(0) else {
                return;
            };
            if !structure.name().starts_with("video/") {
                return;
            }
            if decode_sink.is_linked() {
                return;
            }
            if structure.name() != "video/x-h264" {
                *rejected.lock().unwrap_or_else(|e| e.into_inner()) = Some(MediaError::Unsupported);
                return;
            }
            let dimensions = structure
                .get::<i32>("width")
                .ok()
                .zip(structure.get::<i32>("height").ok());
            let admission = dimensions
                .filter(|(width, height)| *width > 0 && *height > 0)
                .ok_or(MediaError::Unsupported)
                .and_then(|(width, height)| pixel_bytes(width as u32, height as u32))
                .and_then(|bytes| bytes.checked_mul(4).ok_or(MediaError::ResourceExhausted))
                .and_then(|bytes| decode_budget.reserve(bytes));
            match admission {
                Ok(lease) => {
                    *canvas_admission.lock().unwrap_or_else(|e| e.into_inner()) = Some(lease)
                }
                Err(error) => {
                    *rejected.lock().unwrap_or_else(|e| e.into_inner()) = Some(error);
                    return;
                }
            }
            if pad.link(&decode_sink).is_err() {
                *rejected.lock().unwrap_or_else(|e| e.into_inner()) = Some(MediaError::Unsupported);
            }
        });
        let convert_sink = convert.static_pad("sink").ok_or(MediaError::Unsupported)?;
        let rejected = error.clone();
        decode.connect_pad_added(move |_, pad| {
            let Some(caps) = pad.current_caps() else {
                return;
            };
            let Some(structure) = caps.structure(0) else {
                return;
            };
            if !structure.name().starts_with("video/x-raw") {
                return;
            }
            let valid = video::VideoInfo::from_caps(&caps).is_ok_and(|info| {
                info.format_info().bits() == 8
                    && !matches!(
                        info.colorimetry().transfer(),
                        video::VideoTransferFunction::Smpte2084
                            | video::VideoTransferFunction::AribStdB67
                    )
            });
            if !valid || pad.link(&convert_sink).is_err() {
                *rejected.lock().unwrap_or_else(|e| e.into_inner()) = Some(MediaError::Unsupported);
            }
        });
        struct StopOnError<'a>(&'a gst::Pipeline);
        impl Drop for StopOnError<'_> {
            fn drop(&mut self) {
                let _ = self.0.set_state(gst::State::Null);
            }
        }
        let guard = StopOnError(&pipeline);
        pipeline
            .set_state(gst::State::Paused)
            .map_err(decode_error)?;
        let first =
            pull(&pipeline, &sink, &error, cancelled, true)?.ok_or(MediaError::Unsupported)?;
        let format = sample_info(&first)?;
        let lease = canvas
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .ok_or(MediaError::Unsupported)?;
        let info = MediaInfo {
            width: format.width(),
            height: format.height(),
            animated: true,
            frame_count: None,
            loops: LoopCount::Infinite,
        };
        std::mem::forget(guard);
        Ok(Self {
            info,
            pipeline,
            sink,
            first: Some(first),
            first_pts: None,
            last_pts: None,
            playing: false,
            error,
            _lease: lease,
        })
    }

    fn next_sample(
        &mut self,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Option<gst::Sample>, MediaError> {
        if let Some(error) = self.error.lock().unwrap_or_else(|e| e.into_inner()).take() {
            return Err(error);
        }
        let sample = if let Some(first) = self.first.take() {
            Some(first)
        } else {
            if !self.playing {
                self.pipeline
                    .set_state(gst::State::Playing)
                    .map_err(decode_error)?;
                self.playing = true;
            }
            let sample = loop {
                let result = pull(&self.pipeline, &self.sink, &self.error, cancelled, false);
                match result {
                    Ok(Some(sample))
                        if sample.buffer().and_then(|buffer| buffer.pts()) == self.last_pts =>
                    {
                        continue;
                    }
                    result => break result,
                }
            };
            // The single-buffer appsink applies backpressure when the owner stops requesting frames.
            // Keeping the pipeline running avoids flushing/re-prerolling it for every displayed frame.
            sample?
        };
        let Some(sample) = sample else {
            if let Some(bus) = self.pipeline.bus()
                && let Some(error) = bus.pop_filtered(&[gst::MessageType::Error])
            {
                return Err(MediaError::Decode(format!("{error:?}")));
            }
            return if self.sink.is_eos() {
                Ok(None)
            } else {
                Err(MediaError::Decode("video decoder returned no frame".into()))
            };
        };
        Ok(Some(sample))
    }

    pub fn next_native(
        &mut self,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Option<NativeVideoFrame>, MediaError> {
        use std::os::fd::{AsRawFd, BorrowedFd};
        let Some(sample) = self.next_sample(cancelled)? else {
            return Ok(None);
        };
        let info = sample_info(&sample)?;
        if info.format() != video::VideoFormat::Bgra
            || (info.width(), info.height()) != (self.info.width, self.info.height)
        {
            return Err(MediaError::Unsupported);
        }
        let buffer = sample.buffer().ok_or(MediaError::Unsupported)?;
        if buffer.n_memory() != 1 {
            return Err(MediaError::Unsupported);
        }
        let memory = buffer.peek_memory(0);
        let dma = memory
            .downcast_memory_ref::<gstreamer_allocators::DmaBufMemory>()
            .ok_or(MediaError::Unsupported)?;
        let meta = buffer.meta::<video::VideoMeta>();
        let stride = meta
            .as_ref()
            .map_or(info.stride()[0], |meta| meta.stride()[0]);
        let offset = memory.offset()
            + meta
                .as_ref()
                .map_or(info.offset()[0], |meta| meta.offset()[0]);
        if offset % 4 != 0
            || stride < (info.width() * 4) as i32
            || stride % 4 != 0
            || offset
                .checked_add(stride as usize * (info.height() - 1) as usize)
                .and_then(|end| end.checked_add(info.width() as usize * 4))
                .is_none_or(|end| end > memory.maxsize())
        {
            return Err(MediaError::Unsupported);
        }
        let fd = unsafe { BorrowedFd::borrow_raw(dma.fd()) }.try_clone_to_owned()?;
        // DMA-BUF poll waits on the producer's implicit write fence without mapping pixel memory.
        let mut poll = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                return Err(MediaError::Cancelled);
            }
            let result = unsafe { libc::poll(&mut poll, 1, 100) };
            if result > 0 && poll.revents & libc::POLLIN != 0 {
                break;
            }
            if result < 0
                && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
            {
                continue;
            }
            if result < 0
                || poll.revents & (libc::POLLERR | libc::POLLNVAL) != 0
                || std::time::Instant::now() >= deadline
            {
                return Err(MediaError::Decode(
                    "DMA video producer did not complete".into(),
                ));
            }
        }
        let pts = buffer.pts().ok_or(MediaError::Unsupported)?;
        self.last_pts = Some(pts);
        let first = *self.first_pts.get_or_insert(pts);
        let duration = buffer
            .duration()
            .map(|duration| Duration::from_nanos(duration.nseconds()))
            .or_else(|| {
                let fps = info.fps();
                (fps.numer() > 0 && fps.denom() > 0)
                    .then(|| Duration::from_secs_f64(fps.denom() as f64 / fps.numer() as f64))
            })
            .ok_or(MediaError::Unsupported)?;
        let bytes = memory.maxsize();
        Ok(Some(NativeVideoFrame {
            fd,
            width: info.width(),
            height: info.height(),
            stride: stride as u32,
            offset: offset as u64,
            timestamp: Duration::from_nanos(pts.saturating_sub(first).nseconds()),
            duration,
            bytes,
            _sample: sample,
        }))
    }

    pub fn next(
        &mut self,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Option<VideoFrame>, MediaError> {
        let Some(sample) = self.next_sample(cancelled)? else {
            return Ok(None);
        };
        let info = sample_info(&sample)?;
        if (info.width(), info.height()) != (self.info.width, self.info.height) {
            return Err(MediaError::Unsupported);
        }
        let buffer = sample.buffer().ok_or(MediaError::Unsupported)?;
        let frame =
            video::VideoFrameRef::from_buffer_ref_readable(buffer, &info).map_err(decode_error)?;
        let data = frame.plane_data(0).map_err(decode_error)?;
        let stride = frame.plane_stride()[0];
        if stride <= 0 || (stride as u32) < info.width() * 4 {
            return Err(MediaError::Unsupported);
        }
        let width = info.width() as usize;
        let height = info.height() as usize;
        let mut bgra = vec![0; pixel_bytes(info.width(), info.height())?];
        for y in 0..height {
            let row = data
                .get(y * stride as usize..y * stride as usize + width * 4)
                .ok_or(MediaError::Unsupported)?;
            bgra[y * width * 4..(y + 1) * width * 4].copy_from_slice(row);
        }
        let pts = buffer.pts().ok_or(MediaError::Unsupported)?;
        self.last_pts = Some(pts);
        let first = *self.first_pts.get_or_insert(pts);
        let duration = buffer
            .duration()
            .map(|duration| Duration::from_nanos(duration.nseconds()))
            .or_else(|| {
                let fps = info.fps();
                (fps.numer() > 0 && fps.denom() > 0)
                    .then(|| Duration::from_secs_f64(fps.denom() as f64 / fps.numer() as f64))
            })
            .ok_or(MediaError::Unsupported)?;
        let image = image::RgbaImage::from_raw(info.width(), info.height(), bgra)
            .ok_or(MediaError::Unsupported)?;
        Ok(Some(VideoFrame {
            image: image::DynamicImage::ImageRgba8(image),
            timestamp: Duration::from_nanos(pts.saturating_sub(first).nseconds()),
            duration,
        }))
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

fn pull(
    pipeline: &gst::Pipeline,
    sink: &AppSink,
    error: &Mutex<Option<MediaError>>,
    cancelled: &std::sync::atomic::AtomicBool,
    preroll: bool,
) -> Result<Option<gst::Sample>, MediaError> {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        if cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return Err(MediaError::Cancelled);
        }
        if let Some(error) = error.lock().unwrap_or_else(|e| e.into_inner()).take() {
            return Err(error);
        }
        let timeout = gst::ClockTime::from_mseconds(100);
        let sample = if preroll {
            sink.try_pull_preroll(timeout)
        } else {
            sink.try_pull_sample(timeout)
        };
        if sample.is_some() {
            return Ok(sample);
        }
        if let Some(bus) = pipeline.bus()
            && let Some(message) = bus.pop_filtered(&[gst::MessageType::Error])
        {
            return Err(MediaError::Decode(format!("{message:?}")));
        }
        if sink.is_eos() {
            return Ok(None);
        }
        if std::time::Instant::now() >= deadline {
            return Err(MediaError::Decode("video decoding timed out".into()));
        }
    }
}
