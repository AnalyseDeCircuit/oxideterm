use gpui::StatefulInteractiveElement;

impl WorkspaceApp {
    pub(super) fn render_tab_bar(&self, _window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let bar = div()
            .h(px(self.tokens.metrics.tabbar_height))
            .flex()
            .flex_row()
            .items_center()
            .overflow_hidden()
            .border_b_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg));

        // Tauri's scroll container measures the full inline-flex tab row as
        // scrollWidth. GPUI's ScrollHandle computes max_offset from direct
        // child bounds, so tab items must stay direct children of this viewport.
        // Keep overflow hidden instead of overflow_x_scroll: the custom wheel
        // adapter owns browser-style scrollLeft clamping, while track_scroll
        // still measures and applies the offset without GPUI boundary overscroll.
        let mut scroll_viewport = div()
            .id("workspace-tab-scroll-viewport")
            .h_full()
            .flex_1()
            .min_w(px(0.0))
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .pl(px(self.tokens.metrics.tabbar_leading_offset))
            .overflow_hidden()
            .track_scroll(&self.main_window_tabs.scroll_handle)
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                this.handle_tabbar_scroll(event, window, cx);
            }));

        for (tab_index, tab) in self.tabs.iter().enumerate() {
            let tab_id = tab.id;
            if self.detached_tabs.contains(&tab_id) {
                continue;
            }
            let active = Some(tab_id) == self.main_window_tabs.active_tab_id;
            let drag_state = self.main_window_tabs.drag.as_ref();
            let drag_active = drag_state.is_some_and(|drag| drag.active);
            let is_being_dragged = drag_state.is_some_and(|drag| drag.tab_id == tab_id);
            let show_drop_indicator = drag_state.is_some_and(|drag| {
                drag.active
                    && drag.mode == TabDragMode::Reorder
                    && drag.drop_target_index == tab_index
                    && drag.from_index != tab_index
            });
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
                TabKind::Launcher => LucideIcon::Monitor,
                TabKind::Graphics => LucideIcon::Monitor,
                TabKind::Runtime => LucideIcon::Gauge,
                TabKind::ConnectionPool => LucideIcon::Terminal,
                TabKind::ConnectionMonitor => LucideIcon::Activity,
                TabKind::Topology => LucideIcon::Network,
                TabKind::NotificationCenter => LucideIcon::Bell,
                TabKind::Sftp => LucideIcon::FolderInput,
                TabKind::Ide => LucideIcon::Code2,
                TabKind::Forwards => LucideIcon::ArrowLeftRight,
                TabKind::SessionManager => LucideIcon::LayoutList,
                TabKind::PluginManager => LucideIcon::Puzzle,
                TabKind::Plugin { .. } => LucideIcon::Puzzle,
                TabKind::CloudSync => LucideIcon::Cloud,
                TabKind::RemoteDesktop => LucideIcon::Monitor,
                TabKind::Settings => LucideIcon::Settings,
            };
            let tab_text = self.tab_display_title(tab);
            let tab_text_color = if active {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            };
            scroll_viewport = scroll_viewport.child(
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
                    .opacity(if is_being_dragged && drag_active {
                        0.5
                    } else {
                        1.0
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            this.start_tab_drag_candidate(tab_id, tab_index, event, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                            this.open_tab_context_menu(tab_id, event, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .when(show_drop_indicator, |tab| {
                        tab.child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(px(2.0))
                                .bg(rgb(theme.accent)),
                        )
                    })
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
                                        this.request_close_active_tab(window, cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                        )
                    }),
            );
        }

        // Only the trailing filler is draggable. Do not mark the tabbar parent
        // or scroll viewport as WindowControlArea::Drag; those elements contain
        // interactive tabs and close buttons that must keep receiving normal
        // GPUI mouse events.
        scroll_viewport =
            scroll_viewport.child(self.render_window_drag_region("workspace-tabbar-drag-region", cx));

        bar.child(scroll_viewport).into_any_element()
    }

    pub(super) fn render_node_disconnect_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(confirm) = self.node_disconnect_confirm.as_ref() else {
            return div().into_any_element();
        };
        let title = self
            .i18n
            .t("common.confirm.disconnect_node")
            .replace("{{name}}", &confirm.display_name);
        confirm_dialog_with_focus(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div().child(title).into_any_element(),
                description: None,
                cancel_label: div()
                    .child(self.i18n.t("common.actions.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t("common.actions.confirm"))
                    .into_any_element(),
            },
            self.standard_confirm_focus(),
            cx.listener(|this, _event, _window, cx| {
                this.cancel_node_disconnect_confirm(cx);
                cx.stop_propagation();
            }),
            cx.listener(|this, _event, window, cx| {
                this.confirm_node_disconnect_confirm(window, cx);
                cx.stop_propagation();
            }),
        )
    }

    pub(super) fn render_tab_close_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(confirm) = self.main_window_tabs.close_confirm.as_ref() else {
            return div().into_any_element();
        };
        let (title_key, description) = match confirm {
            TabCloseConfirm::Single { .. } => (
                "tabbar.confirm_close_terminal_title",
                self.i18n.t("tabbar.confirm_close_terminal_desc"),
            ),
            TabCloseConfirm::LocalChildProcess { .. }
            | TabCloseConfirm::LocalChildProcessBatch { .. } => {
                ("tabbar.child_process_warning", String::new())
            }
            TabCloseConfirm::Other { tab_ids } => (
                "tabbar.confirm_close_other_title",
                self.i18n
                    .t("tabbar.confirm_close_other_desc")
                    .replace("{{count}}", &tab_ids.len().to_string()),
            ),
        };
        confirm_dialog_with_focus(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div().child(self.i18n.t(title_key)).into_any_element(),
                description: (!description.is_empty())
                    .then(|| div().child(description).into_any_element()),
                cancel_label: div()
                    .child(self.i18n.t("common.actions.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t("common.actions.confirm"))
                    .into_any_element(),
            },
            self.standard_confirm_focus(),
            cx.listener(|this, _event, _window, cx| {
                this.cancel_tab_close_confirm(cx);
                cx.stop_propagation();
            }),
            cx.listener(|this, _event, window, cx| {
                this.confirm_tab_close_confirm(window, cx);
                cx.stop_propagation();
            }),
        )
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
                    .id((
                        gpui::ElementId::from("tabbar-cancel-reconnect"),
                        node_id.0.clone(),
                    ))
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
                    .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                        if !*hovered {
                            // The reconnect tooltip is mounted at the workspace
                            // root, so the tab button owns the explicit leave
                            // clear just like a browser TooltipTrigger.
                            this.clear_workspace_tooltip("tabbar-cancel-reconnect", cx);
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
