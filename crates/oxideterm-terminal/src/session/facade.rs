pub struct TerminalSession {
    audit: Option<oxideterm_audit::AuditContext>,
    backend: Box<dyn TerminalSessionBackend>,
    // The session owns the capability so sandbox cleanup follows the backend
    // lifetime instead of any transient graphics request or UI prompt.
    kitty_file_transmission: Option<KittyFileTransmissionControl>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TelnetSessionConfig {
    pub host: String,
    pub port: u16,
}

/// One-shot URI credentials consumed by the Telnet worker and never persisted.
pub struct TelnetLoginCredentials {
    pub username: zeroize::Zeroizing<String>,
    pub password: Option<zeroize::Zeroizing<String>>,
}

impl std::fmt::Debug for TelnetLoginCredentials {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TelnetLoginCredentials")
            .field("username", &"[redacted userinfo]")
            .field(
                "password",
                &self.password.as_ref().map(|_| "[redacted secret]"),
            )
            .finish()
    }
}

pub struct MoshTerminalConfig {
    pub executable: std::path::PathBuf,
    pub plugin_sessions: Arc<oxideterm_mosh::MoshPluginSessions>,
    pub title: String,
    pub bootstrap: oxideterm_mosh::MoshBootstrapConfig,
    pub bootstrap_context: oxideterm_mosh::MoshBootstrapContext,
    pub prediction: MoshPredictionDisplay,
    /// The workspace runtime outlives panes so shutdown can finish asynchronously.
    pub task_runtime: Arc<tokio::runtime::Runtime>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MoshPredictionDisplay {
    #[default]
    Adaptive,
    Always,
    Never,
}

impl TelnetSessionConfig {
    pub fn endpoint_label(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

impl TerminalSession {
    pub fn local_default(cols: usize, rows: usize) -> Result<Self> {
        Self::local_with_graphics_options(cols, rows, GraphicsOptions::default())
    }

    pub fn local_with_graphics_options(
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
    ) -> Result<Self> {
        Self::local_with_graphics_and_encoding(
            cols,
            rows,
            graphics_options,
            TerminalEncoding::Utf8,
            1000,
        )
    }

    pub fn local_with_graphics_and_encoding(
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
        encoding: TerminalEncoding,
        scrollback_lines: usize,
    ) -> Result<Self> {
        let kitty_file_transmission = Some(graphics_options.kitty_file_transmission.clone());
        let audit = terminal_audit_context("local", "localhost");
        let operation = oxideterm_audit::AuditOperation::in_context(
            audit.as_ref(), oxideterm_audit::AuditCategory::Connection,
            "local_terminal_start", None,
        );
        let backend = LocalPtySession::spawn_with_config_graphics_encoding_and_audit(
            cols, rows, LocalPtyConfig::default(), graphics_options, encoding, scrollback_lines,
            audit.clone());
        operation.result(&backend);
        let backend = backend?;
        Ok(Self {
            audit,
            backend: Box::new(backend),
            kitty_file_transmission,
        })
    }

    pub fn local_with_config_graphics_and_encoding(
        cols: usize,
        rows: usize,
        config: LocalPtyConfig,
        graphics_options: GraphicsOptions,
        encoding: TerminalEncoding,
        scrollback_lines: usize,
    ) -> Result<Self> {
        let kitty_file_transmission = Some(graphics_options.kitty_file_transmission.clone());
        let audit = terminal_audit_context("local", "localhost");
        let operation = oxideterm_audit::AuditOperation::in_context(
            audit.as_ref(), oxideterm_audit::AuditCategory::Connection,
            "local_terminal_start", Some(&format!("shell={} cwd={}",
                config.shell.as_ref().map(|shell| shell.id.as_str()).unwrap_or("default"),
                config.cwd.as_ref().map(|cwd| cwd.display().to_string()).unwrap_or_default())),
        );
        let backend = LocalPtySession::spawn_with_config_graphics_encoding_and_audit(
            cols, rows, config, graphics_options, encoding, scrollback_lines, audit.clone());
        operation.result(&backend);
        let backend = backend?;
        Ok(Self {
            audit,
            backend: Box::new(backend),
            kitty_file_transmission,
        })
    }

    pub fn recording_playback(
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
        scrollback_lines: usize,
    ) -> Self {
        Self {
            audit: None,
            backend: Box::new(PlaybackTerminalSession::new(
                cols,
                rows,
                graphics_options,
                scrollback_lines,
            )),
            kitty_file_transmission: None,
        }
    }

    pub fn ssh(config: SshSessionConfig, cols: usize, rows: usize) -> Self {
        Self::ssh_with_graphics_options(config, cols, rows, GraphicsOptions::default())
    }

    pub fn ssh_with_graphics_options(
        config: SshSessionConfig,
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
    ) -> Self {
        Self::ssh_with_graphics_and_encoding(
            config,
            cols,
            rows,
            graphics_options,
            TerminalEncoding::Utf8,
            1000,
        )
    }

    pub fn ssh_with_graphics_and_encoding(
        config: SshSessionConfig,
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
        encoding: TerminalEncoding,
        scrollback_lines: usize,
    ) -> Self {
        let kitty_file_transmission = Some(graphics_options.kitty_file_transmission.clone());
        let audit = config.audit_context().map(|context| context.consumer());
        Self {
            audit: audit.clone(),
            backend: Box::new(SshPtySession::new_with_audit(
                config,
                audit,
                cols,
                rows,
                graphics_options,
                encoding,
                scrollback_lines,
            )),
            kitty_file_transmission,
        }
    }

    pub fn telnet_with_graphics_and_encoding(
        config: TelnetSessionConfig,
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
        encoding: TerminalEncoding,
        scrollback_lines: usize,
    ) -> Self {
        Self::telnet_with_login_and_encoding(
            config,
            None,
            None,
            cols,
            rows,
            graphics_options,
            encoding,
            scrollback_lines,
        )
    }

    pub fn telnet_with_login_and_encoding(
        config: TelnetSessionConfig,
        login: Option<TelnetLoginCredentials>,
        upstream_proxy: Option<oxideterm_network_proxy::tcp::UpstreamProxyConfig>,
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
        encoding: TerminalEncoding,
        scrollback_lines: usize,
    ) -> Self {
        let kitty_file_transmission = Some(graphics_options.kitty_file_transmission.clone());
        let mut audit = terminal_audit_context("telnet", &config.endpoint_label());
        if let (Some(context), Some(login)) = (&mut audit, &login) {
            context.remote_account = Some(login.username.clone());
        }
        Self {
            audit: audit.clone(),
            backend: Box::new(TelnetSession::new_with_login(
                config,
                login,
                upstream_proxy,
                audit,
                cols,
                rows,
                graphics_options,
                encoding,
                scrollback_lines,
            )),
            kitty_file_transmission,
        }
    }

    pub fn mosh_with_graphics(
        config: MoshTerminalConfig,
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
        scrollback_lines: usize,
    ) -> Self {
        let kitty_file_transmission = Some(graphics_options.kitty_file_transmission.clone());
        let mut audit = terminal_audit_context(
            "mosh",
            &format!(
                "{}@{}:{}",
                config.bootstrap.ssh.username, config.bootstrap.ssh.host, config.bootstrap.ssh.port
            ),
        );
        if let Some(context) = &mut audit {
            context.remote_account = Some(zeroize::Zeroizing::new(config.bootstrap.ssh.username.clone()));
        }
        Self {
            audit: audit.clone(),
            backend: Box::new(MoshTerminalSession::new(
                config,
                audit,
                cols,
                rows,
                graphics_options,
                scrollback_lines,
            )),
            kitty_file_transmission,
        }
    }

    pub fn serial_with_graphics_and_encoding(
        config: SerialSessionConfig,
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
        encoding: TerminalEncoding,
        scrollback_lines: usize,
    ) -> std::result::Result<Self, SerialError> {
        let audit = terminal_audit_context("serial", &config.port_path);
        let result = SerialSession::new(
                config,
                audit.clone(),
                cols,
                rows,
                graphics_options,
                encoding,
                scrollback_lines,
            );
        if result.is_err() {
            let operation = oxideterm_audit::AuditOperation::in_context(
                audit.as_ref(), oxideterm_audit::AuditCategory::Connection,
                "serial_connect", None,
            );
            operation.finish(oxideterm_audit::AuditOutcome::Failed,
                oxideterm_audit::AuditEvidence::Lifecycle, None, None);
        }
        Ok(Self {
            audit,
            backend: Box::new(result?),
            kitty_file_transmission: None,
        })
    }

    pub fn audit_context(&self) -> Option<oxideterm_audit::AuditContext> {
        if let Some(mut context) = self.backend.audit_context()
        {
            // The connection owns the transport identity; the terminal keeps
            // its consumer identity even when the parser worker becomes ready.
            context.consumer_id = self
                .audit
                .as_ref()
                .and_then(|context| context.consumer_id.clone());
            Some(context)
        } else {
            self.audit.clone()
        }
    }

    pub fn kind(&self) -> TerminalSessionKind {
        self.backend.kind()
    }

    pub fn kitty_file_transmission_control(&self) -> Option<KittyFileTransmissionControl> {
        self.kitty_file_transmission.clone()
    }

    pub fn title(&self) -> Option<String> {
        self.backend.title()
    }

    pub fn status(&self) -> TerminalSessionStatus {
        self.backend.status()
    }

    pub fn read_pending(&mut self) -> bool {
        self.backend.read_pending()
    }

    pub fn read_pending_with_budget(&mut self, budget: TerminalDrainBudget) -> TerminalDrainReport {
        self.backend.read_pending_with_budget(budget)
    }

    pub fn pending_output_flush_delay(&self) -> Option<Duration> {
        self.backend.pending_output_flush_delay()
    }

    pub fn activity_receiver(&self) -> TerminalActivityReceiver {
        self.backend.activity_receiver()
    }

    pub fn take_events(&mut self) -> Vec<TerminalEvent> {
        self.backend.take_events()
    }

    pub fn write_input(&mut self, bytes: &[u8]) -> Result<()> {
        self.backend.write_input(bytes)
    }

    pub fn write_protocol_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.backend.write_protocol_bytes(bytes)
    }

    pub fn write_text(&mut self, text: &str) -> Result<()> {
        self.backend.write_text(text)
    }

    pub fn paste_text(&mut self, text: &str) -> Result<()> {
        self.backend.paste_text(text)
    }

    pub fn set_encoding(&mut self, encoding: TerminalEncoding) {
        self.backend.set_encoding(encoding);
    }

    pub fn set_palette(&mut self, palette: TerminalPalette) {
        self.backend.set_palette(palette);
    }

    pub fn mosh_connection_status(&self) -> Option<MoshConnectionStatus> {
        self.backend.mosh_connection_status()
    }

    pub fn set_output_processor(&mut self, processor: Option<TerminalOutputProcessor>) {
        self.backend.set_output_processor(processor);
    }

    pub fn set_output_events_enabled(&mut self, enabled: bool) {
        self.backend.set_output_events_enabled(enabled);
    }

    pub fn set_trigger_rules(
        &mut self,
        rules: Option<Arc<oxideterm_terminal_triggers::CompiledTriggerSet>>,
    ) {
        self.backend.set_trigger_rules(rules);
    }

    pub fn serial_runtime_options(&self) -> Option<SerialRuntimeOptions> {
        self.backend.serial_runtime_options()
    }

    pub fn set_serial_runtime_options(&mut self, options: SerialRuntimeOptions) -> Result<()> {
        let audit = oxideterm_audit::AuditOperation::in_request(
            self.audit.as_ref(),
            oxideterm_audit::AuditCategory::Connection,
            "serial_control",
            Some(&format!("{options:?}")),
        );
        let result = self.backend.set_serial_runtime_options(options);
        audit.result(&result);
        result
    }

    pub fn serial_control_state(&self) -> Option<SerialControlState> {
        self.backend.serial_control_state()
    }

    pub fn set_serial_control_line(
        &mut self,
        line: SerialControlLine,
        asserted: bool,
    ) -> Result<()> {
        let audit = oxideterm_audit::AuditOperation::in_request(
            self.audit.as_ref(),
            oxideterm_audit::AuditCategory::Connection,
            "serial_control",
            Some(&format!("{line:?}={asserted}")),
        );
        let result = self.backend.set_serial_control_line(line, asserted);
        audit.finish(
            if result.is_ok() { oxideterm_audit::AuditOutcome::Sent } else { oxideterm_audit::AuditOutcome::Failed },
            oxideterm_audit::AuditEvidence::Dispatch, None, None,
        );
        result
    }

    pub fn send_serial_break(&mut self) -> Result<()> {
        let audit = oxideterm_audit::AuditOperation::in_request(
            self.audit.as_ref(),
            oxideterm_audit::AuditCategory::Connection,
            "serial_control",
            Some("break"),
        );
        let result = self.backend.send_serial_break();
        audit.finish(
            if result.is_ok() { oxideterm_audit::AuditOutcome::Sent } else { oxideterm_audit::AuditOutcome::Failed },
            oxideterm_audit::AuditEvidence::Dispatch, None, None,
        );
        result
    }

    pub fn send_telnet_control(&mut self, command: TelnetControlCommand) -> Result<()> {
        let audit = oxideterm_audit::AuditOperation::in_request(
            self.audit.as_ref(), oxideterm_audit::AuditCategory::Connection,
            "telnet_control", Some(&format!("{command:?}")),
        );
        let result = self.backend.send_telnet_control(command);
        audit.finish(
            if result.is_ok() { oxideterm_audit::AuditOutcome::Sent } else { oxideterm_audit::AuditOutcome::Failed },
            oxideterm_audit::AuditEvidence::Dispatch, None, None,
        );
        result
    }

    pub fn send_serial_text(&mut self, text: &str) -> Result<()> {
        self.audit_serial_send(text.len(), |backend| backend.write_text(text))
    }

    pub fn send_serial_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.audit_serial_send(bytes.len(), |backend| backend.write_protocol_bytes(bytes))
    }

