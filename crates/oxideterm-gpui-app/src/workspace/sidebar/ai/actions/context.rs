use oxideterm_gpui_ai::conversation::context::{
    AiEnvironmentSnapshot, AiIdeContext, AiRuntimeCommandRecord, AiTerminalPaneContext, AiTerminalReferenceSource,
    build_environment_prompt, build_ide_code_context, build_reference_context, build_single_terminal_context,
    build_split_terminal_context,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_help_markdown(&self) -> String {
        ai_help_markdown_core(|key| self.i18n.t(key))
    }

    pub(in crate::workspace) fn resolve_ai_reference_context(
        &self,
        references: &[AiReferenceMatch],
        cx: &mut Context<Self>,
    ) -> Option<String> {
        build_reference_context(references, |source| {
            let pane = match source {
                AiTerminalReferenceSource::Pane(index) => {
                    let mut panes = Vec::new();
                    self.active_tab(cx)?.root_pane.as_ref()?.collect_pane_ids(&mut panes);
                    *panes.get(index)?
                }
                AiTerminalReferenceSource::ActiveBuffer | AiTerminalReferenceSource::ActiveSelection => self.active_pane_id(cx)?,
            };
            match source {
                AiTerminalReferenceSource::ActiveSelection => self.ai_terminal_pane_selection(pane, cx),
                _ => self.ai_terminal_pane_text(pane, cx),
            }
        })
    }

    pub(in crate::workspace) fn resolve_ai_selected_terminal_context(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        if !self.ai.state.read(cx).chat_ui().include_context || !self.ai_active_terminal_context_available(cx) {
            return None;
        }
        if self.ai.state.read(cx).chat_ui().include_all_panes && self.ai_active_tab_has_split_panes(cx) {
            return self.ai_all_panes_terminal_context(cx);
        }
        self.ai_single_pane_terminal_context(cx)
    }

    pub(in crate::workspace) fn resolve_ai_sidebar_context_block(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let ide = self.ai_active_ide_context(cx)?;
        build_ide_code_context(ide.active_file.as_deref(), ide.active_language.as_deref(),
            ide.code_snippet.as_deref(), ide.snippet_start_line)
    }

    pub(in crate::workspace) fn resolve_ai_sidebar_system_prompt_segment(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let active_tab = self.active_content_tab(cx).map(|tab| ai_tab_kind_label(&tab.kind));
        let terminal_kind = self.active_pane_id(cx).and_then(|pane_id| {
            self.tab_host.read(cx).panes().get(&pane_id).map(|pane| pane.read(cx).session_kind())
        });
        let ssh_endpoint = terminal_kind.filter(|kind| *kind == oxideterm_terminal::TerminalSessionKind::SshPty)
            .and_then(|_| self.ai_active_ssh_session(cx))
            .and_then(|(_, node_id)| self.ssh_nodes.get(&node_id))
            .map(|node| (node.endpoint.username.as_str(), node.endpoint.host.as_str(), node.endpoint.port));
        let context = AiEnvironmentSnapshot {
            local_os: ai_local_os_label(),
            active_tab,
            cwd: self.ai_active_cwd(cx),
            terminal: terminal_kind.map(|kind| ai_terminal_environment(kind, ai_local_os_label())),
            ssh_endpoint,
            tab_count: self.tabs(cx).iter().filter(|tab| self.tab_host.read(cx).container_tab_id(tab.id) == tab.id).count(),
            terminal_count: self.tab_host.read(cx).panes().len(),
            ide: self.ai_active_ide_context(cx).map(|ide| AiIdeContext {
                project_root: ide.project_root, project_name: ide.project_name, git_branch: ide.git_branch,
                active_file: ide.active_file, active_language: ide.active_language, is_dirty: ide.is_dirty,
                open_tab_count: ide.open_tab_count, open_tab_paths: ide.open_tab_paths,
            }),
            sftp: self.ai_active_sftp_context(cx).map(|(_, path, files)| (path, files)),
        };
        Some(build_environment_prompt(context, self.ai_runtime_command_records(cx)))
    }

    pub(in crate::workspace) fn ai_runtime_command_records(
        &self,
        cx: &mut Context<Self>,
    ) -> Vec<AiRuntimeCommandRecord> {
        // Command history remains owned by terminal panes and is never copied
        // from secret-capable AI tool arguments.
        let mut records = Vec::new();
        let mut seen = HashSet::new();
        let tab_host = self.tab_host.read(cx);
        for (pane_id, pane) in tab_host.panes() {
            if self.session_id_for_pane(*pane_id, cx).is_none() {
                continue;
            }
            for record in pane.read(cx).ai_command_records() {
                if !seen.insert(record.command_id.clone()) {
                    continue;
                }
                let source = ai_ledger_source_from_terminal_source(record.source);
                let status = ai_ledger_status_from_terminal_status(record.status);
                records.push(AiRuntimeCommandRecord {
                    command_id: record.command_id,
                    command: record.command,
                    cwd: None,
                    source,
                    status,
                    exit_code: record.exit_code.map(i64::from),
                    started_at: record.started_at as i64,
                    finished_at: record.finished_at.map(|value| value as i64),
                    approval_mode: None,
                    risk: "execute".to_string(),
                });
            }
        }
        records
    }

    pub(in crate::workspace) fn session_id_for_pane(
        &self,
        pane_id: PaneId,
        cx: &App,
    ) -> Option<TerminalSessionId> {
        self.tabs(cx)
            .iter()
            .find_map(|tab| tab.root_pane.as_ref()?.session_id_for_pane(pane_id))
    }

    pub(in crate::workspace) fn ai_single_pane_terminal_context(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let pane_id = self.active_pane_id(cx)?;
        build_single_terminal_context(
            self.ai_terminal_pane_selection(pane_id, cx),
            self.ai_terminal_pane_text(pane_id, cx).as_deref(),
            self.ai_ambient_context_budget(),
        )
    }

    pub(in crate::workspace) fn ai_all_panes_terminal_context(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        // This low-frequency context action snapshots at most four pane nodes
        // so terminal reads can mutably use the GPUI context without cloning a Tab.
        let (root, active_pane_id) = self
            .active_tab(cx)
            .and_then(|tab| Some((tab.root_pane.as_ref()?.clone(), tab.active_pane_id)))?;
        let mut pane_ids = Vec::new();
        root.collect_pane_ids(&mut pane_ids);
        if pane_ids.len() <= 1 {
            return self.ai_single_pane_terminal_context(cx);
        }

        let selection = active_pane_id.and_then(|pane| self.ai_terminal_pane_selection(pane, cx));
        build_split_terminal_context(selection, pane_ids.into_iter().filter_map(|pane_id| {
            Some(AiTerminalPaneContext {
                pane: pane_id,
                session: root.session_id_for_pane(pane_id),
                active: Some(pane_id) == active_pane_id,
                buffer: self.ai_terminal_pane_text(pane_id, cx)?,
                environment: self.terminal_kind_for_pane(pane_id, cx)
                    .map(|kind| ai_terminal_environment(kind, ai_local_os_label())).unwrap_or_default(),
                label: self.terminal_pane_label(pane_id, cx),
            })
        }))
    }

    pub(in crate::workspace) fn ai_active_cwd(&self, cx: &mut Context<Self>) -> Option<String> {
        self.active_pane_id(cx)
            .and_then(|pane_id| self.ai_terminal_pane_text(pane_id, cx))
            .and_then(|text| infer_ai_cwd(&text))
    }

    pub(in crate::workspace) fn ai_active_terminal_session_id(
        &self,
        cx: &App,
    ) -> Option<TerminalSessionId> {
        let tab = self.active_tab(cx)?;
        let pane_id = tab.active_pane_id?;
        tab.root_pane.as_ref()?.session_id_for_pane(pane_id)
    }

    pub(in crate::workspace) fn ai_active_ssh_session(
        &self,
        cx: &App,
    ) -> Option<(TerminalSessionId, NodeId)> {
        let session_id = self.ai_active_terminal_session_id(cx)?;
        let node_id = self
            .workspace_runtime
            .read(cx)
            .ssh_terminal_node_id(session_id)?;
        Some((session_id, node_id))
    }

    pub(in crate::workspace) fn ai_active_ide_context(
        &self,
        cx: &App,
    ) -> Option<oxideterm_gpui_ide::IdeAiContextSnapshot> {
        if !self.settings_store.settings().ai.context_sources.ide {
            return None;
        }
        let active_ide_tab = self
            .active_content_tab(cx)
            .and_then(|tab| (tab.kind == TabKind::Ide).then_some(tab.id));
        self.ide_workspace
            .read(cx)
            .ai_context_snapshot(active_ide_tab, cx)
    }

    pub(in crate::workspace) fn ai_active_sftp_context(
        &self,
        cx: &App,
    ) -> Option<(NodeId, String, Vec<String>)> {
        if !self.settings_store.settings().ai.context_sources.sftp {
            return None;
        }
        let tab_id = self.active_content_tab_id(cx)?;
        let node_id = self.sftp_tab_nodes.get(&tab_id)?.clone();
        let _scope = self.enter_sftp_surface(crate::workspace::sftp::SftpSurfaceId::Tab(tab_id));
        let sftp = self.sftp_view().read(cx);
        let remote_path = sftp.current_remote_path().trim().to_string();
        if remote_path.is_empty() {
            return None;
        }
        Some((node_id, remote_path, sftp.selected_remote_files()))
    }

    pub(in crate::workspace) fn ai_active_terminal_context_available(&self, cx: &App) -> bool {
        let Some(tab) = self.active_tab(cx) else {
            return false;
        };
        matches!(
            tab.kind,
            TabKind::LocalTerminal | TabKind::SshTerminal | TabKind::MoshTerminal | TabKind::Workspace
        )
            && tab
                .active_pane_id
                .is_some_and(|pane_id| self.tab_host.read(cx).panes().contains_key(&pane_id))
    }

    pub(in crate::workspace) fn ai_active_tab_has_split_panes(&self, cx: &App) -> bool {
        self.active_tab(cx)
            .filter(|tab| {
                matches!(
                    tab.kind,
                    TabKind::LocalTerminal | TabKind::SshTerminal | TabKind::MoshTerminal | TabKind::Workspace
                )
            })
            .and_then(|tab| tab.root_pane.as_ref())
            .is_some_and(|root| root.pane_count() > 1)
    }

    pub(in crate::workspace) fn ai_has_ide_context(&self, cx: &App) -> bool {
        self.ai_active_ide_context(cx).is_some()
    }

    pub(in crate::workspace) fn ai_has_sftp_context(&self, cx: &App) -> bool {
        self.ai_active_sftp_context(cx).is_some()
    }

    pub(in crate::workspace) fn ai_terminal_pane_text(
        &self,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        self.tab_host
            .read(cx)
            .panes()
            .get(&pane_id)
            .map(|pane| pane.read(cx).visible_text_snapshot())
            .filter(|text| !text.trim().is_empty())
    }

    pub(in crate::workspace) fn ai_terminal_pane_selection(
        &self,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        self.tab_host
            .read(cx)
            .panes()
            .get(&pane_id)
            .and_then(|pane| pane.read(cx).selected_text_snapshot())
            .filter(|text| !text.trim().is_empty())
    }
}

