use std::{sync::Arc, time::Instant};

use gpui::{
    Anchor, AnchoredPositionMode, AnyElement, App, ClipboardItem, Context, FocusHandle, Focusable,
    FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, Render,
    RenderImage, SharedString, StyledImage, Window, anchored, deferred, div, point, prelude::*, px,
    rgb, rgba,
};
use oxideterm_gpui_ui::context_menu::{
    ContextMenuItemKind, context_menu_action, context_menu_backdrop, context_menu_content,
    context_menu_event_boundary, context_menu_item, context_menu_item_height_estimate,
    context_menu_item_with_shortcut, context_menu_separator,
    context_menu_separator_height_estimate, context_menu_sub_content, context_menu_sub_trigger,
};
use oxideterm_gpui_ui::modal::{TAURI_POPOVER_LAYER_PRIORITY, overlay_content_boundary};
use oxideterm_gpui_ui::progress::progress;
use oxideterm_gpui_ui::scroll::ScrollableElement;
use oxideterm_terminal::{
    DetectedModemProtocol, ModemTransferDirection, SerialControlLine, SerialDisplayMode,
    SerialFlowControl, SerialLineEnding, SerialParity, SerialSendMode, SerialSessionConfig,
    TermMode, TerminalCommandMark, TerminalCursorShape, TerminalLifecycle, TerminalSessionKind,
    TerminalSnapshot, TmuxAction, TmuxUiState,
};
use unicode_width::UnicodeWidthStr;

use super::{
    BACKGROUND_IMAGE_COMPLETION_POLL_INTERVAL, ImageRenderCache, ModemProgressState,
    SmoothScrollSnapshotCache, TerminalCommandNavigationDirection, TerminalContextAction,
    TerminalContextMenu, TerminalPane, TerminalPaneEvent, TmuxPromptKind, TmuxPromptState,
    command_mark_ui_available,
};
use crate::terminal_ui::*;
use crate::terminal_view::*;

const PASTE_PREVIEW_TEXT_RADIUS: f32 = 4.0;
const PASTE_CONFIRM_DIALOG_RADIUS: f32 = 8.0;
const PASTE_CONFIRM_BUTTON_RADIUS: f32 = 4.0;
const TERMINAL_KEY_HINT_RADIUS: f32 = 4.0;
const TERMINAL_CONTEXT_MENU_WIDTH: f32 = 220.0;
const TERMINAL_CONTEXT_MENU_ACTION_COUNT: f32 = 13.0;
const TERMINAL_CONTEXT_MENU_SEPARATOR_COUNT: f32 = 4.0;
const TERMINAL_MODEM_SUBMENU_ACTION_COUNT: f32 = 6.0;
const TERMINAL_CONTEXT_MENU_ACTIONS_BEFORE_MODEM: f32 = 9.0;
const TERMINAL_CONTEXT_MENU_SEPARATORS_BEFORE_MODEM: f32 = 2.0;
const TERMINAL_CONTEXT_MENU_MARGIN: f32 = 8.0;
const SERIAL_CONTROL_BAR_HEIGHT: f32 = 34.0;
const TMUX_CONTROL_BAR_HEIGHT: f32 = 34.0;
const SERIAL_CONTROL_BUTTON_RADIUS: f32 = 999.0;
// Keep diagnostic chrome away from the prompt and command text at the left edge.
const TERMINAL_PERFORMANCE_OVERLAY_INSET: f32 = 8.0;
const TERMINAL_AUTOSUGGEST_MAX_WIDTH: f32 = 520.0;

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

fn clamp_terminal_context_submenu_position(
    menu_left: f32,
    menu_top: f32,
    trigger_top_offset: f32,
    viewport_width: f32,
    viewport_height: f32,
    submenu_width: f32,
    submenu_height: f32,
    margin: f32,
) -> (f32, f32) {
    // Prefer the conventional right edge, then flip to the left when the
    // submenu would cross the window boundary.
    let right_x = menu_left + TERMINAL_CONTEXT_MENU_WIDTH;
    let left_x = menu_left - submenu_width;
    let x = if right_x + submenu_width <= viewport_width - margin {
        right_x
    } else {
        left_x.max(margin)
    };
    let max_y = (viewport_height - submenu_height - margin).max(margin);
    let y = (menu_top + trigger_top_offset).max(margin).min(max_y);
    (x, y)
}

const TERMINAL_VISUAL_BELL_OVERLAY_ALPHA: u8 = 0x66;

fn terminal_pane_base_is_transparent(has_window_background: bool) -> bool {
    // Content-scoped images paint inside the pane above its fallback color.
    // A window-scoped image always needs a transparent pane base, including
    // visual-bell frames, so ordinary shell BEL events cannot hide the image.
    has_window_background
}

fn terminal_visual_bell_overlay_color(bell_background: u32) -> u32 {
    serial_color_alpha(bell_background, TERMINAL_VISUAL_BELL_OVERLAY_ALPHA)
}

fn terminal_cursor_shape_for_render(
    terminal_shape: TerminalCursorShape,
    preferred_shape: TerminalCursorShape,
) -> TerminalCursorShape {
    // Full-screen applications place the cursor in arbitrary scratch cells before hiding it.
    // Preserve that protocol state while still applying the user's shape to visible cursors.
    if terminal_shape == TerminalCursorShape::Hidden {
        TerminalCursorShape::Hidden
    } else {
        preferred_shape
    }
}

