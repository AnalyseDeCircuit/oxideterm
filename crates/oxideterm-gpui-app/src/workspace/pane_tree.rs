use super::*;
pub(super) use oxideterm_gpui_shell::panes::SplitDrag;
use oxideterm_gpui_shell::panes::{
    MountedPane, PaneContent, PaneHost, PaneTree, SPLIT_HANDLE_LINE_ALPHA, ShellPaneRequest,
};
use oxideterm_terminal_triggers::SavedConnectionKind;
#[derive(Clone, Copy)]
enum TerminalPaneInteraction {
    PrivilegePromptSubmit,
    ContextAction,
}

struct ActivePaneSplitTarget {
    tab_id: TabId,
    active_pane_id: PaneId,
    source: TerminalSplitSource,
}

enum TerminalSplitSource {
    Local,
    Ssh(NodeId),
}

fn terminal_split_supported(
    kind: oxideterm_terminal::TerminalSessionKind,
    ssh_node_ready: bool,
) -> bool {
    use oxideterm_terminal::TerminalSessionKind;
    match kind {
        TerminalSessionKind::LocalPty => true,
        TerminalSessionKind::SshPty => ssh_node_ready,
        _ => false,
    }
}

fn terminal_auto_closes_on_exit(kind: oxideterm_terminal::TerminalSessionKind) -> bool {
    // A remote channel or device can end independently of the user's task. Retain
    // its pane and scrollback until explicitly closed, even if the node stays connected.
    kind == oxideterm_terminal::TerminalSessionKind::LocalPty
}

fn serial_profile_line_ending(
    line_ending: oxideterm_terminal::SerialLineEnding,
) -> oxideterm_connections::SerialLineEnding {
    match line_ending {
        oxideterm_terminal::SerialLineEnding::Lf => oxideterm_connections::SerialLineEnding::Lf,
        oxideterm_terminal::SerialLineEnding::CrLf => oxideterm_connections::SerialLineEnding::CrLf,
        oxideterm_terminal::SerialLineEnding::Cr => oxideterm_connections::SerialLineEnding::Cr,
        oxideterm_terminal::SerialLineEnding::None => oxideterm_connections::SerialLineEnding::None,
    }
}

fn serial_profile_send_mode(
    mode: oxideterm_terminal::SerialSendMode,
) -> oxideterm_connections::SerialSendMode {
    match mode {
        oxideterm_terminal::SerialSendMode::Text => oxideterm_connections::SerialSendMode::Text,
        oxideterm_terminal::SerialSendMode::Hex => oxideterm_connections::SerialSendMode::Hex,
    }
}

fn serial_profile_display_mode(
    mode: oxideterm_terminal::SerialDisplayMode,
) -> oxideterm_connections::SerialDisplayMode {
    match mode {
        oxideterm_terminal::SerialDisplayMode::Text => {
            oxideterm_connections::SerialDisplayMode::Text
        }
        oxideterm_terminal::SerialDisplayMode::Hex => oxideterm_connections::SerialDisplayMode::Hex,
        oxideterm_terminal::SerialDisplayMode::Mixed => {
            oxideterm_connections::SerialDisplayMode::Mixed
        }
    }
}

#[derive(Clone)]
struct TerminalInputBroadcastRoute {
    source_pane_id: PaneId,
    session_id: TerminalSessionId,
    ai_runtime: gpui::WeakEntity<crate::workspace::ai_runtime_context::AiRuntimeContextEntity>,
    agent_resources: oxideterm_ai::agent::AgentResourceCoordinator,
    tab_host: gpui::WeakEntity<tabs::WorkspaceTabHostEntity>,
    terminal: gpui::WeakEntity<WorkspaceTerminalEntity>,
}

impl TerminalInputBroadcastRoute {
    fn broadcaster(self) -> TerminalInputBroadcaster {
        Rc::new(move |kind, bytes, cx| self.deliver(kind, bytes, cx))
    }

    fn deliver(&self, kind: TerminalBroadcastInputKind, bytes: &[u8], cx: &mut App) {
        if let Some(runtime) = self.ai_runtime.upgrade() {
            if let Some(key) = runtime.read(cx).terminal_resource_key(self.session_id) {
                if self.agent_resources.has_owner(&key) {
                    self.agent_resources.invalidate(&key);
                }
            }
        }
        let Some(tab_host) = self.tab_host.upgrade() else {
            return;
        };
        let Some(terminal) = self.terminal.upgrade() else {
            return;
        };

        let (live_panes, mut candidates) = {
            let tab_host = tab_host.read(cx);
            let live_panes = tab_host.panes().keys().copied().collect::<HashSet<_>>();
            let mut candidates = Vec::new();
            for tab in tab_host.tabs() {
                if let Some(root) = tab.root_pane.as_ref() {
                    root.collect_pane_ids(&mut candidates);
                }
            }
            (live_panes, candidates)
        };
        candidates
            .retain(|pane_id| *pane_id != self.source_pane_id && live_panes.contains(pane_id));

        let targets = terminal.update(cx, |terminal, _cx| {
            terminal.retain_live_broadcast_targets(&live_panes);
            terminal.filter_broadcast_targets(self.source_pane_id, candidates)
        });
        if targets.is_empty() {
            return;
        }
        let explicit_send = matches!(kind, TerminalBroadcastInputKind::Paste)
            || bytes.contains(&b'\r')
            || bytes.contains(&b'\n');
        let mut batch = explicit_send.then(|| {
            let mut context = oxideterm_audit::AuditContext::current_request()
                .or_else(oxideterm_audit::AuditContext::current);
            if let Some(context) = &mut context {
                context.source = oxideterm_audit::AuditSource::Broadcast;
            }
            oxideterm_audit::AuditOperation::in_context(
                context.as_ref(),
                oxideterm_audit::AuditCategory::Automation,
                "broadcast_send_batch",
                None,
            )
        });
        let parent_id = batch
            .as_ref()
            .and_then(oxideterm_audit::AuditOperation::id)
            .map(str::to_string);
        let target_count = targets.len();
        let mut sent_count = 0usize;
        for pane_id in targets {
            let session_id = tab_host
                .read(cx)
                .tabs()
                .iter()
                .find_map(|tab| tab.root_pane.as_ref()?.session_id_for_pane(pane_id));
            if let Some(runtime) = self.ai_runtime.upgrade() {
                if let Some(key) =
                    session_id.and_then(|id| runtime.read(cx).terminal_resource_key(id))
                {
                    if self.agent_resources.has_owner(&key) {
                        self.agent_resources.invalidate(&key);
                    }
                }
            }
            let Some(pane) = tab_host.read(cx).panes().get(&pane_id).cloned() else {
                continue;
            };
            if pane.update(cx, |pane, cx| {
                // Borrowed input is delivered synchronously and never retained
                // outside the target pane's existing zeroizing write path.
                pane.send_broadcast_input_with_parent(kind, bytes, parent_id.as_deref(), cx)
            }) {
                sent_count += 1;
            }
        }
        if let Some(mut batch) = batch.take() {
            batch.summary(&format!(
                "targets={target_count}; sent={sent_count}; input_bytes={}",
                bytes.len()
            ));
            batch.finish(
                if sent_count == target_count {
                    oxideterm_audit::AuditOutcome::Sent
                } else if sent_count > 0 {
                    oxideterm_audit::AuditOutcome::Partial
                } else {
                    oxideterm_audit::AuditOutcome::Failed
                },
                oxideterm_audit::AuditEvidence::Dispatch,
                None,
                None,
            );
        }
    }
}

