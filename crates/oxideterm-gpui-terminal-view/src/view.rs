use std::{cell::RefCell, mem, rc::Rc, sync::Arc};

use gpui::{
    App, Bounds, ClipboardItem, Context, EventEmitter, FocusHandle, Focusable, Instant, Pixels,
    Subscription, Task,
};
use oxideterm_settings::{TerminalBackspaceSequence, TerminalDeleteSequence};
use oxideterm_terminal_emulator::{
    GraphicsOptions, TermMode, TerminalEmulator, TerminalEmulatorEvent, TerminalEmulatorEvents,
    TerminalPalette, TerminalSize, TerminalSnapshot,
};
use oxideterm_terminal_encoding::{TerminalEncoding, TerminalInputEncoder};
use zeroize::Zeroize;

use crate::{TerminalLayoutCache, TerminalMetrics, TerminalUiTheme, selection::TerminalSelection};

mod canvas;
mod ime;
mod interactions;
mod preferences;
pub use preferences::TerminalViewPreferences;
mod selection;
mod search;
#[cfg(all(test, not(target_family = "wasm")))]
mod tests;

/// The host owns transport, window titles, alerts, and URL-opening policy.
pub enum TerminalViewEvent {
    /// User input or a protocol reply; write these bytes to the peer without echoing them locally.
    SendBytes(Vec<u8>),
    ReviewPaste(zeroize::Zeroizing<String>),
    ClipboardReadFailed,
    Resized(TerminalSize),
    TitleChanged(Option<String>),
    Bell,
    OpenUrl(String),
}

impl Drop for TerminalViewEvent {
    fn drop(&mut self) {
        // GPUI lends the event to subscribers. Clear its wire payload after delivery.
        match self {
            Self::SendBytes(bytes) => bytes.zeroize(),
            Self::TitleChanged(Some(text)) | Self::OpenUrl(text) => text.zeroize(),
            _ => {}
        }
    }
}

pub struct TerminalView {
    // Copying an offscreen selection temporarily pages the emulator without moving the canvas.
    emulator: RefCell<TerminalEmulator>,
    snapshot: Arc<TerminalSnapshot>,
    size: TerminalSize,
    focus: FocusHandle,
    focused: bool,
    focus_subscriptions: Option<[Subscription; 3]>,
    theme: TerminalUiTheme,
    metrics: TerminalMetrics,
    layout: Rc<RefCell<TerminalLayoutCache>>,
    viewport: Option<(Bounds<Pixels>, f32)>,
    selection: Option<TerminalSelection>,
    search: Option<search::PaneSearch>,
    pointer: interactions::PointerState,
    composition: ime::Composition,
    cursor_visible: bool,
    options: preferences::ViewOptions,
    background_alpha: Option<u8>,
    blink_task: Option<Task<()>>,
    sync_task: Option<(Instant, Task<()>)>,
    alt_sends_escape: bool,
    backspace: TerminalBackspaceSequence,
    delete: TerminalDeleteSequence,
    reported_keys: std::collections::HashSet<String>,
}

