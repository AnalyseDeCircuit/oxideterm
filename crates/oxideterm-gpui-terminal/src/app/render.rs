use std::sync::Arc;

use gpui::{
    AnchoredPositionMode, AnyElement, App, ClipboardItem, Context, Corner, FocusHandle, Focusable,
    FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, Render,
    RenderImage, SharedString, StyledImage, Window, anchored, deferred, div, point, prelude::*, px,
    rgb, rgba,
};
use oxideterm_gpui_ui::context_menu::{
    ContextMenuItemKind, context_menu_action, context_menu_backdrop, context_menu_content,
    context_menu_event_boundary, context_menu_item, context_menu_item_height_estimate,
    context_menu_separator, context_menu_separator_height_estimate,
};
use oxideterm_gpui_ui::modal::{TAURI_POPOVER_LAYER_PRIORITY, overlay_content_boundary};
use oxideterm_gpui_ui::progress::progress;
use oxideterm_terminal::{
    DetectedModemProtocol, ModemTransferDirection, TerminalCommandMark, TerminalSnapshot,
};

use super::{
    ModemProgressState, TerminalCommandNavigationDirection, TerminalContextAction,
    TerminalContextMenu, TerminalPane,
};
use crate::terminal_ui::*;
use crate::terminal_view::*;

const PASTE_PREVIEW_TEXT_RADIUS: f32 = 4.0;
const PASTE_CONFIRM_DIALOG_RADIUS: f32 = 8.0;
const PASTE_CONFIRM_BUTTON_RADIUS: f32 = 4.0;
const TERMINAL_KEY_HINT_RADIUS: f32 = 4.0;
const TERMINAL_CONTEXT_MENU_WIDTH: f32 = 220.0;
const TERMINAL_CONTEXT_MENU_ACTION_COUNT: f32 = 17.0;
const TERMINAL_CONTEXT_MENU_SEPARATOR_COUNT: f32 = 4.0;
const TERMINAL_CONTEXT_MENU_MARGIN: f32 = 8.0;

fn clamp_terminal_context_menu_position(
    pointer_x: f32,
    pointer_y: f32,
    viewport_width: f32,
    viewport_height: f32,
    menu_width: f32,
    menu_height: f32,
    margin: f32,
) -> (f32, f32) {
    // Context menus are top-layer window overlays, so collision must use the
    // window viewport instead of the terminal pane that opened the menu.
    let max_x = (viewport_width - menu_width - margin).max(margin);
    let max_y = (viewport_height - menu_height - margin).max(margin);
    (
        pointer_x.max(margin).min(max_x),
        pointer_y.max(margin).min(max_y),
    )
}

