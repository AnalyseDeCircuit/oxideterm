use std::{cell::Cell, sync::Arc};

use alacritty_terminal::{
    event::Event as AlacEvent,
    grid::{Dimensions, Scroll},
    sync::FairMutex,
    term::{Config, Term},
    vte::ansi::Processor,
};
use anyhow::Result;
use crossbeam_channel::{Receiver, unbounded};
use oxideterm_ssh::{
    ConnectionConsumer, SshConfig, SshConnectionRegistry, SshPromptHandler, SshPtyHandle,
    SshTransportClient, SshTransportCommand,
};
use oxideterm_terminal_graphics::GraphicsIngress;
use tokio::runtime::Runtime;
use tokio::sync::broadcast::error::TryRecvError;

use crate::{
    LocalEventListener, LocalPtySession, TermMode, TerminalEvent, TerminalGraphicsState,
    TerminalLifecycle, TerminalProcessInfo, TerminalSearchMatch, TerminalSize, TerminalSnapshot,
    append_grid_line_text, focus_report_sequence, graphics_cursor_from_term,
    normalize_paste_line_endings, search_logical_line_matches, snapshot_from_term,
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
            backend: Box::new(SshPtySession::new(config, cols, rows)),
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

#[derive(Clone)]
pub struct SshSessionConfig {
    config: SshConfig,
    registry: Option<SshConnectionRegistry>,
    consumer: Option<ConnectionConsumer>,
    prompt_handler: Option<Arc<dyn SshPromptHandler>>,
}

impl SshSessionConfig {
    pub fn new(host: impl Into<String>, port: u16, username: impl Into<String>) -> Self {
        Self {
            config: SshConfig::password(host, port, username, ""),
            registry: None,
            consumer: None,
            prompt_handler: None,
        }
    }

    pub fn host(&self) -> &str {
        &self.config.host
    }

    pub fn port(&self) -> u16 {
        self.config.port
    }

    pub fn username(&self) -> &str {
        &self.config.username
    }

    pub fn with_registry(
        mut self,
        registry: SshConnectionRegistry,
        consumer: ConnectionConsumer,
    ) -> Self {
        self.registry = Some(registry);
        self.consumer = Some(consumer);
        self
    }

    pub fn with_prompt_handler(mut self, prompt_handler: Arc<dyn SshPromptHandler>) -> Self {
        self.prompt_handler = Some(prompt_handler);
        self
    }
}

impl From<oxideterm_ssh::SshConfig> for SshSessionConfig {
    fn from(config: oxideterm_ssh::SshConfig) -> Self {
        Self {
            config,
            registry: None,
            consumer: None,
            prompt_handler: None,
        }
    }
}

impl std::fmt::Debug for SshSessionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshSessionConfig")
            .field("config", &self.config)
            .field("registry", &self.registry)
            .field("consumer", &self.consumer)
            .field("prompt_handler", &self.prompt_handler.is_some())
            .finish()
    }
}

pub struct SshPtySession {
    config: SshSessionConfig,
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    parser: Processor,
    event_rx: Receiver<AlacEvent>,
    pending_events: Vec<TerminalEvent>,
    resize: TerminalResize,
    lifecycle: TerminalLifecycle,
    runtime: Option<Runtime>,
    connect_rx: Receiver<Result<SshPtyHandle, String>>,
    handle: Option<SshPtyHandle>,
    title: Option<String>,
    graphics_ingress: GraphicsIngress,
    graphics: TerminalGraphicsState,
}

impl SshPtySession {
    pub fn new(config: SshSessionConfig, cols: usize, rows: usize) -> Self {
        let resize = TerminalResize::new(cols, rows, 0, 0);
        let size = TerminalSize {
            cols: resize.cols,
            rows: resize.rows,
            cell_width: resize.cell_width,
            cell_height: resize.cell_height,
        };
        let (event_tx, event_rx) = unbounded();
        let listener = LocalEventListener { tx: event_tx };

        let mut term_config = Config::default();
        term_config.scrolling_history = 10000;
        term_config.kitty_keyboard = true;
        let term = Arc::new(FairMutex::new(Term::new(
            term_config,
            &size,
            listener.clone(),
        )));

        let runtime = Runtime::new().ok();
        let (connect_tx, connect_rx) = unbounded();
        if let Some(runtime) = runtime.as_ref() {
            let mut ssh_config = config.config.clone();
            ssh_config.cols = resize.cols as u32;
            ssh_config.rows = resize.rows as u32;
            let registry = config.registry.clone();
            let consumer = config.consumer.clone();
            let prompt_handler = config.prompt_handler.clone();
            runtime.spawn(async move {
                let mut client = SshTransportClient::new(ssh_config);
                if let Some(prompt_handler) = prompt_handler {
                    client = client.with_prompt_handler(prompt_handler);
                }
                let result = match (registry, consumer) {
                    (Some(registry), Some(consumer)) => {
                        client.connect_shell_with_registry(registry, consumer).await
                    }
                    _ => client.connect_shell().await,
                }
                .map_err(|error| error.to_string());
                let _ = connect_tx.send(result);
            });
        } else {
            let _ = connect_tx.send(Err("failed to initialize SSH runtime".to_string()));
        }

        Self {
            config,
            term,
            parser: Processor::new(),
            event_rx,
            pending_events: Vec::new(),
            resize,
            lifecycle: TerminalLifecycle::Running,
            runtime,
            connect_rx,
            handle: None,
            title: None,
            graphics_ingress: GraphicsIngress::new(crate::GraphicsOptions::default()),
            graphics: TerminalGraphicsState::default(),
        }
    }

