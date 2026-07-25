// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use cpal::{
    FromSample, Sample, SampleFormat, SizedSample, StreamConfig,
    traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _},
};
use ironrdp::{
    dvc::{DvcClientProcessor, DvcEncode, DvcMessage, DvcProcessor},
    pdu::PduResult,
};
use ironrdp_core::{Encode, EncodeResult, WriteCursor, impl_as_any};

use super::RdpInputEvent;

const AUDIO_INPUT_CHANNEL_NAME: &str = "AUDIO_INPUT";
const AUDIO_INPUT_VERSION: u32 = 2;
const AUDIO_FORMAT_PCM: u16 = 1;
const AUDIO_INPUT_BITS_PER_SAMPLE: u16 = 16;
const AUDIO_INPUT_MAX_FORMATS: usize = 1_000;
const AUDIO_INPUT_MAX_QUEUED_PACKETS: usize = 8;
const AUDIO_INPUT_START_TIMEOUT: Duration = Duration::from_secs(2);
const AUDIO_INPUT_THREAD_NAME: &str = "oxideterm-rdp-microphone";

const MSG_SNDIN_VERSION: u8 = 0x01;
const MSG_SNDIN_FORMATS: u8 = 0x02;
const MSG_SNDIN_OPEN: u8 = 0x03;
const MSG_SNDIN_OPEN_REPLY: u8 = 0x04;
const MSG_SNDIN_DATA_INCOMING: u8 = 0x05;
const MSG_SNDIN_DATA: u8 = 0x06;
const MSG_SNDIN_FORMAT_CHANGE: u8 = 0x07;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PcmCaptureFormat {
    channels: u16,
    sample_rate: u32,
}

impl PcmCaptureFormat {
    fn block_align(self) -> u16 {
        self.channels * (AUDIO_INPUT_BITS_PER_SAMPLE / 8)
    }

    fn average_bytes_per_second(self) -> u32 {
        self.sample_rate * u32::from(self.block_align())
    }

    fn encode_wave_format(self, payload: &mut Vec<u8>) {
        payload.extend_from_slice(&AUDIO_FORMAT_PCM.to_le_bytes());
        payload.extend_from_slice(&self.channels.to_le_bytes());
        payload.extend_from_slice(&self.sample_rate.to_le_bytes());
        payload.extend_from_slice(&self.average_bytes_per_second().to_le_bytes());
        payload.extend_from_slice(&self.block_align().to_le_bytes());
        payload.extend_from_slice(&AUDIO_INPUT_BITS_PER_SAMPLE.to_le_bytes());
        payload.extend_from_slice(&0_u16.to_le_bytes());
    }
}

/// Implements the MS-RDPEAI AUDIO_INPUT dynamic virtual channel.
pub(super) struct AudioInputClient {
    selected_format: Option<PcmCaptureFormat>,
    capture: Option<MicrophoneCaptureWorker>,
    queue: Arc<MicrophonePacketQueue>,
}

impl AudioInputClient {
    pub(super) fn new(input_tx: tokio::sync::mpsc::UnboundedSender<RdpInputEvent>) -> Self {
        Self {
            selected_format: None,
            capture: None,
            queue: Arc::new(MicrophonePacketQueue::new(input_tx)),
        }
    }

    pub(super) fn drain_messages(&self) -> Vec<DvcMessage> {
        self.queue
            .drain()
            .into_iter()
            .flat_map(|packet| {
                [
                    raw_message(vec![MSG_SNDIN_DATA_INCOMING]),
                    raw_message_with_payload(MSG_SNDIN_DATA, packet),
                ]
            })
            .collect()
    }

    fn process_version(&self, payload: &[u8]) -> Vec<DvcMessage> {
        let Some(server_version) = read_u32(payload, 1) else {
            return Vec::new();
        };
        if server_version > AUDIO_INPUT_VERSION {
            return Vec::new();
        }
        raw_messages([raw_message_with_u32(MSG_SNDIN_VERSION, AUDIO_INPUT_VERSION)])
    }

    fn process_formats(&mut self, payload: &[u8]) -> Vec<DvcMessage> {
        let Some(format_count) = read_u32(payload, 1).map(|count| count as usize) else {
            return Vec::new();
        };
        if format_count == 0 || format_count > AUDIO_INPUT_MAX_FORMATS {
            return Vec::new();
        }
        let Some(packet_size) = read_u32(payload, 5).map(|size| size as usize) else {
            return Vec::new();
        };
        if packet_size > payload.len() || packet_size < 9 {
            return Vec::new();
        }

        let mut cursor = 9;
        let mut selected = None;
        for _ in 0..format_count {
            let Some((format, next_cursor)) = parse_wave_format(payload, cursor) else {
                return Vec::new();
            };
            cursor = next_cursor;
            if selected.is_none() && capture_format_supported(format) {
                selected = Some(format);
            }
        }
        self.selected_format = selected;

        let mut response = vec![MSG_SNDIN_FORMATS];
        response.extend_from_slice(&u32::from(selected.is_some()).to_le_bytes());
        let response_size = 9 + selected.map(|_| 18).unwrap_or(0);
        response.extend_from_slice(&(response_size as u32).to_le_bytes());
        if let Some(format) = selected {
            format.encode_wave_format(&mut response);
        }
        raw_messages([
            raw_message(vec![MSG_SNDIN_DATA_INCOMING]),
            raw_message(response),
        ])
    }

