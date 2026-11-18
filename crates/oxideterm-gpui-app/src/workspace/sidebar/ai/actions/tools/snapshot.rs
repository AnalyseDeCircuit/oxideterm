use oxideterm_ai::tool_resources::{
    AiActiveNodeCapture, AiAppSurface, AiApplicationTool, AiLiveResourceOperation, AiRuntimeHealthCapture, AiSerialTargetInput, AiSshNodeTargetInput,
    AiTerminalOutputWait, AiTerminalRunEnd, AiTerminalWaitFailure, AiTerminalWaitProgress, AiTerminalWaitSample,
    AiTargetCapture, AiTerminalTargetInput, AiTerminalTransport, finish_ai_target_discovery,
    finish_ai_workspace_snapshot, ai_read_stable_resource, dispatch_ai_application_tool,
    ai_preference_saved, ai_remember_preference_payload, save_ai_user_preference, plan_ai_settings_write,
};
use oxideterm_workspace::ActiveSessionReadiness;

pub(in crate::workspace) const AI_CONNECT_TARGET_TIMEOUT_TICKS: usize = 900;
pub(in crate::workspace) const AI_CONNECT_TARGET_POLL_INTERVAL_MS: u64 = 100;

fn ai_node_readiness(node: &WorkspaceSshNode) -> ActiveSessionReadiness {
    match node.readiness {
        NodeReadiness::Ready => ActiveSessionReadiness::Ready,
        NodeReadiness::Connecting => ActiveSessionReadiness::Connecting,
        NodeReadiness::Error => ActiveSessionReadiness::Error,
        NodeReadiness::Disconnected => ActiveSessionReadiness::Disconnected,
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_orchestrator_snapshot(
        &self,
        cx: &mut Context<Self>,
    ) -> AiOrchestratorRuntimeSnapshot {
        self.ai_orchestrator_snapshot_for_tool_session(None, cx)
    }

    pub(in crate::workspace) fn ai_orchestrator_snapshot_for_tool_session(
        &self,
        tool_session_id: Option<&ToolSessionId>,
        cx: &mut Context<Self>,
    ) -> AiOrchestratorRuntimeSnapshot {
        let scoped_session = tool_session_id.filter(|session| {
            self.ai.authority
                .read(cx)
                .is_agent_scoped_session(session)
        });
        let mut targets = Vec::new();
        for connection in self.connection_store.connections() {
            targets.push(AiTargetCapture::SavedConnection {
                id: &connection.id,
                name: &connection.name,
                host: &connection.host,
                username: &connection.username,
                port: connection.port,
                group: &connection.group,
            });
        }
        for tab in self.tabs(cx) {
            let session_id = tab.root_pane.as_ref().and_then(|root| root.session_id_for_pane(tab.active_pane_id?));
            targets.push(AiTargetCapture::Surface {
                tab_id: tab.id,
                kind: tab.kind.clone(),
                title: tab.title.clone(),
                session_id,
                visible: self.tab_host.read(cx).surface_is_visible(tab.id),
            });
        }

        for (node_id, node) in &self.ssh_nodes {
            let resolved_connection = self.node_router.resolve_connection_now(node_id).ok();
            let sftp_session_id = resolved_connection
                .as_ref()
                .and_then(|resolved| resolved.sftp_session_id.clone());
            targets.push(AiTargetCapture::SshNode {
                target: AiSshNodeTargetInput {
                    node_id: &node_id.0,
                    connection_id: node.saved_connection_id.as_deref(),
                    readiness: ai_node_readiness(node),
                    host: &node.endpoint.host,
                    port: node.endpoint.port,
                    username: &node.endpoint.username,
                    title: &node.title,
                    terminal_ids: &node.terminal_ids,
                    sftp_session_id: None,
                },
                sftp_session_id,
            });
        }

        for node_id in self.sftp_tab_nodes.values() {
            let Some(node) = self.ssh_nodes.get(node_id) else {
                continue;
            };
            let Some(sftp_session_id) = self
                .node_router
                .resolve_connection_now(node_id)
                .ok()
                .and_then(|resolved| resolved.sftp_session_id)
            else {
                continue;
            };
            targets.push(AiTargetCapture::Sftp(oxideterm_ai::AiSftpTargetInput {
                node_id: node_id.0.clone(),
                session_id: sftp_session_id,
                connection_id: node.saved_connection_id.clone(),
                host: node.endpoint.host.clone(),
            }));
        }

        for ide_target in self.ide_workspace.read(cx).target_snapshots(cx) {
            let Some(node) = self.ssh_nodes.get(&ide_target.node_id) else {
                continue;
            };
            targets.push(AiTargetCapture::Ide {
                tab_id: ide_target.tab_id,
                target: oxideterm_ai::AiIdeTargetInput {
                    node_id: ide_target.node_id.0,
                    connection_id: node.saved_connection_id.clone(),
                    active_editor_tab_id: ide_target.active_editor_tab_id,
                    project_root_path: ide_target.project_root_path,
                    project_name: ide_target.project_name,
                },
            });
        }

        let tab_host = self.tab_host.read(cx);
        for tab in self.tabs(cx) {
            let Some(root) = tab.root_pane.as_ref() else {
                continue;
            };
            let mut pane_ids = Vec::new();
            root.collect_pane_ids(&mut pane_ids);
            for pane_id in pane_ids {
                let Some(session_id) = root.session_id_for_pane(pane_id) else {
                    continue;
                };
                if scoped_session.is_some_and(|session| {
                    !self
                        .ai.authority
                        .read(cx)
                        .agent_can_observe_terminal(session, session_id)
                }) {
                    continue;
                }
                let Some(pane) = tab_host.panes().get(&pane_id) else {
                    continue;
                };
                let serial_config = self.serial_terminal_configs.get(&session_id);
                let (
                    session_kind,
                    terminal_buffer,
                    terminal_screen,
                    accepts_input,
                    terminal_running,
                ) = {
                    let pane = pane.read(cx);
                    let screen = pane.ai_screen_snapshot();
                    let is_alternate_buffer = pane.ai_screen_is_alternate_buffer();
                    (
                        pane.session_kind(),
                        pane.ai_buffer_snapshot(),
                        ai_terminal_screen_snapshot_json(&screen, is_alternate_buffer),
                        pane.ai_accepts_input(),
                        pane.lifecycle().is_running(),
                    )
                };
                targets.push(AiTargetCapture::Terminal(AiTerminalTargetInput {
                    session_id, tab_id: tab.id, pane_id, pane_label: self.terminal_pane_label(pane_id, cx),
                    transport: match session_kind {
                        oxideterm_terminal::TerminalSessionKind::LocalPty => AiTerminalTransport::Local,
                        oxideterm_terminal::TerminalSessionKind::Serial => AiTerminalTransport::Serial,
                        oxideterm_terminal::TerminalSessionKind::Telnet => AiTerminalTransport::Telnet,
                        oxideterm_terminal::TerminalSessionKind::Ble => AiTerminalTransport::Ble,
                        _ => AiTerminalTransport::Other,
                    },
                    serial: serial_config.map(|config| AiSerialTargetInput {
                        port_path: &config.port_path, baud_rate: config.baud_rate, data_bits: config.data_bits,
                        stop_bits: config.stop_bits, parity: format!("{:?}", config.parity).to_lowercase(),
                        flow_control: format!("{:?}", config.flow_control).to_lowercase(),
                    }),
                    terminal_buffer, terminal_screen, accepts_input, terminal_running,
                }));
            }
        }

        let (targets, runtime_handles) = self.ai.authority.update(cx, |authority, _| {
            finish_ai_target_discovery(authority, tool_session_id, targets, |id| NodeId::new(id.to_owned()))
        });
        if scoped_session.is_some() {
            return AiOrchestratorRuntimeSnapshot {
                targets, runtime_handles, ..AiOrchestratorRuntimeSnapshot::background_result_projection()
            };
        }
        let active_tab = self.active_content_tab(cx).map(|tab| (tab.kind.clone(), tab.title.clone()));
        let active_node = self.active_ssh_node_id.as_ref().and_then(|id| {
            self.ssh_nodes.get(id).map(|node| AiActiveNodeCapture {
                host: &node.endpoint.host,
                username: &node.endpoint.username,
                readiness: ai_node_readiness(node),
            })
        });
        let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(10 * 60);
        let (mut total, mut warnings, mut errors) = (0, 0, 0);
        for entry in &self.notification_center.event_log.entries {
            if entry.timestamp >= cutoff {
                total += 1;
                match entry.severity {
                    WorkspaceEventSeverity::Warn => warnings += 1,
                    WorkspaceEventSeverity::Error => errors += 1,
                    _ => {}
                }
            }
        }
        let mut snapshot = AiOrchestratorRuntimeSnapshot::background_result_projection();
        snapshot.targets = targets;
        snapshot.runtime_handles = runtime_handles;
        finish_ai_workspace_snapshot(
            snapshot,
            active_tab.as_ref().map(|(kind, title)| (kind, title.as_str())),
            active_node,
            self.ai_active_terminal_session_id(cx),
            self.active_content_tab_id(cx),
            self.active_ssh_node_id.as_ref().map(|node_id| node_id.0.as_str()),
            self.settings_store.settings(),
            ai_transfers_state(&self.sftp_transfer_manager),
            AiRuntimeHealthCapture {
                open_tabs: self.tabs(cx).len(),
                has_active_tab: self.active_tab_id(cx).is_some(),
                pane_count: self.tab_host.read(cx).panes().len(),
                local_session_count: self.visible_local_terminal_session_count(cx)
                    + self.detached_local_terminals.len(),
                node_states: self.ssh_nodes.values().map(ai_node_readiness),
                recent_events: (total, warnings, errors),
            },
        )
    }

    pub(in crate::workspace) fn ai_model_backend_services(&self, cx: &App) -> AiModelBackendServices {
        self.ai.state.read(cx).model_backend_services(&self.settings_store.settings().ai)
    }

    pub(in crate::workspace) fn ai_live_tool_services(&self) -> AiLiveToolServices {
        // Application owners are copied only into broker-started tasks after a
        // live handle has passed its final GPUI-thread validation.
        AiLiveToolServices {
            node_router: self.node_router.clone(),
            sftp_transfer_manager: self.sftp_transfer_manager.clone(),
            backend_runtime: self.forwarding_runtime.clone(),
        }
    }

    /// Rebuilds the model-visible authority projection on the GPUI thread for
    /// every provider round. It deliberately owns no transport or pane state.
    pub(in crate::workspace) fn ai_runtime_context_prompt(
        &self,
        tool_session_id: &ToolSessionId,
        cx: &mut Context<Self>,
    ) -> String {
        // Issuing from authoritative owners here makes the first provider
        // round useful without trusting the internal target scan as authority.
        let snapshot = self.ai_orchestrator_snapshot_for_tool_session(Some(tool_session_id), cx);
        oxideterm_ai::tool_resources::ai_runtime_context_prompt(
            &snapshot,
            self.ai.authority.read(cx),
            tool_session_id,
            ai_now_ms(),
        )
    }

    pub(in crate::workspace) fn ai_acp_chat_launch(
        &self,
        config: &AiChatStreamConfig,
        cx: &App,
    ) -> Result<Option<AiAcpChatLaunch>, String> {
        if config.execution_backend != AiExecutionBackend::Acp {
            return Ok(None);
        }
        let agent_id = config
            .acp_agent_id
            .as_deref()
            .filter(|agent_id| !agent_id.trim().is_empty())
            .ok_or_else(|| "No ACP agent selected for this execution profile.".to_string())?;
        let mut agent = self
            .settings_store
            .settings()
            .ai
            .acp_agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .cloned()
            .ok_or_else(|| format!("ACP agent `{agent_id}` is not configured."))?;
        if !agent.enabled {
            return Err(format!("ACP agent `{}` is disabled.", agent.id));
        }
        self.resolve_ai_acp_plugin(&mut agent, cx)?;

        let agent_id = agent.id.clone();
        let display_name = if agent.display_name.trim().is_empty() {
            agent_id.clone()
        } else {
            agent.display_name.clone()
        };
        let session_cwd = std::env::current_dir().unwrap_or_else(|_| {
            agent
                .cwd
                .as_deref()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("."))
        });
        let host_policy = oxideterm_ai::AcpHostCapabilityPolicy {
            fs_read_text_file: agent.capability_policy.fs_read_text_file,
            fs_write_text_file: agent.capability_policy.fs_write_text_file,
            terminal: agent.capability_policy.terminal,
        };
        // Copy token-bearing args and environment values exactly once into the
        // zeroizing launch owner that is moved to the ACP worker.
        let launch_config = oxideterm_ai::AcpLaunchConfig {
            id: agent_id,
            display_name,
            command: agent.command,
            args: agent.args,
            env: agent.env,
            cwd: agent.cwd.as_deref().map(std::path::PathBuf::from),
        };
        Ok(Some(AiAcpChatLaunch {
            launch_config,
            session_cwd,
            host_policy,
        }))
    }

    pub(in crate::workspace) fn resolve_ai_tool_approval(
        &mut self,
        generation: u64,
        tool_call_id: String,
        approved: bool,
        cx: &mut Context<Self>,
    ) {
        self.ai.state.update(cx, |ai, _cx| {
            ai.resolve_tool_approval(generation, &tool_call_id, approved);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn resolve_ai_acp_permission(
        &mut self,
        generation: u64,
        tool_call_id: String,
        option_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.ai.state.update(cx, |ai, _cx| {
            ai.resolve_acp_permission_choice(generation, &tool_call_id, option_id);
        });
        self.ai.acp.update(cx, |entity, _cx| {
            entity.remove_file_write_preview(&tool_call_id);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn resolve_ai_tool_candidate_selection(
        &mut self,
        generation: u64,
        tool_call_id: String,
        selected_index: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.ai.state.update(cx, |ai, _cx| {
            ai.resolve_tool_candidate_selection(generation, &tool_call_id, selected_index);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn execute_ai_ui_orchestrator_tool(
        &mut self,
        conversation_id: &str,
        tool_session_id: &ToolSessionId,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AiExecutedToolResult {
        let started = std::time::Instant::now();
        let current_snapshot =
            self.ai_orchestrator_snapshot_for_tool_session(Some(tool_session_id), cx);
        let result = dispatch_ai_application_tool(&current_snapshot, &tool_name, &args, |request| {
            self.invoke_ai_application_tool(request, conversation_id, tool_session_id, &args, &current_snapshot, window, cx)
        });
        self.ai_orchestrator_snapshot_for_tool_session(Some(tool_session_id), cx)
            .to_executed_tool_result(
                tool_call_id,
                tool_name,
                result,
                started.elapsed().as_millis(),
            )
    }

    /// Executes only stable, non-live reads on the UI-owned broker. Live file
    /// and SFTP operations remain unavailable until their real owners expose
    /// capability adapters.
    fn invoke_ai_application_tool(
        &mut self,
        request: AiApplicationTool,
        conversation_id: &str,
        tool_session_id: &ToolSessionId,
        args: &serde_json::Value,
        snapshot: &AiOrchestratorRuntimeSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        use AiApplicationTool::*;
        let result = match request {
            ConnectTarget => return self.execute_ai_connect_target(args, window, cx),
            ObserveTerminal => return self.execute_ai_observe_terminal(tool_session_id, args, cx),
            SendTerminalInput => return {
                self.execute_ai_send_terminal_input(tool_session_id, args, window, cx)
            },
            GetTerminalCommandStatus => return {
                self.execute_ai_get_terminal_command_status(tool_session_id, args, cx)
            },
            ReadResource => return self.execute_ai_read_stable_resource(args, cx),
            WriteResource => return self.execute_ai_write_settings_resource(args, window, cx),
            OpenAppSurface => return {
                self.execute_ai_open_app_surface(tool_session_id, args, window, cx)
            },
            GetState => return self.execute_ai_get_state(tool_session_id, args, cx),
            InspectHostTools => self.execute_ai_inspect_host_tools(tool_session_id, args, cx),
            ControlHostTool => self.execute_ai_control_host_tool(tool_session_id, args, cx),
            ListForwards => self.execute_ai_list_forwards(tool_session_id, args, cx),
            ManageForward => self.execute_ai_manage_forward(tool_session_id, args, cx),
            ListPlugins => Ok(self.execute_ai_list_plugins(cx)),
            ManagePlugin => self.execute_ai_manage_plugin(args, window, cx),
            ListTransportProfiles => Ok(self.execute_ai_list_transport_profiles()),
            OpenTransportProfile => self.execute_ai_open_transport_profile(args, window, cx),
            GetTransportSessionState => self.execute_ai_get_transport_session_state(tool_session_id, args, cx),
            ManageSerialSession => self.execute_ai_manage_serial_session(tool_session_id, args, cx),
            ManageTelnetSession => self.execute_ai_manage_telnet_session(tool_session_id, args, cx),
            ListRemoteDesktopSessions => Ok(self.execute_ai_list_remote_desktop_sessions(cx)),
            ManageRemoteDesktopSession => self.execute_ai_manage_remote_desktop_session(args, window, cx),
            GetCloudSyncState => Ok(self.execute_ai_get_cloud_sync_state(cx)),
            ConfigureCloudSync => self.execute_ai_configure_cloud_sync(args, cx),
            ManageCloudSync => self.execute_ai_manage_cloud_sync(args, window, cx),
            ListCredentials => self.execute_ai_list_credentials(args),
            ManageCredential => self.execute_ai_manage_credential(args, window, cx),
            ListMemoryEntries => Ok(self.execute_ai_list_memory_entries(args)),
            ManageMemoryEntry => self.execute_ai_manage_memory_entry(args, cx),
            CreateBackgroundTask => self.execute_ai_create_background_task(conversation_id, args, cx),
            ListBackgroundTasks => Ok(self.execute_ai_list_background_tasks(conversation_id, cx)),
            GetBackgroundTask => self.execute_ai_get_background_task(conversation_id, args, cx),
            CancelBackgroundTask => self.execute_ai_cancel_background_task(conversation_id, args, cx),
            RememberPreference => return self.execute_ai_remember_preference(args, cx),
            LoadSkill => return {
                self.ai.state.update(cx, |state, _| state.execute_ai_load_skill(conversation_id, args, &snapshot, &self.ai.skills, self.settings_store.settings().ai.skills.enabled))
            },
            ReadSkillResource => return {
                self.ai.state.read(cx).execute_ai_read_skill_resource(conversation_id, args, &snapshot, &self.ai.skills, self.settings_store.settings().ai.skills.enabled)
            },
        };
        request.result(snapshot, result)
    }

    fn execute_ai_read_stable_resource(&self, args: &serde_json::Value, cx: &mut Context<Self>) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        ai_read_stable_resource(&snapshot, args, || self.ai.state.read(cx).rag_store())
    }

    fn execute_ai_get_state(
        &self, tool_session_id: &ToolSessionId, args: &serde_json::Value, cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        oxideterm_ai::tool_resources::ai_target_state(
            &snapshot, self.ai.authority.read(cx), tool_session_id, args,
            |id| self.connection_store.get(id).is_some(),
        )
    }

    pub(in crate::workspace) fn start_ai_ui_orchestrator_tool_execution(
        &mut self,
        owner: AiToolRunContext,
        tool_session_id: ToolSessionId,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        post_user_approval: bool,
        dangerous_command_approved: bool,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let conversation_id = owner.conversation_id.clone();
        if oxideterm_ai::web::is_web_tool(&tool_name) {
            self.start_ai_web_execution(owner, tool_call_id, tool_name, args, sender, cx);
            return;
        }
        let args = match self.prepare_ai_runtime_authority(&tool_session_id, &tool_name, args, cx) {
            Ok(args) => args,
            Err(error) => {
                let snapshot =
                    self.ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx);
                let result = snapshot.to_executed_tool_result(
                    tool_call_id,
                    tool_name,
                    snapshot.fail(
                        "Runtime target is unavailable.",
                        ai_runtime_validation_public_code(&error, post_user_approval),
                        ai_runtime_validation_recovery_message(post_user_approval),
                        "interactive",
                    ),
                    0,
                );
                let _ = sender.send(result);
                return;
            }
        };
        if tool_name == "connect_target" {
            self.start_ai_connect_target_execution(
                &conversation_id,
                tool_session_id,
                tool_call_id,
                tool_name,
                args,
                sender,
                window,
                cx,
            );
            return;
        }
        if matches!(
            tool_name.as_str(),
            "read_resource" | "write_resource" | "transfer_resource"
        ) && args.get("handle_id").is_some()
        {
            self.start_ai_live_resource_execution(
                tool_session_id,
                tool_call_id,
                tool_name,
                args,
                post_user_approval,
                sender,
                cx,
            );
            return;
        }
        if tool_name == "run_command" {
            self.start_ai_terminal_run_command_execution(
                owner,
                tool_session_id,
                tool_call_id,
                tool_name,
                args,
                post_user_approval,
                dangerous_command_approved,
                sender,
                window,
                cx,
            );
            return;
        }
        if tool_name == "wait_terminal_output" {
            self.start_ai_wait_terminal_output_execution(
                owner,
                tool_session_id,
                tool_call_id,
                tool_name,
                args,
                post_user_approval,
                sender,
                cx,
            );
            return;
        }
        let result = self.execute_ai_ui_orchestrator_tool(
            &conversation_id,
            &tool_session_id,
            tool_call_id,
            tool_name,
            args,
            window,
            cx,
        );
        let _ = sender.send(result);
    }

    /// Validates a live resource handle once more after approval, then hands
    /// the real owner adapter to the backend task without exposing internals.
    fn start_ai_live_resource_execution(
        &mut self,
        tool_session_id: ToolSessionId,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        post_user_approval: bool,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        cx: &mut Context<Self>,
    ) {
        let started = std::time::Instant::now();
        let operation = match ai_live_resource_operation(&tool_name, &args) {
            Ok(operation) => operation,
            Err(error) => {
                self.send_ai_live_resource_validation_failure(
                    &tool_session_id,
                    tool_call_id,
                    tool_name,
                    sender,
                    error,
                    post_user_approval,
                    started.elapsed().as_millis(),
                    cx,
                );
                return;
            }
        };
        let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
        let (node_id, sftp_owner, ide_file_system) = if operation.requires_ide_owner() {
            let (tab_id, node_id) = match self.ai.authority.read(cx).validate_ide_handle(
                &tool_session_id,
                raw_handle_id,
                operation.capability(),
            ) {
                Ok(owner) => owner,
                Err(error) => {
                    self.send_ai_live_resource_validation_failure(
                        &tool_session_id,
                        tool_call_id,
                        tool_name,
                        sender,
                        error,
                        post_user_approval,
                        started.elapsed().as_millis(),
                        cx,
                    );
                    return;
                }
            };
            let Some(file_system) = self.ide_workspace.read(cx).ai_owner_file_system(tab_id, cx)
            else {
                self.send_ai_live_resource_validation_failure(
                    &tool_session_id,
                    tool_call_id,
                    tool_name,
                    sender,
                    oxideterm_ai::RuntimeValidationError::new(
                        oxideterm_ai::RuntimeValidationFailure::OwnerClosed,
                    ),
                    post_user_approval,
                    started.elapsed().as_millis(),
                    cx,
                );
                return;
            };
            (node_id, None, Some(file_system))
        } else {
            let owner = match self.ai.authority.read(cx).validate_sftp_handle(
                &tool_session_id,
                raw_handle_id,
                operation.capability(),
            ) {
                Ok(node_id) => node_id,
                Err(error) => {
                    self.send_ai_live_resource_validation_failure(
                        &tool_session_id,
                        tool_call_id,
                        tool_name,
                        sender,
                        error,
                        post_user_approval,
                        started.elapsed().as_millis(),
                        cx,
                    );
                    return;
                }
            };
            (owner.node_id.clone(), Some(owner), None)
        };
        let snapshot = self.ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx);
        let services = self.ai_live_tool_services();
        let audit_context = oxideterm_audit::AuditContext::current_request();
        let execution = async move {
            let mut sender = sender;
            let operation = async {
                match operation {
                    AiLiveResourceOperation::SftpRead | AiLiveResourceOperation::IdeRead => {
                        read_live_resource(
                            &snapshot,
                            &services,
                            node_id,
                            sftp_owner,
                            &args,
                            ide_file_system,
                            post_user_approval,
                        )
                        .await
                    }
                    AiLiveResourceOperation::SftpWrite | AiLiveResourceOperation::IdeWrite => {
                        write_live_resource(
                            &snapshot,
                            &services,
                            node_id,
                            sftp_owner,
                            &args,
                            ide_file_system,
                            post_user_approval,
                        )
                        .await
                    }
                    AiLiveResourceOperation::SftpStartTransfer => match sftp_owner {
                        Some(owner) => {
                            transfer_live_resource(
                                &snapshot,
                                &services,
                                owner,
                                &args,
                                post_user_approval,
                            )
                            .await
                        }
                        None => snapshot.fail(
                            "SFTP capability is unavailable.",
                            "runtime_capability_unavailable",
                            "Rediscover the current SFTP session before retrying.",
                            "write",
                        ),
                    },
                }
            };
            let action = tokio::select! {
                action = operation => Some(action),
                _ = sender.closed() => None,
            };
            if let Some(action) = action {
                let _ = sender.send(snapshot.to_executed_tool_result(
                    tool_call_id,
                    tool_name,
                    action,
                    started.elapsed().as_millis(),
                ));
            }
        };
        self.forwarding_runtime
            .spawn(oxideterm_audit::AuditContext::scope_optional(
                audit_context,
                execution,
            ));
    }

    fn send_ai_live_resource_validation_failure(
        &mut self,
        tool_session_id: &ToolSessionId,
        tool_call_id: String,
        tool_name: String,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        error: oxideterm_ai::RuntimeValidationError,
        post_user_approval: bool,
        duration_ms: u128,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.ai_orchestrator_snapshot_for_tool_session(Some(tool_session_id), cx);
        let result = snapshot.to_executed_tool_result(
            tool_call_id,
            tool_name,
            snapshot.fail(
                "Runtime resource is unavailable.",
                ai_runtime_validation_public_code(&error, post_user_approval),
                ai_runtime_validation_recovery_message(post_user_approval),
                "write",
            ),
            duration_ms,
        );
        let _ = sender.send(result);
    }

    fn prepare_ai_runtime_authority(
        &self,
        tool_session_id: &ToolSessionId,
        tool_name: &str,
        args: serde_json::Value,
        cx: &App,
    ) -> Result<serde_json::Value, oxideterm_ai::RuntimeValidationError> {
        self.ai.authority.read(cx).validate_tool_authority(tool_session_id, tool_name, &args,
            |id| self.connection_store.get(id).is_some(),
            |id| self.connection_store.get(id)
                .and_then(|connection| self.indexed_saved_ssh_node_for_connection(id, connection)).is_some(),
        )?;
        Ok(args)
    }

    /// Validates authority before policy approval. Execution repeats validation
    /// immediately before dispatch to close the approval-time state-change gap.
    pub(in crate::workspace) fn preflight_ai_ui_orchestrator_tool(
        &self,
        tool_session_id: &ToolSessionId,
        tool_name: &str,
        args: &serde_json::Value,
        cx: &App,
    ) -> Result<(), oxideterm_ai::RuntimeValidationError> {
        if matches!(
            tool_name,
            "connect_target"
                | "run_command"
                | "observe_terminal"
                | "send_terminal_input"
                | "get_transport_session_state"
                | "manage_serial_session"
                | "manage_telnet_session"
                | "wait_terminal_output"
                | "get_terminal_command_status"
                | "read_resource"
                | "write_resource"
                | "transfer_resource"
                | "open_app_surface"
                | "get_state"
                | "inspect_host_tools"
                | "control_host_tool"
                | "list_forwards"
                | "manage_forward"
        ) {
            self.prepare_ai_runtime_authority(tool_session_id, tool_name, args.clone(), cx)
                .map(|_| ())
        } else {
            Ok(())
        }
    }

    pub(in crate::workspace) fn start_ai_connect_target_execution(
        &mut self,
        conversation_id: &str,
        tool_session_id: ToolSessionId,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let started = std::time::Instant::now();
        let base = self.execute_ai_ui_orchestrator_tool(
            conversation_id,
            &tool_session_id,
            tool_call_id.clone(),
            tool_name.clone(),
            args.clone(),
            window,
            cx,
        );
        if !base.success {
            let _ = sender.send(base);
            return;
        }
        if base
            .envelope
            .get("summary")
            .and_then(serde_json::Value::as_str)
            == Some("Target is already live.")
        {
            let _ = sender.send(base);
            return;
        }
        if let Some(ready) = self.ai_connect_target_ready_result(
            &tool_session_id,
            &tool_call_id,
            &tool_name,
            &args,
            started.elapsed().as_millis(),
            cx,
        ) {
            let _ = sender.send(ready);
            return;
        }
        cx.spawn(async move |weak, cx| {
            let mut sender = Some(sender);
            for _ in 0..AI_CONNECT_TARGET_TIMEOUT_TICKS {
                // Tauri waits for connectToSaved to finish before returning
                // connect_target. Keep native's UI-thread bridge patient enough
                // for slow SSH/proxy chains, while still polling the snapshot.
                Timer::after(Duration::from_millis(AI_CONNECT_TARGET_POLL_INTERVAL_MS)).await;
                let ready = weak.update(cx, |this, cx| {
                    this.ai_connect_target_ready_result(
                        &tool_session_id,
                        &tool_call_id,
                        &tool_name,
                        &args,
                        started.elapsed().as_millis(),
                        cx,
                    )
                });
                match ready {
                    Ok(Some(result)) => {
                        if let Some(sender) = sender.take() {
                            let _ = sender.send(result);
                        }
                        return;
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
            if let Some(sender) = sender.take() {
                let result = weak.update(cx, |this, cx| {
                    this.ai_connect_target_timeout_result(
                        &tool_session_id,
                        &tool_call_id,
                        &tool_name,
                        &args,
                        &base,
                        started.elapsed().as_millis(),
                        cx,
                    )
                });
                let _ = sender.send(result.unwrap_or(base));
            }
        })
        .detach();
    }

    pub(in crate::workspace) fn execute_ai_connect_target(
        &mut self,
        args: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let resource_ref = match snapshot.plan_saved_connection(args) {
            Ok(resource) => resource,
            Err(result) => return result,
        };
        let Some(connection) = self.connection_store.get(resource_ref.id()).cloned() else {
            return snapshot.saved_connection_removed();
        };
        let Some(config) = oxideterm_session_adapter::ssh_config_from_saved_connection(
            &self.connection_store,
            self.settings_store.settings(),
            &connection,
        ) else {
            if self.try_reuse_active_saved_connection_terminal(
                resource_ref.id(),
                &connection,
                window,
                cx,
            ) {
                return snapshot.saved_connection_opened(resource_ref, true);
            }
            return snapshot.saved_connection_credentials_required(&resource_ref);
        };
        let title = oxideterm_ai::tool_resources::ai_connection_title(&connection.name, &connection.username, &connection.host);
        self.start_saved_connection_flow(
            resource_ref.id().to_string(),
            config,
            title,
            None,
            connection.options.open_sftp_on_connect,
            window,
            cx,
        );
        snapshot.saved_connection_opened(resource_ref, false)
    }

    pub(in crate::workspace) fn execute_ai_observe_terminal(
        &self,
        tool_session_id: &ToolSessionId,
        args: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
        let session_id = match self.ai.authority.read(cx).validate_terminal_handle(
            tool_session_id,
            raw_handle_id,
            oxideterm_ai::RuntimeCapability::TerminalObserve,
        ) {
            Ok(session_id) => session_id,
            Err(error) => {
                return snapshot.fail(
                    "Runtime terminal is unavailable.",
                    error.public_code(),
                    "Rediscover the current terminal before observing it.",
                    "read",
                );
            }
        };
        let Some(target) = snapshot.terminal_target(session_id) else {
            return snapshot.fail("Runtime terminal is unavailable.", "runtime_owner_closed", "The terminal pane is no longer registered.", "read");
        };
        let pane = self.ai_terminal_pane_for_session(session_id, cx);
        let pane = pane.as_ref().map(|pane| pane.read(cx));
        let records = pane.map(|pane| pane.ai_command_records()).unwrap_or_default();
        snapshot.terminal_observed(target, args, records.iter().rev().map(|record| {
            ai_terminal_command_capture(record, pane.is_some_and(|pane| pane.ai_command_prompt_returned(&record.command_id)))
        }), pane.is_some_and(|pane| pane.ai_waiting_for_secret()))
    }

    pub(in crate::workspace) fn execute_ai_send_terminal_input(
        &mut self,
        tool_session_id: &ToolSessionId,
        args: &serde_json::Value,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
        let session_id = match self.ai.authority.read(cx).validate_terminal_handle(
            tool_session_id,
            raw_handle_id,
            oxideterm_ai::RuntimeCapability::TerminalSendInput,
        ) {
            Ok(session_id) => session_id,
            Err(error) => {
                return snapshot.fail(
                    "Runtime terminal is unavailable.",
                    error.public_code(),
                    "Rediscover the current terminal before sending input.",
                    "interactive",
                );
            }
        };
        let target = snapshot.terminal_target(session_id);
        let Some((_pane_id, pane)) = self.ai_terminal_session(session_id, cx) else {
            return snapshot
                .fail(
                    "Runtime terminal is unavailable.",
                    "runtime_owner_closed",
                    "The terminal pane is no longer registered.",
                    "interactive",
                )
                .with_optional_target(target);
        };
        // Interactive input can contain passwords; wipe the assembled payload
        // immediately after it crosses into the terminal owner.
        let payload = zeroize::Zeroizing::new(ai_terminal_input_payload(args));
        if payload.is_empty() {
            return snapshot
                .fail(
                    "No terminal input specified.",
                    "missing_terminal_input",
                    "Provide text or request Enter with append_enter.",
                    "interactive",
                )
                .with_optional_target(target);
        }
        if !pane.read(cx).ai_accepts_input() {
            return snapshot
                .fail(
                    "Failed to send terminal input.",
                    "terminal_send_failed",
                    "The terminal writer is no longer available.",
                    "interactive",
                )
                .with_optional_target(target);
        }
        pane.update(cx, |pane, cx| {
            pane.send_ai_input_bytes(&payload, cx);
        });
        snapshot
            .ok(
                "Terminal input sent.",
                "Input sent.",
                serde_json::Value::Null,
                "interactive",
            )
            .with_optional_target(target)
    }

    pub(in crate::workspace) fn execute_ai_get_terminal_command_status(
        &self,
        tool_session_id: &ToolSessionId,
        args: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
        let session_id = match self.ai.authority.read(cx).validate_terminal_handle(
            tool_session_id,
            raw_handle_id,
            oxideterm_ai::RuntimeCapability::TerminalObserve,
        ) {
            Ok(session_id) => session_id,
            Err(error) => {
                return snapshot.fail(
                    "Runtime terminal is unavailable.",
                    error.public_code(),
                    "Rediscover the current terminal before reading command status.",
                    "read",
                );
            }
        };
        let target = snapshot.terminal_target(session_id);
        let Some(pane) = self.ai_terminal_pane_for_session(session_id, cx) else {
            return snapshot
                .fail(
                    "Runtime terminal is unavailable.",
                    "runtime_owner_closed",
                    "The terminal pane is no longer registered.",
                    "read",
                )
                .with_optional_target(target);
        };
        let pane = pane.read(cx);
        let records = pane.ai_command_records();
        snapshot.terminal_command_status(target, args, records.iter().rev().map(|record| {
            ai_terminal_command_capture(record, pane.ai_command_prompt_returned(&record.command_id))
        }))
    }

    pub(in crate::workspace) fn start_ai_terminal_run_command_execution(
        &mut self,
        owner: AiToolRunContext,
        tool_session_id: ToolSessionId,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        post_user_approval: bool,
        dangerous_command_approved: bool,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let execution_leases = self
            .ai
            .state
            .read(cx)
            .agents
            .tool_leases
            .get(&(tool_session_id.clone(), tool_call_id.clone()))
            .cloned()
            .unwrap_or_default();
        let started = std::time::Instant::now();
        let snapshot = self.ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx);
        let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
        let mut handle_id = raw_handle_id
            .and_then(|value| oxideterm_ai::RuntimeHandleId::parse(value.to_string()).ok());
        let command_owner = match self
            .ai.authority
            .read(cx)
            .validate_run_command_handle(
                &tool_session_id,
                handle_id
                    .as_ref()
                    .map(oxideterm_ai::RuntimeHandleId::as_str),
            ) {
            Ok(owner) => owner,
            Err(error) => {
                let result = snapshot.to_executed_tool_result(
                    tool_call_id,
                    tool_name,
                    snapshot.fail(
                        "Runtime command target is unavailable.",
                        ai_runtime_validation_public_code(&error, post_user_approval),
                        ai_runtime_validation_recovery_message(post_user_approval),
                        ai_run_command_preflight_risk(),
                    ),
                    started.elapsed().as_millis(),
                );
                let _ = sender.send(result);
                return;
            }
        };
        let Some(command) = args
            .get("command")
            .and_then(serde_json::Value::as_str)
            .filter(|command| !command.trim().is_empty())
            .map(|command| zeroize::Zeroizing::new(command.to_string()))
        else {
            let result = snapshot.to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot.fail(
                    "Command is required.",
                    "missing_command",
                    "run_command requires a command.",
                    ai_run_command_preflight_risk(),
                ),
                started.elapsed().as_millis(),
            );
            let _ = sender.send(result);
            return;
        };
        let wait_timeout = Duration::from_secs(
            args.get("timeout_secs")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1800)
                .clamp(1, 1800),
        );
        if command_owner == crate::workspace::ai_runtime_context::AiRunCommandOwner::LocalShell {
            for lease in &execution_leases {
                lease.dispatched();
            }
            let cwd = args
                .get("cwd")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            self.start_ai_owned_local_command(
                owner,
                tool_call_id,
                tool_name,
                command,
                cwd,
                dangerous_command_approved,
                wait_timeout,
                snapshot,
                execution_leases,
                sender,
                cx,
            );
            return;
        }
        let crate::workspace::ai_runtime_context::AiRunCommandOwner::Terminal(session_id) =
            command_owner
        else {
            unreachable!("local shell command returned before terminal dispatch");
        };
        let node_id = self
            .workspace_runtime
            .read(cx)
            .ssh_terminal_nodes()
            .into_iter()
            .find_map(|(id, node)| (id == session_id).then_some(node));
        let target = snapshot.terminal_target(session_id);
        let command = zeroize::Zeroizing::new(ai_command_with_cwd(
            &command,
            args.get("cwd").and_then(serde_json::Value::as_str),
        ));
        let Some((_pane_id, pane)) = self.ai_terminal_session(session_id, cx) else {
            let result = snapshot.to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot
                    .fail(
                        "Runtime terminal is unavailable.",
                        "runtime_owner_closed",
                        "The terminal pane is no longer registered.",
                        "interactive",
                    )
                    .with_optional_target(target),
                started.elapsed().as_millis(),
            );
            let _ = sender.send(result);
            return;
        };
        if !pane.read(cx).ai_accepts_input() {
            let result = snapshot.to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot
                    .fail(
                        "Terminal is not ready.",
                        "terminal_not_ready",
                        "The terminal writer is no longer available.",
                        "interactive",
                    )
                    .with_optional_target(target),
                started.elapsed().as_millis(),
            );
            let _ = sender.send(result);
            return;
        }
        if pane.read(cx).ai_command_input_pending() {
            let _ = sender.send(rejected_ai_tool_result(tool_call_id, tool_name,
                "terminal_command_pending",
                "The previous command has not exited or returned a stable shell prompt. Observe the terminal, continue waiting, or handle its interactive input before running another command."));
            return;
        }
        let before = pane.read(cx).ai_buffer_snapshot();
        let (activity, mut activity_rx) = tokio::sync::watch::channel(0u64);
        let runtime_activity = activity.clone();
        let runtime_subscription = cx.observe(&self.workspace_runtime, move |_, _, _| {
            runtime_activity.send_modify(|revision| *revision = revision.wrapping_add(1));
        });
        let subscription = cx.subscribe(&pane, move |_, _, event, _| {
            if matches!(
                event,
                TerminalPaneEvent::OutputActivity
                    | TerminalPaneEvent::Exited { .. }
                    | TerminalPaneEvent::PrivilegePromptStateChanged
            ) {
                activity.send_modify(|revision| *revision = revision.wrapping_add(1));
            }
        });
        if execution_leases.iter().any(|lease| !lease.is_current()) {
            let _ = sender.send(rejected_ai_tool_result(
                tool_call_id,
                tool_name,
                "terminal_taken_over",
                "User input took control of the terminal before dispatch.",
            ));
            return;
        }
        for lease in &execution_leases {
            lease.dispatched();
        }
        let command_id = pane.update(cx, |pane, cx| {
            let command_id =
                pane.begin_command_mark(&command, TerminalCommandMarkDetectionSource::Ai, cx);
            pane.send_command_line(&command, cx);
            command_id
        });
        let command_resource = owner.resource(
            self,
            cx,
            oxideterm_ai::agent::OwnedResourceKind::TerminalCommand,
            &self.ai_tool_display_name("run_command"),
        );
        self.monitor_ai_terminal_command(
            session_id,
            node_id.clone(),
            command_id.clone(),
            &before,
            execution_leases.clone(),
            command_resource,
            cx,
        );
        if args
            .get("await_output")
            .and_then(serde_json::Value::as_bool)
            == Some(false)
        {
            let result = snapshot.to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot
                    .ok(
                        "Command sent to terminal.",
                        "Command sent to the visible terminal.",
                        serde_json::json!({
                            "executionState": "sent",
                            "visibleInTerminal": true,
                            "commandId": command_id,
                        }),
                        "interactive",
                    )
                    .with_optional_target(target),
                started.elapsed().as_millis(),
            );
            let _ = sender.send(result);
            return;
        }

        let observation = owner.resource(
            self,
            cx,
            oxideterm_ai::agent::OwnedResourceKind::Observation,
            &self.i18n.t("settings_view.ai.waiting_condition"),
        );
        cx.spawn(async move |weak, cx| {
            let _observation = observation;
            let _subscription = subscription;
            let _runtime_subscription = runtime_subscription;
            let mut connection_lost = false;
            let mut outcome_unknown = false;
            let mut sender = Some(sender);
            let mut direction_changed = false;
            let mut previous_wait_state = "";
            let mut last = before.clone();
            let mut changed_at = std::time::Instant::now();
            let mut owner_closed = false;
            let mut user_taken_over = false;
            let wait = AiTerminalCommandWait::with_timeout(std::time::Instant::now(), wait_timeout);
            let mut last_waiting_for_secret = false;
            loop {
                if sender.as_ref().is_none_or(tokio::sync::oneshot::Sender::is_closed) {
                    return;
                }
                if owner.dispatch.as_ref().is_some_and(|guard| guard.check().is_err()) {
                    direction_changed = true;
                    break;
                }
                if execution_leases.iter().any(|lease| !lease.is_current()) {
                    user_taken_over = true;
                    break;
                }
                let current = weak.update(cx, |this, cx| {
                    // Do not let the retained pane entity become authority
                    // after its terminal owner or tool session is revoked.
                    let recovering = node_id.as_ref().and_then(|node| this.node_router.connection_id_for_node(node))
                        .and_then(|id| this.ssh_registry.get(&id))
                        .is_some_and(|connection| matches!(connection.state(), ConnectionState::Connecting | ConnectionState::LinkDown | ConnectionState::Reconnecting | ConnectionState::Error(_)));
                    if recovering {
                        connection_lost = true;
                        return Some((last.clone(), false, None, true, false));
                    }
                    if let Some(node) = &node_id {
                        let alive = this.node_router.connection_id_for_node(node).and_then(|id| this.ssh_registry.get(&id))
                            .is_some_and(|connection| !matches!(connection.state(), ConnectionState::Disconnected | ConnectionState::Disconnecting));
                        if !alive { return None; }
                    }
                    if connection_lost {
                        handle_id = this.ai.authority.update(cx, |runtime, _|
                            runtime.issue_terminal_handle(&tool_session_id, session_id).ok())
                            .map(|projection| projection.handle_id);
                    }
                    let current_session = this
                        .ai.authority
                        .read(cx)
                        .validate_terminal_handle(
                            &tool_session_id,
                            handle_id
                                .as_ref()
                                .map(oxideterm_ai::RuntimeHandleId::as_str),
                            oxideterm_ai::RuntimeCapability::TerminalRunCommand,
                        )
                        .ok();
                    if current_session != Some(session_id) { return None; }
                    let (_, current_pane) = this.ai_terminal_session(session_id, cx)?;
                    Some({
                        let pane = current_pane.read(cx);
                        (
                            pane.ai_buffer_snapshot(),
                            pane.ai_waiting_for_secret(),
                            command_id.as_ref().and_then(|command_id| {
                                pane.ai_command_records()
                                    .into_iter()
                                    .find(|record| record.command_id == *command_id)
                            }),
                            false,
                            command_id.as_deref().is_some_and(|id| pane.ai_command_prompt_returned(id)),
                        )
                    })
                });
                let (current, waiting_for_secret, command_record, recovering, prompt_returned) = match current {
                    Ok(Some(current)) => current,
                    Ok(None) => {
                        owner_closed = true;
                        break;
                    }
                    Err(_) => break,
                };
                if current != last {
                    last = current.clone();
                    changed_at = std::time::Instant::now();
                }
                let command_completed = command_record.as_ref().is_some_and(|record| {
                    record.status == oxideterm_gpui_terminal::TerminalCommandFactStatus::Closed
                });
                if !recovering && (connection_lost && command_record.is_none()
                    || command_record.as_ref().is_some_and(|record| record.status == oxideterm_gpui_terminal::TerminalCommandFactStatus::Stale)) {
                    outcome_unknown = true;
                    break;
                }
                if !recovering { connection_lost = false; }
                last_waiting_for_secret = waiting_for_secret;
                let expired = wait.expired(std::time::Instant::now());
                if recovering && expired {
                    outcome_unknown = true;
                    break;
                }
                if expired || ai_terminal_command_output_ready(
                    command_completed, prompt_returned, &before, &current,
                    changed_at.elapsed(), waiting_for_secret, recovering,
                ) {
                    let result = weak.update(cx, |this, cx| {
                        let current_snapshot = this
                            .ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx);
                        current_snapshot.to_executed_tool_result(
                            tool_call_id.clone(),
                            tool_name.clone(),
                            current_snapshot.terminal_run_observed(
                                command_id.as_deref(), &before, &current, command_completed, prompt_returned,
                                waiting_for_secret, command_record.as_ref().and_then(|record| record.exit_code), expired, target.clone(),
                            ),
                            started.elapsed().as_millis(),
                        )
                    });
                    if let (Some(sender), Ok(result)) = (sender.take(), result) {
                        let _ = sender.send(result);
                    }
                    return;
                }
                // Credentials remain local even when a bounded observation yields to the model.
                let wait_state = if recovering { "waiting_connection" } else if waiting_for_secret { "waiting_user" } else { "waiting_condition" };
                if wait_state != previous_wait_state {
                    let _ = weak.update(cx, |this, cx| {
                        if let Some(run) = this.ai.state.read(cx).agent_run(owner.generation) {
                            let state = if recovering { AgentState::AwaitingConnection } else if waiting_for_secret { AgentState::AwaitingUser } else { AgentState::AwaitingCondition };
                            let _ = this.ai.state.read(cx).agents.services.runtime.set_state(&run, state);
                        }
                        this.apply_ai_tool_status(owner.generation,
                        &owner.conversation_id, &owner.assistant_id, &tool_call_id, &tool_name, &owner.arguments, wait_state,
                        Some(serde_json::json!({"commandId": command_id, "waiting": true, "waitDeadline": ai_now_ms() + wait.deadline().saturating_duration_since(std::time::Instant::now()).as_millis() as i64})), Some("interactive".into()),
                        None, false, None, None, None, cx);
                    });
                    previous_wait_state = wait_state;
                }
                tokio::select! {
                    _ = activity_rx.changed() => {},
                    _ = Timer::after(Duration::from_millis(250)) => {},
                    _ = sender.as_mut().unwrap().closed() => return,
                }
            }
            let result = weak.update(cx, |this, cx| {
                let current_snapshot =
                    this.ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx);
                let end = if outcome_unknown { AiTerminalRunEnd::OutcomeUnknown }
                    else if direction_changed { AiTerminalRunEnd::DirectionChanged }
                    else if user_taken_over { AiTerminalRunEnd::TakenOver }
                    else if owner_closed { AiTerminalRunEnd::OwnerClosed }
                    else { AiTerminalRunEnd::Unconfirmed };
                current_snapshot.terminal_run_finished(
                    tool_call_id, tool_name, started.elapsed().as_millis(), end, &before, &last,
                    command_id, last_waiting_for_secret, target, post_user_approval,
                )
            });
            if let (Some(sender), Ok(result)) = (sender.take(), result) {
                let _ = sender.send(result);
            }
        })
        .detach();
    }

    pub(in crate::workspace) fn start_ai_wait_terminal_output_execution(
        &mut self,
        owner: AiToolRunContext,
        tool_session_id: ToolSessionId,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        post_user_approval: bool,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        cx: &mut Context<Self>,
    ) {
        let started = std::time::Instant::now();
        let snapshot = self.ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx);
        let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
        let mut handle_id = raw_handle_id
            .and_then(|value| oxideterm_ai::RuntimeHandleId::parse(value.to_string()).ok());
        let session_id = match self.ai.authority.read(cx).validate_terminal_handle(
            &tool_session_id,
            handle_id
                .as_ref()
                .map(oxideterm_ai::RuntimeHandleId::as_str),
            oxideterm_ai::RuntimeCapability::TerminalObserve,
        ) {
            Ok(session_id) => session_id,
            Err(error) => {
                let result = snapshot.to_executed_tool_result(
                    tool_call_id,
                    tool_name,
                    snapshot.fail(
                        "Runtime terminal is unavailable.",
                        ai_runtime_validation_public_code(&error, post_user_approval),
                        ai_runtime_validation_recovery_message(post_user_approval),
                        "read",
                    ),
                    started.elapsed().as_millis(),
                );
                let _ = sender.send(result);
                return;
            }
        };
        let Some(pane) = self.ai_terminal_pane_for_session(session_id, cx) else {
            let result = snapshot.to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot.fail(
                    "Runtime terminal is unavailable.",
                    "runtime_owner_closed",
                    "The terminal pane is no longer registered.",
                    "read",
                ),
                started.elapsed().as_millis(),
            );
            let _ = sender.send(result);
            return;
        };
        let node_id = self
            .workspace_runtime
            .read(cx)
            .ssh_terminal_nodes()
            .into_iter()
            .find_map(|(id, node)| (id == session_id).then_some(node));
        let (activity, mut activity_rx) = tokio::sync::watch::channel(0u64);
        let runtime_activity = activity.clone();
        let runtime_subscription = cx.observe(&self.workspace_runtime, move |_, _, _| {
            runtime_activity.send_modify(|revision| *revision = revision.wrapping_add(1));
        });
        let subscription = cx.subscribe(&pane, move |_, _, event, _| {
            if matches!(
                event,
                TerminalPaneEvent::OutputActivity
                    | TerminalPaneEvent::Exited { .. }
                    | TerminalPaneEvent::PrivilegePromptStateChanged
            ) {
                activity.send_modify(|revision| *revision = revision.wrapping_add(1));
            }
        });
        let initial_buffer = pane.read(cx).ai_buffer_snapshot();
        let initial_alternate_screen = pane.read(cx).ai_screen_is_alternate_buffer();
        let observation = owner.resource(
            self,
            cx,
            oxideterm_ai::agent::OwnedResourceKind::Observation,
            &self.i18n.t("settings_view.ai.waiting_condition"),
        );
        cx.spawn(async move |weak, cx| {
            let _observation = observation;
            let _subscriptions = (subscription, runtime_subscription);
            let mut previous_wait_state = "";
            let mut sender = Some(sender);
            let mut wait = AiTerminalOutputWait::new(&args, initial_buffer, initial_alternate_screen, std::time::Instant::now());
            let failure = loop {
                if owner.dispatch.as_ref().is_some_and(|guard| guard.check().is_err()) {
                    break AiTerminalWaitFailure::DirectionChanged;
                }
                if sender.as_ref().is_none_or(tokio::sync::oneshot::Sender::is_closed) {
                    return;
                }
                let observation = weak.update(cx, |this, cx| {
                    // Revalidate the opaque handle on every observation so a
                    // retained pane cannot outlive the tool session authority.
                    let recovering = node_id.as_ref().and_then(|node| this.node_router.connection_id_for_node(node))
                        .and_then(|id| this.ssh_registry.get(&id)).is_some_and(|connection|
                            matches!(connection.state(), ConnectionState::Connecting | ConnectionState::LinkDown | ConnectionState::Reconnecting | ConnectionState::Error(_)));
                    if recovering { return Some((String::new(), false, Vec::new(), true, false, false)); }
                    if let Some(node) = &node_id {
                        let alive = this.node_router.connection_id_for_node(node).and_then(|id| this.ssh_registry.get(&id))
                            .is_some_and(|connection| !matches!(connection.state(), ConnectionState::Disconnected | ConnectionState::Disconnecting));
                        if !alive { return None; }
                    }
                    if wait.connection_lost() {
                        handle_id = this.ai.authority.update(cx, |runtime, _|
                            runtime.issue_terminal_handle(&tool_session_id, session_id).ok()).map(|projection| projection.handle_id);
                    }
                    let current_session = this
                        .ai.authority
                        .read(cx)
                        .validate_terminal_handle(
                            &tool_session_id,
                            handle_id
                                .as_ref()
                                .map(oxideterm_ai::RuntimeHandleId::as_str),
                            oxideterm_ai::RuntimeCapability::TerminalObserve,
                        )
                        .ok();
                    if current_session != Some(session_id) {
                        return None;
                    }
                    let (_, pane) = this.ai_terminal_session(session_id, cx)?;
                    let pane = pane.read(cx);
                    Some((
                        pane.ai_buffer_snapshot(),
                        pane.ai_screen_is_alternate_buffer(),
                        pane.ai_command_records(),
                        false,
                        pane.ai_waiting_for_secret(),
                        wait.requested_command()
                            .map(|id| pane.ai_command_prompt_returned(id))
                            .unwrap_or_else(|| pane.ai_command_records().last().is_some_and(|record| pane.ai_command_prompt_returned(&record.command_id))),
                    ))
                });
                let Ok(Some((buffer, alternate_screen, records, recovering, waiting_for_secret, prompt_returned))) = observation else {
                    break AiTerminalWaitFailure::OwnerClosed;
                };
                let state = if recovering { "waiting_connection" } else { "waiting_condition" };
                if state != previous_wait_state {
                    let _ = weak.update(cx, |this, cx| this.apply_ai_tool_status(owner.generation, &owner.conversation_id,
                        &owner.assistant_id, &tool_call_id, &tool_name, &owner.arguments, state,
                        Some(serde_json::json!({"waiting":true,"waitDeadline":ai_now_ms() + wait.deadline().saturating_duration_since(std::time::Instant::now()).as_millis() as i64})),
                        Some("read".into()), None, false, None, None, None, cx));
                    previous_wait_state = state;
                }
                if recovering {
                    if !wait.recovering(std::time::Instant::now()) {
                        break AiTerminalWaitFailure::TimedOut;
                    }
                    Timer::after(Duration::from_millis(250)).await;
                    continue;
                }
                let records = records.iter().map(|record| ai_terminal_command_capture(record, false)).collect::<Vec<_>>();
                match wait.observe(&snapshot, AiTerminalWaitSample {
                    buffer: &buffer, alternate_screen, waiting_for_secret, prompt_returned,
                }, &records, std::time::Instant::now()) {
                    AiTerminalWaitProgress::Pending => {},
                    AiTerminalWaitProgress::Failed(failure) => break failure,
                    AiTerminalWaitProgress::Observed(action) => {
                        let result = weak.update(cx, |this, cx| {
                            this.ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx)
                                .to_executed_tool_result(tool_call_id, tool_name, action, started.elapsed().as_millis())
                        });
                        if let (Some(sender), Ok(result)) = (sender.take(), result) { let _ = sender.send(result); }
                        return;
                    }
                }
                tokio::select! {
                    _ = activity_rx.changed() => {},
                    _ = Timer::after(Duration::from_millis(250)) => {},
                    _ = sender.as_mut().unwrap().closed() => return,
                }
            };
            let result = weak.update(cx, |this, cx| {
                let snapshot = this.ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx);
                let action = wait.failure(&snapshot, failure);
                snapshot.to_executed_tool_result(tool_call_id, tool_name, action, started.elapsed().as_millis())
            });
            if let (Some(sender), Ok(result)) = (sender.take(), result) {
                let _ = sender.send(result);
            }
        })
        .detach();
    }

    pub(in crate::workspace) fn execute_ai_write_settings_resource(
        &mut self, args: &serde_json::Value, window: &mut Window, cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let write = match plan_ai_settings_write(&snapshot, self.settings_store.settings(), args) {
            Ok(write) => write,
            Err(result) => return result,
        };
        self.edit_settings(|settings| *settings = write.settings, cx);
        self.open_ai_settings_section(Some(write.section), window, cx);
        write.result
    }

    pub(in crate::workspace) fn execute_ai_remember_preference(
        &mut self, args: &serde_json::Value, cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let preference = match ai_remember_preference_payload(&snapshot, args) {
            Ok(preference) => preference.to_owned(),
            Err(result) => return result,
        };
        self.edit_settings(move |settings| save_ai_user_preference(&mut settings.ai.memory, preference), cx);
        ai_preference_saved(&snapshot)
    }

    pub(in crate::workspace) fn execute_ai_open_app_surface(
        &mut self,
        tool_session_id: &ToolSessionId,
        args: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        if let Some(raw_handle_id) = args.get("handle_id").and_then(serde_json::Value::as_str) {
            let tab_id = match self
                .ai.authority
                .read(cx)
                .validate_app_surface_handle(tool_session_id, Some(raw_handle_id))
            {
                Ok(tab_id) => tab_id,
                Err(error) => {
                    return snapshot.fail(
                        "Application surface is unavailable.",
                        error.public_code(),
                        "Rediscover current application surfaces before retrying.",
                        "write",
                    );
                }
            };
            if !self.tabs(cx).iter().any(|tab| tab.id == tab_id) {
                return snapshot.fail(
                    "Application surface is unavailable.",
                    "runtime_owner_closed",
                    "The selected surface is no longer open.",
                    "write",
                );
            }
            if self.tab_host.read(cx).is_detached(tab_id) {
                self.focus_detached_tab_window(tab_id, cx);
            } else {
                self.set_main_window_active_tab(Some(tab_id), cx);
                self.sync_active_tab_surface(cx);
                self.focus_active_pane(window, cx);
                self.reveal_active_tab(window, cx);
            }
            cx.notify();
            return snapshot.ok(
                "Focused application surface.",
                "Focused the selected application surface.",
                serde_json::Value::Null,
                "write",
            );
        }
        let planned = match snapshot.plan_open_surface(args, self.active_ssh_node_id.clone()) {
            Ok(surface) => surface,
            Err(result) => return result,
        };
        match &planned {
            AiAppSurface::LocalTerminal(_) => {
                if let Err(error) = self.create_local_terminal_tab(window, cx) {
                    return snapshot.fail("Failed to open local terminal.", "open_local_terminal_failed", error.to_string(), "write");
                }
                let active_tab = self.active_tab_id(cx);
                return self.ai_orchestrator_snapshot(cx).opened_surface(planned, active_tab);
            }
            AiAppSurface::Settings => self.open_ai_settings_section(args.get("section").and_then(serde_json::Value::as_str), window, cx),
            AiAppSurface::ConnectionManager => { self.open_session_manager_tab(window, cx); }
            AiAppSurface::Runtime => { self.open_connection_pool_tab(window, cx); }
            AiAppSurface::HostTools => { self.open_context_sidebar_panel(ContextSidebarPanel::HostTools, cx); }
            AiAppSurface::FileManager => { self.open_file_manager_tab(window, cx); }
            AiAppSurface::Sftp(node) => { self.open_sftp_tab(node.clone(), window, cx); }
            AiAppSurface::Ide(node) => { self.open_ide_folder_picker_tab(node.clone(), window, cx); }
        }
        snapshot.opened_surface(planned, None)
    }

    fn open_ai_settings_section(&mut self, section: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
                if let Some(section) = section {
                    let target_tab =
                        oxideterm_gpui_settings_view::settings_tab_from_ai_section(section);
                    let target_terminal_page =
                        oxideterm_gpui_settings_view::terminal_settings_page_from_ai_section(
                            section,
                        );
                    self.settings_workspace.update(cx, |settings, cx| {
                        if let Some(tab) = target_tab {
                            settings.set_active_tab(tab, cx);
                        }
                        if let Some(page) = target_terminal_page {
                            settings.set_terminal_page(page, cx);
                        }
                    });
                }

        self.open_settings_tab(window, cx);
    }

    fn ai_terminal_pane_for_session(
        &self,
        session_id: TerminalSessionId,
        cx: &App,
    ) -> Option<gpui::Entity<oxideterm_gpui_terminal::TerminalPane>> {
        // The tab host is the terminal-pane owner; reading command facts must
        // not activate a tab or create a second session.
        let location = self.tab_host.read(cx).terminal_location(session_id)?;
        self.tab_host
            .read(cx)
            .panes()
            .get(&location.pane_id)
            .cloned()
    }

    pub(in crate::workspace) fn ai_terminal_session(
        &self,
        session_id: TerminalSessionId,
        cx: &App,
    ) -> Option<(PaneId, gpui::Entity<oxideterm_gpui_terminal::TerminalPane>)> {
        let location = self.tab_host.read(cx).terminal_location(session_id)?;
        let pane_id = location.pane_id;
        let pane = self.tab_host.read(cx).panes().get(&pane_id)?.clone();
        // Background work follows the stable session without changing the user's active pane.
        Some((pane_id, pane))
    }

    pub(in crate::workspace) fn ai_connect_target_ready_result(
        &mut self,
        tool_session_id: &ToolSessionId,
        tool_call_id: &str,
        tool_name: &str,
        args: &serde_json::Value,
        duration_ms: u128,
        cx: &mut Context<Self>,
    ) -> Option<AiExecutedToolResult> {
        let AiStableResourceOperation::SavedConnection(resource_ref) =
            ai_stable_resource_operation("connect_target", args).ok()?
        else {
            return None;
        };
        let snapshot = self.ai_orchestrator_snapshot_for_tool_session(Some(tool_session_id), cx);
        snapshot.connect_target_ready_result(
            &resource_ref,
            self.connection_store.get(resource_ref.id()).map(|connection| connection.name.as_str()),
            tool_call_id, tool_name, duration_ms,
        )
    }

    pub(in crate::workspace) fn ai_connect_target_timeout_result(
        &mut self,
        tool_session_id: &ToolSessionId,
        tool_call_id: &str,
        tool_name: &str,
        args: &serde_json::Value,
        _base: &AiExecutedToolResult,
        duration_ms: u128,
        cx: &mut Context<Self>,
    ) -> AiExecutedToolResult {
        let snapshot = self.ai_orchestrator_snapshot_for_tool_session(Some(tool_session_id), cx);
        snapshot.connect_target_timeout_result(tool_call_id, tool_name, args, duration_ms)
    }
}