pub(in crate::workspace) fn ai_local_os_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Linux"
    }
}

fn ai_terminal_environment(kind: oxideterm_terminal::TerminalSessionKind, local_os: &str) -> String {
    use oxideterm_terminal::TerminalSessionKind;
    // Telnet and serial panes share the local tab kind but run on another target.
    match kind {
        TerminalSessionKind::LocalPty => format!("Local ({local_os}; shell unverified)"),
        TerminalSessionKind::SshPty => "SSH (remote OS and shell unverified)".to_string(),
        TerminalSessionKind::Mosh => "Mosh (remote OS and shell unverified)".to_string(),
        TerminalSessionKind::Telnet => "Telnet (remote OS and shell unverified)".to_string(),
        TerminalSessionKind::Serial => {
            "Serial device (OS and command interface unverified)".to_string()
        }
        TerminalSessionKind::Ble => {
            "BLE device (OS and command interface unverified)".to_string()
        }
    }
}

pub(in crate::workspace) use oxideterm_ai::tool_resources::ai_tab_kind_label;

pub(in crate::workspace) fn ai_ledger_source_from_terminal_source(
    source: TerminalCommandMarkDetectionSource,
) -> String {
    match source {
        TerminalCommandMarkDetectionSource::Ai => "ai.terminal_input",
        TerminalCommandMarkDetectionSource::Broadcast => "broadcast",
        TerminalCommandMarkDetectionSource::ShellIntegration => "shell_integration",
        TerminalCommandMarkDetectionSource::CommandBar => "command_bar",
        TerminalCommandMarkDetectionSource::QuickCommand => "quick_command",
        TerminalCommandMarkDetectionSource::UserInputObserved => "user.terminal_input",
        TerminalCommandMarkDetectionSource::Heuristic => "user_promoted",
    }
    .to_string()
}

