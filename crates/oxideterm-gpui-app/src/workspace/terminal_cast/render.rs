use super::*;
use crate::workspace::ime::WorkspaceImeTarget;
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
    ToolbarButtonOptions, icon_button, toolbar_button,
};
use oxideterm_gpui_ui::text_input::{text_caret, text_input_anchor_probe};
use oxideterm_terminal_recording::{format_cast_time, format_recording_elapsed};

fn terminal_cast_player_button(tokens: &ThemeTokens, icon: LucideIcon) -> gpui::Div {
    icon_button(
        tokens,
        WorkspaceApp::render_lucide_icon(icon, 15.0, rgb(tokens.ui.text)),
        IconButtonOptions {
            size: 30.0,
            radius: ButtonRadius::Md,
            background: Some(rgba((tokens.ui.bg_panel << 8) | 0xcc)),
            border: Some(rgb(tokens.ui.border)),
            hover_background: Some(rgb(tokens.ui.bg_hover)),
            idle_opacity: 1.0,
            ..IconButtonOptions::compact(30.0)
        },
    )
}

fn terminal_cast_speed_button(
    tokens: &ThemeTokens,
    label: &'static str,
    active: bool,
) -> gpui::Div {
    let background = if active {
        rgba((tokens.ui.accent << 8) | 0x1f)
    } else {
        rgba((tokens.ui.bg_panel << 8) | 0xcc)
    };
    // Playback speed chips borrow outline-button geometry but use cast-player
    // active colors from Tauri, so keep only the feature color contract local.
    toolbar_button(
        tokens,
        label.to_string(),
        None,
        ToolbarButtonOptions {
            button: ButtonOptions {
                variant: ButtonVariant::Outline,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled: false,
            },
            height: Some(30.0),
            padding_x: Some(10.0),
            font_size: Some(12.0),
            background: Some(background),
            border: Some(if active {
                rgb(tokens.ui.accent)
            } else {
                rgb(tokens.ui.border)
            }),
            text_color: Some(if active {
                rgb(tokens.ui.accent)
            } else {
                rgb(tokens.ui.text_muted)
            }),
            hover_background: Some(background),
            ..ToolbarButtonOptions::default()
        },
    )
}

