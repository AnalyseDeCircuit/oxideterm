use super::helpers::parse_port;
use super::{
    App, ConnectionConsumer, ConnectionState, Context, DetectedPort, FORWARDS_DEFAULT_BIND_ADDRESS,
    FORWARDS_DEFAULT_TARGET_HOST, FORWARDS_PORT_SCAN_INTERVAL, FORWARDS_STATS_REFRESH_INTERVAL,
    ForwardEvent, ForwardInput, ForwardRule, ForwardStatus, ForwardType, ForwardUpdate,
    ForwardingDeliveryIntent, ForwardingRuntimeOperation, ForwardingWorkspaceEvent, KeyDownEvent,
    NodeId, NodeReadiness, PortDetectionSnapshot, TabId, TerminalNotice, TerminalNoticeVariant,
    WorkspaceApp,
};
impl WorkspaceApp {
    pub(super) fn submit_forward_create(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        let (
            forward_type,
            bind_address,
            bind_port_value,
            target_host,
            target_port_value,
            skip_health_check,
        ) = {
            let view = self.forwarding.read(cx).view();
            (
                view.forward_type,
                view.bind_address.clone(),
                view.bind_port.clone(),
                view.target_host.clone(),
                view.target_port.clone(),
                view.skip_health_check,
            )
        };
        let Some((bind_port, target_port)) =
            self.validate_forward_form(forward_type, &bind_port_value, &target_port_value, cx)
        else {
            cx.notify();
            return;
        };
        let rule = match forward_type {
            ForwardType::Local => ForwardRule::local(
                bind_address.clone(),
                bind_port,
                target_host.clone(),
                target_port.unwrap_or(0),
            ),
            ForwardType::Remote => ForwardRule::remote(
                bind_address.clone(),
                bind_port,
                target_host,
                target_port.unwrap_or(0),
            ),
            ForwardType::Dynamic => ForwardRule {
                target_host: "0.0.0.0".to_string(),
                ..ForwardRule::dynamic(bind_address, bind_port)
            },
        };
        let check_health = !skip_health_check;
        self.start_forward_operation(
            tab_id,
            node_id,
            "forwards.messages.created",
            true,
            ForwardingRuntimeOperation::Create { rule, check_health },
            cx,
        );
    }

    pub(super) fn create_local_forward_for_detected_port(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        port: DetectedPort,
        cx: &mut Context<Self>,
    ) {
        let mut rule = ForwardRule::local(
            FORWARDS_DEFAULT_BIND_ADDRESS,
            port.port,
            FORWARDS_DEFAULT_TARGET_HOST,
            port.port,
        );
        rule.description = port
            .process_name
            .as_ref()
            .map(|process| format!("{process} ({})", self.i18n.t("forwards.detection.auto")))
            .unwrap_or_else(|| {
                format!(
                    "{} {} ({})",
                    self.i18n.t("forwards.detection.port"),
                    port.port,
                    self.i18n.t("forwards.detection.auto")
                )
            });
        self.dismiss_detected_port(port.port, cx);
        self.start_forward_operation(
            tab_id,
            node_id,
            "forwards.messages.created",
            true,
            ForwardingRuntimeOperation::Create {
                rule,
                check_health: true,
            },
            cx,
        );
    }

    pub(super) fn dismiss_detected_port(&mut self, port: u16, cx: &mut Context<Self>) {
        if let Some(tab_id) = self.main_window_tabs.active_tab_id
            && let Some(node_id) = self.forwarding.read(cx).node_for_tab(tab_id)
        {
            self.forwarding.update(cx, |forwarding, _cx| {
                forwarding.dismiss_detected_port(&node_id, port);
            });
        }
    }

    pub(super) fn submit_forward_edit(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        let (editing, edit_bind_address, edit_bind_port, edit_target_host, edit_target_port) = {
            let view = self.forwarding.read(cx).view();
            let Some(editing) = view.editing_forward.clone() else {
                return;
            };
            (
                editing,
                view.edit_bind_address.clone(),
                view.edit_bind_port.clone(),
                view.edit_target_host.clone(),
                view.edit_target_port.clone(),
            )
        };
        let Some((bind_port, target_port)) = self.validate_forward_form(
            editing.forward_type,
            &edit_bind_port,
            &edit_target_port,
            cx,
        ) else {
            cx.notify();
            return;
        };
        let update = ForwardUpdate {
            bind_address: Some(edit_bind_address),
            bind_port: Some(bind_port),
            target_host: (editing.forward_type != ForwardType::Dynamic).then_some(edit_target_host),
            target_port,
            ..ForwardUpdate::default()
        };
        let forward_id = editing.id;
        self.start_forward_operation(
            tab_id,
            node_id,
            "forwards.messages.updated",
            true,
            ForwardingRuntimeOperation::Update { forward_id, update },
            cx,
        );
    }

