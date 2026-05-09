// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! File-system adapters for the native IDE owner.
//!
//! `oxideterm-ide-core` intentionally stays transport-free. This crate is the
//! bridge layer that binds the core `IdeFileSystem` shape to local disk and to
//! the Tauri-compatible node-first SFTP/agent path.

mod agent;
mod local;
mod node_sftp;

pub use agent::{AgentStatus, NodeAgentIdeFileSystem, NodeAgentMode};
pub use local::LocalIdeFileSystem;
pub use node_sftp::NodeSftpIdeFileSystem;