    fn audit_serial_send(
        &mut self,
        byte_count: usize,
        send: impl FnOnce(&mut dyn TerminalSessionBackend) -> Result<()>,
    ) -> Result<()> {
        if self.kind() != TerminalSessionKind::Serial {
            anyhow::bail!("Serial send requires a serial session");
        }
        let mode = self.serial_runtime_options().map(|options| options.send_mode);
        let operation = oxideterm_audit::AuditOperation::in_request(
            self.audit.as_ref(), oxideterm_audit::AuditCategory::Connection,
            "serial_send", Some(&format!("mode={mode:?}; input_bytes={byte_count}")),
        );
        let result = if self.backend.lifecycle().is_running() {
            send(self.backend.as_mut())
        } else {
            Err(anyhow::anyhow!("Serial session is not running"))
        };
        operation.finish(
            if result.is_ok() { oxideterm_audit::AuditOutcome::Sent } else { oxideterm_audit::AuditOutcome::Failed },
            oxideterm_audit::AuditEvidence::Dispatch, None, None,
        );
        result
    }

    pub fn set_trzsz_policy(&mut self, policy: Option<TrzszTransferPolicy>) {
        self.backend.set_trzsz_policy(policy);
    }

    pub fn take_trzsz_transfer(&mut self) -> Option<TrzszTransfer> {
        self.backend.take_trzsz_transfer()
    }

