// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Notification Center — Phase 1
//!
//! In-memory notification model with Tauri event emission.
//! Phase 2 will add redb persistence for unresolved/security notifications.

mod model;

pub use model::*;

use tauri::{AppHandle, Emitter};
use tracing::warn;

/// Emit a notification to the frontend via Tauri event.
pub fn emit_notification(handle: &AppHandle, record: &NotificationRecord) {
    if let Err(e) = handle.emit("notification:push", record) {
        warn!("Failed to emit notification: {}", e);
    }
}
