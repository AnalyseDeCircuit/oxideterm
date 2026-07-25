// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    sync::{
        Arc,
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
};

use cpal::{
    FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _},
};

use crate::queue::BoundedPcmQueue;

const DEFAULT_BUFFER_DURATION_MILLISECONDS: usize = 500;
const AUDIO_PLAYBACK_THREAD_NAME: &str = "oxideterm-pcm-audio";

/// Plays bounded little-endian signed 16-bit PCM without blocking producers.
#[derive(Debug)]
pub struct PcmS16LePlayback {
    sample_rate: u32,
    channels: u16,
    queue: Arc<BoundedPcmQueue>,
    worker: Option<AudioPlaybackWorker>,
}

impl PcmS16LePlayback {
    /// Creates a lazy playback session with a half-second latency ceiling.
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        let capacity_samples =
            sample_rate as usize * usize::from(channels) * DEFAULT_BUFFER_DURATION_MILLISECONDS
                / 1_000;
        Self {
            sample_rate,
            channels,
            queue: Arc::new(BoundedPcmQueue::new(
                capacity_samples,
                usize::from(channels),
            )),
            worker: None,
        }
    }

    /// Opens the output device once for the active protocol stream.
    pub fn start(&mut self) -> Result<(), String> {
        if self.worker.is_some() {
            return Ok(());
        }
        let worker =
            AudioPlaybackWorker::spawn(Arc::clone(&self.queue), self.sample_rate, self.channels)
                .map_err(|error| format!("spawn playback worker: {error}"))?;
        self.worker = Some(worker);
        Ok(())
    }

    /// Queues complete interleaved frames and discards stale frames on overload.
    pub fn push(&self, bytes: &[u8]) {
        self.queue.push_pcm(bytes);
    }

    /// Stops and joins the device thread before clearing queued samples.
    pub fn stop(&mut self) {
        if let Some(mut worker) = self.worker.take() {
            worker.shutdown();
        }
        self.queue.clear();
    }
}

impl Drop for PcmS16LePlayback {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Owns the non-Send CPAL stream on a dedicated, joinable thread.
#[derive(Debug)]
struct AudioPlaybackWorker {
    shutdown_tx: Option<SyncSender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl AudioPlaybackWorker {
    /// Spawns a session-owned worker with a bounded cancellation channel.
    fn spawn(
        queue: Arc<BoundedPcmQueue>,
        sample_rate: u32,
        channels: u16,
    ) -> std::io::Result<Self> {
        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name(AUDIO_PLAYBACK_THREAD_NAME.to_string())
            .spawn(move || {
                if let Err(error) = run_pcm_output(queue, shutdown_rx, sample_rate, channels) {
                    eprintln!("[oxideterm:pcm-audio] playback unavailable: {error}");
                }
            })?;
        Ok(Self {
            shutdown_tx: Some(shutdown_tx),
            handle: Some(handle),
        })
    }

    /// Cancels the device loop and joins its owner thread deterministically.
    fn shutdown(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.try_send(());
        }
        if let Some(handle) = self.handle.take()
            && let Err(error) = handle.join()
        {
            eprintln!("[oxideterm:pcm-audio] playback worker panicked: {error:?}");
        }
    }
}

impl Drop for AudioPlaybackWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Keeps the selected CPAL stream alive until its owning session cancels it.
fn run_pcm_output(
    queue: Arc<BoundedPcmQueue>,
    shutdown_rx: mpsc::Receiver<()>,
    sample_rate: u32,
    channels: u16,
) -> Result<(), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "no default output device".to_string())?;
    let (config, sample_format) = compatible_output_config(&device, sample_rate, channels)?;
    let stream = build_output_stream(&device, &config, sample_format, queue)?;
    stream
        .play()
        .map_err(|error| format!("start output stream: {error}"))?;

    // Closing the bounded sender also wakes this receive during owner teardown.
    let _ = shutdown_rx.recv();
    Ok(())
}

/// Selects a device format that accepts the protocol sample rate directly.
fn compatible_output_config(
    device: &cpal::Device,
    sample_rate: u32,
    channels: u16,
) -> Result<(StreamConfig, SampleFormat), String> {
    let requested_sample_rate = cpal::SampleRate(sample_rate);
    let supported = device
        .supported_output_configs()
        .map_err(|error| format!("query output formats: {error}"))?
        .filter(|format| {
            format.channels() == channels
                && format.min_sample_rate() <= requested_sample_rate
                && requested_sample_rate <= format.max_sample_rate()
        })
        .min_by_key(|format| sample_format_priority(format.sample_format()))
        .ok_or_else(|| {
            format!(
                "default output device does not support {channels}-channel {sample_rate} Hz audio"
            )
        })?;
    let sample_format = supported.sample_format();
    let config = supported.with_sample_rate(requested_sample_rate).config();
    Ok((config, sample_format))
}

/// Prefers common native formats while accepting every CPAL sample type.
fn sample_format_priority(sample_format: SampleFormat) -> u8 {
    match sample_format {
        SampleFormat::I16 => 0,
        SampleFormat::F32 => 1,
        SampleFormat::U16 => 2,
        SampleFormat::I32 | SampleFormat::U32 | SampleFormat::F64 => 3,
        SampleFormat::I8
        | SampleFormat::I24
        | SampleFormat::I64
        | SampleFormat::U8
        | SampleFormat::U64 => 4,
        _ => u8::MAX,
    }
}

/// Builds a typed callback that converts queued i16 samples on demand.
fn build_output_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    queue: Arc<BoundedPcmQueue>,
) -> Result<Stream, String> {
    match sample_format {
        SampleFormat::I8 => build_typed_output_stream::<i8>(device, config, queue),
        SampleFormat::I16 => build_typed_output_stream::<i16>(device, config, queue),
        SampleFormat::I24 => build_typed_output_stream::<cpal::I24>(device, config, queue),
        SampleFormat::I32 => build_typed_output_stream::<i32>(device, config, queue),
        SampleFormat::I64 => build_typed_output_stream::<i64>(device, config, queue),
        SampleFormat::U8 => build_typed_output_stream::<u8>(device, config, queue),
        SampleFormat::U16 => build_typed_output_stream::<u16>(device, config, queue),
        SampleFormat::U32 => build_typed_output_stream::<u32>(device, config, queue),
        SampleFormat::U64 => build_typed_output_stream::<u64>(device, config, queue),
        SampleFormat::F32 => build_typed_output_stream::<f32>(device, config, queue),
        SampleFormat::F64 => build_typed_output_stream::<f64>(device, config, queue),
        _ => Err(format!(
            "unsupported output sample format: {sample_format:?}"
        )),
    }
}

/// Connects the real-time callback to the nonblocking bounded queue.
fn build_typed_output_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    queue: Arc<BoundedPcmQueue>,
) -> Result<Stream, String>
where
    T: Sample + SizedSample + FromSample<i16>,
{
    device
        .build_output_stream::<T, _, _>(
            config,
            move |output, _| queue.fill(output),
            |error| eprintln!("[oxideterm:pcm-audio] output stream error: {error}"),
            None,
        )
        .map_err(|error| format!("build output stream: {error}"))
}
