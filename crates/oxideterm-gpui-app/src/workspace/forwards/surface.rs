use gpui::prelude::*;

use super::helpers::{
    ForwardButtonVariant, ForwardRowCorners, forward_addresses, forwards_palette_alpha,
    forwards_palette_color, forwards_theme_border, forwards_theme_border_half,
    forwards_theme_card_bg, forwards_theme_card_surface, forwards_theme_hover_bg,
    forwards_theme_panel_bg, forwards_theme_sunken_bg, forwards_theme_with_alpha,
    forwards_transparent,
};
use super::{
    ActiveSurface, AnyElement, App, ButtonOptions, ButtonRadius, ButtonSize, ClipboardItem,
    Context, DefaultHasher, Duration, FORWARDS_PAGE_PADDING, FORWARDS_SECTION_GAP,
    FORWARDS_SECTION_LIST_ESTIMATED_HEIGHT, FORWARDS_SECTION_LIST_OVERSCAN,
    FORWARDS_TABLE_HEADER_H, FORWARDS_TABLE_ROW_H, FORWARDS_TABLE_ROW_LIST_OVERSCAN,
    FORWARDS_TW_ALPHA_30, FORWARDS_TW_ALPHA_50, ForwardRule, ForwardStats, ForwardStatus,
    ForwardType, ForwardingQuickAction, ForwardingRuntimeOperation, Hash, HashMap, Hasher,
    LucideIcon, MouseButton, NodeId, NodeReadiness, TW_BLUE_500, TW_CYAN_500, TW_GREEN_400,
    TW_ORANGE_400, TW_ORANGE_500, Tab, TabId, TabKind, TabTitleSource, TauriVirtualListSpec,
    ToolbarButtonOptions, UiButtonVariant, Window, WorkspaceApp, div, px, rgb,
    rounded_shell_child_radius, settings_ui_font_family,
    sync_tauri_variable_list_state_by_signatures, tauri_virtual_list,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ForwardsSection {
    PortDetection,
    QuickActions,
    Separator,
    Table,
    CreateForm,
    Error,
    RemotePorts,
}

fn forward_rule_row_signature(rule: &ForwardRule) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Forward table rows expose rule identity, addresses, type, status, and
    // description. Live byte counters do not affect row height, so they stay
    // out of the ListState splice signature.
    rule.id.hash(&mut hasher);
    format!("{:?}", rule.forward_type).hash(&mut hasher);
    rule.bind_address.hash(&mut hasher);
    rule.bind_port.hash(&mut hasher);
    rule.target_host.hash(&mut hasher);
    rule.target_port.hash(&mut hasher);
    format!("{:?}", rule.status).hash(&mut hasher);
    rule.description.hash(&mut hasher);
    hasher.finish()
}

