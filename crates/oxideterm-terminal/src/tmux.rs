// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    cell::Cell,
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, RwLock},
};

use alacritty_terminal::{
    event::EventListener, grid::Dimensions, sync::FairMutex, term::Term, vte::ansi::Processor,
};
use oxideterm_terminal_encoding::{TerminalEncoding, TerminalOutputDecoder};
use oxideterm_terminal_graphics::{GraphicsIngress, GraphicsOptions, TerminalGraphicsSegment};
use oxideterm_tmux::{
    CommandGuard, ControlEvent, ControlStream, ControlStreamError, Layout, LayoutKind,
    Notification, PaneId, SessionId, SplitDirection, StreamEvent, WindowId,
};

use crate::{
    AlacEvent, LocalEventListener, LocalEventReceiver, TerminalEvent, TerminalGraphicsState,
    TerminalImageId, TerminalSize, TerminalSnapshot, blank_snapshot_row, graphics_cursor_from_term,
    incremental_snapshot_from_term, interactive_terminal_config,
    shell_integration::TerminalShellIntegration, snapshot_from_term,
};

const INPUT_BYTES_PER_COMMAND: usize = 512;
const CONTROL_PAUSE_AFTER_SECONDS: u64 = 5;

pub(crate) type SharedTmuxTerm = Arc<FairMutex<Term<LocalEventListener>>>;

#[derive(Clone)]
struct SharedTmuxPane {
    term: SharedTmuxTerm,
    graphics: Arc<Mutex<TerminalGraphicsState>>,
}

