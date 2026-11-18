// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! BLE discovery and delivery data shared by the form and the session runtime.

use oxideterm_ble::BleAdapterGuard;
use oxideterm_connections::{BleProfile, ConnectionTerminalOptions};
use oxideterm_terminal::BleSessionConfig;

use crate::form_state::{NewConnectionField, NewConnectionForm, NewConnectionTransport};

/// Device identity carried by a BLE session record.
///
/// A BLE peripheral handle is assigned by the OS at scan time and cannot be
/// restored, so the record keeps the discovery clues the resolver matches
/// against instead of the runtime id.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BleConnectionClues {
    pub adapter_mac: Option<String>,
    pub device_mac: Option<String>,
    pub device_name: Option<String>,
    /// Platform-local identity for the cache-based direct connect; sync and
    /// restarts carry it as host-local metadata.
    pub platform_identifier: Option<String>,
    pub service_uuid: String,
}

/// Card identity reported by every BLE open attempt. Carrying it in the worker
/// result lets unsaved form opens render the same trace card as saved profiles
/// without a connection-store lookup that may disappear mid-scan.
#[derive(Clone, Debug)]
pub struct BleTraceTarget {
    /// Node whose card this attempt owns; a retry for the same node supersedes it.
    pub node_id: String,
    pub label: String,
    pub endpoint: Option<String>,
}

pub struct BlePendingOpen {
    pub cancellation: tokio_util::sync::CancellationToken,
    pub _worker: tokio::task::JoinHandle<()>,
}

impl Drop for BlePendingOpen {
    fn drop(&mut self) {
        // The resolver observes cancellation and finishes link cleanup on the shared runtime.
        self.cancellation.cancel();
    }
}

/// How an open relates to the active-session record that represents it.
pub enum BleStandaloneTarget {
    /// This open registers a new record before the resolver starts scanning.
    New,
    /// A reconnect reuses the record that already owns the attempt id, so the
    /// new surface replaces the released one instead of adding a second row.
    Reconnect { attempt_id: String },
}

/// Inputs for the shared BLE open pipeline used by saved profiles and the
/// connection form.
pub struct BleOpenRequest {
    pub target: BleTraceTarget,
    /// Adapter that produced the caller's scan, when one is already open. The
    /// resolver reuses its still-running discovery window instead of starting
    /// a second scan, which BlueZ would reject.
    pub adapter_guard: Option<BleAdapterGuard>,
    pub selected_device: Option<oxideterm_ble::BlePeripheralId>,
    pub adapter_mac: Option<String>,
    pub device_mac: Option<String>,
    pub device_name: Option<String>,
    /// Platform-local identity for the cache-based direct connect; `None`
    /// falls back to the scan window.
    pub platform_identifier: Option<String>,
    pub service_uuid: String,
    pub title: String,
    pub terminal_options: ConnectionTerminalOptions,
    /// Byte-stream handling options sourced from the saved profile.
    /// Defaults apply for ephemeral (non-saved) opens.
    pub runtime_options: oxideterm_terminal::SerialRuntimeOptions,
    /// Preferred ATT MTU from the profile; `None` keeps platform negotiation.
    pub mtu: Option<u16>,
    /// Prefer the write-without-response characteristic; `false` uses the
    /// acknowledged WRITE characteristic when the device exposes one.
    pub write_without_response: bool,
    pub profile_id: Option<String>,
    /// The open started from the connection form, so completion and failure
    /// must settle its pending state instead of only updating the card.
    pub form_id: Option<uuid::Uuid>,
    pub standalone: BleStandaloneTarget,
}

impl BleOpenRequest {
    /// Discovery clues the active-session record keeps for future reconnects.
    pub fn clues(&self) -> BleConnectionClues {
        BleConnectionClues {
            adapter_mac: self.adapter_mac.clone(),
            device_mac: self.device_mac.clone(),
            device_name: self.device_name.clone(),
            platform_identifier: self.platform_identifier.clone(),
            service_uuid: self.service_uuid.clone(),
        }
    }
}

/// Results produced by background BLE workers that scan, match devices and
/// verify advertised services for saved-profile and form opens.
pub enum BleConnectionWorkerResult {
    /// Resolver crossed a milestone; the overlay card advances its stage.
    StageChanged {
        stage: oxideterm_terminal::BleResolveStage,
        attempt_id: String,
        target: BleTraceTarget,
    },
    /// Resolver found a live device; the terminal tab can be created.
    ScanComplete {
        config: BleSessionConfig,
        adapter_guard: BleAdapterGuard,
        /// Verified link opened by the resolver's service check; the session
        /// worker adopts it instead of reconnecting.
        resolved_link: Option<oxideterm_terminal::BleResolvedLink>,
        terminal_options: ConnectionTerminalOptions,
        title: String,
        attempt_id: String,
        target: BleTraceTarget,
        profile_id: Option<String>,
        form_id: Option<uuid::Uuid>,
    },
    /// Scan failed: device not in range, service missing, or adapter error.
    ScanFailed {
        stage: oxideterm_terminal::BleResolveStage,
        error: oxideterm_terminal::BleResolveError,
        attempt_id: String,
        target: BleTraceTarget,
        profile_id: Option<String>,
        form_id: Option<uuid::Uuid>,
    },
}

pub fn form_from_ble_profile(profile: &BleProfile, ungrouped_label: String) -> NewConnectionForm {
    // The edit form owns only persisted settings; the stored device identity is
    // reused on save when no live scan selection is made.
    let mut form = NewConnectionForm::default();
    form.transport = NewConnectionTransport::Ble;
    form.ble.profile_id = Some(profile.id.clone());
    form.ble.profile_name = profile.name.clone();
    form.group = profile.group.clone().unwrap_or(ungrouped_label);
    form.notes = profile.notes.clone().unwrap_or_default();
    form.icon = profile.icon.clone().unwrap_or_default();
    form.color = profile.color.clone().unwrap_or_default();
    form.icon_background_color = profile.icon_background_color.clone().unwrap_or_default();
    form.ble.selected_service_uuid = profile.service_uuid.parse().ok();
    form.terminal = profile.terminal.clone();
    form.ble.input_line_ending = profile.input_line_ending;
    form.ble.output_line_ending = profile.output_line_ending;
    form.ble.send_mode = profile.send_mode;
    form.ble.display_mode = profile.display_mode;
    form.ble.local_echo = profile.local_echo;
    form.ble.mtu = profile.mtu.map(|mtu| mtu.to_string()).unwrap_or_default();
    form.ble.write_without_response = profile.write_without_response;
    form.focused_field = NewConnectionField::BleProfileName;
    form
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ble_profile_edit_preserves_acknowledged_writes_and_mtu_cap() {
        let mut profile = BleProfile::new("sensor", "6e400001-b5a3-f393-e0a9-e50e24dcca9e");
        profile.write_without_response = false;
        for (mtu, expected) in [(None, ""), (Some(23), "23")] {
            profile.mtu = mtu;
            let form = form_from_ble_profile(&profile, "Ungrouped".into());
            assert!(!form.ble.write_without_response);
            assert_eq!(form.ble.mtu, expected);
        }
    }
}