impl WorkspaceApp {
    pub(in crate::workspace) fn open_forwards_tab(
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
        let tab_id = if let Some(tab_id) = self.forwarding.read(cx).tab_for_node(&node_id) {
            tab_id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::Forwards,
                    title,
                    title_source: TabTitleSource::Static,
                    root_pane: None,
                    active_pane_id: None,
                },
                cx,
            );
            self.forwarding.update(cx, |forwarding, _cx| {
                forwarding.map_tab_to_node(tab_id, node_id.clone(), _cx);
            });
            tab_id
        };

        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.active_ssh_node_id = Some(node_id.clone());
        // Tauri opens the forwarding surface against the selected node but
        // leaves connection establishment to the explicit connect path.
        self.forwarding
            .update(cx, |forwarding, _cx| forwarding.clear_error());
        self.start_port_profiler_for_node(node_id, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn render_forwards_surface(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(tab_id) = self.active_tab_id(cx) else {
            return self.render_empty_workspace(f32::from(window.viewport_size().width), cx);
        };
        self.render_forwards_surface_for_tab(tab_id, window, cx)
    }

    pub(in crate::workspace) fn render_forwards_surface_for_tab(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(node_id) = self.forwarding.read(cx).node_for_tab(tab_id) else {
            return self.render_empty_workspace(f32::from(window.viewport_size().width), cx);
        };
        // Detached renders are also mount boundaries for Entity-owned sampling.
        self.sync_forwarding_sampling_visibility(cx);
        self.sync_forwards_section_list_state(tab_id, &node_id, cx);
        let has_background = self.background_surface_active("forwards");
        let state = self.forwarding.read(cx).section_list_state.clone();
        let workspace = cx.entity();
        let spec = self.forwards_section_list_spec();
        let list_node_id = node_id.clone();
        div()
            .id("forwards-view-scroll")
            .size_full()
            .font_family(settings_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            ))
            .bg(if has_background {
                forwards_transparent()
            } else {
                rgb(theme.bg)
            })
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    workspace.update(cx, |this, cx| {
                        this.render_forwards_section_item(index, tab_id, list_node_id.clone(), cx)
                    })
                },
            ))
            .into_any_element()
    }

    fn sync_forwards_section_list_state(
        &mut self,
        tab_id: TabId,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) {
        let spec = self.forwards_section_list_spec();
        let identity = format!("forwards:{}:{}", tab_id.0, node_id.0);
        let signatures = self.forwards_section_signatures(node_id, cx);
        self.forwarding.update(cx, |forwarding, _cx| {
            sync_tauri_variable_list_state_by_signatures(
                &forwarding.section_list_state,
                &mut forwarding.section_list_cache.borrow_mut(),
                &identity,
                &signatures,
                spec,
            );
        });
    }

    fn forwards_section_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(FORWARDS_SECTION_LIST_ESTIMATED_HEIGHT),
            FORWARDS_SECTION_LIST_OVERSCAN,
        )
    }

    fn forwards_sections(&self, cx: &App) -> Vec<ForwardsSection> {
        let forwarding_view = self.forwarding.read(cx).view();
        let mut sections = Vec::new();
        if !forwarding_view.new_ports.is_empty() {
            sections.push(ForwardsSection::PortDetection);
        }
        sections.extend([
            ForwardsSection::QuickActions,
            ForwardsSection::Separator,
            ForwardsSection::Table,
        ]);
        if forwarding_view.show_new_form {
            sections.push(ForwardsSection::CreateForm);
        }
        if forwarding_view.error.is_some() {
            sections.push(ForwardsSection::Error);
        }
        sections.extend([ForwardsSection::Separator, ForwardsSection::RemotePorts]);
        sections
    }

    fn forwards_section_signatures(&self, node_id: &NodeId, cx: &App) -> Vec<u64> {
        self.forwards_sections(cx)
            .into_iter()
            .enumerate()
            .map(|(index, section)| self.forwards_section_signature(index, section, node_id, cx))
            .collect()
    }

    fn forwards_section_signature(
        &self,
        index: usize,
        section: ForwardsSection,
        node_id: &NodeId,
        cx: &App,
    ) -> u64 {
        let forwarding_view = self.forwarding.read(cx).view();
        let mut hasher = DefaultHasher::new();
        // Forward rows, detected ports, and form/error visibility all affect
        // section height. Hash those states so GPUI ListState remeasures after
        // port operations instead of reusing stale browser-section geometry.
        index.hash(&mut hasher);
        section.hash(&mut hasher);
        node_id.hash(&mut hasher);
        match section {
            ForwardsSection::PortDetection => {
                forwarding_view.new_ports.len().hash(&mut hasher);
            }
            ForwardsSection::QuickActions => {
                self.ssh_nodes
                    .get(node_id)
                    .map(|node| node.readiness == NodeReadiness::Ready)
                    .hash(&mut hasher);
            }
            ForwardsSection::Table | ForwardsSection::RemotePorts => {
                let snapshot = self.forwarding.read(cx).runtime_snapshot(node_id);
                snapshot.rules.len().hash(&mut hasher);
                for rule in snapshot.rules {
                    rule.id.hash(&mut hasher);
                    format!("{:?}", rule.status).hash(&mut hasher);
                }
            }
            ForwardsSection::CreateForm => {
                format!("{:?}", forwarding_view.forward_type).hash(&mut hasher);
                forwarding_view.skip_health_check.hash(&mut hasher);
            }
            ForwardsSection::Error => {
                forwarding_view.error.hash(&mut hasher);
            }
            ForwardsSection::Separator => {}
        }
        hasher.finish()
    }

    fn render_forwards_section_item(
        &mut self,
        index: usize,
        tab_id: TabId,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(section) = self.forwards_sections(cx).get(index).copied() else {
            return div().into_any_element();
        };
        let has_background = self.background_surface_active("forwards");
        let mut inner = div()
            .w_full()
            .min_w(px(0.0))
            .max_w(px(self.forwards_content_outer_max_width()))
            .px(px(FORWARDS_PAGE_PADDING))
            .pb(px(FORWARDS_SECTION_GAP));
        if index == 0 {
            inner = inner.pt(px(FORWARDS_PAGE_PADDING));
        }
        if index + 1 == self.forwards_sections(cx).len() {
            inner = inner.pb(px(FORWARDS_PAGE_PADDING));
        }

        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .justify_center()
            // GPUI List rows need a full-width wrapper before applying the
            // native wide-content cap to the inner forwarding shell.
            .child(inner.child(self.render_forwards_section(
                section,
                tab_id,
                node_id,
                has_background,
                cx,
            )))
            .into_any_element()
    }

    fn forwards_content_outer_max_width(&self) -> f32 {
        // Forwarding tables are an operational surface, so use the native
        // wide-page metric instead of the older browser `max-w-4xl` limit.
        self.tokens.metrics.settings_content_wide_max_width + FORWARDS_PAGE_PADDING * 2.0
    }

    fn render_forwards_section(
        &self,
        section: ForwardsSection,
        tab_id: TabId,
        node_id: NodeId,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match section {
            ForwardsSection::PortDetection => self.render_port_detection_banner(
                node_id,
                tab_id,
                self.forwarding.read(cx).view().new_ports.clone(),
                has_background,
                cx,
            ),
            ForwardsSection::QuickActions => {
                let node_ready = self
                    .ssh_nodes
                    .get(&node_id)
                    .is_some_and(|node| node.readiness == NodeReadiness::Ready);
                self.render_forwards_quick_actions(node_id, node_ready, tab_id, has_background, cx)
            }
            ForwardsSection::Separator => self.render_forwards_separator(has_background),
            ForwardsSection::Table => {
                let snapshot = self.forwarding.read(cx).runtime_snapshot(&node_id);
                self.render_forwards_table(
                    node_id,
                    tab_id,
                    snapshot.rules,
                    snapshot.stats_by_forward_id,
                    has_background,
                    cx,
                )
            }
            ForwardsSection::CreateForm => {
                self.render_forward_create_form(node_id, tab_id, has_background, cx)
            }
            ForwardsSection::Error => self
                .forwarding
                .read(cx)
                .view()
                .error
                .as_ref()
                .map(|error| self.render_forwards_error(error))
                .unwrap_or_else(|| div().into_any_element()),
            ForwardsSection::RemotePorts => {
                let forwards = self.forwarding.read(cx).runtime_snapshot(&node_id).rules;
                self.render_remote_ports_section(node_id, tab_id, &forwards, has_background, cx)
            }
        }
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
        self.workspace_toolbar_action_button(
            String::new(),
            Some(
                div()
                    .size(px(8.0))
                    .rounded_full()
                    .bg(forwards_palette_color(dot_color))
                    .into_any_element(),
            ),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: UiButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: !enabled,
                },
                show_label: false,
                icon_gap: Some(8.0),
                background: Some(forwards_theme_panel_bg(theme.bg_panel, has_background)),
                border: Some(forwards_theme_border(theme.border, has_background)),
                text_color: Some(rgb(theme.text)),
                hover_background: Some(forwards_theme_hover_bg(theme.bg_hover, has_background)),
                height: Some(36.0),
                padding_x: Some(16.0),
                font_size: Some(self.tokens.metrics.ui_text_sm),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, _window, cx| {
                let action = match label_key {
                    "forwards.quick.jupyter" => ForwardingQuickAction::Jupyter,
                    "forwards.quick.tensorboard" => ForwardingQuickAction::Tensorboard,
                    "forwards.quick.vscode" => ForwardingQuickAction::Vscode,
                    _ => unreachable!("unknown forward quick action"),
                };
                this.start_forward_operation(
                    tab_id,
                    node_id.clone(),
                    "forwards.messages.created",
                    true,
                    ForwardingRuntimeOperation::Quick { action, port },
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        // The visible label keeps the Forwards CJK font fallback instead of
        // using toolbar_button's plain String label path.
        .child(self.render_forward_ui_text(self.i18n.t(label_key)))
        .into_any_element()
    }

    fn render_forwards_table(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        forwards: Vec<ForwardRule>,
        stats_by_forward_id: HashMap<String, ForwardStats>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let forward_count = forwards.len();
        let show_new_form = self.forwarding.read(cx).view().show_new_form;
        self.sync_forwards_table_row_list_state(&forwards, cx);
        let table_row_state = self.forwarding.read(cx).table_row_list_state.clone();
        let table_row_spec = self.forwards_table_row_list_spec();
        let workspace = cx.entity();
        let refresh_node_id = node_id.clone();
        let row_node_id = node_id;
        let row_stats_by_forward_id = stats_by_forward_id;
        let row_forwards = forwards.clone();
        let row_has_background = has_background;
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
                                move |this, _event, _window, cx| {
                                    this.forwarding.update(cx, |forwarding, _cx| {
                                        forwarding.refresh_runtime_snapshot(&refresh_node_id);
                                    });
                                    cx.notify();
                                    cx.stop_propagation();
                                },
                                cx,
                            ))
                            .child(
                                self.render_forward_button(
                                    self.i18n.t("forwards.actions.new_forward"),
                                    Some(LucideIcon::Plus),
                                    if show_new_form {
                                        ForwardButtonVariant::Secondary
                                    } else {
                                        ForwardButtonVariant::Primary
                                    },
                                    true,
                                    has_background,
                                    cx.listener(|this, _event, _window, cx| {
                                        let show_new_form =
                                            this.forwarding.read(cx).view().show_new_form;
                                        if show_new_form {
                                            this.begin_forward_create_form_exit(cx);
                                        } else {
                                            this.forwarding.update(cx, |forwarding, _cx| {
                                                forwarding.open_create_form();
                                            });
                                        }
                                        this.forwarding.update(cx, |forwarding, _cx| {
                                            forwarding.clear_error();
                                        });
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
                forwards_theme_card_surface(
                    div()
                        .min_h(px(100.0))
                        .w_full()
                        .overflow_hidden()
                        .rounded(px(self.tokens.radii.lg))
                        .border_1()
                        .border_color(forwards_theme_border(theme.border, has_background))
                        .bg(forwards_theme_card_bg(theme.bg_card, has_background)),
                    theme.bg_card,
                )
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
                            .rounded_b(px(rounded_shell_child_radius(self.tokens.radii.lg)))
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(theme.text_muted))
                            .child(Self::render_lucide_icon(
                                LucideIcon::ArrowUpDown,
                                40.0,
                                forwards_theme_with_alpha(theme.text_muted, FORWARDS_TW_ALPHA_30),
                            ))
                            .child(
                                self.render_forward_ui_text(
                                    self.i18n.t("forwards.table.no_forwards"),
                                ),
                            ),
                    )
                })
                .when(!row_forwards.is_empty(), |table| {
                    table.child(
                        div()
                            .h(px(forward_count as f32 * FORWARDS_TABLE_ROW_H))
                            .child(tauri_virtual_list(
                                table_row_state,
                                table_row_spec,
                                move |index, _window, cx| {
                                    let Some(rule) = row_forwards.get(index).cloned() else {
                                        return div().into_any_element();
                                    };
                                    let node_id = row_node_id.clone();
                                    let stats = row_stats_by_forward_id.get(&rule.id).cloned();
                                    workspace.update(cx, |this, cx| {
                                        this.render_forward_row(
                                            node_id,
                                            tab_id,
                                            rule,
                                            stats,
                                            index + 1 == forward_count,
                                            row_has_background,
                                            cx,
                                        )
                                    })
                                },
                            )),
                    )
                }),
            )
            .into_any_element()
    }

    fn sync_forwards_table_row_list_state(&self, forwards: &[ForwardRule], cx: &mut Context<Self>) {
        let signatures = forwards
            .iter()
            .map(forward_rule_row_signature)
            .collect::<Vec<_>>();
        let spec = self.forwards_table_row_list_spec();
        self.forwarding.update(cx, |forwarding, _cx| {
            sync_tauri_variable_list_state_by_signatures(
                &forwarding.table_row_list_state,
                &mut forwarding.table_row_list_cache.borrow_mut(),
                "forwards-table-rows",
                &signatures,
                spec,
            );
        });
    }

    fn forwards_table_row_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(px(FORWARDS_TABLE_ROW_H), FORWARDS_TABLE_ROW_LIST_OVERSCAN)
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
                        {
                            let node_id = node_id.clone();
                            move |this, _event, _window, cx| {
                                let forward_id = rule_for_stop.id.clone();
                                this.start_forward_operation(
                                    tab_id,
                                    node_id.clone(),
                                    "forwards.messages.stopped",
                                    false,
                                    ForwardingRuntimeOperation::Stop { forward_id },
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        },
                        cx,
                    ))
                })
                .when(stopped, |actions| {
                    actions
                        .child(self.render_forward_icon_button(
                            LucideIcon::Play,
                            theme.text_muted,
                            has_background,
                            {
                                let node_id = node_id.clone();
                                move |this, _event, _window, cx| {
                                    let forward_id = rule_for_restart.id.clone();
                                    this.start_forward_operation(
                                        tab_id,
                                        node_id.clone(),
                                        "forwards.messages.restarted",
                                        true,
                                        ForwardingRuntimeOperation::Restart { forward_id },
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }
                            },
                            cx,
                        ))
                        .child(self.render_forward_icon_button(
                            LucideIcon::Pencil,
                            theme.text_muted,
                            has_background,
                            move |this, _event, _window, cx| {
                                this.open_forward_edit_form(rule_for_edit.clone(), cx);
                                cx.stop_propagation();
                            },
                            cx,
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
                    move |this, _event, _window, cx| {
                        this.forwarding.update(cx, |forwarding, _cx| {
                            forwarding.request_delete(rule_for_delete.clone());
                        });
                        cx.notify();
                        cx.stop_propagation();
                    },
                    cx,
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
        let copied =
            self.forwarding.read(cx).view().copied_forward_id.as_deref() == Some(&forward_id);
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
                        this.forwarding.update(cx, |forwarding, cx| {
                            forwarding.mark_forward_copied(
                                forward_id.clone(),
                                Duration::from_secs(2),
                                cx,
                            );
                        });
                        let _ = tab_id;
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
        )
    }
}