impl Focusable for TerminalPane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.metrics = TerminalMetrics::measure_with_preferences(window, &self.preferences);
        let scrollbar_display_offset = self.smooth_scroll_display_offset();
        let (mut snapshot, smooth_scroll_y_offset, viewport_rows) =
            self.render_snapshot_for_smooth_scroll();
        snapshot.cursor_shape = self.preferences.cursor_shape;
        apply_theme_defaults_to_snapshot(&mut snapshot, &self.theme);
        let rendered_images = self.image_cache.render_images(
            &snapshot.images,
            self.preferences
                .render_policy
                .terminal_graphics
                .decode_images,
        );
        let row_timestamps = self
            .terminal_timestamps_enabled
            .then(|| Arc::new(self.row_timestamps.clone()));

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
            self.hovered_command_mark_id.clone(),
        )
        .highlight_rules(self.preferences.highlight_rules.clone())
        .row_timestamps(row_timestamps)
        .transparent_background(background.is_some())
        .ghost_text(self.terminal_ghost_text())
        .viewport_rows(viewport_rows)
        .scrollbar_display_offset(scrollbar_display_offset)
        .scroll_y_offset(smooth_scroll_y_offset)
        .command_mark_gutter_width(self.command_mark_gutter_width())
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
            .when_some(self.modem_progress.clone(), |pane, transfer| {
                pane.child(self.render_modem_progress_overlay(transfer, cx))
            })
            .when_some(self.context_menu.clone(), |pane, menu| {
                pane.child(self.render_terminal_context_menu(menu, window, cx))
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
    fn render_snapshot_for_smooth_scroll(&self) -> (TerminalSnapshot, gpui::Pixels, usize) {
        let snapshot = self.snapshot.clone();
        let viewport_rows = snapshot.rows;
        if !self.settings.smooth_scroll {
            return (snapshot, px(0.0), viewport_rows);
        }

        let remainder = f32::from(self.scroll_remainder_px);
        if remainder.abs() <= f32::EPSILON {
            return (snapshot, px(0.0), viewport_rows);
        }

        let overscan_rows = viewport_rows.saturating_add(1);
        if remainder > 0.0 && snapshot.display_offset < snapshot.scrollback_lines {
            let display_offset = snapshot.display_offset.saturating_add(1);
            let overscan = self
                .terminal
                .lock()
                .snapshot_with_display_offset(display_offset, overscan_rows);
            return (
                overscan,
                self.scroll_remainder_px - self.metrics.line_height,
                viewport_rows,
            );
        }

        if remainder < 0.0 && snapshot.display_offset > 0 {
            let overscan = self
                .terminal
                .lock()
                .snapshot_with_display_offset(snapshot.display_offset, overscan_rows);
            return (overscan, self.scroll_remainder_px, viewport_rows);
        }

        (snapshot, px(0.0), viewport_rows)
    }

    fn render_terminal_context_menu(
        &self,
        menu: TerminalContextMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (left, top) = self.clamped_terminal_context_menu_window_position(&menu, window);
        let copy_label = self.preferences.command_selection_labels.copy.clone();
        let copy_command_label = self
            .preferences
            .command_selection_labels
            .copy_command
            .clone();
        let send_to_ai_label = self.preferences.command_selection_labels.send_to_ai.clone();
        let fill_command_bar_label = self
            .preferences
            .command_selection_labels
            .fill_command_bar
            .clone();
        let find_label = self.preferences.command_selection_labels.find.clone();
        let select_command_label = self
            .preferences
            .command_selection_labels
            .select_command
            .clone();
        let previous_command_label = self
            .preferences
            .command_selection_labels
            .previous_command
            .clone();
        let next_command_label = self
            .preferences
            .command_selection_labels
            .next_command
            .clone();
        let clear_screen_label = self
            .preferences
            .command_selection_labels
            .clear_screen
            .clone();
        let modem_labels = self.preferences.modem_labels.clone();
        let paste_label = self.preferences.paste_labels.paste.clone();
        let command_mark_id = menu.command_mark_id.clone();
        let has_command_mark = command_mark_id.is_some();
        let has_command_text = self.command_mark_has_command_text(command_mark_id.as_deref());
        let previous_reference_line = menu.reference_line;
        let next_reference_line = menu.reference_line;
        let select_command_mark_id = command_mark_id.clone();
        let copy_command_mark_id = command_mark_id.clone();

        let tokens = &self.theme.tokens;
        let popup = context_menu_event_boundary(
            context_menu_content(tokens)
                .w(px(TERMINAL_CONTEXT_MENU_WIDTH))
                .child(self.render_terminal_context_menu_item(
                    copy_label,
                    !menu.has_selection,
                    |this, _event, _window, cx| {
                        this.copy_selection_from_context_menu(cx);
                    },
                    cx,
                ))
                .child(self.render_terminal_context_menu_item(
                    copy_command_label,
                    !has_command_text,
                    move |this, _event, _window, cx| {
                        this.dismiss_terminal_context_menu(cx);
                        this.copy_command_mark_command_to_clipboard(
                            copy_command_mark_id.as_deref(),
                            cx,
                        );
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
                ))
                .child(context_menu_separator(tokens))
                .child(self.render_terminal_context_menu_item(
                    send_to_ai_label,
                    !menu.has_selection,
                    |this, _event, _window, cx| {
                        this.request_context_action(
                            TerminalContextAction::SendSelectionToAi,
                            true,
                            cx,
                        );
                    },
                    cx,
                ))
                .child(self.render_terminal_context_menu_item(
                    fill_command_bar_label,
                    !menu.has_selection,
                    |this, _event, _window, cx| {
                        this.request_context_action(
                            TerminalContextAction::FillCommandBarFromSelection,
                            true,
                            cx,
                        );
                    },
                    cx,
                ))
                .child(self.render_terminal_context_menu_item(
                    find_label,
                    false,
                    |this, _event, _window, cx| {
                        this.request_context_action(TerminalContextAction::OpenSearch, false, cx);
                    },
                    cx,
                ))
                .child(context_menu_separator(tokens))
                .child(self.render_terminal_context_menu_item(
                    modem_labels.binary_transfer,
                    true,
                    |_this, _event, _window, _cx| {},
                    cx,
                ))
                .child(self.render_terminal_context_menu_item(
                    modem_labels.xmodem_upload,
                    false,
                    |this, _event, _window, cx| {
                        this.dismiss_terminal_context_menu(cx);
                        this.start_manual_modem_transfer(
                            DetectedModemProtocol::Xmodem,
                            ModemTransferDirection::Upload,
                            cx,
                        );
                    },
                    cx,
                ))
                .child(self.render_terminal_context_menu_item(
                    modem_labels.xmodem_receive,
                    false,
                    |this, _event, _window, cx| {
                        this.dismiss_terminal_context_menu(cx);
                        this.start_manual_modem_transfer(
                            DetectedModemProtocol::Xmodem,
                            ModemTransferDirection::Download,
                            cx,
                        );
                    },
                    cx,
                ))
                .child(self.render_terminal_context_menu_item(
                    modem_labels.ymodem_upload,
                    false,
                    |this, _event, _window, cx| {
                        this.dismiss_terminal_context_menu(cx);
                        this.start_manual_modem_transfer(
                            DetectedModemProtocol::Ymodem,
                            ModemTransferDirection::Upload,
                            cx,
                        );
                    },
                    cx,
                ))
                .child(self.render_terminal_context_menu_item(
                    modem_labels.ymodem_receive,
                    false,
                    |this, _event, _window, cx| {
                        this.dismiss_terminal_context_menu(cx);
                        this.start_manual_modem_transfer(
                            DetectedModemProtocol::Ymodem,
                            ModemTransferDirection::Download,
                            cx,
                        );
                    },
                    cx,
                ))
                .child(self.render_terminal_context_menu_item(
                    modem_labels.zmodem_upload,
                    false,
                    |this, _event, _window, cx| {
                        this.dismiss_terminal_context_menu(cx);
                        this.start_manual_modem_transfer(
                            DetectedModemProtocol::Zmodem,
                            ModemTransferDirection::Upload,
                            cx,
                        );
                    },
                    cx,
                ))
                .child(self.render_terminal_context_menu_item(
                    modem_labels.zmodem_receive,
                    false,
                    |this, _event, _window, cx| {
                        this.dismiss_terminal_context_menu(cx);
                        this.start_manual_modem_transfer(
                            DetectedModemProtocol::Zmodem,
                            ModemTransferDirection::Download,
                            cx,
                        );
                    },
                    cx,
                ))
                .child(context_menu_separator(tokens))
                .child(self.render_terminal_context_menu_item(
                    select_command_label,
                    !has_command_mark,
                    move |this, _event, _window, cx| {
                        this.dismiss_terminal_context_menu(cx);
                        this.select_command_mark_by_id(select_command_mark_id.clone(), cx);
                    },
                    cx,
                ))
                .child(self.render_terminal_context_menu_item(
                    previous_command_label,
                    !menu.has_previous_command,
                    move |this, _event, _window, cx| {
                        this.dismiss_terminal_context_menu(cx);
                        this.jump_to_command_mark_from_context_menu(
                            previous_reference_line,
                            TerminalCommandNavigationDirection::Previous,
                            cx,
                        );
                    },
                    cx,
                ))
                .child(self.render_terminal_context_menu_item(
                    next_command_label,
                    !menu.has_next_command,
                    move |this, _event, _window, cx| {
                        this.dismiss_terminal_context_menu(cx);
                        this.jump_to_command_mark_from_context_menu(
                            next_reference_line,
                            TerminalCommandNavigationDirection::Next,
                            cx,
                        );
                    },
                    cx,
                ))
                .child(context_menu_separator(tokens))
                .child(self.render_terminal_context_menu_item(
                    clear_screen_label,
                    false,
                    |this, _event, _window, cx| {
                        this.dismiss_terminal_context_menu(cx);
                        this.clear_screen_from_context_menu(cx);
                    },
                    cx,
                )),
        );

        deferred(
            context_menu_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                        this.dismiss_terminal_context_menu(cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        this.dismiss_terminal_context_menu(cx);
                        cx.stop_propagation();
                    }),
                )
                .child(
                    anchored()
                        .anchor(Corner::TopLeft)
                        .position(point(px(left), px(top)))
                        .position_mode(AnchoredPositionMode::Window)
                        .child(overlay_content_boundary(popup)),
                ),
        )
        .with_priority(TAURI_POPOVER_LAYER_PRIORITY)
        .into_any_element()
    }

    fn render_modem_progress_overlay(
        &self,
        transfer: ModemProgressState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = &self.theme.tokens;
        let status_text = transfer
            .total_text
            .as_ref()
            .map(|total| format!("{} / {}", transfer.transferred_text, total))
            .unwrap_or_else(|| transfer.transferred_text.clone());

        div()
            .absolute()
            .right(px(tokens.spacing.three))
            .bottom(px(tokens.spacing.three))
            .w(px(320.0))
            .rounded(px(tokens.radii.md))
            .border_1()
            .border_color(rgb(tokens.ui.border))
            .bg(rgba((tokens.ui.bg_elevated << 8) | 0xf2))
            .p(px(tokens.spacing.three))
            .shadow_lg()
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(tokens.spacing.three))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .text_size(px(tokens.metrics.ui_text_sm))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(tokens.ui.text))
                                    .child(self.preferences.modem_labels.binary_transfer.clone()),
                            )
                            .when_some(transfer.file_name, |content, file_name| {
                                content.child(
                                    div()
                                        .mt(px(tokens.spacing.one))
                                        .truncate()
                                        .text_size(px(tokens.metrics.ui_text_xs))
                                        .text_color(rgb(tokens.ui.text_muted))
                                        .child(file_name),
                                )
                            })
                            .child(
                                div()
                                    .mt(px(tokens.spacing.one))
                                    .text_size(px(tokens.metrics.ui_text_xs))
                                    .text_color(rgb(tokens.ui.text_muted))
                                    .child(status_text),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .cursor_pointer()
                            .rounded(px(tokens.radii.sm))
                            .border_1()
                            .border_color(rgb(tokens.ui.border))
                            .px(px(tokens.metrics.ui_button_sm_padding_x))
                            .h(px(tokens.metrics.ui_button_sm_height))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(tokens.metrics.ui_text_sm))
                            .text_color(rgb(tokens.ui.text))
                            .hover(|button| button.bg(rgb(tokens.ui.bg_hover)))
                            .child(self.preferences.paste_labels.cancel.clone())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.cancel_active_modem_transfer(cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    ),
            )
            .child(
                progress(tokens, transfer.percent, transfer.percent.is_none())
                    .mt(px(tokens.spacing.three)),
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
        let item = context_menu_item(
            &self.theme.tokens,
            label,
            ContextMenuItemKind::Plain,
            false,
            disabled,
        )
        .w_full();

        context_menu_action(
            item,
            disabled,
            false,
            cx.listener(move |this, event, window, cx| {
                window.prevent_default();
                listener(this, event, window, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn clamped_terminal_context_menu_window_position(
        &self,
        menu: &TerminalContextMenu,
        window: &Window,
    ) -> (f32, f32) {
        let viewport = window.viewport_size();
        let origin = self
            .bounds
            .map(|bounds| bounds.origin)
            .unwrap_or_else(|| point(px(0.0), px(0.0)));
        let menu_height = self.terminal_context_menu_height_estimate();
        clamp_terminal_context_menu_position(
            f32::from(origin.x) + menu.x,
            f32::from(origin.y) + menu.y,
            f32::from(viewport.width),
            f32::from(viewport.height),
            TERMINAL_CONTEXT_MENU_WIDTH,
            menu_height,
            TERMINAL_CONTEXT_MENU_MARGIN,
        )
    }

    fn terminal_context_menu_height_estimate(&self) -> f32 {
        let tokens = &self.theme.tokens;
        // Context menu rendering is token-driven; positioning uses the same
        // Radix-mapped padding and shared line box as the rendered rows.
        tokens.metrics.ui_menu_padding * 2.0
            + TERMINAL_CONTEXT_MENU_ACTION_COUNT * context_menu_item_height_estimate(tokens)
            + TERMINAL_CONTEXT_MENU_SEPARATOR_COUNT * context_menu_separator_height_estimate(tokens)
    }

    fn copy_selection_from_context_menu(&mut self, cx: &mut Context<Self>) {
        self.dismiss_terminal_context_menu(cx);
        let _copied = self.copy_selection_to_clipboard_if_present(cx);
    }

    fn request_context_action(
        &mut self,
        action: TerminalContextAction,
        requires_selection: bool,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_terminal_context_menu(cx);
        if requires_selection && self.selected_text_snapshot().is_none() {
            return;
        }
        // Workspace owns AI and command-bar behavior; the terminal only records
        // the user's menu intent and lets the active-pane owner consume it.
        self.context_action_requested = Some(action);
        cx.notify();
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

#[cfg(test)]
mod tests {
    use super::clamp_terminal_context_menu_position;

    #[test]
    fn context_menu_position_collides_with_window_edges() {
        let placement =
            clamp_terminal_context_menu_position(760.0, 580.0, 800.0, 600.0, 220.0, 300.0, 8.0);

        assert_eq!(placement, (572.0, 292.0));
    }

    #[test]
    fn context_menu_position_keeps_window_margin() {
        let placement =
            clamp_terminal_context_menu_position(-20.0, 2.0, 800.0, 600.0, 220.0, 300.0, 8.0);

        assert_eq!(placement, (8.0, 8.0));
    }
}