impl EventEmitter<TerminalViewEvent> for TerminalView {}

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl TerminalView {
    pub fn new(theme: TerminalUiTheme, metrics: TerminalMetrics, cx: &mut Context<Self>) -> Self {
        // A host can feed output before the first paint supplies measured viewport bounds.
        let size = canvas::grid_size(80, 24, &metrics, 1.0);
        // Do not advertise bitmap protocols to the peer without an image renderer.
        let graphics = GraphicsOptions {
            enabled: false,
            ..Default::default()
        };
        let mut emulator = TerminalEmulator::new(size, graphics);
        emulator.set_palette(theme.palette());
        let snapshot = Arc::new(emulator.snapshot());
        Self {
            emulator: RefCell::new(emulator),
            snapshot,
            size,
            focus: cx.focus_handle(),
            focused: false,
            focus_subscriptions: None,
            theme,
            metrics,
            layout: Default::default(),
            viewport: None,
            selection: None,
            search: None,
            pointer: Default::default(),
            composition: Default::default(),
            cursor_visible: true,
            options: Default::default(),
            background_alpha: None,
            blink_task: None,
            sync_task: None,
            alt_sends_escape: !cx.platform_info().is_mac(),
            backspace: Default::default(),
            delete: Default::default(),
            reported_keys: Default::default(),
        }
    }

    pub fn terminal_size(&self) -> TerminalSize {
        self.size
    }

    pub fn mode(&self) -> TermMode {
        self.emulator.borrow().mode()
    }

    pub fn snapshot(&self) -> &Arc<TerminalSnapshot> {
        &self.snapshot
    }

    pub fn set_theme(&mut self, theme: TerminalUiTheme, cx: &mut Context<Self>) {
        let palette = theme.palette();
        self.theme = theme;
        self.refresh_search_style(cx);
        self.set_palette(palette, cx);
    }

    pub fn set_palette(&mut self, palette: TerminalPalette, cx: &mut Context<Self>) {
        self.emulator.get_mut().set_palette(palette);
        self.publish_snapshot(true, cx);
    }

    pub fn set_background_alpha(&mut self, alpha: Option<u8>, cx: &mut Context<Self>) {
        if self.background_alpha != alpha {
            self.background_alpha = alpha;
            cx.notify();
        }
    }

    pub fn set_metrics(&mut self, metrics: TerminalMetrics, cx: &mut Context<Self>) {
        self.metrics = metrics;
        if let Some((bounds, scale)) = self.viewport {
            self.resize_viewport(bounds, scale, cx);
        } else {
            self.resize_grid(
                canvas::grid_size(self.size.cols, self.size.rows, &self.metrics, 1.0),
                cx,
            );
        }
        cx.notify();
    }

    pub fn set_alt_sends_escape(&mut self, enabled: bool) {
        self.alt_sends_escape = enabled;
    }

    pub fn set_backspace_sequence(&mut self, sequence: TerminalBackspaceSequence) {
        self.backspace = sequence;
    }

    pub fn set_delete_sequence(&mut self, sequence: TerminalDeleteSequence) {
        self.delete = sequence;
    }

    pub fn feed_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        let events = self.emulator.get_mut().feed_bytes(bytes);
        self.route_events(events, cx);
        self.publish_snapshot(false, cx);

        self.refresh_search(cx);

        // A peer can stop writing halfway through a synchronized frame. Its timeout
        // belongs to the parser, not to focus, cursor blink, or the next input packet.
        let deadline = self.emulator.get_mut().sync_deadline();
        if deadline != self.sync_task.as_ref().map(|(at, _)| *at) {
            self.sync_task = deadline.map(|at| {
                let delay = at.saturating_duration_since(Instant::now());
                let task = cx.spawn(async move |view, cx| {
                    cx.background_executor().timer(delay).await;
                    let _ = view.update(cx, |view, cx| {
                        view.sync_task = None;
                        view.feed_bytes(&[], cx);
                    });
                });
                (at, task)
            });
        }
    }

    fn route_events(&mut self, events: TerminalEmulatorEvents, cx: &mut Context<Self>) {
        for event in events {
            match event {
                TerminalEmulatorEvent::PtyWrite(mut bytes) => {
                    cx.emit(TerminalViewEvent::SendBytes(mem::take(&mut *bytes)))
                }
                TerminalEmulatorEvent::Title(mut title) => cx.emit(
                    TerminalViewEvent::TitleChanged(Some(mem::take(&mut *title))),
                ),
                TerminalEmulatorEvent::ResetTitle => cx.emit(TerminalViewEvent::TitleChanged(None)),
                TerminalEmulatorEvent::Bell => cx.emit(TerminalViewEvent::Bell),
                TerminalEmulatorEvent::ClipboardStore(_, mut text) => {
                    if self.options.osc52_copy {
                        cx.write_to_clipboard(ClipboardItem::new_string(mem::take(&mut *text)));
                    }
                }
                // Both are reflected in the post-batch snapshot and cursor state.
                TerminalEmulatorEvent::CursorBlinkingChange | TerminalEmulatorEvent::Wakeup => {}
            }
        }
    }

    fn publish_snapshot(&mut self, full: bool, cx: &mut Context<Self>) {
        // The emulator also rebuilds on full grid damage (including a VT reset).
        let next = if full {
            self.emulator.get_mut().snapshot()
        } else {
            self.emulator.get_mut().incremental_snapshot(&self.snapshot)
        };
        self.snapshot = Arc::new(next.with_generation(self.snapshot.generation.wrapping_add(1)));
        self.sync_selection();
        self.pointer.hovered = None;
        self.update_blink(cx);
        cx.notify();
    }

    pub fn send_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.send_user_bytes(text.as_bytes().to_vec(), cx);
    }

    fn request_paste(&mut self, text: zeroize::Zeroizing<String>, cx: &mut Context<Self>) {
        if self.options.protected_paste && crate::input::paste_needs_confirmation(&text) {
            cx.emit(TerminalViewEvent::ReviewPaste(text));
        } else {
            self.paste_text(&text, cx);
        }
    }

    pub fn paste_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if !text.is_empty() {
            let bracketed = self.mode().contains(TermMode::BRACKETED_PASTE);
            let bytes =
                TerminalInputEncoder::new(TerminalEncoding::Utf8).encode_paste(text, bracketed);
            self.send_user_bytes(bytes, cx);
        }
    }

    fn send_user_bytes(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        if bytes.is_empty() {
            return;
        }
        self.composition = Default::default();
        self.clear_selection(cx);
        if self.snapshot.display_offset != 0 {
            self.scroll_grid(-(self.snapshot.display_offset as i32), cx);
        }
        self.reset_blink(cx);
        cx.emit(TerminalViewEvent::SendBytes(bytes));
        cx.notify();
    }

    fn scroll_grid(&mut self, rows: i32, cx: &mut Context<Self>) {
        let events = self.emulator.get_mut().scroll_display(rows);
        self.route_events(events, cx);
        self.publish_snapshot(false, cx);
    }
}
