use std::sync::Arc;

use gpui::{
    AnyElement, App, Context, FocusHandle, Focusable, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ObjectFit, Render, RenderImage, SharedString, StyledImage, Window, div,
    prelude::*, rgb,
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

        let background = self.preferences.background.clone().filter(|background| {
            self.preferences.render_policy.allow_background_images && background.path.exists()
        });
        let background_layer = background.as_ref().map(|background| {
            terminal_background_layer(
                background.clone(),
                self.background_image_cache.render_blurred_image(background),
            )
        });
        let terminal_element = TerminalElement::new_with_images_and_bidi(
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
        )
        .transparent_background(background.is_some());

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
            .when_some(background_layer, |pane, background| pane.child(background))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .child(terminal_element),
            )
    }
}

fn terminal_background_layer(
    background: TerminalBackgroundPreferences,
    blurred_image: Option<Arc<RenderImage>>,
) -> AnyElement {
    let image = if let Some(blurred_image) = blurred_image {
        gpui::img(blurred_image)
            .size_full()
            .object_fit(terminal_background_object_fit(background.fit))
            .opacity(background.opacity.clamp(0.0, 1.0))
            .into_any_element()
    } else {
        gpui::img(background.path)
            .size_full()
            .object_fit(terminal_background_object_fit(background.fit))
            .opacity(background.opacity.clamp(0.0, 1.0))
            .with_fallback(|| div().size_full().into_any_element())
            .into_any_element()
    };

    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .overflow_hidden()
        .child(image)
        .into_any_element()
}

fn terminal_background_object_fit(fit: TerminalBackgroundFit) -> ObjectFit {
    match fit {
        TerminalBackgroundFit::Cover => ObjectFit::Cover,
        TerminalBackgroundFit::Contain => ObjectFit::Contain,
        TerminalBackgroundFit::Fill => ObjectFit::Fill,
        TerminalBackgroundFit::Tile => ObjectFit::None,
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
