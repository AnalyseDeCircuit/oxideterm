use std::sync::Arc;

use gpui::Timer;
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
use oxideterm_gpui_ui::scroll::ScrollableElement;
use oxideterm_terminal::{
    DetectedModemProtocol, ModemTransferDirection, SerialControlLine, SerialDisplayMode,
    SerialFlowControl, SerialLineEnding, SerialParity, SerialSendMode, SerialSessionConfig,
    TerminalCommandMark, TerminalLifecycle, TerminalSnapshot,
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
const TERMINAL_CONTEXT_MENU_ACTION_COUNT: f32 = 20.0;
const TERMINAL_CONTEXT_MENU_SEPARATOR_COUNT: f32 = 4.0;
const TERMINAL_CONTEXT_MENU_MARGIN: f32 = 8.0;
const SERIAL_CONTROL_BAR_HEIGHT: f32 = 34.0;
const SERIAL_CONTROL_BUTTON_RADIUS: f32 = 999.0;

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

fn terminal_pane_base_is_transparent(bell_flash: bool, has_window_background: bool) -> bool {
    // Content-scoped images paint inside the pane above its fallback color.
    // Only a window-scoped image needs the pane base itself to be transparent.
    !bell_flash && has_window_background
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
        self.drop_retired_images(window, cx);
        let row_timestamps = self
            .terminal_timestamps_enabled
            .then(|| Arc::new(self.row_timestamps.clone()));

        let background = self.preferences.background.clone().filter(|_background| {
            // Keep terminal repaint frames off the filesystem hot path; image
            // fallback and the blurred-image loader handle missing files.
            self.preferences.render_policy.allow_background_images
        });
        let background_layer = background.as_ref().map(|background| {
            terminal_background_layer(
                background.clone(),
                self.background_image_cache.render_blurred_image(background),
            )
        });
        let transparent_pane_base = terminal_pane_base_is_transparent(
            self.bell_flash,
            self.preferences.transparent_background,
        );
        self.drop_retired_images(window, cx);
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
        .transparent_background(background.is_some() || self.preferences.transparent_background)
        .ghost_text(self.terminal_ghost_text())
        .viewport_rows(viewport_rows)
        .scrollbar_display_offset(scrollbar_display_offset)
        .scroll_y_offset(smooth_scroll_y_offset)
        .command_mark_gutter_width(self.command_mark_gutter_width())
        .layout_cache(self.layout_cache.clone());
        let terminal_top = if self.is_serial_transport() {
            SERIAL_CONTROL_BAR_HEIGHT
        } else {
            0.0
        };

        div()
            .id("terminal-pane")
            .size_full()
            .relative()
            .bg(if self.bell_flash {
                rgb(self.theme.bell_background)
            } else if transparent_pane_base {
                rgba(0x00000000)
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
                    .top(px(terminal_top))
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .child(terminal_element),
            )
            .when(self.is_serial_transport(), |pane| {
                pane.child(self.render_serial_control_bar(cx))
            })
            .when_some(self.pending_paste.clone(), |pane, paste| {
                pane.child(self.render_paste_confirm_overlay(&paste, cx))
            })
            .when_some(self.modem_progress.clone(), |pane, transfer| {
                pane.child(self.render_modem_progress_overlay(transfer, cx))
            })
            .when_some(self.context_menu.clone(), |pane, menu| {
                pane.child(self.render_terminal_context_menu(menu, window, cx))
            })
            .when(self.preferences.show_performance_overlay, |pane| {
                pane.child(self.render_terminal_performance_overlay())
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
    fn drop_retired_images(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for image in self.image_cache.take_retired_images() {
            // GPUI keeps painted RenderImage values in the window sprite atlas
            // until the owner explicitly drops the image id.
            cx.drop_image(image, Some(window));
        }
        for image in self.background_image_cache.take_retired_images() {
            // Background blur images use the same atlas path as terminal images.
            cx.drop_image(image, Some(window));
        }
    }

    fn render_serial_control_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(status) = self.serial_status() else {
            return div().into_any_element();
        };
        let labels = &self.preferences.serial_control_labels;
        let running = status.lifecycle.is_running();
        let lifecycle = serial_lifecycle_label(&status.lifecycle, labels);
        let port_state = match status.port_available {
            Some(true) => labels.port_available.clone(),
            Some(false) => labels.port_missing.clone(),
            None => labels.port_unknown.clone(),
        };
        let dtr_label = format!(
            "{} {}",
            labels.dtr,
            if status.control_state.data_terminal_ready {
                &labels.on
            } else {
                &labels.off
            }
        );
        let rts_label = format!(
            "{} {}",
            labels.rts,
            if status.control_state.request_to_send {
                &labels.on
            } else {
                &labels.off
            }
        );
        let send_mode_label = format!(
            "{} {}",
            labels.send_mode,
            serial_send_mode_label(status.runtime_options.send_mode, labels)
        );
        let display_mode_label = format!(
            "{} {}",
            labels.display_mode,
            serial_display_mode_label(status.runtime_options.display_mode, labels)
        );
        let line_ending_label = format!(
            "{} {}",
            labels.line_ending,
            serial_line_ending_label(status.runtime_options.line_ending, labels)
        );
        let local_echo_label = format!(
            "{} {}",
            labels.local_echo,
            if status.runtime_options.local_echo {
                &labels.on
            } else {
                &labels.off
            }
        );

        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(SERIAL_CONTROL_BAR_HEIGHT))
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(10.0))
            .overflow_x_scrollbar()
            .border_b_1()
            .border_color(rgba(serial_color_alpha(self.theme.foreground, 0x33)))
            .bg(rgba(serial_color_alpha(self.theme.background, 0xf0)))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx: &mut App| {
                cx.stop_propagation();
            })
            .child(
                div()
                    .min_w(px(180.0))
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .truncate()
                    .text_size(px(12.0))
                    .text_color(rgb(self.theme.foreground))
                    .child(format!(
                        "{} · {} · {} · {}",
                        labels.serial,
                        status.config.port_path,
                        serial_config_summary(&status.config, labels),
                        lifecycle
                    )),
            )
            .child(self.render_serial_status_chip(port_state))
            .child(
                self.render_serial_control_button(
                    send_mode_label,
                    true,
                    matches!(status.runtime_options.send_mode, SerialSendMode::Hex),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.cycle_serial_send_mode(cx);
                    }),
                ),
            )
            .child(
                self.render_serial_control_button(
                    display_mode_label,
                    true,
                    !matches!(status.runtime_options.display_mode, SerialDisplayMode::Text),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.cycle_serial_display_mode(cx);
                    }),
                ),
            )
            .child(
                self.render_serial_control_button(
                    line_ending_label,
                    true,
                    !matches!(status.runtime_options.line_ending, SerialLineEnding::None),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.cycle_serial_line_ending(cx);
                    }),
                ),
            )
            .child(
                self.render_serial_control_button(
                    local_echo_label,
                    true,
                    status.runtime_options.local_echo,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.toggle_serial_local_echo(cx);
                    }),
                ),
            )
            .child(
                self.render_serial_control_button(labels.refresh.clone(), true, false)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.refresh_serial_port_presence(cx);
                        }),
                    ),
            )
            .child(
                self.render_serial_control_button(
                    labels.reconnect.clone(),
                    status.can_reconnect,
                    false,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.reconnect_serial(cx);
                    }),
                ),
            )
            .child(
                self.render_serial_control_button(labels.send_break.clone(), running, false)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.send_serial_break(cx);
                        }),
                    ),
            )
            .child(
                self.render_serial_control_button(
                    dtr_label,
                    running,
                    status.control_state.data_terminal_ready,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        let Some(status) = this.serial_status() else {
                            return;
                        };
                        this.set_serial_control_line(
                            SerialControlLine::DataTerminalReady,
                            !status.control_state.data_terminal_ready,
                            cx,
                        );
                    }),
                ),
            )
            .child(
                self.render_serial_control_button(
                    rts_label,
                    running,
                    status.control_state.request_to_send,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        let Some(status) = this.serial_status() else {
                            return;
                        };
                        this.set_serial_control_line(
                            SerialControlLine::RequestToSend,
                            !status.control_state.request_to_send,
                            cx,
                        );
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_serial_status_chip(&self, label: String) -> AnyElement {
        div()
            .flex_none()
            .rounded(px(SERIAL_CONTROL_BUTTON_RADIUS))
            .border_1()
            .border_color(rgba(serial_color_alpha(self.theme.foreground, 0x26)))
            .px(px(9.0))
            .h(px(22.0))
            .flex()
            .items_center()
            .text_size(px(11.0))
            .text_color(rgba(serial_color_alpha(self.theme.foreground, 0xb8)))
            .child(label)
            .into_any_element()
    }

    fn render_serial_control_button(
        &self,
        label: String,
        enabled: bool,
        active: bool,
    ) -> gpui::Div {
        let foreground = if active {
            self.theme.tokens.ui.accent_text
        } else {
            self.theme.foreground
        };
        let background = if active {
            self.theme.tokens.ui.accent
        } else {
            self.theme.background
        };
        let hover_background = if active {
            self.theme.tokens.ui.accent_hover
        } else {
            background
        };
        div()
            .flex_none()
            .rounded(px(SERIAL_CONTROL_BUTTON_RADIUS))
            .border_1()
            .border_color(rgba(serial_color_alpha(self.theme.foreground, 0x2d)))
            .bg(rgba(serial_color_alpha(
                background,
                if active { 0xff } else { 0x00 },
            )))
            .px(px(9.0))
            .h(px(22.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .text_size(px(11.0))
            .text_color(rgba(serial_color_alpha(
                foreground,
                if enabled { 0xff } else { 0x66 },
            )))
            .when(!enabled, |button| button.opacity(0.45))
            .hover(move |button| {
                if enabled {
                    button.bg(rgba(serial_color_alpha(
                        hover_background,
                        if active { 0xff } else { 0x22 },
                    )))
                } else {
                    button
                }
            })
            .child(label)
    }
}

fn serial_lifecycle_label(
    lifecycle: &TerminalLifecycle,
    labels: &TerminalSerialControlLabels,
) -> String {
    match lifecycle {
        TerminalLifecycle::Running => labels.connected.clone(),
        TerminalLifecycle::Exited(_) => labels.disconnected.clone(),
        TerminalLifecycle::Closed => labels.closed.clone(),
    }
}

fn serial_config_summary(
    config: &SerialSessionConfig,
    labels: &TerminalSerialControlLabels,
) -> String {
    format!(
        "{} {}{}{} · {}",
        config.baud_rate,
        config.data_bits,
        serial_parity_letter(config.parity),
        config.stop_bits,
        serial_flow_label(config.flow_control, labels)
    )
}

fn serial_parity_letter(parity: SerialParity) -> &'static str {
    match parity {
        SerialParity::None => "N",
        SerialParity::Odd => "O",
        SerialParity::Even => "E",
    }
}

fn serial_flow_label<'a>(
    flow_control: SerialFlowControl,
    labels: &'a TerminalSerialControlLabels,
) -> &'a str {
    match flow_control {
        SerialFlowControl::None => labels.flow_none.as_str(),
        SerialFlowControl::Software => labels.flow_software.as_str(),
        SerialFlowControl::Hardware => labels.flow_hardware.as_str(),
    }
}

