use std::time::Duration;

use gpui::{
    App, Bounds, Context, Entity, FocusHandle, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, Render, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder,
};
use oxideterm_terminal_emulator::{TermMode, TerminalSize};

use super::{TerminalView, TerminalViewEvent, ime::TerminalViewInputHandler};
use crate::{
    TerminalCanvasHost, TerminalElement, TerminalElementInput, TerminalMetrics,
    input::should_blink_cursor_for_mode, metrics::TERMINAL_CONTENT_PADDING,
};

pub(super) fn grid_size(
    cols: usize,
    rows: usize,
    metrics: &TerminalMetrics,
    scale: f32,
) -> TerminalSize {
    TerminalSize {
        cols,
        rows,
        cell_width: (metrics.cell_width_f32() * scale).ceil().max(1.0) as u16,
        cell_height: (metrics.line_height_f32() * scale).ceil().max(1.0) as u16,
    }
}

fn cells_in_span(span: Pixels, cell: f32) -> usize {
    // Pixel-aligned windows can be a fraction of a float ULP short of a whole cell.
    let cells = (f32::from(span) - 2.0 * TERMINAL_CONTENT_PADDING) / cell;
    let whole = if (cells - cells.round()).abs() <= 0.0001 {
        cells.round()
    } else {
        cells.floor()
    };
    (whole as usize).clamp(2, u16::MAX.into())
}

impl TerminalCanvasHost for TerminalView {
    fn viewport_changed(&mut self, bounds: Bounds<Pixels>, scale: f32, cx: &mut Context<Self>) {
        if self.viewport != Some((bounds, scale)) {
            self.viewport = Some((bounds, scale));
            self.pointer.hovered = None;
            self.resize_viewport(bounds, scale, cx);
            // An origin-only change still has to reach the next element's input geometry.
            cx.notify();
        }
    }

    fn register_input(
        view: Entity<Self>,
        focus: &FocusHandle,
        content: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.handle_input(focus, TerminalViewInputHandler::new(content, view), cx);
    }
}

impl TerminalView {
    pub(super) fn resize_viewport(
        &mut self,
        bounds: Bounds<Pixels>,
        scale: f32,
        cx: &mut Context<Self>,
    ) {
        self.resize_grid(
            grid_size(
                cells_in_span(bounds.size.width, self.metrics.cell_width_f32()),
                cells_in_span(bounds.size.height, self.metrics.line_height_f32()),
                &self.metrics,
                scale,
            ),
            cx,
        );
    }

    pub(super) fn resize_grid(&mut self, size: TerminalSize, cx: &mut Context<Self>) {
        let old = self.size;
        if (old.cols, old.rows, old.cell_width, old.cell_height)
            == (size.cols, size.rows, size.cell_width, size.cell_height)
        {
            return;
        }
        self.size = size;
        let events = self.emulator.get_mut().resize(size);
        self.route_events(events, cx);
        self.publish_snapshot(true, cx);
        self.refresh_search(cx);
        cx.emit(TerminalViewEvent::Resized(size));
    }

    fn observe_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.focus_subscriptions.is_none() {
            self.focus_subscriptions = Some([
                cx.on_focus(&self.focus, window, Self::sync_focus),
                cx.on_blur(&self.focus, window, Self::sync_focus),
                cx.observe_window_activation(window, Self::sync_focus),
            ]);
            self.sync_focus(window, cx);
        }
    }

    fn sync_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self.focus.is_focused(window) && window.is_window_active();
        if self.focused == focused {
            return;
        }
        self.focused = focused;
        if !focused {
            self.composition = Default::default();
            self.pointer = Default::default();
            self.reported_keys.clear();
        }
        if self.mode().contains(TermMode::FOCUS_IN_OUT) {
            let bytes = if focused { b"\x1b[I" } else { b"\x1b[O" };
            cx.emit(TerminalViewEvent::SendBytes(bytes.to_vec()));
        }
        self.reset_blink(cx);
        cx.notify();
    }

    pub(super) fn reset_blink(&mut self, cx: &mut Context<Self>) {
        self.blink_task = None;
        self.cursor_visible = true;
        self.update_blink(cx);
    }

    pub(super) fn update_blink(&mut self, cx: &mut Context<Self>) {
        let blink = self.composition.text.is_empty()
            && should_blink_cursor_for_mode(
                self.options.blink,
                self.focused,
                self.emulator.borrow().cursor_blinking(),
                self.mode().contains(TermMode::ALT_SCREEN),
                self.snapshot.cursor_shape,
            );
        if !blink {
            self.blink_task = None;
            self.cursor_visible = true;
        } else if self.blink_task.is_none() {
            // Holding the task (and only a weak view in its future) cancels it on blur or drop.
            self.blink_task = Some(cx.spawn(async |view, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(500))
                        .await;
                    if view
                        .update(cx, |view, cx| {
                            view.cursor_visible = !view.cursor_visible;
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.observe_focus(window, cx);
        let mut snapshot = self.snapshot.as_ref().clone();
        if snapshot.cursor_shape != oxideterm_terminal_emulator::TerminalCursorShape::Hidden
            && let Some(shape) = self.options.cursor
        {
            snapshot.cursor_shape = shape;
        }
        let element = TerminalElement::new_with_images_and_bidi(
            snapshot,
            Vec::new(),
            self.selection.filter(|s| !s.is_empty()),
            self.metrics.clone(),
            self.theme.clone(),
            self.cursor_visible,
            self.composition.marked_text(),
            None,
            self.search
                .as_ref()
                .map(|s| s.matches.clone())
                .unwrap_or_default(),
            self.search.as_ref().and_then(|s| s.selected),
            self.pointer.hovered.clone(),
            self.options.bidi,
            Some(TerminalElementInput {
                view: cx.entity(),
                focus_handle: self.focus.clone(),
                last_viewport_bounds: self.viewport.map(|(bounds, _)| bounds),
                last_viewport_scale_factor_bits: self.viewport.map(|(_, scale)| scale.to_bits()),
            }),
        )
        .transparent_background(self.background_alpha.is_some())
        .precomputed_search_matches()
        .marked_text_caret(Some(self.composition.selection.end))
        .detect_file_paths_as_links(false)
        .layout_cache(self.layout.clone());

        let mut root = div()
            .id("terminal-view")
            .when_some(self.background_alpha, |root, alpha| {
                root.bg(gpui::rgba((self.theme.background << 8) | u32::from(alpha)))
            })
            .px(gpui::px(self.options.inset_x))
            .py(gpui::px(self.options.inset_y))
            .size_full()
            .overflow_hidden()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_key_up(cx.listener(Self::key_up))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_scroll(cx.listener(Self::scroll))
            .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                if !hovered && view.pointer.hovered.take().is_some() {
                    cx.notify();
                }
            }));
        for button in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
            root = root
                .on_mouse_down(button, cx.listener(Self::mouse_down))
                .on_mouse_up(button, cx.listener(Self::mouse_up))
                .on_mouse_up_out(button, cx.listener(Self::mouse_up));
        }
        div()
            .relative()
            .size_full()
            .child(root.child(element))
            .children(self.search_panel(cx))
    }
}
