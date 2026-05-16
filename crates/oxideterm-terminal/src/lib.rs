use std::{borrow::Cow, collections::HashMap, env, sync::Arc, thread::JoinHandle};

use alacritty_terminal::{
    event::{Event as AlacEvent, EventListener, Notify, OnResize, WindowSize},
    event_loop::{EventLoop, Msg, Notifier, State},
    grid::{Dimensions, Scroll},
    index::Line,
    sync::FairMutex,
    term::{
        Config, Term,
        cell::{Cell, Flags},
    },
    tty::{self, Shell},
};
use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender, unbounded};

mod color;
mod data;
mod process;
mod search;

pub use alacritty_terminal::term::TermMode;
pub use data::{
    TerminalAttrs, TerminalCell, TerminalColor, TerminalCursorShape, TerminalRow,
    TerminalSearchMatch, TerminalSearchRange, TerminalSnapshot,
};
pub use process::{TerminalLifecycle, TerminalProcessInfo};

use color::{
    OXIDETERM_DARK_THEME, attrs_from_flags, color_for_alacritty_request_with_override,
    style_colors_for_cell,
};
use process::{ProcessState, TerminalSignal, signal_process_group};
use search::{append_grid_line_text, search_logical_line_matches, viewport_row_for_grid_line};

#[derive(Clone)]
struct LocalEventListener {
    tx: Sender<AlacEvent>,
}

impl EventListener for LocalEventListener {
    fn send_event(&self, event: AlacEvent) {
        let _ = self.tx.send(event);
    }
}

#[derive(Clone)]
pub enum TerminalEvent {
    TitleChanged(String),
    TitleReset,
    Bell,
    Wakeup,
    BlinkChanged(bool),
    ChildExited(Option<i32>),
    ClipboardStore(String),
    ClipboardLoad(Arc<dyn Fn(&str) -> String + Sync + Send + 'static>),
}

#[derive(Clone, Copy, Debug)]
struct TerminalSize {
    cols: usize,
    rows: usize,
    cell_width: u16,
    cell_height: u16,
}

impl Dimensions for TerminalSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

fn window_size(size: TerminalSize) -> WindowSize {
    WindowSize {
        num_lines: size.rows as u16,
        num_cols: size.cols as u16,
        cell_width: size.cell_width,
        cell_height: size.cell_height,
    }
}

fn oxideterm_terminal_env() -> HashMap<String, String> {
    HashMap::from([
        ("OXIDETERM_TERM".to_string(), "true".to_string()),
        ("TERM".to_string(), "xterm-256color".to_string()),
        ("TERM_PROGRAM".to_string(), "oxideterm".to_string()),
        (
            "TERM_PROGRAM_VERSION".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        ),
        ("COLORTERM".to_string(), "truecolor".to_string()),
    ])
}

fn focus_report_sequence(enabled: bool, focused: bool) -> Option<&'static [u8]> {
    enabled.then_some(if focused { b"\x1b[I" } else { b"\x1b[O" })
}

pub struct LocalTerminal {
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    notifier: Notifier,
    event_rx: Receiver<AlacEvent>,
    pending_events: Vec<TerminalEvent>,
    io_thread: Option<JoinHandle<(EventLoop<tty::Pty, LocalEventListener>, State)>>,
    size: TerminalSize,
    lifecycle: TerminalLifecycle,
    process: ProcessState,
}