    fn title_text(&self) -> String {
        format!("{}@{}", self.config.username(), self.config.host())
    }

    fn process_connect_result(&mut self) -> bool {
        let Ok(result) = self.connect_rx.try_recv() else {
            return false;
        };

        match result {
            Ok(handle) => {
                self.handle = Some(handle);
                let _ = self.send_command(SshTransportCommand::Resize {
                    cols: self.resize.cols as u16,
                    rows: self.resize.rows as u16,
                });
                self.title = Some(self.title_text());
                self.pending_events
                    .push(TerminalEvent::TitleChanged(self.title_text()));
                true
            }
            Err(error) => {
                self.lifecycle = TerminalLifecycle::Exited(None);
                self.feed_transport_output(
                    format!("\r\nSSH connection failed: {error}\r\n").as_bytes(),
                );
                self.pending_events.push(TerminalEvent::ChildExited(None));
                true
            }
        }
    }

    fn feed_transport_output(&mut self, bytes: &[u8]) {
        let mut term = self.term.lock();
        let size = TerminalSize {
            cols: self.resize.cols,
            rows: self.resize.rows,
            cell_width: self.resize.cell_width,
            cell_height: self.resize.cell_height,
        };
        let cursor = Cell::new(graphics_cursor_from_term(&term, size));
        let events = self.graphics_ingress.advance_with(
            bytes,
            |terminal_bytes| {
                self.parser.advance(&mut *term, terminal_bytes);
                cursor.set(graphics_cursor_from_term(&term, size));
            },
            || cursor.get(),
        );
        drop(term);
        for event in events {
            if let Some(response) = self.graphics.handle_event(event) {
                let _ = self.write_input(&response);
            }
        }
    }

    fn drain_transport_output(&mut self) -> bool {
        let mut changed = false;
        loop {
            let result = {
                let Some(handle) = self.handle.as_mut() else {
                    return changed;
                };
                handle.output_rx.try_recv()
            };

            match result {
                Ok(bytes) => {
                    self.feed_transport_output(&bytes);
                    changed = true;
                }
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Closed) => {
                    if self.lifecycle.is_running() {
                        self.lifecycle = TerminalLifecycle::Exited(None);
                        self.pending_events.push(TerminalEvent::ChildExited(None));
                    }
                    return true;
                }
                Err(TryRecvError::Empty) => return changed,
            }
        }
    }

    fn handle_alacritty_event(&mut self, event: AlacEvent) -> bool {
        match event {
            AlacEvent::Title(title) => {
                self.title = Some(title.clone());
                self.pending_events.push(TerminalEvent::TitleChanged(title));
                false
            }
            AlacEvent::ResetTitle => {
                self.title = Some(self.title_text());
                self.pending_events
                    .push(TerminalEvent::TitleChanged(self.title_text()));
                false
            }
            AlacEvent::Bell => {
                self.pending_events.push(TerminalEvent::Bell);
                false
            }
            AlacEvent::Wakeup | AlacEvent::MouseCursorDirty => {
                self.pending_events.push(TerminalEvent::Wakeup);
                true
            }
            AlacEvent::CursorBlinkingChange => {
                let blinking = self.term.lock().cursor_style().blinking;
                self.pending_events
                    .push(TerminalEvent::BlinkChanged(blinking));
                true
            }
            AlacEvent::PtyWrite(text) => {
                let _ = self.write_input(text.as_bytes());
                false
            }
            AlacEvent::ClipboardStore(_, text) => {
                self.pending_events
                    .push(TerminalEvent::ClipboardStore(text));
                false
            }
            AlacEvent::ClipboardLoad(_, formatter) => {
                self.pending_events
                    .push(TerminalEvent::ClipboardLoad(formatter));
                false
            }
            AlacEvent::ColorRequest(_, _) | AlacEvent::TextAreaSizeRequest(_) => false,
            AlacEvent::ChildExit(_) | AlacEvent::Exit => false,
        }
    }

    fn send_command(&mut self, command: SshTransportCommand) -> Result<()> {
        let Some(handle) = self.handle.as_mut() else {
            anyhow::bail!(
                "SSH PTY backend for {}@{}:{} is still connecting",
                self.config.username(),
                self.config.host(),
                self.config.port()
            );
        };
        handle
            .command_tx
            .try_send(command)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }
}

impl TerminalSessionBackend for SshPtySession {
    fn kind(&self) -> TerminalSessionKind {
        TerminalSessionKind::SshPty
    }

    fn title(&self) -> Option<String> {
        Some(self.title.clone().unwrap_or_else(|| self.title_text()))
    }

