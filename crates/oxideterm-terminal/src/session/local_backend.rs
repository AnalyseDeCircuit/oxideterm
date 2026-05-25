impl TerminalSessionBackend for LocalPtySession {
    fn kind(&self) -> TerminalSessionKind {
        TerminalSessionKind::LocalPty
    }

    fn title(&self) -> Option<String> {
        self.title.clone()
    }

    fn lifecycle(&self) -> TerminalLifecycle {
        LocalPtySession::lifecycle(self)
    }

    fn process_info(&self) -> TerminalProcessInfo {
        LocalPtySession::process_info(self)
    }

    fn refresh_process_info(&mut self) {
        LocalPtySession::refresh_process_info(self);
    }

    fn read_pending(&mut self) -> bool {
        self.drain_output()
    }

    fn read_pending_with_budget(&mut self, budget: TerminalDrainBudget) -> TerminalDrainReport {
        LocalPtySession::drain_output_with_budget(self, budget)
    }

    fn take_events(&mut self) -> Vec<TerminalEvent> {
        LocalPtySession::take_events(self)
    }

    fn write_input(&mut self, bytes: &[u8]) -> Result<()> {
        LocalPtySession::write_input(self, bytes)
    }

    fn write_protocol_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        LocalPtySession::write_protocol_bytes(self, bytes)
    }

    fn write_text(&mut self, text: &str) -> Result<()> {
        LocalPtySession::write_text(self, text)
    }

    fn paste_text(&mut self, text: &str) -> Result<()> {
        LocalPtySession::paste_text(self, text)
    }

    fn set_encoding(&mut self, encoding: TerminalEncoding) {
        LocalPtySession::set_encoding(self, encoding);
    }

    fn set_output_processor(&mut self, processor: Option<TerminalOutputProcessor>) {
        LocalPtySession::set_output_processor(self, processor);
    }

    fn mode(&self) -> TermMode {
        LocalPtySession::mode(self)
    }

    fn set_focused(&mut self, focused: bool) -> Result<()> {
        LocalPtySession::set_focused(self, focused)
    }

    fn resize_with_cell_size(&mut self, resize: TerminalResize) -> Result<()> {
        self.apply_resize(resize)
    }

    fn scroll_lines(&mut self, delta: i32) {
        LocalPtySession::scroll_lines(self, delta);
    }

    fn page_up(&mut self) {
        LocalPtySession::page_up(self);
    }

    fn page_down(&mut self) {
        LocalPtySession::page_down(self);
    }

    fn scroll_to_top(&mut self) {
        LocalPtySession::scroll_to_top(self);
    }

    fn scroll_to_bottom(&mut self) {
        LocalPtySession::scroll_to_bottom(self);
    }

    fn scroll_to_display_offset(&mut self, offset: usize) {
        LocalPtySession::scroll_to_display_offset(self, offset);
    }

    fn search_matches(&self, query: &str) -> Vec<TerminalSearchMatch> {
        LocalPtySession::search_matches(self, query)
    }

    fn clear_buffer(&mut self) {
        LocalPtySession::clear_buffer(self);
    }

    fn command_output_text(&self, mark: &TerminalCommandMark) -> String {
        let term = self.term.lock();
        command_output_text_from_term(&term, mark)
    }

    fn snapshot(&self) -> TerminalSnapshot {
        LocalPtySession::snapshot(self)
    }

    fn terminate_active_task(&mut self) -> Result<()> {
        LocalPtySession::terminate_active_task(self)
    }

    fn kill_active_task(&mut self) -> Result<()> {
        LocalPtySession::kill_active_task(self)
    }

    fn shutdown(&mut self) {
        LocalPtySession::shutdown(self);
    }
}
