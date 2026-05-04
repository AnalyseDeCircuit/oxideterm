use std::{
    ops::Range,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use gpui::{
    Bounds, ClipboardItem, Context, FocusHandle, Pixels, SharedString, Subscription, Timer, Window,
    px,
};
use oxideterm_terminal::{
    SshSessionConfig, TermMode, TerminalEvent, TerminalSession, TerminalSnapshot,
};
use parking_lot::Mutex;

use crate::terminal_ui::*;
use crate::terminal_view::*;

mod ime;
mod interactions;
mod render;
mod scrollbar;

pub(crate) use ime::TerminalInputHandler;
use scrollbar::{ScrollbarDrag, ScrollbarGeometry};

pub struct TerminalPane {
    terminal: Arc<Mutex<TerminalSession>>,
    focus_handle: FocusHandle,
    settings: TerminalUiSettings,
    theme: TerminalUiTheme,
    snapshot: TerminalSnapshot,
    metrics: TerminalMetrics,
    selection: Option<TerminalSelection>,
    marked_text: Option<String>,
    search_query: Option<String>,
    selected_search_match: Option<usize>,
    hovered_link: Option<TerminalLinkRange>,
    selecting: bool,
    last_mouse_report_point: Option<TerminalPoint>,
    title: SharedString,
    bell_flash: bool,
    terminal_exited: bool,
    scroll_px: Pixels,
    scrollbar_drag: Option<ScrollbarDrag>,
    focused: bool,
    cursor_visible: bool,
    cursor_blink_terminal_enabled: bool,
    last_cursor_blink: Instant,
    bounds: Option<Bounds<Pixels>>,
    last_pty_resize: Option<(usize, usize, u16, u16)>,
    pending_pty_resize: Option<(usize, usize, u16, u16)>,
    pty_resize_generation: u64,
    _subscriptions: Vec<Subscription>,
}

const PTY_RESIZE_DEBOUNCE: Duration = Duration::from_millis(50);

impl TerminalPane {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Result<Self> {
        Self::new_with_preferences(TerminalUiPreferences::default(), window, cx)
    }

    pub fn new_with_preferences(
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        let terminal = Arc::new(Mutex::new(TerminalSession::local_default(
            DEFAULT_COLS,
            DEFAULT_ROWS,
        )?));
        Self::from_session(terminal, preferences, window, cx)
    }

    pub fn new_ssh(
        config: SshSessionConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        Self::new_ssh_with_preferences(config, TerminalUiPreferences::default(), window, cx)
    }

    pub fn new_ssh_with_preferences(
        config: SshSessionConfig,
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        let terminal = Arc::new(Mutex::new(TerminalSession::ssh(
            config,
            DEFAULT_COLS,
            DEFAULT_ROWS,
        )));
        Self::from_session(terminal, preferences, window, cx)
    }

    fn from_session(
        terminal: Arc<Mutex<TerminalSession>>,
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        let snapshot = terminal.lock().snapshot();
        let focus_handle = cx.focus_handle();
        let metrics = TerminalMetrics::measure_with_preferences(window, &preferences);
        window.focus(&focus_handle);
        terminal.lock().set_focused(true)?;

        let focus_in = cx.on_focus_in(&focus_handle, window, |this, _window, cx| {
            this.handle_focus_change(true, cx);
        });
        let focus_out = cx.on_focus_out(&focus_handle, window, |this, _event, _window, cx| {
            this.handle_focus_change(false, cx);
        });

        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(Duration::from_millis(16)).await;
                if weak
                    .update(cx, |this, cx| {
                        this.tick(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        Ok(Self {
            terminal,
            focus_handle,
            settings: TerminalUiSettings::from_preferences(&preferences),
            theme: preferences.theme,
            snapshot,
            metrics,
            selection: None,
            marked_text: None,
            search_query: None,
            selected_search_match: None,
            hovered_link: None,
            selecting: false,
            last_mouse_report_point: None,
            title: SharedString::from("OxideTerm"),
            bell_flash: false,
            terminal_exited: false,
            scroll_px: px(0.0),
            scrollbar_drag: None,
            focused: true,
            cursor_visible: true,
            cursor_blink_terminal_enabled: false,
            last_cursor_blink: Instant::now(),
            bounds: None,
            last_pty_resize: None,
            pending_pty_resize: None,
            pty_resize_generation: 0,
            _subscriptions: vec![focus_in, focus_out],
        })
    }

    pub fn title(&self) -> SharedString {
        self.title.clone()
    }

    pub fn focus(&self, window: &mut Window) {
        window.focus(&self.focus_handle);
    }

    pub fn shutdown(&mut self) {
        self.terminal.lock().shutdown();
    }

    pub fn set_search_query(
        &mut self,
        query: Option<String>,
        selected_match: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.search_query = query;
        self.selected_search_match = selected_match;
        if self.search_query.is_some() {
            self.scroll_to_selected_search_match(cx);
        }
        cx.notify();
    }

    pub fn select_next_search_result(&mut self, forward: bool, cx: &mut Context<Self>) {
        self.select_next_search_match(forward, cx);
    }

    pub fn copy_to_clipboard(&mut self, cx: &mut Context<Self>) {
        self.copy_current_selection_or_snapshot(cx);
    }

    pub fn paste_from_clipboard(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text())
            && self.terminal_accepts_input()
            && self.terminal.lock().paste_text(&text).is_ok()
        {
            self.reset_cursor_blink();
            cx.notify();
        }
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        let (changed, events) = {
            let mut terminal = self.terminal.lock();
            terminal.refresh_process_info();
            let changed = terminal.read_pending();
            (changed, terminal.take_events())
        };

        for event in events {
            self.handle_terminal_event(event, cx);
        }

        if changed {
            self.snapshot = self.terminal.lock().snapshot();
            cx.notify();
        }

        self.update_cursor_blink(cx);
    }

    fn handle_terminal_event(&mut self, event: TerminalEvent, cx: &mut Context<Self>) {
        match event {
            TerminalEvent::TitleChanged(title) => {
                self.title = title.into();
                cx.notify();
            }
            TerminalEvent::TitleReset => {
                self.title = SharedString::from("OxideTerm");
                cx.notify();
            }
            TerminalEvent::Bell => {
                self.bell_flash = true;
                cx.notify();
                cx.spawn(async move |weak, cx| {
                    Timer::after(Duration::from_millis(180)).await;
                    let _ = weak.update(cx, |this, cx| {
                        this.bell_flash = false;
                        cx.notify();
                    });
                })
                .detach();
            }
            TerminalEvent::Wakeup => {
                cx.notify();
            }
            TerminalEvent::BlinkChanged(blinking) => {
                self.cursor_blink_terminal_enabled = blinking;
                self.reset_cursor_blink();
                cx.notify();
            }
            TerminalEvent::ChildExited(code) => {
                self.terminal_exited = true;
                self.title = match code {
                    Some(code) => format!("Process exited ({code})").into(),
                    None => "Process exited".into(),
                };
                cx.notify();
            }
            TerminalEvent::ClipboardStore(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            TerminalEvent::ClipboardLoad(formatter) => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    let response = formatter(&text);
                    self.send_bytes(response.as_bytes(), cx);
                }
            }
        }
    }

    fn handle_focus_change(&mut self, focused: bool, cx: &mut Context<Self>) {
        self.focused = focused;
        let _ = self.terminal.lock().set_focused(focused);
        self.reset_cursor_blink();
        cx.notify();
    }

    fn send_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if !self.terminal_accepts_input() {
            return;
        }

        if self.terminal.lock().write_input(bytes).is_ok() {
            self.reset_cursor_blink();
            cx.notify();
        }
    }

    fn terminal_accepts_input(&self) -> bool {
        !self.terminal_exited && self.terminal.lock().lifecycle().is_running()
    }

    fn commit_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.marked_text = None;
        self.send_bytes(text.as_bytes(), cx);
    }

    fn set_marked_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.marked_text = (!text.is_empty()).then(|| text.to_string());
        cx.notify();
    }

    fn clear_marked_text(&mut self, cx: &mut Context<Self>) {
        if self.marked_text.take().is_some() {
            cx.notify();
        }
    }

    fn marked_text_range(&self) -> Option<Range<usize>> {
        self.marked_text
            .as_ref()
            .map(|text| 0..text.encode_utf16().count())
    }

    fn should_blink_cursor(&self) -> bool {
        let alt_screen = self.terminal.lock().mode().contains(TermMode::ALT_SCREEN);
        should_blink_cursor_for_mode(
            self.settings.blink_mode,
            self.focused,
            self.cursor_blink_terminal_enabled,
            alt_screen,
            self.snapshot.cursor_shape,
        )
    }

    fn reset_cursor_blink(&mut self) {
        self.cursor_visible = true;
        self.last_cursor_blink = Instant::now();
    }

    fn update_cursor_blink(&mut self, cx: &mut Context<Self>) {
        if !self.should_blink_cursor() {
            if !self.cursor_visible {
                self.cursor_visible = true;
                cx.notify();
            }
            self.last_cursor_blink = Instant::now();
            return;
        }

        if self.last_cursor_blink.elapsed() >= CURSOR_BLINK_INTERVAL {
            self.cursor_visible = !self.cursor_visible;
            self.last_cursor_blink = Instant::now();
            cx.notify();
        }
    }

    pub fn apply_viewport_bounds(
        &mut self,
        bounds: Bounds<Pixels>,
        scale_factor: f32,
        cx: &mut Context<Self>,
    ) {
        self.bounds = Some(bounds);
        let cell_width = self.metrics.cell_width_f32();
        let line_height = self.metrics.line_height_f32();
        let width =
            (f32::from(bounds.size.width) - TERMINAL_CONTENT_PADDING * 2.0).max(cell_width * 2.0);
        let height =
            (f32::from(bounds.size.height) - TERMINAL_CONTENT_PADDING * 2.0).max(line_height * 2.0);
        let cols = whole_cells_in_span(width, cell_width).max(2);
        let rows = whole_cells_in_span(height, line_height).max(2);
        let cell_width_px = (cell_width * scale_factor).ceil().max(1.0) as u16;
        let cell_height_px = (line_height * scale_factor).ceil().max(1.0) as u16;
        let resize = (cols, rows, cell_width_px, cell_height_px);

        if self.last_pty_resize == Some(resize) || self.pending_pty_resize == Some(resize) {
            return;
        }

        self.pending_pty_resize = Some(resize);
        self.pty_resize_generation = self.pty_resize_generation.wrapping_add(1);
        let generation = self.pty_resize_generation;
        cx.spawn(async move |weak, cx| {
            Timer::after(PTY_RESIZE_DEBOUNCE).await;
            let _ = weak.update(cx, |view, cx| {
                view.flush_pending_pty_resize(generation, cx);
            });
        })
        .detach();
    }

    fn flush_pending_pty_resize(&mut self, generation: u64, cx: &mut Context<Self>) {
        if generation != self.pty_resize_generation {
            return;
        }
        let Some((cols, rows, cell_width_px, cell_height_px)) = self.pending_pty_resize.take()
        else {
            return;
        };
        let resize = (cols, rows, cell_width_px, cell_height_px);
        if self.last_pty_resize == Some(resize) {
            return;
        }

        let resized = {
            let mut terminal = self.terminal.lock();
            terminal
                .resize_with_cell_size(cols, rows, cell_width_px, cell_height_px)
                .is_ok()
        };
        if resized {
            self.last_pty_resize = Some(resize);
            self.snapshot = self.terminal.lock().snapshot();
            cx.notify();
        }
    }

    fn content_origin(&self) -> gpui::Point<Pixels> {
        self.bounds
            .map(|bounds| bounds.origin)
            .unwrap_or_else(|| gpui::point(px(0.0), px(0.0)))
    }
}

fn whole_cells_in_span(span: f32, cell_span: f32) -> usize {
    let cells = span / cell_span;
    let nearest_integer = cells.round();
    if (cells - nearest_integer).abs() <= 0.0001 {
        nearest_integer.max(0.0) as usize
    } else {
        cells.floor().max(0.0) as usize
    }
}
