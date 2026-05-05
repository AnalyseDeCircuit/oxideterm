use gpui::{
    App, Context, FocusHandle, Focusable, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Render, SharedString, Window, div, prelude::*, px, rgb,
};
use oxideterm_terminal::TerminalSnapshot;

use super::TerminalPane;
use crate::terminal_ui::*;
use crate::terminal_view::*;

impl Focusable for TerminalPane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.metrics = TerminalMetrics::measure_with_preferences(window, &self.preferences);
        let mut snapshot = self.snapshot.clone();
        snapshot.cursor_shape = self.preferences.cursor_shape;
        apply_theme_defaults_to_snapshot(&mut snapshot, &self.theme);
        let rendered_images = self.image_cache.render_images(
            &snapshot.images,
            self.preferences
                .render_policy
                .terminal_graphics
                .decode_images,
        );

        let (lifecycle, process_info) = {
            let terminal = self.terminal.lock();
            (terminal.lifecycle(), terminal.process_info())
        };
        let link_preview = self
            .hovered_link
            .as_ref()
            .map(|link| format!(" · link {}", link.target))
            .unwrap_or_default();
        let header = format!(
            "{} · {}x{} · offset {} · {}{} · partial",
            self.title,
            self.snapshot.cols,
            self.snapshot.rows,
            self.snapshot.display_offset,
            terminal_lifecycle_label(&lifecycle),
            terminal_process_header(&process_info),
        );
        let header = format!("{header}{link_preview}");
        div()
            .id("terminal-pane")
            .size_full()
            .relative()
            .bg(if self.bell_flash {
                rgb(self.theme.bell_background)
            } else {
                rgb(self.theme.background)
            })
            .text_color(rgb(self.theme.foreground))
            .font_family(SharedString::from(self.preferences.font_family.clone()))
            .text_size(self.metrics.font_size)
            .line_height(self.metrics.line_height)
            .track_focus(&self.focus_handle)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle);
                    this.handle_mouse_down(event, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                    this.handle_mouse_down(event, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                    this.handle_mouse_down(event, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                this.handle_mouse_move(event, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _window, cx| {
                    this.handle_mouse_up(event, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseUpEvent, _window, cx| {
                    this.handle_mouse_up(event, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(|this, event: &MouseUpEvent, _window, cx| {
                    this.handle_mouse_up(event, cx);
                }),
            )
            .on_key_down(cx.listener(|this, event, _window, cx| {
                this.handle_key(event, cx);
            }))
            .on_key_up(cx.listener(|this, event, _window, cx| {
                this.handle_key_up(event, cx);
            }))
            .on_scroll_wheel(cx.listener(|this, event, _window, cx| {
                this.handle_scroll(event, cx);
            }))
            .child(
                div()
                    .absolute()
                    .right_3()
                    .bottom_3()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .bg(rgb(self.theme.header_background))
                    .text_color(rgb(self.theme.header_foreground))
                    .text_size(px(12.0))
                    .child(header),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .child(TerminalElement::new_with_images_and_bidi(
                        snapshot,
                        rendered_images,
                        self.selection.filter(|s| !s.is_empty()),
                        self.metrics.clone(),
                        self.theme.clone(),
                        self.cursor_visible,
                        self.marked_text.clone(),
                        self.search_query.clone(),
                        self.search_query
                            .as_deref()
                            .map(|query| self.terminal.lock().search_matches(query))
                            .unwrap_or_default(),
                        self.selected_search_match,
                        self.hovered_link.clone(),
                        self.settings.bidi_enabled,
                        Some(TerminalElementInput {
                            focus_handle: self.focus_handle.clone(),
                            view: cx.entity(),
                        }),
                    )),
            )
    }
}

fn apply_theme_defaults_to_snapshot(snapshot: &mut TerminalSnapshot, theme: &TerminalUiTheme) {
    let default_background = terminal_color_from_hex(OXIDETERM_TERMINAL_BACKGROUND);
    let default_foreground = terminal_color_from_hex(OXIDETERM_TERMINAL_FOREGROUND);
    let themed_background = terminal_color_from_hex(theme.background);
    let themed_foreground = terminal_color_from_hex(theme.foreground);

    for row in &mut snapshot.lines {
        for cell in &mut row.cells {
            if cell.bg == default_background {
                cell.bg = themed_background;
            }
            if cell.fg == default_foreground {
                cell.fg = themed_foreground;
            }
        }
    }
}