    fn process_open(&mut self, payload: &[u8]) -> Vec<DvcMessage> {
        let Some(frames_per_packet) = read_u32(payload, 1) else {
            return Vec::new();
        };
        let Some(initial_format) = read_u32(payload, 5) else {
            return Vec::new();
        };
        if initial_format != 0 || frames_per_packet == 0 {
            return raw_messages([raw_message_with_u32(MSG_SNDIN_OPEN_REPLY, 1)]);
        }
        let Some(format) = self.selected_format else {
            return raw_messages([raw_message_with_u32(MSG_SNDIN_OPEN_REPLY, 1)]);
        };
        let max_frames_per_packet = format.sample_rate.saturating_mul(2);
        if frames_per_packet > max_frames_per_packet {
            return raw_messages([raw_message_with_u32(MSG_SNDIN_OPEN_REPLY, 1)]);
        }

        self.stop_capture();
        match MicrophoneCaptureWorker::spawn(format, frames_per_packet, Arc::clone(&self.queue)) {
            Ok(capture) => {
                self.capture = Some(capture);
                raw_messages([
                    raw_message_with_u32(MSG_SNDIN_FORMAT_CHANGE, 0),
                    raw_message_with_u32(MSG_SNDIN_OPEN_REPLY, 0),
                ])
            }
            Err(error) => {
                eprintln!("[oxideterm:rdp-audio] microphone unavailable: {error}");
                raw_messages([raw_message_with_u32(MSG_SNDIN_OPEN_REPLY, 1)])
            }
        }
    }

    fn process_format_change(&mut self, payload: &[u8]) -> Vec<DvcMessage> {
        let Some(format_index) = read_u32(payload, 1) else {
            return Vec::new();
        };
        if format_index != 0 || self.selected_format.is_none() {
            return Vec::new();
        }
        raw_messages([raw_message_with_u32(MSG_SNDIN_FORMAT_CHANGE, 0)])
    }

    fn stop_capture(&mut self) {
        if let Some(mut capture) = self.capture.take() {
            capture.shutdown();
        }
        self.queue.clear();
    }
}

impl Drop for AudioInputClient {
    fn drop(&mut self) {
        self.stop_capture();
    }
}

impl_as_any!(AudioInputClient);

impl DvcProcessor for AudioInputClient {
    fn channel_name(&self) -> &str {
        AUDIO_INPUT_CHANNEL_NAME
    }

    fn start(&mut self, _channel_id: u32) -> PduResult<Vec<DvcMessage>> {
        Ok(Vec::new())
    }

    fn process(&mut self, _channel_id: u32, payload: &[u8]) -> PduResult<Vec<DvcMessage>> {
        let messages = match payload.first().copied() {
            Some(MSG_SNDIN_VERSION) => self.process_version(payload),
            Some(MSG_SNDIN_FORMATS) => self.process_formats(payload),
            Some(MSG_SNDIN_OPEN) => self.process_open(payload),
            Some(MSG_SNDIN_FORMAT_CHANGE) => self.process_format_change(payload),
            _ => Vec::new(),
        };
        Ok(messages)
    }

    fn close(&mut self, _channel_id: u32) {
        self.stop_capture();
    }
}

impl DvcClientProcessor for AudioInputClient {}

#[derive(Debug)]
struct RawAudioInputMessage(Vec<u8>);

impl Encode for RawAudioInputMessage {
    fn encode(&self, destination: &mut WriteCursor<'_>) -> EncodeResult<()> {
        destination.write_slice(&self.0);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "SNDIN_PDU"
    }

    fn size(&self) -> usize {
        self.0.len()
    }
}

impl DvcEncode for RawAudioInputMessage {}

fn raw_message(payload: Vec<u8>) -> DvcMessage {
    Box::new(RawAudioInputMessage(payload))
}

fn raw_message_with_u32(message_id: u8, value: u32) -> DvcMessage {
    raw_message_with_payload(message_id, value.to_le_bytes().to_vec())
}

fn raw_message_with_payload(message_id: u8, payload: Vec<u8>) -> DvcMessage {
    let mut message = Vec::with_capacity(payload.len() + 1);
    message.push(message_id);
    message.extend_from_slice(&payload);
    raw_message(message)
}