impl WorkspaceApp {
    pub(super) fn register_terminal_pane(
        &mut self,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        pane: gpui::Entity<TerminalPane>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let window_handle = window.window_handle();
        let terminal_label = pane.read(cx).title().to_string();
        let broadcaster = TerminalInputBroadcastRoute {
            source_pane_id: pane_id,
            session_id,
            ai_runtime: self.ai.authority.downgrade(),
            agent_resources: self.ai.state.read(cx).agents.services.resources.clone(),
            tab_host: self.tab_host.downgrade(),
            terminal: self.terminal.downgrade(),
        }
        .broadcaster();
        pane.update(cx, |pane, _cx| {
            // Weak routing endpoints follow pane remounts without taking
            // ownership of the pane, its SSH channel, or the physical node.
            pane.set_input_broadcaster(Some(broadcaster));
            // Plugin input hooks attribute keystrokes to this session id.
            pane.set_plugin_session_id(Some(session_id.0.to_string()));
        });
        self.tab_host.update(cx, |tab_host, cx| {
            tab_host.register_terminal_pane(pane_id, session_id, pane.clone(), window_handle, cx);
        });
        // The live terminal session is the capability owner. A later tab move
        // reuses this registration instead of minting another owner identity.
        self.ai.authority.update(cx, |runtime, _cx| {
            runtime.register_terminal_session(session_id, terminal_label);
        });
        // Publish the GATT command surface for plugin host calls. This shared
        // pane funnel is the single registration point, so every session
        // creation path (current and future) publishes BLE sessions; non-BLE
        // sessions expose no handle and are skipped. Registry cleanup on close
        // is hygiene; the session worker's drop ends pumps and CCCD state on
        // its own.
        if let Some(handle) = pane.read(cx).shared_session().lock().ble_gatt_handle() {
            self.ble_gatt_handles
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(session_id, handle);
        }
        self.refresh_native_plugin_terminal_hooks(cx);
        // A newly registered pane has no stored toolbar state, so its chips
        // render the contributed default states with live plugin availability
        // without touching the states other panes display.
        self.push_plugin_toolbar_specs_for_pane(pane_id, &pane, cx);
    }

