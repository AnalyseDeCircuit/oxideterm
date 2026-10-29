// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! BLE form submit path, the shared open-request types consumed by the
//! session-manager resolver pipeline, and the saved-profile-to-form
//! conversion. Everything transport-specific to BLE in the new-connection
//! flow lives here; other transports keep their own submit implementations.

use gpui::{Context, Window};
use oxideterm_ble::BleAdapterGuard;
use oxideterm_connections::{BleProfile, ConnectionTerminalOptions, SaveBleProfileRequest};
use oxideterm_terminal::BleSessionConfig;

use super::ConnectionFormState;
use super::form_state::{
    NewConnectionField, NewConnectionForm, NewConnectionSubmitAction, NewConnectionTransport,
    terminal_serial_runtime_options_from_ble_form,
};
use super::ssh_flow::{
    asset_color_from_form, asset_icon_from_form, saved_profile_notes,
    serial_profile_group_from_form,
};
use crate::workspace::WorkspaceApp;
use crate::workspace::standalone_connections::BleConnectionClues;

/// Card identity reported by every BLE open attempt. Carrying it in the worker
/// result lets unsaved form opens render the same trace card as saved profiles
/// without a connection-store lookup that may disappear mid-scan.
#[derive(Clone, Debug)]
pub(in crate::workspace) struct BleTraceTarget {
    /// Node whose card this attempt owns; a retry for the same node supersedes it.
    pub node_id: String,
    pub label: String,
    pub endpoint: Option<String>,
}

pub(in crate::workspace) struct BlePendingOpen {
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
pub(in crate::workspace) enum BleStandaloneTarget {
    /// This open registers a new record before the resolver starts scanning.
    New,
    /// A reconnect reuses the record that already owns the attempt id, so the
    /// new surface replaces the released one instead of adding a second row.
    Reconnect { attempt_id: String },
}

/// Inputs for the shared BLE open pipeline used by saved profiles and the
/// connection form.
pub(in crate::workspace) struct BleOpenRequest {
    pub target: BleTraceTarget,
    /// Adapter that produced the caller's scan, when one is already open. The
    /// resolver reuses its still-running discovery window instead of starting
    /// a second scan, which BlueZ would reject.
    pub adapter_guard: Option<BleAdapterGuard>,
    pub selected_device: Option<oxideterm_ble::BlePeripheralId>,
    pub adapter_mac: Option<String>,
    pub device_mac: Option<String>,
    pub device_name: Option<String>,
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
    pub(in crate::workspace) fn clues(&self) -> BleConnectionClues {
        BleConnectionClues {
            adapter_mac: self.adapter_mac.clone(),
            device_mac: self.device_mac.clone(),
            device_name: self.device_name.clone(),
            service_uuid: self.service_uuid.clone(),
        }
    }
}

/// Results produced by background BLE workers that scan, match devices and
/// verify advertised services for saved-profile and form opens.
pub(in crate::workspace) enum BleConnectionWorkerResult {
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

/// Identity captured from the BLE form when the user starts an open. The
/// resolver keeps the live peripheral identity; persisted clues are used only
/// when opening saved profiles without a current scan selection.
struct PendingBleOpen {
    form_id: uuid::Uuid,
    adapter_guard: Option<BleAdapterGuard>,
    selected_device: Option<oxideterm_ble::BlePeripheralId>,
    adapter_mac: Option<String>,
    device_mac: Option<String>,
    device_name: Option<String>,
    service_uuid: String,
    label: String,
    title: String,
    terminal_options: ConnectionTerminalOptions,
    runtime_options: oxideterm_terminal::SerialRuntimeOptions,
    mtu: Option<u16>,
    write_without_response: bool,
}

impl WorkspaceApp {
    pub(super) fn submit_ble_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((open, save_request)) = self.with_connection_form_mut(cx, |this, form, cx| {
            let form = form?;
            let editing_id = form.ble.profile_id.clone();
            let existing = editing_id.as_deref().and_then(|id| {
                this.connection_store
                    .ble_profiles()
                    .iter()
                    .find(|profile| profile.id == id)
                    .cloned()
            });
            // The open button is gated on a selected service that carries
            // both notify and write characteristics; re-check the selection
            // here because stale scan state can outlive the gating pass.
            // Saving an existing profile keeps working without a live scan.
            let selection = form
                .ble
                .selected_device_id
                .clone()
                .zip(form.ble.selected_service_uuid);
            if selection.is_none()
                && (action != NewConnectionSubmitAction::Save || existing.is_none())
            {
                form.error = Some(this.i18n.t("modals.new_connection.ble_open_failed"));
                cx.notify();
                return None;
            }
            let (device_id, service_uuid) = selection.unzip();
            let device = device_id.as_ref().and_then(|device_id| {
                form.ble
                    .devices
                    .iter()
                    .find(|device| &device.id == device_id)
                    .cloned()
            });
            let device_label = device
                .as_ref()
                .map(|device| {
                    device
                        .name
                        .clone()
                        .unwrap_or_else(|| device.address.clone())
                })
                .unwrap_or_default();
            // A live selection is the authority; otherwise the stored identity
            // keeps an edit saveable while the device is out of scan range.
            let (adapter_mac, device_mac, device_name, service_uuid_text) =
                if let Some(device) = &device {
                    (
                        form.ble
                            .adapters
                            .get(form.ble.selected_adapter_index)
                            .and_then(|adapter| adapter.adapter_mac.clone()),
                        device.persistent_address().map(ToOwned::to_owned),
                        device.name.clone(),
                        service_uuid
                            .expect("selection paired a service")
                            .to_string(),
                    )
                } else {
                    match &existing {
                        Some(profile) => (
                            profile.adapter_mac.clone(),
                            profile.device_mac.clone(),
                            profile.device_name.clone(),
                            profile.service_uuid.clone(),
                        ),
                        None => (None, None, None, String::new()),
                    }
                };
            let profile_name = {
                let name = form.ble.profile_name.trim();
                if name.is_empty() {
                    device_label
                } else {
                    name.to_string()
                }
            };
            // MTU is a text input in the form; an empty draft means "platform
            // default" and anything else must parse inside the BLE spec range.
            let mtu_text = form.ble.mtu.trim();
            let mtu = if mtu_text.is_empty() {
                None
            } else {
                match mtu_text.parse::<u16>() {
                    Ok(value)
                        if (oxideterm_connections::BLE_ATT_MTU_MIN
                            ..=oxideterm_connections::BLE_ATT_MTU_MAX)
                            .contains(&value) =>
                    {
                        Some(value)
                    }
                    _ => {
                        form.error = Some(this.i18n.t("modals.new_connection.ble_mtu_invalid"));
                        cx.notify();
                        return None;
                    }
                }
            };
            let should_save = editing_id.is_some() || action != NewConnectionSubmitAction::Connect;
            let save_request = should_save.then(|| SaveBleProfileRequest {
                id: editing_id.clone(),
                name: profile_name.clone(),
                group: serial_profile_group_from_form(&form.group, &this.i18n),
                notes: saved_profile_notes(&form.notes),
                icon: asset_icon_from_form(&form.icon),
                color: asset_color_from_form(&form.color),
                icon_background_color: asset_color_from_form(&form.icon_background_color),
                adapter_mac: adapter_mac.clone(),
                device_mac: device_mac.clone(),
                device_name: device_name.clone(),
                service_uuid: service_uuid_text.clone(),
                terminal: form.terminal.clone(),
                input_line_ending: Some(form.ble.input_line_ending),
                output_line_ending: Some(form.ble.output_line_ending),
                send_mode: Some(form.ble.send_mode),
                display_mode: Some(form.ble.display_mode),
                local_echo: Some(form.ble.local_echo),
                mtu: Some(mtu),
                write_without_response: Some(form.ble.write_without_response),
                connect_on_open: existing.as_ref().map(|profile| profile.connect_on_open),
            });
            let open = PendingBleOpen {
                form_id: form.ble.instance_id,
                // The adapter that produced the user's selection is handed
                // to the resolver so the discovered peripheral stays
                // addressable while the card reports the short re-check.
                adapter_guard: form.ble.adapter_guard.take(),
                selected_device: device_id,
                adapter_mac,
                device_mac,
                device_name,
                service_uuid: service_uuid_text,
                title: format!("BLE {}", profile_name),
                label: profile_name,
                terminal_options: form.terminal.clone(),
                runtime_options: terminal_serial_runtime_options_from_ble_form(&form),
                mtu,
                write_without_response: form.ble.write_without_response,
            };
            form.pending = true;
            form.error = None;
            Some((open, save_request))
        }) else {
            return;
        };

