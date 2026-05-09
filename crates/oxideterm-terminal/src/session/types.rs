#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalSessionKind {
    LocalPty,
    SshPty,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalResize {
    pub cols: usize,
    pub rows: usize,
    pub cell_width: u16,
    pub cell_height: u16,
}

impl TerminalResize {
    pub fn new(cols: usize, rows: usize, cell_width: u16, cell_height: u16) -> Self {
        Self {
            cols: cols.max(2),
            rows: rows.max(2),
            cell_width,
            cell_height,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSessionStatus {
    pub kind: TerminalSessionKind,
    pub title: Option<String>,
    pub lifecycle: TerminalLifecycle,
    pub process_info: TerminalProcessInfo,
}

pub trait TerminalSessionBackend: Send {
    fn kind(&self) -> TerminalSessionKind;
    fn title(&self) -> Option<String>;
    fn lifecycle(&self) -> TerminalLifecycle;
    fn is_interactive(&self) -> bool {
        self.lifecycle().is_running()
    }
    fn process_info(&self) -> TerminalProcessInfo;
    fn refresh_process_info(&mut self);
    fn read_pending(&mut self) -> bool;
    fn read_pending_with_budget(&mut self, budget: TerminalDrainBudget) -> TerminalDrainReport;
    fn take_events(&mut self) -> Vec<TerminalEvent>;
    fn write_input(&mut self, bytes: &[u8]) -> Result<()>;
    fn write_protocol_bytes(&mut self, bytes: &[u8]) -> Result<()>;
    fn write_text(&mut self, text: &str) -> Result<()>;
    fn paste_text(&mut self, text: &str) -> Result<()>;
    fn set_encoding(&mut self, encoding: TerminalEncoding);
    fn set_trzsz_policy(&mut self, _policy: Option<TrzszTransferPolicy>) {}
    fn take_trzsz_transfer(&mut self) -> Option<TrzszTransfer> {
        None
    }
    fn feed_trzsz_terminal_output(&mut self, _bytes: &[u8]) {}
    fn interrupt_trzsz_transfer(&mut self) {}
    fn finish_trzsz_transfer(&mut self) {}
    fn mode(&self) -> TermMode;
    fn set_focused(&mut self, focused: bool) -> Result<()>;
    fn resize_with_cell_size(&mut self, resize: TerminalResize) -> Result<()>;
    fn scroll_lines(&mut self, delta: i32);
    fn page_up(&mut self);
    fn page_down(&mut self);
    fn scroll_to_top(&mut self);
    fn scroll_to_bottom(&mut self);
    fn scroll_to_display_offset(&mut self, offset: usize);
    fn search_matches(&self, query: &str) -> Vec<TerminalSearchMatch>;
    fn snapshot(&self) -> TerminalSnapshot;
    fn terminate_active_task(&mut self) -> Result<()>;
    fn kill_active_task(&mut self) -> Result<()>;
    fn shutdown(&mut self);
    fn ssh_connection_handle(&self) -> Option<SshConnectionHandle> {
        None
    }

    fn status(&self) -> TerminalSessionStatus {
        TerminalSessionStatus {
            kind: self.kind(),
            title: self.title(),
            lifecycle: self.lifecycle(),
            process_info: self.process_info(),
        }
    }
}