fn raw_messages<const N: usize>(messages: [DvcMessage; N]) -> Vec<DvcMessage> {
    Vec::from(messages)
}

fn read_u32(payload: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        payload
            .get(offset..offset.checked_add(4)?)?
            .try_into()
            .ok()?,
    ))
}

fn parse_wave_format(payload: &[u8], offset: usize) -> Option<(PcmCaptureFormat, usize)> {
    let fixed = payload.get(offset..offset.checked_add(18)?)?;
    let format_tag = u16::from_le_bytes(fixed[0..2].try_into().ok()?);
    let channels = u16::from_le_bytes(fixed[2..4].try_into().ok()?);
    let sample_rate = u32::from_le_bytes(fixed[4..8].try_into().ok()?);
    let block_align = u16::from_le_bytes(fixed[12..14].try_into().ok()?);
    let bits_per_sample = u16::from_le_bytes(fixed[14..16].try_into().ok()?);
    let extra_size = u16::from_le_bytes(fixed[16..18].try_into().ok()?) as usize;
    let next_offset = offset.checked_add(18)?.checked_add(extra_size)?;
    payload.get(offset..next_offset)?;

    let format = PcmCaptureFormat {
        channels,
        sample_rate,
    };
    if format_tag != AUDIO_FORMAT_PCM
        || bits_per_sample != AUDIO_INPUT_BITS_PER_SAMPLE
        || block_align != format.block_align()
    {
        return Some((
            PcmCaptureFormat {
                channels: 0,
                sample_rate: 0,
            },
            next_offset,
        ));
    }
    Some((format, next_offset))
}

fn capture_format_supported(format: PcmCaptureFormat) -> bool {
    if !(1..=2).contains(&format.channels) || format.sample_rate == 0 {
        return false;
    }
    let host = cpal::default_host();
    let Some(device) = host.default_input_device() else {
        return false;
    };
    compatible_input_config(&device, format).is_ok()
}

#[derive(Debug)]
struct MicrophonePacketQueue {
    packets: Mutex<VecDeque<Vec<u8>>>,
    notification_pending: AtomicBool,
    input_tx: tokio::sync::mpsc::UnboundedSender<RdpInputEvent>,
}

impl MicrophonePacketQueue {
    fn new(input_tx: tokio::sync::mpsc::UnboundedSender<RdpInputEvent>) -> Self {
        Self {
            packets: Mutex::new(VecDeque::with_capacity(AUDIO_INPUT_MAX_QUEUED_PACKETS)),
            notification_pending: AtomicBool::new(false),
            input_tx,
        }
    }

    fn push(&self, packet: Vec<u8>) {
        let mut packets = match self.packets.try_lock() {
            Ok(packets) => packets,
            Err(_) => return,
        };
        if packets.len() == AUDIO_INPUT_MAX_QUEUED_PACKETS {
            packets.pop_front();
        }
        packets.push_back(packet);
        drop(packets);

        if !self.notification_pending.swap(true, Ordering::AcqRel) {
            let _ = self.input_tx.send(RdpInputEvent::MicrophoneReady);
        }
    }

    fn drain(&self) -> Vec<Vec<u8>> {
        let packets = {
            let mut packets = match self.packets.lock() {
                Ok(packets) => packets,
                Err(error) => error.into_inner(),
            };
            packets.drain(..).collect::<Vec<_>>()
        };
        self.notification_pending.store(false, Ordering::Release);
        let should_rearm = self
            .packets
            .lock()
            .map(|packets| !packets.is_empty())
            .unwrap_or(true);
        if should_rearm && !self.notification_pending.swap(true, Ordering::AcqRel) {
            let _ = self.input_tx.send(RdpInputEvent::MicrophoneReady);
        }
        packets
    }

    fn clear(&self) {
        match self.packets.lock() {
            Ok(mut packets) => packets.clear(),
            Err(error) => error.into_inner().clear(),
        }
        self.notification_pending.store(false, Ordering::Release);
    }
}

