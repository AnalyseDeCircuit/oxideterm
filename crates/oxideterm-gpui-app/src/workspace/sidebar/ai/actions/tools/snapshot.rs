use oxideterm_gpui_ai::conversation::terminal_tools::{
    AiTerminalRequest, AiTerminalTool, AiTerminalToolReply, AiTerminalWatchBindings,
    capture_terminal_target, capture_terminal_watch,
};
use oxideterm_gpui_ai::conversation::terminal_watch::{
    AiTerminalWatchSample,
    AiTerminalWatchStatus,
};
use oxideterm_ai::tool_resources::{
    AiActionResultLite, AiActiveNodeCapture, AiAppSurface, AiApplicationTool, AiRuntimeHealthCapture, AiSshNodeTargetInput,
    AiTargetCapture, AiStableResourceOperation, ai_live_resource_operation, ai_stable_resource_operation, finish_ai_target_discovery,
    finish_ai_workspace_snapshot, ai_read_stable_resource, dispatch_ai_application_tool,
    capture_live_transfers, ai_authorized_execution_route, AiAuthorizedExecutionRoute,
    ai_preference_saved, ai_remember_preference_payload, save_ai_user_preference, plan_ai_settings_write,
};
use oxideterm_workspace::ActiveSessionReadiness;

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
        targets.extend(self.connection_store.connections().iter().map(AiTargetCapture::saved_connection));
        let tabs = self.tab_host.read(cx);
        targets.extend(self.tabs(cx).iter().map(|tab| AiTargetCapture::surface(tab, tabs.surface_is_visible(tab.id))));

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
                targets.push(capture_terminal_target(
                    pane.read(cx), session_id, tab.id, pane_id, self.terminal_pane_label(pane_id, cx),
                    self.serial_terminal_configs.get(&session_id),
                ));
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
        let recent_events = oxideterm_ai::tool_resources::recent_runtime_health(
            self.notification_center.event_log.entries.iter().map(|entry| (
                entry.timestamp, match entry.severity {
                    WorkspaceEventSeverity::Warn => Some(false),
                    WorkspaceEventSeverity::Error => Some(true),
                    _ => None,
                },
            )),
        );
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
            capture_live_transfers(&self.sftp_transfer_manager),
            AiRuntimeHealthCapture {
                open_tabs: self.tabs(cx).len(),
                has_active_tab: self.active_tab_id(cx).is_some(),
                pane_count: self.tab_host.read(cx).panes().len(),
                local_session_count: self.visible_local_terminal_session_count(cx)
                    + self.detached_local_terminals.len(),
                node_states: self.ssh_nodes.values().map(ai_node_readiness),
                recent_events,
            },
        )
    }

    pub(in crate::workspace) fn ai_model_backend_services(&self, cx: &App) -> AiModelBackendServices {
        self.ai.state.read(cx).model_backend_services(&self.settings_store.settings().ai)
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
        oxideterm_gpui_ai::conversation::prepare_acp_chat_launch(
            config, &self.settings_store.settings().ai,
            |agent| self.resolve_ai_acp_plugin(agent, cx),
        )
    }

    pub(in crate::workspace) fn resolve_ai_acp_permission(
        &mut self,
        generation: u64,
        tool_call_id: String,
        option_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.ai.conversations.resolve_acp_permission(generation, &tool_call_id, option_id, cx);
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
            ObserveTerminal => return self.execute_ai_terminal_tool(AiTerminalTool::Observe, tool_session_id, args, cx),
            SendTerminalInput => return self.execute_ai_terminal_tool(AiTerminalTool::Input, tool_session_id, args, cx),
            GetTerminalCommandStatus => return self.execute_ai_terminal_tool(AiTerminalTool::Status, tool_session_id, args, cx),
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
            GetTransportSessionState | ManageSerialSession | ManageTelnetSession =>
                self.execute_ai_terminal_transport_tool(&request, tool_session_id, args, cx),
            ListRemoteDesktopSessions => Ok(self.execute_ai_list_remote_desktop_sessions(cx)),
            ManageRemoteDesktopSession => self.execute_ai_manage_remote_desktop_session(args, window, cx),
            GetCloudSyncState => Ok(self.execute_ai_get_cloud_sync_state(cx)),
            ConfigureCloudSync => self.execute_ai_configure_cloud_sync(args, cx),
            ManageCloudSync => self.execute_ai_manage_cloud_sync(args, window, cx),
            ListCredentials => self.execute_ai_list_credentials(args),
            ManageCredential => self.execute_ai_manage_credential(args, window, cx),
            ListMemoryEntries => Ok(self.execute_ai_list_memory_entries(args)),
            ManageMemoryEntry => self.execute_ai_manage_memory_entry(args, cx),
            CreateBackgroundTask => self.ai.background.read(cx).execute_ai_create_background_task(conversation_id, args),
            ListBackgroundTasks => Ok(self.ai.background.read(cx).execute_ai_list_background_tasks(conversation_id)),
            GetBackgroundTask => self.ai.background.read(cx).execute_ai_get_background_task(conversation_id, args),
            CancelBackgroundTask => self.ai.background.read(cx).execute_ai_cancel_background_task(conversation_id, args),
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
        if let Err(error) = self.validate_ai_runtime_authority(&tool_session_id, &tool_name, &args, cx) {
            let snapshot =
                self.ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx);
            let _ = sender.send(snapshot.tool_owner_rejected(
                tool_call_id, tool_name, &error, post_user_approval, 0, false,
            ));
            return;
        }
        match ai_authorized_execution_route(&tool_name, &args) {
            AiAuthorizedExecutionRoute::Connect => self.start_ai_connect_target_execution(
                &conversation_id, tool_session_id, tool_call_id, tool_name,
                args, sender, window, cx,
            ),
            AiAuthorizedExecutionRoute::LiveFile => self.start_ai_live_resource_execution(
                tool_session_id, tool_call_id, tool_name, args, post_user_approval, sender, cx,
            ),
            kind @ (AiAuthorizedExecutionRoute::RunCommand | AiAuthorizedExecutionRoute::WaitOutput) => {
                self.start_ai_terminal_execution(
                    matches!(kind, AiAuthorizedExecutionRoute::RunCommand),
                    owner, tool_session_id, tool_call_id, tool_name, args,
                    post_user_approval, dangerous_command_approved, sender, cx,
                );
            }
            AiAuthorizedExecutionRoute::Application => {
                let result = self.execute_ai_ui_orchestrator_tool(
                    &conversation_id, &tool_session_id, tool_call_id, tool_name, args, window, cx,
                );
                let _ = sender.send(result);
            }
        }

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
        let owners = (|| {
            let operation = ai_live_resource_operation(&tool_name, &args)?;
            let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
            let authority = self.ai.authority.read(cx);
            if operation.requires_ide_owner() {
                let (tab_id, node_id) = authority.validate_ide_handle(&tool_session_id, raw_handle_id, operation.capability())?;
                let fs = self.ide_workspace.read(cx).ai_owner_file_system(tab_id, cx)
                    .ok_or_else(|| oxideterm_ai::RuntimeValidationError::new(
                        oxideterm_ai::RuntimeValidationFailure::OwnerClosed,
                    ))?;
                Ok((operation, AiLiveFileOwner::Ide { node_id, file_system: fs }))
            } else {
                let owner = authority.validate_sftp_handle(&tool_session_id, raw_handle_id, operation.capability())?;
                Ok((operation, AiLiveFileOwner::Sftp(owner)))
            }
        })();
        let (operation, owner) = match owners {
            Ok(owners) => owners,
            Err(error) => {
                let snapshot = self.ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx);
                let _ = sender.send(snapshot.tool_owner_rejected(
                    tool_call_id, tool_name, &error, post_user_approval, started.elapsed().as_millis(), true,
                ));
                return;
            }
        };
        let snapshot = self.ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx);
        AiLiveToolServices {
            node_router: self.node_router.clone(),
            sftp_transfer_manager: self.sftp_transfer_manager.clone(),
            backend_runtime: self.forwarding_runtime.clone(),
        }.start(owner, operation, snapshot, args, post_user_approval, tool_call_id, tool_name, started, sender);
    }

    fn validate_ai_runtime_authority(
        &self,
        tool_session_id: &ToolSessionId,
        tool_name: &str,
        args: &serde_json::Value,
        cx: &App,
    ) -> Result<(), oxideterm_ai::RuntimeValidationError> {
        self.ai.authority.read(cx).validate_tool_authority(tool_session_id, tool_name, args,
            |id| self.connection_store.get(id).is_some(),
            |id| self.connection_store.get(id)
                .and_then(|connection| self.indexed_saved_ssh_node_for_connection(id, connection)).is_some(),
        )
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
        let initial = oxideterm_gpui_ai::conversation::terminal_watch::connection_needs_wait(&base).then(|| {
            self.ai_connect_target_ready_result(&tool_session_id, &tool_call_id, &tool_name, &args, started.elapsed().as_millis(), cx)
        }).flatten();
        let weak = cx.weak_entity();
        oxideterm_gpui_ai::conversation::terminal_watch::wait_for_connected_target(
            base, initial, sender, started, cx, move |base, elapsed, cx| {
                weak.update(cx, |app, cx| {
                    if base.is_some() {
                        let snapshot = app.ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx);
                        Some(snapshot.connect_target_timeout_result(&tool_call_id, &tool_name, &args, elapsed))
                    } else {
                        app.ai_connect_target_ready_result(&tool_session_id, &tool_call_id, &tool_name, &args, elapsed, cx)
                    }
                }).ok()
            },
        );
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

    fn execute_ai_terminal_tool(
        &self, kind: AiTerminalTool, turn: &ToolSessionId, args: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let session = self.ai.authority.read(cx).validate_terminal_handle(
            turn, args.get("handle_id").and_then(serde_json::Value::as_str), kind.capability(),
        );
        match session {
            Ok(session) => kind.execute(
                &snapshot, session, self.ai_terminal_pane_for_session(session, cx).as_ref(), args, cx,
            ),
            Err(error) => kind.denied(&snapshot, &error),
        }
    }

    fn start_ai_terminal_execution(
        &mut self, run: bool, owner: AiToolRunContext, turn: ToolSessionId,
        call: String, name: String, args: serde_json::Value, approved: bool,
        dangerous_approved: bool, sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        cx: &mut Context<Self>,
    ) {
        let reply = AiTerminalToolReply::new(
            self.ai_orchestrator_snapshot_for_tool_session(Some(&turn), cx), call, name, sender, approved,
        );
        let mut handle = args.get("handle_id").and_then(serde_json::Value::as_str)
            .and_then(|id| oxideterm_ai::RuntimeHandleId::parse(id.to_owned()).ok());
        let raw = handle.as_ref().map(oxideterm_ai::RuntimeHandleId::as_str);
        let authority = self.ai.authority.read(cx);
        let capability = if run { oxideterm_ai::RuntimeCapability::TerminalRunCommand }
            else { oxideterm_ai::RuntimeCapability::TerminalObserve };
        use oxideterm_ai::AiRunCommandOwner::{LocalShell, Terminal};
        let bound = if run {
            authority.validate_run_command_handle(&turn, raw)
        } else {
            authority.validate_terminal_handle(&turn, raw, capability).map(Terminal)
        };
        let bound = match bound {
            Ok(bound) => bound,
            Err(error) => return reply.deny(&error, run),
        };
        let request = if run {
            match reply.snapshot.plan_terminal_command(&args) {
                Ok(command) => AiTerminalRequest::Run(command),
                Err(error) => return reply.complete(error),
            }
        } else {
            AiTerminalRequest::Wait(args)
        };
        let leases = if run {
            self.ai.state.read(cx).agents.tool_leases.get(&(turn.clone(), reply.call.clone()))
                .cloned().unwrap_or_default()
        } else { Vec::new() };
        let session = match bound {
            Terminal(id) => id,
            LocalShell => {
                let AiTerminalRequest::Run(command) = request else { unreachable!() };
                for lease in &leases { lease.dispatched(); }
                return self.start_ai_owned_local_command(
                    owner, reply.call, reply.name, command.text, command.cwd.map(str::to_owned),
                    dangerous_approved, command.timeout, reply.snapshot, leases, reply.sender, cx,
                );
            }
        };
        let Some(pane) = self.ai_terminal_pane_for_session(session, cx) else {
            return reply.owner_closed(session, run);
        };
        let node = self.ai_terminal_node(session, cx);
        let weak = cx.weak_entity();
        let sample_host = weak.clone();
        let sample_turn = turn.clone();
        let sample_node = node.clone();
        let sample = move |recovered, command: Option<&str>, cx: &mut gpui::AsyncApp| {
            sample_host.update(cx, |app, cx| app.sample_ai_watched_terminal(
                &sample_turn, session, sample_node.as_ref(), &mut handle,
                capability, recovered, command, cx,
            )).unwrap_or(AiTerminalWatchSample::HostClosed)
        };
        let finish_host = weak.clone();
        let finish = move |cx: &mut gpui::AsyncApp| {
            finish_host.update(cx, |app, cx| app.ai_orchestrator_snapshot_for_tool_session(Some(&turn), cx)).ok()
        };
        let status_owner = owner.clone();
        let status_call = reply.call.clone();
        let status_name = reply.name.clone();
        let status = move |command: Option<&str>, status, deadline, cx: &mut gpui::AsyncApp| {
            let _ = weak.update(cx, |app, cx| app.report_ai_terminal_watch_status(
                &status_owner, &status_call, &status_name, command, status, deadline, run, cx,
            ));
        };
        // The current pane is resolved only after final authority validation.
        let (activity, pane_observer, runtime_observer) = self.watch_ai_terminal_activity(&pane, cx);
        let bindings = AiTerminalWatchBindings { activity, pane_observer, runtime_observer, sample, finish, status };
        let state = self.ai.state.clone();
        let command_label = self.ai_tool_display_name("run_command");
        let observation_label = self.i18n.t("settings_view.ai.waiting_condition");
        request.start(
            reply, &pane, session, leases, owner, &state, &command_label, &observation_label,
            bindings, |id, before, leases, record, cx| {
                self.monitor_ai_terminal_command(session, node, id, before, leases, record, cx)
            }, cx,
        );
    }

    fn ai_terminal_node(&self, session: TerminalSessionId, cx: &App) -> Option<NodeId> {
        self.workspace_runtime.read(cx).ssh_terminal_nodes().into_iter()
            .find_map(|(id, node)| (id == session).then_some(node))
    }

    fn watch_ai_terminal_activity(
        &self,
        pane: &Entity<TerminalPane>,
        cx: &mut Context<Self>,
    ) -> (tokio::sync::watch::Receiver<u64>, Subscription, Subscription) {
        let (activity, activity_rx) = tokio::sync::watch::channel(0u64);
        let runtime_activity = activity.clone();
        let runtime_subscription = cx.observe(&self.workspace_runtime, move |_, _, _| {
            runtime_activity.send_modify(|revision| *revision = revision.wrapping_add(1));
        });
        let subscription = cx.subscribe(pane, move |_, _, event, _| {
            if matches!(
                event,
                TerminalPaneEvent::OutputActivity
                    | TerminalPaneEvent::Exited { .. }
                    | TerminalPaneEvent::PrivilegePromptStateChanged
            ) {
                activity.send_modify(|revision| *revision = revision.wrapping_add(1));
            }
        });
        (activity_rx, subscription, runtime_subscription)
    }

    fn sample_ai_watched_terminal(
        &self,
        tool_session_id: &ToolSessionId,
        session_id: TerminalSessionId,
        node_id: Option<&NodeId>,
        handle_id: &mut Option<oxideterm_ai::RuntimeHandleId>,
        capability: oxideterm_ai::RuntimeCapability,
        recovered: bool,
        command: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AiTerminalWatchSample {
        if let Some(node_id) = node_id {
            let connection = self.node_router.connection_id_for_node(node_id)
                .and_then(|id| self.ssh_registry.get(&id));
            let Some(connection) = connection else { return AiTerminalWatchSample::OwnerClosed };
            match connection.state() {
                ConnectionState::Connecting | ConnectionState::LinkDown
                | ConnectionState::Reconnecting | ConnectionState::Error(_) => return AiTerminalWatchSample::Recovering,
                ConnectionState::Disconnected | ConnectionState::Disconnecting => return AiTerminalWatchSample::OwnerClosed,
                _ => {}
            }
        }
        // Sample only the current owner; a retained pane is not authority.
        if recovered {
            *handle_id = self.ai.authority.update(cx, |authority, _| {
                authority.issue_terminal_handle(tool_session_id, session_id).ok()
            }).map(|projection| projection.handle_id);
        }
        if self.ai.authority.read(cx).validate_terminal_handle(
            tool_session_id,
            handle_id.as_ref().map(oxideterm_ai::RuntimeHandleId::as_str),
            capability,
        ).ok() != Some(session_id) {
            return AiTerminalWatchSample::OwnerClosed;
        }
        let Some(pane) = self.ai_terminal_pane_for_session(session_id, cx) else { return AiTerminalWatchSample::OwnerClosed };
        capture_terminal_watch(
            pane.read(cx), capability == oxideterm_ai::RuntimeCapability::TerminalRunCommand, command,
        )
    }

    fn report_ai_terminal_watch_status(
        &mut self,
        owner: &AiToolRunContext,
        call_id: &str,
        tool_name: &str,
        command_id: Option<&str>,
        status: AiTerminalWatchStatus,
        deadline_ms: i64,
        run: bool,
        cx: &mut Context<Self>,
    ) {
        let (data, risk) = self.ai.state.read(cx).terminal_watch_status(
            owner.generation, command_id, status, deadline_ms, run,
        );
        self.report_ai_tool_progress(owner, call_id, tool_name, status.protocol_status(), Some(data), risk, cx);
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
            self.settings_workspace.update(cx, |settings, cx| settings.open_ai_section(section, cx));
        }
        self.open_settings_tab(window, cx);
    }

    fn ai_terminal_pane_for_session(
        &self,
        session_id: TerminalSessionId,
        cx: &App,
    ) -> Option<gpui::Entity<oxideterm_gpui_terminal::TerminalPane>> {
        self.ai_terminal_session(session_id, cx).map(|(_, pane)| pane)
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

}
