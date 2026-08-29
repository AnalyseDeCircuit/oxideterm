// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Instant,
};

use crossbeam_channel::Sender;
use oxide_spice_helper_protocol::{
    HelperEvent, HelperPlaybackStateKind, HelperRecordStateKind, HelperRequest,
};
use oxideterm_pcm_audio::{PcmS16LeCapture, PcmS16LePlayback};

struct PlaybackChannel {
    sample_rate_hz: u32,
    channels: u16,
    muted: bool,
    volumes: Vec<u16>,
    playback: PcmS16LePlayback,
}

#[derive(Clone, Default)]
struct ChannelGain {
    muted: bool,
    volumes: Vec<u16>,
}

struct RecordChannel {
    stream_generation: u64,
    sample_rate_hz: u32,
    channels: u16,
    gain: Arc<RwLock<ChannelGain>>,
    _capture: PcmS16LeCapture,
}

#[derive(Default)]
pub(crate) struct SpiceAudioRuntime {
    playback_enabled: bool,
    capture_enabled: bool,
    playback: HashMap<u8, PlaybackChannel>,
    playback_settings: HashMap<u8, ChannelGain>,
    record: HashMap<u8, RecordChannel>,
    record_settings: HashMap<u8, ChannelGain>,
}

impl SpiceAudioRuntime {
    pub(crate) fn new(playback_enabled: bool, capture_enabled: bool) -> Self {
        Self {
            playback_enabled,
            capture_enabled,
            playback: HashMap::new(),
            playback_settings: HashMap::new(),
            record: HashMap::new(),
            record_settings: HashMap::new(),
        }
    }

    /// Consumes PCM on the SPICE reader thread so audio never enters the GPUI mailbox.
    pub(crate) fn handle_event(
        &mut self,
        event: &mut HelperEvent,
        request_tx: &Sender<HelperRequest>,
    ) -> bool {
        match event {
            HelperEvent::PlaybackState {
                channel_id,
                state,
                channels,
                sample_rate_hz,
                ..
            } => {
                if matches!(
                    state,
                    HelperPlaybackStateKind::Stopped | HelperPlaybackStateKind::Closed
                ) {
                    self.playback.remove(channel_id);
                } else if *state == HelperPlaybackStateKind::Started
                    && let (Some(channels), Some(sample_rate_hz)) = (channels, sample_rate_hz)
                    && let Ok(channels) = u16::try_from(*channels)
                    && channels > 0
                {
                    self.ensure_playback(*channel_id, *sample_rate_hz, channels);
                }
                false
            }
            HelperEvent::PlaybackSettings {
                channel_id,
                volumes,
                muted,
                ..
            } => {
                self.playback_settings.insert(
                    *channel_id,
                    ChannelGain {
                        muted: *muted,
                        volumes: volumes.clone(),
                    },
                );
                if let Some(channel) = self.playback.get_mut(channel_id) {
                    channel.volumes.clone_from(volumes);
                    channel.muted = *muted;
                }
                false
            }
            HelperEvent::PlaybackData {
                channel_id,
                channels,
                sample_rate_hz,
                pcm_s16le,
                ..
            } => {
                if self.playback_enabled
                    && let Ok(channels) = u16::try_from(*channels)
                    && channels > 0
                {
                    self.ensure_playback(*channel_id, *sample_rate_hz, channels);
                    if let Some(channel) = self.playback.get(channel_id) {
                        apply_channel_volume(
                            pcm_s16le,
                            usize::from(channel.channels),
                            channel.muted,
                            &channel.volumes,
                        );
                        channel.playback.push(pcm_s16le);
                    }
                }
                true
            }
            HelperEvent::RecordState {
                channel_id,
                stream_generation,
                state,
                start_timestamp_ms,
                channels,
                sample_rate_hz,
                ..
            } => {
                match state {
                    HelperRecordStateKind::StartRequested if self.capture_enabled => {
                        self.record.remove(channel_id);
                        let _ = request_tx.try_send(HelperRequest::RecordBegin {
                            channel_id: *channel_id,
                        });
                    }
                    HelperRecordStateKind::Recording if self.capture_enabled => {
                        if let (Some(start_timestamp_ms), Some(channels), Some(sample_rate_hz)) =
                            (start_timestamp_ms, channels, sample_rate_hz)
                            && let Ok(channels) = u16::try_from(*channels)
                            && channels > 0
                        {
                            self.ensure_capture(
                                *channel_id,
                                *stream_generation,
                                *start_timestamp_ms,
                                *sample_rate_hz,
                                channels,
                                request_tx,
                            );
                        }
                    }
                    HelperRecordStateKind::Stopped | HelperRecordStateKind::Closed => {
                        self.record.remove(channel_id);
                    }
                    HelperRecordStateKind::StartRequested | HelperRecordStateKind::Recording => {}
                }
                false
            }
            HelperEvent::RecordSettings {
                channel_id,
                volumes,
                muted,
            } => {
                let gain = ChannelGain {
                    muted: *muted,
                    volumes: volumes.clone(),
                };
                self.record_settings.insert(*channel_id, gain.clone());
                if let Some(record) = self.record.get(channel_id)
                    && let Ok(mut current) = record.gain.write()
                {
                    *current = gain;
                }
                false
            }
            _ => false,
        }
    }