impl LocalTerminal {
    pub fn spawn_default(cols: usize, rows: usize) -> Result<Self> {
        let size = TerminalSize {
            cols: cols.max(2),
            rows: rows.max(2),
            cell_width: 0,
            cell_height: 0,
        };

        let shell = env::var("SHELL").unwrap_or_else(|_| {
            if cfg!(windows) {
                "powershell.exe".to_string()
            } else {
                "/bin/zsh".to_string()
            }
        });

        let (event_tx, event_rx) = unbounded();

        let mut config = Config::default();
        config.scrolling_history = 10000;
        config.kitty_keyboard = true;

        let listener = LocalEventListener {
            tx: event_tx.clone(),
        };
        let term = Arc::new(FairMutex::new(Term::new(config, &size, listener.clone())));
        let cwd = env::current_dir().ok();
        let pty = tty::new(
            &tty::Options {
                shell: Some(Shell::new(shell, Vec::new())),
                working_directory: cwd.clone(),
                drain_on_exit: true,
                env: oxideterm_terminal_env(),
                #[cfg(target_os = "windows")]
                escape_args: false,
            },
            window_size(size),
            0,
        )
        .context("failed to spawn local shell PTY")?;
        let shell_pid = Some(pty.child().id());
        let pty_master = pty.file().try_clone().ok();
        let process = ProcessState::new(shell_pid, pty_master, cwd);
        let event_loop = EventLoop::new(term.clone(), listener, pty, true, false)
            .context("failed to create terminal event loop")?;
        let pty_tx = event_loop.channel();
        let notifier = Notifier(pty_tx);
        let io_thread = event_loop.spawn();

        Ok(Self {
            term,
            notifier,
            event_rx,
            pending_events: Vec::new(),
            io_thread: Some(io_thread),
            size,
            lifecycle: TerminalLifecycle::Running,
            process,
        })
    }

    pub fn drain_output(&mut self) -> bool {
        let mut changed = false;
        while let Ok(event) = self.event_rx.try_recv() {
            if self.handle_alacritty_event(event) {
                changed = true;
            }
        }
        changed
    }

    pub fn take_events(&mut self) -> Vec<TerminalEvent> {
        std::mem::take(&mut self.pending_events)
    }

    pub fn write_input(&mut self, bytes: &[u8]) -> Result<()> {
        if self.lifecycle.is_running() && !bytes.is_empty() {
            self.notifier.notify(Cow::Owned(bytes.to_vec()));
        }
        Ok(())
    }

    pub fn lifecycle(&self) -> TerminalLifecycle {
        self.lifecycle.clone()
    }

    pub fn process_info(&self) -> TerminalProcessInfo {
        self.process.info.clone()
    }

    pub fn refresh_process_info(&mut self) {
        if self.lifecycle.is_running() {
            self.process.refresh();
        }
    }

    pub fn terminate_active_task(&mut self) -> Result<()> {
        self.signal_active_task(TerminalSignal::Terminate)
    }

    pub fn kill_active_task(&mut self) -> Result<()> {
        self.signal_active_task(TerminalSignal::Kill)
    }

    fn signal_active_task(&mut self, signal: TerminalSignal) -> Result<()> {
        self.refresh_process_info();
        let foreground_group = self.process.info.foreground_process_group_id;
        let shell_pid = self.process.info.shell_pid;
        if foreground_group.is_none() || foreground_group == shell_pid {
            anyhow::bail!("no foreground terminal task is active");
        }

        signal_process_group(foreground_group, signal)
    }

