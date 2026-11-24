// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Connection form and authentication state shared by the connection surfaces.

#[cfg(not(target_family = "wasm"))]
pub mod ble;
#[cfg(not(target_family = "wasm"))]
pub mod configuration;
#[cfg(not(target_family = "wasm"))]
pub mod conversion;
#[cfg(not(target_family = "wasm"))]
pub mod entity;
#[cfg(not(target_family = "wasm"))]
mod form_entity;
#[cfg(not(target_family = "wasm"))]
pub mod form_state;
pub mod icons;
mod input;
#[cfg(not(target_family = "wasm"))]
pub mod launch;
#[cfg(not(target_family = "wasm"))]
pub mod preflight;
#[cfg(not(target_family = "wasm"))]
pub mod profiles;
#[cfg(not(target_family = "wasm"))]
pub mod prompts;
#[cfg(not(target_family = "wasm"))]
pub mod proxy_launch;
#[cfg(not(target_family = "wasm"))]
pub mod saved_auth;
#[cfg(not(target_family = "wasm"))]
pub mod session_manager;
#[cfg(not(target_family = "wasm"))]
pub mod submission;
pub mod views;
pub use views::NewConnectionSelect;
#[cfg(not(target_family = "wasm"))]
pub mod workers;

#[cfg(not(target_family = "wasm"))]
pub use ble::BleConnectionWorkerResult;
#[cfg(not(target_family = "wasm"))]
pub use form_entity::ConnectionFormState;
#[cfg(not(target_family = "wasm"))]
pub use form_state::{
    ConnectionRouteTarget, NewConnectionField, NewConnectionForm, NewConnectionProxyHop,
    SavedConnectionPromptAction, SshAuthTab,
};
#[cfg(not(target_family = "wasm"))]
pub use launch::{SshConnectionIntent, SshConnectionWorkerResult};
#[cfg(not(target_family = "wasm"))]
pub use prompts::{HostKeyChallenge, KeyboardInteractiveChallenge};
