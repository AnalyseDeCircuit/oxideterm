// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    borrow::Cow,
    collections::VecDeque,
    sync::{
        Arc, Mutex, TryLockError,
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
};

use cpal::{
    FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _},
};
use ironrdp::rdpsnd::{
    client::RdpsndClientHandler,
    pdu::{AudioFormat, PitchPdu, VolumePdu, WaveFormat},
};

const PCM_CHANNELS: u16 = 2;
const PCM_SAMPLE_RATE: u32 = 44_100;
const PCM_BITS_PER_SAMPLE: u16 = 16;
const PCM_BYTES_PER_SAMPLE: u16 = PCM_BITS_PER_SAMPLE / 8;
const PCM_BLOCK_ALIGN: u16 = PCM_CHANNELS * PCM_BYTES_PER_SAMPLE;
const PCM_AVERAGE_BYTES_PER_SECOND: u32 = PCM_SAMPLE_RATE * PCM_BLOCK_ALIGN as u32;
const PCM_BUFFER_DURATION_MILLISECONDS: usize = 500;
const PCM_BUFFER_CAPACITY_SAMPLES: usize =
    PCM_SAMPLE_RATE as usize * PCM_CHANNELS as usize * PCM_BUFFER_DURATION_MILLISECONDS / 1_000;
const AUDIO_PLAYBACK_THREAD_NAME: &str = "oxideterm-rdp-audio";

/// Plays the single PCM format advertised through the RDPSND channel.
#[derive(Debug)]
pub(super) struct PcmRdpsndBackend {
    formats: [AudioFormat; 1],
    queue: Arc<BoundedPcmQueue>,
    worker: Option<AudioPlaybackWorker>,
}

impl PcmRdpsndBackend {
    /// Creates a backend without opening the local device until audio arrives.
    pub(super) fn new() -> Self {
        Self {
            formats: [AudioFormat {
                format: WaveFormat::PCM,
                n_channels: PCM_CHANNELS,
                n_samples_per_sec: PCM_SAMPLE_RATE,
                n_avg_bytes_per_sec: PCM_AVERAGE_BYTES_PER_SECOND,
                n_block_align: PCM_BLOCK_ALIGN,
                bits_per_sample: PCM_BITS_PER_SAMPLE,
                data: None,
            }],
            queue: Arc::new(BoundedPcmQueue::new(
                PCM_BUFFER_CAPACITY_SAMPLES,
                usize::from(PCM_CHANNELS),
            )),
            worker: None,
        }
    }

    /// Starts exactly one device worker for the active RDPSND stream.
    fn ensure_worker(&mut self) {
        if self.worker.is_some() {
            return;
        }

        match AudioPlaybackWorker::spawn(Arc::clone(&self.queue)) {
            Ok(worker) => self.worker = Some(worker),
            Err(error) => {
                eprintln!("[oxideterm:rdp-audio] failed to start playback worker: {error}");
            }
        }
    }
}

impl Drop for PcmRdpsndBackend {
    fn drop(&mut self) {
        self.close();
    }
}

impl RdpsndClientHandler for PcmRdpsndBackend {
    fn get_formats(&self) -> &[AudioFormat] {
        &self.formats
    }

    fn wave(&mut self, _format_no: usize, _timestamp: u32, data: Cow<'_, [u8]>) {
        // Only one exact PCM format is advertised. The wire format number
        // indexes the server's format table and must not index this local list.
        self.queue.push_pcm(data.as_ref());
        self.ensure_worker();
    }

    fn set_volume(&mut self, _volume: VolumePdu) {
        // Volume is intentionally not advertised until local gain is supported.
    }

    fn set_pitch(&mut self, _pitch: PitchPdu) {
        // Pitch is intentionally not advertised until local resampling exists.
    }

    fn close(&mut self) {
        if let Some(mut worker) = self.worker.take() {
            worker.shutdown();
        }
        self.queue.clear();
    }
}

/// Owns the non-Send CPAL stream on a dedicated, joinable thread.
#[derive(Debug)]
struct AudioPlaybackWorker {
    shutdown_tx: Option<SyncSender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl AudioPlaybackWorker {
    /// Spawns the session-owned output thread with a bounded shutdown channel.
    fn spawn(queue: Arc<BoundedPcmQueue>) -> std::io::Result<Self> {
        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name(AUDIO_PLAYBACK_THREAD_NAME.to_string())
            .spawn(move || {
                if let Err(error) = run_pcm_output(queue, shutdown_rx) {
                    eprintln!("[oxideterm:rdp-audio] playback unavailable: {error}");
                }
            })?;

        Ok(Self {
            shutdown_tx: Some(shutdown_tx),
            handle: Some(handle),
        })
    }

    /// Stops the device loop and joins it before the owning RDP channel exits.
    fn shutdown(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.try_send(());
        }
        if let Some(handle) = self.handle.take()
            && let Err(error) = handle.join()
        {
            eprintln!("[oxideterm:rdp-audio] playback worker panicked: {error:?}");
        }
    }
}

impl Drop for AudioPlaybackWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Opens a compatible output stream and keeps it alive until session cleanup.
fn run_pcm_output(
    queue: Arc<BoundedPcmQueue>,
    shutdown_rx: mpsc::Receiver<()>,
) -> Result<(), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "no default output device".to_string())?;
    let (config, sample_format) = compatible_output_config(&device)?;
    let stream = build_output_stream(&device, &config, sample_format, queue)?;
    stream
        .play()
        .map_err(|error| format!("start output stream: {error}"))?;

    // The bounded channel also wakes when its sender is dropped.
    let _ = shutdown_rx.recv();
    Ok(())
}