#[derive(Debug)]
struct MicrophoneCaptureWorker {
    shutdown_tx: Option<SyncSender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl MicrophoneCaptureWorker {
    fn spawn(
        format: PcmCaptureFormat,
        frames_per_packet: u32,
        queue: Arc<MicrophonePacketQueue>,
    ) -> Result<Self, String> {
        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name(AUDIO_INPUT_THREAD_NAME.to_string())
            .spawn(move || {
                run_microphone_input(format, frames_per_packet, queue, shutdown_rx, startup_tx)
            })
            .map_err(|error| format!("start microphone thread: {error}"))?;
        match startup_rx.recv_timeout(AUDIO_INPUT_START_TIMEOUT) {
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

    fn shutdown(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.try_send(());
        }
        if let Some(handle) = self.handle.take()
            && let Err(error) = handle.join()
        {
            eprintln!("[oxideterm:rdp-audio] microphone worker panicked: {error:?}");
        }
    }
}

impl Drop for MicrophoneCaptureWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_microphone_input(
    format: PcmCaptureFormat,
    frames_per_packet: u32,
    queue: Arc<MicrophonePacketQueue>,
    shutdown_rx: mpsc::Receiver<()>,
    startup_tx: SyncSender<Result<(), String>>,
) {
    let result = (|| {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "no default input device".to_string())?;
        let (config, sample_format) = compatible_input_config(&device, format)?;
        let packet_samples = usize::try_from(frames_per_packet)
            .ok()
            .and_then(|frames| frames.checked_mul(usize::from(format.channels)))
            .ok_or_else(|| "microphone packet size overflowed".to_string())?;
        let stream = build_input_stream(&device, &config, sample_format, packet_samples, queue)?;
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
    format: PcmCaptureFormat,
) -> Result<(StreamConfig, SampleFormat), String> {
    let requested_rate = cpal::SampleRate(format.sample_rate);
    let supported = device
        .supported_input_configs()
        .map_err(|error| format!("query input formats: {error}"))?
        .filter(|config| {
            config.channels() == format.channels
                && config.min_sample_rate() <= requested_rate
                && requested_rate <= config.max_sample_rate()
        })
        .min_by_key(|config| super::audio::sample_format_priority(config.sample_format()))
        .ok_or_else(|| {
            format!(
                "default input device does not support {}-channel {} Hz audio",
                format.channels, format.sample_rate
            )
        })?;
    let sample_format = supported.sample_format();
    Ok((
        supported.with_sample_rate(requested_rate).config(),
        sample_format,
    ))
}

fn build_input_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    packet_samples: usize,
    queue: Arc<MicrophonePacketQueue>,
) -> Result<cpal::Stream, String> {
    match sample_format {
        SampleFormat::I8 => build_typed_input_stream::<i8>(device, config, packet_samples, queue),
        SampleFormat::I16 => build_typed_input_stream::<i16>(device, config, packet_samples, queue),
        SampleFormat::I24 => {
            build_typed_input_stream::<cpal::I24>(device, config, packet_samples, queue)
        }
        SampleFormat::I32 => build_typed_input_stream::<i32>(device, config, packet_samples, queue),
        SampleFormat::I64 => build_typed_input_stream::<i64>(device, config, packet_samples, queue),
        SampleFormat::U8 => build_typed_input_stream::<u8>(device, config, packet_samples, queue),
        SampleFormat::U16 => build_typed_input_stream::<u16>(device, config, packet_samples, queue),
        SampleFormat::U32 => build_typed_input_stream::<u32>(device, config, packet_samples, queue),
        SampleFormat::U64 => build_typed_input_stream::<u64>(device, config, packet_samples, queue),
        SampleFormat::F32 => build_typed_input_stream::<f32>(device, config, packet_samples, queue),
        SampleFormat::F64 => build_typed_input_stream::<f64>(device, config, packet_samples, queue),
        _ => Err(format!(
            "unsupported input sample format: {sample_format:?}"
        )),
    }
}

fn build_typed_input_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    packet_samples: usize,
    queue: Arc<MicrophonePacketQueue>,
) -> Result<cpal::Stream, String>
where
    T: Sample + SizedSample,
    i16: FromSample<T>,
{
    let mut pending_samples = Vec::with_capacity(packet_samples);
    device
        .build_input_stream::<T, _, _>(
            config,
            move |input, _| {
                for sample in input {
                    pending_samples.push(i16::from_sample(*sample));
                    if pending_samples.len() == packet_samples {
                        let mut packet = Vec::with_capacity(packet_samples * 2);
                        for sample in pending_samples.drain(..) {
                            packet.extend_from_slice(&sample.to_le_bytes());
                        }
                        queue.push(packet);
                    }
                }
            },
            |error| eprintln!("[oxideterm:rdp-audio] input stream error: {error}"),
            None,
        )
        .map_err(|error| format!("build input stream: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pcm_wave_format_and_skips_extra_data() {
        let mut payload = vec![0; 9];
        PcmCaptureFormat {
            channels: 2,
            sample_rate: 48_000,
        }
        .encode_wave_format(&mut payload);

        let (format, next) = parse_wave_format(&payload, 9).expect("valid format");

        assert_eq!(format.channels, 2);
        assert_eq!(format.sample_rate, 48_000);
        assert_eq!(next, payload.len());
    }

    #[test]
    fn rejects_truncated_wave_format() {
        assert!(parse_wave_format(&[0; 17], 0).is_none());
    }
}