    fn ensure_playback(&mut self, channel_id: u8, sample_rate_hz: u32, channels: u16) {
        if !self.playback_enabled {
            return;
        }
        let format_changed = self.playback.get(&channel_id).is_none_or(|channel| {
            channel.sample_rate_hz != sample_rate_hz || channel.channels != channels
        });
        if !format_changed {
            return;
        }
        let mut playback = PcmS16LePlayback::new(sample_rate_hz, channels);
        if playback.start().is_ok() {
            let gain = self
                .playback_settings
                .get(&channel_id)
                .cloned()
                .unwrap_or_default();
            self.playback.insert(
                channel_id,
                PlaybackChannel {
                    sample_rate_hz,
                    channels,
                    muted: gain.muted,
                    volumes: gain.volumes,
                    playback,
                },
            );
        } else {
            self.playback.remove(&channel_id);
        }
    }

    fn ensure_capture(
        &mut self,
        channel_id: u8,
        stream_generation: u64,
        start_timestamp_ms: u32,
        sample_rate_hz: u32,
        channels: u16,
        request_tx: &Sender<HelperRequest>,
    ) {
        let format_unchanged = self.record.get(&channel_id).is_some_and(|record| {
            record.stream_generation == stream_generation
                && record.sample_rate_hz == sample_rate_hz
                && record.channels == channels
        });
        if format_unchanged {
            return;
        }
        self.record.remove(&channel_id);
        let gain = Arc::new(RwLock::new(
            self.record_settings
                .get(&channel_id)
                .cloned()
                .unwrap_or_default(),
        ));
        let capture_gain = Arc::clone(&gain);
        let capture_requests = request_tx.clone();
        let capture_started_at = Instant::now();
        let frames_per_packet = usize::try_from(sample_rate_hz / 100)
            .unwrap_or(usize::MAX)
            .max(1);
        let capture = PcmS16LeCapture::start(
            sample_rate_hz,
            channels,
            frames_per_packet,
            move |mut pcm_s16le| {
                if let Ok(gain) = capture_gain.try_read() {
                    apply_channel_volume(
                        &mut pcm_s16le,
                        usize::from(channels),
                        gain.muted,
                        &gain.volumes,
                    );
                }
                let elapsed_ms =
                    (capture_started_at.elapsed().as_millis() % (u128::from(u32::MAX) + 1)) as u32;
                let _ = capture_requests.try_send(HelperRequest::RecordData {
                    channel_id,
                    timestamp_ms: start_timestamp_ms.wrapping_add(elapsed_ms),
                    pcm_s16le,
                });
            },
        );
        if let Ok(capture) = capture {
            self.record.insert(
                channel_id,
                RecordChannel {
                    stream_generation,
                    sample_rate_hz,
                    channels,
                    gain,
                    _capture: capture,
                },
            );
        }
    }
}

fn apply_channel_volume(bytes: &mut [u8], channels: usize, muted: bool, volumes: &[u16]) {
    let bytes_per_frame = channels.saturating_mul(size_of::<i16>());
    if muted {
        bytes.fill(0);
        return;
    }
    if channels == 0 || bytes_per_frame == 0 || volumes.is_empty() {
        return;
    }
    for frame in bytes.chunks_exact_mut(bytes_per_frame) {
        for (channel_index, sample) in frame.chunks_exact_mut(size_of::<i16>()).enumerate() {
            let volume = volumes.get(channel_index).copied().unwrap_or(u16::MAX);
            let value = i32::from(i16::from_le_bytes([sample[0], sample[1]]));
            let scaled = value * i32::from(volume) / i32::from(u16::MAX);
            sample.copy_from_slice(&(scaled as i16).to_le_bytes());
        }
    }
}