    pub fn feed_trzsz_terminal_output(&mut self, bytes: &[u8]) {
        self.backend.feed_trzsz_terminal_output(bytes);
    }

    pub fn feed_recording_output(&mut self, bytes: &[u8]) {
        self.backend.feed_recording_output(bytes);
    }

    pub fn reset_recording_playback(&mut self, cols: usize, rows: usize) {
        self.backend.reset_recording_playback(cols, rows);
    }

    pub fn interrupt_trzsz_transfer(&mut self) {
        self.backend.interrupt_trzsz_transfer();
    }

    pub fn finish_trzsz_transfer(&mut self) {
        self.backend.finish_trzsz_transfer();
    }

    pub fn begin_modem_transfer(
        &mut self,
        request: TerminalModemTransferRequest,
    ) -> Result<Option<ModemTransfer>> {
        self.backend.begin_modem_transfer(request)
    }

    pub fn interrupt_modem_transfer(&mut self) {
        self.backend.interrupt_modem_transfer();
    }

    pub fn finish_modem_transfer(&mut self) {
        self.backend.finish_modem_transfer();
    }

    pub fn lifecycle(&self) -> TerminalLifecycle {
        self.backend.lifecycle()
    }

    pub fn is_interactive(&self) -> bool {
        self.backend.is_interactive()
    }