    fn validate_forward_form(
        &mut self,
        forward_type: ForwardType,
        bind_port: &str,
        target_port: &str,
        cx: &mut Context<Self>,
    ) -> Option<(u16, Option<u16>)> {
        let Some(bind_port) = parse_port(bind_port) else {
            let error = self.i18n.t(if bind_port.trim().is_empty() {
                "forwards.form.port_required"
            } else {
                "forwards.form.port_invalid"
            });
            self.forwarding
                .update(cx, |forwarding, _cx| forwarding.set_error(error));
            return None;
        };
        if forward_type == ForwardType::Dynamic {
            self.forwarding
                .update(cx, |forwarding, _cx| forwarding.clear_error());
            return Some((bind_port, None));
        }
        let Some(target_port) = parse_port(target_port) else {
            let error = self.i18n.t(if target_port.trim().is_empty() {
                "forwards.form.port_required"
            } else {
                "forwards.form.port_invalid"
            });
            self.forwarding
                .update(cx, |forwarding, _cx| forwarding.set_error(error));
            return None;
        };
        self.forwarding
            .update(cx, |forwarding, _cx| forwarding.clear_error());
        Some((bind_port, Some(target_port)))
    }

    pub(super) fn start_forward_operation(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        message_key: &'static str,
        sync_saved_forwards_on_success: bool,
        operation: ForwardingRuntimeOperation,
        cx: &mut Context<Self>,
    ) {
        // Tauri gates ForwardsView work on nodeReady and its node_forwarding
        // commands require an existing forwarding manager; opening this surface
        // must not become an implicit SSH connect action.
        if !self.node_is_ready_for_forwarding(&node_id) {
            let error = self.i18n.t("forwards.messages.node_not_ready");
            self.forwarding
                .update(cx, |forwarding, _cx| forwarding.set_error(error));
            cx.notify();
            return;
        }
        let owner_connection_id = self
            .ssh_nodes
            .get(&node_id)
            .and_then(|node| node.saved_connection_id.clone());
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
        cx.notify();
    }

    pub(super) fn start_port_profiler_for_node(&mut self, node_id: NodeId, cx: &mut Context<Self>) {
        // Tauri starts a per-connection ResourceProfiler from usePortDetection
        // and leaves it running after the Forwards view unmounts. Native mirrors
        // that lifecycle with a node-owned scanner that emits PortScan results
        // independent of the currently active tab.
        self.forwarding.update(cx, |forwarding, _cx| {
            forwarding.track_port_profiler(node_id.clone());
        });
        self.sync_forwarding_view_port_detection(&node_id, cx);
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
        self.sync_forwarding_view_port_detection(&node_id, cx);
    }

