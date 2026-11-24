// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! GPU and NPU monitoring domain contract.
//!
//! Provider modules own vendor commands and parsing, while the sampler owns the
//! page-scoped lifecycle. GPUI-specific state and rendering stay in the app.

mod model;
mod provider;
#[cfg(not(target_family = "wasm"))]
mod sampler;

use std::time::Duration;

pub use model::{
    GpuDevice, GpuProcess, GpuProvider, GpuSnapshot, GpuSnapshotStatus, GpuSummary, GpuUpdate,
    gpu_device_row_signature,
};
pub use provider::{GPU_END_MARKER, build_gpu_sample_command, parse_gpu_snapshot};
#[cfg(not(target_family = "wasm"))]
pub use sampler::{GpuSamplingTask, start_gpu_sampling_on};

pub const GPU_SAMPLE_INTERVAL: Duration = Duration::from_secs(2);
// A Windows sample runs the vendor query once (nvidia-smi is typically one to
// two seconds); the timeout only bounds hung reads.
pub const GPU_SAMPLE_TIMEOUT: Duration = Duration::from_secs(10);
pub const GPU_CHANNEL_OPEN_TIMEOUT: Duration = Duration::from_secs(10);
// Multi-accelerator hosts can return sizeable per-process and per-tile data,
// while the cap still prevents an unbounded remote command response.
pub const GPU_MAX_OUTPUT_SIZE: usize = 512 * 1024;
