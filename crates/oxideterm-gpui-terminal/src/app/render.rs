use std::sync::Arc;

use gpui::{
    AnyElement, App, Context, FocusHandle, Focusable, FontWeight, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ObjectFit, Render, RenderImage, SharedString, StyledImage,
    Window, div, prelude::*, px, rgb, rgba,
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
        .highlight_rules(self.preferences.highlight_rules.clone())
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
            .when_some(self.pending_paste.clone(), |pane, paste| {
                pane.child(self.render_paste_confirm_overlay(&paste, cx))
            })
    }
}

impl TerminalPane {
    fn render_paste_confirm_overlay(&self, content: &str, cx: &mut Context<Self>) -> AnyElement {
        const PREVIEW_MAX_LINES: usize = 5;

        let lines = content.split('\n').collect::<Vec<_>>();
        let remaining_lines = lines.len().saturating_sub(PREVIEW_MAX_LINES);
        let title = label_with_count(&self.preferences.paste_labels.title_template, lines.len());
        let more_lines = label_with_count(
            &self.preferences.paste_labels.more_lines_template,
            remaining_lines,
        );

        let mut preview = div()
            .rounded(px(4.0))
            .border_1()
            .border_color(rgb(0x2f343d))
            .bg(rgb(0x090b0f))
            .p(px(8.0))
            .mb(px(12.0))
            .max_h(px(128.0))
            .overflow_hidden()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .font_family(SharedString::from("JetBrainsMono Nerd Font"))
            .text_size(px(12.0))
            .text_color(rgb(0x9ca3af));

        for line in lines.iter().take(PREVIEW_MAX_LINES) {
            let rendered_line = if line.is_empty() {
                "\u{00a0}".to_string()
            } else {
                (*line).to_string()
            };
            preview = preview.child(div().overflow_hidden().child(rendered_line));
        }
        if remaining_lines > 0 {
            preview = preview.child(div().italic().text_color(rgb(0x9ca3af)).child(more_lines));
        }

        let cancel_label = self.preferences.paste_labels.cancel.clone();
        let paste_label = self.preferences.paste_labels.paste.clone();
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000033))
            .child(
                div()
                    .w(px(448.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(rgba(0xeab30880))
                    .bg(rgba(0x151922f2))
                    .shadow_lg()
                    .p(px(16.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .mb(px(12.0))
                            .child(
                                div()
                                    .size(px(16.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(14.0))
                                    .text_color(rgb(0xeab308))
                                    .child("!"),
                            )
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(0xfef3c7))
                                    .child(title),
                            ),
                    )
                    .child(preview)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(16.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .text_size(px(12.0))
                                    .text_color(rgb(0x9ca3af))
                                    .child(self.render_key_hint(
                                        "Enter",
                                        &self.preferences.paste_labels.confirm,
                                    ))
                                    .child(div().mx(px(8.0)).text_color(rgb(0x9ca3af)).child("·"))
                                    .child(self.render_key_hint(
                                        "Esc",
                                        &self.preferences.paste_labels.cancel,
                                    )),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .px(px(12.0))
                                            .py(px(4.0))
                                            .text_size(px(12.0))
                                            .text_color(rgb(0x9ca3af))
                                            .cursor_pointer()
                                            .child(cancel_label)
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|this, _event, _window, cx| {
                                                    this.cancel_pending_paste(cx);
                                                }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .rounded(px(4.0))
                                            .bg(rgb(0xca8a04))
                                            .px(px(12.0))
                                            .py(px(4.0))
                                            .text_size(px(12.0))
                                            .text_color(rgb(0xffffff))
                                            .cursor_pointer()
                                            .child(paste_label)
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|this, _event, _window, cx| {
                                                    this.confirm_pending_paste(cx);
                                                }),
                                            ),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_key_hint(&self, key: &'static str, label: &str) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .rounded(px(4.0))
                    .bg(rgb(0x222834))
                    .px(px(6.0))
                    .py(px(2.0))
                    .text_size(px(10.0))
                    .text_color(rgb(0x9ca3af))
                    .child(key),
            )
            .child(label.to_string())
            .into_any_element()
    }
}

fn label_with_count(template: &str, count: usize) -> String {
    template.replace("{{count}}", &count.to_string())
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