    pub fn process_info(&self) -> TerminalProcessInfo {
        self.backend.process_info()
    }

    pub fn process_info_probe(&self) -> Option<TerminalProcessProbe> {
        self.backend.process_info_probe()
    }

    pub fn cwd_integration_launch_state(&self) -> TerminalCwdIntegrationLaunchState {
        self.backend.cwd_integration_launch_state()
    }

    pub fn apply_process_info(&mut self, info: TerminalProcessInfo) -> bool {
        self.backend.apply_process_info(info)
    }

    pub fn refresh_process_info(&mut self) {
        self.backend.refresh_process_info();
    }

    pub fn buffer_line_count(&self) -> usize {
        self.backend.buffer_line_count()
    }

    pub fn terminate_active_task(&mut self) -> Result<()> {
        self.backend.terminate_active_task()
    }

    pub fn kill_active_task(&mut self) -> Result<()> {
        self.backend.kill_active_task()
    }

    pub fn mode(&self) -> TermMode {
        self.backend.mode()
    }

    pub fn begin_tmux_pane_selection(&mut self, col: usize, row: usize) -> Result<Option<bool>> {
        self.backend.begin_tmux_pane_selection(col, row)
    }

    pub fn tmux_local_point(&self, col: usize, row: usize) -> (usize, usize) {
        self.backend.tmux_local_point(col, row)
    }