    pub fn paste_text(&mut self, text: &str) -> Result<()> {
        let paste_text = if self.mode().contains(TermMode::BRACKETED_PASTE) {
            format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', ""))
        } else {
            normalize_paste_line_endings(text)
        };

        self.write_input(paste_text.as_bytes())
    }

    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    pub fn set_focused(&mut self, focused: bool) -> Result<()> {
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

    fn handle_alacritty_event(&mut self, event: AlacEvent) -> bool {
        match event {
            AlacEvent::Title(title) => {
                self.pending_events.push(TerminalEvent::TitleChanged(title));
                false
            }
            AlacEvent::ResetTitle => {
                self.pending_events.push(TerminalEvent::TitleReset);
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
            AlacEvent::ColorRequest(index, formatter) => {
                let override_color = (index <= 268)
                    .then(|| self.term.lock().colors()[index])
                    .flatten();
                let color = color_for_alacritty_request_with_override(index, override_color);
                let _ = self.write_input(formatter(color).as_bytes());
                false
            }
            AlacEvent::TextAreaSizeRequest(formatter) => {
                let response = formatter(window_size(self.size));
                let _ = self.write_input(response.as_bytes());
                false
            }
            AlacEvent::ChildExit(status) => {
                let code = status.code();
                self.lifecycle = TerminalLifecycle::Exited(code);
                self.process.mark_exited();
                self.pending_events.push(TerminalEvent::ChildExited(code));
                self.join_io_thread();
                true
            }
            AlacEvent::Exit => false,
        }
    }

    pub fn shutdown(&mut self) {
        if matches!(self.lifecycle, TerminalLifecycle::Closed) {
            return;
        }

        if self.lifecycle.is_running() {
            let _ = self.notifier.0.send(Msg::Shutdown);
        }

        self.lifecycle = TerminalLifecycle::Closed;
        self.process.mark_exited();
        self.join_io_thread();
    }

    fn join_io_thread(&mut self) {
        if let Some(io_thread) = self.io_thread.take() {
            if let Err(error) = io_thread.join() {
                tracing::debug!(
                    ?error,
                    "terminal event loop thread panicked during shutdown"
                );
            }
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) -> Result<()> {
        self.resize_with_cell_size(cols, rows, self.size.cell_width, self.size.cell_height)
    }

    pub fn resize_with_cell_size(
        &mut self,
        cols: usize,
        rows: usize,
        cell_width: u16,
        cell_height: u16,
    ) -> Result<()> {
        let next = TerminalSize {
            cols: cols.max(2),
            rows: rows.max(2),
            cell_width,
            cell_height,
        };

        if next.cols == self.size.cols
            && next.rows == self.size.rows
            && next.cell_width == self.size.cell_width
            && next.cell_height == self.size.cell_height
        {
            return Ok(());
        }

        if next.cols != self.size.cols || next.rows != self.size.rows {
            self.term.lock().resize(next);
        }
        self.notifier.on_resize(window_size(next));
        self.size = next;
        Ok(())
    }

    pub fn scroll_lines(&mut self, delta: i32) {
        if delta != 0 {
            self.term.lock().scroll_display(Scroll::Delta(delta));
        }
    }

    pub fn page_up(&mut self) {
        self.term.lock().scroll_display(Scroll::PageUp);
    }

    pub fn page_down(&mut self) {
        self.term.lock().scroll_display(Scroll::PageDown);
    }

    pub fn scroll_to_top(&mut self) {
        self.term.lock().scroll_display(Scroll::Top);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.term.lock().scroll_display(Scroll::Bottom);
    }

    pub fn scroll_to_display_offset(&mut self, offset: usize) {
        let mut term = self.term.lock();
        let max_offset = term.total_lines().saturating_sub(term.screen_lines());
        let target = offset.min(max_offset);
        let current = term.grid().display_offset();
        let delta = target as i32 - current as i32;
        if delta != 0 {
            term.scroll_display(Scroll::Delta(delta));
        }
    }

    pub fn search_matches(&self, query: &str) -> Vec<TerminalSearchMatch> {
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
            let row = &grid[Line(line)];
            append_grid_line_text(
                row[..].iter(),
                line,
                self.size.cols,
                &mut logical_text,
                &mut logical_map,
            );

            let wrapped = row
                .last()
                .is_some_and(|cell| cell.flags.contains(Flags::WRAPLINE));
            if wrapped && line + 1 < bottom_line {
                continue;
            }

            matches.extend(search_logical_line_matches(
                &logical_text,
                &logical_map,
                query,
                self.size.cols,
            ));
            logical_text.clear();
            logical_map.clear();
        }

        matches
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        let term = self.term.lock();
        snapshot_from_term(&term, self.size)
    }
}

fn normalize_paste_line_endings(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                normalized.push('\r');
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
            }
            '\n' => normalized.push('\r'),
            _ => normalized.push(ch),
        }
    }
    normalized
}