impl Focusable for TerminalPane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.metrics_dirty {
            self.metrics = TerminalMetrics::measure_with_preferences(window, &self.preferences);
            self.metrics_dirty = false;
        }
        if self.snapshot_dirty {
            // Hidden panes keep their emulator state current without copying the full grid. The
            // first visible render after activation materializes exactly one latest snapshot.
            let snapshot_started = Instant::now();
            let snapshot = self.terminal.lock().snapshot_incremental(&self.snapshot);
            if snapshot.display_offset == 0 {
                self.clear_smooth_scroll_remainder();
            }
            self.snapshot = self.stamp_snapshot(snapshot);
            self.snapshot_dirty = false;
            self.render_stats.snapshot_micros = snapshot_started
                .elapsed()
                .as_micros()
                .min(u128::from(u64::MAX)) as u64;
        }
        if self.preferences.show_performance_overlay {
            // Element timings are published after prepaint and paint, so the pane displays the
            // most recently completed frame without scheduling an extra diagnostic repaint.
            let performance = self.layout_cache.lock().performance();
            self.render_stats.layout_micros = performance.layout_micros;
            self.render_stats.paint_micros = performance.paint_micros;
            self.render_stats.layout_cache_hit_percent = performance.cache_hit_percent;
        }
        let scrollbar_display_offset = self.smooth_scroll_display_offset();
        let (mut snapshot, smooth_scroll_y_offset, viewport_rows) =
            self.render_snapshot_for_smooth_scroll();
        snapshot.cursor_shape =
            terminal_cursor_shape_for_render(snapshot.cursor_shape, self.preferences.cursor_shape);
        let (terminal_mode, tmux_state) = {
            let terminal = self.terminal.lock();
            (terminal.mode(), terminal.tmux_state())
        };
        let tmux_message = tmux_state.as_ref().and_then(|state| {
            (state.message_generation > self.dismissed_tmux_message_generation)
                .then(|| {
                    state
                        .message
                        .clone()
                        .map(|message| (state.message_generation, message))
                })
                .flatten()
        });
        let terminal_top = if tmux_state.is_some() {
            TMUX_CONTROL_BAR_HEIGHT
        } else if self.is_serial_transport() {
            SERIAL_CONTROL_BAR_HEIGHT
        } else {
            0.0
        };
        let decode_images = self
            .preferences
            .render_policy
            .terminal_graphics
            .decode_images;
        let image_requests = self
            .image_cache
            .take_preparation_requests(&snapshot.images, decode_images);
        if !image_requests.is_empty() {
            let worker_requests = image_requests.clone();
            let preparation_task = cx.background_executor().spawn(async move {
                let started = Instant::now();
                let prepared = worker_requests
                    .into_iter()
                    .filter_map(ImageRenderCache::prepare_snapshot)
                    .collect();
                (prepared, started.elapsed())
            });
            cx.spawn(async move |weak, cx| {
                let (prepared, elapsed) = preparation_task.await;
                let _ = weak.update(cx, |this, cx| {
                    this.image_cache
                        .finish_preparations(&image_requests, prepared);
                    this.render_stats.image_prepare_micros =
                        elapsed.as_micros().min(u128::from(u64::MAX)) as u64;
                    cx.notify();
                });
            })
            .detach();
        }
        let rendered_images = self
            .image_cache
            .cached_images(&snapshot.images, decode_images);
        self.drop_retired_images(window, cx);
        let row_timestamps = self
            .terminal_timestamps_enabled
            .then(|| self.row_timestamps.clone());
        let search_matches = self.current_search_matches();

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
        self.ensure_background_image_completion_poll(cx);
        let transparent_pane_base =
            terminal_pane_base_is_transparent(self.preferences.transparent_background);
        let bell_flash_layer = self.bell_flash.then(|| {
            // Keep the flash above image backgrounds but below terminal text.
            // This preserves visual feedback without replacing the background.
            div()
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .left_0()
                .bg(rgba(terminal_visual_bell_overlay_color(
                    self.theme.bell_background,
                )))
        });
        self.drop_retired_images(window, cx);
        let command_mark_ui_visible =
            command_mark_ui_available(self.settings.command_marks_enabled, terminal_mode);
        if self.command_marks_render_cache_dirty {
            self.command_marks_render_cache = Arc::from(self.command_marks.clone());
            self.command_marks_render_cache_dirty = false;
        }
        let selected_command_mark_id = if command_mark_ui_visible {
            self.selected_command_mark_id.clone()
        } else {
            None
        };
        let hovered_command_mark_id = if command_mark_ui_visible {
            self.hovered_command_mark_id.clone()
        } else {
            None
        };
        let autosuggest_overlay = {
            let candidates = self.terminal_autosuggest_candidates();
            (!candidates.is_empty())
                .then(|| self.render_terminal_autosuggest_overlay(candidates, terminal_top, cx))
        };
        let terminal_element = TerminalElement::new_with_images_and_bidi(
            snapshot,
            rendered_images,
            self.selection.filter(|s| !s.is_empty()),
            self.metrics.clone(),
            self.theme.clone(),
            self.cursor_visible,
            self.marked_text.clone(),
            self.search_query.clone(),
            search_matches,
            self.selected_search_match,
            self.hovered_link.clone(),
            self.settings.bidi_enabled,
            Some(TerminalElementInput {
                focus_handle: self.focus_handle.clone(),
                view: cx.entity(),
                last_viewport_bounds: self.bounds,
                last_viewport_scale_factor_bits: self.viewport_scale_factor_bits,
            }),
        )
        .detect_file_paths_as_links(self.settings.detect_file_paths_as_links)
        .precomputed_search_matches()
        .command_marks(
            if command_mark_ui_visible {
                self.command_marks_render_cache.clone()
            } else {
                Arc::from([])
            },
            selected_command_mark_id,
            hovered_command_mark_id,
        )
        .highlight_rules(self.preferences.highlight_rules.clone())
        .transient_command_highlight(
            self.command_context_highlighting_enabled
                .then(|| self.command_fact_ledger.transient_command_highlight())
                .flatten(),
        )
        .semantic_coloring(
            self.semantic_coloring_enabled() && !terminal_mode.contains(TermMode::ALT_SCREEN),
        )
        .semantic_scheme(self.preferences.semantic_scheme.clone())
        .semantic_shell(self.preferences.semantic_shell)
        .row_timestamps(row_timestamps)
        .transparent_background(background.is_some() || self.preferences.transparent_background)
        .ghost_text(self.terminal_ghost_text())
        .viewport_rows(viewport_rows)
        .scrollbar_display_offset(scrollbar_display_offset)
        .scroll_y_offset(smooth_scroll_y_offset)
        .performance_metrics_enabled(self.preferences.show_performance_overlay)
        .command_mark_gutter_width(if command_mark_ui_visible {
            self.command_mark_gutter_width()
        } else {
            0.0
        })
        .layout_cache(self.layout_cache.clone());
        div()
            .id("terminal-pane")
            .size_full()
            .relative()
            .bg(if transparent_pane_base {
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
                    window.focus(&this.focus_handle, cx);
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
                    window.focus(&this.focus_handle, cx);
                    let mode = this.terminal.lock().mode();
                    if mouse_mode(mode, event.modifiers.shift) {
                        this.handle_mouse_down(event, cx);
                    } else if this.right_click_paste_requested(mode, event.modifiers) {
                        window.prevent_default();
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
            .when_some(bell_flash_layer, |pane, flash| pane.child(flash))
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
            .when_some(tmux_state, |pane, state| {
                pane.child(self.render_tmux_control_bar(&state, cx))
            })
            .when_some(tmux_message, |pane, message| {
                pane.child(self.render_tmux_message(message, cx))
            })
            .when_some(self.tmux_prompt.clone(), |pane, prompt| {
                pane.child(self.render_tmux_prompt_overlay(&prompt, cx))
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
            .when_some(autosuggest_overlay, |pane, overlay| pane.child(overlay))
            .when(self.preferences.show_performance_overlay, |pane| {
                pane.child(self.render_terminal_performance_overlay())
            })
            .when(
                command_mark_ui_visible && self.settings.command_marks_show_hover_actions,
                |pane| {
                    pane.when_some(self.selected_command_mark(), |pane, mark| {
                        pane.child(self.render_command_mark_actions(mark, cx))
                    })
                },
            )
    }
}

impl TerminalPane {
    fn render_terminal_autosuggest_overlay(
        &self,
        candidates: Vec<crate::command_facts::TerminalAutosuggestCandidate>,
        terminal_top: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(anchor) = self.cursor_anchor() else {
            return div().into_any_element();
        };
        let tokens = &self.theme.tokens;
        let popup_margin = tokens.spacing.two;
        let popup_gap = tokens.spacing.one;
        let row_height = tokens.metrics.ui_button_sm_height;
        let popup_padding = tokens.metrics.ui_menu_padding;
        let badge_width = row_height;
        let widest_command_cells = candidates
            .iter()
            .map(|candidate| UnicodeWidthStr::width(candidate.command.as_str()))
            .max()
            .unwrap_or_default();
        let available_width = (anchor.container_width - popup_margin * 2.0).max(0.0);
        let desired_width = widest_command_cells as f32 * anchor.char_width
            + badge_width
            + tokens.metrics.ui_menu_item_padding_x * 2.0
            + popup_padding * 2.0;
        let popup_width = desired_width
            .max(tokens.metrics.ui_menu_min_width.min(available_width))
            .min(TERMINAL_AUTOSUGGEST_MAX_WIDTH.min(available_width));
        let query_width = UnicodeWidthStr::width(self.input_tracker.state().value.as_str()) as f32
            * anchor.char_width;
        let preferred_left = anchor.x - query_width;
        let max_left = (anchor.container_width - popup_width - popup_margin).max(popup_margin);
        let popup_left = preferred_left.max(popup_margin).min(max_left);
        let popup_height = row_height * candidates.len() as f32 + popup_padding * 2.0;
        let cursor_top = terminal_top + anchor.y;
        let container_height = terminal_top + anchor.container_height;
        let max_top = (container_height - popup_height - popup_margin).max(popup_margin);
        let popup_top = if cursor_top - popup_height - popup_gap >= popup_margin {
            cursor_top - popup_height - popup_gap
        } else {
            (cursor_top + anchor.line_height + popup_gap).min(max_top)
        };
        let selected_index = self
            .autosuggest_selected_index
            .filter(|index| *index < candidates.len());
        let history_source_short = self
            .preferences
            .autosuggest_labels
            .history_source
            .chars()
            .next()
            .map(|character| character.to_string())
            .unwrap_or_default();

        let mut list = div()
            .w(px(popup_width))
            .rounded(px(tokens.radii.md))
            .border_1()
            .border_color(rgb(tokens.ui.border_strong))
            .bg(rgb(tokens.ui.bg_elevated))
            .p(px(popup_padding))
            .shadow_lg()
            .overflow_hidden()
            .on_scroll_wheel(|_event, _window, cx| cx.stop_propagation());
        for (index, candidate) in candidates.into_iter().enumerate() {
            let command = candidate.command;
            let command_for_click = command.clone();
            list = list.child(
                div()
                    .id(("terminal-autosuggest-row", index))
                    .h(px(row_height))
                    .min_w_0()
                    .flex()
                    .items_center()
                    .rounded(px(tokens.radii.sm))
                    .cursor_pointer()
                    .when(selected_index == Some(index), |row| {
                        row.bg(rgb(tokens.ui.bg_active))
                    })
                    .hover(|row| row.bg(rgb(tokens.ui.bg_hover)))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .px(px(tokens.metrics.ui_menu_item_padding_x))
                            .truncate()
                            .text_size(px(tokens.metrics.ui_text_sm))
                            .text_color(rgb(tokens.ui.text))
                            .child(command),
                    )
                    .child(
                        div()
                            .w(px(badge_width))
                            .h_full()
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(rgb(tokens.ui.accent))
                            .text_size(px(tokens.metrics.ui_text_xs))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(tokens.ui.accent_text))
                            .child(history_source_short.clone()),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.fill_terminal_autosuggest_command(&command_for_click, false, cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        }

        div()
            .absolute()
            .left(px(popup_left))
            .top(px(popup_top))
            .child(overlay_content_boundary(list))
            .into_any_element()
    }

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

    fn send_tmux_action(&mut self, action: TmuxAction, cx: &mut Context<Self>) {
        if self.terminal.lock().tmux_action(action).unwrap_or(false) {
            self.snapshot_dirty = true;
            cx.notify();
        }
    }

    fn open_tmux_prompt(&mut self, kind: TmuxPromptKind, value: String, cx: &mut Context<Self>) {
        self.tmux_prompt = Some(TmuxPromptState { kind, value });
        self.marked_text = None;
        cx.notify();
    }

    pub(super) fn cancel_tmux_prompt(&mut self, cx: &mut Context<Self>) {
        self.tmux_prompt = None;
        self.marked_text = None;
        cx.notify();
    }

    pub(super) fn submit_tmux_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.tmux_prompt.take() else {
            return;
        };
        let input_is_empty = match &prompt.kind {
            TmuxPromptKind::Command => prompt.value.trim().is_empty(),
            _ => prompt.value.is_empty(),
        };
        if input_is_empty {
            self.tmux_prompt = Some(prompt);
            return;
        }
        let action = match prompt.kind {
            TmuxPromptKind::RenameSession(id) => TmuxAction::RenameSession {
                id,
                name: prompt.value,
            },
            TmuxPromptKind::RenameWindow(id) => TmuxAction::RenameWindow {
                id,
                name: prompt.value,
            },
            TmuxPromptKind::Command => TmuxAction::RunCommand(prompt.value),
        };
        self.marked_text = None;
        self.send_tmux_action(action, cx);
    }

    fn render_tmux_control_bar(&self, state: &TmuxUiState, cx: &mut Context<Self>) -> AnyElement {
        let labels = &self.preferences.tmux_labels;
        let mut controls = Vec::<AnyElement>::new();
        controls.push(self.render_serial_status_chip(if state.ready {
            labels.tmux.clone()
        } else {
            format!("{} · {}", labels.tmux, labels.initializing)
        }));
        for session in &state.sessions {
            let session_id = session.id;
            controls.push(
                self.render_serial_control_button(
                    format!("${session_id} {}", session.name),
                    state.ready,
                    session.active,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.send_tmux_action(TmuxAction::SelectSession(session_id), cx);
                    }),
                )
                .into_any_element(),
            );
        }
        if let Some(session) = state.sessions.iter().find(|session| session.active) {
            let session_id = session.id;
            let session_name = session.name.clone();
            controls.push(
                self.render_serial_control_button(
                    labels.rename_session.clone(),
                    state.ready,
                    false,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.open_tmux_prompt(
                            TmuxPromptKind::RenameSession(session_id),
                            session_name.clone(),
                            cx,
                        );
                    }),
                )
                .into_any_element(),
            );
        }
        for tmux_window in &state.windows {
            let window_id = tmux_window.id;
            controls.push(
                self.render_serial_control_button(
                    format!(
                        "{}:{}{}",
                        tmux_window.index, tmux_window.name, tmux_window.flags
                    ),
                    state.ready,
                    tmux_window.active,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.send_tmux_action(TmuxAction::SelectWindow(window_id), cx);
                    }),
                )
                .into_any_element(),
            );
        }
        if let Some(tmux_window) = state.windows.iter().find(|window| window.active) {
            let window_id = tmux_window.id;
            let window_name = tmux_window.name.clone();
            controls.push(
                self.render_serial_control_button(labels.rename_window.clone(), state.ready, false)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.open_tmux_prompt(
                                TmuxPromptKind::RenameWindow(window_id),
                                window_name.clone(),
                                cx,
                            );
                        }),
                    )
                    .into_any_element(),
            );
        }
        controls.push(
            self.render_serial_control_button(labels.command.clone(), state.ready, false)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.open_tmux_prompt(TmuxPromptKind::Command, String::new(), cx);
                    }),
                )
                .into_any_element(),
        );

        let actions = [
            (
                labels.previous_window.clone(),
                TmuxAction::PreviousWindow,
                true,
            ),
            (labels.next_window.clone(), TmuxAction::NextWindow, true),
            (labels.new_session.clone(), TmuxAction::NewSession, true),
            (
                labels.close_session.clone(),
                TmuxAction::CloseSession,
                state.sessions.len() > 1,
            ),
            (labels.new_window.clone(), TmuxAction::NewWindow, true),
            (
                labels.split_horizontal.clone(),
                TmuxAction::SplitHorizontal,
                true,
            ),
            (
                labels.split_vertical.clone(),
                TmuxAction::SplitVertical,
                true,
            ),
            (
                labels.resize_left.clone(),
                TmuxAction::ResizePaneLeft,
                state.pane_count > 1,
            ),
            (
                labels.resize_right.clone(),
                TmuxAction::ResizePaneRight,
                state.pane_count > 1,
            ),
            (
                labels.resize_up.clone(),
                TmuxAction::ResizePaneUp,
                state.pane_count > 1,
            ),
            (
                labels.resize_down.clone(),
                TmuxAction::ResizePaneDown,
                state.pane_count > 1,
            ),
            (
                labels.close_pane.clone(),
                TmuxAction::ClosePane,
                state.pane_count > 1,
            ),
            (
                labels.close_window.clone(),
                TmuxAction::CloseWindow,
                !state.windows.is_empty(),
            ),
            (labels.detach.clone(), TmuxAction::Detach, true),
        ];
        if state.pane_in_mode {
            controls.push(
                self.render_serial_control_button(labels.cancel_mode.clone(), state.ready, true)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.send_tmux_action(TmuxAction::CancelPaneMode, cx);
                        }),
                    )
                    .into_any_element(),
            );
        }
        for (label, action, enabled) in actions {
            controls.push(
                self.render_serial_control_button(label, state.ready && enabled, false)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            if enabled {
                                this.send_tmux_action(action.clone(), cx);
                            }
                        }),
                    )
                    .into_any_element(),
            );
        }
        if let Some(error) = &state.error {
            controls.push(self.render_serial_status_chip(if error.is_empty() {
                labels.command_failed.clone()
            } else {
                format!("{} · {error}", labels.command_failed)
            }));
        }

        let control_row = div()
            .size_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .px(px(8.0))
            .children(controls);
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(TMUX_CONTROL_BAR_HEIGHT))
            .border_b_1()
            .border_color(rgba(serial_color_alpha(self.theme.foreground, 0x33)))
            .bg(rgba(serial_color_alpha(self.theme.background, 0xf0)))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx: &mut App| {
                cx.stop_propagation();
            })
            .child(div().size_full().overflow_x_scrollbar().child(control_row))
            .into_any_element()
    }

    fn render_tmux_prompt_overlay(
        &self,
        prompt: &TmuxPromptState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let labels = &self.preferences.tmux_labels;
        let title = match prompt.kind {
            TmuxPromptKind::RenameSession(_) => labels.rename_session.clone(),
            TmuxPromptKind::RenameWindow(_) => labels.rename_window.clone(),
            TmuxPromptKind::Command => labels.command_prompt.clone(),
        };
        let placeholder = match prompt.kind {
            TmuxPromptKind::Command => labels.command_placeholder.clone(),
            _ => labels.name_placeholder.clone(),
        };
        let mut value = prompt.value.clone();
        if let Some(marked_text) = &self.marked_text {
            value.push_str(marked_text);
        }
        let input_text = if value.is_empty() { placeholder } else { value };
        let input_color = if prompt.value.is_empty() && self.marked_text.is_none() {
            serial_color_alpha(self.theme.foreground, 0x77)
        } else {
            self.theme.foreground
        };
        let submit_enabled = match &prompt.kind {
            TmuxPromptKind::Command => !prompt.value.trim().is_empty(),
            _ => !prompt.value.is_empty(),
        };

        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000055))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx: &mut App| {
                cx.stop_propagation();
            })
            .child(
                div()
                    .w(px(480.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(rgba(serial_color_alpha(self.theme.foreground, 0x44)))
                    .bg(rgba(serial_color_alpha(self.theme.background, 0xfa)))
                    .shadow_lg()
                    .p(px(16.0))
                    .child(
                        div()
                            .mb(px(12.0))
                            .text_size(px(14.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(title),
                    )
                    .child(
                        div()
                            .h(px(36.0))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(rgba(serial_color_alpha(self.theme.foreground, 0x55)))
                            .bg(rgba(serial_color_alpha(self.theme.background, 0xff)))
                            .text_color(rgb(input_color))
                            .overflow_hidden()
                            .child(input_text),
                    )
                    .child(
                        div()
                            .mt(px(14.0))
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(
                                self.render_serial_control_button(
                                    labels.cancel.clone(),
                                    true,
                                    false,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.cancel_tmux_prompt(cx);
                                    }),
                                ),
                            )
                            .child(
                                self.render_serial_control_button(
                                    labels.confirm.clone(),
                                    submit_enabled,
                                    true,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        if submit_enabled {
                                            this.submit_tmux_prompt(cx);
                                        }
                                    }),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_tmux_message(
        &self,
        (generation, message): (u64, String),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .absolute()
            .top(px(TMUX_CONTROL_BAR_HEIGHT + 8.0))
            .right(px(8.0))
            .max_w(px(520.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(rgba(serial_color_alpha(self.theme.foreground, 0x44)))
            .bg(rgba(serial_color_alpha(self.theme.background, 0xf5)))
            .shadow_lg()
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(div().flex_1().child(message))
            .child(
                div()
                    .cursor_pointer()
                    .text_color(rgba(serial_color_alpha(self.theme.foreground, 0x99)))
                    .child("×")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.dismissed_tmux_message_generation = generation;
                            cx.notify();
                        }),
                    ),
            )
            .into_any_element()
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

        // The scroll wrapper transfers its own styles to the viewport, so the
        // control row must remain a separately styled child to stay horizontal.
        let control_row = div()
            .size_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .px(px(10.0))
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
            );

        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(SERIAL_CONTROL_BAR_HEIGHT))
            .border_b_1()
            .border_color(rgba(serial_color_alpha(self.theme.foreground, 0x33)))
            .bg(rgba(serial_color_alpha(self.theme.background, 0xf0)))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx: &mut App| {
                cx.stop_propagation();
            })
            .child(div().size_full().overflow_x_scrollbar().child(control_row))
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
    fn ensure_background_image_completion_poll(&mut self, cx: &mut Context<Self>) {
        if self.background_image_poll_active || !self.background_image_cache.has_pending() {
            return;
        }
        self.background_image_poll_active = true;
        cx.spawn(async move |weak, cx| {
            loop {
                cx.background_executor()
                    .timer(BACKGROUND_IMAGE_COMPLETION_POLL_INTERVAL)
                    .await;
                let Ok(pending) = weak.update(cx, |this, cx| {
                    let changed = this.background_image_cache.drain_completed();
                    let pending = this.background_image_cache.has_pending();
                    if changed {
                        cx.notify();
                    }
                    if !pending {
                        this.background_image_poll_active = false;
                    }
                    pending
                }) else {
                    break;
                };
                if !pending {
                    break;
                }
            }
        })
        .detach();
    }

    fn render_snapshot_for_smooth_scroll(&mut self) -> (TerminalSnapshot, gpui::Pixels, usize) {
        let snapshot = self.snapshot.clone();
        let viewport_rows = snapshot.rows;
        if !self.settings.smooth_scroll {
            return (snapshot, px(0.0), viewport_rows);
        }

        let line_height = self.metrics.line_height_f32();
        if line_height <= f32::EPSILON {
            return (snapshot, px(0.0), viewport_rows);
        }

        let visual_display_offset = self.smooth_scroll_display_offset();
        let base_display_offset = visual_display_offset.ceil() as usize;
        let y_offset = px((visual_display_offset - base_display_offset as f32) * line_height);
        let needs_overscan = f32::from(y_offset).abs() > f32::EPSILON;
        if base_display_offset == snapshot.display_offset && !needs_overscan {
            return (snapshot, px(0.0), viewport_rows);
        }

        // Absolute visual positioning lets repeated wheel events accumulate without requiring
        // one overscan row per pending line. Only the fractional leading row needs clipping.
        let requested_rows = viewport_rows.saturating_add(usize::from(needs_overscan));
        let animated_snapshot =
            self.smooth_scroll_overscan_snapshot(base_display_offset, requested_rows);
        (animated_snapshot, y_offset, viewport_rows)
    }

    fn smooth_scroll_overscan_snapshot(
        &mut self,
        display_offset: usize,
        rows: usize,
    ) -> TerminalSnapshot {
        if let Some(cached) = &self.smooth_scroll_snapshot_cache
            && cached.source_generation == self.snapshot.generation
            && cached.display_offset == display_offset
            && cached.rows == rows
        {
            return cached.snapshot.clone();
        }
        let snapshot_started = self.preferences.show_performance_overlay.then(Instant::now);
        let snapshot = self
            .terminal
            .lock()
            .snapshot_with_display_offset(display_offset, rows);
        if let Some(snapshot_started) = snapshot_started {
            // Count only cache misses because cached overscan snapshots do not rebuild terminal
            // rows and therefore do not represent smooth-scroll snapshot work.
            self.render_stats.scroll_snapshot_micros = snapshot_started
                .elapsed()
                .as_micros()
                .min(u128::from(u64::MAX))
                as u64;
            self.render_stats.scroll_snapshot_count =
                self.render_stats.scroll_snapshot_count.saturating_add(1);
        }
        self.smooth_scroll_snapshot_cache = Some(SmoothScrollSnapshotCache {
            source_generation: self.snapshot.generation,
            display_offset,
            rows,
            snapshot: snapshot.clone(),
        });
        snapshot
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
        let find_label = self.preferences.command_selection_labels.find.clone();
        let manage_triggers_label = self
            .preferences
            .command_selection_labels
            .manage_triggers
            .clone();
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
        let clear_screen_shortcut = self
            .preferences
            .command_selection_labels
            .clear_screen_shortcut
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
        let modem_submenu_open = menu.modem_submenu_open;
        let tokens = &self.theme.tokens;
        let menu_visible =
            self.context_menu_presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        let submenu_height = tokens.metrics.ui_menu_padding * 2.0
            + TERMINAL_MODEM_SUBMENU_ACTION_COUNT * context_menu_item_height_estimate(tokens);
        let modem_trigger_top_offset = tokens.metrics.ui_menu_padding
            + TERMINAL_CONTEXT_MENU_ACTIONS_BEFORE_MODEM
                * context_menu_item_height_estimate(tokens)
            + TERMINAL_CONTEXT_MENU_SEPARATORS_BEFORE_MODEM
                * context_menu_separator_height_estimate(tokens);
        let viewport = window.viewport_size();
        let (submenu_left, submenu_top) = clamp_terminal_context_submenu_position(
            left,
            top,
            modem_trigger_top_offset,
            f32::from(viewport.width),
            f32::from(viewport.height),
            TERMINAL_CONTEXT_MENU_WIDTH,
            submenu_height,
            TERMINAL_CONTEXT_MENU_MARGIN,
        );
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
                .child(self.render_terminal_context_menu_item(
                    manage_triggers_label,
                    false,
                    |this, _event, _window, cx| {
                        this.request_context_action(
                            TerminalContextAction::OpenSessionTriggers,
                            false,
                            cx,
                        );
                    },
                    cx,
                ))
                .child(context_menu_separator(tokens))
                .child(
                    self.render_terminal_context_submenu_trigger(modem_labels.binary_transfer, cx),
                )
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
                .child(self.render_terminal_context_menu_item_with_shortcut(
                    clear_screen_label,
                    clear_screen_shortcut,
                    false,
                    |this, _event, _window, cx| {
                        this.dismiss_terminal_context_menu(cx);
                        this.clear_buffer(cx);
                    },
                    cx,
                )),
        );

        let modem_submenu = modem_submenu_open.then(|| {
            context_menu_event_boundary(
                context_menu_sub_content(tokens)
                    .w(px(TERMINAL_CONTEXT_MENU_WIDTH))
                    .child(self.render_terminal_modem_context_menu_item(
                        modem_labels.xmodem_upload,
                        DetectedModemProtocol::Xmodem,
                        ModemTransferDirection::Upload,
                        cx,
                    ))
                    .child(self.render_terminal_modem_context_menu_item(
                        modem_labels.xmodem_receive,
                        DetectedModemProtocol::Xmodem,
                        ModemTransferDirection::Download,
                        cx,
                    ))
                    .child(self.render_terminal_modem_context_menu_item(
                        modem_labels.ymodem_upload,
                        DetectedModemProtocol::Ymodem,
                        ModemTransferDirection::Upload,
                        cx,
                    ))
                    .child(self.render_terminal_modem_context_menu_item(
                        modem_labels.ymodem_receive,
                        DetectedModemProtocol::Ymodem,
                        ModemTransferDirection::Download,
                        cx,
                    ))
                    .child(self.render_terminal_modem_context_menu_item(
                        modem_labels.zmodem_upload,
                        DetectedModemProtocol::Zmodem,
                        ModemTransferDirection::Upload,
                        cx,
                    ))
                    .child(self.render_terminal_modem_context_menu_item(
                        modem_labels.zmodem_receive,
                        DetectedModemProtocol::Zmodem,
                        ModemTransferDirection::Download,
                        cx,
                    )),
            )
        });

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
                        .anchor(Anchor::TopLeft)
                        .position(point(px(left), px(top)))
                        .position_mode(AnchoredPositionMode::Window)
                        .child(oxideterm_gpui_ui::motion::fade(
                            tokens,
                            "terminal-context-menu-presence",
                            overlay_content_boundary(popup),
                            oxideterm_gpui_ui::motion::MotionDuration::Micro,
                            menu_visible,
                        )),
                )
                .when_some(modem_submenu, |backdrop, submenu| {
                    backdrop.child(
                        anchored()
                            .anchor(Anchor::TopLeft)
                            .position(point(px(submenu_left), px(submenu_top)))
                            .position_mode(AnchoredPositionMode::Window)
                            .child(overlay_content_boundary(submenu)),
                    )
                }),
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
        self.render_terminal_context_menu_item_with_submenu_policy(
            label, disabled, true, listener, cx,
        )
    }

    fn render_terminal_context_menu_item_with_shortcut(
        &self,
        label: String,
        shortcut: Option<String>,
        disabled: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let disabled = disabled
            || self.context_menu_presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Exiting;
        let item = if let Some(shortcut) = shortcut {
            context_menu_item_with_shortcut(
                &self.theme.tokens,
                label,
                div()
                    .text_size(px(self.theme.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.theme.tokens.ui.text_muted))
                    .child(shortcut),
            )
        } else {
            context_menu_item(
                &self.theme.tokens,
                label,
                ContextMenuItemKind::Plain,
                false,
                disabled,
            )
        }
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
        .on_mouse_move(cx.listener(|this, _event: &MouseMoveEvent, _window, cx| {
            this.set_terminal_modem_submenu_open(false, cx);
        }))
        .into_any_element()
    }

    fn render_terminal_context_menu_item_with_submenu_policy(
        &self,
        label: String,
        disabled: bool,
        close_modem_submenu_on_hover: bool,
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
        .when(close_modem_submenu_on_hover, |item| {
            item.on_mouse_move(cx.listener(|this, _event: &MouseMoveEvent, _window, cx| {
                this.set_terminal_modem_submenu_open(false, cx);
            }))
        })
        .into_any_element()
    }

    fn render_terminal_context_submenu_trigger(
        &self,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let disabled =
            self.context_menu_presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Exiting;
        let trigger = context_menu_sub_trigger(&self.theme.tokens, label, false, disabled).w_full();

        context_menu_action(
            trigger,
            disabled,
            false,
            cx.listener(|this, _event, window, cx| {
                window.prevent_default();
                this.set_terminal_modem_submenu_open(true, cx);
                cx.stop_propagation();
            }),
        )
        .on_mouse_move(cx.listener(|this, _event: &MouseMoveEvent, _window, cx| {
            this.set_terminal_modem_submenu_open(true, cx);
        }))
        .into_any_element()
    }

    fn render_terminal_modem_context_menu_item(
        &self,
        label: String,
        protocol: DetectedModemProtocol,
        direction: ModemTransferDirection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_terminal_context_menu_item_with_submenu_policy(
            label,
            false,
            false,
            move |this, _event, _window, cx| {
                this.dismiss_terminal_context_menu(cx);
                this.start_manual_modem_transfer(protocol, direction, cx);
            },
            cx,
        )
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
        cx.emit(TerminalPaneEvent::ContextActionRequested);
        cx.notify();
    }

    fn set_terminal_modem_submenu_open(&mut self, open: bool, cx: &mut Context<Self>) {
        let Some(menu) = self.context_menu.as_mut() else {
            return;
        };
        if menu.modem_submenu_open == open {
            return;
        }
        // Submenu visibility belongs to the live context-menu instance so a
        // newly opened terminal menu never inherits stale expansion state.
        menu.modem_submenu_open = open;
        cx.notify();
    }

    pub(super) fn dismiss_terminal_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.context_menu.is_none() {
            return;
        }
        self.set_terminal_modem_submenu_open(false, cx);
        let Some(generation) = self.context_menu_presence.begin_exit() else {
            return;
        };
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.theme.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        );
        cx.spawn(async move |weak, cx| {
            cx.background_executor().timer(delay).await;
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
        let backend = match self.session_kind() {
            TerminalSessionKind::LocalPty => "L",
            TerminalSessionKind::SshPty => "SSH",
            TerminalSessionKind::Telnet => "TN",
            TerminalSessionKind::Mosh => "M",
            TerminalSessionKind::Serial => "SER",
        };
        div()
            .absolute()
            .top(px(TERMINAL_PERFORMANCE_OVERLAY_INSET))
            .right(px(TERMINAL_PERFORMANCE_OVERLAY_INSET))
            .flex()
            .flex_col()
            .items_start()
            .px(px(8.0))
            .py(px(2.0))
            .rounded(px(4.0))
            .border_1()
            .border_color(rgba(0xffffff33))
            .bg(rgba(0x0d0f12dd))
            .text_size(px(10.0))
            .font_family(SharedString::from(self.preferences.font_family.clone()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .line_height(px(16.0))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(stats.tier.color()))
                            .child(stats.tier.label()),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgba(0xe6e8eb99))
                            .child(backend),
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
                    ),
            )
            .child(
                div()
                    .line_height(px(16.0))
                    .text_color(rgba(0xe6e8eb99))
                    .child(format!(
                        "d{} d95{} db{} dc{} dp{} dl{} l95{}us",
                        stats.drain_micros,
                        stats.drain_p95_micros,
                        stats.drained_bytes,
                        stats.max_data_chunk_bytes,
                        stats.output_processing_micros,
                        stats.terminal_lock_wait_micros,
                        stats.input_latency_p95_micros,
                    )),
            )
            .child(
                div()
                    .line_height(px(16.0))
                    .text_color(rgba(0xe6e8eb99))
                    .child(format!(
                        "s{} ly{} p{} c{}% ss{}#{} q{} i{}",
                        stats.snapshot_micros,
                        stats.layout_micros,
                        stats.paint_micros,
                        stats.layout_cache_hit_percent,
                        stats.scroll_snapshot_micros,
                        stats.scroll_snapshot_count,
                        stats.search_micros,
                        stats.image_prepare_micros,
                    )),
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

#[cfg(test)]
mod tests {
    use oxideterm_terminal::TerminalCursorShape;

    use super::{
        TERMINAL_VISUAL_BELL_OVERLAY_ALPHA, terminal_cursor_shape_for_render,
        terminal_pane_base_is_transparent, terminal_visual_bell_overlay_color,
    };

    #[test]
    fn terminal_pane_base_keeps_window_background_visible_during_visual_bell() {
        assert!(terminal_pane_base_is_transparent(true));
        assert!(!terminal_pane_base_is_transparent(false));
        assert_eq!(
            terminal_visual_bell_overlay_color(0x17131a) & 0xff,
            u32::from(TERMINAL_VISUAL_BELL_OVERLAY_ALPHA)
        );
    }

    #[test]
    fn terminal_application_can_hide_the_configured_cursor() {
        assert_eq!(
            terminal_cursor_shape_for_render(TerminalCursorShape::Hidden, TerminalCursorShape::Bar),
            TerminalCursorShape::Hidden
        );
        assert_eq!(
            terminal_cursor_shape_for_render(TerminalCursorShape::Block, TerminalCursorShape::Bar),
            TerminalCursorShape::Bar
        );
    }
}
