use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use alacritty_terminal::{
    event::{Event, EventListener},
    grid::Scroll,
    term::{ClipboardType, Config, MIN_COLUMNS, MIN_SCREEN_LINES, Term},
    vte::ansi::Processor,
};
use oxideterm_terminal_graphics::{GraphicsIngress, TerminalGraphicsSegment};
use web_time::Instant;
use zeroize::Zeroizing;

use crate::{
    GraphicsOptions, TermMode, TerminalGraphicsState, TerminalPalette, TerminalSize,
    TerminalSnapshot,
    convert::{incremental_snapshot_from_term, snapshot_from_term},
    graphics::{apply_graphics_terminal_event, graphics_cursor_from_term},
    palette::color_for_alacritty_request,
    size::window_size,
};

pub enum TerminalEmulatorEvent {
    /// Write these bytes to the same peer that supplied the input.
    PtyWrite(Zeroizing<Vec<u8>>),
    Title(Zeroizing<String>),
    ResetTitle,
    Bell,
    CursorBlinkingChange,
    Wakeup,
    ClipboardStore(ClipboardType, Zeroizing<String>),
}

pub type TerminalEmulatorEvents = Vec<TerminalEmulatorEvent>;

#[derive(Clone, Default)]
struct Listener(Rc<RefCell<Vec<Event>>>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        self.0.borrow_mut().push(event);
    }
}

impl Listener {
    fn drain(
        &self,
        term: &Term<Self>,
        size: TerminalSize,
        palette: &TerminalPalette,
        out: &mut TerminalEmulatorEvents,
    ) {
        use TerminalEmulatorEvent as Output;
        for event in self.0.borrow_mut().drain(..) {
            let reply = |text: String| Output::PtyWrite(Zeroizing::new(text.into_bytes()));
            out.push(match event {
                Event::PtyWrite(text) => reply(text),
                Event::ColorRequest(index, format) => reply(format(color_for_alacritty_request(
                    index,
                    palette,
                    term.colors(),
                ))),
                Event::ColorSchemeRequest => reply(palette.color_scheme_report().to_owned()),
                Event::TextAreaSizeRequest(format) => reply(format(window_size(size))),
                Event::Title(text) => Output::Title(Zeroizing::new(text)),
                Event::ResetTitle => Output::ResetTitle,
                Event::Bell => Output::Bell,
                Event::CursorBlinkingChange => Output::CursorBlinkingChange,
                Event::ClipboardStore(target, text) => {
                    Output::ClipboardStore(target, Zeroizing::new(text))
                }
                Event::Wakeup | Event::MouseCursorDirty => Output::Wakeup,
                // There is no child process, and the grid only accepts clipboard stores.
                Event::Exit | Event::ChildExit(_) | Event::ClipboardLoad(..) => continue,
            });
        }
    }
}

/// One persistent parser and grid per terminal stream. The caller owns I/O and timers.
pub struct TerminalEmulator {
    term: Term<Listener>,
    parser: Processor,
    listener: Listener,
    ingress: GraphicsIngress,
    graphics: TerminalGraphicsState,
    palette: TerminalPalette,
    size: TerminalSize,
}

impl TerminalEmulator {
    pub fn new(size: TerminalSize, graphics_options: GraphicsOptions) -> Self {
        let size = grid_size(size);
        let listener = Listener::default();
        let config = Config {
            kitty_keyboard: true,
            ..Config::default()
        };
        let mut term = Term::new(config, &size, listener.clone());
        let mut ingress = GraphicsIngress::new(graphics_options);
        ingress.attach_grid(term.graphics_events());
        Self {
            term,
            parser: Processor::new(),
            listener,
            ingress,
            graphics: Default::default(),
            palette: Default::default(),
            size,
        }
    }

