// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    sync::mpsc::{self, SyncSender},
    thread::{self, JoinHandle},
    time::Duration,
};

use cpal::{
    FromSample, Sample, SampleFormat, SizedSample, StreamConfig,
    traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _},
};

const CAPTURE_START_TIMEOUT: Duration = Duration::from_secs(2);
const AUDIO_CAPTURE_THREAD_NAME: &str = "oxideterm-pcm-capture";

/// Owns one joinable microphone stream that emits bounded-size S16LE packets.
#[derive(Debug)]
pub struct PcmS16LeCapture {
    shutdown_tx: Option<SyncSender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl PcmS16LeCapture {
    pub fn start<F>(
        sample_rate: u32,
        channels: u16,
        frames_per_packet: usize,
        on_packet: F,
    ) -> Result<Self, String>
    where
        F: FnMut(Vec<u8>) + Send + 'static,
    {
        let packet_samples = frames_per_packet
            .checked_mul(usize::from(channels))
            .filter(|samples| *samples > 0)
            .ok_or_else(|| "microphone packet size is invalid".to_string())?;
        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name(AUDIO_CAPTURE_THREAD_NAME.to_string())
            .spawn(move || {
                run_pcm_input(
                    sample_rate,
                    channels,
                    packet_samples,
                    on_packet,
                    shutdown_rx,
                    startup_tx,
                );
            })
            .map_err(|error| format!("start microphone thread: {error}"))?;
        match startup_rx.recv_timeout(CAPTURE_START_TIMEOUT) {
            Ok(Ok(())) => Ok(Self {
                shutdown_tx: Some(shutdown_tx),
                handle: Some(handle),
            }),
            Ok(Err(error)) => {
                let _ = handle.join();
                Err(error)
            }
            Err(error) => {
                let _ = shutdown_tx.try_send(());
                let _ = handle.join();
                Err(format!("microphone startup timed out: {error}"))
            }
        }
    }

    /// Stops the device before joining its owner thread.
    pub fn stop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.try_send(());
        }
        if let Some(handle) = self.handle.take()
            && let Err(error) = handle.join()
        {
            eprintln!("[oxideterm:pcm-audio] capture worker panicked: {error:?}");
        }
    }
}

impl Drop for PcmS16LeCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_pcm_input<F>(
    sample_rate: u32,
    channels: u16,
    packet_samples: usize,
    on_packet: F,
    shutdown_rx: mpsc::Receiver<()>,
    startup_tx: SyncSender<Result<(), String>>,
) where
    F: FnMut(Vec<u8>) + Send + 'static,
{
    let result = (|| {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "no default input device".to_string())?;
        let (config, sample_format) = compatible_input_config(&device, sample_rate, channels)?;
        let stream =
            build_input_stream(&device, &config, sample_format, packet_samples, on_packet)?;
        stream
            .play()
            .map_err(|error| format!("start input stream: {error}"))?;
        let _ = startup_tx.send(Ok(()));
        let _ = shutdown_rx.recv();
        Ok::<_, String>(())
    })();
    if let Err(error) = result {
        let _ = startup_tx.try_send(Err(error));
    }
}

fn compatible_input_config(
    device: &cpal::Device,
    sample_rate: u32,
    channels: u16,
) -> Result<(StreamConfig, SampleFormat), String> {
    let requested_rate = cpal::SampleRate(sample_rate);
    let supported = device
        .supported_input_configs()
        .map_err(|error| format!("query input formats: {error}"))?
        .filter(|config| {
            config.channels() == channels
                && config.min_sample_rate() <= requested_rate
                && requested_rate <= config.max_sample_rate()
        })
        .min_by_key(|config| sample_format_priority(config.sample_format()))
        .ok_or_else(|| {
            format!(
                "default input device does not support {channels}-channel {sample_rate} Hz audio"
            )
        })?;
    let sample_format = supported.sample_format();
    Ok((
        supported.with_sample_rate(requested_rate).config(),
        sample_format,
    ))
}

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

fn build_input_stream<F>(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    packet_samples: usize,
    on_packet: F,
) -> Result<cpal::Stream, String>
where
    F: FnMut(Vec<u8>) + Send + 'static,
{
    match sample_format {
        SampleFormat::I8 => {
            build_typed_input_stream::<i8, _>(device, config, packet_samples, on_packet)
        }
        SampleFormat::I16 => {
            build_typed_input_stream::<i16, _>(device, config, packet_samples, on_packet)
        }
        SampleFormat::I24 => {
            build_typed_input_stream::<cpal::I24, _>(device, config, packet_samples, on_packet)
        }
        SampleFormat::I32 => {
            build_typed_input_stream::<i32, _>(device, config, packet_samples, on_packet)
        }
        SampleFormat::I64 => {
            build_typed_input_stream::<i64, _>(device, config, packet_samples, on_packet)
        }
        SampleFormat::U8 => {
            build_typed_input_stream::<u8, _>(device, config, packet_samples, on_packet)
        }
        SampleFormat::U16 => {
            build_typed_input_stream::<u16, _>(device, config, packet_samples, on_packet)
        }
        SampleFormat::U32 => {
            build_typed_input_stream::<u32, _>(device, config, packet_samples, on_packet)
        }
        SampleFormat::U64 => {
            build_typed_input_stream::<u64, _>(device, config, packet_samples, on_packet)
        }
        SampleFormat::F32 => {
            build_typed_input_stream::<f32, _>(device, config, packet_samples, on_packet)
        }
        SampleFormat::F64 => {
            build_typed_input_stream::<f64, _>(device, config, packet_samples, on_packet)
        }
        _ => Err(format!(
            "unsupported input sample format: {sample_format:?}"
        )),
    }
}

fn build_typed_input_stream<T, F>(
    device: &cpal::Device,
    config: &StreamConfig,
    packet_samples: usize,
    mut on_packet: F,
) -> Result<cpal::Stream, String>
where
    T: Sample + SizedSample,
    i16: FromSample<T>,
    F: FnMut(Vec<u8>) + Send + 'static,
{
    let mut pending_samples = Vec::with_capacity(packet_samples);
    device
        .build_input_stream::<T, _, _>(
            config,
            move |input, _| {
                for sample in input {
                    pending_samples.push(i16::from_sample(*sample));
                    if pending_samples.len() == packet_samples {
                        let mut packet = Vec::with_capacity(packet_samples * size_of::<i16>());
                        for sample in pending_samples.drain(..) {
                            packet.extend_from_slice(&sample.to_le_bytes());
                        }
                        on_packet(packet);
                    }
                }
            },
            |error| eprintln!("[oxideterm:pcm-audio] input stream error: {error}"),
            None,
        )
        .map_err(|error| format!("build input stream: {error}"))
}
