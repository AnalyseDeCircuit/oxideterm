use super::*;

pub enum PublicMcpClientEffect {
    RevokeRecordings,
    StopRecordings,
    CloseTerminals,
    CloseDesktops,
    SetDesktopObservation(bool),
    ReleaseDesktopInputs,
    ClearDesktopClipboard,
}

pub enum PublicMcpClientChange {
    Enabled(bool),
    ApprovalMode(ClientApprovalMode),
    ToolGroup(ToolGroup, bool),
    Remove,
}

impl PublicMcpHost {
    // The native adapter holds admission until the returned live-session effects are applied.
    pub fn change_client(
        &mut self,
        background: &PublicMcpBackground,
        client: &ClientRef,
        change: PublicMcpClientChange,
    ) -> Result<Vec<PublicMcpClientEffect>, String> {
        let mut effects = Vec::new();
        match change {
            PublicMcpClientChange::Enabled(enabled) => {
                self.set_client_enabled(client, enabled)?;
                if !enabled {
                    self.revoke_client_policy(background, client, &mut effects);
                    self.remove_client_directory_refs(client);
                }
            }
            PublicMcpClientChange::ApprovalMode(mode) => {
                self.set_client_approval_mode(client, mode)?;
                self.revoke_client_policy(background, client, &mut effects);
            }
            PublicMcpClientChange::ToolGroup(group, enabled) => {
                self.set_client_tool_group(client, group, enabled)?;
                self.update_runtime_group(background, client, group, enabled, &mut effects);
            }
            PublicMcpClientChange::Remove => {
                self.remove_client(client)?;
                self.revoke_client_policy(background, client, &mut effects);
                self.remove_client_directory_refs(client);
            }
        }
        Ok(effects)
    }

    pub fn change_requested_access(
        &mut self,
        background: &PublicMcpBackground,
        request: &DomainRequest,
    ) -> Result<(Vec<PublicMcpClientEffect>, serde_json::Value), String> {
        let prior = self
            .state
            .clients
            .get(&request.client_ref)
            .ok_or_else(|| "The external MCP client no longer exists".to_owned())?
            .tool_groups;
        let client = &request.client_ref;
        let (requested, granting) = match &request.call {
            PublicToolCall::RequestAccess(args) => (&args.groups, true),
            PublicToolCall::RevokeAccess(args) => (&args.groups, false),
            _ => unreachable!(),
        };
        let changed = requested
            .iter()
            .copied()
            .filter(|group| prior.contains(group) != granting)
            .collect::<BTreeSet<_>>();
        let groups = if granting {
            prior.union(&changed).copied().collect()
        } else {
            prior.difference(&changed).copied().collect()
        };
        self.set_client_groups(client, groups)?;
        let mut effects = Vec::new();
        for group in &changed {
            self.update_runtime_group(background, client, *group, granting, &mut effects);
        }
        let client = self.state.clients.get(client);
        let response = if granting {
            json!({"outcome":"granted","client":client})
        } else {
            json!({"outcome":"revoked","revoked_groups":changed,"client":client})
        };
        Ok((effects, response))
    }

    pub fn suspend_client_policy(
        &self,
        background: &PublicMcpBackground,
        client: &ClientRef,
    ) -> Vec<PublicMcpClientEffect> {
        let mut effects = Vec::new();
        self.revoke_client_policy(background, client, &mut effects);
        effects
    }

    fn remove_client_directory_refs(&mut self, client: &ClientRef) {
        self.remove_client_connection_refs(client);
        self.remove_client_quick_command_refs(client);
        self.remove_client_addon_refs(client);
    }

    fn revoke_client_policy(
        &self,
        background: &PublicMcpBackground,
        client: &ClientRef,
        effects: &mut Vec<PublicMcpClientEffect>,
    ) {
        use PublicMcpClientEffect::*;
        self.state.broker.cancel_client(client);
        self.history.lock().revoke_client(client);
        effects.extend([RevokeRecordings, CloseDesktops, CloseTerminals]);
        background.revoke_public_mcp_client_forwards(client);
        background.revoke_public_mcp_client_transfers(client);
        background.revoke_public_mcp_client_file_sessions(client);
        self.revoke_client_sync_handles(client);
        self.revoke_client_runtime(client);
    }

    fn update_runtime_group(
        &self,
        background: &PublicMcpBackground,
        client: &ClientRef,
        tool_group: ToolGroup,
        enabled: bool,
        effects: &mut Vec<PublicMcpClientEffect>,
    ) {
        use PublicMcpClientEffect::*;
        if enabled {
            match tool_group {
                ToolGroup::DesktopObserve => effects.push(SetDesktopObservation(true)),
                ToolGroup::FileWrite | ToolGroup::WorkspaceEdit => {
                    background.reset_public_mcp_client_workspace_edit_cancellation(client)
                }
                _ => {}
            }
            return;
        }
        self.state
            .approvals
            .revoke_client_tool_group(client, tool_group);
        self.state
            .broker
            .cancel_client_tool_group(client, tool_group);
        match tool_group {
            ToolGroup::NodeSession => self.revoke_client_policy(background, client, effects),
            ToolGroup::TerminalSession => effects.push(CloseTerminals),
            ToolGroup::RecordingControl => effects.push(StopRecordings),
            ToolGroup::RecordingContent => self.revoke_public_mcp_client_recording_content(client),
            ToolGroup::DesktopSession => effects.push(CloseDesktops),
            ToolGroup::DesktopObserve => effects.push(SetDesktopObservation(false)),
            ToolGroup::DesktopInput => effects.push(ReleaseDesktopInputs),
            ToolGroup::DesktopClipboard => effects.push(ClearDesktopClipboard),
            ToolGroup::TerminalObserve => self.history.lock().revoke_client(client),
            ToolGroup::CommandExecute => self.revoke_client_commands(client),
            ToolGroup::QuickCommandExecute => {
                self.revoke_client_commands_for_group(client, ToolGroup::QuickCommandExecute)
            }
            ToolGroup::ArtifactTransfer => {
                background.revoke_public_mcp_client_transfers(client);
                self.state.artifacts.revoke_client(client);
            }
            ToolGroup::ForwardManage => background.revoke_public_mcp_client_forwards(client),
            ToolGroup::FileRead => background.revoke_public_mcp_client_file_sessions(client),
            ToolGroup::FileWrite => {
                background.cancel_public_mcp_client_uploads(client);
                background.cancel_public_mcp_client_workspace_edits(client);
            }
            ToolGroup::WorkspaceRead => background.revoke_public_mcp_client_workspaces(client),
            ToolGroup::WorkspaceEdit => background.cancel_public_mcp_client_workspace_edits(client),
            ToolGroup::CloudSync => self.revoke_client_sync_handles(client),
            ToolGroup::Basic
            | ToolGroup::ConnectionDirectory
            | ToolGroup::ConnectionRead
            | ToolGroup::ConnectionManage
            | ToolGroup::CredentialManage
            | ToolGroup::TerminalInput
            | ToolGroup::CommandObserve
            | ToolGroup::AuditRead
            | ToolGroup::HostToolsObserve
            | ToolGroup::HostToolsOperate
            | ToolGroup::QuickCommandRead
            | ToolGroup::QuickCommandContentRead
            | ToolGroup::QuickCommandManage
            | ToolGroup::AddonRead
            | ToolGroup::AddonManage
            | ToolGroup::ForwardRead => {}
        }
    }
}