    pub(in crate::workspace) fn maybe_start_forwards_port_scan(&mut self, cx: &mut Context<Self>) {
        let nodes = self.forwarding.read(cx).tracked_port_profiler_nodes();
        for node_id in nodes {
            if !self.forwards_node_has_visible_tab(&node_id, cx) {
                if let Some(connection_id) = self.forwarding_connection_id_for_node(&node_id) {
                    // Hidden forwarding pages stop their profiler without touching tunnel owners.
                    self.forwarding_service
                        .registry()
                        .stop_port_profiler(&connection_id);
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
            .any(|tab_id| self.forwards_tab_is_visible(*tab_id))
        {
            return;
        }
        let visible_nodes = self
            .forwarding
            .read(cx)
            .tab_node_mappings()
            .iter()
            .filter(|(tab_id, _)| self.forwards_tab_is_visible(**tab_id))
            .map(|(_, node_id)| node_id.clone())
            .collect::<Vec<_>>();
        let changed = self.forwarding.update(cx, |forwarding, _cx| {
            if !forwarding.mark_stats_refreshed_if_due(FORWARDS_STATS_REFRESH_INTERVAL) {
                return false;
            }
            let mut changed = false;
            for node_id in &visible_nodes {
                changed |= forwarding.refresh_runtime_snapshot(node_id);
            }
            changed
        });
        if changed {
            cx.notify();
        }
    }

    fn start_port_scan(
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
            self.forwarding.update(cx, |forwarding, _cx| {
                forwarding.mark_port_scan_not_ready(node_id.clone());
            });
            self.sync_forwarding_view_port_detection(&node_id, cx);
            cx.notify();
            return;
        }

        let owner_connection_id = self
            .ssh_nodes
            .get(&node_id)
            .and_then(|node| node.saved_connection_id.clone());
        self.forwarding.update(cx, |forwarding, _cx| {
            forwarding.request_port_scan(
                node_id.clone(),
                owner_connection_id,
                restart_degraded_profiler,
            );
        });
        self.sync_forwarding_view_port_detection(&node_id, cx);
        cx.notify();
    }

    fn node_is_ready_for_forwarding(&self, node_id: &NodeId) -> bool {
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
                    binding,
                    result,
                } => {
                    self.remember_forwarding_binding(binding);
                    self.forwarding
                        .update(cx, |forwarding, _cx| forwarding.finish_operation());
                    match result {
                        Ok(()) => {
                            if sync_saved_forwards_on_success {
                                // Persisted mutations are durable even when the initiating tab
                                // becomes hidden before the worker completes.
                                self.queue_cloud_sync_dirty_refresh(cx);
                            }
                            if self.forwards_tab_is_visible(tab_id) {
                                let _ = message_key;
                                let (show_new_form, editing_forward) = {
                                    let view = self.forwarding.read(cx).view();
                                    (view.show_new_form, view.editing_forward.is_some())
                                };
                                if show_new_form {
                                    self.begin_forward_create_form_exit(cx);
                                }
                                if editing_forward {
                                    self.begin_forward_edit_form_exit(cx);
                                }
                                self.forwarding.update(cx, |forwarding, _cx| {
                                    forwarding.reset_completed_operation_form();
                                });
                                changed = true;
                            }
                        }
                        Err(error) => {
                            if self.forwards_tab_is_visible(tab_id) {
                                self.forwarding.update(cx, |forwarding, _cx| {
                                    forwarding.set_error(error);
                                });
                                changed = true;
                            } else {
                                self.push_forward_status_notice(
                                    self.i18n.t("forwards.toast.error_title"),
                                    Some(error),
                                    TerminalNoticeVariant::Error,
                                    cx,
                                );
                            }
                        }
                    }
                }
                ForwardingDeliveryIntent::Binding { binding } => {
                    self.remember_forwarding_binding(binding);
                    changed = true;
                }
                ForwardingDeliveryIntent::PortScan { node_id, binding } => {
                    self.remember_forwarding_binding(binding);
                    if self.active_forwards_tab_matches_node(&node_id, cx) {
                        self.sync_forwarding_view_port_detection(&node_id, cx);
                        changed = true;
                    }
                }
                ForwardingDeliveryIntent::ReconnectRestore {
                    node_id,
                    result,
                    restored,
                    detail,
                    job_id,
                    created_forwards,
                    bindings,
                } => {
                    changed |= self.apply_reconnect_forward_restore_completion(
                        node_id,
                        result,
                        restored,
                        detail,
                        job_id,
                        created_forwards,
                        bindings,
                        cx,
                    );
                }
                ForwardingDeliveryIntent::Runtime(ForwardEvent::StatusChanged {
                    session_id,
                    status,
                    error,
                    ..
                }) => {
                    let visible = self.active_forwards_tab_matches_session(&session_id, cx);
                    match status {
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
                    }
                    changed |= visible;
                }
                ForwardingDeliveryIntent::Runtime(ForwardEvent::StatsUpdated {
                    session_id,
                    ..
                }) => {
                    if self.active_forwards_tab_matches_session(&session_id, cx) {
                        changed = true;
                    }
                }
                ForwardingDeliveryIntent::Runtime(ForwardEvent::SessionSuspended {
                    session_id,
                    forward_ids,
                }) => {
                    let visible = self.active_forwards_tab_matches_session(&session_id, cx);
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
                    changed |= visible;
                }
                ForwardingDeliveryIntent::Runtime(ForwardEvent::PortDetected {
                    connection_id,
                    new_ports,
                    closed_ports,
                    all_ports,
                }) => {
                    let Some(node_id) = self.forwarding_node_for_connection_id(&connection_id)
                    else {
                        continue;
                    };
                    self.forwarding.update(cx, |forwarding, _cx| {
                        forwarding.apply_port_detection_result(
                            &node_id,
                            Some(connection_id),
                            Ok(PortDetectionSnapshot {
                                new_ports,
                                closed_ports,
                                all_ports,
                                has_scanned: true,
                            }),
                        );
                    });
                    if self.active_forwards_tab_matches_node(&node_id, cx) {
                        self.sync_forwarding_view_port_detection(&node_id, cx);
                        changed = true;
                    }
                }
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

    fn active_forwards_tab_matches_session(&self, session_id: &str, cx: &App) -> bool {
        self.forwarding
            .read(cx)
            .tab_node_mappings()
            .iter()
            .any(|(tab_id, node_id)| {
                self.forwards_tab_is_visible(*tab_id)
                    && self.forwarding_session_id_for_node(node_id) == session_id
            })
    }

    fn active_forwards_tab_matches_node(&self, node_id: &NodeId, cx: &App) -> bool {
        self.forwarding
            .read(cx)
            .tab_node_mappings()
            .iter()
            .any(|(tab_id, visible_node_id)| {
                visible_node_id == node_id && self.forwards_tab_is_visible(*tab_id)
            })
    }

    fn forwards_node_has_visible_tab(&self, node_id: &NodeId, cx: &App) -> bool {
        self.forwarding
            .read(cx)
            .tab_node_mappings()
            .iter()
            .any(|(tab_id, visible_node_id)| {
                visible_node_id == node_id && self.forwards_tab_is_visible(*tab_id)
            })
    }

    fn forwards_tab_is_visible(&self, tab_id: TabId) -> bool {
        super::forwarding_tab_mount_is_visible(
            tab_id,
            self.main_window_tabs.active_tab_id,
            self.detached_tab_windows.contains_key(&tab_id),
        )
    }

    pub(in crate::workspace) fn forwarding_connection_id_for_node(
        &self,
        node_id: &NodeId,
    ) -> Option<String> {
        self.forwarding_service.connection_id_for_node(node_id)
    }

    fn forwarding_node_for_connection_id(&self, connection_id: &str) -> Option<NodeId> {
        self.forwarding_service
            .node_for_connection_id(connection_id)
    }

    pub(in crate::workspace) fn release_forwarding_binding_for_node(
        &mut self,
        node_id: &NodeId,
    ) -> Option<String> {
        self.forwarding_service.release_binding_for_node(node_id)
    }

    fn sync_forwarding_view_port_detection(&mut self, node_id: &NodeId, cx: &mut Context<Self>) {
        self.forwarding.update(cx, |forwarding, _cx| {
            forwarding.sync_active_port_detection(node_id);
        });
    }

    pub(in crate::workspace) fn remember_forwarding_binding(
        &mut self,
        binding: Option<(String, String, ConnectionConsumer)>,
    ) {
        let node_is_disconnected = binding
            .as_ref()
            .and_then(|(session_id, _, _)| {
                super::ForwardingRuntimeService::node_id_for_session(session_id)
            })
            .and_then(|node_id| self.ssh_nodes.get(&node_id))
            .is_some_and(|node| node.readiness == NodeReadiness::Disconnected);
        self.forwarding_service
            .remember_binding(binding, node_is_disconnected);
    }

    pub(in crate::workspace) fn forwarding_session_id_for_node(&self, node_id: &NodeId) -> String {
        super::ForwardingRuntimeService::session_id_for_node(node_id)
    }

    pub(super) fn open_forward_edit_form(&mut self, rule: ForwardRule, cx: &mut Context<Self>) {
        self.forwarding
            .update(cx, |forwarding, _cx| forwarding.open_edit_form(rule));
        cx.notify();
    }

    pub(in crate::workspace) fn handle_forwards_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(input) = self.forwarding.read(cx).view().focused_input else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }
        match key {
            "escape" => {
                self.forwarding
                    .update(cx, |forwarding, _cx| forwarding.clear_input_focus());
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            "backspace" => {
                let changed = self
                    .forwarding
                    .update(cx, |forwarding, _cx| forwarding.backspace_input(input));
                if changed {
                    // Empty Backspace is only visible if it also clears an
                    // existing validation error.
                    cx.notify();
                }
                true
            }
            _ => false,
        }
    }

    pub(in crate::workspace) fn forward_input_value<'a>(
        &self,
        input: ForwardInput,
        cx: &'a App,
    ) -> &'a str {
        self.forwarding.read(cx).input_value(input)
    }
}
