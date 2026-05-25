// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Plugin-facing host API projections that do not own GPUI workspace state.

pub mod ai;
pub mod app;
pub mod backend;
pub mod capabilities;
pub mod forwarding;
pub mod ide;
pub mod profiler;
pub mod readonly;
pub mod runtime;
pub mod settings;
pub mod sftp;
pub mod sync;
pub mod terminal;
pub mod transfers;
