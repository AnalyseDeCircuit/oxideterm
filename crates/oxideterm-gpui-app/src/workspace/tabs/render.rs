impl WorkspaceApp {
    pub(super) fn render_tab_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let scroll_x = self.tab_scroll_x.max(0.0);
        let mut bar = div()
            .h(px(self.tokens.metrics.tabbar_height))
            .flex()
            .flex_row()
            .items_center()
            .pl(px(self.tokens.metrics.tabbar_leading_offset))
            .overflow_hidden()
            .border_b_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg))
            .on_scroll_wheel(cx.listener(|this, event, window, cx| {
                this.handle_tabbar_scroll(event, window, cx);
            }));

        let mut tabs_row = div()
            .h_full()
            .flex()
            .flex_row()
            .items_center()
            .flex_none()
            .relative()
            .left(px(-scroll_x));

        for tab in &self.tabs {
            let tab_id = tab.id;
            let active = Some(tab_id) == self.active_tab_id;
            let tab_width = self.tab_visual_width(tab);
            let reconnect_node_id = self.reconnect_node_id_for_tab(tab);
            let reconnect_job = reconnect_node_id
                .as_ref()
                .and_then(|node_id| self.reconnect_orchestrator.job(&node_id.0))
                .filter(|job| job.ended_at.is_none());
            let show_reconnect_progress = reconnect_job.is_some();
            let icon = match tab.kind {
                TabKind::LocalTerminal => LucideIcon::Square,
                TabKind::SshTerminal => LucideIcon::Terminal,
                TabKind::FileManager => LucideIcon::FolderOpen,
                TabKind::Sftp => LucideIcon::FolderInput,
                TabKind::Ide => LucideIcon::Code2,
                TabKind::Forwards => LucideIcon::ArrowLeftRight,
                TabKind::SessionManager => LucideIcon::LayoutList,
                TabKind::Settings => LucideIcon::Settings,
            };
            let tab_text = self.tab_display_title(tab);
            let tab_text_color = if active {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            };
            tabs_row = tabs_row.child(
                div()
                    .id(("workspace-tab", tab_id.0))
                    .h_full()
                    .flex_none()
                    .w(px(tab_width))
                    .min_w(px(self.tokens.metrics.tab_min_width))
                    .max_w(px(self.tokens.metrics.tab_max_width))
                    .px(px(self.tokens.metrics.tab_padding_x))
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(self.tokens.metrics.tab_gap))
                    .border_r_1()
                    .border_color(if show_reconnect_progress {
                        rgb(0xf59e0b)
                    } else {
                        rgb(theme.border)
                    })
                    .bg(if active {
                        rgb(theme.bg_panel)
                    } else {
                        rgb(theme.bg)
                    })
                    .text_color(if active {
                        rgb(theme.text)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.set_active_tab(tab_id, window, cx);
                        }),
                    )
                    .when(active || show_reconnect_progress, |tab| {
                        tab.child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .right_0()
                                .h(px(self.tokens.metrics.tab_active_accent_height))
                                .bg(rgb(if show_reconnect_progress {
                                    0xf59e0b
                                } else {
                                    theme.accent
                                })),
                        )
                    })
                    .child(Self::render_lucide_icon(
                        icon,
                        self.tokens.metrics.tab_icon_size,
                        tab_text_color,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(self.tokens.metrics.tab_font_size))
                            .child(tab_text),
                    )
                    .when_some(
                        reconnect_job.zip(reconnect_node_id),
                        |tab, (job, node_id)| {
                            tab.child(self.render_tab_reconnect_indicator(&job, node_id, cx))
                        },
                    )
                    .when(!show_reconnect_progress, |tab| {
                        tab.child(
                            div()
                                .size(px(self.tokens.metrics.tab_close_button_size))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(self.tokens.radii.sm))
                                .cursor_pointer()
                                .text_color(rgb(theme.text_muted))
                                .child(Self::render_lucide_icon(
                                    LucideIcon::X,
                                    self.tokens.metrics.tab_close_icon_size,
                                    rgb(theme.text_muted),
                                ))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, window, cx| {
                                        this.set_active_tab(tab_id, window, cx);
                                        this.close_active_tab(window, cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                        )
                    }),
            );
        }

        bar = bar
            .child(tabs_row)
            .child(div().flex_1().min_w(px(0.0)))
            .when_some(self.render_legacy_terminal_actions(cx), |bar, actions| {
                bar.child(actions)
            });
        bar.into_any_element()
    }

    fn reconnect_node_id_for_tab(&self, tab: &Tab) -> Option<NodeId> {
        match tab.kind {
            TabKind::SshTerminal => {
                if let Some(active_pane_id) = tab.active_pane_id
                    && let Some(session_id) = tab
                        .root_pane
                        .as_ref()
                        .and_then(|root| root.session_id_for_pane(active_pane_id))
                    && let Some(node_id) = self.terminal_ssh_nodes.get(&session_id)
                {
                    return Some(node_id.clone());
                }
                let mut session_ids = Vec::new();
                tab.root_pane
                    .as_ref()
                    .map(|root| root.collect_session_ids(&mut session_ids));
                session_ids
                    .into_iter()
                    .filter_map(|session_id| self.terminal_ssh_nodes.get(&session_id))
                    .find(|node_id| self.has_active_reconnect_job(node_id))
                    .cloned()
                    .or_else(|| {
                        tab.root_pane.as_ref().and_then(|root| {
                            let mut session_ids = Vec::new();
                            root.collect_session_ids(&mut session_ids);
                            session_ids
                                .first()
                                .and_then(|session_id| self.terminal_ssh_nodes.get(session_id))
                                .cloned()
                        })
                    })
            }
            TabKind::Sftp => self
                .sftp_tab_nodes
                .get(&tab.id)
                .cloned()
                .filter(|node_id| self.has_active_reconnect_job(node_id)),
            TabKind::Forwards => self
                .forward_tab_nodes
                .get(&tab.id)
                .cloned()
                .filter(|node_id| self.has_active_reconnect_job(node_id)),
            TabKind::Ide => self
                .ide_tab_nodes
                .get(&tab.id)
                .cloned()
                .filter(|node_id| self.has_active_reconnect_job(node_id)),
            _ => None,
        }
    }

    fn render_tab_reconnect_indicator(
        &self,
        job: &ReconnectJob,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let phase_label = reconnect_phase_label(&job.status);
        let phase_text = if job.attempt > 1 {
            format!("{phase_label} {}/{}", job.attempt, job.max_attempts)
        } else {
            phase_label.to_string()
        };
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(10.0))
            .text_color(rgb(0xf59e0b))
            .child(Self::render_lucide_icon(
                LucideIcon::RefreshCw,
                12.0,
                rgb(0xf59e0b),
            ))
            .child(self.render_reconnect_phase_strip(job))
            .child(div().max_w(px(72.0)).truncate().child(phase_text))
            .child(
                div()
                    .size(px(self.tokens.metrics.tab_close_button_size))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.sm))
                    .cursor_pointer()
                    .hover(move |button| button.bg(rgb(theme.bg_hover)))
                    .child(Self::render_lucide_icon(
                        LucideIcon::X,
                        self.tokens.metrics.tab_close_icon_size,
                        rgb(0xf59e0b),
                    ))
                    .on_mouse_move(cx.listener({
                        let label = self.i18n.t("sessions.tree.actions.cancel_reconnect");
                        move |this, event: &MouseMoveEvent, _window, cx| {
                            this.queue_workspace_tooltip(
                                "tabbar-cancel-reconnect",
                                label.clone(),
                                f32::from(event.position.x) + 12.0,
                                f32::from(event.position.y) + 16.0,
                                cx,
                            );
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.cancel_reconnect_for_node(&node_id, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_reconnect_phase_strip(&self, job: &ReconnectJob) -> AnyElement {
        let phases = [
            ReconnectPhase::Snapshot,
            ReconnectPhase::GracePeriod,
            ReconnectPhase::SshConnect,
            ReconnectPhase::AwaitTerminal,
            ReconnectPhase::RestoreForwards,
            ReconnectPhase::ResumeTransfers,
            ReconnectPhase::RestoreIde,
            ReconnectPhase::Verify,
        ];
        div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .children(phases.into_iter().map(|phase| {
                let result = job
                    .phase_history
                    .iter()
                    .rev()
                    .find(|event| event.phase == phase)
                    .map(|event| event.result);
                let color = match result {
                    Some(PhaseResult::Ok) => 0x10b981,
                    Some(PhaseResult::Failed) => 0xef4444,
                    Some(PhaseResult::Skipped) => self.tokens.ui.text_muted,
                    Some(PhaseResult::Running) => 0xf59e0b,
                    None => self.tokens.ui.border,
                };
                div()
                    .size(px(4.0))
                    .rounded_full()
                    .bg(rgb(color))
                    .into_any_element()
            }))
            .into_any_element()
    }

    fn render_legacy_terminal_actions(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let active_tab = self.active_tab()?;
        if !matches!(active_tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal) {
            return None;
        }
        let command_bar = &self.settings_store.settings().terminal.command_bar;
        if command_bar.enabled && !command_bar.show_legacy_toolbar {
            return None;
        }

        let theme = self.tokens.ui;
        let is_local_terminal = active_tab.kind == TabKind::LocalTerminal;
        let can_split = is_local_terminal
            && active_tab
                .root_pane
                .as_ref()
                .is_some_and(|root| root.pane_count() < MAX_PANES_PER_TAB);
        let pane_count = active_tab
            .root_pane
            .as_ref()
            .map(|root| root.pane_count())
            .unwrap_or(1);
        let active_pane_id = self.active_pane_id();
        let broadcast_targets =
            self.terminal_broadcast_target_panes(active_pane_id.unwrap_or(PaneId(0)));
        let broadcast_label = if self.terminal_broadcast_enabled {
            if self.terminal_broadcast_targets.is_empty() {
                self.i18n.t("terminal.command_bar.all_targets")
            } else {
                broadcast_targets.len().to_string()
            }
        } else {
            String::new()
        };

        Some(
            div()
                .h_full()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(4.0))
                .px(px(8.0))
                .border_l_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.bg))
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
                                .rounded_md()
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
                            terminal_legacy_icon_button(
                                &self.tokens,
                                LucideIcon::ArrowLeftRight,
                                can_split,
                            )
                            .when(can_split, |button| {
                                button.on_mouse_down(
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
                            }),
                        )
                        .child(
                            terminal_legacy_icon_button(
                                &self.tokens,
                                LucideIcon::PanelLeft,
                                can_split,
                            )
                            .when(can_split, |button| {
                                button.on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, window, cx| {
                                        this.split_active_pane(SplitDirection::Vertical, window, cx);
                                        cx.stop_propagation();
                                    }),
                                )
                            }),
                        )
                        .when(pane_count > 1, |actions| {
                            actions.child(
                                div()
                                    .h(px(20.0))
                                    .min_w(px(20.0))
                                    .px(px(5.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .bg(rgba((theme.bg_panel << 8) | 0xcc))
                                    .text_size(px(11.0))
                                    .text_color(rgb(theme.text_muted))
                                    .child(pane_count.to_string()),
                            )
                        })
                })
                .child(
                    terminal_legacy_icon_button(&self.tokens, LucideIcon::Radio, true)
                        .bg(if self.terminal_broadcast_enabled {
                            rgba(0xf9731626)
                        } else {
                            rgba(0x00000000)
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.terminal_broadcast_menu_open =
                                    !this.terminal_broadcast_menu_open;
                                this.terminal_quick_commands_open = false;
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                )
                .child(terminal_legacy_icon_button(
                    &self.tokens,
                    LucideIcon::Square,
                    false,
                ))
                .child(terminal_legacy_icon_button(
                    &self.tokens,
                    LucideIcon::Play,
                    false,
                ))
                .into_any_element(),
        )
    }

    pub(super) fn render_empty_workspace(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .px(px(16.0))
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text_muted))
            .font_family(settings_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            ))
            .child(
                div()
                    .w_full()
                    .max_w(px(384.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(24.0))
                    .child(self.render_welcome_brand())
                    .child(self.render_welcome_actions(cx))
                    .child(self.render_welcome_shortcuts()),
            )
            .into_any_element()
    }

    fn render_welcome_brand(&self) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .items_center()
                    .text_size(px(48.0))
                    .line_height(px(48.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("layout.empty.title"))
                    .child(
                        div()
                            .w(px(3.0))
                            .h(px(34.0))
                            .ml(px(6.0))
                            .rounded(px(self.tokens.radii.active_indicator))
                            .bg(rgb(self.tokens.ui.accent)),
                    ),
            )
            .into_any_element()
    }

    fn render_welcome_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .child(self.render_welcome_action_button(
                LucideIcon::Plus,
                "layout.empty.new_connection",
                true,
                true,
                cx,
            ))
            .child(self.render_welcome_action_button(
                LucideIcon::Terminal,
                "layout.empty.new_local_terminal",
                false,
                false,
                cx,
            ))
            .into_any_element()
    }

    fn render_welcome_action_button(
        &self,
        icon: LucideIcon,
        label_key: &str,
        opens_connection_form: bool,
        filled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (bg, border) = if filled {
            (rgb(theme.bg_panel), rgb(theme.border))
        } else {
            (rgba(0x00000000), rgb(theme.border))
        };
        div()
            .h(px(self.tokens.metrics.ui_button_default_height))
            .px(px(self.tokens.metrics.ui_button_default_padding_x))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(theme.text))
            .cursor_pointer()
            .hover(move |button| {
                button
                    .bg(rgb(theme.bg_hover))
                    .border_color(rgb(theme.border_strong))
            })
            .child(Self::render_lucide_icon(icon, 16.0, rgb(theme.text)))
            .child(self.i18n.t(label_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if opens_connection_form {
                        this.open_new_connection_form(window, cx);
                    } else {
                        let _ = this.create_local_terminal_tab(window, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_welcome_shortcuts(&self) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_center()
            .gap_x(px(20.0))
            .gap_y(px(8.0))
            .pt(px(4.0))
            .child(self.render_welcome_shortcut(shortcut_key("K"), "command_palette.title"))
            .child(self.render_welcome_shortcut(shortcut_key("N"), "layout.empty.new_connection"))
            .child(
                self.render_welcome_shortcut(shortcut_key("T"), "layout.empty.new_local_terminal"),
            )
            .child(
                self.render_welcome_shortcut(shortcut_key("/"), "layout.empty.keyboard_shortcuts"),
            )
            .into_any_element()
    }

    fn render_welcome_shortcut(&self, key: String, label_key: &str) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(11.0))
                    .line_height(px(14.0))
                    .text_color(rgb(theme.text))
                    .child(key),
            )
            .child(self.i18n.t(label_key))
            .into_any_element()
    }
}

fn terminal_legacy_icon_button(
    tokens: &ThemeTokens,
    icon: LucideIcon,
    enabled: bool,
) -> gpui::Div {
    let theme = tokens.ui;
    let color = if enabled {
        rgb(theme.text_muted)
    } else {
        rgba((theme.text_muted << 8) | 0x59)
    };
    div()
        .size(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .text_color(color)
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(move |style| style.bg(rgb(theme.bg_hover)))
        })
        .child(WorkspaceApp::render_lucide_icon(icon, 14.0, color))
}