    /// Feed an empty slice at the sync deadline to publish an unterminated frame.
    #[must_use = "route terminal replies back to the peer"]
    pub fn feed_bytes(&mut self, bytes: &[u8]) -> TerminalEmulatorEvents {
        let mut out = Vec::new();
        let timed_out = self
            .parser
            .sync_timeout()
            .sync_timeout()
            .is_some_and(|at| at <= Instant::now());
        if timed_out {
            self.parser.stop_sync(&mut self.term);
        }
        let redraw = timed_out
            || !bytes.is_empty()
            || self
                .ingress
                .sync_deadline()
                .is_some_and(|at| at <= Instant::now());
        self.listener
            .drain(&self.term, self.size, &self.palette, &mut out);
        let cursor = Cell::new(graphics_cursor_from_term(&self.term, self.size));
        self.ingress.advance_ordered(
            bytes,
            |segment| {
                match segment {
                    TerminalGraphicsSegment::Terminal(bytes) => {
                        self.parser.advance(&mut self.term, &bytes);
                        self.listener
                            .drain(&self.term, self.size, &self.palette, &mut out);
                    }
                    TerminalGraphicsSegment::Event(event) => {
                        apply_graphics_terminal_event(&mut self.term, &event);
                        if let Some(bytes) = self.graphics.handle_event(event) {
                            out.push(TerminalEmulatorEvent::PtyWrite(Zeroizing::new(bytes)));
                        }
                    }
                }
                cursor.set(graphics_cursor_from_term(&self.term, self.size));
            },
            || cursor.get(),
        );
        if redraw
            && !out
                .iter()
                .any(|event| matches!(event, TerminalEmulatorEvent::Wakeup))
        {
            out.push(TerminalEmulatorEvent::Wakeup);
        }
        out
    }

    pub fn sync_deadline(&self) -> Option<Instant> {
        self.ingress
            .sync_deadline()
            .into_iter()
            .chain(self.parser.sync_timeout().sync_timeout())
            .min()
    }

    pub fn resize(&mut self, size: TerminalSize) -> TerminalEmulatorEvents {
        self.size = grid_size(size);
        self.term.resize(self.size);
        let mut out = Vec::new();
        self.listener
            .drain(&self.term, self.size, &self.palette, &mut out);
        for event in self.ingress.synchronize_grid() {
            if let Some(bytes) = self.graphics.handle_event(event) {
                out.push(TerminalEmulatorEvent::PtyWrite(Zeroizing::new(bytes)));
            }
        }
        out
    }

    pub fn scroll_display(&mut self, delta: i32) -> TerminalEmulatorEvents {
        self.term.scroll_display(Scroll::Delta(delta));
        let mut out = Vec::new();
        self.listener
            .drain(&self.term, self.size, &self.palette, &mut out);
        out
    }

    pub fn mode(&self) -> TermMode {
        *self.term.mode()
    }

    pub fn set_scrollback_limit(&mut self, lines: usize) -> bool {
        self.term.set_scrollback_limit(lines)
    }

    pub fn cursor_blinking(&self) -> bool {
        self.term.cursor_style().blinking
    }

    pub fn search_matches(&self, query: &str) -> Vec<crate::TerminalSearchMatch> {
        crate::search::search_matches_from_term(&self.term, self.size.cols, query)
    }

    pub fn begin_search(
        &self,
        query: &crate::search::scrollback::TerminalSearchQuery,
    ) -> crate::search::scrollback::ScrollbackScan {
        crate::search::scrollback::ScrollbackScan::new(&self.term, self.size.cols, query)
    }

    pub fn advance_search(&self, scan: &mut crate::search::scrollback::ScrollbackScan) -> bool {
        scan.advance(|| &self.term, &|| false)
    }

    pub fn finish_search(
        &self,
        scan: crate::search::scrollback::ScrollbackScan,
    ) -> Vec<crate::TerminalSearchMatch> {
        scan.finish(&|| false).resolve(|| &self.term)
    }

    pub fn set_selection(&mut self, selection: Option<crate::TerminalSelectionRange>) {
        crate::selection::set_term_selection(&mut self.term, selection);
    }

    pub fn selection(&self) -> Option<crate::TerminalSelectionRange> {
        crate::selection::term_selection(&self.term)
    }

    pub fn snapshot(&mut self) -> TerminalSnapshot {
        let snapshot = snapshot_from_term(&self.term, self.size, &self.graphics, &self.palette);
        self.term.reset_damage();
        snapshot
    }

    /// Pass the last snapshot from this emulator; damage is consumed once per frame.
    pub fn incremental_snapshot(&mut self, previous: &TerminalSnapshot) -> TerminalSnapshot {
        incremental_snapshot_from_term(
            &mut self.term,
            self.size,
            &self.graphics,
            &self.palette,
            previous,
        )
    }

    pub fn palette_mut(&mut self) -> &mut TerminalPalette {
        self.term.mark_fully_damaged();
        &mut self.palette
    }

    pub fn set_palette(&mut self, palette: TerminalPalette) {
        *self.palette_mut() = palette;
    }

    pub fn graphics_options(&self) -> &GraphicsOptions {
        self.ingress.options()
    }
}

fn grid_size(size: TerminalSize) -> TerminalSize {
    TerminalSize {
        cols: size.cols.clamp(MIN_COLUMNS, u16::MAX.into()),
        rows: size.rows.clamp(MIN_SCREEN_LINES, u16::MAX.into()),
        ..size
    }
}