fn snapshot_from_term<T: EventListener>(term: &Term<T>, size: TerminalSize) -> TerminalSnapshot {
    let content = term.renderable_content();
    let scrollback_lines = term.total_lines().saturating_sub(term.screen_lines());
    let mut rows = vec![
        TerminalRow {
            wrapped: false,
            active_input: false,
            cells: vec![
                TerminalCell {
                    ch: ' ',
                    zerowidth: String::new(),
                    wide: false,
                    fg: OXIDETERM_DARK_THEME.foreground,
                    bg: OXIDETERM_DARK_THEME.ansi_background,
                    attrs: TerminalAttrs::default(),
                    hyperlink: None,
                    cursor: false,
                };
                size.cols
            ],
        };
        size.rows
    ];

    for indexed in content.display_iter {
        let Some(row) = viewport_row_for_grid_line(indexed.point.line.0, content.display_offset)
        else {
            continue;
        };

        let col = indexed.point.column.0;
        if row >= size.rows || col >= size.cols {
            continue;
        }

        let cell: &Cell = &indexed.cell;
        if cell.flags.contains(Flags::WRAPLINE) {
            rows[row].wrapped = true;
        }

        if cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }

        let mut ch = cell.c;
        if ch == '\0' {
            ch = ' ';
        }

        let attrs = attrs_from_flags(cell.flags);
        let (fg, bg) = style_colors_for_cell(cell.fg, cell.bg, ch, attrs);

        rows[row].cells[col] = TerminalCell {
            ch,
            zerowidth: cell.zerowidth().into_iter().flatten().copied().collect(),
            wide: cell.flags.contains(Flags::WIDE_CHAR),
            fg,
            bg,
            attrs,
            hyperlink: cell
                .hyperlink()
                .map(|hyperlink| hyperlink.uri().to_string()),
            cursor: false,
        };
    }

    let cursor_row = (content.cursor.point.line.0 + content.display_offset as i32).max(0) as usize;
    let cursor_col = content.cursor.point.column.0;

    if cursor_row < rows.len() && cursor_col < size.cols {
        rows[cursor_row].cells[cursor_col].cursor = true;
        mark_active_input_rows(&mut rows, cursor_row);
    }

    TerminalSnapshot {
        cols: size.cols,
        rows: size.rows,
        cursor_col,
        cursor_row,
        cursor_shape: content.cursor.shape.into(),
        display_offset: content.display_offset,
        scrollback_lines,
        lines: rows,
    }
}

fn mark_active_input_rows(rows: &mut [TerminalRow], cursor_row: usize) {
    let mut start = cursor_row;
    while start > 0 && rows.get(start - 1).is_some_and(|row| row.wrapped) {
        start -= 1;
    }

    let mut end = cursor_row + 1;
    while end < rows.len() && rows.get(end - 1).is_some_and(|row| row.wrapped) {
        end += 1;
    }

    for row in &mut rows[start..end] {
        row.active_input = true;
    }
}

