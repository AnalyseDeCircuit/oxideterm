use oxideterm_ai::tool_resources::{
    AiForwardOperation, AiPluginAction, AiTransportLifecycle, AiSerialSessionCapture, ai_action_accepted, ai_forwarding_operation, ai_credential_scopes, ai_list_credential_metadata, ai_list_transport_profiles,
    ai_plugin_action, ai_plugin_action_accepted, AiCredentialAction, AiCredentialKind, AiSerialControl,
    ai_credential_action, ai_serial_control, ai_transport_session_action_accepted,
};

// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

impl WorkspaceApp {
    fn ai_node_for_tool_authority(
        &self,
        tool_session_id: &ToolSessionId,
        arguments: &serde_json::Value,
        cx: &App,
    ) -> Result<NodeId, oxideterm_ai::RuntimeValidationError> {
        if let Some(handle_id) = arguments
            .get("handle_id")
            .and_then(serde_json::Value::as_str)
        {
            return self
                .ai.authority
                .read(cx)
                .validate_node_handle(tool_session_id, Some(handle_id));
        }
        let resource_id = arguments
            .pointer("/resource_ref/id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                oxideterm_ai::RuntimeValidationError::new(
                    oxideterm_ai::RuntimeValidationFailure::CapabilityUnavailable,
                )
            })?;
        let connection = self.connection_store.get(resource_id).ok_or_else(|| {
            oxideterm_ai::RuntimeValidationError::new(
                oxideterm_ai::RuntimeValidationFailure::OwnerClosed,
            )
        })?;
        // Stable saved-connection authority must resolve through the same route
        // and owner checks as terminal opens, never through a stale index alone.
        self.indexed_saved_ssh_node_for_connection(resource_id, connection)
            .ok_or_else(|| {
                oxideterm_ai::RuntimeValidationError::new(
                    oxideterm_ai::RuntimeValidationFailure::OwnerClosed,
                )
            })
    }

    fn ai_node_connection_id(&self, node_id: &NodeId) -> Result<String, String> {
        self.node_router
            .resolve_connection_now(node_id)
            .map(|connection| connection.connection_id)
            .map_err(|_| "The selected SSH node is not currently connected.".to_string())
    }

    pub(in crate::workspace) fn execute_ai_inspect_host_tools(
        &mut self,
        tool_session_id: &ToolSessionId,
        arguments: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        let node_id = self
            .ai_node_for_tool_authority(tool_session_id, arguments, cx)
            .map_err(|_| "The Host Tools node capability is no longer available.".to_string())?;
        let connection_id = self.ai_node_connection_id(&node_id)?;
        let resource = arguments
            .get("resource")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("overview");
        let refresh_requested = arguments
            .get("refresh")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let snapshot = oxideterm_ai::sanitize_json_for_ai(
            &self.host_tools.read(cx).ai_snapshot(&connection_id, resource),
        );
        let refresh_accepted = refresh_requested
            && self.host_tools.update(cx, |host_tools, cx| {
                host_tools.request_ai_snapshot_refresh(connection_id.clone(), resource, cx)
            });
        Ok(serde_json::json!({
            "node": {
                "state": "connected",
            },
            "resource": resource,
            "refreshRequested": refresh_requested,
            "refreshAccepted": refresh_accepted,
            "snapshot": snapshot,
        }))
    }

    pub(in crate::workspace) fn execute_ai_list_forwards(
        &self,
        tool_session_id: &ToolSessionId,
        arguments: &serde_json::Value,
        cx: &App,
    ) -> Result<serde_json::Value, String> {
        let node_id = self
            .ai_node_for_tool_authority(tool_session_id, arguments, cx)
            .map_err(|_| "The forwarding node capability is no longer available.".to_string())?;
        Ok(self.forwarding.read(cx).ai_snapshot_for_node(&node_id))
    }

    pub(in crate::workspace) fn execute_ai_control_host_tool(
        &mut self,
        tool_session_id: &ToolSessionId,
        arguments: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        let node_id = self
            .ai_node_for_tool_authority(tool_session_id, arguments, cx)
            .map_err(|_| "The Host Tools node capability is no longer available.".to_string())?;
        let connection_id = self.ai_node_connection_id(&node_id)?;
        let resource = arguments
            .get("resource")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A Host Tools resource is required.".to_string())?;
        let action = arguments
            .get("action")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A Host Tools action is required.".to_string())?;
        let entity_id = arguments
            .get("entity_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A Host Tools entity identifier is required.".to_string())?;
        let value = arguments.get("value").and_then(serde_json::Value::as_str);
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.execute_ai_action(
                connection_id,
                resource,
                action,
                entity_id,
                value,
                cx,
            )
        })?;
        Ok(serde_json::json!({
            "accepted": true,
            "resource": resource,
            "action": action,
            "entityId": entity_id,
        }))
    }

    pub(in crate::workspace) fn execute_ai_manage_forward(
        &mut self,
        tool_session_id: &ToolSessionId,
        arguments: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        let node_id = self
            .ai_node_for_tool_authority(tool_session_id, arguments, cx)
            .map_err(|_| "The forwarding node capability is no longer available.".to_string())?;
        if !self.node_is_ready_for_forwarding(&node_id) {
            return Err("The selected SSH node is not ready for forwarding.".to_string());
        }
        let operation = ai_forwarding_operation(arguments)?;
        let action = operation.action();
        let (operation, message_key, sync_saved_forwards) = match operation {
            AiForwardOperation::Scan => {
                self.start_port_scan(node_id, true, cx);
                return Ok(ai_action_accepted(action));
            }
            AiForwardOperation::Create { rule } => (forwards::ForwardingRuntimeOperation::Create { rule, check_health: true }, "forwards.messages.created", true),
            AiForwardOperation::Update { forward_id, update } => (forwards::ForwardingRuntimeOperation::Update { forward_id, update }, "forwards.messages.updated", true),
            AiForwardOperation::Stop { forward_id } => (forwards::ForwardingRuntimeOperation::Stop { forward_id }, "forwards.messages.stopped", false),
            AiForwardOperation::Restart { forward_id } => (forwards::ForwardingRuntimeOperation::Restart { forward_id }, "forwards.messages.restarted", false),
            AiForwardOperation::Delete { forward_id } => (forwards::ForwardingRuntimeOperation::Delete { forward_id }, "forwards.messages.deleted", true),
        };
        let tab_id = self.forwarding.read(cx).tab_for_node(&node_id)
            .or_else(|| self.active_tab_id(cx))
            .ok_or_else(|| "No workspace tab is available for forwarding delivery.".to_string())?;
        self.start_forward_operation(
            tab_id,
            node_id,
            message_key,
            sync_saved_forwards,
            operation,
            cx,
        );
        Ok(ai_action_accepted(action))
    }

    pub(in crate::workspace) fn execute_ai_list_plugins(
        &self,
        cx: &App,
    ) -> serde_json::Value {
        let plugins = self
            .plugin_entity
            .read(cx)
            .registry()
            .plugins()
            .iter()
            .map(|plugin| {
                serde_json::json!({
                    "id": plugin.manifest.id,
                    "name": plugin.manifest.name,
                    "version": plugin.manifest.version,
                    "state": ai_plugin_state_label(plugin.state),
                    "enabled": plugin.config.enabled,
                    "requestedCapabilities": plugin.manifest.permissions.capabilities,
                    "approvedCapabilities": plugin.config.approved_capabilities,
                    "contributions": {
                        "aiTools": plugin.manifest.contributes.as_ref()
                            .and_then(|contributes| contributes.ai_tools.as_ref())
                            .map_or(0, Vec::len),
                        "hostMonitors": plugin.manifest.contributes.as_ref()
                            .and_then(|contributes| contributes.host_monitors.as_ref())
                            .map_or(0, Vec::len),
                        "apiCommands": plugin.manifest.contributes.as_ref()
                            .and_then(|contributes| contributes.api_commands.as_ref())
                            .map_or(0, Vec::len),
                    }
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({ "plugins": plugins })
    }

    pub(in crate::workspace) fn execute_ai_manage_plugin(
        &mut self,
        arguments: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        let (action, operation) = ai_plugin_action(arguments)?;
        let plugin_id = operation.plugin_id();
        match operation {
        AiPluginAction::OpenManager => {
            self.open_plugin_manager_tab(window, cx);
        }
        AiPluginAction::Install { package_url, checksum, overwrite } => {
            let settings_path = self.settings_store.path().to_path_buf();
            if self.plugin_entity.read(cx).manager_operation_in_flight() {
                return Err("Another plugin operation is already running.".to_string());
            }
            let mut retired_desktops = if overwrite {
                self.remote_desktop
                    .update(cx, |desktops, cx| desktops.stop_plugins(None, cx))
            } else {
                Vec::new()
            };
            if overwrite { retired_desktops.push(self.mosh_plugin_sessions.stop()); }
            let accepted = self.plugin_entity.update(cx, |plugins, _cx| {
                plugins.start_package_install(
                    settings_path,
                    None,
                    package_url,
                    checksum,
                    overwrite,
                    retired_desktops,
                )
            });
            if !accepted {
                return Err("Another plugin operation is already running.".to_string());
            }
            cx.notify();
        }
        AiPluginAction::Enable { plugin_id, enabled } => {
                self.plugin_entity
                    .update(cx, |plugins, _cx| {
                        plugins.set_plugin_enabled(plugin_id, enabled, _cx)
                    })
                    .map(|_| {
                        self.bootstrap_native_plugin_runtime(cx);
                    })?;
            }
            AiPluginAction::Uninstall { plugin_id, remove_storage } => {
                if self.plugin_entity.read(cx).manager_operation_in_flight() {
                    return Err("Another plugin operation is already running.".to_string());
                }
                self.plugin_entity.update(cx, |plugins, cx| {
                    plugins.set_plugin_enabled(plugin_id, false, cx)?;
                    plugins.begin_remote_desktop_removal(plugin_id);
                    Ok::<_, String>(())
                })?;
                self.stop_acp_plugin(Some(plugin_id), cx);
                let mut workers = self.remote_desktop.update(cx, |desktops, cx| {
                    desktops.stop_plugins(Some(plugin_id), cx)
                });
                if plugin_id == "com.oxideterm.terminal.mosh" {
                    workers.push(self.mosh_plugin_sessions.stop());
                }
                let plugin_id = plugin_id.to_string();
                let receiver = self.plugin_entity.update(cx, |plugins, cx| {
                    plugins.start_plugin_uninstall(plugin_id.clone(), remove_storage, workers, cx)
                });
                cx.spawn(async move |workspace, cx| {
                    let result = receiver.await;
                    let _ = workspace.update(cx, |workspace, cx| {
                        if let Ok(Err(error)) = result {
                            workspace.plugin_entity.update(cx, |plugins, _cx| {
                                plugins
                                    .registry_mut()
                                    .record_manager_error(plugin_id, error)
                            });
                        }
                        workspace.bootstrap_native_plugin_runtime(cx);
                        cx.notify();
                    });
                })
                .detach();
            }
            AiPluginAction::Invoke { plugin_id, command_id, arguments: command_arguments } => {
                let host_api_resolver = self.native_plugin_host_api_resolver(cx);
                self.plugin_entity.update(cx, |plugins, _cx| {
                    plugins.start_runtime_command_with_arguments(
                        plugin_id.to_string(),
                        command_id.to_string(),
                        command_arguments,
                        host_api_resolver,
                    )
                });
            }
            AiPluginAction::Configure { plugin_id, setting_id, value } => {
                self.plugin_entity.update(cx, |plugins, _cx| {
                    plugins.set_plugin_setting_value(plugin_id, setting_id, value)
                })?;
            }
        }
        if !matches!(action, "open_manager" | "install") { cx.notify(); }
        Ok(ai_plugin_action_accepted(action, plugin_id))
    }

    pub(in crate::workspace) fn execute_ai_list_transport_profiles(&self) -> serde_json::Value {
        ai_list_transport_profiles(self.connection_store.serial_profiles(),
            self.connection_store.telnet_profiles(), self.connection_store.remote_desktop_profiles())
    }

    pub(in crate::workspace) fn execute_ai_open_transport_profile(
        &mut self,
        arguments: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        let transport = arguments
            .get("transport")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A transport is required.".to_string())?;
        let profile_id = arguments
            .get("profile_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A profile identifier is required.".to_string())?;
        match transport {
            "serial"
                if self
                    .connection_store
                    .serial_profiles()
                    .iter()
                    .any(|profile| profile.id == profile_id) =>
            {
                self.open_saved_serial_profile(profile_id, window, cx);
            }
            "telnet"
                if self
                    .connection_store
                    .telnet_profiles()
                    .iter()
                    .any(|profile| profile.id == profile_id) =>
            {
                self.open_saved_telnet_profile(profile_id, window, cx);
            }
            "rdp" | "vnc" => {
                let profile = self
                    .connection_store
                    .get_remote_desktop_profile(profile_id)
                    .ok_or_else(|| "The remote desktop profile no longer exists.".to_string())?;
                let expected_protocol = match transport {
                    "rdp" => oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp,
                    _ => oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc,
                };
                if profile.protocol != expected_protocol {
                    return Err(
                        "The profile protocol does not match the requested transport.".to_string(),
                    );
                }
                self.open_saved_remote_desktop_profile(profile_id, window, cx);
            }
            _ => return Err("The selected transport profile no longer exists.".to_string()),
        }
        Ok(serde_json::json!({
            "accepted": true,
            "transport": transport,
            "profileId": profile_id,
        }))
    }

    pub(in crate::workspace) fn execute_ai_get_transport_session_state(
        &self,
        tool_session_id: &ToolSessionId,
        arguments: &serde_json::Value,
        cx: &App,
    ) -> Result<serde_json::Value, String> {
        let handle_id = arguments
            .get("handle_id")
            .and_then(serde_json::Value::as_str);
        let session_id = self
            .ai.authority
            .read(cx)
            .validate_terminal_handle(
                tool_session_id,
                handle_id,
                oxideterm_ai::RuntimeCapability::TerminalObserve,
            )
            .map_err(|error| error.public_code().to_string())?;
        let pane = self
            .ai_terminal_pane_for_session(session_id, cx)
            .ok_or_else(|| "The terminal session owner is no longer available.".to_string())?;
        let pane = pane.read(cx);
        use oxideterm_terminal::{TerminalLifecycle, TerminalSessionKind};
        let lifecycle = match pane.lifecycle() {
            TerminalLifecycle::Running => AiTransportLifecycle::Running,
            TerminalLifecycle::Exited(code) => AiTransportLifecycle::Exited(code),
            TerminalLifecycle::Closed => AiTransportLifecycle::Closed,
        };
        match pane.session_kind() {
            TerminalSessionKind::Telnet => Ok(lifecycle.telnet()),
            TerminalSessionKind::Serial => {
                let status = pane.serial_status().ok_or_else(|| "Serial session state is unavailable.".to_string())?;
                Ok(AiSerialSessionCapture {
                    port: status.config.port_path, available: status.port_available, baud_rate: status.config.baud_rate,
                    data_bits: status.config.data_bits, stop_bits: status.config.stop_bits,
                    parity: format!("{:?}", status.config.parity).to_lowercase(),
                    flow_control: format!("{:?}", status.config.flow_control).to_lowercase(),
                    dtr: status.control_state.data_terminal_ready, rts: status.control_state.request_to_send,
                    local_echo: status.runtime_options.local_echo,
                    line_ending: format!("{:?}", status.runtime_options.line_ending).to_lowercase(),
                    output_line_ending: format!("{:?}", status.runtime_options.output_line_ending).to_lowercase(),
                    display_mode: format!("{:?}", status.runtime_options.display_mode).to_lowercase(),
                    send_mode: format!("{:?}", status.runtime_options.send_mode).to_lowercase(),
                    can_reconnect: status.can_reconnect,
                }.json(lifecycle))
            }
            _ => Err("The selected terminal is not a serial or Telnet session.".to_string()),
        }
    }

    pub(in crate::workspace) fn execute_ai_manage_serial_session(
        &mut self,
        tool_session_id: &ToolSessionId,
        arguments: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        let handle_id = arguments
            .get("handle_id")
            .and_then(serde_json::Value::as_str);
        let session_id = self
            .ai.authority
            .read(cx)
            .validate_terminal_handle(
                tool_session_id,
                handle_id,
                oxideterm_ai::RuntimeCapability::TerminalSendInput,
            )
            .map_err(|error| error.public_code().to_string())?;
        let pane = self
            .ai_terminal_pane_for_session(session_id, cx)
            .ok_or_else(|| "The serial terminal owner is no longer available.".to_string())?;
        use oxideterm_gpui_connections::form_state::{
            terminal_serial_display_mode_from_profile, terminal_serial_line_ending_from_profile,
            terminal_serial_send_mode_from_profile,
        };
        use oxideterm_gpui_terminal::TerminalSerialAction as Serial;
        let (action, operation) = ai_serial_control(arguments)?;
        let serial_action = match operation {
            AiSerialControl::RefreshPort => Serial::RefreshPortPresence,
            AiSerialControl::SendBreak => Serial::SendBreak,
            AiSerialControl::SetDtr(value) => Serial::SetDataTerminalReady(value),
            AiSerialControl::SetRts(value) => Serial::SetRequestToSend(value),
            AiSerialControl::SetLocalEcho(value) => Serial::SetLocalEcho(value),
            AiSerialControl::SetLineEnding(value) => Serial::SetLineEnding(terminal_serial_line_ending_from_profile(value)),
            AiSerialControl::SetOutputLineEnding(value) => Serial::SetOutputLineEnding(terminal_serial_line_ending_from_profile(value)),
            AiSerialControl::SetDisplayMode(value) => Serial::SetDisplayMode(terminal_serial_display_mode_from_profile(value)),
            AiSerialControl::SetSendMode(value) => Serial::SetSendMode(terminal_serial_send_mode_from_profile(value)),
        };
        pane.update(cx, |pane, cx| pane.apply_serial_action(serial_action, cx))?;
        Ok(ai_transport_session_action_accepted(action, session_id))
    }

    pub(in crate::workspace) fn execute_ai_manage_telnet_session(
        &mut self,
        tool_session_id: &ToolSessionId,
        arguments: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        // Resolve the current-turn capability before touching the session owner.
        let handle_id = arguments
            .get("handle_id")
            .and_then(serde_json::Value::as_str);
        let session_id = self
            .ai.authority
            .read(cx)
            .validate_terminal_handle(
                tool_session_id,
                handle_id,
                oxideterm_ai::RuntimeCapability::TerminalSendInput,
            )
            .map_err(|error| error.public_code().to_string())?;
        let pane = self
            .ai_terminal_pane_for_session(session_id, cx)
            .ok_or_else(|| "The Telnet terminal owner is no longer available.".to_string())?;
        let action = arguments
            .get("action")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A Telnet action is required.".to_string())?;
        let telnet_action = match action {
            "noop" => oxideterm_gpui_terminal::TerminalTelnetAction::SendControl(
                oxideterm_terminal::TelnetControlCommand::NoOperation,
            ),
            "break" => oxideterm_gpui_terminal::TerminalTelnetAction::SendControl(
                oxideterm_terminal::TelnetControlCommand::Break,
            ),
            "interrupt_process" => oxideterm_gpui_terminal::TerminalTelnetAction::SendControl(
                oxideterm_terminal::TelnetControlCommand::InterruptProcess,
            ),
            "abort_output" => oxideterm_gpui_terminal::TerminalTelnetAction::SendControl(
                oxideterm_terminal::TelnetControlCommand::AbortOutput,
            ),
            "are_you_there" => oxideterm_gpui_terminal::TerminalTelnetAction::SendControl(
                oxideterm_terminal::TelnetControlCommand::AreYouThere,
            ),
            "erase_character" => oxideterm_gpui_terminal::TerminalTelnetAction::SendControl(
                oxideterm_terminal::TelnetControlCommand::EraseCharacter,
            ),
            "erase_line" => oxideterm_gpui_terminal::TerminalTelnetAction::SendControl(
                oxideterm_terminal::TelnetControlCommand::EraseLine,
            ),
            "go_ahead" => oxideterm_gpui_terminal::TerminalTelnetAction::SendControl(
                oxideterm_terminal::TelnetControlCommand::GoAhead,
            ),
            "disconnect" => {
                return Err(
                    "Telnet disconnect must be requested from Active Sessions.".to_string(),
                );
            }
            _ => return Err("The requested Telnet action is unsupported.".to_string()),
        };
        pane.update(cx, |pane, cx| pane.apply_telnet_action(telnet_action, cx))?;
        Ok(ai_transport_session_action_accepted(action, session_id))
    }

    pub(in crate::workspace) fn execute_ai_list_remote_desktop_sessions(
        &self,
        cx: &App,
    ) -> serde_json::Value {
        self.remote_desktop.read(cx).ai_snapshot(cx)
    }

    pub(in crate::workspace) fn execute_ai_manage_remote_desktop_session(
        &mut self,
        arguments: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        let tab_id = arguments
            .get("tab_id")
            .and_then(serde_json::Value::as_str)
            .and_then(|value| value.parse::<u64>().ok())
            .map(TabId)
            .ok_or_else(|| "A valid remote desktop tab identifier is required.".to_string())?;
        let session = self
            .remote_desktop
            .read(cx)
            .session(tab_id)
            .ok_or_else(|| "The remote desktop session no longer exists.".to_string())?;
        let action = arguments
            .get("action")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A remote desktop session action is required.".to_string())?;
        match action {
            "disconnect" => {
                if !session.read(cx).ai_can_disconnect() {
                    return Err("The remote desktop session is already disconnected.".to_string());
                }
                self.disconnect_remote_desktop(tab_id, window, cx);
            }
            "reconnect" => {
                return Err(
                    "Remote desktop reconnect creates a new tab from Active Sessions."
                        .to_string(),
                );
            }
            _ => return Err("Unsupported remote desktop session action.".to_string()),
        }
        Ok(serde_json::json!({
            "accepted": true,
            "action": action,
            "tabId": tab_id.0.to_string(),
        }))
    }

    pub(in crate::workspace) fn execute_ai_get_cloud_sync_state(
        &self,
        cx: &App,
    ) -> serde_json::Value {
        oxideterm_ai::sanitize_json_for_ai(&self.cloud_sync.read(cx).ai_snapshot())
    }

    pub(in crate::workspace) fn execute_ai_manage_cloud_sync(
        &mut self,
        arguments: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        let action = arguments
            .get("action")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A Cloud Sync action is required.".to_string())?;
        if action != "open" && self.cloud_sync.read(cx).operation_in_flight() {
            return Err("Another Cloud Sync operation is already running.".to_string());
        }
        match action {
            "open" => self.open_cloud_sync_tab(window, cx),
            "check" => self.start_cloud_sync_check_with_options(false, cx),
            "upload_preview" => self.start_cloud_sync_upload_preview(cx),
            "pull_preview" => self.start_cloud_sync_pull_preview(cx),
            _ => return Err("Unsupported Cloud Sync action.".to_string()),
        }
        Ok(ai_action_accepted(action))
    }

    pub(in crate::workspace) fn execute_ai_configure_cloud_sync(
        &mut self,
        arguments: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        if self.cloud_sync.read(cx).operation_in_flight() {
            return Err("Cloud Sync configuration cannot change while an operation is running."
                .to_string());
        }
        // Return any active IME-owned draft before refreshing the form projection.
        self.apply_focused_cloud_sync_input_draft(cx);
        let (current_settings, current_scope) = {
            let cloud_sync = self.cloud_sync.read(cx);
            let state = cloud_sync.controller.store.state();
            (state.settings.clone(), state.sync_scope.clone())
        };
        let (settings, scope, updated_fields) =
            oxideterm_gpui_cloud_sync::apply_cloud_sync_configuration_patch(
                &current_settings,
                &current_scope,
                arguments,
            )?;
        let settings_for_view = settings.clone();
        let save_result = self.cloud_sync.update(cx, |cloud_sync, cx| {
            let state = cloud_sync.controller.store.state_mut();
            state.settings = settings;
            state.sync_scope = scope;
            state.last_error = None;
            if let Err(error) = cloud_sync.controller.store.save() {
                // Roll back only non-secret state; protected credentials were never read or moved.
                let message = error.to_string();
                let state = cloud_sync.controller.store.state_mut();
                state.settings = current_settings.clone();
                state.sync_scope = current_scope.clone();
                state.last_error = Some(message.clone());
                return Err(message);
            }
            cloud_sync
                .view
                .form
                .apply_changed_non_secret_settings(&current_settings, &settings_for_view);
            cx.notify();
            Ok(())
        });
        save_result.map_err(|error| format!("Failed to save Cloud Sync configuration: {error}"))?;

        self.invalidate_cloud_sync_snapshot_caches(cx);
        self.reschedule_cloud_sync_auto_upload(cx);
        self.queue_cloud_sync_dirty_refresh(cx);
        Ok(serde_json::json!({
            "accepted": true,
            "updatedFields": updated_fields,
            "state": self.execute_ai_get_cloud_sync_state(cx),
        }))
    }

    pub(in crate::workspace) fn execute_ai_list_credentials(&self, arguments: &serde_json::Value) -> Result<serde_json::Value, String> {
        let scopes = ai_credential_scopes(arguments, self.connection_store.connections().iter().map(|connection| connection.id.as_str()));
        let mut privilege_credentials = Vec::new();
        for scope in &scopes {
            privilege_credentials.extend(self.connection_store.list_privilege_credentials(scope).map_err(|error| error.to_string())?);
        }
        Ok(ai_list_credential_metadata(self.connection_store.managed_ssh_keys(), privilege_credentials,
            self.connection_store.remote_desktop_profiles()))
    }

    pub(in crate::workspace) fn execute_ai_manage_credential(
        &mut self,
        arguments: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        let (action, id, credential) = match ai_credential_action(arguments)? {
            AiCredentialAction::Open(connection_id) => {
                self.open_privilege_credentials_settings(connection_id.map(str::to_owned), window, cx);
                return Ok(ai_action_accepted("open_manager"));
            }
            AiCredentialAction::Delete { action, id, credential } => (action, id, credential),
        };
        let deleted = match credential {
            AiCredentialKind::ManagedSshKey { force } => self.connection_store
                .delete_managed_ssh_key(id, force).map(|result| result.deleted),
            AiCredentialKind::Privilege { connection_id } => self.connection_store
                .delete_privilege_credential(connection_id, id),
            AiCredentialKind::RemoteDesktop => self.connection_store.delete_remote_desktop_credential(id),
        }.map_err(|error| error.to_string())?;
        if deleted {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        cx.notify();
        Ok(credential.result(action, id, deleted))
    }

    pub(in crate::workspace) fn execute_ai_list_memory_entries(&self, arguments: &serde_json::Value) -> serde_json::Value {
        oxideterm_ai::tool_resources::ai_list_memory_entries(&self.settings_store.settings().ai.memory.entries, arguments)
    }

    pub(in crate::workspace) fn execute_ai_manage_memory_entry(
        &mut self,
        arguments: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        let mut entries = self.settings_store.settings().ai.memory.entries.clone();
        let result = oxideterm_ai::tool_resources::ai_manage_memory_entry(
            &mut entries, arguments, |kind| self.ai_resolve_memory_scope_id(kind, cx),
        )?;
        let clears_legacy_content = arguments.get("action").and_then(serde_json::Value::as_str) == Some("delete")
            && arguments.get("id").and_then(serde_json::Value::as_str) == Some("legacy-user-memory");
        self.edit_settings(move |settings| {
            settings.ai.memory.entries = entries;
            if clears_legacy_content {
                settings.ai.memory.content.clear();
            }
        }, cx);
        Ok(result)
    }

    fn ai_resolve_memory_scope_id(
        &self,
        scope_kind: oxideterm_settings::AiMemoryScopeKind,
        cx: &App,
    ) -> Result<Option<String>, String> {
        match scope_kind {
            oxideterm_settings::AiMemoryScopeKind::User => Ok(Some(whoami::username())),
            oxideterm_settings::AiMemoryScopeKind::Workspace => Ok(Some(
                oxideterm_settings::AI_APPLICATION_WORKSPACE_MEMORY_SCOPE_ID.to_string(),
            )),
            oxideterm_settings::AiMemoryScopeKind::Project => self
                .active_terminal_cwd_snapshot(cx)
                .map(|snapshot| Some(snapshot.path().to_string()))
                .ok_or_else(|| {
                    "No active terminal project is available for project-scoped memory."
                        .to_string()
                }),
            oxideterm_settings::AiMemoryScopeKind::Host => self
                .active_ssh_terminal_node_id(cx)
                .and_then(|node_id| self.node_router.resolve_connection_now(&node_id).ok())
                .map(|connection| Some(connection.connection_id))
                .ok_or_else(|| {
                    "No active SSH host is available for host-scoped memory.".to_string()
                }),
        }
    }
}

fn ai_plugin_state_label(state: plugin_host::NativePluginState) -> &'static str {
    match state {
        plugin_host::NativePluginState::Discovered => "discovered",
        plugin_host::NativePluginState::Disabled => "disabled",
        plugin_host::NativePluginState::UnsupportedLegacyJs => "unsupported_legacy_js",
        plugin_host::NativePluginState::ReadyManifestOnly => "ready_manifest_only",
        plugin_host::NativePluginState::ReadyWasm => "ready_wasm",
        plugin_host::NativePluginState::ReadyProcess => "ready_process",
        plugin_host::NativePluginState::Loading => "loading",
        plugin_host::NativePluginState::Active => "active",
        plugin_host::NativePluginState::Error => "error",
        plugin_host::NativePluginState::AutoDisabled => "auto_disabled",
    }
}
