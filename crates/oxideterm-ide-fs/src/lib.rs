// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! File-system adapters for the native IDE owner.
//!
//! `oxideterm-ide-core` intentionally stays transport-free. This crate is the
//! bridge layer that binds the core `IdeFileSystem` shape to local disk and to
//! the Tauri-compatible node-first SFTP/agent path.

#[cfg(not(target_family = "wasm"))]
mod agent;
#[cfg(not(target_family = "wasm"))]
mod local;
#[cfg(not(target_family = "wasm"))]
mod node_sftp;

#[cfg(not(target_family = "wasm"))]
pub use agent::{
    AgentInstallProgress, AgentStatus, IdeSearchMatch, IdeWatchSubscription,
    NodeAgentIdeFileSystem, NodeAgentMode, NodeAgentRpcError,
    ReadFileResult as NodeAgentReadFileResult, RemoteAccess,
    SymbolIndexResult as NodeAgentSymbolIndexResult, SymbolInfo as NodeAgentSymbolInfo,
    SymbolKind as NodeAgentSymbolKind, WriteFileResult as NodeAgentWriteFileResult,
};
#[cfg(not(target_family = "wasm"))]
pub use local::LocalIdeFileSystem;
#[cfg(not(target_family = "wasm"))]
pub use node_sftp::NodeSftpIdeFileSystem;
#[cfg(not(target_family = "wasm"))]
pub use oxideterm_sftp::TransferDirection;

pub mod text;
