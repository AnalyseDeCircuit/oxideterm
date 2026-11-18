// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Connection form and authentication state shared by the desktop connection surfaces.

pub mod ble;
pub mod configuration;
pub mod conversion;
pub mod entity;
mod form_entity;
pub mod form_state;
pub mod icons;
mod input;
pub mod launch;
pub mod preflight;
pub mod profiles;
pub mod prompts;
pub mod proxy_launch;
pub mod saved_auth;
pub mod session_manager;
pub mod submission;
pub mod views;
pub mod workers;

pub use ble::BleConnectionWorkerResult;
pub use form_entity::ConnectionFormState;
pub use form_state::{
    ConnectionRouteTarget, NewConnectionField, NewConnectionForm, NewConnectionProxyHop,
    NewConnectionSelect, SavedConnectionPromptAction, SshAuthTab,
};
pub use launch::{SshConnectionIntent, SshConnectionWorkerResult};
pub use prompts::{HostKeyChallenge, KeyboardInteractiveChallenge};