#[derive(Clone, Copy)]
enum ExternalReply {
    Ignore,
    Capture(PaneId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmuxSessionInfo {
    pub id: u64,
    pub name: String,
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmuxWindowInfo {
    pub id: u64,
    pub index: u64,
    pub name: String,
    pub active: bool,
    pub flags: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TmuxUiState {
    pub ready: bool,
    pub sessions: Vec<TmuxSessionInfo>,
    pub windows: Vec<TmuxWindowInfo>,
    pub active_pane: Option<u64>,
    pub pane_count: usize,
    pub pane_in_mode: bool,
    pub error: Option<String>,
    pub message: Option<String>,
    pub message_generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TmuxSeparatorDirection {
    LeftRight,
    TopBottom,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TmuxSeparator {
    pub direction: TmuxSeparatorDirection,
    before_pane: PaneId,
    after_pane: PaneId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TmuxAction {
    SelectSession(u64),
    SelectWindow(u64),
    PreviousWindow,
    NextWindow,
    NewSession,
    CloseSession,
    NewWindow,
    CloseWindow,
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    ResizePaneLeft,
    ResizePaneRight,
    ResizePaneUp,
    ResizePaneDown,
    Detach,
    CancelPaneMode,
    RenameSession { id: u64, name: String },
    RenameWindow { id: u64, name: String },
    RunCommand(String),
}

#[derive(Default)]
struct TmuxDisplayState {
    active: bool,
    ready: bool,
    pane: Option<PaneId>,
    term: Option<SharedTmuxTerm>,
    layout: Option<Layout>,
    panes: HashMap<PaneId, SharedTmuxPane>,
    sessions: Vec<TmuxSessionInfo>,
    windows: Vec<TmuxWindowInfo>,
    pane_modes: HashMap<PaneId, bool>,
    error: Option<String>,
    message: Option<String>,
    message_generation: u64,
    route_keys_through_client: bool,
}

/// Shares only the currently displayed emulator with the pane owner.
///
/// The transport reader retains the complete tmux domain. UI teardown cannot
/// own or cancel the PTY/SSH connection through this display projection.
#[derive(Default)]
pub(crate) struct TmuxDisplay {
    state: RwLock<TmuxDisplayState>,
    snapshot_cache: Mutex<HashMap<PaneId, TerminalSnapshot>>,
    external_replies: Mutex<VecDeque<ExternalReply>>,
}

impl TmuxDisplay {
    pub(crate) fn is_active(&self) -> bool {
        self.state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .active
    }

    pub(crate) fn reset(&self) {
        self.leave();
    }

    pub(crate) fn term(&self) -> Option<SharedTmuxTerm> {
        self.state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .term
            .clone()
    }

    fn is_ready(&self) -> bool {
        self.state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .ready
    }

    pub(crate) fn ui_state(&self) -> Option<TmuxUiState> {
        let state = self
            .state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.active.then(|| TmuxUiState {
            ready: state.ready,
            sessions: state.sessions.clone(),
            windows: state.windows.clone(),
            active_pane: state.pane.map(|pane| pane.0),
            pane_count: state.panes.len(),
            pane_in_mode: state
                .pane
                .and_then(|pane| state.pane_modes.get(&pane).copied())
                .unwrap_or(false),
            error: state.error.clone(),
            message: state.message.clone(),
            message_generation: state.message_generation,
        })
    }

    pub(crate) fn action_command(&self, action: TmuxAction) -> Option<Vec<u8>> {
        let state = self
            .state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.ready {
            return None;
        }
        let active_pane = state.pane?;
        let active_window = state.windows.iter().find(|window| window.active);
        let command = match action {
            TmuxAction::SelectSession(id)
                if state.sessions.iter().any(|session| session.id == id) =>
            {
                format!("switch-client -t ${id}\n")
            }
            TmuxAction::SelectWindow(id) if state.windows.iter().any(|window| window.id == id) => {
                format!("select-window -t @{id}\n")
            }
            TmuxAction::PreviousWindow => "previous-window\n".to_string(),
            TmuxAction::NextWindow => "next-window\n".to_string(),
            TmuxAction::NewSession => "new-session -d\n".to_string(),
            TmuxAction::CloseSession => {
                let session = state.sessions.iter().find(|session| session.active)?;
                format!("switch-client -n ; kill-session -t ${}\n", session.id)
            }
            TmuxAction::NewWindow => "new-window\n".to_string(),
            TmuxAction::CloseWindow if active_window.is_some() => {
                format!("kill-window -t @{}\n", active_window?.id)
            }
            TmuxAction::SplitHorizontal => format!("split-window -h -t {active_pane}\n"),
            TmuxAction::SplitVertical => format!("split-window -v -t {active_pane}\n"),
            TmuxAction::ClosePane if state.panes.len() > 1 => {
                format!("kill-pane -t {active_pane}\n")
            }
            TmuxAction::ResizePaneLeft => format!("resize-pane -L 2 -t {active_pane}\n"),
            TmuxAction::ResizePaneRight => format!("resize-pane -R 2 -t {active_pane}\n"),
            TmuxAction::ResizePaneUp => format!("resize-pane -U 1 -t {active_pane}\n"),
            TmuxAction::ResizePaneDown => format!("resize-pane -D 1 -t {active_pane}\n"),
            TmuxAction::Detach => "\n".to_string(),
            TmuxAction::CancelPaneMode if state.pane_modes.get(&active_pane) == Some(&true) => {
                format!("send-keys -X -t {active_pane} cancel\n")
            }
            TmuxAction::RenameSession { id, name }
                if state.sessions.iter().any(|session| session.id == id) =>
            {
                format!("rename-session -t ${id} {}\n", quote_tmux_argument(&name)?)
            }
            TmuxAction::RenameWindow { id, name }
                if state.windows.iter().any(|window| window.id == id) =>
            {
                format!("rename-window -t @{id} {}\n", quote_tmux_argument(&name)?)
            }
            TmuxAction::RunCommand(command) => normalize_tmux_command(&command)?,
            _ => return None,
        };
        self.expect_ignored_replies(1);
        Some(command.into_bytes())
    }

    fn pane(&self) -> Option<PaneId> {
        self.state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pane
    }

    pub(crate) fn input_commands(&self, bytes: &[u8]) -> Option<Vec<Vec<u8>>> {
        let state = self
            .state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.ready {
            return state.active.then(Vec::new);
        }
        let Some(pane) = state.pane else {
            return Some(Vec::new());
        };
        let mut commands = encoded_input_commands(pane, bytes, state.route_keys_through_client);
        let mut replies = commands
            .iter()
            .map(|_| ExternalReply::Ignore)
            .collect::<VecDeque<_>>();
        if state.pane_modes.get(&pane) == Some(&true) {
            commands.push(format!("capture-pane -p -e -t {pane}\n").into_bytes());
            replies.push_back(ExternalReply::Capture(pane));
        }
        self.external_replies
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .extend(replies);
        Some(commands)
    }

    pub(crate) fn paste_commands(&self, bytes: &[u8]) -> Option<Vec<Vec<u8>>> {
        self.literal_input_commands(bytes)
    }

    pub(crate) fn protocol_commands(&self, bytes: &[u8]) -> Option<Vec<Vec<u8>>> {
        self.literal_input_commands(bytes)
    }

    fn literal_input_commands(&self, bytes: &[u8]) -> Option<Vec<Vec<u8>>> {
        let state = self
            .state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.ready {
            return state.active.then(Vec::new);
        }
        let commands = state
            .pane
            .map(|pane| pane_reply_commands(pane, bytes))
            .unwrap_or_default();
        self.expect_ignored_replies(commands.len());
        Some(commands)
    }

    pub(crate) fn resize_command(&self, cols: usize, rows: usize) -> Option<Vec<u8>> {
        let command = self
            .state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .ready
            .then(|| format!("refresh-client -C {cols},{rows}\n").into_bytes());
        if command.is_some() {
            self.expect_ignored_replies(1);
        }
        command
    }

    pub(crate) fn select_pane_command(&self, col: usize, row: usize) -> Option<Vec<u8>> {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.ready {
            return None;
        }
        let pane = state.layout.as_ref()?.pane_at(col, row)?;
        if state.pane == Some(pane) {
            return None;
        }
        state.pane = Some(pane);
        state.term = state.panes.get(&pane).map(|pane| pane.term.clone());
        self.expect_ignored_replies(1);
        Some(format!("select-pane -t {pane}\n").into_bytes())
    }

    pub(crate) fn separator_at(&self, col: usize, row: usize) -> Option<TmuxSeparator> {
        let state = self
            .state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.ready {
            return None;
        }
        separator_at_layout(state.layout.as_ref()?, col, row)
    }

    pub(crate) fn resize_separator_command(
        &self,
        separator: TmuxSeparator,
        delta: i32,
    ) -> Option<Vec<u8>> {
        if delta == 0 {
            return None;
        }
        let state = self
            .state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.ready
            || !state.panes.contains_key(&separator.before_pane)
            || !state.panes.contains_key(&separator.after_pane)
        {
            return None;
        }
        let amount = delta.unsigned_abs();
        let command = match (separator.direction, delta.is_positive()) {
            (TmuxSeparatorDirection::LeftRight, true) => {
                format!("resize-pane -R {amount} -t {}\n", separator.before_pane)
            }
            (TmuxSeparatorDirection::LeftRight, false) => {
                format!("resize-pane -L {amount} -t {}\n", separator.after_pane)
            }
            (TmuxSeparatorDirection::TopBottom, true) => {
                format!("resize-pane -D {amount} -t {}\n", separator.before_pane)
            }
            (TmuxSeparatorDirection::TopBottom, false) => {
                format!("resize-pane -U {amount} -t {}\n", separator.after_pane)
            }
        };
        drop(state);
        self.expect_ignored_replies(1);
        Some(command.into_bytes())
    }

    pub(crate) fn local_point(&self, col: usize, row: usize) -> (usize, usize) {
        let state = self
            .state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(active) = state.pane else {
            return (col, row);
        };
        let Some((_, cell)) = state
            .layout
            .as_ref()
            .and_then(|layout| layout.panes().find(|(pane, _)| *pane == active))
        else {
            return (col, row);
        };
        (
            col.saturating_sub(usize::from(cell.x)),
            row.saturating_sub(usize::from(cell.y)),
        )
    }

    pub(crate) fn snapshot(
        &self,
        size: TerminalSize,
        previous: Option<&TerminalSnapshot>,
    ) -> Option<TerminalSnapshot> {
        let state = self
            .state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.active {
            return None;
        }
        let layout = state.layout.as_ref()?;
        let mut snapshot_cache = self
            .snapshot_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        snapshot_cache.retain(|pane, _| state.panes.contains_key(pane));
        let mut lines = (0..size.rows)
            .map(|row| blank_snapshot_row(size, 0, row))
            .collect::<Vec<_>>();
        let mut cursor_col = 0;
        let mut cursor_row = 0;
        let mut cursor_shape = crate::TerminalCursorShape::Block;
        let mut display_offset = 0;
        let mut scrollback_lines = 0;
        let mut images = Vec::new();

        for (pane, cell) in layout.panes() {
            let Some(shared_pane) = state.panes.get(&pane) else {
                continue;
            };
            let pane_size = TerminalSize {
                cols: usize::from(cell.width),
                rows: usize::from(cell.height),
                cell_width: size.cell_width,
                cell_height: size.cell_height,
            };
            let pane_snapshot = {
                let mut term = shared_pane.term.lock();
                let graphics = shared_pane
                    .graphics
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let snapshot = if let Some(previous) = snapshot_cache.get(&pane) {
                    incremental_snapshot_from_term(&mut term, pane_size, &graphics, previous)
                } else {
                    let snapshot = snapshot_from_term(&term, pane_size, &graphics);
                    term.reset_damage();
                    snapshot
                };
                snapshot_cache.insert(pane, snapshot.clone());
                snapshot
            };
            let active = state.pane == Some(pane);
            for (pane_row, source) in pane_snapshot.lines.iter().enumerate() {
                let target_row = usize::from(cell.y).saturating_add(pane_row);
                let Some(target) = lines.get_mut(target_row) else {
                    continue;
                };
                let target_col = usize::from(cell.x);
                let available = size.cols.saturating_sub(target_col);
                let copy_len = source.cells.len().min(available);
                let target_cells = target.cells_mut();
                for (target, source) in target_cells[target_col..target_col + copy_len]
                    .iter_mut()
                    .zip(source.cells.iter())
                {
                    *target = source.clone();
                    if !active {
                        target.cursor = false;
                    }
                }
            }
            if active {
                cursor_col = usize::from(cell.x).saturating_add(pane_snapshot.cursor_col);
                cursor_row = usize::from(cell.y).saturating_add(pane_snapshot.cursor_row);
                cursor_shape = pane_snapshot.cursor_shape;
                display_offset = pane_snapshot.display_offset;
                scrollback_lines = pane_snapshot.scrollback_lines;
            }
            for mut image in pane_snapshot.images {
                let available_cols = usize::from(cell.width).saturating_sub(image.col);
                let available_rows = usize::from(cell.height).saturating_sub(image.row);
                let visible_cols = image.cols.min(available_cols);
                let visible_rows = image.rows.min(available_rows);
                if visible_cols == 0 || visible_rows == 0 {
                    continue;
                }
                if visible_cols < image.cols {
                    image.source_width =
                        proportional_image_extent(image.source_width, visible_cols, image.cols);
                    image.cols = visible_cols;
                }
                if visible_rows < image.rows {
                    image.source_height =
                        proportional_image_extent(image.source_height, visible_rows, image.rows);
                    image.rows = visible_rows;
                }
                image.id = namespaced_image_id(pane, image.id);
                image.row = usize::from(cell.y).saturating_add(image.row);
                image.col = usize::from(cell.x).saturating_add(image.col);
                images.push(image);
            }
        }
        draw_layout_borders(layout, &mut lines, size);
        for line in &mut lines {
            line.source_id = 0;
            line.refresh_signature();
        }
        let mut snapshot = TerminalSnapshot {
            generation: 0,
            cols: size.cols,
            rows: size.rows,
            cursor_col,
            cursor_row,
            cursor_shape,
            display_offset,
            scrollback_lines,
            lines,
            images,
        };
        if let Some(previous) = previous {
            snapshot.reuse_unchanged_rows_from(previous);
        }
        Some(snapshot)
    }

    fn enter(&self) {
        {
            let mut state = self
                .state
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.active = true;
            state.ready = false;
            state.pane = None;
            state.term = None;
            state.layout = None;
            state.panes.clear();
            state.windows.clear();
            state.pane_modes.clear();
            state.error = None;
            state.message = None;
        }
        self.snapshot_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.external_replies
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    fn show(&self, pane: PaneId, term: SharedTmuxTerm) {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.active = true;
        state.ready = true;
        state.pane = Some(pane);
        state.term = Some(term);
    }

    fn publish_layout(&self, layout: Option<Layout>, panes: HashMap<PaneId, SharedTmuxPane>) {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.layout = layout;
        state.panes = panes;
    }

    fn publish_metadata(
        &self,
        sessions: Vec<TmuxSessionInfo>,
        windows: Vec<TmuxWindowInfo>,
        pane_modes: HashMap<PaneId, bool>,
    ) {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.sessions = sessions;
        state.windows = windows;
        state.pane_modes = pane_modes;
    }

    fn set_error(&self, error: Option<String>) {
        self.state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .error = error;
    }

    fn set_message(&self, message: String) {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.message_generation = state.message_generation.wrapping_add(1).max(1);
        state.message = Some(message);
    }

    fn set_key_routing(&self, enabled: bool) {
        self.state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .route_keys_through_client = enabled;
    }

    fn expect_ignored_replies(&self, count: usize) {
        self.external_replies
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .extend((0..count).map(|_| ExternalReply::Ignore));
    }

    fn take_external_reply(&self) -> Option<ExternalReply> {
        self.external_replies
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front()
    }

    fn leave(&self) {
        {
            let mut state = self
                .state
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let message_generation = state.message_generation;
            *state = TmuxDisplayState::default();
            // Keep message revisions monotonic across detach and re-entry so a
            // previously dismissed notification cannot hide a later one.
            state.message_generation = message_generation;
        }
        self.snapshot_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.external_replies
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }
}

pub(crate) struct TmuxAdvance {
    pub(crate) commands: Vec<Vec<u8>>,
    pub(crate) entered: bool,
    pub(crate) exited: bool,
    pub(crate) changed: bool,
}

impl TmuxAdvance {
    fn new() -> Self {
        Self {
            commands: Vec::new(),
            entered: false,
            exited: false,
            changed: false,
        }
    }
}

struct TmuxPane {
    term: SharedTmuxTerm,
    listener: LocalEventListener,
    events: LocalEventReceiver,
    parser: Processor,
    decoder: TerminalOutputDecoder,
    window: Option<WindowId>,
    cursor: (usize, usize),
    shell_integration: TerminalShellIntegration,
    graphics_ingress: GraphicsIngress,
    graphics: Arc<Mutex<TerminalGraphicsState>>,
    graphics_alt_screen_active: bool,
}

#[derive(Clone, Copy)]
enum BootstrapQuery {
    Ignore,
    Configure,
    Version,
    Sessions,
    Windows,
    Panes,
    Capture(PaneId),
    Cursors,
    PaneMode(PaneId),
    ActiveTarget,
}

struct BootstrapReply {
    query: BootstrapQuery,
    number: u64,
    lines: Vec<Vec<u8>>,
}

pub(crate) struct TmuxController {
    stream: ControlStream,
    display: Arc<TmuxDisplay>,
    listener: LocalEventListener,
    panes: HashMap<PaneId, TmuxPane>,
    active_session: Option<SessionId>,
    active_window: Option<WindowId>,
    active_pane: Option<PaneId>,
    window_active_panes: HashMap<WindowId, PaneId>,
    window_layouts: HashMap<WindowId, Layout>,
    sessions: HashMap<SessionId, String>,
    windows: HashMap<WindowId, TmuxWindowInfo>,
    pane_modes: HashMap<PaneId, bool>,
    route_keys_through_client: bool,
    report_client_colors: bool,
    pending_queries: VecDeque<BootstrapQuery>,
    active_reply: Option<BootstrapReply>,
    size: TerminalSize,
    encoding: TerminalEncoding,
    scrollback_lines: usize,
    graphics_options: GraphicsOptions,
}

impl TmuxController {
    pub(crate) fn new(
        display: Arc<TmuxDisplay>,
        listener: LocalEventListener,
        size: TerminalSize,
        encoding: TerminalEncoding,
        scrollback_lines: usize,
        graphics_options: GraphicsOptions,
    ) -> Self {
        Self {
            stream: ControlStream::new(),
            display,
            listener,
            panes: HashMap::new(),
            active_session: None,
            active_window: None,
            active_pane: None,
            window_active_panes: HashMap::new(),
            window_layouts: HashMap::new(),
            sessions: HashMap::new(),
            windows: HashMap::new(),
            pane_modes: HashMap::new(),
            route_keys_through_client: false,
            report_client_colors: false,
            pending_queries: VecDeque::new(),
            active_reply: None,
            size,
            encoding,
            scrollback_lines,
            graphics_options,
        }
    }

    pub(crate) fn advance(
        &mut self,
        bytes: &[u8],
        mut terminal_output: impl FnMut(&[u8]),
        record_output: bool,
        mut emit: impl FnMut(TerminalEvent),
    ) -> Result<TmuxAdvance, ControlStreamError> {
        let mut outcome = TmuxAdvance::new();
        let mut stream = std::mem::take(&mut self.stream);
        let result = stream.advance(bytes, |event| match event {
            StreamEvent::TerminalOutput(bytes) => terminal_output(bytes.as_ref()),
            StreamEvent::ControlModeEntered => self.enter(&mut outcome),
            StreamEvent::Control(event) => {
                self.handle_control_event(event, record_output, &mut emit, &mut outcome)
            }
            StreamEvent::ControlModeExited => self.leave(&mut outcome),
            _ => {}
        });
        self.stream = stream;
        result.map(|()| outcome)
    }

    pub(crate) fn set_encoding(&mut self, encoding: TerminalEncoding) {
        if self.encoding == encoding {
            return;
        }
        self.encoding = encoding;
        for pane in self.panes.values_mut() {
            pane.decoder.set_encoding(encoding);
            pane.decoder.reset();
        }
    }

    pub(crate) fn resize(&mut self, size: TerminalSize) {
        self.size = size;
    }

    fn enter(&mut self, outcome: &mut TmuxAdvance) {
        self.panes.clear();
        self.active_session = None;
        self.active_window = None;
        self.active_pane = None;
        self.window_active_panes.clear();
        self.window_layouts.clear();
        self.sessions.clear();
        self.windows.clear();
        self.pane_modes.clear();
        self.route_keys_through_client = false;
        self.report_client_colors = false;
        self.display.enter();
        self.pending_queries.clear();
        self.active_reply = None;
        self.pending_queries.push_back(BootstrapQuery::Configure);
        self.pending_queries.push_back(BootstrapQuery::Version);
        self.pending_queries.push_back(BootstrapQuery::Sessions);
        self.pending_queries.push_back(BootstrapQuery::Windows);
        self.pending_queries.push_back(BootstrapQuery::Panes);
        outcome.commands.push(
            format!("refresh-client -C {},{}\n", self.size.cols, self.size.rows).into_bytes(),
        );
        outcome
            .commands
            .push(b"display-message -p '#{version}'\n".to_vec());
        outcome
            .commands
            .push(b"list-sessions -F '#{session_id} #{session_name}'\n".to_vec());
        outcome.commands.push(
            b"list-windows -F '#{window_id} #{window_index} #{window_active} #{window_flags} #{window_name}'\n"
                .to_vec(),
        );
        outcome.commands.push(
            b"list-panes -s -F '#{pane_id} #{window_id} #{pane_active} #{pane_width} #{pane_height} #{cursor_x} #{cursor_y} #{window_layout}'\n"
                .to_vec(),
        );
        outcome.entered = true;
        outcome.changed = true;
    }

    fn leave(&mut self, outcome: &mut TmuxAdvance) {
        self.display.leave();
        self.pending_queries.clear();
        self.active_reply = None;
        self.panes.clear();
        self.active_session = None;
        self.active_window = None;
        self.active_pane = None;
        self.window_active_panes.clear();
        self.window_layouts.clear();
        self.sessions.clear();
        self.windows.clear();
        self.pane_modes.clear();
        self.route_keys_through_client = false;
        self.report_client_colors = false;
        outcome.exited = true;
        outcome.changed = true;
    }

    fn handle_control_event(
        &mut self,
        event: ControlEvent<'_>,
        record_output: bool,
        emit: &mut impl FnMut(TerminalEvent),
        outcome: &mut TmuxAdvance,
    ) {
        match event {
            ControlEvent::Begin(guard) => self.begin_reply(guard),
            ControlEvent::CommandOutput(line) => {
                if let Some(reply) = self.active_reply.as_mut() {
                    reply.lines.push(line.to_vec());
                }
            }
            ControlEvent::End(guard) => self.finish_reply(guard, true, outcome),
            ControlEvent::Error(guard) => self.finish_reply(guard, false, outcome),
            ControlEvent::Notification(notification) => {
                self.handle_notification(notification, record_output, emit, outcome)
            }
            ControlEvent::Text(_) => {}
            _ => {}
        }
    }

    fn begin_reply(&mut self, guard: CommandGuard) {
        if !guard.is_control_command() {
            return;
        }
        if self.active_reply.is_none() {
            let query = self
                .pending_queries
                .pop_front()
                .or_else(|| {
                    self.display.take_external_reply().map(|reply| match reply {
                        ExternalReply::Ignore => BootstrapQuery::Ignore,
                        ExternalReply::Capture(pane) => BootstrapQuery::Capture(pane),
                    })
                })
                .unwrap_or(BootstrapQuery::Ignore);
            self.active_reply = Some(BootstrapReply {
                query,
                number: guard.number,
                lines: Vec::new(),
            });
        }
    }

    fn finish_reply(&mut self, guard: CommandGuard, succeeded: bool, outcome: &mut TmuxAdvance) {
        let Some(reply) = self.active_reply.take() else {
            return;
        };
        if reply.number != guard.number {
            self.active_reply = Some(reply);
            return;
        }
        if !succeeded {
            let message = reply
                .lines
                .iter()
                .map(|line| String::from_utf8_lossy(line))
                .collect::<Vec<_>>()
                .join("\n")
                .chars()
                .take(256)
                .collect();
            self.display.set_error(Some(message));
        }

        match reply.query {
            BootstrapQuery::Ignore => {
                if succeeded {
                    self.display.set_error(None);
                }
            }
            BootstrapQuery::Configure => {}
            BootstrapQuery::Version => {
                let version = reply.lines.first().map(Vec::as_slice).unwrap_or_default();
                self.route_keys_through_client = tmux_version_at_least(version, 3, 4);
                self.report_client_colors = tmux_version_at_least(version, 3, 5);
                self.display.set_key_routing(self.route_keys_through_client);
            }
            BootstrapQuery::Sessions => {
                if succeeded {
                    self.sessions.clear();
                    for line in &reply.lines {
                        self.apply_session_record(line);
                    }
                }
                self.publish_metadata();
            }
            BootstrapQuery::Windows => {
                if succeeded {
                    self.windows.clear();
                    for line in &reply.lines {
                        self.apply_window_record(line);
                    }
                }
                self.publish_metadata();
            }
            BootstrapQuery::Panes => {
                if succeeded {
                    for line in &reply.lines {
                        self.apply_pane_record(line);
                    }
                }
                let panes = self.panes.keys().copied().collect::<Vec<_>>();
                for pane in panes {
                    self.pending_queries
                        .push_back(BootstrapQuery::Capture(pane));
                    outcome
                        .commands
                        .push(format!("capture-pane -p -e -t {pane}\n").into_bytes());
                }
                self.pending_queries.push_back(BootstrapQuery::Cursors);
                outcome
                    .commands
                    .push(b"list-panes -s -F '#{pane_id} #{cursor_x} #{cursor_y}'\n".to_vec());
                self.pending_queries.push_back(BootstrapQuery::ActiveTarget);
                outcome
                    .commands
                    .push(b"display-message -p '#{session_id} #{window_id} #{pane_id}'\n".to_vec());
            }
            BootstrapQuery::Capture(pane) => {
                if succeeded {
                    self.apply_capture(pane, &reply.lines);
                    self.refresh_display();
                }
            }
            BootstrapQuery::Cursors => {
                if succeeded {
                    for line in &reply.lines {
                        self.apply_cursor_record(line);
                    }
                }
            }
            BootstrapQuery::PaneMode(pane) => {
                if succeeded {
                    let in_mode = reply.lines.first().is_some_and(|line| line == b"1");
                    self.pane_modes.insert(pane, in_mode);
                    self.publish_metadata();
                    self.pending_queries
                        .push_back(BootstrapQuery::Capture(pane));
                    outcome
                        .commands
                        .push(format!("capture-pane -p -e -t {pane}\n").into_bytes());
                }
            }
            BootstrapQuery::ActiveTarget => {
                if succeeded && let Some(line) = reply.lines.first() {
                    self.apply_active_record(line);
                }
                if self.active_pane.is_none() {
                    self.active_pane = self.panes.keys().copied().min_by_key(|pane| pane.0);
                }
                self.refresh_display();
                self.display.set_error(None);
                // Pause mode is enabled only after bootstrap so its command
                // response cannot be mistaken for one of the startup queries.
                outcome.commands.push(
                    format!("refresh-client -f pause-after={CONTROL_PAUSE_AFTER_SECONDS}\n")
                        .into_bytes(),
                );
            }
        }
        outcome.changed = true;
    }

    fn handle_notification(
        &mut self,
        notification: Notification<'_>,
        record_output: bool,
        emit: &mut impl FnMut(TerminalEvent),
        outcome: &mut TmuxAdvance,
    ) {
        match notification {
            Notification::Output { pane, bytes }
            | Notification::ExtendedOutput { pane, bytes, .. } => {
                self.feed_pane(pane, bytes.as_ref(), record_output, emit, outcome);
                outcome.changed = true;
            }
            Notification::Pause(pane) => {
                self.pending_queries
                    .push_back(BootstrapQuery::Capture(pane));
                self.pending_queries.push_back(BootstrapQuery::Ignore);
                self.pending_queries.push_back(BootstrapQuery::Cursors);
                // tmux discards output while a pane is paused. Capture its
                // current grid before continuing so no visible state is lost.
                outcome
                    .commands
                    .push(format!("capture-pane -p -e -t {pane}\n").into_bytes());
                outcome
                    .commands
                    .push(format!("refresh-client -A {pane}:continue\n").into_bytes());
                outcome
                    .commands
                    .push(b"list-panes -s -F '#{pane_id} #{cursor_x} #{cursor_y}'\n".to_vec());
            }
            Notification::PaneModeChanged(pane) => {
                self.pending_queries
                    .push_back(BootstrapQuery::PaneMode(pane));
                outcome.commands.push(
                    format!("display-message -p -t {pane} '#{{pane_in_mode}}'\n").into_bytes(),
                );
            }
            Notification::LayoutChanged {
                window,
                layout,
                visible_layout,
                flags,
            } => {
                let selected_layout = visible_layout
                    .filter(|layout| !layout.is_empty())
                    .unwrap_or(layout);
                if let Ok(layout) = Layout::parse(selected_layout) {
                    let pane_ids = layout.panes().map(|(pane, _)| pane).collect::<Vec<_>>();
                    self.panes.retain(|pane, state| {
                        state.window != Some(window) || pane_ids.contains(pane)
                    });
                    for (pane, cell) in layout.panes() {
                        self.ensure_pane(pane, cell.width as usize, cell.height as usize)
                            .window = Some(window);
                    }
                    self.window_layouts.insert(window, layout);
                }
                let current_window = flags.is_some_and(|flags| flags.contains(&b'*'))
                    || self.active_window == Some(window);
                if let Some(window_state) = self.windows.get_mut(&window)
                    && let Some(flags) = flags
                {
                    window_state.flags = String::from_utf8_lossy(flags).into_owned();
                }
                if current_window {
                    self.active_window = Some(window);
                    if let Some(pane) = self.window_active_panes.get(&window).copied() {
                        self.active_pane = Some(pane);
                    }
                    self.refresh_display();
                }
                outcome.changed = true;
            }
            Notification::WindowPaneChanged { window, pane } => {
                self.window_active_panes.insert(window, pane);
                if self.active_window == Some(window) {
                    self.active_pane = Some(pane);
                    self.refresh_display();
                }
                outcome.changed = true;
            }
            Notification::SessionWindowChanged { session, window } => {
                if self.active_session.is_none() || self.active_session == Some(session) {
                    self.active_session = Some(session);
                    self.active_window = Some(window);
                    self.active_pane = self.window_active_panes.get(&window).copied();
                    self.refresh_display();
                }
                outcome.changed = true;
            }
            Notification::SessionChanged { session, name } => {
                let session_changed = self.active_session != Some(session);
                self.active_session = Some(session);
                self.sessions
                    .insert(session, String::from_utf8_lossy(name).into_owned());
                if session_changed && self.display.is_ready() {
                    self.begin_session_sync(outcome);
                } else {
                    self.publish_metadata();
                }
            }
            Notification::SessionRenamed { session, name } => {
                self.sessions
                    .insert(session, String::from_utf8_lossy(name).into_owned());
                self.publish_metadata();
                outcome.changed = true;
            }
            Notification::SessionsChanged => {
                self.pending_queries.push_back(BootstrapQuery::Sessions);
                outcome
                    .commands
                    .push(b"list-sessions -F '#{session_id} #{session_name}'\n".to_vec());
            }
            Notification::WindowAdded(window) => {
                self.windows.entry(window).or_insert(TmuxWindowInfo {
                    id: window.0,
                    index: window.0,
                    name: window.to_string(),
                    active: false,
                    flags: String::new(),
                });
                self.pending_queries.push_back(BootstrapQuery::Windows);
                outcome.commands.push(
                    b"list-windows -F '#{window_id} #{window_index} #{window_active} #{window_flags} #{window_name}'\n"
                        .to_vec(),
                );
                self.publish_metadata();
                outcome.changed = true;
            }
            Notification::WindowRenamed { window, name } => {
                if let Some(window) = self.windows.get_mut(&window) {
                    window.name = String::from_utf8_lossy(name).into_owned();
                }
                self.publish_metadata();
                outcome.changed = true;
            }
            Notification::WindowClosed(window) => {
                self.window_active_panes.remove(&window);
                self.window_layouts.remove(&window);
                self.windows.remove(&window);
                self.panes.retain(|_, pane| pane.window != Some(window));
                if self.active_window == Some(window) {
                    self.active_window = None;
                    self.active_pane = None;
                }
                self.refresh_display();
                outcome.changed = true;
            }
            Notification::ConfigError { message } => {
                self.display
                    .set_error(Some(String::from_utf8_lossy(message).into_owned()));
                outcome.changed = true;
            }
            Notification::Message { message } => {
                self.display
                    .set_message(String::from_utf8_lossy(message).into_owned());
                outcome.changed = true;
            }
            Notification::Exit { reason } => {
                self.display
                    .set_error(reason.map(|reason| String::from_utf8_lossy(reason).into_owned()));
            }
            _ => {}
        }
    }

    fn feed_pane(
        &mut self,
        pane: PaneId,
        bytes: &[u8],
        record_output: bool,
        emit: &mut impl FnMut(TerminalEvent),
        outcome: &mut TmuxAdvance,
    ) {
        let active = self.display.pane() == Some(pane);
        let configured_size = self.size;
        let graphics_commands = {
            let pane_state = self.ensure_pane_preserving_layout(pane);
            let pane_size = {
                let term = pane_state.term.lock();
                TerminalSize {
                    cols: term.columns(),
                    rows: term.screen_lines(),
                    cell_width: configured_size.cell_width,
                    cell_height: configured_size.cell_height,
                }
            };
            let mut term = pane_state.term.lock();
            let cursor = Cell::new(graphics_cursor_from_term(&term, pane_size));
            let mut graphics = pane_state
                .graphics
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let parser = &mut pane_state.parser;
            let decoder = &mut pane_state.decoder;
            let shell_integration = &mut pane_state.shell_integration;
            let graphics_alt_screen_active = &mut pane_state.graphics_alt_screen_active;
            let mut graphics_commands = Vec::new();
            pane_state.graphics_ingress.advance_ordered(
                bytes,
                |segment| match segment {
                    TerminalGraphicsSegment::Terminal(terminal_bytes) => {
                        let decoded = decoder.decode_to_utf8_bytes(&terminal_bytes);
                        if active && record_output {
                            let (_, recordable) = shell_integration.advance_with_recording(
                                parser,
                                &mut *term,
                                decoded.as_ref(),
                                &mut *emit,
                            );
                            if !recordable.is_empty() {
                                emit(TerminalEvent::Output(recordable));
                            }
                        } else {
                            shell_integration.advance(
                                parser,
                                &mut *term,
                                decoded.as_ref(),
                                |event| {
                                    if active {
                                        emit(event);
                                    }
                                },
                            );
                        }
                        graphics.clear_for_alt_screen_transition(&term, graphics_alt_screen_active);
                        cursor.set(graphics_cursor_from_term(&term, pane_size));
                    }
                    TerminalGraphicsSegment::Event(event) => {
                        if let Some(response) = graphics.handle_event(event) {
                            graphics_commands.extend(pane_reply_commands(pane, &response));
                        }
                    }
                },
                || cursor.get(),
            );
            graphics_commands
        };
        self.pending_queries
            .extend((0..graphics_commands.len()).map(|_| BootstrapQuery::Ignore));
        outcome.commands.extend(graphics_commands);
        self.drain_pane_events(pane, active, outcome);
    }

    fn drain_pane_events(&mut self, pane: PaneId, active: bool, outcome: &mut TmuxAdvance) {
        let report_client_colors = self.report_client_colors;
        let Some(pane_state) = self.panes.get_mut(&pane) else {
            return;
        };
        let mut pane_inputs = Vec::new();
        while let Ok(event) = pane_state.events.try_recv() {
            match event {
                AlacEvent::PtyWrite(text) => {
                    pane_inputs.extend(pane_reply_commands(pane, text.as_bytes()));
                }
                AlacEvent::ColorRequest(index, formatter) => {
                    let override_color = (index <= 268)
                        .then(|| pane_state.term.lock().colors()[index])
                        .flatten();
                    let color =
                        crate::color_for_alacritty_request_with_override(index, override_color);
                    let response = formatter(color);
                    pane_inputs.extend(pane_reply_commands(pane, response.as_bytes()));
                    if report_client_colors
                        && matches!(index, 256 | 257)
                        && let Some(report) = tmux_client_report_command(pane, &response)
                    {
                        pane_inputs.push(report);
                    }
                }
                AlacEvent::TextAreaSizeRequest(formatter) => {
                    let term = pane_state.term.lock();
                    let size = TerminalSize {
                        cols: term.columns(),
                        rows: term.screen_lines(),
                        cell_width: self.size.cell_width,
                        cell_height: self.size.cell_height,
                    };
                    drop(term);
                    pane_inputs.extend(pane_reply_commands(
                        pane,
                        formatter(crate::window_size(size)).as_bytes(),
                    ));
                }
                AlacEvent::ClipboardLoad(_, formatter) if !active => {
                    // Background panes must not read the user's clipboard.
                    pane_inputs.extend(pane_reply_commands(pane, formatter("").as_bytes()));
                }
                AlacEvent::Wakeup | AlacEvent::MouseCursorDirty => {}
                AlacEvent::ChildExit(_) | AlacEvent::Exit => {}
                event if active => self.listener.send_event(event),
                _ => {}
            }
        }
        for command in pane_inputs {
            // Every control command has a response guard, including send-keys.
            // Track it so a device reply cannot shift a pending query response.
            self.pending_queries.push_back(BootstrapQuery::Ignore);
            outcome.commands.push(command);
        }
    }

    fn begin_session_sync(&mut self, outcome: &mut TmuxAdvance) {
        self.panes.clear();
        self.active_window = None;
        self.active_pane = None;
        self.window_active_panes.clear();
        self.window_layouts.clear();
        self.windows.clear();
        self.pane_modes.clear();
        self.pending_queries.clear();
        self.active_reply = None;
        self.display.enter();
        self.display.set_key_routing(self.route_keys_through_client);
        self.pending_queries.push_back(BootstrapQuery::Sessions);
        self.pending_queries.push_back(BootstrapQuery::Windows);
        self.pending_queries.push_back(BootstrapQuery::Panes);
        outcome
            .commands
            .push(b"list-sessions -F '#{session_id} #{session_name}'\n".to_vec());
        outcome.commands.push(
            b"list-windows -F '#{window_id} #{window_index} #{window_active} #{window_flags} #{window_name}'\n"
                .to_vec(),
        );
        outcome.commands.push(
            b"list-panes -s -F '#{pane_id} #{window_id} #{pane_active} #{pane_width} #{pane_height} #{cursor_x} #{cursor_y} #{window_layout}'\n"
                .to_vec(),
        );
        outcome.changed = true;
    }

    fn ensure_pane(&mut self, pane: PaneId, cols: usize, rows: usize) -> &mut TmuxPane {
        let encoding = self.encoding;
        let scrollback_lines = self.scrollback_lines;
        let cell_width = self.size.cell_width;
        let cell_height = self.size.cell_height;
        let graphics_options = self.graphics_options.clone();
        let pane_state = self.panes.entry(pane).or_insert_with(|| {
            let (pane_listener, pane_events) = crate::local_event_channel();
            let size = TerminalSize {
                cols: cols.max(2),
                rows: rows.max(2),
                cell_width,
                cell_height,
            };
            TmuxPane {
                term: Arc::new(FairMutex::new(Term::new(
                    interactive_terminal_config(scrollback_lines),
                    &size,
                    pane_listener.clone(),
                ))),
                listener: pane_listener,
                events: pane_events,
                parser: Processor::new(),
                decoder: TerminalOutputDecoder::new(encoding),
                window: None,
                cursor: (0, 0),
                shell_integration: TerminalShellIntegration::default(),
                graphics_ingress: GraphicsIngress::new(graphics_options),
                graphics: Arc::new(Mutex::new(TerminalGraphicsState::default())),
                graphics_alt_screen_active: false,
            }
        });
        let size = TerminalSize {
            cols: cols.max(2),
            rows: rows.max(2),
            cell_width,
            cell_height,
        };
        let mut term = pane_state.term.lock();
        if term.columns() != size.cols || term.screen_lines() != size.rows {
            term.resize(size);
        }
        drop(term);
        pane_state
    }

    fn ensure_pane_preserving_layout(&mut self, pane: PaneId) -> &mut TmuxPane {
        // Layout notifications are authoritative for existing pane emulators. Only an
        // output notification for an unknown pane may fall back to the client dimensions.
        let (cols, rows) = self
            .panes
            .get(&pane)
            .map(|pane_state| {
                let term = pane_state.term.lock();
                (term.columns(), term.screen_lines())
            })
            .unwrap_or((self.size.cols, self.size.rows));
        self.ensure_pane(pane, cols, rows)
    }

    fn apply_pane_record(&mut self, line: &[u8]) {
        let mut fields = line.split(|byte| *byte == b' ');
        let (
            Some(pane),
            Some(window),
            Some(active),
            Some(width),
            Some(height),
            Some(cursor_x),
            Some(cursor_y),
            Some(layout),
        ) = (
            fields.next().and_then(oxideterm_tmux::PaneId::parse_wire),
            fields.next().and_then(oxideterm_tmux::WindowId::parse_wire),
            fields.next().and_then(parse_u64),
            fields.next().and_then(parse_usize),
            fields.next().and_then(parse_usize),
            fields.next().and_then(parse_usize),
            fields.next().and_then(parse_usize),
            fields.next(),
        )
        else {
            return;
        };
        if fields.next().is_some() {
            return;
        }
        let pane_state = self.ensure_pane(pane, width, height);
        pane_state.window = Some(window);
        pane_state.cursor = (cursor_x, cursor_y);
        if !self.window_layouts.contains_key(&window)
            && let Ok(layout) = Layout::parse(layout)
        {
            self.window_layouts.insert(window, layout);
        }
        if active != 0 {
            self.window_active_panes.insert(window, pane);
        }
    }

    fn apply_session_record(&mut self, line: &[u8]) {
        let (id, name) = split_wire_field(line);
        let Some(id) = oxideterm_tmux::SessionId::parse_wire(id) else {
            return;
        };
        self.sessions
            .insert(id, String::from_utf8_lossy(name).into_owned());
    }

    fn apply_window_record(&mut self, line: &[u8]) {
        let mut fields = line.splitn(5, |byte| *byte == b' ');
        let (Some(id), Some(index), Some(active), Some(flags), Some(name)) = (
            fields.next().and_then(oxideterm_tmux::WindowId::parse_wire),
            fields.next().and_then(parse_u64),
            fields.next().and_then(parse_u64),
            fields.next(),
            fields.next(),
        ) else {
            return;
        };
        self.windows.insert(
            id,
            TmuxWindowInfo {
                id: id.0,
                index,
                name: String::from_utf8_lossy(name).into_owned(),
                active: active != 0,
                flags: String::from_utf8_lossy(flags).into_owned(),
            },
        );
    }

    fn apply_active_record(&mut self, line: &[u8]) {
        let mut fields = line.split(|byte| *byte == b' ');
        let (Some(session), Some(window), Some(pane)) = (
            fields
                .next()
                .and_then(oxideterm_tmux::SessionId::parse_wire),
            fields.next().and_then(oxideterm_tmux::WindowId::parse_wire),
            fields.next().and_then(oxideterm_tmux::PaneId::parse_wire),
        ) else {
            return;
        };
        if fields.next().is_some() {
            return;
        }
        self.active_session = Some(session);
        self.active_window = Some(window);
        self.active_pane = Some(pane);
        self.window_active_panes.insert(window, pane);
        self.ensure_pane_preserving_layout(pane).window = Some(window);
        self.publish_metadata();
    }

    fn apply_cursor_record(&mut self, line: &[u8]) {
        let mut fields = line.split(|byte| *byte == b' ');
        let (Some(pane), Some(cursor_x), Some(cursor_y)) = (
            fields.next().and_then(oxideterm_tmux::PaneId::parse_wire),
            fields.next().and_then(parse_usize),
            fields.next().and_then(parse_usize),
        ) else {
            return;
        };
        if fields.next().is_some() {
            return;
        }
        let Some(pane_state) = self.panes.get_mut(&pane) else {
            return;
        };
        pane_state.cursor = (cursor_x, cursor_y);
        let cursor = format!(
            "\x1b[{};{}H",
            cursor_y.saturating_add(1),
            cursor_x.saturating_add(1)
        );
        pane_state
            .parser
            .advance(&mut *pane_state.term.lock(), cursor.as_bytes());
    }

    fn apply_capture(&mut self, pane: PaneId, lines: &[Vec<u8>]) {
        let Some(pane_state) = self.panes.get_mut(&pane) else {
            return;
        };
        let size = {
            let term = pane_state.term.lock();
            TerminalSize {
                cols: term.columns(),
                rows: term.screen_lines(),
                cell_width: self.size.cell_width,
                cell_height: self.size.cell_height,
            }
        };
        pane_state.term = Arc::new(FairMutex::new(Term::new(
            interactive_terminal_config(self.scrollback_lines),
            &size,
            pane_state.listener.clone(),
        )));
        pane_state.parser = Processor::new();
        pane_state.decoder.reset();
        pane_state.shell_integration = TerminalShellIntegration::default();
        pane_state.graphics_alt_screen_active = false;

        let mut term = pane_state.term.lock();
        for (index, line) in lines.iter().enumerate() {
            pane_state.parser.advance(&mut *term, line);
            if index + 1 < lines.len() {
                pane_state.parser.advance(&mut *term, b"\r\n");
            }
        }
        let cursor = format!(
            "\x1b[{};{}H",
            pane_state.cursor.1.saturating_add(1),
            pane_state.cursor.0.saturating_add(1)
        );
        pane_state.parser.advance(&mut *term, cursor.as_bytes());
    }

    fn refresh_display(&self) {
        let layout = self
            .active_window
            .and_then(|window| self.window_layouts.get(&window))
            .cloned();
        let panes = self
            .panes
            .iter()
            .map(|(pane, state)| {
                (
                    *pane,
                    SharedTmuxPane {
                        term: state.term.clone(),
                        graphics: state.graphics.clone(),
                    },
                )
            })
            .collect();
        self.display.publish_layout(layout, panes);
        self.publish_metadata();
        self.show_active_term();
    }

    fn publish_metadata(&self) {
        let mut sessions = self
            .sessions
            .iter()
            .map(|(id, name)| TmuxSessionInfo {
                id: id.0,
                name: name.clone(),
                active: self.active_session == Some(*id),
            })
            .collect::<Vec<_>>();
        sessions.sort_by_key(|session| session.id);
        let mut windows = self.windows.values().cloned().collect::<Vec<_>>();
        for window in &mut windows {
            window.active = self
                .active_window
                .is_some_and(|active| active.0 == window.id);
        }
        windows.sort_by_key(|window| window.index);
        self.display
            .publish_metadata(sessions, windows, self.pane_modes.clone());
    }

    fn show_active_term(&self) {
        let selected = self.display.pane().filter(|pane| {
            self.panes
                .get(pane)
                .is_some_and(|state| state.window == self.active_window)
        });
        let Some(pane) = selected.or(self.active_pane) else {
            return;
        };
        let Some(term) = self.panes.get(&pane).map(|pane| pane.term.clone()) else {
            return;
        };
        self.display.show(pane, term);
    }
}

fn pane_reply_commands(pane: PaneId, bytes: &[u8]) -> Vec<Vec<u8>> {
    encoded_input_commands(pane, bytes, false)
}

fn tmux_client_report_command(pane: PaneId, response: &str) -> Option<Vec<u8>> {
    let report = quote_tmux_argument(&format!("{pane}:{response}"))?;
    Some(format!("refresh-client -r {report}\n").into_bytes())
}

fn encoded_input_commands(pane: PaneId, bytes: &[u8], through_client: bool) -> Vec<Vec<u8>> {
    bytes
        .chunks(INPUT_BYTES_PER_COMMAND)
        .map(|chunk| {
            let mut command = if through_client {
                // User keys pass through the client key tables; protocol
                // replies bypass them and return literally to the pane.
                format!("send-keys -K -H -t {pane}").into_bytes()
            } else {
                format!("send-keys -H -t {pane}").into_bytes()
            };
            for byte in chunk {
                command.push(b' ');
                command.push(hex_digit(byte >> 4));
                command.push(hex_digit(byte & 0x0f));
            }
            command.push(b'\n');
            command
        })
        .collect()
}

fn draw_layout_borders(layout: &Layout, lines: &mut [crate::TerminalRow], size: TerminalSize) {
    let LayoutKind::Split {
        direction,
        children,
    } = &layout.kind
    else {
        return;
    };
    for child in children.iter().skip(1) {
        match direction {
            SplitDirection::LeftRight => {
                let Some(col) = usize::from(child.cell.x).checked_sub(1) else {
                    continue;
                };
                let start = usize::from(layout.cell.y);
                let end = start
                    .saturating_add(usize::from(layout.cell.height))
                    .min(size.rows);
                for row in start..end {
                    set_border_cell(lines, row, col, '│', size);
                }
            }
            SplitDirection::TopBottom => {
                let Some(row) = usize::from(child.cell.y).checked_sub(1) else {
                    continue;
                };
                let start = usize::from(layout.cell.x);
                let end = start
                    .saturating_add(usize::from(layout.cell.width))
                    .min(size.cols);
                for col in start..end {
                    set_border_cell(lines, row, col, '─', size);
                }
            }
        }
    }
    for child in children {
        draw_layout_borders(child, lines, size);
    }
}

fn separator_at_layout(layout: &Layout, col: usize, row: usize) -> Option<TmuxSeparator> {
    let LayoutKind::Split {
        direction,
        children,
    } = &layout.kind
    else {
        return None;
    };
    if let Some(separator) = children
        .iter()
        .find_map(|child| separator_at_layout(child, col, row))
    {
        return Some(separator);
    }

    for pair in children.windows(2) {
        let before = &pair[0];
        let after = &pair[1];
        let (on_separator, before_point, after_point, separator_direction) = match direction {
            SplitDirection::LeftRight => {
                let boundary = usize::from(before.cell.x) + usize::from(before.cell.width);
                let vertical_start = usize::from(before.cell.y).max(usize::from(after.cell.y));
                let vertical_end = (usize::from(before.cell.y) + usize::from(before.cell.height))
                    .min(usize::from(after.cell.y) + usize::from(after.cell.height));
                (
                    col == boundary && row >= vertical_start && row < vertical_end,
                    (boundary.saturating_sub(1), row),
                    (boundary.saturating_add(1), row),
                    TmuxSeparatorDirection::LeftRight,
                )
            }
            SplitDirection::TopBottom => {
                let boundary = usize::from(before.cell.y) + usize::from(before.cell.height);
                let horizontal_start = usize::from(before.cell.x).max(usize::from(after.cell.x));
                let horizontal_end = (usize::from(before.cell.x) + usize::from(before.cell.width))
                    .min(usize::from(after.cell.x) + usize::from(after.cell.width));
                (
                    row == boundary && col >= horizontal_start && col < horizontal_end,
                    (col, boundary.saturating_sub(1)),
                    (col, boundary.saturating_add(1)),
                    TmuxSeparatorDirection::TopBottom,
                )
            }
        };
        if !on_separator {
            continue;
        }
        let before_pane = before
            .pane_at(before_point.0, before_point.1)
            .or_else(|| before.panes().next().map(|(pane, _)| pane))?;
        let after_pane = after
            .pane_at(after_point.0, after_point.1)
            .or_else(|| after.panes().next().map(|(pane, _)| pane))?;
        return Some(TmuxSeparator {
            direction: separator_direction,
            before_pane,
            after_pane,
        });
    }
    None
}

fn set_border_cell(
    lines: &mut [crate::TerminalRow],
    row: usize,
    col: usize,
    border: char,
    size: TerminalSize,
) {
    if row >= size.rows || col >= size.cols {
        return;
    }
    let Some(cell) = lines
        .get_mut(row)
        .and_then(|line| line.cells_mut().get_mut(col))
    else {
        return;
    };
    cell.ch = match (cell.ch, border) {
        ('│', '─') | ('─', '│') | ('┼', _) => '┼',
        _ => border,
    };
    cell.cursor = false;
}

fn namespaced_image_id(pane: PaneId, image: TerminalImageId) -> TerminalImageId {
    // Graphics protocols scope ids to one terminal. Mix the pane identity into
    // the composite id so equal Kitty or iTerm ids cannot share a render cache entry.
    let mut value = image.0 ^ pane.0.rotate_left(32) ^ 0x9e37_79b9_7f4a_7c15;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    TerminalImageId(value ^ (value >> 31))
}

fn proportional_image_extent(source_extent: u32, visible_cells: usize, total_cells: usize) -> u32 {
    let numerator = u64::from(source_extent).saturating_mul(visible_cells as u64);
    let denominator = total_cells.max(1) as u64;
    numerator.div_ceil(denominator).min(u64::from(u32::MAX)) as u32
}

fn quote_tmux_argument(value: &str) -> Option<String> {
    if value.is_empty()
        || value
            .chars()
            .any(|character| matches!(character, '\0' | '\r' | '\n'))
    {
        return None;
    }
    Some(format!("'{}'", value.replace('\'', "'\\''")))
}

fn normalize_tmux_command(command: &str) -> Option<String> {
    let command = command.trim();
    if command.is_empty()
        || command
            .chars()
            .any(|character| matches!(character, '\0' | '\r' | '\n'))
    {
        return None;
    }
    Some(format!("{command}\n"))
}

fn hex_digit(value: u8) -> u8 {
    match value {
        0..=9 => b'0' + value,
        _ => b'a' + value - 10,
    }
}

fn parse_u64(bytes: &[u8]) -> Option<u64> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

fn parse_usize(bytes: &[u8]) -> Option<usize> {
    usize::try_from(parse_u64(bytes)?).ok()
}

fn split_wire_field(bytes: &[u8]) -> (&[u8], &[u8]) {
    bytes
        .iter()
        .position(|byte| *byte == b' ')
        .map_or((bytes, &[]), |index| (&bytes[..index], &bytes[index + 1..]))
}

fn tmux_version_at_least(version: &[u8], required_major: u64, required_minor: u64) -> bool {
    let version = version.strip_prefix(b"next-").unwrap_or(version);
    let (major, remainder) = split_wire_version(version, b'.');
    let major = parse_leading_decimal(major).unwrap_or(0);
    let minor = parse_leading_decimal(remainder).unwrap_or(0);
    (major, minor) >= (required_major, required_minor)
}

fn split_wire_version(bytes: &[u8], delimiter: u8) -> (&[u8], &[u8]) {
    bytes
        .iter()
        .position(|byte| *byte == delimiter)
        .map_or((bytes, &[]), |index| (&bytes[..index], &bytes[index + 1..]))
}

fn parse_leading_decimal(bytes: &[u8]) -> Option<u64> {
    let length = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    parse_u64(&bytes[..length])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_sized_panes_survive_active_sync_and_output() {
        let (listener, _) = crate::local_event_channel();
        let display = Arc::new(TmuxDisplay::default());
        let size = TerminalSize {
            cols: 80,
            rows: 24,
            cell_width: 0,
            cell_height: 0,
        };
        let mut controller = TmuxController::new(
            display,
            listener,
            size,
            TerminalEncoding::Utf8,
            100,
            GraphicsOptions::default(),
        );
        let upper_pane = PaneId(1);
        let left_pane = PaneId(2);
        controller.ensure_pane(upper_pane, 80, 11).window = Some(WindowId(1));
        controller.ensure_pane(left_pane, 39, 24).window = Some(WindowId(2));

        controller.apply_active_record(b"$1 @1 %1");
        let mut outcome = TmuxAdvance::new();
        controller.feed_pane(
            upper_pane,
            b"vertical output",
            false,
            &mut |_| {},
            &mut outcome,
        );
        controller.feed_pane(
            left_pane,
            b"horizontal output",
            false,
            &mut |_| {},
            &mut outcome,
        );

        let upper_term = controller.panes[&upper_pane].term.lock();
        assert_eq!((upper_term.columns(), upper_term.screen_lines()), (80, 11));
        drop(upper_term);
        let left_term = controller.panes[&left_pane].term.lock();
        assert_eq!((left_term.columns(), left_term.screen_lines()), (39, 24));
    }

    #[test]
    fn control_mode_takes_over_the_existing_stream_and_restores_it_on_exit() {
        let (listener, _) = crate::local_event_channel();
        let display = Arc::new(TmuxDisplay::default());
        let size = TerminalSize {
            cols: 80,
            rows: 24,
            cell_width: 0,
            cell_height: 0,
        };
        let mut controller = TmuxController::new(
            display.clone(),
            listener,
            size,
            TerminalEncoding::Utf8,
            100,
            GraphicsOptions::default(),
        );
        let mut ordinary = Vec::new();

        let entered = controller
            .advance(
                b"prompt\x1bP1000p",
                |bytes| ordinary.extend_from_slice(bytes),
                false,
                |_| {},
            )
            .unwrap();
        assert_eq!(ordinary, b"prompt");
        assert!(entered.entered);
        assert_eq!(entered.commands.len(), 5);

        controller
            .advance(b"%begin 1 1 1\n%end 1 1 1\n", |_| {}, false, |_| {})
            .unwrap();
        controller
            .advance(b"%begin 1 2 1\n3.4\n%end 1 2 1\n", |_| {}, false, |_| {})
            .unwrap();
        controller
            .advance(
                b"%begin 1 3 1\n$1 demo\n%end 1 3 1\n",
                |_| {},
                false,
                |_| {},
            )
            .unwrap();
        controller
            .advance(
                b"%begin 1 4 1\n@1 0 1 * shell\n%end 1 4 1\n",
                |_| {},
                false,
                |_| {},
            )
            .unwrap();
        let bootstrap = controller
            .advance(
                b"%begin 1 5 1\n%1 @1 1 80 24 5 2 80x24,0,0,1\n%end 1 5 1\n",
                |_| {},
                false,
                |_| {},
            )
            .unwrap();
        assert_eq!(bootstrap.commands.len(), 3);
        controller
            .advance(
                b"%begin 1 6 1\nready\n%end 1 6 1\n%begin 1 7 1\n%1 5 2\n%end 1 7 1\n%begin 1 8 1\n$1 @1 %1\n%end 1 8 1\n%output %1 \\015next\n",
                |_| {},
                false,
                |_| {},
            )
            .unwrap();

        let snapshot = display.snapshot(size, None).expect("composed tmux layout");
        let text = snapshot
            .lines
            .iter()
            .flat_map(|line| line.cells.iter().map(|cell| cell.ch))
            .collect::<String>();
        assert!(text.contains("ready"));
        assert!(text.contains("next"));

        controller
            .advance(
                b"%exit\n\x1b\\shell",
                |bytes| ordinary.extend_from_slice(bytes),
                false,
                |_| {},
            )
            .unwrap();
        assert!(!display.is_active());
        assert!(ordinary.ends_with(b"shell"));
    }
}