fn terminal_cast_text_button(tokens: &ThemeTokens, label: &'static str) -> gpui::Div {
    toolbar_button(
        tokens,
        label.to_string(),
        None,
        ToolbarButtonOptions {
            button: ButtonOptions {
                variant: ButtonVariant::Outline,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled: false,
            },
            height: Some(30.0),
            padding_x: Some(10.0),
            font_size: Some(12.0),
            background: Some(rgba((tokens.ui.bg_panel << 8) | 0xcc)),
            border: Some(rgb(tokens.ui.border)),
            text_color: Some(rgb(tokens.ui.text_muted)),
            hover_background: Some(rgb(tokens.ui.bg_hover)),
            ..ToolbarButtonOptions::default()
        },
    )
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_terminal_recording_controls(
        &self,
        status: TerminalRecordingStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let is_paused = status.state == TerminalRecordingState::Paused;
        div()
            .absolute()
            .top(px(12.0))
            .right(px(12.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded_lg()
            .border_1()
            .border_color(rgba(0xef444459))
            .bg(rgba((theme.bg_elevated << 8) | 0xe6))
            .px(px(10.0))
            .py(px(6.0))
            .shadow_lg()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(12.0))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_color(rgba(0xfca5a5ff))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Circle,
                        9.0,
                        if is_paused {
                            rgb(theme.text_muted)
                        } else {
                            rgba(0xf87171ff)
                        },
                    ))
                    .child(if is_paused {
                        self.i18n.t("terminal.recording.paused")
                    } else {
                        self.i18n.t("terminal.recording.recording")
                    })
                    .child(format_recording_elapsed(status.elapsed)),
            )
            .child(
                terminal_cast_player_button(
                    &self.tokens,
                    if is_paused {
                        LucideIcon::Play
                    } else {
                        LucideIcon::Pause
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if is_paused {
                            this.resume_active_terminal_recording(cx);
                        } else {
                            this.pause_active_terminal_recording(cx);
                        }
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                terminal_cast_player_button(&self.tokens, LucideIcon::Square).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.stop_active_terminal_recording(cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                terminal_cast_player_button(&self.tokens, LucideIcon::Trash2).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.discard_active_terminal_recording(cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_terminal_cast_player(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let player = self.terminal_cast_player.as_ref()?;
        const PLAYER_PANEL_ALPHA: u32 = 0xf2;
        const PLAYER_BORDER_ALPHA: u32 = 0x99;
        let theme = self.tokens.ui;
        let recording = player.playback.recording();
        let position = player.playback.position();
        let duration = recording.duration;
        let progress = if duration <= 0.0 {
            0.0
        } else {
            (position / duration).clamp(0.0, 1.0) as f32
        };
        let search_target = WorkspaceImeTarget::TerminalCastSearch;
        let search_marked = self.marked_text_for_target(search_target);
        let search_empty = player.search_query.is_empty() && search_marked.is_none();
        let search_text = if search_empty {
            self.i18n.t("terminal.recording.search_placeholder")
        } else {
            player.search_query.clone()
        };
        let search_results = player.playback.search(&player.search_query);
        let pane = player.pane.clone();
        let workspace = cx.entity();
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .flex()
                .flex_col()
                .bg(rgba((theme.bg_sunken << 8) | PLAYER_PANEL_ALPHA))
                .child(
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .bg(rgba((theme.bg_sunken << 8) | PLAYER_PANEL_ALPHA))
                        .child(
                            div()
                                .h(px(48.0))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(12.0))
                                .px(px(16.0))
                                .border_b_1()
                                .border_color(rgba((theme.border << 8) | PLAYER_BORDER_ALPHA))
                                .child(
                                    div()
                                        .min_w(px(0.0))
                                        .flex_1()
                                        .flex()
                                        .items_center()
                                        .gap(px(12.0))
                                        .child(
                                            div()
                                                .max_w(px(400.0))
                                                .truncate()
                                                .text_size(px(14.0))
                                                .text_color(rgb(theme.text))
                                                .child(recording.file_name.clone()),
                                        )
                                        .child(
                                            div()
                                                .flex_none()
                                                .text_size(px(11.0))
                                                .font_family(settings_mono_font_family(
                                                    self.settings_store.settings(),
                                                ))
                                                .text_color(rgb(theme.text_muted))
                                                .child(format!(
                                                    "{}x{}",
                                                    recording.width, recording.height
                                                )),
                                        ),
                                )
                                .child(
                                    div()
                                        .size(px(28.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .bg(if player.search_visible {
                                            rgb(theme.bg_hover)
                                        } else {
                                            rgba(0x00000000)
                                        })
                                        .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, window, cx| {
                                                if let Some(player) =
                                                    this.terminal_cast_player.as_mut()
                                                {
                                                    player.search_visible = !player.search_visible;
                                                    player.search_focused = player.search_visible;
                                                    if !player.search_visible {
                                                        player.search_query.clear();
                                                        this.update_terminal_cast_search(cx);
                                                    }
                                                }
                                                this.terminal_command_bar_focused = false;
                                                this.ime_marked_text = None;
                                                window.focus(&this.focus_handle);
                                                cx.stop_propagation();
                                                cx.notify();
                                            }),
                                        )
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::Search,
                                            16.0,
                                            if player.search_visible {
                                                rgb(theme.text)
                                            } else {
                                                rgb(theme.text_muted)
                                            },
                                        )),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgb(theme.text_muted))
                                        .when(!player.search_query.is_empty(), |label| {
                                            label.child(format!(
                                                "{} {}",
                                                search_results.len(),
                                                self.i18n.t("terminal.recording.matches")
                                            ))
                                        }),
                                )
                                .child(
                                    div()
                                        .size(px(28.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.close_terminal_cast_player(cx);
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::X,
                                            16.0,
                                            rgb(theme.text_muted),
                                        )),
                                ),
                        )
                        .when(player.search_visible, |player_view| {
                            player_view.child(
                                div()
                                    .flex_none()
                                    .border_b_1()
                                    .border_color(rgba((theme.border << 8) | PLAYER_BORDER_ALPHA))
                                    .bg(rgba((theme.bg_panel << 8) | 0x99))
                                    .px(px(16.0))
                                    .py(px(8.0))
                                    .child(
                                        div()
                                            .max_w(px(512.0))
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::Search,
                                                14.0,
                                                rgb(theme.text_muted),
                                            ))
                                            .child(text_input_anchor_probe(
                                                search_target.anchor_id(),
                                                div()
                                                    .h(px(30.0))
                                                    .flex_1()
                                                    .min_w(px(0.0))
                                                    .flex()
                                                    .items_center()
                                                    .rounded_md()
                                                    .border_1()
                                                    .border_color(if player.search_focused {
                                                        rgba((theme.accent << 8) | 0x80)
                                                    } else {
                                                        rgba((theme.border << 8) | 0x80)
                                                    })
                                                    .bg(rgba((theme.bg_hover << 8) | 0x99))
                                                    .px(px(8.0))
                                                    .text_size(px(13.0))
                                                    .text_color(if search_empty {
                                                        rgb(theme.text_muted)
                                                    } else {
                                                        rgb(theme.text)
                                                    })
                                                    .cursor_text()
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(
                                                            move |this, event: &gpui::MouseDownEvent, window, cx| {
                                                                if let Some(player) = this
                                                                    .terminal_cast_player
                                                                    .as_mut()
                                                                {
                                                                    player.search_focused = true;
                                                                    player.search_visible = true;
                                                                }
                                                                this.terminal_command_bar_focused =
                                                                    false;
                                                                this.ime_marked_text = None;
                                                                window.focus(&this.focus_handle);
                                                                this.begin_ime_selection_from_mouse_down(search_target, event, window, cx);
                                                                cx.stop_propagation();
                                                                cx.notify();
                                                            },
                                                        ),
                                                    )
                                                    .on_mouse_move(cx.listener(
                                                        |this, event: &gpui::MouseMoveEvent, window, cx| {
                                                            this.update_ime_selection_drag_from_mouse_move(
                                                                event, window, cx,
                                                            );
                                                        },
                                                    ))
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .overflow_hidden()
                                                            .child(search_text)
                                                            .when_some(
                                                                search_marked,
                                                                |input, marked| {
                                                                    input.child(
                                                                        div()
                                                                            .underline()
                                                                            .text_color(rgb(
                                                                                theme.text,
                                                                            ))
                                                                            .child(
                                                                                marked.to_string(),
                                                                            ),
                                                                    )
                                                                },
                                                            )
                                                            .when(
                                                                player.search_focused,
                                                                |input| {
                                                                    input.child(text_caret(
                                                                        &self.tokens,
                                                                        self.new_connection_caret_visible,
                                                                    ))
                                                                },
                                                            ),
                                                    ),
                                                move |anchor, _window, cx| {
                                                    let _ = workspace.update(cx, |this, cx| {
                                                        this.update_text_input_anchor(anchor, cx);
                                                    });
                                                },
                                            ))
                                            .when(!player.search_query.is_empty(), |row| {
                                                row.child(
                                                    div()
                                                        .flex_none()
                                                        .text_size(px(12.0))
                                                        .text_color(rgb(theme.text_muted))
                                                        .child(format!(
                                                            "{} {}",
                                                            search_results.len(),
                                                            self.i18n
                                                                .t("terminal.recording.matches")
                                                        )),
                                                )
                                            }),
                                    ),
                            )
                        })
                        .when(!player.search_query.is_empty(), |player_view| {
                            player_view.child(
                                div()
                                    .flex_none()
                                    .max_h(px(118.0))
                                    .overflow_hidden()
                                    .border_b_1()
                                    .border_color(rgba((theme.border << 8) | PLAYER_BORDER_ALPHA))
                                    .bg(rgba((theme.bg_panel << 8) | 0x99))
                                    .px(px(16.0))
                                    .py(px(8.0))
                                    .when(search_results.is_empty(), |panel| {
                                        panel.child(
                                            div()
                                                .text_size(px(12.0))
                                                .text_color(rgb(theme.text_muted))
                                                .child(
                                                    self.i18n
                                                        .t("terminal.recording.search_no_results"),
                                                ),
                                        )
                                    })
                                    .when(!search_results.is_empty(), |panel| {
                                        panel.child(div().flex().flex_col().gap(px(2.0)).children(
                                            search_results.iter().map(|result| {
                                                let at = result.at;
                                                let snippet = result.snippet.clone();
                                                div()
                                                    .h(px(22.0))
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(8.0))
                                                    .rounded_md()
                                                    .px(px(6.0))
                                                    .cursor_pointer()
                                                    .hover(move |style| {
                                                        style.bg(rgb(theme.bg_hover))
                                                    })
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(
                                                            move |this, _event, _window, cx| {
                                                                if let Some(player) =
                                                                    &this.terminal_cast_player
                                                                {
                                                                    this.seek_terminal_cast(
                                                                        at / player
                                                                            .playback
                                                                            .recording()
                                                                            .duration
                                                                            .max(1.0),
                                                                        cx,
                                                                    );
                                                                }
                                                                cx.stop_propagation();
                                                            },
                                                        ),
                                                    )
                                                    .child(
                                                        div()
                                                            .w(px(48.0))
                                                            .text_size(px(11.0))
                                                            .font_family(settings_mono_font_family(
                                                                self.settings_store.settings(),
                                                            ))
                                                            .text_color(rgb(theme.text_muted))
                                                            .child(format_cast_time(at)),
                                                    )
                                                    .child(Self::render_lucide_icon(
                                                        LucideIcon::ChevronRight,
                                                        12.0,
                                                        rgb(theme.text_muted),
                                                    ))
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .min_w(px(0.0))
                                                            .truncate()
                                                            .text_size(px(11.0))
                                                            .font_family(settings_mono_font_family(
                                                                self.settings_store.settings(),
                                                            ))
                                                            .text_color(rgb(theme.text_muted))
                                                            .child(snippet),
                                                    )
                                            }),
                                        ))
                                    }),
                            )
                        })
                        .child(div().flex_1().min_h(px(0.0)).child(
                            pane.map(|pane| pane.into_any_element()).unwrap_or_else(|| {
                                div()
                                    .size_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("terminal.recording.player_empty"))
                                    .into_any_element()
                            }),
                        ))
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .flex_col()
                                .gap(px(10.0))
                                .p(px(14.0))
                                .border_t_1()
                                .border_color(rgba((theme.border << 8) | PLAYER_BORDER_ALPHA))
                                .child(select_anchor_probe(
                                    SelectAnchorId::TerminalCastSeekbar,
                                    div()
                                        .h(px(10.0))
                                        .flex()
                                        .items_center()
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(
                                                |this, event: &MouseDownEvent, _window, cx| {
                                                    this.terminal_cast_seek_dragging = true;
                                                    this.apply_terminal_cast_seek_from_x(
                                                        f32::from(event.position.x),
                                                        cx,
                                                    );
                                                    cx.stop_propagation();
                                                },
                                            ),
                                        )
                                        .child(
                                            div()
                                                .h(px(6.0))
                                                .w_full()
                                                .rounded_full()
                                                .overflow_hidden()
                                                .bg(rgba((theme.bg_panel << 8) | 0xcc))
                                                .child(
                                                    div()
                                                        .h_full()
                                                        .w(relative(progress))
                                                        .rounded_full()
                                                        .bg(rgb(theme.accent)),
                                                ),
                                        ),
                                    {
                                        let workspace = cx.entity();
                                        move |anchor, _window, cx| {
                                            let _ = workspace.update(cx, |this, _cx| {
                                                this.select_anchors.insert(anchor.id, anchor);
                                            });
                                        }
                                    },
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap(px(10.0))
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .child(
                                                    terminal_cast_player_button(
                                                        &self.tokens,
                                                        if player.playback.playing() {
                                                            LucideIcon::Pause
                                                        } else {
                                                            LucideIcon::Play
                                                        },
                                                    )
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _event, _window, cx| {
                                                            this.toggle_terminal_cast_playback(cx);
                                                            cx.stop_propagation();
                                                        }),
                                                    ),
                                                )
                                                .child(
                                                    terminal_cast_speed_button(
                                                        &self.tokens,
                                                        "0.5x",
                                                        player.playback.speed() == 0.5,
                                                    )
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _event, _window, cx| {
                                                            this.set_terminal_cast_speed(0.5, cx);
                                                            cx.stop_propagation();
                                                        }),
                                                    ),
                                                )
                                                .child(
                                                    terminal_cast_speed_button(
                                                        &self.tokens,
                                                        "1x",
                                                        player.playback.speed() == 1.0,
                                                    )
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _event, _window, cx| {
                                                            this.set_terminal_cast_speed(1.0, cx);
                                                            cx.stop_propagation();
                                                        }),
                                                    ),
                                                )
                                                .child(
                                                    terminal_cast_speed_button(
                                                        &self.tokens,
                                                        "2x",
                                                        player.playback.speed() == 2.0,
                                                    )
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _event, _window, cx| {
                                                            this.set_terminal_cast_speed(2.0, cx);
                                                            cx.stop_propagation();
                                                        }),
                                                    ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .child(
                                                    terminal_cast_text_button(&self.tokens, "-10s")
                                                        .on_mouse_down(
                                                            MouseButton::Left,
                                                            cx.listener(
                                                                |this, _event, _window, cx| {
                                                                    if let Some(player) =
                                                                        &this.terminal_cast_player
                                                                    {
                                                                        let target = (player
                                                                            .playback
                                                                            .position()
                                                                            - 10.0)
                                                                            / player
                                                                                .playback
                                                                                .recording()
                                                                                .duration
                                                                                .max(1.0);
                                                                        this.seek_terminal_cast(
                                                                            target, cx,
                                                                        );
                                                                    }
                                                                    cx.stop_propagation();
                                                                },
                                                            ),
                                                        ),
                                                )
                                                .child(
                                                    terminal_cast_text_button(&self.tokens, "+10s")
                                                        .on_mouse_down(
                                                            MouseButton::Left,
                                                            cx.listener(
                                                                |this, _event, _window, cx| {
                                                                    if let Some(player) =
                                                                        &this.terminal_cast_player
                                                                    {
                                                                        let target = (player
                                                                            .playback
                                                                            .position()
                                                                            + 10.0)
                                                                            / player
                                                                                .playback
                                                                                .recording()
                                                                                .duration
                                                                                .max(1.0);
                                                                        this.seek_terminal_cast(
                                                                            target, cx,
                                                                        );
                                                                    }
                                                                    cx.stop_propagation();
                                                                },
                                                            ),
                                                        ),
                                                ),
                                        ),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}
