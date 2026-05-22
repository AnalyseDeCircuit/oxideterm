use super::actions::TerminalBroadcastMenuPlacement;
use super::ime::WorkspaceImeTarget;
use super::*;
use oxideterm_gpui_ui::context_menu::{ContextMenuActionableStyle, context_menu_event_boundary};
use oxideterm_gpui_ui::text_input::{text_caret, text_input_anchor_probe};
use oxideterm_terminal_recording::format_recording_elapsed;

pub(in crate::workspace) mod completion;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_terminal_surface(
        &self,
        root_pane: &PaneNode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let terminal = self.render_pane_tree(root_pane, cx);
        let recording_status = self.active_terminal_recording_status(cx);
        let recording_active = recording_status.state != TerminalRecordingState::Idle;
        if !self.settings_store.settings().terminal.command_bar.enabled {
            return div()
                .size_full()
                .relative()
                .child(terminal)
                .when(recording_active, |surface| {
                    surface.child(self.render_terminal_recording_controls(recording_status, cx))
                })
                .into_any_element();
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(terminal)
                    .when(recording_active, |surface| {
                        surface.child(self.render_terminal_recording_controls(recording_status, cx))
                    }),
            )
            .child(self.render_terminal_command_bar(cx))
            .into_any_element()
    }

    fn render_terminal_command_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        const COMMAND_BAR_BG_ALPHA: u32 = 0xf2; // Tauri bg-theme-bg/95
        const COMMAND_BAR_BORDER_ALPHA: u32 = 0xb3; // Tauri border-theme-border/70
        const COMMAND_BAR_INPUT_BORDER_ALPHA: u32 = 0x73; // Tauri border-theme-border/45
        const COMMAND_BAR_FOCUSED_BORDER_ALPHA: u32 = 0x73; // Tauri border-theme-accent/45

        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::TerminalCommandBar;
        let workspace = cx.entity();
        let focused = self.terminal_command_bar_focused;
        let marked_text = self.marked_text_for_target(target);
        let command_is_empty = self.terminal_command_bar_draft.is_empty();
        let command_suggestions = if focused {
            self.terminal_command_bar_suggestions(false, cx)
        } else {
            Vec::new()
        };
        let ghost_text = self.terminal_command_ghost_text(&command_suggestions);
        let showing_placeholder = command_is_empty && marked_text.is_none();
        let command_text = if showing_placeholder {
            self.i18n.t("terminal.command_bar.command_placeholder")
        } else {
            self.terminal_command_bar_draft.clone()
        };
        let target_label = self
            .active_tab()
            .map(|tab| match tab.kind {
                TabKind::LocalTerminal => self.i18n.t("terminal.command_bar.local_shell"),
                TabKind::SshTerminal => tab.title.clone(),
                _ => tab.title.clone(),
            })
            .unwrap_or_else(|| self.i18n.t("terminal.command_bar.remote_shell"));
        let active_pane_id = self.active_pane_id();
        let is_local_terminal = self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::LocalTerminal);
        let can_split = self.active_tab().is_some_and(|tab| {
            tab.kind == TabKind::LocalTerminal
                && tab
                    .root_pane
                    .as_ref()
                    .is_some_and(|root| root.pane_count() < MAX_PANES_PER_TAB)
        });
        let broadcast_targets =
            self.terminal_broadcast_target_panes(active_pane_id.unwrap_or(PaneId(0)));
        let broadcast_label = if self.terminal_broadcast_enabled {
            if self.terminal_broadcast_targets.is_empty() {
                self.i18n.t("terminal.command_bar.all_targets")
            } else {
                format!("{}", broadcast_targets.len())
            }
        } else {
            String::new()
        };
        let quick_commands_enabled = self
            .settings_store
            .settings()
            .terminal
            .command_bar
            .quick_commands_enabled;
        let recording_status = self.active_terminal_recording_status(cx);
        let recording_active = recording_status.state != TerminalRecordingState::Idle;

        div()
            .relative()
            .flex_none()
            .border_t_1()
            .border_color(rgba((theme.border << 8) | COMMAND_BAR_BORDER_ALPHA))
            .bg(rgba((theme.bg << 8) | COMMAND_BAR_BG_ALPHA))
            .px(px(12.0))
            .py(px(4.0))
            .shadow_lg()
            .when(
                focused
                    && self.terminal_command_suggestions_open
                    && !command_suggestions.is_empty(),
                |bar| bar.child(self.render_terminal_command_suggestions(&command_suggestions, cx)),
            )
            .child(
                div()
                    .min_h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(rgb(theme.text_muted))
                            .child(target_label),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .when(
                                self.terminal_broadcast_enabled && !broadcast_label.is_empty(),
                                |actions| {
                                    actions.child(
                                        div()
                                            .h(px(20.0))
                                            .px(px(6.0))
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .rounded(px(self.tokens.radii.md))
                                            .border_1()
                                            .border_color(rgba(0xf973164d))
                                            .bg(rgba(0xf973161a))
                                            .text_size(px(11.0))
                                            .text_color(rgba(0xfdba74ff))
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::Radio,
                                                12.0,
                                                rgba(0xfdba74ff),
                                            ))
                                            .child(broadcast_label),
                                    )
                                },
                            )
                            .when(is_local_terminal, |actions| {
                                actions
                                    .child(
                                        div()
                                            .size(px(24.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px(self.tokens.radii.md))
                                            .text_color(if can_split {
                                                rgb(theme.text_muted)
                                            } else {
                                                rgba((theme.text_muted << 8) | 0x59)
                                            })
                                            .when(can_split, |button| {
                                                button
                                                    .cursor_pointer()
                                                    .hover(move |style| {
                                                        style.bg(rgb(theme.bg_hover))
                                                    })
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _event, window, cx| {
                                                            this.split_active_pane(
                                                                SplitDirection::Horizontal,
                                                                window,
                                                                cx,
                                                            );
                                                            cx.stop_propagation();
                                                        }),
                                                    )
                                            })
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::SplitSquareHorizontal,
                                                14.0,
                                                if can_split {
                                                    rgb(theme.text_muted)
                                                } else {
                                                    rgba((theme.text_muted << 8) | 0x59)
                                                },
                                            )),
                                    )
                                    .child(
                                        div()
                                            .size(px(24.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px(self.tokens.radii.md))
                                            .when(can_split, |button| {
                                                button
                                                    .cursor_pointer()
                                                    .hover(move |style| {
                                                        style.bg(rgb(theme.bg_hover))
                                                    })
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _event, window, cx| {
                                                            this.split_active_pane(
                                                                SplitDirection::Vertical,
                                                                window,
                                                                cx,
                                                            );
                                                            cx.stop_propagation();
                                                        }),
                                                    )
                                            })
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::SplitSquareVertical,
                                                14.0,
                                                if can_split {
                                                    rgb(theme.text_muted)
                                                } else {
                                                    rgba((theme.text_muted << 8) | 0x59)
                                                },
                                            )),
                                    )
                            })
                            .child(
                                div()
                                    .relative()
                                    .size(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(self.tokens.radii.md))
                                    .cursor_pointer()
                                    .bg(if self.terminal_broadcast_enabled {
                                        rgba(0xf9731626)
                                    } else {
                                        rgba((theme.bg_hover << 8) | 0x00)
                                    })
                                    .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.toggle_terminal_broadcast_menu();
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    )
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::Radio,
                                        14.0,
                                        if self.terminal_broadcast_enabled {
                                            rgba(0xfb923cff)
                                        } else {
                                            rgb(theme.text_muted)
                                        },
                                    )),
                            )
                            .when(recording_active, |actions| {
                                actions.child(
                                    div()
                                        .h(px(20.0))
                                        .px(px(6.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .rounded(px(self.tokens.radii.md))
                                        .border_1()
                                        .border_color(rgba(0xef44444d))
                                        .bg(rgba(0xef44441a))
                                        .text_size(px(11.0))
                                        .text_color(rgba(0xfca5a5ff))
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::Circle,
                                            10.0,
                                            rgba(0xfca5a5ff),
                                        ))
                                        .child(format_recording_elapsed(recording_status.elapsed)),
                                )
                            })
                            .child(
                                div()
                                    .size(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(self.tokens.radii.md))
                                    .cursor_pointer()
                                    .bg(if recording_active {
                                        rgba(0xef444426)
                                    } else {
                                        rgba(0x00000000)
                                    })
                                    .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            match recording_status.state {
                                                TerminalRecordingState::Idle => {
                                                    this.start_active_terminal_recording(cx)
                                                }
                                                TerminalRecordingState::Recording => {
                                                    this.pause_active_terminal_recording(cx)
                                                }
                                                TerminalRecordingState::Paused => {
                                                    this.resume_active_terminal_recording(cx)
                                                }
                                            }
                                            cx.stop_propagation();
                                        }),
                                    )
                                    .child(Self::render_lucide_icon(
                                        match recording_status.state {
                                            TerminalRecordingState::Paused => LucideIcon::Play,
                                            _ => LucideIcon::Circle,
                                        },
                                        14.0,
                                        if recording_active {
                                            rgba(0xf87171ff)
                                        } else {
                                            rgb(theme.text_muted)
                                        },
                                    )),
                            )
                            .when(recording_active, |actions| {
                                actions
                                    .child(
                                        div()
                                            .size(px(24.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px(self.tokens.radii.md))
                                            .cursor_pointer()
                                            .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|this, _event, _window, cx| {
                                                    this.stop_active_terminal_recording(cx);
                                                    cx.stop_propagation();
                                                }),
                                            )
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::Square,
                                                14.0,
                                                rgba(0xf87171ff),
                                            )),
                                    )
                                    .child(
                                        div()
                                            .size(px(24.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px(self.tokens.radii.md))
                                            .cursor_pointer()
                                            .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|this, _event, _window, cx| {
                                                    this.discard_active_terminal_recording(cx);
                                                    cx.stop_propagation();
                                                }),
                                            )
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::Trash2,
                                                14.0,
                                                rgba(0xf87171ff),
                                            )),
                                    )
                            })
                            .child(
                                div()
                                    .size(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(self.tokens.radii.md))
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, window, cx| {
                                            this.open_terminal_cast_file(window, cx);
                                            cx.stop_propagation();
                                        }),
                                    )
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::FilePlay,
                                        14.0,
                                        rgb(theme.text_muted),
                                    )),
                            ),
                    ),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .pt(px(4.0))
                    .border_t_1()
                    .border_color(if focused {
                        rgba((theme.accent << 8) | COMMAND_BAR_FOCUSED_BORDER_ALPHA)
                    } else {
                        rgba((theme.border << 8) | COMMAND_BAR_INPUT_BORDER_ALPHA)
                    })
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_text()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                            this.terminal_command_bar_focused = true;
                            this.ime_marked_text = None;
                            window.focus(&this.focus_handle);
                            this.begin_ime_selection_from_mouse_down(
                                WorkspaceImeTarget::TerminalCommandBar,
                                event,
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(
                        cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                            this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                        }),
                    )
                    .child(Self::render_lucide_icon(
                        LucideIcon::ChevronRight,
                        16.0,
                        rgb(theme.text_muted),
                    ))
                    .child(text_input_anchor_probe(
                        target.anchor_id(),
                        div()
                            .h(px(24.0))
                            .flex_1()
                            .flex()
                            .items_center()
                            .overflow_hidden()
                            .text_size(px(13.0))
                            .font_family(settings_mono_font_family(self.settings_store.settings()))
                            .text_color(if showing_placeholder {
                                rgb(theme.text_muted)
                            } else {
                                rgb(theme.text)
                            })
                            .when(focused && showing_placeholder, |input| {
                                input.child(text_caret(
                                    &self.tokens,
                                    self.new_connection_caret_visible,
                                ))
                            })
                            .child(command_text)
                            .when_some(marked_text, |input, marked| {
                                input.child(
                                    div()
                                        .underline()
                                        .text_color(rgb(theme.text))
                                        .child(marked.to_string()),
                                )
                            })
                            .when(focused && !showing_placeholder, |input| {
                                input.child(text_caret(
                                    &self.tokens,
                                    self.new_connection_caret_visible,
                                ))
                            })
                            .when_some(ghost_text, |input, ghost| {
                                input.child(
                                    div()
                                        .text_color(rgba((theme.text_muted << 8) | 0x99))
                                        .child(ghost),
                                )
                            }),
                        move |anchor, _window, cx| {
                            let _ = workspace.update(cx, |this, cx| {
                                this.update_text_input_anchor(anchor, cx);
                            });
                        },
                    ))
                    .when(quick_commands_enabled, |input_row| {
                        input_row.child(
                            div()
                                .size(px(24.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(self.tokens.radii.md))
                                .cursor_pointer()
                                .bg(if self.terminal_quick_commands_open {
                                    rgba((theme.accent << 8) | 0x1a)
                                } else {
                                    rgba(0x00000000)
                                })
                                .text_color(if self.terminal_quick_commands_open {
                                    rgb(theme.accent)
                                } else {
                                    rgb(theme.text_muted)
                                })
                                .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.terminal_quick_commands_open =
                                            !this.terminal_quick_commands_open;
                                        this.dismiss_terminal_broadcast_menu();
                                        if !this.terminal_quick_commands_open {
                                            this.close_terminal_quick_commands_popover();
                                        }
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                )
                                .child(Self::render_lucide_icon(
                                    LucideIcon::Zap,
                                    14.0,
                                    if self.terminal_quick_commands_open {
                                        rgb(theme.accent)
                                    } else {
                                        rgb(theme.text_muted)
                                    },
                                )),
                        )
                    }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_terminal_quick_commands_popover(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_quick_commands_popover(cx)
    }

    pub(in crate::workspace) fn render_terminal_broadcast_menu(
        &self,
        placement: TerminalBroadcastMenuPlacement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let entries = self.terminal_broadcast_entries();
        let active_pane_id = self.active_pane_id();
        let selectable = entries
            .iter()
            .filter(|(pane_id, _, _)| Some(*pane_id) != active_pane_id)
            .map(|(pane_id, _, _)| *pane_id)
            .collect::<Vec<_>>();
        let all_selected = !selectable.is_empty()
            && selectable
                .iter()
                .all(|pane_id| self.terminal_broadcast_targets.contains(pane_id));

        let mut menu = context_menu_event_boundary(
            div()
                .absolute()
                .right(px(12.0))
                .w(px(260.0))
                .max_h(px(320.0))
                .overflow_hidden()
                .rounded(px(self.tokens.radii.lg))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgba((theme.bg_elevated << 8) | 0xf2))
                .shadow_lg()
                .p(px(6.0))
                .text_size(px(12.0)),
        )
        .child(
            div()
                .px(px(6.0))
                .py(px(4.0))
                .text_size(px(11.0))
                .text_color(rgb(theme.text_muted))
                .child(self.i18n.t("terminal.broadcast.select_targets")),
        );
        menu = match placement {
            TerminalBroadcastMenuPlacement::Bottom(offset) => menu.bottom(px(offset)),
            TerminalBroadcastMenuPlacement::Top(offset) => menu.top(px(offset)),
        };

        if entries.len() <= 1 {
            menu = menu.child(
                div()
                    .px(px(8.0))
                    .py(px(12.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("terminal.broadcast.no_targets")),
            );
        } else {
            for (pane_id, label, kind) in entries {
                let is_current = Some(pane_id) == active_pane_id;
                let checked = self.terminal_broadcast_targets.contains(&pane_id);
                let badge = match kind {
                    TabKind::LocalTerminal => self.i18n.t("terminal.typeLocal"),
                    TabKind::SshTerminal => self.i18n.t("terminal.typeSsh"),
                    _ => String::new(),
                };
                let row_color = if is_current {
                    rgb(theme.text_muted)
                } else {
                    rgb(theme.text)
                };
                let row = div()
                    .h(px(30.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(8.0))
                    .rounded(px(self.tokens.radii.md))
                    .text_color(row_color)
                    .child(if checked {
                        Self::render_lucide_icon(LucideIcon::Check, 12.0, rgba(0xfb923cff))
                    } else if is_current {
                        div()
                            .size(px(12.0))
                            .rounded_full()
                            .bg(rgba(0xfb923cff))
                            .into_any_element()
                    } else {
                        div().size(px(12.0)).into_any_element()
                    })
                    .child(div().flex_1().truncate().child(label))
                    .when(!badge.is_empty(), |row| {
                        row.child(
                            div()
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(px(self.tokens.radii.md))
                                .text_size(px(10.0))
                                .text_color(rgb(theme.text_muted))
                                .bg(rgba((theme.bg_panel << 8) | 0x99))
                                .child(badge),
                        )
                    })
                    .when(is_current, |row| {
                        row.child(
                            div()
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(px(self.tokens.radii.md))
                                .text_size(px(10.0))
                                .text_color(rgba(0xfb923cff))
                                .bg(rgba(0xf9731626))
                                .child(self.i18n.t("terminal.broadcast.current")),
                        )
                    });
                // Broadcast rows are checkbox-style menu items. Keep current
                // pane disabled through the shared menu action guard.
                let row = self.render_terminal_broadcast_menu_action(
                    row,
                    is_current,
                    false,
                    Some(rgb(theme.bg_hover)),
                    cx.listener(move |this, _event, _window, _cx| {
                        if this.terminal_broadcast_targets.remove(&pane_id) {
                            if this.terminal_broadcast_targets.is_empty() {
                                this.terminal_broadcast_enabled = false;
                            }
                        } else {
                            this.terminal_broadcast_targets.insert(pane_id);
                            this.terminal_broadcast_enabled = true;
                        }
                        this.keep_terminal_broadcast_menu_open();
                    }),
                    cx,
                );
                menu = menu.child(row);
            }

            let select_all_disabled = selectable.is_empty();
            let select_all_label = div()
                .text_size(px(11.0))
                .text_color(rgb(theme.text_muted))
                .child(if all_selected {
                    self.i18n.t("terminal.broadcast.deselect_all")
                } else {
                    self.i18n.t("terminal.broadcast.select_all")
                });
            menu = menu.child(
                div()
                    .mt(px(4.0))
                    .pt(px(6.0))
                    .border_t_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(6.0))
                    .child(self.workspace_context_menu_persistent_styled_action(
                        select_all_label,
                        select_all_disabled,
                        false,
                        ContextMenuActionableStyle {
                            hover_background: None,
                            hover_text_color: Some(rgb(theme.accent)),
                        },
                        move |this, _event, _window, _cx| {
                            if all_selected {
                                this.terminal_broadcast_enabled = false;
                                this.terminal_broadcast_targets.clear();
                            } else {
                                this.terminal_broadcast_targets =
                                    selectable.iter().copied().collect();
                                this.terminal_broadcast_enabled =
                                    !this.terminal_broadcast_targets.is_empty();
                            }
                            this.keep_terminal_broadcast_menu_open();
                        },
                        cx,
                    ))
                    .when(self.terminal_broadcast_enabled, |footer| {
                        footer.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgba(0xfb923cff))
                                .child(self.i18n.t("terminal.broadcast.target_count")),
                        )
                    }),
            );
        }

        menu.into_any_element()
    }

    fn render_terminal_broadcast_menu_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        hover_bg: Option<gpui::Rgba>,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Tauri broadcast target rows are Radix menu items with a disabled
        // current-terminal row. Keep native hover/cursor and action blocking
        // coupled to the shared context-menu guard.
        self.workspace_context_menu_persistent_styled_action(
            item,
            disabled,
            loading,
            ContextMenuActionableStyle {
                hover_background: hover_bg,
                hover_text_color: None,
            },
            move |_this, event, window, cx| listener(event, window, cx),
            cx,
        )
    }
}