    fn lifecycle(&self) -> TerminalLifecycle {
        self.lifecycle.clone()
    }

    fn process_info(&self) -> TerminalProcessInfo {
        TerminalProcessInfo::default()
    }

    fn refresh_process_info(&mut self) {}

    fn read_pending(&mut self) -> bool {
        let mut changed = self.process_connect_result();
        changed |= self.drain_transport_output();
        while let Ok(event) = self.event_rx.try_recv() {
            if self.handle_alacritty_event(event) {
                changed = true;
            }
        }
        changed
    }

    fn take_events(&mut self) -> Vec<TerminalEvent> {
        std::mem::take(&mut self.pending_events)
    }

    fn write_input(&mut self, bytes: &[u8]) -> Result<()> {
        if self.lifecycle.is_running() && !bytes.is_empty() {
            self.send_command(SshTransportCommand::Data(bytes.to_vec()))?;
        }
        Ok(())
    }

    fn paste_text(&mut self, text: &str) -> Result<()> {
        let paste_text = if self.mode().contains(TermMode::BRACKETED_PASTE) {
            format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', ""))
        } else {
            normalize_paste_line_endings(text)
        };

        self.write_input(paste_text.as_bytes())
    }

    fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    fn set_focused(&mut self, focused: bool) -> Result<()> {
        let should_report = {
            let mut term = self.term.lock();
            term.is_focused = focused;
            term.mode().contains(TermMode::FOCUS_IN_OUT)
        };

        if let Some(report) = focus_report_sequence(should_report, focused) {
            self.write_input(report)?;
        }

        Ok(())
    }

    fn resize_with_cell_size(&mut self, resize: TerminalResize) -> Result<()> {
        self.resize = resize;
        let size = TerminalSize {
            cols: resize.cols,
            rows: resize.rows,
            cell_width: resize.cell_width,
            cell_height: resize.cell_height,
        };
        self.term.lock().resize(size);
        let _ = self.send_command(SshTransportCommand::Resize {
            cols: resize.cols as u16,
            rows: resize.rows as u16,
        });
        Ok(())
    }

    fn scroll_lines(&mut self, delta: i32) {
        if delta != 0 {
            self.term.lock().scroll_display(Scroll::Delta(delta));
        }
    }

    fn page_up(&mut self) {
        self.term.lock().scroll_display(Scroll::PageUp);
    }

    fn page_down(&mut self) {
        self.term.lock().scroll_display(Scroll::PageDown);
    }

    fn scroll_to_top(&mut self) {
        self.term.lock().scroll_display(Scroll::Top);
    }

    fn scroll_to_bottom(&mut self) {
        self.term.lock().scroll_display(Scroll::Bottom);
    }

    fn scroll_to_display_offset(&mut self, offset: usize) {
        let mut term = self.term.lock();
        let max_offset = term.total_lines().saturating_sub(term.screen_lines());
        let target = offset.min(max_offset);
        let current = term.grid().display_offset();
        let delta = target as i32 - current as i32;
        if delta != 0 {
            term.scroll_display(Scroll::Delta(delta));
        }
    }

    fn search_matches(&self, query: &str) -> Vec<TerminalSearchMatch> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }

        let term = self.term.lock();
        let grid = term.grid();
        let top_line = -(term.total_lines().saturating_sub(term.screen_lines()) as i32);
        let bottom_line = term.screen_lines() as i32;
        let mut matches = Vec::new();
        let mut logical_text = String::new();
        let mut logical_map = Vec::new();

        for line in top_line..bottom_line {
            let row = &grid[alacritty_terminal::index::Line(line)];
            append_grid_line_text(
                row[..].iter(),
                line,
                self.resize.cols,
                &mut logical_text,
                &mut logical_map,
            );

            let wrapped = row.last().is_some_and(|cell| {
                cell.flags
                    .contains(alacritty_terminal::term::cell::Flags::WRAPLINE)
            });
            if wrapped && line + 1 < bottom_line {
                continue;
            }

            matches.extend(search_logical_line_matches(
                &logical_text,
                &logical_map,
                query,
                self.resize.cols,
            ));
            logical_text.clear();
            logical_map.clear();
        }

        matches
    }

    fn snapshot(&self) -> TerminalSnapshot {
        let term = self.term.lock();
        snapshot_from_term(
            &term,
            TerminalSize {
                cols: self.resize.cols,
                rows: self.resize.rows,
                cell_width: self.resize.cell_width,
                cell_height: self.resize.cell_height,
            },
            &self.graphics,
        )
    }

    fn terminate_active_task(&mut self) -> Result<()> {
        self.write_input(b"\x03")
    }

    fn kill_active_task(&mut self) -> Result<()> {
        self.write_input(b"\x03")
    }

    fn shutdown(&mut self) {
        if matches!(self.lifecycle, TerminalLifecycle::Closed) {
            return;
        }
        let _ = self.send_command(SshTransportCommand::Close);
        self.handle = None;
        self.runtime = None;
        self.lifecycle = TerminalLifecycle::Closed;
    }
}