pub(in crate::workspace) fn ai_ledger_status_from_terminal_status(
    status: oxideterm_gpui_terminal::TerminalCommandFactStatus,
) -> String {
    match status {
        oxideterm_gpui_terminal::TerminalCommandFactStatus::Open => "running",
        oxideterm_gpui_terminal::TerminalCommandFactStatus::Closed => "completed",
        oxideterm_gpui_terminal::TerminalCommandFactStatus::Stale => "stale",
    }
    .to_string()
}

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_ambient_context_budget(&self) -> usize {
        let settings = &self.settings_store.settings().ai;
        let window = oxideterm_ai::model_context_window(settings.active_model.as_deref().unwrap_or_default(),
            &settings.model_context_windows, settings.active_provider_id.as_deref(), &settings.user_context_windows);
        oxideterm_ai::agent::ambient_context_budget(window.max(1) as usize)
    }
}

#[cfg(test)]
mod terminal_environment_tests {
    use super::ai_terminal_environment;
    use oxideterm_terminal::TerminalSessionKind;

    #[test]
    fn only_local_pty_inherits_the_application_host_os() {
        for (kind, expected) in [
            (TerminalSessionKind::LocalPty, "Local (macOS; shell unverified)"),
            (TerminalSessionKind::SshPty, "SSH (remote OS and shell unverified)"),
            (TerminalSessionKind::Mosh, "Mosh (remote OS and shell unverified)"),
            (TerminalSessionKind::Telnet, "Telnet (remote OS and shell unverified)"),
            (TerminalSessionKind::Serial, "Serial device (OS and command interface unverified)"),
            (TerminalSessionKind::Ble, "BLE device (OS and command interface unverified)"),
        ] {
            assert_eq!(ai_terminal_environment(kind, "macOS"), expected);
        }
    }
}
