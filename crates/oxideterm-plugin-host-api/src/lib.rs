// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Plugin-facing host API projections that do not own GPUI workspace state.

pub mod ai;
pub mod app;
pub mod capabilities;
pub mod forwarding;
pub mod ide;
pub mod profiler;
pub mod settings;
pub mod sftp;
pub mod terminal;
pub mod transfers;
