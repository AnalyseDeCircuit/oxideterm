#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalSessionKind {
    LocalPty,
    SshPty,
    Telnet,
    Serial,
    RawTcp,
    RawUdp,
}

pub type TerminalOutputProcessor = Arc<dyn Fn(&[u8]) -> Vec<u8> + Send + Sync>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SerialControlState {
    pub data_terminal_ready: bool,
    pub request_to_send: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialControlLine {
    DataTerminalReady,
    RequestToSend,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialLineEnding {
    Lf,
    CrLf,
    Cr,
    None,
}

impl Default for SerialLineEnding {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialDisplayMode {
    Text,
    Hex,
    Mixed,
}

impl Default for SerialDisplayMode {
    fn default() -> Self {
        Self::Text
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialSendMode {
    Text,
    Hex,
}

impl Default for SerialSendMode {
    fn default() -> Self {
        Self::Text
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SerialRuntimeOptions {
    pub line_ending: SerialLineEnding,
    pub display_mode: SerialDisplayMode,
    pub send_mode: SerialSendMode,
    pub local_echo: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawTcpLineEnding {
    Lf,
    CrLf,
    Cr,
    None,
}

impl Default for RawTcpLineEnding {
    fn default() -> Self {
        Self::CrLf
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawTcpDisplayMode {
    Text,
    Hex,
    Mixed,
}

impl Default for RawTcpDisplayMode {
    fn default() -> Self {
        Self::Text
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawTcpSendMode {
    Text,
    Hex,
}

impl Default for RawTcpSendMode {
    fn default() -> Self {
        Self::Text
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawTcpRuntimeOptions {
    pub line_ending: RawTcpLineEnding,
    pub display_mode: RawTcpDisplayMode,
    pub send_mode: RawTcpSendMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawUdpLineEnding {
    Lf,
    CrLf,
    Cr,
    None,
}

impl Default for RawUdpLineEnding {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawUdpDisplayMode {
    Text,
    Hex,
    Mixed,
}

impl Default for RawUdpDisplayMode {
    fn default() -> Self {
        Self::Text
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawUdpSendMode {
    Text,
    Hex,
}

impl Default for RawUdpSendMode {
    fn default() -> Self {
        Self::Text
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawUdpRuntimeOptions {
    pub line_ending: RawUdpLineEnding,
    pub display_mode: RawUdpDisplayMode,
    pub send_mode: RawUdpSendMode,
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
    fn set_output_processor(&mut self, _processor: Option<TerminalOutputProcessor>) {}
    fn set_output_events_enabled(&mut self, _enabled: bool) {}
    fn set_raw_tcp_runtime_options(&mut self, _options: RawTcpRuntimeOptions) -> Result<()> {
        bail!("Raw TCP runtime options are only supported by Raw TCP sessions")
    }
    fn set_raw_udp_runtime_options(&mut self, _options: RawUdpRuntimeOptions) -> Result<()> {
        bail!("Raw UDP runtime options are only supported by Raw UDP sessions")
    }
    fn serial_runtime_options(&self) -> Option<SerialRuntimeOptions> {
        None
    }
    fn set_serial_runtime_options(&mut self, _options: SerialRuntimeOptions) -> Result<()> {
        bail!("Serial runtime options are only supported by serial sessions")
    }
    fn serial_control_state(&self) -> Option<SerialControlState> {
        None
    }
    fn set_serial_control_line(
        &mut self,
        _line: SerialControlLine,
        _asserted: bool,
    ) -> Result<()> {
        bail!("Serial control lines are only supported by serial sessions")
    }
    fn send_serial_break(&mut self) -> Result<()> {
        bail!("Serial break is only supported by serial sessions")
    }
    fn set_trzsz_policy(&mut self, _policy: Option<TrzszTransferPolicy>) {}
    fn take_trzsz_transfer(&mut self) -> Option<TrzszTransfer> {
        None
    }
    fn feed_recording_output(&mut self, _bytes: &[u8]) {}
    fn reset_recording_playback(&mut self, _cols: usize, _rows: usize) {}
    fn feed_trzsz_terminal_output(&mut self, _bytes: &[u8]) {}
    fn interrupt_trzsz_transfer(&mut self) {}
    fn finish_trzsz_transfer(&mut self) {}
    fn start_modem_transfer(
        &mut self,
        _request: TerminalModemTransferRequest,
    ) -> Option<ModemTransfer> {
        None
    }
    fn interrupt_modem_transfer(&mut self) {}
    fn finish_modem_transfer(&mut self) {}
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
    fn clear_buffer(&mut self);
    fn buffer_text(&self) -> String {
        String::new()
    }
    fn command_output_text(&self, _mark: &TerminalCommandMark) -> String {
        String::new()
    }
    fn snapshot(&self) -> TerminalSnapshot;
    fn snapshot_with_display_offset(
        &self,
        display_offset: usize,
        rows: usize,
    ) -> TerminalSnapshot {
        let _ = (display_offset, rows);
        self.snapshot()
    }
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
