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
    /// Local gate retained for reconnects that never reach a saved profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_connect_command: Option<String>,
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
    /// Local gate that must succeed before any BLE adapter work starts.
    pub pre_connect_command: Option<String>,
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
            pre_connect_command: self.pre_connect_command.clone(),
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

impl BleOpenRequest {
    pub fn from_saved_profile(profile: BleProfile, standalone: BleStandaloneTarget) -> Self {
        Self {
            target: BleTraceTarget {
                node_id: profile.id.clone(),
                label: profile.name.clone(),
                endpoint: profile
                    .device_name
                    .clone()
                    .or_else(|| profile.device_mac.clone()),
            },
            adapter_guard: None,
            selected_device: None,
            adapter_mac: profile.adapter_mac.clone(),
            device_mac: profile.device_mac.clone(),
            device_name: profile.device_name.clone(),
            platform_identifier: profile.platform_identifier.clone(),
            service_uuid: profile.service_uuid.clone(),
            title: format!("BLE {}", profile.name),
            terminal_options: profile.terminal.clone(),
            runtime_options: crate::form_state::terminal_serial_runtime_options_from_ble_profile(
                &profile,
            ),
            mtu: profile.mtu,
            write_without_response: profile.write_without_response,
            pre_connect_command: profile.pre_connect_command.clone(),
            profile_id: Some(profile.id),
            form_id: None,
            standalone,
        }
    }
}

impl crate::entity::ConnectionFlowEntity {
    pub fn start_ble_resolver(
        &mut self,
        request: BleOpenRequest,
        attempt_id: String,
        runtime: &tokio::runtime::Runtime,
        cx: &mut gpui::Context<Self>,
    ) -> BlePendingOpen {
        let tx = self.ble_worker_sender();
        let BleOpenRequest {
            target,
            adapter_guard,
            selected_device,
            adapter_mac,
            device_mac,
            device_name,
            platform_identifier,
            service_uuid,
            title,
            terminal_options,
            runtime_options,
            mtu,
            write_without_response,
            pre_connect_command,
            profile_id,
            form_id,
            standalone: _,
        } = request;
        let cancellation = tokio_util::sync::CancellationToken::new();
        if let Some(form) = self
            .form
            .form
            .as_mut()
            .filter(|form| Some(form.ble.instance_id) == form_id)
        {
            form.ble.open_cancel = Some(cancellation.clone().drop_guard());
            cx.notify();
        }
        let cancel_worker = cancellation.clone();

        // Live selections keep their peripheral handle; saved profiles resolve
        // persisted clues afresh. The long-lived runtime keeps the adapter's
        // platform tasks alive through the handoff to the terminal session.
        let worker = runtime.spawn(async move {
            let mut last_stage = oxideterm_terminal::BleResolveStage::OpeningAdapter;
            let stage_tx = tx.clone();
            let stage_target = target.clone();
            let stage_attempt_id = attempt_id.clone();
            let result = oxideterm_terminal::ble_resolve_profile_to_session(
                adapter_guard,
                selected_device.as_ref(),
                &cancel_worker,
                adapter_mac.as_deref(),
                device_mac.as_deref(),
                device_name.as_deref(),
                platform_identifier.as_deref(),
                &service_uuid,
                runtime_options,
                mtu,
                write_without_response,
                pre_connect_command,
                |stage| {
                    last_stage = stage;
                    let _ = stage_tx.send(BleConnectionWorkerResult::StageChanged {
                        stage,
                        attempt_id: stage_attempt_id.clone(),
                        target: stage_target.clone(),
                    });
                },
            )
            .await;
            match result {
                Ok((config, guard, resolved_link)) => {
                    let _ = tx.send(BleConnectionWorkerResult::ScanComplete {
                        config,
                        adapter_guard: guard,
                        resolved_link,
                        terminal_options,
                        title,
                        attempt_id,
                        target,
                        profile_id,
                        form_id,
                    });
                }
                Err(error) => {
                    let _ = tx.send(BleConnectionWorkerResult::ScanFailed {
                        stage: last_stage,
                        error,
                        attempt_id,
                        target,
                        profile_id,
                        form_id,
                    });
                }
            }
        });
        BlePendingOpen {
            cancellation,
            _worker: worker,
        }
    }

    pub fn is_ble_form_current(&self, form_id: Option<uuid::Uuid>) -> bool {
        self.form
            .form
            .as_ref()
            .is_some_and(|form| Some(form.ble.instance_id) == form_id)
    }

    pub fn finish_ble_form_open(
        &mut self,
        form_id: Option<uuid::Uuid>,
        error: Option<&str>,
        i18n: &oxideterm_i18n::I18n,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.is_ble_form_current(form_id) {
            return;
        }
        if let Some(detail) = error {
            if let Some(form) = self.form.form.as_mut() {
                form.pending = false;
                form.error = Some(format!(
                    "{}: {detail}",
                    i18n.t("modals.new_connection.ble_open_failed")
                ));
            }
        } else {
            self.form.clear();
        }
        cx.notify();
    }
}

/// Maps a resolver failure to the current locale's sentence. The raw
/// underlying error text stays as reported by btleplug inside the
/// `{error}` slot.
pub fn localize_ble_resolve_error(
    i18n: &oxideterm_i18n::I18n,
    error: &oxideterm_terminal::BleResolveError,
) -> String {
    use oxideterm_terminal::BleResolveError;
    match error {
        BleResolveError::Cancelled => i18n.t("connections.ble.resolve.cancelled"),
        BleResolveError::NoAdapters => i18n.t("connections.ble.resolve.no_adapters"),
        BleResolveError::AdapterOpen(error) => i18n.t_with(
            "connections.ble.resolve.adapter_open_failed",
            &[("error", error)],
        ),
        BleResolveError::DeviceEnumeration(error) => i18n.t_with(
            "connections.ble.resolve.device_enumeration_failed",
            &[("error", error)],
        ),
        BleResolveError::Scan(error) => {
            i18n.t_with("connections.ble.resolve.scan_failed", &[("error", error)])
        }
        BleResolveError::NoDeviceMatched { identity } => i18n.t_with(
            "connections.ble.resolve.no_device_matched",
            &[("identity", identity)],
        ),
        BleResolveError::AmbiguousDevice { identity } => i18n.t_with(
            "connections.ble.resolve.ambiguous_device",
            &[("identity", identity)],
        ),
        BleResolveError::DeviceBusy { device } => {
            i18n.t_with("connections.ble.resolve.device_busy", &[("device", device)])
        }
        BleResolveError::Connect(error) => i18n.t_with(
            "connections.ble.resolve.connect_failed",
            &[("error", error)],
        ),
        BleResolveError::ServiceDiscovery(error) => i18n.t_with(
            "connections.ble.resolve.service_discovery_failed",
            &[("error", error)],
        ),
        BleResolveError::ServiceMismatch { service_uuid } => i18n.t_with(
            "connections.ble.resolve.service_mismatch",
            &[("uuid", service_uuid)],
        ),
        BleResolveError::DirectConnectUnavailable { identity, reason } => i18n.t_with(
            "connections.ble.resolve.direct_connect_failed",
            &[("identity", identity), ("reason", reason)],
        ),
        BleResolveError::InvalidServiceUuid { service_uuid } => i18n.t_with(
            "connections.ble.resolve.invalid_service_uuid",
            &[("uuid", service_uuid)],
        ),
        BleResolveError::PreConnectCommand(failure) => {
            oxideterm_terminal::pre_connect_failure_message(Some(i18n), *failure)
        }
    }
}