    pub fn tmux_state(&self) -> Option<crate::TmuxUiState> {
        self.backend.tmux_state()
    }

    pub fn tmux_action(&mut self, action: crate::TmuxAction) -> Result<bool> {
        let context = self.audit_context();
        let audit = oxideterm_audit::AuditOperation::in_request(
            context.as_ref(), oxideterm_audit::AuditCategory::Automation,
            "tmux_control", Some(&tmux_action_audit_detail(&action)),
        );
        self.backend.tmux_action(action, audit)
    }

    pub fn tmux_separator_at(&self, col: usize, row: usize) -> Option<crate::TmuxSeparator> {
        self.backend.tmux_separator_at(col, row)
    }

    pub fn resize_tmux_separator(
        &mut self,
        separator: crate::TmuxSeparator,
        delta: i32,
    ) -> Result<bool> {
        self.backend.resize_tmux_separator(separator, delta)
    }

    pub fn set_focused(&mut self, focused: bool) -> Result<()> {
        self.backend.set_focused(focused)
    }

    pub fn resize_with_cell_size(
        &mut self,
        cols: usize,
        rows: usize,
        cell_width: u16,
        cell_height: u16,
    ) -> Result<()> {
        self.backend
            .resize_with_cell_size(TerminalResize::new(cols, rows, cell_width, cell_height))
    }

    pub fn scroll_lines(&mut self, delta: i32) {
        self.backend.scroll_lines(delta);
    }

    pub fn scroll_lines_snapshot_incremental(
        &mut self,
        delta: i32,
        previous: &TerminalSnapshot,
    ) -> TerminalSnapshot {
        self.backend
            .scroll_lines_snapshot_incremental(delta, previous)
    }

    pub fn page_up(&mut self) {
        self.backend.page_up();
    }

    pub fn page_down(&mut self) {
        self.backend.page_down();
    }

    pub fn scroll_to_top(&mut self) {
        self.backend.scroll_to_top();
    }

    pub fn scroll_to_bottom(&mut self) {
        self.backend.scroll_to_bottom();
    }

    pub fn scroll_to_display_offset(&mut self, offset: usize) {
        self.backend.scroll_to_display_offset(offset);
    }

