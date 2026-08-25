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
                .ai_runtime_context
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
            .map(|connection| connection.connection_id.to_string())
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
        let action = arguments
            .get("action")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A forwarding action is required.".to_string())?;
        if action == "scan_ports" {
            self.start_port_scan(node_id, true, cx);
            return Ok(serde_json::json!({ "accepted": true, "action": action }));
        }

        let operation = ai_forwarding_operation(arguments)?;
        let tab_id = self
            .forwarding
            .read(cx)
            .tab_for_node(&node_id)
            .or_else(|| self.active_tab_id(cx))
            .ok_or_else(|| "No workspace tab is available for forwarding delivery.".to_string())?;
        let (message_key, sync_saved_forwards) = match action {
            "create" => ("forwards.messages.created", true),
            "update" => ("forwards.messages.updated", true),
            "stop" => ("forwards.messages.stopped", false),
            "restart" => ("forwards.messages.restarted", false),
            "delete" => ("forwards.messages.deleted", true),
            _ => return Err("Unsupported forwarding action.".to_string()),
        };
        self.start_forward_operation(
            tab_id,
            node_id,
            message_key,
            sync_saved_forwards,
            operation,
            cx,
        );
        Ok(serde_json::json!({ "accepted": true, "action": action }))
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
        let action = arguments
            .get("action")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A plugin action is required.".to_string())?;
        if action == "open_manager" {
            self.open_plugin_manager_tab(window, cx);
            return Ok(serde_json::json!({ "accepted": true, "action": action }));
        }
        if action == "install" {
            let package_url = arguments
                .get("package_url")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "A plugin package URL is required.".to_string())?;
            let parsed_url =
                url::Url::parse(package_url).map_err(|_| "The plugin package URL is invalid.")?;
            if !matches!(parsed_url.scheme(), "http" | "https")
                || !parsed_url.username().is_empty()
                || parsed_url.password().is_some()
                || parsed_url.query().is_some()
                || parsed_url.fragment().is_some()
            {
                return Err(
                    "Plugin package URLs must use HTTP(S) without credentials, query strings, or fragments."
                        .to_string(),
                );
            }
            let checksum = arguments
                .get("checksum")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let overwrite = arguments
                .get("overwrite")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let settings_path = self.settings_store.path().to_path_buf();
            let accepted = self.plugin_entity.update(cx, |plugins, _cx| {
                plugins.start_package_install(
                    settings_path,
                    None,
                    zeroize::Zeroizing::new(package_url.to_string()),
                    checksum,
                    overwrite,
                )
            });
            if !accepted {
                return Err("Another plugin operation is already running.".to_string());
            }
            cx.notify();
            return Ok(serde_json::json!({ "accepted": true, "action": action }));
        }
        let plugin_id = arguments
            .get("plugin_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A plugin identifier is required.".to_string())?;
        let result = match action {
            "enable" | "disable" => {
                let enabled = action == "enable";
                self.plugin_entity
                    .update(cx, |plugins, _cx| {
                        plugins.set_plugin_enabled(plugin_id, enabled)
                    })
                    .map(|_| {
                        if enabled {
                            self.bootstrap_native_plugin_runtime(cx);
                        }
                    })
            }
            "uninstall" => {
                let remove_storage = arguments
                    .get("remove_storage")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                self.plugin_entity.update(cx, |plugins, _cx| {
                    plugins.uninstall_plugin(plugin_id, remove_storage)
                })
            }
            "invoke" => {
                let command_id = arguments
                    .get("command_id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| "A plugin command identifier is required.".to_string())?;
                let command_arguments = arguments
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                let host_api_resolver = self.native_plugin_host_api_resolver(cx);
                self.plugin_entity.update(cx, |plugins, _cx| {
                    plugins.start_runtime_command_with_arguments(
                        plugin_id.to_string(),
                        command_id.to_string(),
                        command_arguments,
                        host_api_resolver,
                    )
                });
                Ok(())
            }
            "configure" => {
                let setting_id = arguments
                    .get("setting_id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| "A plugin setting identifier is required.".to_string())?;
                let value = arguments
                    .get("value")
                    .cloned()
                    .ok_or_else(|| "A plugin setting value is required.".to_string())?;
                self.plugin_entity.update(cx, |plugins, _cx| {
                    plugins.set_plugin_setting_value(plugin_id, setting_id, value)
                })
            }
            _ => Err("Unsupported plugin action.".to_string()),
        };
        result?;
        cx.notify();
        Ok(serde_json::json!({
            "accepted": true,
            "action": action,
            "pluginId": plugin_id,
        }))
    }

    pub(in crate::workspace) fn execute_ai_list_transport_profiles(
        &self,
    ) -> serde_json::Value {
        let serial = self
            .connection_store
            .serial_profiles()
            .iter()
            .map(|profile| {
                serde_json::json!({
                    "id": profile.id,
                    "name": profile.name,
                    "group": profile.group,
                    "portPath": profile.port_path,
                    "baudRate": profile.baud_rate,
                    "dataBits": profile.data_bits,
                    "stopBits": profile.stop_bits,
                    "parity": format!("{:?}", profile.parity).to_lowercase(),
                    "flowControl": format!("{:?}", profile.flow_control).to_lowercase(),
                })
            })
            .collect::<Vec<_>>();
        let telnet = self
            .connection_store
            .telnet_profiles()
            .iter()
            .map(|profile| {
                serde_json::json!({
                    "id": profile.id,
                    "name": profile.name,
                    "group": profile.group,
                    "host": profile.host,
                    "port": profile.port,
                })
            })
            .collect::<Vec<_>>();
        let remote_desktop = self
            .connection_store
            .remote_desktop_profiles()
            .iter()
            .map(|profile| {
                serde_json::json!({
                    "id": profile.id,
                    "name": profile.name,
                    "group": profile.group,
                    "protocol": format!("{:?}", profile.protocol).to_lowercase(),
                    "host": profile.host,
                    "port": profile.port,
                    "username": profile.username,
                    "readOnly": profile.read_only,
                    "hasCredential": profile.credential_ref.is_some(),
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "serial": serial,
            "telnet": telnet,
            "remoteDesktop": remote_desktop,
        })
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
            .ai_runtime_context
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
        let lifecycle = match pane.lifecycle() {
            oxideterm_terminal::TerminalLifecycle::Running => {
                serde_json::json!({ "state": "running" })
            }
            oxideterm_terminal::TerminalLifecycle::Exited(exit_code) => {
                serde_json::json!({ "state": "exited", "exitCode": exit_code })
            }
            oxideterm_terminal::TerminalLifecycle::Closed => {
                serde_json::json!({ "state": "closed" })
            }
        };
        match pane.session_kind() {
            oxideterm_terminal::TerminalSessionKind::Telnet => Ok(serde_json::json!({
                "transport": "telnet",
                "lifecycle": lifecycle,
            })),
            oxideterm_terminal::TerminalSessionKind::Serial => {
                let status = pane
                    .serial_status()
                    .ok_or_else(|| "Serial session state is unavailable.".to_string())?;
                Ok(serde_json::json!({
                    "transport": "serial",
                    "lifecycle": lifecycle,
                    "port": {
                        "path": status.config.port_path,
                        "available": status.port_available,
                        "baudRate": status.config.baud_rate,
                        "dataBits": status.config.data_bits,
                        "stopBits": status.config.stop_bits,
                        "parity": format!("{:?}", status.config.parity).to_lowercase(),
                        "flowControl": format!("{:?}", status.config.flow_control).to_lowercase(),
                    },
                    "controlLines": {
                        "dtr": status.control_state.data_terminal_ready,
                        "rts": status.control_state.request_to_send,
                    },
                    "runtime": {
                        "localEcho": status.runtime_options.local_echo,
                        "lineEnding": format!("{:?}", status.runtime_options.line_ending).to_lowercase(),
                        "displayMode": format!("{:?}", status.runtime_options.display_mode).to_lowercase(),
                        "sendMode": format!("{:?}", status.runtime_options.send_mode).to_lowercase(),
                    },
                    "canReconnect": status.can_reconnect,
                }))
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
            .ai_runtime_context
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
        let action = arguments
            .get("action")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A serial action is required.".to_string())?;
        let enabled = arguments
            .get("enabled")
            .and_then(serde_json::Value::as_bool);
        let value = arguments.get("value").and_then(serde_json::Value::as_str);
        let serial_action = match action {
            "refresh_port" => oxideterm_gpui_terminal::TerminalSerialAction::RefreshPortPresence,
            "reconnect" => oxideterm_gpui_terminal::TerminalSerialAction::Reconnect,
            "send_break" => oxideterm_gpui_terminal::TerminalSerialAction::SendBreak,
            "set_dtr" => oxideterm_gpui_terminal::TerminalSerialAction::SetDataTerminalReady(
                enabled.ok_or_else(|| "The DTR state is required.".to_string())?,
            ),
            "set_rts" => oxideterm_gpui_terminal::TerminalSerialAction::SetRequestToSend(
                enabled.ok_or_else(|| "The RTS state is required.".to_string())?,
            ),
            "set_local_echo" => oxideterm_gpui_terminal::TerminalSerialAction::SetLocalEcho(
                enabled.ok_or_else(|| "The local echo state is required.".to_string())?,
            ),
            "set_line_ending" => {
                let line_ending = match value {
                    Some("none") => oxideterm_terminal::SerialLineEnding::None,
                    Some("lf") => oxideterm_terminal::SerialLineEnding::Lf,
                    Some("crlf") => oxideterm_terminal::SerialLineEnding::CrLf,
                    Some("cr") => oxideterm_terminal::SerialLineEnding::Cr,
                    _ => return Err("A valid serial line ending is required.".to_string()),
                };
                oxideterm_gpui_terminal::TerminalSerialAction::SetLineEnding(line_ending)
            }
            "set_display_mode" => {
                let display_mode = match value {
                    Some("text") => oxideterm_terminal::SerialDisplayMode::Text,
                    Some("hex") => oxideterm_terminal::SerialDisplayMode::Hex,
                    Some("mixed") => oxideterm_terminal::SerialDisplayMode::Mixed,
                    _ => return Err("A valid serial display mode is required.".to_string()),
                };
                oxideterm_gpui_terminal::TerminalSerialAction::SetDisplayMode(display_mode)
            }
            "set_send_mode" => {
                let send_mode = match value {
                    Some("text") => oxideterm_terminal::SerialSendMode::Text,
                    Some("hex") => oxideterm_terminal::SerialSendMode::Hex,
                    _ => return Err("A valid serial send mode is required.".to_string()),
                };
                oxideterm_gpui_terminal::TerminalSerialAction::SetSendMode(send_mode)
            }
            _ => return Err("Unsupported serial action.".to_string()),
        };
        pane.update(cx, |pane, cx| pane.apply_serial_action(serial_action, cx))?;
        Ok(serde_json::json!({
            "accepted": true,
            "action": action,
            "sessionId": session_id.0.to_string(),
        }))
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
            .ai_runtime_context
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
            "disconnect" => oxideterm_gpui_terminal::TerminalTelnetAction::Disconnect,
            _ => return Err("The requested Telnet action is unsupported.".to_string()),
        };
        pane.update(cx, |pane, cx| pane.apply_telnet_action(telnet_action, cx))?;
        Ok(serde_json::json!({
            "accepted": true,
            "action": action,
            "sessionId": session_id.0.to_string(),
        }))
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
                if !session.read(cx).ai_can_reconnect() {
                    return Err("The remote desktop session is not ready to reconnect.".to_string());
                }
                self.reconnect_remote_desktop(tab_id, window, cx);
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
        Ok(serde_json::json!({ "accepted": true, "action": action }))
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

    pub(in crate::workspace) fn execute_ai_list_credentials(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let managed_ssh_keys = self
            .connection_store
            .managed_ssh_keys()
            .into_iter()
            .map(|key| {
                serde_json::json!({
                    "id": key.id,
                    "name": key.name,
                    "fingerprint": key.fingerprint,
                    "requiresPassphrase": key.requires_passphrase,
                    "origin": format!("{:?}", key.origin).to_lowercase(),
                    "createdAt": key.created_at,
                    "updatedAt": key.updated_at,
                })
            })
            .collect::<Vec<_>>();
        let requested_scope = arguments
            .get("connection_id")
            .and_then(serde_json::Value::as_str);
        let scopes = requested_scope.map_or_else(
            || {
                std::iter::once(
                    oxideterm_connections::LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string(),
                )
                .chain(
                    self.connection_store
                        .connections()
                        .iter()
                        .map(|connection| connection.id.clone()),
                )
                .collect::<Vec<_>>()
            },
            |scope| vec![scope.to_string()],
        );
        let mut privilege_credentials = Vec::new();
        for scope in &scopes {
            privilege_credentials.extend(
                self.connection_store
                    .list_privilege_credentials(scope)
                    .map_err(|error| error.to_string())?,
            );
        }
        let privilege = privilege_credentials
            .into_iter()
            .map(|credential| {
                serde_json::json!({
                    "id": credential.id,
                    "connectionId": credential.connection_id,
                    "label": credential.label,
                    "kind": format!("{:?}", credential.kind).to_lowercase(),
                    "usernameHint": credential.username_hint,
                    "enabled": credential.enabled,
                    "requireClickToSend": credential.require_click_to_send,
                    "hasSecret": credential.keychain_id.is_some(),
                    "createdAt": credential.created_at.to_rfc3339(),
                    "updatedAt": credential.updated_at.to_rfc3339(),
                })
            })
            .collect::<Vec<_>>();
        let remote_desktop = self
            .connection_store
            .remote_desktop_profiles()
            .iter()
            .filter(|profile| profile.credential_ref.is_some())
            .map(|profile| {
                serde_json::json!({
                    "profileId": profile.id,
                    "profileName": profile.name,
                    "protocol": format!("{:?}", profile.protocol).to_lowercase(),
                    "hasCredential": true,
                })
            })
            .collect::<Vec<_>>();
        Ok(serde_json::json!({
            "managedSshKeys": managed_ssh_keys,
            "privilegeCredentials": privilege,
            "remoteDesktopCredentials": remote_desktop,
        }))
    }

    pub(in crate::workspace) fn execute_ai_manage_credential(
        &mut self,
        arguments: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        let action = arguments
            .get("action")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A credential action is required.".to_string())?;
        if action == "open_manager" {
            let scope_id = arguments
                .get("connection_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            self.open_privilege_credentials_settings(scope_id, window, cx);
            return Ok(serde_json::json!({ "accepted": true, "action": action }));
        }
        let kind = arguments
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A credential kind is required.".to_string())?;
        let id = arguments
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A credential identifier is required.".to_string())?;
        let deleted = match kind {
            "managed_ssh_key" => {
                let force = arguments
                    .get("force")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                self.connection_store
                    .delete_managed_ssh_key(id, force)
                    .map(|result| result.deleted)
                    .map_err(|error| error.to_string())?
            }
            "privilege" => {
                let connection_id = arguments
                    .get("connection_id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| "A privilege credential scope is required.".to_string())?;
                self.connection_store
                    .delete_privilege_credential(connection_id, id)
                    .map_err(|error| error.to_string())?
            }
            "remote_desktop" => self
                .connection_store
                .delete_remote_desktop_credential(id)
                .map_err(|error| error.to_string())?,
            _ => return Err("Unsupported credential kind.".to_string()),
        };
        if deleted {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        cx.notify();
        Ok(serde_json::json!({
            "accepted": deleted,
            "action": action,
            "kind": kind,
            "id": id,
        }))
    }

    pub(in crate::workspace) fn execute_ai_list_memory_entries(
        &self,
        arguments: &serde_json::Value,
    ) -> serde_json::Value {
        let now_ms = ai_memory_now_ms();
        let scope_kind = arguments
            .get("scope_kind")
            .and_then(serde_json::Value::as_str);
        let scope_id = arguments
            .get("scope_id")
            .and_then(serde_json::Value::as_str);
        let memory_kind = arguments
            .get("memory_kind")
            .and_then(serde_json::Value::as_str);
        let include_expired = arguments
            .get("include_expired")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let entries = self
            .settings_store
            .settings()
            .ai
            .memory
            .entries
            .iter()
            .filter(|entry| include_expired || !entry.is_expired(now_ms))
            .filter(|entry| {
                scope_kind.is_none_or(|kind| ai_memory_scope_label(entry.scope_kind) == kind)
            })
            .filter(|entry| {
                scope_id.is_none_or(|scope_id| entry.scope_id.as_deref() == Some(scope_id))
            })
            .filter(|entry| {
                memory_kind.is_none_or(|kind| ai_memory_kind_label(entry.memory_kind) == kind)
            })
            .map(ai_memory_entry_json)
            .collect::<Vec<_>>();
        serde_json::json!({ "entries": entries })
    }

    pub(in crate::workspace) fn execute_ai_manage_memory_entry(
        &mut self,
        arguments: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        let action = arguments
            .get("action")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A memory action is required.".to_string())?;
        let now_ms = ai_memory_now_ms();
        let mut entries = self.settings_store.settings().ai.memory.entries.clone();
        let result = match action {
            "create" => {
                let content = ai_memory_entry_content(arguments)?;
                let scope_kind = ai_memory_scope_from_arguments(arguments)?;
                let scope_id = self.ai_resolve_memory_scope_id(arguments, scope_kind, cx)?;
                let memory_kind = ai_memory_kind_from_arguments(arguments)?;
                let expires_at_ms = ai_memory_expiry(arguments, memory_kind, now_ms)?;
                let normalized = ai_normalized_memory_content(&content);
                if let Some(existing) = entries.iter_mut().find(|entry| {
                    entry.scope_kind == scope_kind
                        && entry.scope_id == scope_id
                        && ai_normalized_memory_content(&entry.content) == normalized
                }) {
                    existing.last_used_at_ms = Some(now_ms);
                    existing.use_count = existing.use_count.saturating_add(1);
                    existing.updated_at_ms = now_ms;
                    existing.revision = existing.revision.saturating_add(1);
                    ai_memory_entry_json(existing)
                } else {
                    let entry = oxideterm_settings::AiMemoryEntry {
                        id: uuid::Uuid::new_v4().to_string(),
                        content,
                        scope_kind,
                        scope_id,
                        memory_kind,
                        source: oxideterm_settings::AiMemorySource::Assistant,
                        created_at_ms: now_ms,
                        updated_at_ms: now_ms,
                        last_used_at_ms: None,
                        use_count: 0,
                        expires_at_ms,
                        revision: 1,
                    };
                    let result = ai_memory_entry_json(&entry);
                    entries.push(entry);
                    result
                }
            }
            "update" => {
                let (entry_id, normalized_content, scope_kind, scope_id) = {
                    let entry = ai_memory_entry_mut(arguments, &mut entries)?;
                    if let Some(content) = arguments
                        .get("content")
                        .and_then(serde_json::Value::as_str)
                    {
                        let content = content.trim();
                        if !oxideterm_ai::preference_is_safe_to_persist(content) {
                            return Err(
                                "Memory cannot store credentials or one-time task instructions."
                                    .to_string(),
                            );
                        }
                        entry.content = content.to_string();
                    }
                    if arguments.get("scope_kind").is_some() {
                        entry.scope_kind = ai_memory_scope_from_arguments(arguments)?;
                        entry.scope_id =
                            self.ai_resolve_memory_scope_id(arguments, entry.scope_kind, cx)?;
                    }
                    if arguments.get("memory_kind").is_some() {
                        entry.memory_kind = ai_memory_kind_from_arguments(arguments)?;
                    }
                    if arguments.get("memory_kind").is_some()
                        || arguments.get("expires_at_ms").is_some()
                        || entry.memory_kind == oxideterm_settings::AiMemoryKind::Temporary
                    {
                        entry.expires_at_ms =
                            ai_memory_expiry(arguments, entry.memory_kind, now_ms)?;
                    }
                    entry.updated_at_ms = now_ms;
                    entry.revision = entry.revision.saturating_add(1);
                    (
                        entry.id.clone(),
                        ai_normalized_memory_content(&entry.content),
                        entry.scope_kind,
                        entry.scope_id.clone(),
                    )
                };
                if entries.iter().any(|entry| {
                    entry.id != entry_id
                        && entry.scope_kind == scope_kind
                        && entry.scope_id == scope_id
                        && ai_normalized_memory_content(&entry.content) == normalized_content
                }) {
                    return Err(
                        "An equivalent memory entry already exists in this scope.".to_string()
                    );
                }
                let entry = entries
                    .iter()
                    .find(|entry| entry.id == entry_id)
                    .ok_or_else(|| "The memory entry no longer exists.".to_string())?;
                ai_memory_entry_json(entry)
            }
            "touch" => {
                let entry = ai_memory_entry_mut(arguments, &mut entries)?;
                entry.last_used_at_ms = Some(now_ms);
                entry.use_count = entry.use_count.saturating_add(1);
                entry.updated_at_ms = now_ms;
                entry.revision = entry.revision.saturating_add(1);
                ai_memory_entry_json(entry)
            }
            "delete" => {
                let id = ai_memory_id(arguments)?;
                let expected_revision = ai_memory_expected_revision(arguments)?;
                let index = entries
                    .iter()
                    .position(|entry| entry.id == id)
                    .ok_or_else(|| "The memory entry no longer exists.".to_string())?;
                if entries[index].revision != expected_revision {
                    return Err("The memory entry changed; refresh it before deleting.".to_string());
                }
                entries.remove(index);
                serde_json::json!({ "id": id, "deleted": true })
            }
            _ => return Err("Unsupported memory action.".to_string()),
        };
        let clears_legacy_content = action == "delete"
            && arguments.get("id").and_then(serde_json::Value::as_str)
                == Some("legacy-user-memory");
        self.edit_settings(
            move |settings| {
                settings.ai.memory.entries = entries;
                if clears_legacy_content {
                    settings.ai.memory.content.clear();
                }
            },
            cx,
        );
        Ok(serde_json::json!({ "accepted": true, "action": action, "entry": result }))
    }

    fn ai_resolve_memory_scope_id(
        &self,
        arguments: &serde_json::Value,
        scope_kind: oxideterm_settings::AiMemoryScopeKind,
        cx: &App,
    ) -> Result<Option<String>, String> {
        if let Some(scope_id) = arguments
            .get("scope_id")
            .and_then(serde_json::Value::as_str)
        {
            return Ok(Some(scope_id.to_string()));
        }
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
                .map(|connection| Some(connection.connection_id.to_string()))
                .ok_or_else(|| {
                    "No active SSH host is available for host-scoped memory.".to_string()
                }),
        }
    }
}

fn ai_memory_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn ai_memory_scope_label(scope: oxideterm_settings::AiMemoryScopeKind) -> &'static str {
    match scope {
        oxideterm_settings::AiMemoryScopeKind::User => "user",
        oxideterm_settings::AiMemoryScopeKind::Workspace => "workspace",
        oxideterm_settings::AiMemoryScopeKind::Project => "project",
        oxideterm_settings::AiMemoryScopeKind::Host => "host",
    }
}

fn ai_memory_kind_label(kind: oxideterm_settings::AiMemoryKind) -> &'static str {
    match kind {
        oxideterm_settings::AiMemoryKind::LongTerm => "long_term",
        oxideterm_settings::AiMemoryKind::Temporary => "temporary",
    }
}

fn ai_memory_entry_json(entry: &oxideterm_settings::AiMemoryEntry) -> serde_json::Value {
    serde_json::json!({
        "id": entry.id,
        "content": oxideterm_ai::sanitize_for_ai(&entry.content),
        "scopeKind": ai_memory_scope_label(entry.scope_kind),
        "scopeId": entry.scope_id,
        "memoryKind": ai_memory_kind_label(entry.memory_kind),
        "source": format!("{:?}", entry.source).to_lowercase(),
        "createdAtMs": entry.created_at_ms,
        "updatedAtMs": entry.updated_at_ms,
        "lastUsedAtMs": entry.last_used_at_ms,
        "useCount": entry.use_count,
        "expiresAtMs": entry.expires_at_ms,
        "revision": entry.revision,
    })
}

fn ai_memory_entry_content(arguments: &serde_json::Value) -> Result<String, String> {
    let content = arguments
        .get("content")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|content| !content.is_empty())
        .ok_or_else(|| "Memory content is required.".to_string())?;
    if !oxideterm_ai::preference_is_safe_to_persist(content) {
        return Err("Memory cannot store credentials or one-time task instructions.".to_string());
    }
    Ok(content.to_string())
}

fn ai_memory_scope_from_arguments(
    arguments: &serde_json::Value,
) -> Result<oxideterm_settings::AiMemoryScopeKind, String> {
    match arguments
        .get("scope_kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("user")
    {
        "user" => Ok(oxideterm_settings::AiMemoryScopeKind::User),
        "workspace" => Ok(oxideterm_settings::AiMemoryScopeKind::Workspace),
        "project" => Ok(oxideterm_settings::AiMemoryScopeKind::Project),
        "host" => Ok(oxideterm_settings::AiMemoryScopeKind::Host),
        _ => Err("Unsupported memory scope.".to_string()),
    }
}

fn ai_memory_kind_from_arguments(
    arguments: &serde_json::Value,
) -> Result<oxideterm_settings::AiMemoryKind, String> {
    match arguments
        .get("memory_kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("long_term")
    {
        "long_term" => Ok(oxideterm_settings::AiMemoryKind::LongTerm),
        "temporary" => Ok(oxideterm_settings::AiMemoryKind::Temporary),
        _ => Err("Unsupported memory kind.".to_string()),
    }
}

fn ai_memory_expiry(
    arguments: &serde_json::Value,
    memory_kind: oxideterm_settings::AiMemoryKind,
    now_ms: i64,
) -> Result<Option<i64>, String> {
    let expires_at_ms = arguments
        .get("expires_at_ms")
        .and_then(serde_json::Value::as_i64);
    if memory_kind == oxideterm_settings::AiMemoryKind::Temporary {
        let expires_at_ms = expires_at_ms
            .filter(|expires_at_ms| *expires_at_ms > now_ms)
            .ok_or_else(|| "Temporary memory requires a future expiry time.".to_string())?;
        Ok(Some(expires_at_ms))
    } else {
        Ok(expires_at_ms.filter(|expires_at_ms| *expires_at_ms > now_ms))
    }
}

fn ai_normalized_memory_content(content: &str) -> String {
    content
        .split_whitespace()
        .flat_map(str::chars)
        .flat_map(char::to_lowercase)
        .collect()
}

fn ai_memory_id(arguments: &serde_json::Value) -> Result<&str, String> {
    arguments
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "A memory entry identifier is required.".to_string())
}

fn ai_memory_expected_revision(arguments: &serde_json::Value) -> Result<u64, String> {
    arguments
        .get("expected_revision")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "The current memory revision is required.".to_string())
}

fn ai_memory_entry_mut<'a>(
    arguments: &serde_json::Value,
    entries: &'a mut [oxideterm_settings::AiMemoryEntry],
) -> Result<&'a mut oxideterm_settings::AiMemoryEntry, String> {
    let id = ai_memory_id(arguments)?;
    let expected_revision = ai_memory_expected_revision(arguments)?;
    let entry = entries
        .iter_mut()
        .find(|entry| entry.id == id)
        .ok_or_else(|| "The memory entry no longer exists.".to_string())?;
    if entry.revision != expected_revision {
        return Err("The memory entry changed; refresh it before updating.".to_string());
    }
    Ok(entry)
}

fn ai_forwarding_operation(
    arguments: &serde_json::Value,
) -> Result<forwards::ForwardingRuntimeOperation, String> {
    let action = arguments
        .get("action")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let forward_id = || {
        arguments
            .get("forward_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "A forwarding rule identifier is required.".to_string())
    };
    match action {
        "create" => {
            let rule = arguments
                .get("rule")
                .cloned()
                .and_then(|value| {
                    serde_json::from_value::<oxideterm_forwarding::ForwardRule>(value).ok()
                })
                .ok_or_else(|| "The forwarding rule is invalid.".to_string())?;
            Ok(forwards::ForwardingRuntimeOperation::Create {
                rule,
                check_health: true,
            })
        }
        "update" => {
            let rule = arguments
                .get("rule")
                .and_then(serde_json::Value::as_object)
                .ok_or_else(|| "The forwarding update is invalid.".to_string())?;
            let update = oxideterm_forwarding::ForwardUpdate {
                forward_type: rule
                    .get("forwardType")
                    .cloned()
                    .and_then(|value| serde_json::from_value(value).ok()),
                bind_address: rule
                    .get("bindAddress")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                bind_port: rule
                    .get("bindPort")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|port| u16::try_from(port).ok()),
                target_host: rule
                    .get("targetHost")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                target_port: rule
                    .get("targetPort")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|port| u16::try_from(port).ok()),
                description: rule
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
            };
            Ok(forwards::ForwardingRuntimeOperation::Update {
                forward_id: forward_id()?,
                update,
            })
        }
        "stop" => Ok(forwards::ForwardingRuntimeOperation::Stop {
            forward_id: forward_id()?,
        }),
        "restart" => Ok(forwards::ForwardingRuntimeOperation::Restart {
            forward_id: forward_id()?,
        }),
        "delete" => Ok(forwards::ForwardingRuntimeOperation::Delete {
            forward_id: forward_id()?,
        }),
        _ => Err("Unsupported forwarding action.".to_string()),
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

fn ai_application_action_result(
    snapshot: &AiOrchestratorRuntimeSnapshot,
    result: Result<serde_json::Value, String>,
    summary: &str,
    risk: &'static str,
) -> AiActionResultLite {
    match result {
        Ok(data) => snapshot.ok(
            summary,
            serde_json::to_string_pretty(&data).unwrap_or_default(),
            data,
            risk,
        ),
        Err(message) => snapshot.fail(summary, "application_tool_failed", message, risk),
    }
}
