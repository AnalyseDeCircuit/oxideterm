use oxideterm_ai::tool_resources::{
    AiForwardOperation, AiPluginAction, ai_action_accepted, ai_forwarding_operation, ai_connection_credential_inventory, ai_list_transport_profiles,
    ai_plugin_action, ai_plugin_action_accepted, AiCredentialAction,
    ai_credential_action,
    AiCloudSyncAction, AiProfileTransport, ai_cloud_sync_action, ai_host_tool_control,
    ai_host_tools_inspection, ai_host_tools_query, ai_open_transport_profile, ai_remote_desktop_tab, ai_remote_desktop_disconnect,
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
        let (resource, refresh_requested) = ai_host_tools_query(arguments);
        let snapshot = oxideterm_ai::sanitize_json_for_ai(
            &self.host_tools.read(cx).ai_snapshot(&connection_id, resource),
        );
        let refresh_accepted = refresh_requested
            && self.host_tools.update(cx, |host_tools, cx| {
                host_tools.request_ai_snapshot_refresh(connection_id.clone(), resource, cx)
            });
        Ok(ai_host_tools_inspection(resource, refresh_requested, refresh_accepted, snapshot))
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
        let command = ai_host_tool_control(arguments)?;
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.execute_ai_action(
                connection_id,
                command.resource, command.action, command.entity_id, command.value, cx,
            )
        })?;
        Ok(command.accepted())
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

    pub(in crate::workspace) fn execute_ai_list_plugins(&self, cx: &App) -> serde_json::Value {
        self.plugin_entity.read(cx).registry().ai_tool_inventory()
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
        let action = ai_open_transport_profile(
            arguments,
            self.connection_store.serial_profiles(),
            self.connection_store.telnet_profiles(),
            self.connection_store.remote_desktop_profiles(),
        )?;
        match action.kind {
            AiProfileTransport::Serial => self.open_saved_serial_profile(action.id, window, cx),
            AiProfileTransport::Telnet => self.open_saved_telnet_profile(action.id, window, cx),
            AiProfileTransport::RemoteDesktop => self.open_saved_remote_desktop_profile(action.id, window, cx),
        }
        Ok(action.accepted())
    }

    fn execute_ai_terminal_transport_tool(
        &self, operation: &AiApplicationTool, turn: &ToolSessionId, arguments: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        use oxideterm_ai::RuntimeCapability;
        use oxideterm_gpui_ai::conversation::terminal_tools::{serial_control, telnet_control, transport_state};
        let (capability, missing, effect) = match operation {
            AiApplicationTool::ManageSerialSession => (RuntimeCapability::TerminalSendInput, "The serial terminal owner is no longer available.", Some(serial_control as fn(_, _, _, &mut App) -> _)),
            AiApplicationTool::ManageTelnetSession => (RuntimeCapability::TerminalSendInput, "The Telnet terminal owner is no longer available.", Some(telnet_control as fn(_, _, _, &mut App) -> _)),
            _ => (RuntimeCapability::TerminalObserve, "The terminal session owner is no longer available.", None),
        };
        let id = self.ai.authority.read(cx).validate_terminal_handle(
            turn, arguments.get("handle_id").and_then(serde_json::Value::as_str), capability,
        ).map_err(|error| error.public_code().to_owned())?;
        let pane = self.ai_terminal_pane_for_session(id, cx).ok_or_else(|| missing.to_owned())?;
        match effect {
            Some(apply) => apply(&pane, id, arguments, cx),
            None => transport_state(&pane, cx),
        }
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
        let tab_id = ai_remote_desktop_tab(arguments)?;
        let session = self
            .remote_desktop
            .read(cx)
            .session(tab_id)
            .ok_or_else(|| "The remote desktop session no longer exists.".to_string())?;
        let completion = ai_remote_desktop_disconnect(arguments, tab_id, || session.read(cx).ai_can_disconnect())?;
        self.disconnect_remote_desktop(tab_id, window, cx);
        Ok(completion)
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
        let action = ai_cloud_sync_action(arguments, self.cloud_sync.read(cx).operation_in_flight())?;
        match action {
            AiCloudSyncAction::Open => self.open_cloud_sync_tab(window, cx),
            AiCloudSyncAction::Check => self.start_cloud_sync_check_with_options(false, cx),
            AiCloudSyncAction::UploadPreview => self.start_cloud_sync_upload_preview(cx),
            AiCloudSyncAction::PullPreview => self.start_cloud_sync_pull_preview(cx),
        }
        Ok(action.accepted())
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
        let updated_fields = self.cloud_sync.update(cx, |cloud_sync, cx| {
            cloud_sync.apply_configuration_patch(arguments, cx)
        })?;

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
        ai_connection_credential_inventory(&self.connection_store, arguments)
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
        let deleted = credential.delete_from(&mut self.connection_store, id)?;
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
