use crate::{
    LoopCount, MediaError, MediaFrame, MediaInfo, MemoryBudget, OutputParams, decoder::Decoder,
    pixels,
};
use async_channel::{Receiver, Sender};
use std::{
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub enum MediaReply {
    Frame { info: MediaInfo, frame: MediaFrame },
    End,
    Error(MediaError),
}

struct Request {
    earliest: Duration,
    output: OutputParams,
}

#[derive(Clone, Default)]
struct Delivery {
    native_device: Option<crate::NativeVideoDevice>,
    software_video: bool,
}

#[derive(Default)]
struct PlaybackControl {
    cancelled: AtomicBool,
    paused: Mutex<bool>,
    wake: Condvar,
    #[cfg(test)]
    pause_observer: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl PlaybackControl {
    fn wait(&self) -> Result<(), MediaError> {
        let mut paused = self
            .paused
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        while *paused && !self.cancelled.load(Ordering::Acquire) {
            #[cfg(test)]
            if let Some(observer) = &*self.pause_observer.lock().unwrap() {
                let _ = observer.send(());
            }
            paused = self
                .wake
                .wait(paused)
                .unwrap_or_else(|error| error.into_inner());
        }
        if self.cancelled.load(Ordering::Acquire) {
            Err(MediaError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Dropping the stream cancels its worker without joining on the caller's thread.
pub struct MediaStream {
    sender: Sender<Request>,
    receiver: Receiver<MediaReply>,
    control: Arc<PlaybackControl>,
    pending: Arc<AtomicBool>,
    budget: MemoryBudget,
    #[cfg(test)]
    worker: Option<std::thread::JoinHandle<()>>,
}

impl MediaStream {
    pub fn spawn(path: PathBuf, generation: u64) -> Result<Self, MediaError> {
        Self::spawn_with_delivery(path, generation, Delivery::default())
    }

    /// Avoid video-device dependencies when the window uses a software renderer.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    pub fn spawn_software(path: PathBuf, generation: u64) -> Result<Self, MediaError> {
        Self::spawn_with_delivery(
            path,
            generation,
            Delivery {
                software_video: true,
                ..Default::default()
            },
        )
    }

    /// Keep native video frames in system storage for GPU composition.
    pub fn spawn_native(
        path: PathBuf,
        generation: u64,
        device: crate::NativeVideoDevice,
    ) -> Result<Self, MediaError> {
        Self::spawn_with_delivery(
            path,
            generation,
            Delivery {
                native_device: Some(device),
                ..Default::default()
            },
        )
    }

    fn spawn_with_delivery(
        path: PathBuf,
        generation: u64,
        delivery: Delivery,
    ) -> Result<Self, MediaError> {
        let (sender, requests) = async_channel::bounded(1);
        let (replies, receiver) = async_channel::bounded(1);
        let control = Arc::new(PlaybackControl::default());
        let worker_control = control.clone();
        let budget = MemoryBudget::default();
        let worker_budget = budget.clone();
        let worker = std::thread::Builder::new()
            .name("background-decode".into())
            .spawn(move || {
                run(
                    path,
                    generation,
                    requests,
                    replies,
                    worker_control,
                    worker_budget,
                    delivery,
                );
            })?;
        #[cfg(not(test))]
        drop(worker);
        Ok(Self {
            sender,
            receiver,
            control,
            pending: Arc::new(AtomicBool::new(false)),
            budget,
            #[cfg(test)]
            worker: Some(worker),
        })
    }

    pub fn request_next(&mut self, earliest: Duration, output: OutputParams) -> bool {
        if self.control.cancelled.load(Ordering::Acquire)
            || self.pending.swap(true, Ordering::AcqRel)
        {
            return false;
        }
        if self.sender.try_send(Request { earliest, output }).is_err() {
            self.pending.store(false, Ordering::Release);
            return false;
        }
        true
    }

    pub fn try_recv(&mut self) -> Option<MediaReply> {
        let reply = self.receiver.try_recv().ok()?;
        self.pending.store(false, Ordering::Release);
        Some(reply)
    }

    pub fn recv(&self) -> impl std::future::Future<Output = Option<MediaReply>> + Send + 'static {
        let receiver = self.receiver.clone();
        let pending = self.pending.clone();
        async move {
            let reply = receiver.recv().await.ok();
            pending.store(false, Ordering::Release);
            reply
        }
    }

    pub fn has_pending_request(&self) -> bool {
        self.pending.load(Ordering::Acquire)
    }
    pub fn memory_budget(&self) -> MemoryBudget {
        self.budget.clone()
    }

    pub fn pause(&self) {
        *self
            .control
            .paused
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = true;
    }

    pub fn resume(&self) {
        *self
            .control
            .paused
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = false;
        self.control.wake.notify_one();
    }

    pub fn close(&mut self) {
        let _paused = self
            .control
            .paused
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        self.control.cancelled.store(true, Ordering::Release);
        self.control.wake.notify_all();
        self.sender.close();
        self.receiver.close();
        while self.receiver.try_recv().is_ok() {}
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

impl Drop for MediaStream {
    fn drop(&mut self) {
        self.close();
    }
}

fn run(
    path: PathBuf,
    generation: u64,
    requests: Receiver<Request>,
    replies: Sender<MediaReply>,
    control: Arc<PlaybackControl>,
    budget: MemoryBudget,
    mut delivery: Delivery,
) {
    // Even metadata and native codec construction stay on the owning worker.
    let mut decoder = None;
    let mut timestamp = Duration::ZERO;
    let mut sequence = 0u64;
    let mut loops = 0u32;
    let mut loop_offset = Duration::ZERO;
    let mut loop_start_sequence = 0;
    let mut processor = pixels::Processor::default();
    while let Ok(request) = requests.recv_blocking() {
        let result = loop {
            let result = (|| {
                control.wait()?;
                if decoder.is_none() {
                    decoder = Some(Decoder::open_with_video_mode(
                        &path,
                        &budget,
                        &control.cancelled,
                        delivery.software_video,
                        delivery.native_device.clone(),
                    )?);
                }
                let decoder = decoder.as_mut().expect("opened decoder");
                let mut processing_time = Duration::ZERO;
                loop {
                    control.wait()?;
                    let started = std::time::Instant::now();
                    let Some((image, duration, pts)) = decoder
                        .next_for_stream(&control.cancelled, delivery.native_device.is_some())?
                    else {
                        if sequence == loop_start_sequence {
                            return Err(MediaError::Decode("media contains no frames".into()));
                        }
                        loops = loops.saturating_add(1);
                        if !decoder.info.animated
                            || matches!(decoder.info.loops, LoopCount::Finite(n) if loops >= n)
                        {
                            return Ok(MediaReply::End);
                        }
                        decoder.restart(&budget, &control.cancelled)?;
                        loop_offset = timestamp;
                        loop_start_sequence = sequence;
                        continue;
                    };
                    let start = pts.map_or(timestamp, |pts| loop_offset.saturating_add(pts));
                    timestamp = start.saturating_add(duration);
                    sequence = sequence
                        .checked_add(1)
                        .ok_or(MediaError::ResourceExhausted)?;
                    // Composition still advances for discarded frames, but processing and upload do not.
                    let final_frame = matches!(decoder.info.loops, LoopCount::Finite(n) if loops + 1 >= n)
                        && decoder.info.frame_count.is_some_and(|count| {
                            sequence - loop_start_sequence == u64::from(count)
                        });
                    if !duration.is_zero() && timestamp <= request.earliest && !final_frame {
                        processing_time += started.elapsed();
                        continue;
                    }
                    let mut native = None;
                    let (pixels, lease, dimensions) = match image {
                        crate::decoder::DecodedImage::Pixels(image) => {
                            processor.process(image, request.output, &budget, decoder.is_bgra())?
                        }
                        crate::decoder::DecodedImage::Native(frame) => {
                            let dimensions = crate::output_dimensions(
                                (decoder.info.width, decoder.info.height),
                                request.output,
                            )?;
                            let lease = budget.reserve(frame.byte_len())?;
                            native = Some(Arc::new(frame));
                            (Vec::new(), lease, dimensions)
                        }
                    };
                    return Ok(MediaReply::Frame {
                        info: decoder.info.clone(),
                        frame: MediaFrame {
                            generation,
                            sequence,
                            timestamp: start,
                            duration,
                            processing_time: processing_time + started.elapsed(),
                            width: dimensions.0,
                            height: dimensions.1,
                            pixels,
                            native,
                            _lease: Some(lease),
                            recycle: Some(Arc::downgrade(&processor.pool)),
                        },
                    });
                }
            })();
            if delivery.native_device.is_some()
                && matches!(result, Err(MediaError::Unsupported | MediaError::Decode(_)))
            {
                log::debug!("Native background delivery unavailable; restarting with CPU delivery");
                delivery.native_device = None;
                delivery.software_video = true;
                decoder = None;
                timestamp = loop_offset;
                loop_start_sequence = sequence;
                continue;
            }
            break result;
        };
        let failed = result.is_err();
        let ended = matches!(result, Ok(MediaReply::End));
        if control.cancelled.load(Ordering::Acquire) {
            break;
        }
        if replies
            .send_blocking(result.unwrap_or_else(MediaReply::Error))
            .is_err()
            || failed
            || ended
        {
            break;
        }
    }
}
