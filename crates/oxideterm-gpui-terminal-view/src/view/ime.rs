use std::{mem, ops::Range};

use gpui::{
    App, Bounds, ClipboardItem, Context, ElementInputHandler, Entity, EntityInputHandler,
    InputHandler, Pixels, Point, UTF16Selection, Window, point, px,
};
use zeroize::Zeroizing;

use super::TerminalView;
use crate::element::{ime_cursor_bounds_for_snapshot, marked_text_cells_before_utf16};

/// Only preedit is editable by the platform. Committed text belongs to the peer,
/// and neither the scrollback nor an echoed password is an IME document.
#[derive(Default)]
pub(super) struct Composition {
    pub(super) text: Zeroizing<String>,
    pub(super) selection: Range<usize>,
}

fn offset(text: &str, utf16: usize) -> (usize, usize) {
    let (mut bytes, mut units) = (0, 0);
    for ch in text.chars() {
        if units >= utf16 {
            break;
        }
        bytes += ch.len_utf8();
        units += ch.len_utf16();
    }
    (bytes, units)
}

impl Composition {
    pub(super) fn marked_text(&self) -> Option<String> {
        (!self.text.is_empty()).then(|| self.text.to_string())
    }

    fn replace(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        selection: Option<Range<usize>>,
    ) {
        let ((start, base), end) = match range {
            Some(r) => (
                offset(&self.text, r.start),
                offset(&self.text, r.end.max(r.start)).0,
            ),
            None => ((0, 0), self.text.len()),
        };
        self.text.replace_range(start..end, new_text);
        let caret = new_text.encode_utf16().count();
        let selected = selection.unwrap_or(caret..caret);
        self.selection = base + offset(new_text, selected.start).1
            ..base + offset(new_text, selected.end.max(selected.start)).1;
    }
}

impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text = &self.composition.text;
        let (start, first) = offset(text, range.start);
        let (end, last) = offset(text, range.end.max(range.start));
        *adjusted = Some(first..last);
        Some(text[start..end].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.composition.selection.clone(),
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        (!self.composition.text.is_empty()).then(|| 0..self.composition.text.encode_utf16().count())
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.composition.text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.composition = Default::default();
        self.reset_blink(cx);
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut composition = mem::take(&mut self.composition);
        if composition.text.is_empty() || range.is_none() {
            self.send_text(text, cx);
        } else {
            composition.replace(range, text, None);
            self.send_text(&composition.text, cx);
        }
        // Even an empty commit removes the painted preedit.
        self.update_blink(cx);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composition.replace(range, text, selection);
        self.reset_blink(cx);
        cx.notify();
    }

    fn paste(&mut self, item: ClipboardItem, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = item.text().map(Zeroizing::new) {
            self.request_paste(text, cx);
        }
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        content: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let mut bounds = ime_cursor_bounds_for_snapshot(&self.snapshot, &self.metrics)?;
        let cells = marked_text_cells_before_utf16(&self.composition.text, range.start);
        bounds.origin +=
            content.origin + point(px(cells as f32 * self.metrics.cell_width_f32()), px(0.0));
        Some(bounds)
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

/// Keep GPUI's entity and paint-time bounds adapter, but use terminal key-repeat
/// and raw-key routing rather than the editor's press-and-hold and IME preferences.
pub(super) struct TerminalViewInputHandler(ElementInputHandler<TerminalView>);

impl TerminalViewInputHandler {
    pub(super) fn new(bounds: Bounds<Pixels>, view: Entity<TerminalView>) -> Self {
        Self(ElementInputHandler::new(bounds, view))
    }
}

impl InputHandler for TerminalViewInputHandler {
    fn apple_press_and_hold_enabled(&mut self) -> bool {
        false
    }

    fn selected_text_range(
        &mut self,
        ignore: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        self.0.selected_text_range(ignore, window, cx)
    }

    fn marked_text_range(&mut self, window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.0.marked_text_range(window, cx)
    }

    fn text_length_utf16(&mut self, window: &mut Window, cx: &mut App) -> Option<usize> {
        self.0.text_length_utf16(window, cx)
    }

    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        self.0.text_for_range(range, adjusted, window, cx)
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0.replace_text_in_range(range, text, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0
            .replace_and_mark_text_in_range(range, text, selection, window, cx);
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut App) {
        self.0.unmark_text(window, cx);
    }

    fn paste(&mut self, item: ClipboardItem, window: &mut Window, cx: &mut App) {
        self.0.paste(item, window, cx);
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        self.0.bounds_for_range(range, window, cx)
    }

    fn element_bounds(&mut self, window: &mut Window, cx: &mut App) -> Option<Bounds<Pixels>> {
        self.0.element_bounds(window, cx)
    }

    fn character_index_for_point(
        &mut self,
        location: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<usize> {
        self.0.character_index_for_point(location, window, cx)
    }
}
