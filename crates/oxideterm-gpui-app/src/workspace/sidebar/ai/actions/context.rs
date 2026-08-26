impl WorkspaceApp {
    pub(in crate::workspace) fn ai_help_markdown(&self) -> String {
        ai_help_markdown_core(|key| self.i18n.t(key))
    }

    pub(in crate::workspace) fn resolve_ai_reference_context(
        &self,
        references: &[AiReferenceMatch],
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let blocks = references
            .iter()
            .filter_map(|reference| {
                self.resolve_ai_reference_content(reference, cx)
                    .and_then(|content| ai_reference_context_block(reference, &content))
            })
            .collect::<Vec<_>>();
        (!blocks.is_empty()).then(|| blocks.join("\n\n"))
    }

    pub(in crate::workspace) fn resolve_ai_selected_terminal_context(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        if !self.ai_entity.read(cx).chat_ui().include_context || !self.ai_active_terminal_context_available(cx) {
            return None;
        }
        if self.ai_entity.read(cx).chat_ui().include_all_panes && self.ai_active_tab_has_split_panes(cx) {
            return self.ai_all_panes_terminal_context(cx);
        }
        self.ai_single_pane_terminal_context(cx)
    }

    pub(in crate::workspace) fn resolve_ai_sidebar_context_block(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let mut blocks = Vec::new();
        // Terminal output is controlled by the per-message Context chip and
        // assembled separately. Keep auxiliary sources independent so enabling
        // terminal context cannot replace the IDE code snapshot.
        if let Some(ide) = self.ai_active_ide_context(cx)
            && let (Some(active_file), Some(snippet)) = (ide.active_file, ide.code_snippet)
        {
            let language = ide.active_language.unwrap_or_else(|| "text".to_string());
            blocks.push(format!(
                "=== Code: {active_file} ({language}, lines {}+) ===\n{snippet}",
                ide.snippet_start_line
            ));
        }
        (!blocks.is_empty()).then(|| blocks.join("\n\n"))
    }

    pub(in crate::workspace) fn resolve_ai_sidebar_system_prompt_segment(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let mut parts = Vec::new();
        parts.push("## Environment".to_string());
        parts.push(format!("- Local OS: {}", ai_local_os_label()));
        if let Some(tab) = self.active_tab(cx) {
            parts.push(format!("- Active tab: {}", ai_tab_kind_label(&tab.kind)));
        }
        if let Some(cwd) = self.ai_active_cwd(cx) {
            parts.push(format!("- Current working directory: {cwd}"));
        }
        match self.active_tab(cx).map(|tab| &tab.kind) {
            Some(TabKind::SshTerminal) => {
                if let Some((_, node_id)) = self.ai_active_ssh_session(cx)
                    && let Some(node) = self.ssh_nodes.get(&node_id)
                {
                    parts.push(format!(
                        "- Terminal: SSH to {}@{}:{}",
                        node.endpoint.username, node.endpoint.host, node.endpoint.port
                    ));
                }
            }
            Some(TabKind::LocalTerminal) => {
                if self.ai_active_terminal_session_id(cx).is_some() {
                    parts.push(format!("- Terminal: Local ({})", ai_local_os_label()));
                }
            }
            Some(TabKind::MoshTerminal) => {
                if self.ai_active_terminal_session_id(cx).is_some() {
                    parts.push("- Terminal: Mosh (remote OS unknown)".to_string());
                }
            }
            _ => parts.push("- Terminal: No active terminal".to_string()),
        }
        parts.push(String::new());
        parts.push("## Runtime State".to_string());
        parts.push(format!("- Open tabs: {}", self.tabs(cx).len()));
        parts.push(format!(
            "- Runtime terminal sessions: {}",
            self.tab_host.read(cx).panes().len()
        ));
        parts.push(
            "- Tabs, pane ids, and terminal session ids are memory-only and do not survive an app restart/reload."
                .to_string(),
        );

        if let Some(ide) = self.ai_active_ide_context(cx) {
            parts.push(String::new());
            parts.push("## IDE Context".to_string());
            parts.push(format!(
                "- Project: {} ({})",
                ide.project_name, ide.project_root
            ));
            if let Some(branch) = ide.git_branch {
                parts.push(format!("- Git: {branch}"));
            }
            if let Some(active_file) = ide.active_file {
                let language = ide.active_language.unwrap_or_else(|| "unknown".to_string());
                let dirty = if ide.is_dirty { " [unsaved]" } else { "" };
                parts.push(format!("- Editing: {active_file} ({language}){dirty}"));
            }
            if ide.open_tab_count > 1 {
                parts.push(format!(
                    "- Open tabs ({}): {}",
                    ide.open_tab_count,
                    ide.open_tab_paths.join(", ")
                ));
            }
        }

        if let Some((_, remote_path, selected_files)) = self.ai_active_sftp_context(cx) {
            parts.push(String::new());
            parts.push("## File Browser Context".to_string());
            parts.push(format!("- CWD: {remote_path}"));
            if !selected_files.is_empty() {
                let shown = selected_files.iter().take(20).cloned().collect::<Vec<_>>();
                let suffix = if selected_files.len() > 20 {
                    format!(" ... +{} more", selected_files.len() - 20)
                } else {
                    String::new()
                };
                parts.push(format!(
                    "- Selected ({}): [{}{}]",
                    selected_files.len(),
                    shown.join(", "),
                    suffix
                ));
            }
        }

        if let Some(context_chips) = self.resolve_ai_runtime_context_chips_prompt(cx) {
            parts.push(String::new());
            parts.push(context_chips);
        }

        (!parts.is_empty()).then(|| parts.join("\n"))
    }

    pub(in crate::workspace) fn resolve_ai_runtime_context_chips_prompt(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let mut chips = Vec::new();
        let mut command_records = self.ai_runtime_command_records(cx);
        command_records.sort_by(|left, right| {
            right
                .finished_at
                .unwrap_or(right.started_at)
                .cmp(&left.finished_at.unwrap_or(left.started_at))
        });
        for record in command_records.iter().take(5) {
            let kind = if record.status == "error" {
                "recent_error"
            } else {
                "recent_command"
            };
            let exit_suffix = record
                .exit_code
                .map(|code| format!(" (exit {code})"))
                .unwrap_or_default();
            chips.push(format!(
                "- {kind}:{} {}",
                exit_suffix,
                serde_json::json!({
                    "status": record.status,
                    "approvalMode": record.approval_mode,
                    "source": record.source,
                    "risk": record.risk,
                    "commandChars": record.command.chars().count(),
                    "hasCwd": record.cwd.is_some(),
                })
            ));
            if chips.len() >= 8 {
                break;
            }
        }
        if chips.len() < 8 {
            let mut sessions = self.ai_runtime_cli_agent_sessions(&command_records, cx);
            sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
            for session in sessions.into_iter().take(3) {
                chips.push(format!(
                    "- cli_agent: {} is {}{} {}",
                    session.kind,
                    session.status,
                    session
                        .live_terminal
                        .then(|| " in a live terminal".to_string())
                        .unwrap_or_default(),
                    serde_json::json!({
                        "kind": session.kind,
                        "status": session.status,
                        "label": session.label,
                    })
                ));
                if chips.len() >= 8 {
                    break;
                }
            }
        }
        if chips.is_empty() {
            return None;
        }
        Some(
            [
                "## Runtime Context Chips".to_string(),
                "These are non-authoritative runtime hints. Rediscover a current handle before acting on a live target.".to_string(),
            ]
            .into_iter()
            .chain(chips)
            .collect::<Vec<_>>()
            .join("\n"),
        )
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

    pub(in crate::workspace) fn ai_runtime_cli_agent_sessions(
        &self,
        records: &[AiRuntimeCommandRecord],
        _cx: &mut Context<Self>,
    ) -> Vec<AiCliAgentSession> {
        let mut sessions = Vec::new();
        let mut seen = HashSet::new();
        for record in records {
            let Some(kind) = detect_ai_cli_agent_kind(&record.command) else {
                continue;
            };
            let id = format!("cli-agent:{kind}:live-terminal");
            if !seen.insert(id.clone()) {
                continue;
            }
            let status = match record.status.as_str() {
                "waiting_for_input" => "waiting_for_input",
                "error" => "failed",
                _ => "running",
            };
            sessions.push(AiCliAgentSession {
                id,
                kind: kind.clone(),
                label: format!("{kind} agent"),
                status: status.to_string(),
                live_terminal: true,
                started_at: record.started_at,
                updated_at: record.finished_at.unwrap_or(record.started_at),
            });
        }
        sessions
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
        let mut parts = Vec::new();
        if let Some(selection) = self.ai_terminal_pane_selection(pane_id, cx) {
            parts.push("=== SELECTED TEXT (Focus Area) ===".to_string());
            parts.push(selection);
            parts.push(String::new());
        }
        if let Some(buffer) = self.ai_terminal_pane_text(pane_id, cx) {
            let (buffer, line_count) = self.ai_limited_terminal_buffer(
                &buffer,
                self.settings_store
                    .settings()
                    .ai
                    .context_visible_lines
                    .max(0) as usize,
                self.settings_store.settings().ai.context_max_chars.max(0) as usize,
            );
            if !buffer.trim().is_empty() {
                parts.push(format!("=== Terminal Output (last {line_count} lines) ==="));
                parts.push(buffer);
            }
        }
        (!parts.is_empty()).then(|| parts.join("\n"))
    }

    pub(in crate::workspace) fn ai_all_panes_terminal_context(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        // This low-frequency context action snapshots at most four pane nodes
        // so terminal reads can mutably use the GPUI context without cloning a Tab.
        let (root, active_pane_id, terminal_type) = self.active_tab(cx).and_then(|tab| {
            Some((
                tab.root_pane.as_ref()?.clone(),
                tab.active_pane_id,
                if tab.kind == TabKind::SshTerminal {
                    "SSH"
                } else {
                    "Local"
                },
            ))
        })?;
        let mut pane_ids = Vec::new();
        root.collect_pane_ids(&mut pane_ids);
        if pane_ids.len() <= 1 {
            return self.ai_single_pane_terminal_context(cx);
        }

        let mut parts = Vec::new();
        if let Some(active_pane_id) = active_pane_id
            && let Some(selection) = self.ai_terminal_pane_selection(active_pane_id, cx)
        {
            parts.push("=== SELECTED TEXT (Focus Area) ===".to_string());
            parts.push(selection);
            parts.push(String::new());
        }
        for pane_id in pane_ids {
            let Some(buffer) = self.ai_terminal_pane_text(pane_id, cx) else {
                continue;
            };
            let (buffer, line_count) = self.ai_limited_terminal_buffer(&buffer, 30, 4000);
            if buffer.trim().is_empty() {
                continue;
            }
            let label = if Some(pane_id) == active_pane_id {
                "Active Pane"
            } else {
                "Pane"
            };
            parts.push(format!(
                "=== {label} ({terminal_type}) — last {line_count} lines ==="
            ));
            parts.push(buffer);
            parts.push(String::new());
        }
        (!parts.is_empty()).then(|| parts.join("\n"))
    }

    pub(in crate::workspace) fn ai_limited_terminal_buffer(
        &self,
        buffer: &str,
        max_lines: usize,
        max_chars: usize,
    ) -> (String, usize) {
        let max_lines = max_lines.max(1);
        let mut lines = buffer.lines().rev().take(max_lines).collect::<Vec<_>>();
        lines.reverse();
        let mut text = lines.join("\n");
        if max_chars > 0 && text.len() > max_chars {
            let start = text
                .char_indices()
                .map(|(index, _)| index)
                .find(|index| text.len().saturating_sub(*index) <= max_chars)
                .unwrap_or(0);
            text = text[start..].to_string();
        }
        let line_count = text.lines().count();
        (text, line_count)
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
        cx: &mut Context<Self>,
    ) -> Option<oxideterm_gpui_ide::IdeAiContextSnapshot> {
        if !self.settings_store.settings().ai.context_sources.ide {
            return None;
        }
        let active_ide_tab = self
            .active_tab(cx)
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
        let tab_id = self.active_tab(cx)?.id;
        let node_id = self.sftp_tab_nodes.get(&tab_id)?.clone();
        let sftp = self.sftp_view.read(cx);
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
            TabKind::LocalTerminal | TabKind::SshTerminal | TabKind::MoshTerminal
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
                    TabKind::LocalTerminal | TabKind::SshTerminal | TabKind::MoshTerminal
                )
            })
            .and_then(|tab| tab.root_pane.as_ref())
            .is_some_and(|root| root.pane_count() > 1)
    }

    pub(in crate::workspace) fn ai_has_ide_context(&self, cx: &mut Context<Self>) -> bool {
        self.ai_active_ide_context(cx).is_some()
    }

    pub(in crate::workspace) fn ai_has_sftp_context(&self, cx: &App) -> bool {
        self.ai_active_sftp_context(cx).is_some()
    }

    pub(in crate::workspace) fn resolve_ai_reference_content(
        &self,
        reference: &AiReferenceMatch,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        match reference.reference_type.as_str() {
            "buffer" => self
                .active_pane_id(cx)
                .and_then(|pane_id| self.ai_terminal_pane_text(pane_id, cx)),
            "selection" => self
                .active_pane_id(cx)
                .and_then(|pane_id| self.ai_terminal_pane_selection(pane_id, cx)),
            "error" => self
                .active_pane_id(cx)
                .and_then(|pane_id| self.ai_terminal_pane_text(pane_id, cx))
                .and_then(|text| extract_ai_error_context(&text)),
            "pane" => self
                .ai_pane_reference_id(reference, cx)
                .and_then(|pane_id| self.ai_terminal_pane_text(pane_id, cx)),
            "cwd" => self
                .active_pane_id(cx)
                .and_then(|pane_id| self.ai_terminal_pane_text(pane_id, cx))
                .and_then(|text| infer_ai_cwd(&text)),
            _ => None,
        }
    }

    pub(in crate::workspace) fn ai_pane_reference_id(
        &self,
        reference: &AiReferenceMatch,
        cx: &App,
    ) -> Option<PaneId> {
        let index = reference.value.as_deref()?.parse::<usize>().ok()?;
        if index == 0 {
            return None;
        }
        let mut pane_ids = Vec::new();
        self.active_tab(cx)?
            .root_pane
            .as_ref()?
            .collect_pane_ids(&mut pane_ids);
        pane_ids.get(index - 1).copied()
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

pub(in crate::workspace) fn ai_tab_kind_label(kind: &TabKind) -> &'static str {
    match kind {
        TabKind::LocalTerminal => "local_terminal",
        TabKind::SshTerminal => "terminal",
        TabKind::MoshTerminal => "mosh_terminal",
        TabKind::Sftp => "sftp",
        TabKind::Ide => "ide",
        TabKind::Forwards => "forwards",
        TabKind::Settings => "settings",
        TabKind::FileManager => "file_manager",
        TabKind::SessionManager => "session_manager",
        TabKind::Runtime => "runtime",
        TabKind::ConnectionPool => "runtime",
        TabKind::Topology => "topology",
        TabKind::Graphics => "graphics",
        TabKind::NotificationCenter => "notifications",
        TabKind::PluginManager => "plugin_manager",
        TabKind::Plugin { .. } => "plugin",
        TabKind::CloudSync => "cloud_sync",
        TabKind::RemoteDesktop => "remote_desktop",
    }
}

pub(in crate::workspace) fn ai_ledger_source_from_terminal_source(
    source: TerminalCommandMarkDetectionSource,
) -> String {
    match source {
        TerminalCommandMarkDetectionSource::Ai => "ai.terminal_input",
        TerminalCommandMarkDetectionSource::Broadcast => "broadcast",
        TerminalCommandMarkDetectionSource::ShellIntegration => "shell_integration",
        TerminalCommandMarkDetectionSource::CommandBar => "command_bar",
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