/// Selects a local device format that accepts the advertised PCM rate directly.
fn compatible_output_config(device: &cpal::Device) -> Result<(StreamConfig, SampleFormat), String> {
    let requested_sample_rate = cpal::SampleRate(PCM_SAMPLE_RATE);
    let supported = device
        .supported_output_configs()
        .map_err(|error| format!("query output formats: {error}"))?
        .filter(|format| {
            format.channels() == PCM_CHANNELS
                && format.min_sample_rate() <= requested_sample_rate
                && requested_sample_rate <= format.max_sample_rate()
        })
        .min_by_key(|format| sample_format_priority(format.sample_format()))
        .ok_or_else(|| {
            format!(
                "default output device does not support {PCM_CHANNELS}-channel {PCM_SAMPLE_RATE} Hz audio"
            )
        })?;
    let sample_format = supported.sample_format();
    let config = supported.with_sample_rate(requested_sample_rate).config();
    Ok((config, sample_format))
}

/// Prefers common native formats while still supporting every CPAL sample type.
pub(super) fn sample_format_priority(sample_format: SampleFormat) -> u8 {
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

/// Builds a typed CPAL stream and converts the negotiated i16 PCM on demand.
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

/// Connects the nonblocking callback boundary to the bounded PCM queue.
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
            |error| eprintln!("[oxideterm:rdp-audio] output stream error: {error}"),
            None,
        )
        .map_err(|error| format!("build output stream: {error}"))
}

/// Buffers at most a fixed amount of PCM without waiting in either caller.
#[derive(Debug)]
struct BoundedPcmQueue {
    samples: Mutex<VecDeque<i16>>,
    capacity_samples: usize,
    channels: usize,
}

impl BoundedPcmQueue {
    /// Creates a frame-aligned queue with a fixed sample capacity.
    fn new(capacity_samples: usize, channels: usize) -> Self {
        let aligned_capacity = capacity_samples - capacity_samples % channels;
        Self {
            samples: Mutex::new(VecDeque::with_capacity(aligned_capacity)),
            capacity_samples: aligned_capacity,
            channels,
        }
    }

    /// Appends complete PCM frames, dropping oldest frames when latency grows.
    fn push_pcm(&self, bytes: &[u8]) {
        let mut samples = match self.samples.try_lock() {
            Ok(samples) => samples,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
            Err(TryLockError::WouldBlock) => return,
        };
        let bytes_per_frame = self.channels * usize::from(PCM_BYTES_PER_SAMPLE);
        let complete_bytes = bytes.len() - bytes.len() % bytes_per_frame;
        let retained_bytes = self.capacity_samples * usize::from(PCM_BYTES_PER_SAMPLE);
        let first_byte = complete_bytes.saturating_sub(retained_bytes);
        let incoming_samples = (complete_bytes - first_byte) / usize::from(PCM_BYTES_PER_SAMPLE);
        let overflow_samples = samples
            .len()
            .saturating_add(incoming_samples)
            .saturating_sub(self.capacity_samples);
        let samples_to_drop = overflow_samples.div_ceil(self.channels) * self.channels;

        for _ in 0..samples_to_drop.min(samples.len()) {
            samples.pop_front();
        }
        for sample in bytes[first_byte..complete_bytes].chunks_exact(2) {
            samples.push_back(i16::from_le_bytes([sample[0], sample[1]]));
        }
    }

    /// Fills the real-time callback without blocking and emits silence on underrun.
    fn fill<T>(&self, output: &mut [T])
    where
        T: Sample + FromSample<i16>,
    {
        let mut samples = match self.samples.try_lock() {
            Ok(samples) => Some(samples),
            Err(TryLockError::Poisoned(error)) => Some(error.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        };

        for output_sample in output {
            let pcm_sample = samples
                .as_mut()
                .and_then(|samples| samples.pop_front())
                .unwrap_or(0);
            *output_sample = T::from_sample(pcm_sample);
        }
    }

    /// Clears queued audio after its device callback has stopped.
    fn clear(&self) {
        match self.samples.lock() {
            Ok(mut samples) => samples.clear(),
            Err(error) => error.into_inner().clear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes test samples using the RDPSND little-endian PCM representation.
    fn pcm_bytes(samples: &[i16]) -> Vec<u8> {
        samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect()
    }

    #[test]
    fn queue_drops_oldest_complete_frames_when_capacity_is_reached() {
        let queue = BoundedPcmQueue::new(4, 2);
        queue.push_pcm(&pcm_bytes(&[1, 2, 3, 4]));
        queue.push_pcm(&pcm_bytes(&[5, 6]));
        let mut output = [0_i16; 4];

        queue.fill(&mut output);

        assert_eq!(output, [3, 4, 5, 6]);
    }

    #[test]
    fn queue_emits_silence_after_buffered_audio_is_consumed() {
        let queue = BoundedPcmQueue::new(4, 2);
        queue.push_pcm(&pcm_bytes(&[10, -10]));
        let mut output = [1_i16; 4];

        queue.fill(&mut output);

        assert_eq!(output, [10, -10, 0, 0]);
    }

    #[test]
    fn queue_drops_input_instead_of_waiting_for_the_callback_lock() {
        let queue = BoundedPcmQueue::new(4, 2);
        let guard = queue.samples.lock().expect("test queue lock");

        queue.push_pcm(&pcm_bytes(&[1, 2]));

        assert!(guard.is_empty());
    }
}