    pub fn search_matches(&self, query: &str) -> Vec<TerminalSearchMatch> {
        self.backend.search_matches(query)
    }

    pub fn search_source(&self) -> Option<crate::TerminalSearchSource> {
        self.backend.search_source()
    }

    pub fn set_selection(&self, selection: Option<crate::TerminalSelectionRange>) {
        self.backend.set_selection(selection);
    }

    pub fn selection(&self) -> Option<crate::TerminalSelectionRange> {
        self.backend.selection()
    }

    pub fn clear_buffer(&mut self) {
        self.backend.clear_buffer();
    }

    pub fn buffer_text(&self) -> String {
        self.backend.buffer_text()
    }

    pub fn command_output_text(&self, mark: &TerminalCommandMark) -> String {
        self.backend.command_output_text(mark)
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        self.backend.snapshot()
    }

    pub fn snapshot_incremental(&self, previous: &TerminalSnapshot) -> TerminalSnapshot {
        self.backend.snapshot_incremental(previous)
    }

    pub fn try_render_snapshot(
        &self,
        previous: &TerminalSnapshot,
        allow_defer: bool,
    ) -> Option<(TerminalSnapshot, Option<crate::TerminalSelectionRange>, TermMode)> {
        self.backend.try_render_snapshot(previous, allow_defer)
    }

    pub fn snapshot_with_display_offset(
        &self,
        display_offset: usize,
        rows: usize,
    ) -> TerminalSnapshot {
        self.backend.snapshot_with_display_offset(display_offset, rows)
    }

    pub fn shutdown(&mut self) {
        self.backend.shutdown();
    }

    pub fn ssh_connection_handle(&self) -> Option<SshConnectionHandle> {
        self.backend.ssh_connection_handle()
    }
}

fn terminal_audit_context(protocol: &str, target: &str) -> Option<oxideterm_audit::AuditContext> {
    oxideterm_audit::AuditContext::current_request()
        .or_else(oxideterm_audit::AuditContext::current)
        .map(|context| context.session(protocol, target).consumer())
}

fn tmux_action_audit_detail(action: &crate::TmuxAction) -> String {
    use crate::TmuxAction;
    match action {
        TmuxAction::SelectSession(id) => format!("select_session id={id}"),
        TmuxAction::SelectWindow(id) => format!("select_window id={id}"),
        TmuxAction::PreviousWindow => "previous_window".into(),
        TmuxAction::NextWindow => "next_window".into(),
        TmuxAction::NewSession => "new_session".into(),
        TmuxAction::CloseSession => "close_session".into(),
        TmuxAction::NewWindow => "new_window".into(),
        TmuxAction::CloseWindow => "close_window".into(),
        TmuxAction::SplitHorizontal => "split_horizontal".into(),
        TmuxAction::SplitVertical => "split_vertical".into(),
        TmuxAction::ClosePane => "close_pane".into(),
        TmuxAction::ResizePaneLeft => "resize_pane_left".into(),
        TmuxAction::ResizePaneRight => "resize_pane_right".into(),
        TmuxAction::ResizePaneUp => "resize_pane_up".into(),
        TmuxAction::ResizePaneDown => "resize_pane_down".into(),
        TmuxAction::Detach => "detach".into(),
        TmuxAction::CancelPaneMode => "cancel_pane_mode".into(),
        TmuxAction::RenameSession { id, .. } => format!("rename_session id={id}"),
        TmuxAction::RenameWindow { id, .. } => format!("rename_window id={id}"),
        TmuxAction::RunCommand(_) => "run_command".into(),
    }
}

#[cfg(test)]
mod tmux_audit_tests {
    use super::*;

    #[test]
    fn tmux_control_detail_excludes_arbitrary_command_and_rename_text() {
        assert_eq!(tmux_action_audit_detail(&crate::TmuxAction::RunCommand(
            "display-message private-token".into())), "run_command");
        assert_eq!(tmux_action_audit_detail(&crate::TmuxAction::RenameWindow {
            id: 7, name: "private-token".into(),
        }), "rename_window id=7");
    }
}