    pub(super) fn handle_terminal_pane_delivery(
        &mut self,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        window_handle: AnyWindowHandle,
        event: TerminalPaneEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TerminalPaneEvent::LocalHistoryFile(file) => {
                let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() else {
                    return;
                };
                let history = self
                    .local_terminal_histories
                    .entry(file)
                    .or_insert_with_key(|file| {
                        SharedTerminalCommandHistory::from_commands(file.commands())
                    })
                    .clone();
                pane.update(cx, |pane, cx| pane.bind_local_shell_history(history, cx));
            }
            TerminalPaneEvent::X11ForwardingStatus(status) => {
                self.push_workspace_notice(x11_forwarding_notice(status, &self.i18n), cx);
            }
            TerminalPaneEvent::InputActivity => {
                if self.active_pane_id(cx) == Some(pane_id) {
                    self.cancel_pending_privilege_totp();
                }
                oxideterm_gpui_background::note_input_activity(window_handle.window_id(), cx);
            }
            TerminalPaneEvent::Exited { .. } => {
                self.queue_auto_close_terminal_session(session_id, cx);
            }
            // TabHost consumes this signal before ordinary pane delivery.
            TerminalPaneEvent::OutputActivity => {
                if self.active_pane_id(cx) == Some(pane_id) {
                    self.schedule_privilege_prompt_sync(cx);
                }
            }
            // TabHost consumes BellActivity for background tab highlighting.
            TerminalPaneEvent::BellActivity => {}
            TerminalPaneEvent::HoverTooltipChanged => {
                let id = format!("terminal-timestamp-{}", pane_id.0);
                let tooltip = self
                    .tab_host
                    .read(cx)
                    .panes()
                    .get(&pane_id)
                    .and_then(|pane| {
                        pane.read(cx)
                            .hover_tooltip()
                            .map(|(label, position)| (label.to_string(), position))
                    });
                // Discard pending text before replacing the target; IDs isolate split panes.
                self.clear_workspace_tooltip(&id, cx);
                if let Some((label, position)) = tooltip {
                    self.queue_workspace_tooltip(
                        id,
                        label,
                        f32::from(position.x) + 12.0,
                        f32::from(position.y) + 16.0,
                        cx,
                    );
                }
            }
            TerminalPaneEvent::TriggerInteractionRequested => {
                let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() else {
                    return;
                };
                let owner = pane.downgrade();
                if let Some((matched, item)) =
                    pane.update(cx, |pane, _| pane.take_trigger_interaction())
                {
                    self.handle_terminal_trigger_interaction(
                        pane_id, session_id, owner, matched, item, cx,
                    );
                }
            }
            TerminalPaneEvent::TriggerMatchesAvailable => {
                let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() else {
                    return;
                };
                let pane_owner = pane.downgrade();
                let matches = pane.update(cx, |pane, _cx| pane.take_trigger_matches());
                self.handle_terminal_trigger_matches(pane_id, session_id, pane_owner, matches, cx);
            }
            TerminalPaneEvent::ExternalPathsDropped => {
                let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() else {
                    return;
                };
                let paths = pane.update(cx, |pane, _cx| pane.take_dropped_external_paths());
                self.handle_sftp_terminal_drop(pane_id, session_id, paths, cx);
            }
            TerminalPaneEvent::ClipboardImagePasted => {
                let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() else {
                    return;
                };
                let Some(image) = pane.update(cx, |pane, _cx| pane.take_pasted_clipboard_image())
                else {
                    return;
                };
                self.handle_sftp_terminal_clipboard_image_paste(pane_id, session_id, image, cx);
            }
            TerminalPaneEvent::CurrentDirectoryChanged => {
                if self.active_pane_id(cx) == Some(pane_id) {
                    self.sync_active_terminal_metadata_context(cx);
                }
                if self
                    .terminal
                    .read(cx)
                    .last_ssh_terminal_panes
                    .values()
                    .any(|pane| *pane == pane_id)
                    && (!self.sftp_pages.is_empty() || self.embedded_sftp_node_id.is_some())
                {
                    cx.notify();
                }
            }
            TerminalPaneEvent::RecordingStatusChanged => {
                if self.active_pane_id(cx) == Some(pane_id) {
                    self.sync_active_terminal_recording_elapsed_tick(cx);
                }
            }
            TerminalPaneEvent::SessionLogStatusChanged => {
                if self.active_pane_id(cx) == Some(pane_id) {
                    cx.notify();
                }
            }
            TerminalPaneEvent::SearchStatusChanged => {
                if let Some(search) = self.search.panes.get_mut(&pane_id)
                    && search.visible
                    && let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id)
                {
                    search.sync_from_terminal(pane.read(cx).search_status());
                    cx.notify();
                }
            }
            TerminalPaneEvent::SerialRuntimeOptionsChanged { options } => {
                // Serial profiles persist line-ending changes only; BLE persists
                // all five fields (line endings, send/display mode, local echo).
                let input = options.line_ending;
                let output = options.output_line_ending;
                let Some(saved_connection) = self.terminal_saved_connection_refs.get(&session_id)
                else {
                    return;
                };
                match saved_connection.kind {
                    oxideterm_terminal_triggers::SavedConnectionKind::Serial => {
                        let profile_id = saved_connection.id.clone();
                        match self.connection_store.set_serial_profile_line_endings(
                            &profile_id,
                            Some(serial_profile_line_ending(input)),
                            Some(serial_profile_line_ending(output)),
                        ) {
                            Ok(true) => self.queue_cloud_sync_dirty_refresh(cx),
                            Ok(false) => {}
                            Err(error) => {
                                tracing::warn!(%profile_id, %error, "failed to persist Serial runtime options")
                            }
                        }
                    }
                    oxideterm_terminal_triggers::SavedConnectionKind::Ble => {
                        let profile_id = saved_connection.id.clone();
                        match self.connection_store.set_ble_profile_runtime_options(
                            &profile_id,
                            Some(serial_profile_line_ending(input)),
                            Some(serial_profile_line_ending(output)),
                            Some(serial_profile_send_mode(options.send_mode)),
                            Some(serial_profile_display_mode(options.display_mode)),
                            Some(options.local_echo),
                        ) {
                            Ok(true) => self.queue_cloud_sync_dirty_refresh(cx),
                            Ok(false) => {}
                            Err(error) => {
                                tracing::warn!(%profile_id, %error, "failed to persist BLE runtime options")
                            }
                        }
                    }
                    _ => {}
                }
            }
            TerminalPaneEvent::TelnetLineEndingsChanged { input, output } => {
                self.persist_telnet_line_endings(session_id, input, output, cx);
            }
            TerminalPaneEvent::ToolbarActionInvoked { id } => {
                // Spec ids are "plugin_id:action_id"; an unknown pair means
                // the plugin stopped declaring the chip, so drop the click.
                let Some((plugin_id, action_id)) = id.split_once(':') else {
                    return;
                };
                let Some(definition) = self.plugin_toolbar_definition(plugin_id, action_id, cx)
                else {
                    return;
                };
                // The toolbar chip stops mouse propagation, so the container
                // click handler that normally activates and focuses the pane
                // never runs. Clicking the chip is a pane-level interaction:
                // activate the clicked pane first so the synchronous overlay
                // handover (anchor claim and render gate) targets the pane
                // that owns the click; keyboard focus lands on the next frame
                // through needs_active_pane_focus.
                let tab_id = self
                    .tab_host
                    .read(cx)
                    .tabs()
                    .iter()
                    .find(|tab| {
                        tab.root_pane
                            .as_ref()
                            .is_some_and(|root| root.contains_pane(pane_id))
                    })
                    .map(|tab| tab.id);
                if let Some(tab_id) = tab_id {
                    self.tab_host
                        .update(cx, |host, _| host.set_active_pane(Some(tab_id), pane_id));
                    self.needs_active_pane_focus = true;
                }
                if !self.native_plugin_available(plugin_id, cx) {
                    return;
                }
                // The plugin owns the state semantics. Wake its runtime so
                // the first click (every pane at the default state, runtime
                // stopped) is still delivered, then hand the click over as an
                // event; the plugin reports the resolved state back through
                // toolbar.reportActionState.
                let resolver = self.native_plugin_host_api_resolver(cx);
                self.plugin_entity.update(cx, |plugins, _cx| {
                    let _ = plugins.start_runtime_plugin_activation(plugin_id, resolver);
                });
                let current = self
                    .plugin_toolbar_state(plugin_id, action_id, pane_id)
                    .cloned()
                    .unwrap_or_else(|| definition.default_state.clone());
                self.dispatch_native_plugin_event(
                    plugin_id.to_string(),
                    plugin_host::NATIVE_PLUGIN_TOOLBAR_ACTION_CLICKED_EVENT,
                    serde_json::json!({
                        "actionId": action_id,
                        "paneId": pane_id.0,
                        "currentState": current,
                    }),
                    cx,
                );
            }
            TerminalPaneEvent::PrivilegePromptStateChanged => {
                if self.active_pane_id(cx) == Some(pane_id)
                    && self.sync_active_privilege_prompt_inline_hint(cx)
                {
                    cx.notify();
                }
            }
            TerminalPaneEvent::PrivilegePromptSubmitRequested => self
                .deliver_terminal_pane_interaction(
                    pane_id,
                    window_handle,
                    TerminalPaneInteraction::PrivilegePromptSubmit,
                    cx,
                ),
            TerminalPaneEvent::ContextActionRequested => self.deliver_terminal_pane_interaction(
                pane_id,
                window_handle,
                TerminalPaneInteraction::ContextAction,
                cx,
            ),
        }
    }

    fn persist_telnet_line_endings(
        &mut self,
        session_id: TerminalSessionId,
        input: Option<oxideterm_terminal::SerialLineEnding>,
        output: Option<oxideterm_terminal::SerialLineEnding>,
        cx: &mut Context<Self>,
    ) {
        self.standalone_connections
            .set_telnet_line_endings(session_id, input, output);
        let Some(saved) = self
            .terminal_saved_connection_refs
            .get(&session_id)
            .filter(|saved| saved.kind == SavedConnectionKind::Telnet)
        else {
            return;
        };
        let profile_id = saved.id.clone();
        match self.connection_store.set_telnet_profile_line_endings(
            &profile_id,
            input.map(serial_profile_line_ending),
            output.map(serial_profile_line_ending),
        ) {
            Ok(true) => self.queue_cloud_sync_dirty_refresh(cx),
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(%profile_id, %error, "failed to persist Telnet line endings")
            }
        }
    }

    fn deliver_terminal_pane_interaction(
        &mut self,
        pane_id: PaneId,
        window_handle: AnyWindowHandle,
        interaction: TerminalPaneInteraction,
        cx: &mut Context<Self>,
    ) {
        // Defer the window-scoped action without putting secrets or selected text in the event.
        cx.spawn(async move |weak, cx| {
            let _ = cx.update_window(window_handle, |_, window, cx| {
                weak.update(cx, |workspace, cx| {
                    if workspace.active_pane_id(cx) != Some(pane_id) {
                        // A request cannot follow focus into another pane.
                        if let Some(pane) =
                            workspace.tab_host.read(cx).panes().get(&pane_id).cloned()
                        {
                            pane.update(cx, |pane, _cx| match interaction {
                                TerminalPaneInteraction::PrivilegePromptSubmit => {
                                    pane.take_privilege_prompt_submit_request();
                                }
                                TerminalPaneInteraction::ContextAction => {
                                    pane.take_context_action_request();
                                    pane.take_archive_path_request();
                                }
                            });
                        }
                        return;
                    }

                    let handled = match interaction {
                        TerminalPaneInteraction::PrivilegePromptSubmit => {
                            workspace.handle_active_privilege_prompt_submit_request(window, cx)
                        }
                        TerminalPaneInteraction::ContextAction => workspace
                            .handle_terminal_context_action_request_for_pane(pane_id, window, cx),
                    };
                    if handled {
                        cx.notify();
                    }
                })
            });
        })
        .detach();
    }

    pub(super) fn bind_terminal_location(
        &mut self,
        tab_id: TabId,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) {
        self.tab_host.update(cx, |tab_host, _cx| {
            tab_host.bind_terminal_location(session_id, TerminalLocation { tab_id, pane_id });
        });
        self.refresh_terminal_trigger_pane(pane_id, cx);
        self.debug_assert_terminal_location(session_id, cx);
    }

    fn debug_assert_terminal_location(&self, session_id: TerminalSessionId, cx: &App) {
        // Release builds compile out the invariant checks; consume the ID so
        // release packaging remains warning-free.
        #[cfg(not(debug_assertions))]
        let _ = (session_id, cx);
        #[cfg(debug_assertions)]
        if let Some(location) = self.tab_host.read(cx).terminal_location(session_id) {
            let tree_location = self.tab_by_id(location.tab_id, cx).and_then(|tab| {
                tab.root_pane
                    .as_ref()
                    .and_then(|root| root.pane_id_for_session(session_id))
            });
            debug_assert_eq!(tree_location, Some(location.pane_id));
            debug_assert!(
                self.tab_host
                    .read(cx)
                    .panes()
                    .contains_key(&location.pane_id)
            );
        }
    }

    pub(super) fn remove_terminal_pane(
        &mut self,
        pane_id: &PaneId,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Entity<TerminalPane>> {
        // Capture the session id up front: the GATT registry cleanup below
        // keys on it, and tab_host unbinds the pane->session mapping at the
        // end of this funnel, so a late lookup could silently miss.
        let session_id = self
            .tab_host
            .read(cx)
            .terminal_session_id_for_pane(*pane_id);
        self.release_shell_ide_for_pane(*pane_id, cx);
        self.clear_workspace_tooltip(&format!("terminal-timestamp-{}", pane_id.0), cx);
        if let Some(terminal) = self
            .tab_host
            .read(cx)
            .panes()
            .get(pane_id)
            .map(|pane| pane.read(cx).shared_session())
        {
            self.close_plugin_connection_terminal(&terminal, cx);
        }
        // Drop the BLE GATT registry entry with the pane. Notification pumps
        // observe the session worker's drop and tear down their routes
        // themselves, so no explicit cancellation is needed here.
        if let Some(session_id) = session_id {
            self.ble_gatt_handles
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&session_id);
        }
        // Toolbar states are pane-anchored; dropping them releases the
        // overlay anchor and re-derives the affected plugin runtimes.
        self.plugin_toolbar_pane_closed(pane_id, cx);
        if self
            .ai
            .state
            .read(cx)
            .terminal_inline_panel()
            .target
            .is_some_and(|(id, _)| id == *pane_id)
        {
            self.ai
                .state
                .update(cx, |ai, _| ai.close_terminal_inline_panel());
        }
        if self.search.focused == Some(*pane_id) {
            self.ime_marked_text = None;
            self.clear_ime_selection();
        }
        self.search.remove(*pane_id);
        self.terminal.update(cx, |terminal, _| {
            terminal.sync_groups_mut().remove(*pane_id);
            terminal
                .last_ssh_terminal_panes
                .retain(|_, pane| pane != pane_id);
        });
        self.tab_host
            .update(cx, |tab_host, _cx| tab_host.remove_terminal_pane(*pane_id))
    }

    pub(super) fn queue_auto_close_terminal_session(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) {
        if !self.terminal_session_auto_closes_on_exit(session_id, cx) {
            return;
        }
        if self.pending_auto_close_terminal_sessions.insert(session_id) {
            cx.notify();
        }
    }

    fn terminal_session_auto_closes_on_exit(
        &self,
        session_id: TerminalSessionId,
        cx: &App,
    ) -> bool {
        self.tab_host
            .read(cx)
            .terminal_location(session_id)
            .and_then(|location| self.terminal_kind_for_pane(location.pane_id, cx))
            .is_some_and(terminal_auto_closes_on_exit)
    }

    pub(super) fn schedule_pending_auto_close_terminal_sessions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_auto_close_terminal_sessions.is_empty()
            || self.auto_close_terminal_sessions_scheduled
        {
            return;
        }
        self.auto_close_terminal_sessions_scheduled = true;
        let workspace = cx.entity();
        window.on_next_frame(move |window, cx| {
            let _ = workspace.update(cx, |this, cx| {
                this.auto_close_terminal_sessions_scheduled = false;
                this.drain_pending_auto_close_terminal_sessions(window, cx);
            });
        });
    }

    fn drain_pending_auto_close_terminal_sessions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session_ids: Vec<_> = self.pending_auto_close_terminal_sessions.drain().collect();
        for session_id in session_ids {
            if !self.terminal_session_auto_closes_on_exit(session_id, cx) {
                continue;
            }
            self.close_terminal_session(session_id, window, cx);
        }
    }

    pub(super) fn can_split_active_pane(&self, cx: &App) -> bool {
        self.active_pane_split_target(cx).is_some()
    }

    fn active_pane_split_target(&self, cx: &App) -> Option<ActivePaneSplitTarget> {
        let (tab_id, active_pane_id, pane_count) = self.active_tab(cx).and_then(|tab| {
            Some((
                tab.id,
                tab.active_pane_id?,
                tab.root_pane.as_ref()?.pane_count(),
            ))
        })?;
        if pane_count >= MAX_PANES_PER_TAB {
            return None;
        }

        let kind = self.terminal_kind_for_pane(active_pane_id, cx)?;
        let node = self.active_ssh_terminal_node_id(cx);
        if !terminal_split_supported(
            kind,
            node.as_ref()
                .is_some_and(|id| self.node_is_ready_for_terminal(id)),
        ) {
            return None;
        }
        let source = match kind {
            oxideterm_terminal::TerminalSessionKind::LocalPty => TerminalSplitSource::Local,
            oxideterm_terminal::TerminalSessionKind::SshPty => {
                let node = self.active_ssh_terminal_node_id(cx)?;
                if !self.node_is_ready_for_terminal(&node) {
                    return None;
                }
                TerminalSplitSource::Ssh(node)
            }
            _ => return None,
        };
        Some(ActivePaneSplitTarget {
            tab_id,
            active_pane_id,
            source,
        })
    }

    pub(super) fn split_active_pane(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.active_pane_split_target(cx) else {
            return;
        };
        if let TerminalSplitSource::Ssh(node_id) = &target.source {
            let node_id = node_id.clone();
            self.split_ssh_terminal_pane(target, node_id, direction, window, cx);
            return;
        }

        let group_id = self.alloc_pane_id(cx);
        let pane_id = self.alloc_pane_id(cx);
        let session_id = self.alloc_session_id(cx);
        let mut preferences =
            self.prepare_terminal_preferences_for_tab_kind(&TabKind::LocalTerminal, cx);
        let host = self.tab_host.read(cx);
        let source_instance = host
            .tab_by_id(target.tab_id)
            .and_then(|tab| tab.root_pane.as_ref())
            .and_then(|root| root.session_id_for_pane(target.active_pane_id))
            .and_then(|session_id| host.local_sessions.get(&session_id))
            .cloned();
        let mut local_config = self.local_terminal_config();
        if let Some(instance) = &source_instance {
            local_config.shell = instance.shell.clone();
            local_config.cwd = instance.cwd.clone();
            local_config.remove_env = instance.remove_env.clone();
        }
        if let Some(pane) = self.tab_host.read(cx).panes().get(&target.active_pane_id) {
            if let Some(snapshot) = terminal_cwd::terminal_cwd_snapshot_from_pane(
                oxideterm_environment::CurrentDirectoryScope::Local,
                pane.read(cx),
            ) {
                local_config.cwd = Some(std::path::PathBuf::from(snapshot.path()));
            }
        }
        let instance = source_instance.unwrap_or_else(|| {
            local_sessions::LocalTerminalInstance::new(
                &local_config,
                self.local_terminal_tab_title(),
            )
        });
        let local_preference_overrides =
            self.terminal_preference_overrides_for_local_shell(local_config.shell.as_ref());
        local_preference_overrides.apply_to(&mut preferences);
        let shared_session = match TerminalPane::local_shared_session(local_config, &preferences) {
            Ok(session) => session,
            Err(error) => {
                self.connections.session_manager.update(cx, |manager, cx| {
                    manager.set_status(Some(error.to_string()), cx)
                });
                return;
            }
        };
        let pane = cx.new(|cx| {
            TerminalPane::from_shared_session(shared_session, preferences, window, cx)
                .expect("failed to initialize split terminal view")
                .with_preference_overrides(local_preference_overrides)
        });

        if self.tab_host.update(cx, |tab_host, _| {
            tab_host.split_pane(
                target.tab_id,
                target.active_pane_id,
                group_id,
                direction,
                pane_id,
                session_id,
            )
        }) {
            self.tab_host.update(cx, |host, _| {
                host.local_sessions.insert(session_id, instance);
            });
            self.register_terminal_pane(pane_id, session_id, pane.clone(), window, cx);
            self.bind_terminal_location(target.tab_id, pane_id, session_id, cx);
            self.activate_embedded_sftp_sidebar_if_visible(cx);
            self.needs_active_pane_focus = true;
            pane.update(cx, |pane, cx| pane.focus(window, cx));
            cx.notify();
        } else {
            let _ = pane.update(cx, |pane, _cx| pane.shutdown());
        }
    }

    fn split_ssh_terminal_pane(
        &mut self,
        target: ActivePaneSplitTarget,
        node_id: NodeId,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let group_id = self.alloc_pane_id(cx);
        let Ok((pane_id, session_id)) =
            self.create_ssh_terminal_pane_for_existing_node(&node_id, None, true, window, cx)
        else {
            return;
        };
        let mounted = self.tab_host.update(cx, |tab_host, _| {
            tab_host.split_pane(
                target.tab_id,
                target.active_pane_id,
                group_id,
                direction,
                pane_id,
                session_id,
            )
        });
        if mounted {
            self.bind_terminal_location(target.tab_id, pane_id, session_id, cx);
            self.activate_embedded_sftp_sidebar_if_visible(cx);
            self.needs_active_pane_focus = true;
            self.focus_active_pane(window, cx);
            cx.notify();
        } else {
            self.rollback_unmounted_ssh_terminal_pane(pane_id, session_id, cx);
        }
    }

    fn rollback_unmounted_ssh_terminal_pane(
        &mut self,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) {
        // The node remains owned by NodeRouter; only the failed pane's projections and consumer
        // are revoked before its session is shut down.
        self.pending_auto_close_terminal_sessions
            .remove(&session_id);
        self.terminal_saved_connection_refs.remove(&session_id);
        self.clear_terminal_trigger_session_overrides(session_id);
        self.unregister_ssh_terminal_session(session_id, cx);
        if let Some(pane) = self.remove_terminal_pane(&pane_id, cx) {
            let _ = pane.update(cx, |pane, _cx| pane.shutdown());
        }
    }

    pub(super) fn close_active_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((tab_id, active_pane_id, pane_count)) = self.active_tab(cx).and_then(|tab| {
            let active_pane_id = tab.active_pane_id?;
            let root_pane = tab.root_pane.as_ref()?;
            Some((tab.id, active_pane_id, root_pane.pane_count()))
        }) else {
            return;
        };
        if pane_count <= 1 {
            return;
        }

        if let Some(page) = self
            .tab_by_id(tab_id, cx)
            .and_then(|tab| tab.root_pane.as_ref()?.page_id_for_pane(active_pane_id))
        {
            self.request_close_tab_by_id(page, window, cx);
        } else {
            self.request_close_terminal_pane(tab_id, active_pane_id, window, cx);
        }
    }

    pub(in crate::workspace) fn close_terminal_pane_in_tab(
        &mut self,
        tab_id: TabId,
        active_pane_id: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((_, surface)) = self
            .ide_workspace
            .read(cx)
            .shell_surface_for_pane(active_pane_id)
        {
            surface.update(cx, |surface, cx| surface.request_shell_return(cx));
            return;
        }
        let Some(root) = self
            .tab_by_id(tab_id, cx)
            .and_then(|tab| tab.root_pane.as_ref())
        else {
            return;
        };
        let Some(session_id) = root.session_id_for_pane(active_pane_id) else {
            return;
        };
        if root.pane_count() == 1 {
            self.close_tab_by_id(tab_id, window, cx);
            return;
        }
        let session_id = Some(session_id);
        if let Some(session_id) = session_id {
            self.standalone_connections.release_surface(
                standalone_connections::StandaloneConnectionSurface::Terminal(session_id),
            );
            self.release_public_mcp_terminal_for_closed_session(session_id, cx);
            self.serial_terminal_configs.remove(&session_id);
            self.telnet_terminal_profile_ids.remove(&session_id);
            self.terminal_saved_connection_refs.remove(&session_id);
            self.clear_terminal_trigger_session_overrides(session_id);
            self.unregister_ssh_terminal_session(session_id, cx);
        }
        self.sync_session_retention(cx);

        if let Some(pane) = self.remove_terminal_pane(&active_pane_id, cx) {
            let _ = pane.update(cx, |pane, _cx| pane.shutdown());
        }

        if self
            .tab_host
            .update(cx, |tab_host, _| {
                tab_host.close_pane(tab_id, active_pane_id)
            })
            .is_some()
        {
            self.sync_collapsed_workspace_tabs(cx);
            if self.active_tab_id(cx) == Some(tab_id) {
                self.sync_active_tab_surface(cx);
            }
            if self.active_tab_id(cx) == Some(tab_id) || self.tab_host.read(cx).is_detached(tab_id)
            {
                self.focus_tab_terminal(tab_id, window, cx);
            }
            cx.notify();
        }
    }

    pub(super) fn reset_active_tab_to_single_pane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((tab_id, active_pane_id, root_pane)) = self
            .active_tab(cx)
            .and_then(|tab| Some((tab.id, tab.active_pane_id?, tab.root_pane.as_ref()?.clone())))
        else {
            return;
        };
        if root_pane.pane_count() <= 1 {
            return;
        }
        let mut pane_ids = Vec::new();
        root_pane.collect_pane_ids(&mut pane_ids);
        for pane in pane_ids.into_iter().filter(|id| *id != active_pane_id) {
            if let Some(page) = root_pane.page_id_for_pane(pane) {
                self.close_tab_by_id(page, window, cx);
            } else {
                self.close_terminal_pane_in_tab(tab_id, pane, window, cx);
            }
        }
        self.sync_active_tab_surface(cx);
        self.needs_active_pane_focus = true;
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(super) fn update_split_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(drag) = self.split_drag.as_ref() {
            self.pane_request(drag.resize_at(event.position), window, cx);
        }
    }
    pub(super) fn finish_split_drag(&mut self, cx: &mut Context<Self>) {
        if self.split_drag.take().is_some() {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn split_drag_belongs_to_tab(&self, tab_id: TabId) -> bool {
        self.split_drag
            .as_ref()
            .is_some_and(|drag| drag.tab_id == tab_id)
    }

    fn toggle_terminal_pane_zoom(
        &mut self,
        tab_id: TabId,
        pane_id: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .tab_host
            .update(cx, |host, _| host.toggle_pane_zoom(tab_id, pane_id))
        {
            self.finish_split_drag(cx);
            self.focus_tab_terminal(tab_id, window, cx);
            cx.notify();
        }
    }

    pub(super) fn render_pane_tree(
        &mut self,
        node: &PaneNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_pane_tree_for_tab(self.active_tab_id(cx), node, window, cx)
    }

    fn move_workspace_page_out(
        &mut self,
        page: TabId,
        new_window: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(container) = self.tab_host.update(cx, |host, _| host.unembed_page(page)) else {
            return;
        };
        if self
            .tab_by_id(container, cx)
            .is_some_and(|tab| tab.root_pane.is_none())
        {
            self.close_tab_by_id(container, window, cx);
        }
        self.sync_collapsed_workspace_tabs(cx);
        self.set_main_window_active_tab(Some(page), cx);
        self.sync_active_tab_surface(cx);
        self.sync_ide_surface_mount(page, cx);
        if new_window {
            self.detach_tab_to_window(page, None, window, cx);
        } else if let Some(main) = self
            .window_registry
            .handle_for_role(window_registry::WindowRole::Main)
        {
            if main.window_id() != window.window_handle().window_id() {
                let _ = main.update(cx, |_, window, _| window.activate_window());
            }
        }
        cx.notify();
    }

    pub(super) fn render_pane_tree_for_tab(
        &mut self,
        tab_id: Option<TabId>,
        node: &PaneNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(tab) = tab_id else {
            return div().into_any_element();
        };
        let i18n = self.i18n.clone();
        PaneTree {
            tab,
            active: self.tab_by_id(tab, cx).and_then(|t| t.active_pane_id),
            zoomed: self.tab_host.read(cx).zoomed_pane(tab),
            drag: self.split_drag.clone(),
            tokens: self.tokens,
            i18n: &i18n,
            header_background: self.workspace_chrome_background(self.tokens.ui.bg),
            divider: self.workspace_divider_color(self.tokens.ui.divider, SPLIT_HANDLE_LINE_ALPHA),
            supports_detach: true,
            drop_regions: self
                .window_registry
                .handle_for_role(window_registry::WindowRole::Main)
                .filter(|main| main.window_id() == window.window_handle().window_id())
                .map(|_| self.split_drop_regions.clone()),
        }
        .render(node, self, window, cx)
    }
}

impl PaneHost for WorkspaceApp {
    fn mount_pane(
        &mut self,
        _tab: TabId,
        pane_id: PaneId,
        content: PaneContent,
        split: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<MountedPane> {
        match content {
            PaneContent::Page(page_id) => {
                let page = self.tab_by_id(page_id, cx)?.clone();
                let content = if page.kind == TabKind::Knowledge {
                    self.render_knowledge_workspace_surface()
                } else {
                    self.render_tab_content_for_id(page_id, &page.kind, None, window, cx)
                };
                Some(MountedPane {
                    title: page.title,
                    body: content,
                    capture_focus: true,
                    toolbar: None,
                    overlays: Vec::new(),
                })
            }
            PaneContent::Terminal(_) => {
                let title = if split {
                    self.terminal_pane_label(pane_id, cx)
                } else {
                    String::new()
                };
                if let Some((_, surface)) =
                    self.ide_workspace.read(cx).shell_surface_for_pane(pane_id)
                {
                    return Some(MountedPane {
                        title,
                        body: surface.into_any_element(),
                        capture_focus: true,
                        toolbar: None,
                        overlays: Vec::new(),
                    });
                }
                let pane = self.tab_host.read(cx).panes().get(&pane_id)?.clone();
                let toolbar = self
                    .render_terminal_sync_member_header(pane_id, cx)
                    .map(|bar| (bar, terminal_command_bar::TERMINAL_SYNC_HEADER_HEIGHT));
                let mut overlays = Vec::new();
                let inline = self.ai.state.read(cx).terminal_inline_panel();
                if inline.open && inline.target.is_some_and(|(id, _)| id == pane_id) {
                    overlays.push(self.ai.inline_panel.clone().into_any_element())
                }
                if self
                    .search
                    .panes
                    .get(&pane_id)
                    .is_some_and(|search| search.visible || search.exiting)
                {
                    overlays.push(self.render_search_bar(pane_id, cx))
                }
                Some(MountedPane {
                    title,
                    body: pane.into_any_element(),
                    capture_focus: false,
                    toolbar,
                    overlays,
                })
            }
        }
    }
    fn pane_request(
        &mut self,
        request: ShellPaneRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match request {
            ShellPaneRequest::Drop { source, target } => {
                if self.tab_host.read(cx).is_detached(source) {
                    self.return_detached_tab_to_main(source, window, cx);
                }
                let (direction, before) = target.edge.placement();
                return self.combine_tabs_at(
                    source,
                    target.tab,
                    target.pane,
                    direction,
                    before,
                    window,
                    cx,
                );
            }
            ShellPaneRequest::StartResize(drag) => {
                self.split_drag = Some(drag);
                cx.notify()
            }
            ShellPaneRequest::Reset(tab, group) => {
                if self
                    .tab_host
                    .update(cx, |host, _| host.reset_group_sizes(Some(tab), group))
                {
                    cx.notify()
                }
            }
            ShellPaneRequest::Resize(tab, group, sizes) => {
                if self.tab_host.update(cx, |host, _| {
                    host.update_group_sizes(Some(tab), group, &sizes)
                }) {
                    cx.notify()
                }
            }
            ShellPaneRequest::Swap(tab, source, target) => {
                if self
                    .tab_host
                    .update(cx, |host, _| host.swap_panes(tab, source, target))
                {
                    cx.notify()
                }
            }
            ShellPaneRequest::ToggleZoom(tab, pane) => {
                self.toggle_terminal_pane_zoom(tab, pane, window, cx)
            }
            ShellPaneRequest::MoveOut(_, _, PaneContent::Page(page), detach) => {
                self.move_workspace_page_out(page, detach, window, cx)
            }
            ShellPaneRequest::MoveOut(tab, pane, _, detach) => {
                self.move_terminal_pane_out(tab, pane, detach, window, cx)
            }
            ShellPaneRequest::ClosePane(_, _, PaneContent::Page(page)) => {
                self.request_close_tab_by_id(page, window, cx)
            }
            ShellPaneRequest::ClosePane(tab, pane, _) => {
                self.request_close_terminal_pane(tab, pane, window, cx)
            }
            ShellPaneRequest::Focus(container, pane_id, PaneContent::Page(page_id)) => {
                let previous = self.tab_host.read(cx).focused_page_id(container);
                if previous != page_id {
                    self.release_remote_desktop_inputs_for_tab(previous, cx);
                }
                if self
                    .tab_host
                    .update(cx, |host, _| host.set_active_pane(Some(container), pane_id))
                {
                    self.blur_text_inputs(cx);
                    if !self.tab_host.read(cx).is_detached(container) {
                        self.sync_active_tab_surface(cx);
                    }
                }
                cx.notify();
            }
            ShellPaneRequest::Focus(tab, pane_id, PaneContent::Terminal(_))
                if self
                    .ide_workspace
                    .read(cx)
                    .shell_surface_for_pane(pane_id)
                    .is_some() =>
            {
                let changed = self
                    .tab_host
                    .update(cx, |host, _| host.set_active_pane(Some(tab), pane_id));
                if changed {
                    self.blur_text_inputs(cx);
                    if !self.tab_host.read(cx).is_outside_main_window(tab) {
                        self.sync_active_tab_surface(cx);
                    }
                }
                cx.notify();
            }
            ShellPaneRequest::Focus(tab_id, pane_id, PaneContent::Terminal(_)) => {
                // Release logical input ownership before focusing the native pane.
                self.blur_text_inputs(cx);
                self.release_remote_desktop_inputs_for_tab(tab_id, cx);
                self.tab_host.update(cx, |tab_host, _| {
                    tab_host.set_active_pane(Some(tab_id), pane_id);
                });
                if !self.tab_host.read(cx).is_outside_main_window(tab_id) {
                    self.set_main_window_active_tab(Some(tab_id), cx);
                }
                if !self.tab_host.read(cx).is_detached(tab_id) {
                    self.sync_active_tab_surface(cx);
                    self.sync_active_terminal_metadata_context(cx);
                    self.sync_active_terminal_recording_elapsed_tick(cx);
                    self.sync_active_privilege_prompt_inline_hint(cx);
                } else {
                    self.remember_ssh_terminal_pane(pane_id, cx);
                }
                if let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() {
                    pane.update(cx, |pane, cx| pane.focus(window, cx));
                }
                cx.notify();
            }
        }
        true
    }
}
fn x11_forwarding_notice(
    status: oxideterm_ssh::X11ForwardingStatus,
    i18n: &I18n,
) -> TerminalNotice {
    use oxideterm_ssh::{X11ForwardingFailure, X11ForwardingStatus};
    let (title, description, variant) = match status {
        X11ForwardingStatus::Accepted => (
            "ssh.x11.accepted",
            "ssh.x11.accepted_description",
            TerminalNoticeVariant::Success,
        ),
        X11ForwardingStatus::Unavailable(failure) => (
            "ssh.x11.unavailable",
            match failure {
                X11ForwardingFailure::LocalDisplayUnavailable => {
                    "ssh.x11.local_display_unavailable"
                }
                X11ForwardingFailure::LocalAuthorizationUnavailable => {
                    "ssh.x11.local_authorization_unavailable"
                }
                X11ForwardingFailure::ServerRejected => "ssh.x11.server_rejected",
                X11ForwardingFailure::RequestTimedOut => "ssh.x11.request_timed_out",
                X11ForwardingFailure::ChannelUnavailable => "ssh.x11.channel_unavailable",
            },
            TerminalNoticeVariant::Warning,
        ),
    };
    TerminalNotice {
        title: i18n.t(title),
        description: Some(i18n.t(description)),
        status_text: matches!(status, X11ForwardingStatus::Unavailable(_))
            .then(|| i18n.t("settings_view.connections.shell_integration.terminal_unaffected")),
        variant,
        progress: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x11_notices_distinguish_acceptance_and_optional_forwarding_failures() {
        use oxideterm_ssh::{X11ForwardingFailure, X11ForwardingStatus};
        let i18n = I18n::new(oxideterm_i18n::Locale::En);
        let accepted = x11_forwarding_notice(X11ForwardingStatus::Accepted, &i18n);
        assert_eq!(accepted.title, "X11 forwarding accepted");
        assert_eq!(accepted.variant, TerminalNoticeVariant::Success);
        assert_eq!(accepted.status_text, None);
        for (failure, description) in [
            (
                X11ForwardingFailure::LocalDisplayUnavailable,
                "No valid local display address was found. Start a local X server and configure DISPLAY before starting OxideTerm.",
            ),
            (
                X11ForwardingFailure::ServerRejected,
                "The SSH server rejected X11 forwarding. Check the server forwarding policy and xauth installation.",
            ),
            (
                X11ForwardingFailure::RequestTimedOut,
                "The SSH server did not respond to the X11 forwarding request in time.",
            ),
        ] {
            let notice = x11_forwarding_notice(X11ForwardingStatus::Unavailable(failure), &i18n);
            assert_eq!(notice.title, "X11 forwarding unavailable");
            assert_eq!(notice.description.as_deref(), Some(description));
            assert_eq!(notice.variant, TerminalNoticeVariant::Warning);
            assert_eq!(
                notice.status_text.as_deref(),
                Some("SSH terminal use is unaffected.")
            );
        }
    }

    #[test]
    fn remote_terminal_exit_does_not_request_auto_close() {
        use oxideterm_terminal::TerminalSessionKind::*;
        for (kind, auto_close) in [
            (LocalPty, true),
            (SshPty, false),
            (Telnet, false),
            (Mosh, false),
            (Serial, false),
        ] {
            assert_eq!(terminal_auto_closes_on_exit(kind), auto_close, "{kind:?}");
        }
    }

    #[test]
    fn terminal_split_support_matches_transport_ownership() {
        use oxideterm_terminal::TerminalSessionKind::*;
        for (kind, ready, supported) in [
            (LocalPty, false, true),
            (SshPty, true, true),
            (SshPty, false, false),
            (Serial, true, false),
            (Telnet, true, false),
            (Mosh, true, false),
        ] {
            assert_eq!(
                terminal_split_supported(kind, ready),
                supported,
                "{kind:?}, ready={ready}"
            );
        }
    }
}
