use super::*;
use crate::workspace::ConfirmKeyboardAction;
use oxideterm_gpui_forwards::ForwardsInputKey;
use oxideterm_gpui_ui::motion::{self, MotionDuration};

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_forward_edit_modal_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(page) = self.forwarding_page(cx) else {
            return false;
        };
        if !page.read(cx).edit_form_open() {
            return false;
        }
        if self.handle_forwards_key(event, cx) {
            return true;
        }
        if event.keystroke.key.as_str() == "escape" {
            let delay = motion::duration(&self.tokens, MotionDuration::Overlay);
            page.update(cx, |page, cx| page.begin_edit_form_exit(delay, cx));
            self.ime_marked_text = None;
            return true;
        }
        false
    }

    pub(in crate::workspace) fn handle_forward_delete_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let forwarding = self.forwarding.read(cx);
        let Some(tab_id) = forwarding.page_id() else {
            return false;
        };
        let (Some(page), Some(node_id)) = (
            forwarding.page_state(tab_id),
            forwarding.node_for_tab(tab_id),
        ) else {
            return false;
        };
        if !page.read(cx).delete_confirm_open() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                page.update(cx, |page, cx| {
                    page.clear_pending_delete();
                    cx.notify();
                });
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                if let Some(forward_id) = page.update(cx, |page, cx| {
                    let id = page.take_delete();
                    cx.notify();
                    id
                }) {
                    self.start_forward_operation(
                        tab_id,
                        node_id,
                        "forwards.messages.deleted",
                        true,
                        ForwardingRuntimeOperation::Delete { forward_id },
                        cx,
                    );
                }
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn create_forward(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        rule: ForwardRule,
        start: bool,
        check_health: bool,
        cx: &mut Context<Self>,
    ) {
        if self.forwarding.read(cx).operation_pending(tab_id, cx) {
            return;
        }
        let can_save = self.forward_owner_for_node(&node_id).is_some();
        let operation = if can_save {
            let deferred = start.then(|| ForwardingRuntimeOperation::Create {
                rule: rule.clone(),
                check_health,
            });
            self.forwarding.update(cx, |forwarding, _| {
                forwarding.set_start_after_save(tab_id, deferred)
            });
            ForwardingRuntimeOperation::Save { rule }
        } else {
            ForwardingRuntimeOperation::Create { rule, check_health }
        };
        self.start_forward_operation(
            tab_id,
            node_id,
            "forwards.messages.created",
            true,
            operation,
            cx,
        );
    }
    pub(in crate::workspace) fn start_forward_operation(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        message_key: &'static str,
        sync_saved_forwards_on_success: bool,
        operation: ForwardingRuntimeOperation,
        cx: &mut Context<Self>,
    ) {
        let Some(page) = self.forwarding.read(cx).page_state(tab_id) else {
            return;
        };
        if page.read(cx).pending() {
            return;
        }
        let needs_connection = matches!(
            operation,
            ForwardingRuntimeOperation::Create { .. }
                | ForwardingRuntimeOperation::Restart { .. }
                | ForwardingRuntimeOperation::Quick { .. }
        );
        if needs_connection && !self.node_is_ready_for_forwarding(&node_id) {
            // Only an explicit start requests SSH. Authentication and jump-host
            // handling stay with the existing connection flow; no shell is opened.
            if self.ensure_node_connection_started(&node_id, cx) {
                page.update(cx, |page, cx| {
                    page.begin_operation();
                    cx.notify();
                });
                self.forwarding.update(cx, |forwarding, _| {
                    forwarding.set_deferred_start(
                        tab_id,
                        (message_key, sync_saved_forwards_on_success, operation),
                    );
                });
                return;
            }
            let error = self.i18n.t("forwards.messages.node_not_ready");
            page.update(cx, |page, cx| {
                page.set_error(error);
                cx.notify();
            });
            return;
        }
        let owner_connection_id = self.forward_owner_for_node(&node_id);
        page.update(cx, |page, cx| {
            page.begin_operation();
            cx.notify();
        });
        self.forwarding.update(cx, |forwarding, _cx| {
            forwarding.request_operation(
                tab_id,
                node_id,
                owner_connection_id,
                message_key,
                sync_saved_forwards_on_success,
                operation,
            );
        });
    }

    pub(in crate::workspace) fn resume_forward_starts(
        &mut self,
        node_id: &NodeId,
        state: &NodeReadiness,
        cx: &mut Context<Self>,
    ) {
        self.resume_ssh_tunnel_starts(node_id, state, cx);
        if !matches!(
            state,
            NodeReadiness::Ready | NodeReadiness::Error | NodeReadiness::Disconnected
        ) {
            return;
        }
        let tabs = self
            .forwarding
            .read(cx)
            .tab_node_mappings()
            .iter()
            .filter(|(_, node)| *node == node_id)
            .map(|(tab, _)| *tab)
            .collect::<Vec<_>>();
        self.forwarding.update(cx, |forwarding, cx| {
            forwarding.refresh_runtime_snapshot(node_id, cx);
            forwarding.notify_node(node_id, cx);
        });
        for tab_id in tabs {
            let pending = self
                .forwarding
                .update(cx, |forwarding, _| forwarding.take_deferred_start(tab_id));
            if let Some((message, sync, operation)) = pending {
                if let Some(page) = self.forwarding.read(cx).page_state(tab_id) {
                    page.update(cx, |page, cx| {
                        page.finish_operation();
                        cx.notify();
                    });
                }
                if *state == NodeReadiness::Ready {
                    self.start_forward_operation(
                        tab_id,
                        node_id.clone(),
                        message,
                        sync,
                        operation,
                        cx,
                    );
                } else {
                    let error = self.i18n.t("forwards.messages.connection_not_ready");
                    if let Some(page) = self.forwarding.read(cx).page_state(tab_id) {
                        page.update(cx, |page, cx| {
                            page.set_error(error);
                            cx.notify();
                        });
                    }
                }
            }
        }
    }

    pub(super) fn start_port_profiler_for_node(&mut self, node_id: NodeId, cx: &mut Context<Self>) {
        // Port profiling is view sampling, not tunnel ownership. It can stop
        // while hidden without releasing listeners, managers, or SSH consumers.
        self.forwarding.update(cx, |forwarding, _cx| {
            forwarding.track_port_profiler(node_id.clone());
        });
        self.start_port_scan(node_id, true, cx);
    }

    pub(in crate::workspace) fn start_port_profiler_for_node_without_notify(
        &mut self,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        self.forwarding.update(cx, |forwarding, _cx| {
            forwarding.track_port_profiler(node_id.clone());
        });
    }

    pub(in crate::workspace) fn maybe_start_forwards_port_scan(&mut self, cx: &mut Context<Self>) {
        let nodes = self.forwarding.read(cx).tracked_port_profiler_nodes();
        for node_id in nodes {
            if !self.forwards_node_has_visible_tab(&node_id, cx) {
                if let Some(connection_id) = self.forwarding_connection_id_for_node(&node_id) {
                    self.stop_hidden_port_profiler(&connection_id, cx);
                }
                self.forwarding.update(cx, |forwarding, _cx| {
                    forwarding.reset_hidden_port_scan_schedule(&node_id);
                });
                continue;
            }
            let due = self
                .forwarding
                .read(cx)
                .port_scan_due(&node_id, FORWARDS_PORT_SCAN_INTERVAL);
            if due {
                self.start_port_scan(node_id, false, cx);
            }
        }
    }

    pub(in crate::workspace) fn maybe_refresh_forwards_stats(&mut self, cx: &mut Context<Self>) {
        if !self
            .forwarding
            .read(cx)
            .tab_node_mappings()
            .keys()
            .any(|tab_id| self.forwards_tab_is_visible(*tab_id, cx))
        {
            return;
        }
        let visible_nodes = self
            .forwarding
            .read(cx)
            .tab_node_mappings()
            .iter()
            .filter(|(tab_id, _)| self.forwards_tab_is_visible(**tab_id, cx))
            .map(|(_, node_id)| node_id.clone())
            .collect::<Vec<_>>();
        self.forwarding.update(cx, |forwarding, cx| {
            if forwarding.stats_due(cx) {
                for node_id in &visible_nodes {
                    forwarding.refresh_runtime_snapshot(node_id, cx);
                }
            }
        });
    }

    pub(in crate::workspace) fn sync_forwarding_sampling_visibility(&mut self, cx: &mut App) {
        let sampling_visible = self
            .forwarding
            .read(cx)
            .tab_node_mappings()
            .keys()
            .any(|tab_id| self.forwards_tab_is_visible(*tab_id, cx));
        let visibility_changed = self.forwarding.update(cx, |forwarding, cx| {
            forwarding.set_sampling_visible(sampling_visible, cx)
        });
        if !visibility_changed || sampling_visible {
            return;
        }

        // Hiding the view stops only sampling profilers. Tunnel managers,
        // listeners, and SSH consumers retain their independent runtime owner.
        for node_id in self.forwarding.read(cx).tracked_port_profiler_nodes() {
            if let Some(connection_id) = self.forwarding_connection_id_for_node(&node_id) {
                self.forwarding_service
                    .registry()
                    .stop_port_profiler(&connection_id);
            }
        }
    }

    pub(in crate::workspace) fn stop_hidden_port_profiler(&self, connection_id: &str, cx: &App) {
        // Several logical nodes can share a physical profiler, including while a scan completes.
        let visible = self
            .forwarding
            .read(cx)
            .tab_node_mappings()
            .iter()
            .any(|(tab_id, node)| {
                self.forwards_tab_is_visible(*tab_id, cx)
                    && self.node_router.connection_id_for_node(node).as_deref()
                        == Some(connection_id)
            });
        if !visible {
            self.forwarding_service
                .registry()
                .stop_port_profiler(connection_id);
        }
    }

    pub(in crate::workspace) fn start_port_scan(
        &mut self,
        node_id: NodeId,
        restart_degraded_profiler: bool,
        cx: &mut Context<Self>,
    ) {
        if self.forwarding.read(cx).port_scan_pending(&node_id) {
            return;
        }
        // Port detection follows the same nodeReady gate as Tauri's
        // usePortDetection hook. A restored Forwards tab should stay passive
        // until the user reconnects the node explicitly.
        if !self.node_is_ready_for_forwarding(&node_id) {
            self.forwarding.update(cx, |forwarding, cx| {
                forwarding.mark_port_scan_not_ready(node_id.clone());
                forwarding.notify_node(&node_id, cx);
            });
            return;
        }

        let owner_connection_id = self.forward_owner_for_node(&node_id);
        self.forwarding.update(cx, |forwarding, cx| {
            forwarding.request_port_scan(
                node_id.clone(),
                owner_connection_id,
                restart_degraded_profiler,
            );
            forwarding.notify_node(&node_id, cx);
        });
    }

    pub(in crate::workspace) fn node_is_ready_for_forwarding(&self, node_id: &NodeId) -> bool {
        self.ssh_nodes
            .get(node_id)
            .is_some_and(|node| node.readiness == NodeReadiness::Ready)
            && self
                .node_router
                .connection_id_for_node(node_id)
                .and_then(|connection_id| self.ssh_registry.get(&connection_id))
                .is_some_and(|handle| {
                    matches!(
                        handle.state(),
                        ConnectionState::Active | ConnectionState::Idle
                    )
                })
    }

    pub(in crate::workspace) fn handle_forwarding_workspace_event(
        &mut self,
        event: ForwardingWorkspaceEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ForwardingWorkspaceEvent::DeliveryReady => {}
            ForwardingWorkspaceEvent::SamplingDue => {
                self.sync_forwarding_sampling_visibility(cx);
                if !self.forwarding.read(cx).sampling_visible(cx) {
                    return;
                }
                // The Entity owns cadence; the root only supplies the current
                // cross-window visibility and runtime adapters.
                self.maybe_start_forwards_port_scan(cx);
                self.maybe_refresh_forwards_stats(cx);
                return;
            }
        }
        let intents = self
            .forwarding
            .update(cx, |forwarding, _cx| forwarding.take_delivery_intents());
        let mut changed = false;
        for intent in intents {
            match intent {
                ForwardingDeliveryIntent::Operation {
                    tab_id,
                    message_key,
                    sync_saved_forwards_on_success,
                    result,
                } => {
                    let visible = self.forwards_tab_is_visible(tab_id, cx);
                    let delay = motion::duration(&self.tokens, MotionDuration::Overlay);
                    if let Some(page) = self.forwarding.read(cx).page_state(tab_id) {
                        page.update(cx, |page, cx| {
                            page.complete_operation(result.clone(), visible, delay, cx)
                        });
                    }
                    let start = self
                        .forwarding
                        .update(cx, |forwarding, _| forwarding.take_start_after_save(tab_id));
                    match result {
                        Ok(()) => {
                            if sync_saved_forwards_on_success {
                                self.queue_cloud_sync_dirty_refresh(cx);
                            }
                            if let Some(operation) = start
                                && let Some(node) = self.forwarding.read(cx).node_for_tab(tab_id)
                            {
                                self.start_forward_operation(
                                    tab_id,
                                    node,
                                    message_key,
                                    true,
                                    operation,
                                    cx,
                                );
                            }
                        }
                        Err(error) if !visible => {
                            self.push_forward_status_notice(
                                self.i18n.t("forwards.toast.error_title"),
                                Some(error),
                                TerminalNoticeVariant::Error,
                                cx,
                            );
                        }
                        Err(_) => {}
                    }
                    self.flush_ssh_tunnel_stops(cx);
                    if let Some(node) = self.forwarding.read(cx).node_for_tab(tab_id) {
                        self.resume_ssh_tunnel_starts(&node, &NodeReadiness::Ready, cx);
                    }
                }
                ForwardingDeliveryIntent::PortScan { connection_id } => {
                    self.stop_hidden_port_profiler(&connection_id, cx);
                }
                ForwardingDeliveryIntent::ReconnectRestore {
                    node_id,
                    result,
                    restored,
                    detail,
                    job_id,
                    created_forwards,
                } => {
                    changed |= self.apply_reconnect_forward_restore_completion(
                        node_id,
                        result,
                        restored,
                        detail,
                        job_id,
                        created_forwards,
                        cx,
                    );
                }
                ForwardingDeliveryIntent::Runtime(ForwardEvent::StatusChanged {
                    status,
                    error,
                    ..
                }) => match status {
                    ForwardStatus::Suspended => {
                        let description = self.i18n.t("forwards.toast.suspended_desc");
                        self.push_forward_status_notice(
                            self.i18n.t("forwards.toast.suspended_title"),
                            Some(description),
                            TerminalNoticeVariant::Warning,
                            cx,
                        );
                    }
                    ForwardStatus::Error => {
                        self.push_forward_status_notice(
                            self.i18n.t("forwards.toast.error_title"),
                            error,
                            TerminalNoticeVariant::Error,
                            cx,
                        );
                    }
                    _ => {}
                },
                ForwardingDeliveryIntent::Runtime(ForwardEvent::StatsUpdated { .. }) => {}
                ForwardingDeliveryIntent::Runtime(ForwardEvent::SessionSuspended {
                    forward_ids,
                    ..
                }) => {
                    // Tauri handles sessionSuspended as a toast-only runtime
                    // event. Keep inline form errors reserved for create/edit
                    // validation and operation failures.
                    self.push_forward_status_notice(
                        self.i18n.t("forwards.toast.session_suspended_title"),
                        Some(
                            self.i18n
                                .t("forwards.toast.session_suspended_desc")
                                .replace("{{count}}", &forward_ids.len().to_string()),
                        ),
                        TerminalNoticeVariant::Warning,
                        cx,
                    );
                }
                ForwardingDeliveryIntent::Runtime(ForwardEvent::PortDetected { .. }) => {}
            }
        }
        if changed {
            cx.notify();
        }
    }

    fn push_forward_status_notice(
        &self,
        title: String,
        description: Option<String>,
        variant: TerminalNoticeVariant,
        cx: &App,
    ) {
        // Tauri's ForwardsView emits toast() for suspended/error status events
        // while keeping create-form failures inline. Mirror that split so bind
        // and remote-open classes remain visible without turning every failed
        // form submission into a workspace toast.
        self.push_workspace_notice(
            TerminalNotice {
                title,
                description,
                status_text: None,
                progress: None,
                variant,
            },
            cx,
        );
    }

    fn forwards_node_has_visible_tab(&self, node_id: &NodeId, cx: &App) -> bool {
        self.forwarding
            .read(cx)
            .tab_node_mappings()
            .iter()
            .any(|(tab_id, visible_node_id)| {
                visible_node_id == node_id && self.forwards_tab_is_visible(*tab_id, cx)
            })
    }

    fn forwards_tab_is_visible(&self, tab_id: TabId, cx: &App) -> bool {
        self.tab_host.read(cx).surface_is_visible(tab_id)
    }

    pub(in crate::workspace) fn forwarding_connection_id_for_node(
        &self,
        node_id: &NodeId,
    ) -> Option<String> {
        // Teardown targets the manager that actually started the profiler, even after a rebind.
        self.forwarding_service
            .manager_for_node(node_id)
            .map(|manager| manager.ssh_connection_handle().connection_id().to_owned())
            .or_else(|| self.node_router.connection_id_for_node(node_id))
    }

    pub(in crate::workspace) fn forward_owner_for_node(&self, node_id: &NodeId) -> Option<String> {
        let origin = self.node_router.node_metadata(node_id)?.origin;
        if matches!(origin, oxideterm_ssh::NodeOrigin::DrillDown { .. }) {
            return self.ssh_nodes.get(node_id)?.saved_connection_id.clone();
        }
        origin.saved_target_id().map(str::to_owned)
    }

    pub(in crate::workspace) fn forwarding_session_id_for_node(&self, node_id: &NodeId) -> String {
        super::ForwardingRuntimeService::session_id_for_node(node_id)
    }

    pub(in crate::workspace) fn handle_forwards_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(page) = self.forwarding_page(cx) else {
            return false;
        };
        match page.update(cx, |page, cx| page.handle_input_key(event, cx)) {
            ForwardsInputKey::Unhandled => false,
            ForwardsInputKey::Handled => true,
            ForwardsInputKey::Blur => {
                self.ime_marked_text = None;
                true
            }
        }
    }
}