fn serial_send_mode_label<'a>(
    send_mode: SerialSendMode,
    labels: &'a TerminalSerialControlLabels,
) -> &'a str {
    match send_mode {
        SerialSendMode::Text => labels.text_mode.as_str(),
        SerialSendMode::Hex => labels.hex_mode.as_str(),
    }
}

fn serial_display_mode_label<'a>(
    display_mode: SerialDisplayMode,
    labels: &'a TerminalSerialControlLabels,
) -> &'a str {
    match display_mode {
        SerialDisplayMode::Text => labels.text_mode.as_str(),
        SerialDisplayMode::Hex => labels.hex_mode.as_str(),
        SerialDisplayMode::Mixed => labels.mixed_mode.as_str(),
    }
}

fn serial_line_ending_label<'a>(
    line_ending: SerialLineEnding,
    labels: &'a TerminalSerialControlLabels,
) -> &'a str {
    match line_ending {
        SerialLineEnding::Lf => labels.line_ending_lf.as_str(),
        SerialLineEnding::CrLf => labels.line_ending_crlf.as_str(),
        SerialLineEnding::Cr => labels.line_ending_cr.as_str(),
        SerialLineEnding::None => labels.line_ending_none.as_str(),
    }
}

fn serial_color_alpha(rgb_color: u32, alpha: u8) -> u32 {
    (rgb_color << 8) | u32::from(alpha)
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
        let insert_selection_label = self
            .preferences
            .command_selection_labels
            .insert_selection_into_command
            .clone();
        let replace_command_label = self
            .preferences
            .command_selection_labels
            .replace_command_with_selection
            .clone();
        let reconnect_transport_label = self
            .preferences
            .command_selection_labels
            .reconnect_transport
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
        let copy_command_mark_id = command_mark_id;
        let free_type_insert_selection_available =
            self.free_type_context_insert_selection_available(&menu);
        let free_type_replace_command_available =
            self.free_type_context_replace_command_available(&menu);
        let insert_target = menu.target;
        let replace_target = menu.target;

        let tokens = &self.theme.tokens;
        let menu_visible =
            self.context_menu_presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible;
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
                .child(self.render_terminal_context_menu_item(
                    insert_selection_label,
                    !free_type_insert_selection_available,
                    move |this, _event, _window, cx| {
                        this.dismiss_terminal_context_menu(cx);
                        this.insert_selection_into_free_type_command_from_context_menu(
                            insert_target,
                            false,
                            cx,
                        );
                    },
                    cx,
                ))
                .child(self.render_terminal_context_menu_item(
                    replace_command_label,
                    !free_type_replace_command_available,
                    move |this, _event, _window, cx| {
                        this.dismiss_terminal_context_menu(cx);
                        this.insert_selection_into_free_type_command_from_context_menu(
                            replace_target,
                            true,
                            cx,
                        );
                    },
                    cx,
                ))
                .when(self.is_raw_socket_transport(), |content| {
                    content.child(self.render_terminal_context_menu_item(
                        reconnect_transport_label,
                        !self.can_reconnect_raw_socket(),
                        |this, _event, _window, cx| {
                            this.dismiss_terminal_context_menu(cx);
                            this.reconnect_raw_socket(cx);
                        },
                        cx,
                    ))
                })
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
                        .child(oxideterm_gpui_ui::motion::fade(
                            tokens,
                            "terminal-context-menu-presence",
                            overlay_content_boundary(popup),
                            oxideterm_gpui_ui::motion::MotionDuration::Micro,
                            menu_visible,
                        )),
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
        let disabled = disabled
            || self.context_menu_presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Exiting;
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

    pub(super) fn dismiss_terminal_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.context_menu.is_none() {
            return;
        }
        let Some(generation) = self.context_menu_presence.begin_exit() else {
            return;
        };
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.theme.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        );
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this.context_menu_presence.finish_exit(generation) {
                    this.context_menu = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn selected_command_mark(&self) -> Option<TerminalCommandMark> {
        let selected_id = self.selected_command_mark_id.as_deref()?;
        self.command_marks
            .iter()
            .find(|mark| mark.command_id == selected_id)
            .cloned()
    }

    fn render_terminal_performance_overlay(&self) -> AnyElement {
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
    use super::{clamp_terminal_context_menu_position, terminal_pane_base_is_transparent};

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

    #[test]
    fn terminal_pane_base_exposes_window_background_except_during_bell_flash() {
        assert!(terminal_pane_base_is_transparent(false, true));
        assert!(!terminal_pane_base_is_transparent(false, false));
        assert!(!terminal_pane_base_is_transparent(true, true));
    }
}
