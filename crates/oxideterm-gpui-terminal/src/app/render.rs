use std::sync::Arc;

use gpui::{
    AnyElement, App, ClipboardItem, Context, FocusHandle, Focusable, FontWeight, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, Render, RenderImage, SharedString,
    StyledImage, Window, div, prelude::*, px, rgb, rgba,
};
use oxideterm_terminal::{TerminalCommandMark, TerminalSnapshot};

use super::{TerminalContextMenu, TerminalPane};
use crate::terminal_ui::*;
use crate::terminal_view::*;

const PASTE_PREVIEW_TEXT_RADIUS: f32 = 4.0;
const PASTE_CONFIRM_DIALOG_RADIUS: f32 = 8.0;
const PASTE_CONFIRM_BUTTON_RADIUS: f32 = 4.0;
const TERMINAL_KEY_HINT_RADIUS: f32 = 4.0;
const TERMINAL_CONTEXT_MENU_WIDTH: f32 = 176.0;
const TERMINAL_CONTEXT_MENU_ITEM_HEIGHT: f32 = 34.0;
const TERMINAL_CONTEXT_MENU_MARGIN: f32 = 8.0;
const TERMINAL_CONTEXT_MENU_RADIUS: f32 = 6.0;

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
        .command_marks(
            if self.settings.command_marks_enabled {
                self.command_marks.clone()
            } else {
                Vec::new()
            },
            self.selected_command_mark_id.clone(),
        )
        .highlight_rules(self.preferences.highlight_rules.clone())
        .transparent_background(background.is_some())
        .ghost_text(self.autosuggest_ghost_text())
        .layout_cache(self.layout_cache.clone());

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
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle);
                    let mode = this.terminal.lock().mode();
                    if mouse_mode(mode, event.modifiers.shift) {
                        this.handle_mouse_down(event, cx);
                    } else {
                        window.prevent_default();
                        this.open_terminal_context_menu(event, cx);
                    }
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
            .on_key_down(cx.listener(|this, event, window, cx| {
                if this.handle_key(event, cx) {
                    // Terminal-owned shortcuts and control keys must not fall
                    // through to GPUI defaults after being sent to the PTY.
                    window.prevent_default();
                    cx.stop_propagation();
                }
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
            .when_some(self.context_menu, |pane, menu| {
                pane.child(self.render_terminal_context_menu(menu, cx))
            })
            .when(self.preferences.show_fps_overlay, |pane| {
                pane.child(self.render_fps_overlay())
            })
            .when(
                self.settings.command_marks_enabled
                    && self.settings.command_marks_show_hover_actions,
                |pane| {
                    pane.when_some(self.selected_command_mark(), |pane, mark| {
                        pane.child(self.render_command_mark_actions(mark, cx))
                    })
                },
            )
    }
}

impl TerminalPane {
    fn render_terminal_context_menu(
        &self,
        menu: TerminalContextMenu,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (left, top) = self.clamped_terminal_context_menu_position(menu);
        let copy_label = self.preferences.command_selection_labels.copy.clone();
        let paste_label = self.preferences.paste_labels.paste.clone();

        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                    this.dismiss_terminal_context_menu(cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    this.dismiss_terminal_context_menu(cx);
                }),
            )
            .child(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .w(px(TERMINAL_CONTEXT_MENU_WIDTH))
                    .rounded(px(TERMINAL_CONTEXT_MENU_RADIUS))
                    .border_1()
                    .border_color(rgba(0x2f343ddd))
                    .bg(rgba(0x111827f2))
                    .p(px(4.0))
                    .shadow_lg()
                    .child(self.render_terminal_context_menu_item(
                        copy_label,
                        !menu.can_copy,
                        |this, _event, _window, cx| {
                            this.copy_selection_from_context_menu(cx);
                        },
                        cx,
                    ))
                    .child(self.render_terminal_context_menu_item(
                        paste_label,
                        false,
                        |this, _event, _window, cx| {
                            this.dismiss_terminal_context_menu(cx);
                            this.paste_from_clipboard(cx);
                        },
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_terminal_context_menu_item(
        &self,
        label: String,
        disabled: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .h(px(TERMINAL_CONTEXT_MENU_ITEM_HEIGHT))
            .w_full()
            .rounded(px(PASTE_CONFIRM_BUTTON_RADIUS))
            .px(px(10.0))
            .flex()
            .items_center()
            .text_size(px(13.0))
            .text_color(if disabled {
                rgba(0xe5e7eb73)
            } else {
                rgb(0xe5e7eb)
            })
            .when(!disabled, |item| {
                item.cursor_pointer()
                    .hover(|item| item.bg(rgba(0x37415199)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            listener(this, event, window, cx);
                        }),
                    )
            })
            .child(label)
            .into_any_element()
    }

    fn clamped_terminal_context_menu_position(&self, menu: TerminalContextMenu) -> (f32, f32) {
        let bounds = self.bounds.map(|bounds| bounds.size);
        let max_x = bounds
            .map(|size| {
                f32::from(size.width) - TERMINAL_CONTEXT_MENU_WIDTH - TERMINAL_CONTEXT_MENU_MARGIN
            })
            .unwrap_or(menu.x);
        let menu_height = TERMINAL_CONTEXT_MENU_ITEM_HEIGHT * 2.0 + 8.0;
        let max_y = bounds
            .map(|size| f32::from(size.height) - menu_height - TERMINAL_CONTEXT_MENU_MARGIN)
            .unwrap_or(menu.y);

        (
            menu.x
                .max(TERMINAL_CONTEXT_MENU_MARGIN)
                .min(max_x.max(TERMINAL_CONTEXT_MENU_MARGIN)),
            menu.y
                .max(TERMINAL_CONTEXT_MENU_MARGIN)
                .min(max_y.max(TERMINAL_CONTEXT_MENU_MARGIN)),
        )
    }

    fn copy_selection_from_context_menu(&mut self, cx: &mut Context<Self>) {
        self.dismiss_terminal_context_menu(cx);
        let _copied = self.copy_selection_to_clipboard_if_present(cx);
    }

    fn dismiss_terminal_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.context_menu.take().is_some() {
            cx.notify();
        }
    }

    fn selected_command_mark(&self) -> Option<TerminalCommandMark> {
        let selected_id = self.selected_command_mark_id.as_deref()?;
        self.command_marks
            .iter()
            .find(|mark| mark.command_id == selected_id)
            .cloned()
    }

    fn render_fps_overlay(&self) -> AnyElement {
        let stats = self.render_stats;
        div()
            .absolute()
            .top(px(8.0))
            .left(px(8.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(8.0))
            .py(px(2.0))
            .rounded(px(4.0))
            .border_1()
            .border_color(rgba(0xffffff33))
            .bg(rgba(0x0d0f12dd))
            .text_size(px(10.0))
            .font_family(SharedString::from(self.preferences.font_family.clone()))
            .line_height(px(20.0))
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(stats.tier.color()))
                    .child(stats.tier.label()),
            )
            .child(div().text_color(rgba(0xe6e8eb99)).child("|"))
            .child(
                div()
                    .text_color(rgb(self.theme.foreground))
                    .child(stats.fps.to_string()),
            )
            .child(div().text_color(rgba(0xe6e8eb99)).child("fps"))
            .child(div().text_color(rgba(0xe6e8eb99)).child("·"))
            .child(
                div()
                    .text_color(rgba(0xe6e8eb99))
                    .child(stats.writes_per_sec.to_string()),
            )
            .child(div().text_color(rgba(0xe6e8eb99)).child("wps"))
            .child(div().text_color(rgba(0xe6e8eb99)).child("·"))
            .child(
                div()
                    .text_color(rgba(0xe6e8eb99))
                    .child(format!("{}b", stats.pending_bytes)),
            )
            .into_any_element()
    }

    fn render_command_mark_actions(
        &self,
        mark: TerminalCommandMark,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let action_top = self.command_mark_action_top(&mark);
        let copy_label = self.preferences.command_selection_labels.copy.clone();
        let _copy_title = self.preferences.command_selection_labels.copy_title.clone();

        div()
            .absolute()
            .top(px(action_top))
            .right(px(10.0))
            .flex()
            .gap(px(4.0))
            .child(
                div()
                    .rounded_full()
                    .border_1()
                    .border_color(rgba(0x60a5fa59))
                    .bg(rgba(0x0f172aeb))
                    .px(px(7.0))
                    .py(px(3.0))
                    .text_size(px(10.0))
                    .line_height(px(10.0))
                    .text_color(rgb(0xbfdbfe))
                    .cursor_pointer()
                    .child(copy_label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.copy_command_mark_output_to_clipboard(&mark, cx);
                        }),
                    ),
            )
            .into_any_element()
    }

    fn command_mark_action_top(&self, mark: &TerminalCommandMark) -> f32 {
        let Some(bounds) = self.bounds else {
            return 0.0;
        };
        let viewport_start = self
            .snapshot
            .scrollback_lines
            .saturating_sub(self.snapshot.display_offset);
        let end_line = self.selectable_command_mark_end_line(mark);
        let visible_start = mark.start_line.max(viewport_start);
        let visible_end =
            end_line.min(viewport_start.saturating_add(self.snapshot.rows.saturating_sub(1)));
        let start_row = visible_start.saturating_sub(viewport_start);
        let end_row = visible_end.saturating_sub(viewport_start);
        let overlay_top = start_row as f32 * self.metrics.line_height_f32();
        let overlay_bottom = (end_row + 1) as f32 * self.metrics.line_height_f32();
        let actions_height = 22.0;
        let gap = 5.0;
        let viewport_height = f32::from(bounds.size.height);
        let space_above = overlay_top;
        let space_below = viewport_height - overlay_bottom;
        let top = if space_above >= actions_height + gap || space_below < actions_height + gap {
            overlay_top - actions_height - gap
        } else {
            overlay_bottom + gap
        };
        top.clamp(0.0, (viewport_height - actions_height).max(0.0))
    }

    fn copy_command_mark_output_to_clipboard(
        &mut self,
        mark: &TerminalCommandMark,
        cx: &mut Context<Self>,
    ) {
        let output = self.terminal.lock().command_output_text(mark);
        cx.write_to_clipboard(ClipboardItem::new_string(output));
    }

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
            .rounded(px(PASTE_PREVIEW_TEXT_RADIUS))
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
            .font_family(SharedString::from(self.preferences.font_family.clone()))
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
                    .rounded(px(PASTE_CONFIRM_DIALOG_RADIUS))
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
                                            .rounded(px(PASTE_CONFIRM_BUTTON_RADIUS))
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
                    .rounded(px(TERMINAL_KEY_HINT_RADIUS))
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
        let uses_default_theme = row
            .cells
            .iter()
            .any(|cell| cell.bg == default_background || cell.fg == default_foreground);
        if !uses_default_theme {
            continue;
        }

        for cell in row.cells_mut() {
            if cell.bg == default_background {
                cell.bg = themed_background;
            }
            if cell.fg == default_foreground {
                cell.fg = themed_foreground;
            }
        }
        row.refresh_signature();
    }
}
