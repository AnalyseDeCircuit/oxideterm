// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Session-manager BLE actions: saved-profile open/delete entries and the
//! shared trace-card pipeline that reports resolver milestones on the
//! connection overlay.

use super::*;
use crate::workspace::new_connection::{
    BleConnectionWorkerResult, BleOpenRequest, BleStandaloneTarget, BleTraceTarget,
    ConnectionFormState,
};
use crate::workspace::standalone_connections::{
    StandaloneConnectionKind, StandaloneConnectionLaunch, StandaloneConnectionSurface,
};

impl WorkspaceApp {
    pub(super) fn request_delete_ble_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(profile) = self
            .connection_store
            .ble_profiles()
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
        else {
            return;
        };
        self.session_manager.update(cx, |session_manager, cx| {
            close_session_menu_state(session_manager);
            session_manager.delete_confirm = Some(SessionManagerDeleteConfirm::BleProfile {
                id: profile.id,
                name: profile.name,
            });
            cx.notify();
        });
    }

    pub(super) fn delete_ble_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        let (status, changed) = match self.connection_store.delete_ble_profile(id) {
            Ok(true) => (self.i18n.t("sessionManager.ble_profiles.delete"), true),
            Ok(false) => (
                self.i18n.t("sessionManager.ble_profiles.delete_failed"),
                false,
            ),
            Err(error) => (
                format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.ble_profiles.delete_failed")
                ),
                false,
            ),
        };
        self.session_manager.update(cx, |session_manager, cx| {
            if changed {
                session_manager
                    .selected_items
                    .remove(&SessionManagerSelectionTarget::Ble(id.to_string()));
            }
            session_manager.set_status(Some(status), cx)
        });
        if changed {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }

    pub(super) fn open_saved_ble_profile(
        &mut self,
        id: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self
            .connection_store
            .ble_profiles()
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
        else {
            return;
        };
        self.start_ble_open_flow(
            BleOpenRequest {
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
                service_uuid: profile.service_uuid.clone(),
                title: format!("BLE {}", profile.name),
                terminal_options: profile.terminal.clone(),
                runtime_options:
                    crate::workspace::new_connection::terminal_serial_runtime_options_from_ble_profile(
                        &profile,
                    ),
                mtu: profile.mtu,
                write_without_response: profile.write_without_response,
                profile_id: Some(profile.id),
                form_id: None,
                standalone: BleStandaloneTarget::New,
            },
            cx,
        );
    }

    /// Shared BLE open pipeline. Saved profiles and the connection form both
    /// enter here so the right-corner trace card reports scan, match and
    /// service verification instead of only the modal's inline state.
    pub(in crate::workspace) fn start_ble_open_flow(
        &mut self,
        request: BleOpenRequest,
        cx: &mut Context<Self>,
    ) {
        // The trace card is keyed by the same attempt identity that owns the
        // active-session record, so a reconnect reuses the existing row and a
        // fresh open registers exactly one.
        let attempt_id = match request.standalone {
            BleStandaloneTarget::Reconnect { attempt_id } => attempt_id,
            BleStandaloneTarget::New => {
                let clues = request.clues();
                let launch = match request.profile_id.as_ref() {
                    Some(profile_id) => StandaloneConnectionLaunch::SavedBle {
                        profile_id: profile_id.clone(),
                        clues,
                        terminal_options: request.terminal_options.clone(),
                    },
                    None => StandaloneConnectionLaunch::Ble {
                        clues,
                        terminal_options: request.terminal_options.clone(),
                    },
                };
                // Registered before the scan so the active-session row reports
                // Connecting while the resolver is still matching the device.
                self.standalone_connections.insert_pending(
                    StandaloneConnectionKind::Ble,
                    request.target.label.clone(),
                    launch,
                )
            }
        };
        self.push_ble_trace_event(
            &request.target,
            &attempt_id,
            oxideterm_terminal::BleResolveStage::OpeningAdapter,
            ConnectionTraceStatus::Running,
            None,
            cx,
        );

        let BleOpenRequest {
            target,
            adapter_guard,
            selected_device,
            adapter_mac,
            device_mac,
            device_name,
            service_uuid,
            title,
            terminal_options,
            runtime_options,
            mtu,
            write_without_response,
            profile_id,
            form_id,
            standalone: _,
        } = request;
        let tx = self.ble_worker_sender(cx);
        let cancellation = tokio_util::sync::CancellationToken::new();
        if let Some(form_id) = form_id {
            self.update_connection_form_state(cx, |state| {
                if let Some(form) = state
                    .form
                    .as_mut()
                    .filter(|form| form.ble.instance_id == form_id)
                {
                    form.ble.open_cancel = Some(cancellation.clone().drop_guard());
                }
            });
        }
        let cancel_worker = cancellation.clone();
        let owned_attempt = attempt_id.clone();

        // Live selections keep their peripheral handle; saved profiles resolve
        // persisted clues afresh. The long-lived runtime keeps the adapter's
        // platform tasks alive through the handoff to the terminal session.
        let runtime = self.forwarding_runtime.clone();
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
                &service_uuid,
                runtime_options,
                mtu,
                write_without_response,
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
                Ok((config, guard)) => {
                    let _ = tx.send(BleConnectionWorkerResult::ScanComplete {
                        config,
                        adapter_guard: guard,
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
        self.standalone_connections.set_ble_pending(
            &owned_attempt,
            crate::workspace::new_connection::BlePendingOpen {
                cancellation,
                _worker: worker,
            },
        );
    }

    pub(in crate::workspace) fn handle_ble_worker_result(
        &mut self,
        result: BleConnectionWorkerResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let attempt = match &result {
            BleConnectionWorkerResult::StageChanged { attempt_id, .. }
            | BleConnectionWorkerResult::ScanComplete { attempt_id, .. }
            | BleConnectionWorkerResult::ScanFailed { attempt_id, .. } => attempt_id,
        };
        if !self.standalone_connections.is_connecting_attempt(attempt) {
            return;
        }
        match result {
            BleConnectionWorkerResult::StageChanged {
                stage,
                attempt_id,
                target,
            } => {
                self.push_ble_trace_event(
                    &target,
                    &attempt_id,
                    stage,
                    ConnectionTraceStatus::Running,
                    None,
                    cx,
                );
            }
            BleConnectionWorkerResult::ScanComplete {
                config,
                adapter_guard,
                terminal_options,
                title,
                attempt_id,
                target,
                profile_id,
                form_id,
            } => {
                if form_id.is_some() && !self.ble_form_is_current(form_id, cx) {
                    self.standalone_connections.cancel_attempt(&attempt_id);
                    return;
                }
                let created = self.create_ble_terminal_tab(
                    config,
                    Some(adapter_guard),
                    terminal_options,
                    title,
                    window,
                    cx,
                );
                match created {
                    Ok(session_id) => {
                        // The active-session row now observes this terminal; a
                        // reconnect replaces the released surface because the
                        // record still owns the attempt id.
                        if !self.standalone_connections.bind_surface_for_attempt(
                            &attempt_id,
                            StandaloneConnectionSurface::Terminal(session_id),
                        ) {
                            self.close_terminal_session(session_id, window, cx);
                            return;
                        }
                        if let Some(profile_id) = profile_id.as_deref() {
                            self.register_terminal_saved_connection(
                                session_id,
                                oxideterm_terminal_triggers::SavedConnectionKind::Ble,
                                profile_id.to_string(),
                                cx,
                            );
                            let _ = self.connection_store.mark_ble_profile_used(profile_id);
                            self.queue_cloud_sync_dirty_refresh(cx);
                        }
                        if self.ble_form_is_current(form_id, cx) {
                            self.update_connection_form_state(cx, ConnectionFormState::clear);
                        }
                        // A ready terminal is its own confirmation; the overlay
                        // card is removed by the same delivered event.
                        self.push_ble_trace_event(
                            &target,
                            &attempt_id,
                            oxideterm_terminal::BleResolveStage::Ready,
                            ConnectionTraceStatus::Ready,
                            None,
                            cx,
                        );
                    }
                    Err(error) => {
                        let detail = error.to_string();
                        self.standalone_connections.mark_attempt_error(&attempt_id);
                        if self.ble_form_is_current(form_id, cx) {
                            self.fail_ble_form_open(&detail, cx);
                        }
                        self.push_ble_trace_event(
                            &target,
                            &attempt_id,
                            oxideterm_terminal::BleResolveStage::Ready,
                            ConnectionTraceStatus::Failed,
                            Some(detail),
                            cx,
                        );
                    }
                }
            }
            BleConnectionWorkerResult::ScanFailed {
                stage,
                error,
                attempt_id,
                target,
                profile_id,
                form_id,
            } => {
                if matches!(error, oxideterm_terminal::BleResolveError::Cancelled) {
                    self.standalone_connections.cancel_attempt(&attempt_id);
                    return;
                }
                tracing::warn!(
                    category = "ble.open",
                    profile_id = profile_id.as_deref().unwrap_or(""),
                    "BLE open failed"
                );
                let detail = self.localize_ble_resolve_error(&error);
                // The record keeps its failure visible in the active-session
                // list, which is where the reconnect action lives.
                self.standalone_connections.mark_attempt_error(&attempt_id);
                if self.ble_form_is_current(form_id, cx) {
                    self.fail_ble_form_open(&detail, cx);
                }
                // The failure card stays visible until dismissed or retried.
                self.push_ble_trace_event(
                    &target,
                    &attempt_id,
                    stage,
                    ConnectionTraceStatus::Failed,
                    Some(detail),
                    cx,
                );
            }
        }
    }

    /// Maps a resolver failure to the current locale's sentence. The raw
    /// underlying error text stays as reported by btleplug inside the
    /// `{error}` slot.
    fn localize_ble_resolve_error(&self, error: &oxideterm_terminal::BleResolveError) -> String {
        use oxideterm_terminal::BleResolveError;
        match error {
            BleResolveError::Cancelled => self.i18n.t("connections.ble.resolve.cancelled"),
            BleResolveError::AdapterEnumeration(error) => self.i18n.t_with(
                "connections.ble.resolve.adapter_enumeration_failed",
                &[("error", error)],
            ),
            BleResolveError::NoAdapters => self.i18n.t("connections.ble.resolve.no_adapters"),
            BleResolveError::AdapterOpen(error) => self.i18n.t_with(
                "connections.ble.resolve.adapter_open_failed",
                &[("error", error)],
            ),
            BleResolveError::DeviceEnumeration(error) => self.i18n.t_with(
                "connections.ble.resolve.device_enumeration_failed",
                &[("error", error)],
            ),
            BleResolveError::Scan(error) => self
                .i18n
                .t_with("connections.ble.resolve.scan_failed", &[("error", error)]),
            BleResolveError::NoDeviceMatched { identity } => self.i18n.t_with(
                "connections.ble.resolve.no_device_matched",
                &[("identity", identity)],
            ),
            BleResolveError::AmbiguousDevice { identity } => self.i18n.t_with(
                "connections.ble.resolve.ambiguous_device",
                &[("identity", identity)],
            ),
            BleResolveError::Connect(error) => self.i18n.t_with(
                "connections.ble.resolve.connect_failed",
                &[("error", error)],
            ),
            BleResolveError::ServiceDiscovery(error) => self.i18n.t_with(
                "connections.ble.resolve.service_discovery_failed",
                &[("error", error)],
            ),
            BleResolveError::ServiceMismatch { service_uuid } => self.i18n.t_with(
                "connections.ble.resolve.service_mismatch",
                &[("uuid", service_uuid)],
            ),
            BleResolveError::InvalidServiceUuid { service_uuid } => self.i18n.t_with(
                "connections.ble.resolve.invalid_service_uuid",
                &[("uuid", service_uuid)],
            ),
        }
    }

    fn ble_form_is_current(&mut self, form_id: Option<uuid::Uuid>, cx: &mut Context<Self>) -> bool {
        self.update_connection_form_state(cx, |state| {
            state
                .form
                .as_ref()
                .is_some_and(|form| Some(form.ble.instance_id) == form_id)
        })
    }

    fn fail_ble_form_open(&mut self, detail: &str, cx: &mut Context<Self>) {
        let message = format!(
            "{}: {detail}",
            self.i18n.t("modals.new_connection.ble_open_failed")
        );
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.pending = false;
                form.error = Some(message);
            }
        });
    }

    fn push_ble_trace_event(
        &self,
        target: &BleTraceTarget,
        attempt_id: &str,
        stage: oxideterm_terminal::BleResolveStage,
        status: ConnectionTraceStatus,
        detail: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let stage = match stage {
            oxideterm_terminal::BleResolveStage::OpeningAdapter => {
                ConnectionTraceStage::BleOpeningAdapter
            }
            oxideterm_terminal::BleResolveStage::Scanning => ConnectionTraceStage::BleScanning,
            oxideterm_terminal::BleResolveStage::Matching => ConnectionTraceStage::BleMatching,
            oxideterm_terminal::BleResolveStage::VerifyingService => {
                ConnectionTraceStage::BleVerifyingService
            }
            oxideterm_terminal::BleResolveStage::Ready => ConnectionTraceStage::BleReady,
        };
        let event = ConnectionTraceEvent {
            attempt_id: attempt_id.to_string(),
            node_id: NodeId(target.node_id.clone()),
            stage,
            status,
            progress: 0.0,
            elapsed_ms: 0,
            detail,
            label: Some(target.label.clone()),
            endpoint: target.endpoint.clone(),
            step_index: None,
            total_steps: None,
            mode: ConnectionTraceMode::Connect,
        };
        self.apply_workspace_overlay_intent(
            WorkspaceOverlayIntent::ConnectionTraceEvents(vec![event]),
            cx,
        );
    }
}