        if action == NewConnectionSubmitAction::Save {
            let request = save_request.expect("ble save action must build a ble profile request");
            match self.connection_store.upsert_ble_profile(request) {
                Ok(_) => {
                    self.queue_cloud_sync_dirty_refresh(cx);
                    self.update_connection_form_state(cx, ConnectionFormState::clear);
                }
                Err(error) => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.pending = false;
                            form.error = Some(format!(
                                "{}: {error}",
                                self.i18n.t("modals.new_connection.ble_save_failed")
                            ));
                        }
                    });
                }
            }
            cx.notify();
            return;
        }

        // SaveAndConnect persists the profile before opening so the session
        // manager reflects the device that is about to connect.
        let mut profile_id = None;
        if action == NewConnectionSubmitAction::SaveAndConnect {
            let request =
                save_request.expect("ble save-and-open action must build a ble profile request");
            match self.connection_store.upsert_ble_profile(request) {
                Ok(saved) => {
                    profile_id = Some(saved.id);
                    self.queue_cloud_sync_dirty_refresh(cx);
                }
                Err(error) => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.pending = false;
                            form.error = Some(format!(
                                "{}: {error}",
                                self.i18n.t("modals.new_connection.ble_save_failed")
                            ));
                        }
                    });
                    cx.notify();
                    return;
                }
            }
        }

        // Open through the shared trace pipeline so the right-corner card shows
        // the same stages as saved-profile opens. The handed-over guard keeps
        // its discovery window open, so the resolver reuses the cached
        // peripherals instead of starting a second scan BlueZ would reject.
        let node_id = format!(
            "ble-form:{}",
            open.device_mac
                .as_deref()
                .or(open.device_name.as_deref())
                .unwrap_or("")
        );
        self.start_ble_open_flow(
            BleOpenRequest {
                target: BleTraceTarget {
                    node_id,
                    label: open.label,
                    endpoint: open.device_name.clone().or_else(|| open.device_mac.clone()),
                },
                adapter_guard: open.adapter_guard,
                selected_device: open.selected_device,
                adapter_mac: open.adapter_mac,
                device_mac: open.device_mac,
                device_name: open.device_name,
                service_uuid: open.service_uuid,
                title: open.title,
                terminal_options: open.terminal_options,
                runtime_options: open.runtime_options,
                mtu: open.mtu,
                write_without_response: open.write_without_response,
                profile_id,
                form_id: Some(open.form_id),
                standalone: BleStandaloneTarget::New,
            },
            cx,
        );
        cx.notify();
    }
}

pub(in crate::workspace) fn form_from_ble_profile(
    profile: &BleProfile,
    ungrouped_label: String,
) -> NewConnectionForm {
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
