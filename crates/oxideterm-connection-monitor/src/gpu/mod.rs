// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! GPU and NPU monitoring domain contract.
//!
//! Provider modules own vendor commands and parsing, while the sampler owns the
//! page-scoped lifecycle. GPUI-specific state and rendering stay in the app.

mod model;
mod provider;
mod sampler;

pub use model::{
    GpuDevice, GpuProcess, GpuProvider, GpuSnapshot, GpuSnapshotStatus, GpuSummary, GpuUpdate,
    gpu_device_row_signature,
};
pub use provider::{GPU_END_MARKER, build_gpu_sample_command, parse_gpu_snapshot};
pub use sampler::{
    GPU_CHANNEL_OPEN_TIMEOUT, GPU_MAX_OUTPUT_SIZE, GPU_SAMPLE_INTERVAL, GPU_SAMPLE_TIMEOUT,
    GpuSamplingTask, start_gpu_sampling_on,
};
