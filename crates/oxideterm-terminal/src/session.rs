use anyhow::Result;

use crate::{
    LocalPtySession, TermMode, TerminalAttrs, TerminalCell, TerminalColor, TerminalCursorShape,
    TerminalEvent, TerminalLifecycle, TerminalProcessInfo, TerminalRow, TerminalSearchMatch,
    TerminalSnapshot,
};

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
    fn process_info(&self) -> TerminalProcessInfo;
    fn refresh_process_info(&mut self);
    fn read_pending(&mut self) -> bool;
    fn take_events(&mut self) -> Vec<TerminalEvent>;
    fn write_input(&mut self, bytes: &[u8]) -> Result<()>;
    fn paste_text(&mut self, text: &str) -> Result<()>;
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

    fn status(&self) -> TerminalSessionStatus {
        TerminalSessionStatus {
            kind: self.kind(),
            title: self.title(),
            lifecycle: self.lifecycle(),
            process_info: self.process_info(),
        }
    }
}

pub struct TerminalSession {
    backend: Box<dyn TerminalSessionBackend>,
}

impl TerminalSession {
    pub fn local_default(cols: usize, rows: usize) -> Result<Self> {
        Ok(Self {
            backend: Box::new(LocalPtySession::spawn_default(cols, rows)?),
        })
    }

    pub fn ssh(config: SshSessionConfig, cols: usize, rows: usize) -> Self {
        Self {
            backend: Box::new(SshPtySession::new_disconnected(config, cols, rows)),
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

    pub fn take_events(&mut self) -> Vec<TerminalEvent> {
        self.backend.take_events()
    }

    pub fn write_input(&mut self, bytes: &[u8]) -> Result<()> {
        self.backend.write_input(bytes)
    }

    pub fn paste_text(&mut self, text: &str) -> Result<()> {
        self.backend.paste_text(text)
    }

    pub fn lifecycle(&self) -> TerminalLifecycle {
        self.backend.lifecycle()
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

    pub fn snapshot(&self) -> TerminalSnapshot {
        self.backend.snapshot()
    }

    pub fn shutdown(&mut self) {
        self.backend.shutdown();
    }
}

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

    fn take_events(&mut self) -> Vec<TerminalEvent> {
        LocalPtySession::take_events(self)
    }

    fn write_input(&mut self, bytes: &[u8]) -> Result<()> {
        LocalPtySession::write_input(self, bytes)
    }

    fn paste_text(&mut self, text: &str) -> Result<()> {
        LocalPtySession::paste_text(self, text)
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshSessionConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
}

impl SshSessionConfig {
    pub fn new(host: impl Into<String>, port: u16, username: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port,
            username: username.into(),
        }
    }
}

impl From<oxideterm_ssh::SshConfig> for SshSessionConfig {
    fn from(config: oxideterm_ssh::SshConfig) -> Self {
        Self {
            host: config.host,
            port: config.port,
            username: config.username,
        }
    }
}

pub struct SshPtySession {
    config: SshSessionConfig,
    resize: TerminalResize,
    lifecycle: TerminalLifecycle,
}

impl SshPtySession {
    pub fn new_disconnected(config: SshSessionConfig, cols: usize, rows: usize) -> Self {
        Self {
            config,
            resize: TerminalResize::new(cols, rows, 0, 0),
            lifecycle: TerminalLifecycle::Closed,
        }
    }

    fn unsupported(&self) -> anyhow::Error {
        anyhow::anyhow!(
            "SSH PTY backend for {}@{}:{} is not connected yet",
            self.config.username,
            self.config.host,
            self.config.port
        )
    }
}

impl TerminalSessionBackend for SshPtySession {
    fn kind(&self) -> TerminalSessionKind {
        TerminalSessionKind::SshPty
    }

    fn title(&self) -> Option<String> {
        Some(format!("{}@{}", self.config.username, self.config.host))
    }

    fn lifecycle(&self) -> TerminalLifecycle {
        self.lifecycle.clone()
    }

    fn process_info(&self) -> TerminalProcessInfo {
        TerminalProcessInfo::default()
    }

    fn refresh_process_info(&mut self) {}

    fn read_pending(&mut self) -> bool {
        false
    }

    fn take_events(&mut self) -> Vec<TerminalEvent> {
        Vec::new()
    }

    fn write_input(&mut self, _bytes: &[u8]) -> Result<()> {
        Err(self.unsupported())
    }

    fn paste_text(&mut self, _text: &str) -> Result<()> {
        Err(self.unsupported())
    }

    fn mode(&self) -> TermMode {
        TermMode::empty()
    }

    fn set_focused(&mut self, _focused: bool) -> Result<()> {
        Ok(())
    }

    fn resize_with_cell_size(&mut self, resize: TerminalResize) -> Result<()> {
        self.resize = resize;
        Ok(())
    }

    fn scroll_lines(&mut self, _delta: i32) {}

    fn page_up(&mut self) {}

    fn page_down(&mut self) {}

    fn scroll_to_top(&mut self) {}

    fn scroll_to_bottom(&mut self) {}

    fn scroll_to_display_offset(&mut self, _offset: usize) {}

    fn search_matches(&self, _query: &str) -> Vec<TerminalSearchMatch> {
        Vec::new()
    }

    fn snapshot(&self) -> TerminalSnapshot {
        blank_snapshot(self.resize)
    }

    fn terminate_active_task(&mut self) -> Result<()> {
        Err(self.unsupported())
    }

    fn kill_active_task(&mut self) -> Result<()> {
        Err(self.unsupported())
    }

    fn shutdown(&mut self) {
        self.lifecycle = TerminalLifecycle::Closed;
    }
}

fn blank_snapshot(resize: TerminalResize) -> TerminalSnapshot {
    let blank_cell = TerminalCell {
        ch: ' ',
        zerowidth: String::new(),
        wide: false,
        fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
        bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
        attrs: TerminalAttrs::default(),
        hyperlink: None,
        cursor: false,
    };

    TerminalSnapshot {
        cols: resize.cols,
        rows: resize.rows,
        cursor_col: 0,
        cursor_row: 0,
        cursor_shape: TerminalCursorShape::Hidden,
        display_offset: 0,
        scrollback_lines: 0,
        lines: vec![
            TerminalRow {
                cells: vec![blank_cell; resize.cols],
                wrapped: false,
                active_input: false,
            };
            resize.rows
        ],
    }
}
