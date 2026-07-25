// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::borrow::Cow;

use cpal::SampleFormat;
use ironrdp::rdpsnd::{
    client::RdpsndClientHandler,
    pdu::{AudioFormat, PitchPdu, VolumePdu, WaveFormat},
};
use oxideterm_pcm_audio::PcmS16LePlayback;

const PCM_CHANNELS: u16 = 2;
const PCM_SAMPLE_RATE: u32 = 44_100;
const PCM_BITS_PER_SAMPLE: u16 = 16;
const PCM_BYTES_PER_SAMPLE: u16 = PCM_BITS_PER_SAMPLE / 8;
const PCM_BLOCK_ALIGN: u16 = PCM_CHANNELS * PCM_BYTES_PER_SAMPLE;
const PCM_AVERAGE_BYTES_PER_SECOND: u32 = PCM_SAMPLE_RATE * PCM_BLOCK_ALIGN as u32;

/// Prefers common native formats for both playback and capture devices.
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

/// Plays the single PCM format advertised through the RDPSND channel.
#[derive(Debug)]
pub(super) struct PcmRdpsndBackend {
    formats: [AudioFormat; 1],
    playback: PcmS16LePlayback,
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
            playback: PcmS16LePlayback::new(PCM_SAMPLE_RATE, PCM_CHANNELS),
        }
    }

    /// Starts the session-owned device worker when the first wave arrives.
    fn ensure_worker(&mut self) {
        if let Err(error) = self.playback.start() {
            eprintln!("[oxideterm:rdp-audio] failed to start playback worker: {error}");
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
        self.ensure_worker();
        self.playback.push(data.as_ref());
    }

    fn set_volume(&mut self, _volume: VolumePdu) {
        // Volume is intentionally not advertised until local gain is supported.
    }

    fn set_pitch(&mut self, _pitch: PitchPdu) {
        // Pitch is intentionally not advertised until local resampling exists.
    }

    fn close(&mut self) {
        self.playback.stop();
    }
}
