// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Session-manager BLE actions: saved-profile open/delete entries and the
//! shared trace-card pipeline that reports resolver milestones on the
//! connection overlay.

use super::*;
use crate::workspace::new_connection::{
    BleConnectionWorkerResult, BleOpenRequest, BleStandaloneTarget, BleTraceTarget,
};
use crate::workspace::standalone_connections::{
    StandaloneConnectionKind, StandaloneConnectionLaunch, StandaloneConnectionSurface,
};

impl WorkspaceApp {
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
        // Revive the profile's released row when one exists: reopening an
        // already-known device from the session manager should reuse its row
        // instead of stacking a second one in the active list.
        let standalone = self
            .standalone_connections
            .released_saved_ble_record(&profile.id)
            .and_then(|record_id| {
                self.begin_standalone_reconnect(&record_id, cx)
                    .map(|attempt_id| BleStandaloneTarget::Reconnect { attempt_id })
            });
        self.start_ble_open_flow(
            BleOpenRequest::from_saved_profile(
                profile,
                standalone.unwrap_or(BleStandaloneTarget::New),
            ),
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
        let attempt_id = match &request.standalone {
            BleStandaloneTarget::Reconnect { attempt_id } => attempt_id.clone(),
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
        // Standalone records are plain workspace state: every mutation needs
        // an explicit notify or the sidebar keeps showing the pre-open list.
        cx.notify();
        // A saved-profile reopen of a device this process already opened must
        // not reach the radio: the row stays in the active list marked failed
        // (like the serial port reservation, which also runs after the row
        // exists) and the localized reason goes to the status line only, with
        // no trace card. The async DeviceBusy claim still guards the form
        // path and same-tick races.
        if request.profile_id.is_some() {
            if let Some(device) = oxideterm_terminal::ble_device_reservation_label(
                request.device_mac.as_deref(),
                request.platform_identifier.as_deref(),
            ) {
                let status = self.i18n.t_with(
                    "connections.ble.resolve.device_busy",
                    &[("device", &device)],
                );
                self.standalone_connections.mark_attempt_error(&attempt_id);
                self.connections
                    .session_manager
                    .update(cx, |session_manager, cx| {
                        session_manager.set_status(Some(status), cx)
                    });
                return;
            }
        }
        self.push_ble_trace_event(
            &request.target,
            &attempt_id,
            oxideterm_terminal::BleResolveStage::OpeningAdapter,
            ConnectionTraceStatus::Running,
            None,
            cx,
        );

        let pending = self.connections.flow.update(cx, |flow, cx| {
            flow.start_ble_resolver(request, attempt_id.clone(), &self.forwarding_runtime, cx)
        });
        self.standalone_connections
            .set_ble_pending(&attempt_id, pending);
    }

    /// Closes a resolver link that no session will adopt, on the shared
    /// runtime: the link is still open on the radio and has no other owner.
    fn abandon_ble_resolved_link(
        &self,
        resolved_link: Option<oxideterm_terminal::BleResolvedLink>,
    ) {
        if let Some(link) = resolved_link {
            self.forwarding_runtime.clone().spawn(async move {
                link.close().await;
            });
        }
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
            // The attempt is already gone; a verified link would otherwise be
            // dropped while still open on the radio.
            if let BleConnectionWorkerResult::ScanComplete { resolved_link, .. } = result {
                self.abandon_ble_resolved_link(resolved_link);
            }
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
                resolved_link,
                terminal_options,
                title,
                attempt_id,
                target,
                profile_id,
                form_id,
            } => {
                if form_id.is_some() && !self.connections.flow.read(cx).is_ble_form_current(form_id)
                {
                    self.abandon_ble_resolved_link(resolved_link);
                    self.standalone_connections.cancel_attempt(&attempt_id);
                    self.sync_session_retention(cx);
                    return;
                }
                let created = self.create_ble_terminal_tab(
                    config,
                    Some(adapter_guard),
                    resolved_link,
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
                        self.connections.flow.update(cx, |flow, cx| {
                            flow.finish_ble_form_open(form_id, None, &self.i18n, cx);
                        });
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
                        self.connections.flow.update(cx, |flow, cx| {
                            flow.finish_ble_form_open(form_id, Some(&detail), &self.i18n, cx);
                        });
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
                    self.sync_session_retention(cx);
                    return;
                }
                tracing::warn!(
                    category = "ble.open",
                    profile_id = profile_id.as_deref().unwrap_or(""),
                    "BLE open failed"
                );
                let detail =
                    oxideterm_gpui_connections::ble::localize_ble_resolve_error(&self.i18n, &error);
                // The record keeps its failure visible in the active-session
                // list, which is where the reconnect action lives.
                self.standalone_connections.mark_attempt_error(&attempt_id);
                self.connections.flow.update(cx, |flow, cx| {
                    flow.finish_ble_form_open(form_id, Some(&detail), &self.i18n, cx);
                });
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
            oxideterm_terminal::BleResolveStage::DirectConnect => {
                ConnectionTraceStage::BleDirectConnect
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
