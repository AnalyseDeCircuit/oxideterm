impl WorkspaceApp {
    pub(super) fn open_forwards_tab(
        &mut self,
        node_id: NodeId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let node_title = self
            .ssh_nodes
            .get(&node_id)
            .map(|node| node.title.clone())
            .unwrap_or_else(|| node_id.0.clone());
        let title = format!("{} · {}", self.i18n.t("forwards.table.title"), node_title);
        let tab_id = if let Some((tab_id, _)) = self
            .forward_tab_nodes
            .iter()
            .find(|(_, existing_node_id)| *existing_node_id == &node_id)
        {
            *tab_id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::Forwards,
                title,
                title_source: TabTitleSource::Static,
                root_pane: None,
                active_pane_id: None,
            });
            self.forward_tab_nodes.insert(tab_id, node_id.clone());
            tab_id
        };

        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.active_ssh_node_id = Some(node_id.clone());
        self.ensure_node_connection_started(&node_id);
        self.forwarding_view.error = None;
        self.start_port_scan_for_forwards_tab(tab_id, cx);
        cx.notify();
    }

    pub(super) fn render_forwards_surface(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(tab_id) = self.active_tab_id else {
            return self.render_empty_workspace(cx);
        };
        let Some(node_id) = self.forward_tab_nodes.get(&tab_id).cloned() else {
            return self.render_empty_workspace(cx);
        };
        let node_ready = self
            .ssh_nodes
            .get(&node_id)
            .is_some_and(|node| node.readiness == NodeReadiness::Ready);
        let manager = self.forwarding_manager_for_node_readonly(&node_id);
        let forwards = manager
            .as_ref()
            .map(|manager| manager.list_forwards())
            .unwrap_or_default();
        let forwards_for_remote_ports = forwards.clone();
        let has_background = self.terminal_background_preferences("forwards").is_some();

        let mut surface = div()
            .id("forwards-view-scroll")
            .size_full()
            .overflow_y_scroll()
            .p(px(FORWARDS_PAGE_PADDING))
            .font_family(settings_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            ))
            .bg(if has_background {
                forwards_transparent()
            } else {
                rgb(theme.bg)
            })
            .child(
                div()
                    .w_full()
                    .max_w(px(FORWARDS_MAX_WIDTH))
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .gap(px(FORWARDS_SECTION_GAP))
                    .when(!self.forwarding_view.new_ports.is_empty(), |page| {
                        page.child(self.render_port_detection_banner(
                            node_id.clone(),
                            tab_id,
                            self.forwarding_view.new_ports.clone(),
                            has_background,
                            cx,
                        ))
                    })
                    .child(self.render_forwards_quick_actions(
                        node_id.clone(),
                        node_ready,
                        tab_id,
                        has_background,
                        cx,
                    ))
                    .child(self.render_forwards_separator(has_background))
                    .child(self.render_forwards_table(
                        node_id.clone(),
                        tab_id,
                        forwards,
                        manager,
                        has_background,
                        cx,
                    ))
                    .when(self.forwarding_view.show_new_form, |page| {
                        page.child(self.render_forward_create_form(
                            node_id.clone(),
                            tab_id,
                            has_background,
                            cx,
                        ))
                    })
                    .when_some(self.forwarding_view.error.as_ref(), |page, error| {
                        page.child(self.render_forwards_error(error))
                    })
                    .child(self.render_forwards_separator(has_background))
                    .child(self.render_remote_ports_section(
                        node_id.clone(),
                        tab_id,
                        &forwards_for_remote_ports,
                        has_background,
                        cx,
                    )),
            );
        if self.forwarding_view.editing_forward.is_some() {
            surface = surface.child(self.render_forward_edit_modal(
                node_id.clone(),
                tab_id,
                has_background,
                cx,
            ));
        }
        if self.forwarding_view.pending_delete_forward.is_some() {
            surface = surface.child(self.render_forward_delete_confirm(
                node_id,
                tab_id,
                has_background,
                cx,
            ));
        }
        surface.into_any_element()
    }

    fn render_forwards_quick_actions(
        &self,
        node_id: NodeId,
        node_ready: bool,
        tab_id: TabId,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(self.render_forwards_section_title(self.i18n.t("forwards.quick.title")))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.0))
                    .child(self.render_forwards_quick_button(
                        "forwards.quick.jupyter",
                        TW_ORANGE_500,
                        node_id.clone(),
                        tab_id,
                        8888,
                        node_ready,
                        has_background,
                        cx,
                    ))
                    .child(self.render_forwards_quick_button(
                        "forwards.quick.tensorboard",
                        TW_BLUE_500,
                        node_id.clone(),
                        tab_id,
                        6006,
                        node_ready,
                        has_background,
                        cx,
                    ))
                    .child(self.render_forwards_quick_button(
                        "forwards.quick.vscode",
                        TW_CYAN_500,
                        node_id,
                        tab_id,
                        8080,
                        node_ready,
                        has_background,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_forwards_quick_button(
        &self,
        label_key: &'static str,
        dot_color: u32,
        node_id: NodeId,
        tab_id: TabId,
        port: u16,
        enabled: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(36.0))
            .px_4()
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(forwards_theme_border(theme.border, has_background))
            .bg(forwards_theme_panel_bg(theme.bg_panel, has_background))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(theme.text))
            .opacity(if enabled { 1.0 } else { 0.5 })
            .child(
                div()
                    .size(px(8.0))
                    .rounded_full()
                    .bg(forwards_palette_color(dot_color)),
            )
            .child(self.render_forward_ui_text(self.i18n.t(label_key)))
            .when(enabled, |button| {
                button
                    .cursor_pointer()
                    .hover(move |button| {
                        button.bg(forwards_theme_hover_bg(theme.bg_hover, has_background))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            let persist = this.forward_persist_context_for_node(&node_id);
                            let registry = this.forwarding_registry.clone();
                            this.start_forward_operation(
                                tab_id,
                                node_id.clone(),
                                "forwards.messages.created",
                                move |manager| {
                                    Box::pin(async move {
                                        let created = match label_key {
                                            "forwards.quick.jupyter" => {
                                                manager.forward_jupyter(port, port).await?
                                            }
                                            "forwards.quick.tensorboard" => {
                                                manager.forward_tensorboard(port, port).await?
                                            }
                                            "forwards.quick.vscode" => {
                                                manager.forward_vscode(port, port).await?
                                            }
                                            _ => unreachable!("unknown forward quick action"),
                                        };
                                        if let Some((session_id, owner_connection_id)) = persist {
                                            let forward_id = created.id.clone();
                                            let _ = registry.sync_persisted_forward_rule(
                                                &forward_id,
                                                &session_id,
                                                owner_connection_id,
                                                created,
                                            );
                                        }
                                        Ok(())
                                    })
                                },
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
            })
            .into_any_element()
    }

    fn render_forwards_table(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        forwards: Vec<ForwardRule>,
        manager: Option<Arc<ForwardingManager>>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let forward_count = forwards.len();
        let has_running_forwards = forwards.iter().any(|rule| {
            matches!(
                rule.status,
                ForwardStatus::Active | ForwardStatus::Starting | ForwardStatus::Error
            )
        });
        let has_suspended_forwards = forwards
            .iter()
            .any(|rule| matches!(rule.status, ForwardStatus::Suspended));
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(self.render_forwards_section_title(self.i18n.t("forwards.table.title")))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(self.render_forward_icon_button(
                                LucideIcon::RefreshCcw,
                                theme.text_muted,
                                has_background,
                                cx.listener(|_this, _event, _window, cx| {
                                    cx.notify();
                                    cx.stop_propagation();
                                }),
                            ))
                            .when(has_running_forwards, |actions| {
                                actions.child(
                                    self.render_forward_button(
                                        self.i18n.t("forwards.actions.stop_all"),
                                        Some(LucideIcon::Square),
                                        ForwardButtonVariant::Secondary,
                                        true,
                                        has_background,
                                        cx.listener({
                                            let node_id = node_id.clone();
                                            move |this, _event, _window, cx| {
                                                this.stop_all_forward_rules(
                                                    tab_id,
                                                    node_id.clone(),
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                            }
                                        }),
                                    )
                                    .h(px(32.0))
                                    .px_3()
                                    .text_size(px(self.tokens.metrics.ui_text_xs)),
                                )
                            })
                            .when(has_running_forwards, |actions| {
                                actions.child(
                                    self.render_forward_button(
                                        self.i18n.t("forwards.actions.suspend_all"),
                                        Some(LucideIcon::Pause),
                                        ForwardButtonVariant::Secondary,
                                        true,
                                        has_background,
                                        cx.listener({
                                            let node_id = node_id.clone();
                                            move |this, _event, _window, cx| {
                                                this.suspend_all_forward_rules(
                                                    tab_id,
                                                    node_id.clone(),
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                            }
                                        }),
                                    )
                                    .h(px(32.0))
                                    .px_3()
                                    .text_size(px(self.tokens.metrics.ui_text_xs)),
                                )
                            })
                            .when(has_suspended_forwards, |actions| {
                                actions.child(
                                    self.render_forward_button(
                                        self.i18n.t("forwards.actions.restore_suspended"),
                                        Some(LucideIcon::Play),
                                        ForwardButtonVariant::Secondary,
                                        true,
                                        has_background,
                                        cx.listener({
                                            let node_id = node_id.clone();
                                            move |this, _event, _window, cx| {
                                                this.restore_suspended_forward_rules(
                                                    tab_id,
                                                    node_id.clone(),
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                            }
                                        }),
                                    )
                                    .h(px(32.0))
                                    .px_3()
                                    .text_size(px(self.tokens.metrics.ui_text_xs)),
                                )
                            })
                            .child(
                                self.render_forward_button(
                                    self.i18n.t("forwards.actions.new_forward"),
                                    Some(LucideIcon::Plus),
                                    if self.forwarding_view.show_new_form {
                                        ForwardButtonVariant::Secondary
                                    } else {
                                        ForwardButtonVariant::Primary
                                    },
                                    true,
                                    has_background,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.forwarding_view.show_new_form =
                                            !this.forwarding_view.show_new_form;
                                        this.forwarding_view.error = None;
                                        cx.notify();
                                        cx.stop_propagation();
                                    }),
                                )
                                .h(px(32.0))
                                .px_3()
                                .text_size(px(self.tokens.metrics.ui_text_xs)),
                            ),
                    ),
            )
            .child(
                div()
                    .min_h(px(100.0))
                    .w_full()
                    .overflow_hidden()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(forwards_theme_border(theme.border, has_background))
                    .bg(forwards_theme_card_bg(theme.bg_card, has_background))
                    .child(self.render_forward_table_header(has_background))
                    .when(forwards.is_empty(), |table| {
                        table.child(
                            div()
                                .h(px(120.0))
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .gap(px(12.0))
                                .rounded_b(px(self.tokens.radii.lg))
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .text_color(rgb(theme.text_muted))
                                .child(Self::render_lucide_icon(
                                    LucideIcon::ArrowUpDown,
                                    40.0,
                                    forwards_theme_with_alpha(
                                        theme.text_muted,
                                        FORWARDS_TW_ALPHA_30,
                                    ),
                                ))
                                .child(self.render_forward_ui_text(
                                    self.i18n.t("forwards.table.no_forwards"),
                                )),
                        )
                    })
                    .children(forwards.into_iter().enumerate().map(|(index, rule)| {
                        let stats = matches!(rule.status, ForwardStatus::Active)
                            .then(|| {
                                manager
                                    .as_ref()
                                    .and_then(|manager| manager.get_stats(&rule.id).ok())
                            })
                            .flatten();
                        self.render_forward_row(
                            node_id.clone(),
                            tab_id,
                            rule,
                            stats,
                            index + 1 == forward_count,
                            has_background,
                            cx,
                        )
                    })),
            )
            .into_any_element()
    }

    fn render_forward_table_header(&self, has_background: bool) -> AnyElement {
        let theme = self.tokens.ui;
        self.forward_row_base(
            FORWARDS_TABLE_HEADER_H,
            forwards_theme_panel_bg(theme.bg_panel, has_background),
            ForwardRowCorners::Top,
        )
        .border_b_1()
        .border_color(forwards_theme_border(theme.border, has_background))
        .text_size(px(self.tokens.metrics.ui_text_sm))
        .text_color(rgb(theme.text_muted))
        .child(self.forward_cell(0.9, self.i18n.t("forwards.table.type")))
        .child(self.forward_cell(1.35, self.i18n.t("forwards.table.local_address")))
        .child(self.forward_cell(1.35, self.i18n.t("forwards.table.remote_address")))
        .child(self.forward_cell(1.6, self.i18n.t("forwards.table.status")))
        .child(
            div()
                .w(px(128.0))
                .pr(px(16.0))
                .text_align(gpui::TextAlign::Right)
                .child(self.render_forward_ui_text(self.i18n.t("forwards.table.actions"))),
        )
        .into_any_element()
    }

    fn render_forward_row(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        rule: ForwardRule,
        stats: Option<ForwardStats>,
        rounded_bottom: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (local, remote) = forward_addresses(&rule);
        let active = matches!(rule.status, ForwardStatus::Active);
        let stopped = matches!(rule.status, ForwardStatus::Stopped);
        let rule_for_stop = rule.clone();
        let rule_for_restart = rule.clone();
        let rule_for_delete = rule.clone();
        let rule_for_edit = rule.clone();

        self.forward_row_base(
            FORWARDS_TABLE_ROW_H,
            forwards_theme_sunken_bg(theme.bg_sunken, has_background),
            if rounded_bottom {
                ForwardRowCorners::Bottom
            } else {
                ForwardRowCorners::None
            },
        )
        .border_b_1()
        .border_color(forwards_theme_border_half(theme.border, has_background))
        .hover(move |row| row.bg(forwards_theme_hover_bg(theme.bg_hover, has_background)))
        .text_size(px(self.tokens.metrics.ui_text_sm))
        .child(self.forward_cell_element(0.9, self.render_forward_type_badge(rule.forward_type)))
        .child(self.render_forward_address_cell(&rule, local, tab_id, cx))
        .child(self.forward_mono_cell(1.35, remote))
        .child(self.forward_cell_element(1.6, self.render_forward_status(&rule.status, stats)))
        .child(
            div()
                .w(px(128.0))
                .pr(px(10.0))
                .flex()
                .justify_end()
                .gap(px(4.0))
                .when(active, |actions| {
                    actions.child(self.render_forward_icon_button(
                        LucideIcon::Square,
                        theme.text_muted,
                        has_background,
                        cx.listener({
                            let node_id = node_id.clone();
                            move |this, _event, _window, cx| {
                                let forward_id = rule_for_stop.id.clone();
                                this.start_forward_operation(
                                    tab_id,
                                    node_id.clone(),
                                    "forwards.messages.stopped",
                                    move |manager| {
                                        Box::pin(async move {
                                            manager.stop_forward(&forward_id).await.map(|_| ())
                                        })
                                    },
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        }),
                    ))
                })
                .when(stopped, |actions| {
                    actions
                        .child(self.render_forward_icon_button(
                            LucideIcon::Play,
                            theme.text_muted,
                            has_background,
                            cx.listener({
                                let node_id = node_id.clone();
                                move |this, _event, _window, cx| {
                                    let forward_id = rule_for_restart.id.clone();
                                    let persist = this.forward_persist_context_for_node(&node_id);
                                    let registry = this.forwarding_registry.clone();
                                    this.start_forward_operation(
                                        tab_id,
                                        node_id.clone(),
                                        "forwards.messages.restarted",
                                        move |manager| {
                                            Box::pin(async move {
                                                let restarted =
                                                    manager.restart_forward(&forward_id).await?;
                                                if let Some((session_id, owner_connection_id)) =
                                                    persist
                                                {
                                                    let forward_id = restarted.id.clone();
                                                    let _ = registry.sync_persisted_forward_rule(
                                                        &forward_id,
                                                        &session_id,
                                                        owner_connection_id,
                                                        restarted,
                                                    );
                                                }
                                                Ok(())
                                            })
                                        },
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }
                            }),
                        ))
                        .child(self.render_forward_icon_button(
                            LucideIcon::Pencil,
                            theme.text_muted,
                            has_background,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_forward_edit_form(rule_for_edit.clone(), cx);
                                cx.stop_propagation();
                            }),
                        ))
                })
                .when(matches!(rule.status, ForwardStatus::Suspended), |actions| {
                    actions.child(
                        div()
                            .px_2()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(forwards_palette_alpha(TW_ORANGE_400, FORWARDS_TW_ALPHA_50))
                            .child(self.render_forward_ui_text(
                                self.i18n.t("forwards.actions.will_recover"),
                            )),
                    )
                })
                .child(self.render_forward_icon_button(
                    LucideIcon::Trash2,
                    theme.text_muted,
                    has_background,
                    cx.listener(move |this, _event, _window, cx| {
                        this.forwarding_view.pending_delete_forward = Some(rule_for_delete.clone());
                        this.forwarding_view.error = None;
                        cx.notify();
                        cx.stop_propagation();
                    }),
                )),
        )
        .into_any_element()
    }

    fn render_forward_address_cell(
        &self,
        rule: &ForwardRule,
        address: String,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let should_copy = rule.forward_type != ForwardType::Remote
            && matches!(rule.status, ForwardStatus::Active);
        if !should_copy {
            return self.forward_mono_cell(1.35, address);
        }

        let forward_id = rule.id.clone();
        let copied = self.forwarding_view.copied_forward_id.as_deref() == Some(&forward_id);
        self.forward_cell_element(
            1.35,
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .truncate()
                .font_family(self.forward_mono_font())
                .text_color(rgb(self.tokens.ui.text))
                .hover({
                    let accent = self.tokens.ui.accent;
                    move |cell| cell.text_color(rgb(accent))
                })
                .cursor_pointer()
                .child(address.clone())
                .child(Self::render_lucide_icon(
                    if copied {
                        LucideIcon::Check
                    } else {
                        LucideIcon::Copy
                    },
                    12.0,
                    if copied {
                        forwards_palette_color(TW_GREEN_400)
                    } else {
                        rgb(self.tokens.ui.text_muted)
                    },
                ))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(address.clone()));
                        this.forwarding_view.copied_forward_id = Some(forward_id.clone());
                        cx.notify();

                        let copied_forward_id = forward_id.clone();
                        cx.spawn(async move |weak, cx| {
                            Timer::after(Duration::from_secs(2)).await;
                            let _ = weak.update(cx, |this, cx| {
                                if this.forwarding_view.copied_forward_id.as_deref()
                                    == Some(copied_forward_id.as_str())
                                {
                                    this.forwarding_view.copied_forward_id = None;
                                    cx.notify();
                                }
                            });
                        })
                        .detach();
                        let _ = tab_id;
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
        )
    }

}