impl Drop for LocalTerminal {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn control_code_for_ascii(ch: char) -> Option<u8> {
    let lower = ch.to_ascii_lowercase();
    if lower.is_ascii_lowercase() {
        Some((lower as u8) & 0x1f)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use alacritty_terminal::{
        event::VoidListener,
        term::Config,
        vte::ansi::{Color, NamedColor, Processor, Rgb, StdSyncHandler},
    };

    use crate::{
        color::{
            DEFAULT_MINIMUM_CONTRAST_SCORE, OXIDETERM_DARK_THEME,
            color_for_alacritty_request_with_override, indexed_color_to_rgb,
            perceptual_contrast_score, style_colors_for_cell,
        },
        process::{parse_lsof_cwd, parse_process_table_for_group},
        search::search_line_matches,
    };

    #[test]
    fn focus_reports_are_gated_by_terminal_mode() {
        assert_eq!(focus_report_sequence(false, true), None);
        assert_eq!(focus_report_sequence(false, false), None);
        assert_eq!(
            focus_report_sequence(true, true),
            Some(b"\x1b[I".as_slice())
        );
        assert_eq!(
            focus_report_sequence(true, false),
            Some(b"\x1b[O".as_slice())
        );
    }

    #[test]
    fn lifecycle_reports_running_state() {
        assert!(TerminalLifecycle::Running.is_running());
        assert!(!TerminalLifecycle::Exited(Some(0)).is_running());
        assert!(!TerminalLifecycle::Closed.is_running());
    }

    #[test]
    fn process_group_parser_ignores_zombies_and_picks_latest_pid() {
        let ps_output = "\
          100   42 S\n\
          101   42 Z\n\
          205   99 S\n\
          103   42 S+\n";

        assert_eq!(parse_process_table_for_group(ps_output, 42), Some(103));
        assert_eq!(parse_process_table_for_group(ps_output, 123), None);
    }

    #[test]
    fn lsof_cwd_parser_reads_name_record() {
        let lsof_output = "p12345\nn/Users/dominical/Documents/OxideTerm\n";
        assert_eq!(
            parse_lsof_cwd(lsof_output),
            Some(PathBuf::from("/Users/dominical/Documents/OxideTerm"))
        );
    }

    #[test]
    fn search_line_matches_reports_terminal_range_columns() {
        let matches = search_line_matches(-3, "cargo test cargo", "cargo", 80);

        assert_eq!(
            matches,
            vec![
                TerminalSearchMatch {
                    line: -3,
                    start_col: 0,
                    end_col: 5,
                    ranges: vec![TerminalSearchRange {
                        line: -3,
                        start_col: 0,
                        end_col: 5,
                    }],
                },
                TerminalSearchMatch {
                    line: -3,
                    start_col: 11,
                    end_col: 16,
                    ranges: vec![TerminalSearchRange {
                        line: -3,
                        start_col: 11,
                        end_col: 16,
                    }],
                },
            ]
        );
    }

    #[test]
    fn search_line_matches_clips_to_terminal_columns() {
        let matches = search_line_matches(0, "abcde", "cde", 4);

        assert_eq!(
            matches,
            vec![TerminalSearchMatch {
                line: 0,
                start_col: 2,
                end_col: 4,
                ranges: vec![TerminalSearchRange {
                    line: 0,
                    start_col: 2,
                    end_col: 4,
                }],
            }]
        );
    }

    #[test]
    fn logical_search_splits_matches_across_wrapped_rows() {
        let cell_map = vec![(-1, 0), (-1, 1), (-1, 2), (0, 0), (0, 1), (0, 2)];
        let matches = search_logical_line_matches("abcdef", &cell_map, "cde", 80);

        assert_eq!(
            matches,
            vec![TerminalSearchMatch {
                line: -1,
                start_col: 2,
                end_col: 3,
                ranges: vec![
                    TerminalSearchRange {
                        line: -1,
                        start_col: 2,
                        end_col: 3,
                    },
                    TerminalSearchRange {
                        line: 0,
                        start_col: 0,
                        end_col: 2,
                    },
                ],
            }]
        );
    }

    #[test]
    fn scrolled_grid_lines_map_into_viewport_rows() {
        assert_eq!(viewport_row_for_grid_line(-10, 10), Some(0));
        assert_eq!(viewport_row_for_grid_line(-1, 10), Some(9));
        assert_eq!(viewport_row_for_grid_line(0, 10), Some(10));
        assert_eq!(viewport_row_for_grid_line(-11, 10), None);
    }

    #[test]
    fn snapshot_preserves_soft_wrapped_visual_rows() {
        let size = TerminalSize {
            cols: 10,
            rows: 6,
            cell_width: 8,
            cell_height: 17,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();
        parser.advance(&mut term, b"012345678901234567890123456789X");

        let snapshot = snapshot_from_term(&term, size);
        let row_text = |row: usize| -> String {
            snapshot.lines[row]
                .cells
                .iter()
                .map(|cell| cell.ch)
                .collect::<String>()
        };

        assert_eq!(row_text(0), "0123456789");
        assert_eq!(row_text(1), "0123456789");
        assert_eq!(row_text(2), "0123456789");
        assert_eq!(&row_text(3)[..1], "X");
        assert!(snapshot.lines[0].wrapped);
        assert!(snapshot.lines[1].wrapped);
        assert!(snapshot.lines[2].wrapped);
        assert!(!snapshot.lines[3].wrapped);
        assert!(snapshot.lines[0].active_input);
        assert!(snapshot.lines[1].active_input);
        assert!(snapshot.lines[2].active_input);
        assert!(snapshot.lines[3].active_input);
    }

    #[test]
    fn color_request_uses_oxideterm_terminal_palette_indices() {
        let dim_background = color_for_alacritty_request_with_override(268, None);
        assert_eq!(dim_background.r, OXIDETERM_DARK_THEME.ansi[0].r);
        assert_eq!(dim_background.g, OXIDETERM_DARK_THEME.ansi[0].g);
        assert_eq!(dim_background.b, OXIDETERM_DARK_THEME.ansi[0].b);

        let out_of_range = color_for_alacritty_request_with_override(999, None);
        assert_eq!((out_of_range.r, out_of_range.g, out_of_range.b), (0, 0, 0));
    }

    #[test]
    fn window_size_preserves_physical_cell_dimensions() {
        let size = TerminalSize {
            cols: 97,
            rows: 42,
            cell_width: 16,
            cell_height: 34,
        };

        let window = window_size(size);
        assert_eq!(window.num_cols, 97);
        assert_eq!(window.num_lines, 42);
        assert_eq!(window.cell_width, 16);
        assert_eq!(window.cell_height, 34);
    }

    #[test]
    fn color_request_prefers_alacritty_runtime_overrides() {
        let override_color = Rgb {
            r: 12,
            g: 34,
            b: 56,
        };

        let color = color_for_alacritty_request_with_override(4, Some(override_color));
        assert_eq!((color.r, color.g, color.b), (12, 34, 56));
    }

    #[test]
    fn minimum_contrast_adjusts_theme_defined_ansi_colors() {
        let (fg, bg) = style_colors_for_cell(
            Color::Named(NamedColor::White),
            Color::Indexed(15),
            'x',
            TerminalAttrs::default(),
        );

        assert_ne!(fg, OXIDETERM_DARK_THEME.ansi[7]);
        assert_eq!(bg, OXIDETERM_DARK_THEME.ansi[15]);
        assert!(perceptual_contrast_score(fg, bg).abs() >= DEFAULT_MINIMUM_CONTRAST_SCORE);
    }

    #[test]
    fn app_chosen_truecolor_and_256_colors_bypass_contrast_adjustment() {
        let red_rgb = Rgb { r: 255, g: 0, b: 0 };
        let (truecolor_fg, _) = style_colors_for_cell(
            Color::Spec(red_rgb),
            Color::Named(NamedColor::Background),
            'x',
            TerminalAttrs::default(),
        );
        assert_eq!(truecolor_fg, TerminalColor::rgb(255, 0, 0));

        let (indexed_fg, _) = style_colors_for_cell(
            Color::Indexed(196),
            Color::Named(NamedColor::Background),
            'x',
            TerminalAttrs::default(),
        );
        assert_eq!(indexed_fg, indexed_color_to_rgb(196));
    }

    #[test]
    fn decorative_characters_bypass_contrast_adjustment() {
        let (fg, bg) = style_colors_for_cell(
            Color::Named(NamedColor::White),
            Color::Indexed(15),
            '\u{e0b0}',
            TerminalAttrs::default(),
        );

        assert_eq!(fg, OXIDETERM_DARK_THEME.ansi[7]);
        assert_eq!(bg, OXIDETERM_DARK_THEME.ansi[15]);
    }
}
