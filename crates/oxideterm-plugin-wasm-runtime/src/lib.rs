// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Lean Wasmtime runtime for OxideTerm native Wasm plugins.
//!
//! This crate intentionally owns only the Wasm execution boundary: entry path
//! validation, WASI setup, guest ABI calls, timeout interruption, and outbound
//! frame capture. It must not depend on the host API crate, GPUI, SSH, SFTP, or
//! connection models, so it can be reused by both the main app and the optional
//! sidecar binary.

mod index;
mod paths;
mod runtime;

pub use index::{
    WasmRuntimeAsset, WasmRuntimeDescriptor, WasmRuntimeHostChannel, WasmRuntimeIndex,
    WasmRuntimeSupport,
};
pub use paths::resolve_wasm_runtime_entry;
pub use runtime::NativeWasmPluginRuntime;

#[cfg(test)]
mod tests;
