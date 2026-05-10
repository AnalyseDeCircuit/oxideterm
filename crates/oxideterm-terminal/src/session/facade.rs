pub struct TerminalSession {
    backend: Box<dyn TerminalSessionBackend>,
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
        Ok(Self {
            backend: Box::new(LocalPtySession::spawn_with_graphics_and_encoding(
                cols,
                rows,
                graphics_options,
                encoding,
                scrollback_lines,
            )?),
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
        Ok(Self {
            backend: Box::new(LocalPtySession::spawn_with_config_graphics_and_encoding(
                cols,
                rows,
                config,
                graphics_options,
                encoding,
                scrollback_lines,
            )?),
        })
    }

    pub fn recording_playback(
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
        scrollback_lines: usize,
    ) -> Self {
        Self {
            backend: Box::new(PlaybackTerminalSession::new(
                cols,
                rows,
                graphics_options,
                scrollback_lines,
            )),
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
        Self {
            backend: Box::new(SshPtySession::new(
                config,
                cols,
                rows,
                graphics_options,
                encoding,
                scrollback_lines,
            )),
        }
    }

    pub fn kind(&self) -> TerminalSessionKind {
        self.backend.kind()
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

    pub fn lifecycle(&self) -> TerminalLifecycle {
        self.backend.lifecycle()
    }

    pub fn is_interactive(&self) -> bool {
        self.backend.is_interactive()
    }

    pub fn process_info(&self) -> TerminalProcessInfo {
        self.backend.process_info()
    }

    pub fn refresh_process_info(&mut self) {
        self.backend.refresh_process_info();
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

    pub fn command_output_text(&self, mark: &TerminalCommandMark) -> String {
        self.backend.command_output_text(mark)
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        self.backend.snapshot()
    }

    pub fn shutdown(&mut self) {
        self.backend.shutdown();
    }

    pub fn ssh_connection_handle(&self) -> Option<SshConnectionHandle> {
        self.backend.ssh_connection_handle()
    }
}
