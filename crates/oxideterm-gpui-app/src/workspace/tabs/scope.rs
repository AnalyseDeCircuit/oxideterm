use super::*;
use oxideterm_gpui_ui::button::{ButtonRadius, ButtonVariant};

impl WorkspaceApp {
    fn collect_tab_affinities(&self, cx: &App) -> HashMap<TabId, Vec<TabAffinity>> {
        self.tab_host
            .read(cx)
            .unfiltered_main_tabs()
            .map(|(_, tab)| (tab.id, self.tab_affinities(tab.id, cx)))
            .collect()
    }

    pub(in crate::workspace) fn sync_tab_scope(&mut self, cx: &mut App) {
        if *self.tab_host.read(cx).scope() == TabScope::All {
            return;
        }
        let affinities = self.collect_tab_affinities(cx);
        let previous = self.tab_host.update(cx, |host, _| {
            host.update_affinities(affinities);
            host.reconcile_scope_selection()
        });
        self.apply_main_window_active_tab_change(previous, self.active_tab_id(cx), cx);
    }

    pub(in crate::workspace) fn reveal_tab_scope(&mut self, tab: TabId, cx: &mut App) {
        let scope = self.tab_host.read(cx).scope().clone();
        if scope == TabScope::All {
            return;
        }
        let container = self.tab_host.read(cx).container_tab_id(tab);
        let affinities = self.collect_tab_affinities(cx);
        let target = affinities.get(&container).and_then(|values| {
            if values
                .iter()
                .any(|value| scope == TabScope::Affinity(value.clone()))
            {
                None
            } else {
                values.first().cloned().map(TabScope::Affinity)
            }
        });
        self.tab_host.update(cx, |host, _| {
            host.update_affinities(affinities);
            if let Some(scope) = target {
                host.prepare_scope(scope);
            }
        });
    }

    pub(in crate::workspace) fn select_tab_scope(
        &mut self,
        scope: TabScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_workspace_tooltip("titlebar-tab-scope", cx);
        let affinities = self.collect_tab_affinities(cx);
        let previous = self.tab_host.update(cx, |host, _| {
            host.update_affinities(affinities);
            host.switch_scope(scope)
        });
        self.apply_main_window_active_tab_change(previous, self.active_tab_id(cx), cx);
        self.main_window_tabs
            .scroll_handle
            .set_offset(Point::new(px(0.0), px(0.0)));
        self.main_window_tabs.drag = None;
        self.sync_active_tab_surface(cx);
        self.focus_active_tab_keyboard_owner(window, cx);
        self.reveal_active_tab(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn tab_scope_label(&self, scope: &TabScope, cx: &App) -> String {
        match scope {
            TabScope::All => self.i18n.t("sidebar.tab_scope.all"),
            TabScope::Affinity(TabAffinity::Global) => self.i18n.t("sidebar.tab_scope.global"),
            TabScope::Affinity(TabAffinity::LocalTerminal) => {
                self.i18n.t("sidebar.tab_scope.local")
            }
            TabScope::Affinity(TabAffinity::Ssh(node)) => self
                .ssh_nodes
                .get(node)
                .map(|node| node.title.clone())
                .unwrap_or_else(|| self.i18n.t("sidebar.tab_scope.connection")),
            TabScope::Affinity(TabAffinity::Standalone(id)) => self
                .standalone_connections
                .record(id)
                .map(|record| record.title.clone())
                .unwrap_or_else(|| self.i18n.t("sidebar.tab_scope.connection")),
            TabScope::Affinity(TabAffinity::FileTransfer(id)) => self
                .standalone_sftp_sessions
                .get(id)
                .map(|session| session.title.clone())
                .or_else(|| {
                    self.standalone_sftp_tabs
                        .iter()
                        .find(|(_, binding)| binding.contains_endpoint(id))
                        .and_then(|(tab, _)| self.tab_by_id(*tab, cx))
                        .map(|tab| tab.title.clone())
                })
                .unwrap_or_else(|| self.i18n.t("sidebar.panels.sftp")),
            TabScope::Affinity(TabAffinity::Plugin(id)) => self
                .plugin_entity
                .read(cx)
                .connections
                .profile_for_instance(id)
                .and_then(|profile| {
                    self.connection_store
                        .plugin_profiles()
                        .iter()
                        .find(|value| value.id == profile)
                })
                .map(|profile| profile.name.clone())
                .unwrap_or_else(|| self.i18n.t("sidebar.tab_scope.connection")),
        }
    }

    pub(in crate::workspace) fn tab_scope_choices(
        &self,
        cx: &App,
    ) -> Vec<(TabScope, String, Option<String>)> {
        let mut connections = Vec::new();
        let mut identities = HashSet::new();
        for (id, node) in &self.ssh_nodes {
            let scope = TabScope::Affinity(TabAffinity::Ssh(id.clone()));
            identities.insert(scope.clone());
            connections.push((
                scope,
                node.title.clone(),
                Some(format!(
                    "{}@{}:{}",
                    node.endpoint.username, node.endpoint.host, node.endpoint.port
                )),
            ));
        }
        let mut add = |affinity: TabAffinity| {
            let scope = TabScope::Affinity(affinity);
            if identities.insert(scope.clone()) {
                let label = self.tab_scope_label(&scope, cx);
                connections.push((scope, label, None));
            }
        };
        for record in self.standalone_connections.records() {
            add(TabAffinity::Standalone(record.id.clone()));
        }
        for id in self.plugin_entity.read(cx).connections.sessions.keys() {
            add(TabAffinity::Plugin(id.clone()));
        }
        for id in self
            .standalone_sftp_sessions
            .keys()
            .chain(self.ftp_sessions.keys())
        {
            add(TabAffinity::FileTransfer(id.clone()));
        }
        connections.sort_by_cached_key(|(scope, label, _)| {
            let identity = match scope {
                TabScope::Affinity(TabAffinity::Ssh(node)) => node.0.as_str(),
                TabScope::Affinity(
                    TabAffinity::Standalone(id)
                    | TabAffinity::FileTransfer(id)
                    | TabAffinity::Plugin(id),
                ) => id.as_str(),
                _ => "",
            };
            (label.to_lowercase(), identity.to_owned())
        });
        let mut choices = [
            TabScope::All,
            TabScope::Affinity(TabAffinity::Global),
            TabScope::Affinity(TabAffinity::LocalTerminal),
        ]
        .into_iter()
        .map(|scope| {
            let label = self.tab_scope_label(&scope, cx);
            (scope, label, None)
        })
        .collect::<Vec<_>>();
        choices.extend(connections);
        choices
    }

    pub(in crate::workspace) fn render_tab_scope_trigger(
        &self,
        window: &Window,
        available_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let host = self.tab_host.read(cx);
        let filtered = *host.scope() != TabScope::All;
        let label = self.tab_scope_label(host.scope(), cx);
        let visible = host.main_tabs().count().to_string();
        let total = host.unfiltered_main_tabs().count().to_string();
        let count = self
            .i18n
            .t(if filtered {
                "sidebar.tab_scope.count_filtered"
            } else {
                "sidebar.tab_scope.count_all"
            })
            .replace("{{visible}}", &visible)
            .replace("{{total}}", &total);
        let tooltip = self
            .i18n
            .t(if filtered {
                "sidebar.tab_scope.filtered_tooltip"
            } else {
                "sidebar.tab_scope.all_tooltip"
            })
            .replace("{{visible}}", &visible)
            .replace("{{total}}", &total)
            .replace("{{scope}}", &label);
        let icon_size = self.tokens.metrics.ui_select_check_size;
        let gap = self.tokens.spacing.two;
        let side_width = self.tab_text_width(&count, self.tokens.metrics.ui_text_sm, window)
            + icon_size
            + gap * 2.0;
        let width = (self.tab_text_width(&label, self.tokens.metrics.ui_text_sm, window)
            + side_width * 2.0
            + self.tokens.metrics.ui_control_padding_x * 2.0)
            .clamp(100.0, 280.0)
            .min(available_width.max(0.0));
        let hover_bg = rgb(self.tokens.ui.bg_hover);
        let workspace = cx.entity().downgrade();
        oxideterm_gpui_ui::select::select_inline_trigger_chrome(&self.tokens, false, false, false)
            .id("titlebar-tab-scope")
            .w(px(width))
            .flex_none()
            .h(px(self.tokens.metrics.titlebar_height - 6.0))
            .mx(px(gap))
            .border_0()
            .bg(rgba(0x00000000))
            .role(gpui::Role::Button)
            .aria_label(tooltip.clone())
            // Balance the counter and chevron so the name stays on the window center.
            .child(
                div()
                    .w(px(side_width))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .pr(px(gap))
                    .when(filtered, |slot| {
                        slot.child(Self::render_lucide_icon(
                            LucideIcon::ListFilter,
                            icon_size,
                            rgb(self.tokens.ui.accent),
                        ))
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_align(gpui::TextAlign::Center)
                    .child(label),
            )
            .child(
                div()
                    .w(px(side_width))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(gap))
                    .pl(px(gap))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(count)
                    .child(Self::render_lucide_icon(
                        LucideIcon::ChevronDown,
                        icon_size,
                        rgb(self.tokens.ui.text_muted),
                    )),
            )
            .hover(move |trigger| trigger.bg(hover_bg))
            .on_mouse_move(
                cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                    this.queue_workspace_tooltip(
                        "titlebar-tab-scope",
                        tooltip.clone(),
                        f32::from(event.position.x) + 12.0,
                        f32::from(event.position.y) + 16.0,
                        cx,
                    );
                }),
            )
            .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                if !*hovered {
                    this.clear_workspace_tooltip("titlebar-tab-scope", cx);
                }
            }))
            .on_a11y_action(gpui::AccessibleAction::Click, move |_, window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.clear_workspace_tooltip("titlebar-tab-scope", cx);
                    this.open_tab_scope_picker(window, cx);
                });
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.clear_workspace_tooltip("titlebar-tab-scope", cx);
                    this.open_tab_scope_picker(window, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_empty_tab_scope(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        oxideterm_gpui_ui::empty_state(
            &self.tokens,
            Self::render_lucide_icon(LucideIcon::LayoutList, 24.0, rgb(self.tokens.ui.accent)),
            self.i18n.t("sidebar.tab_scope.empty"),
            None,
            Some(
                oxideterm_gpui_ui::button::button_with(
                    &self.tokens,
                    self.i18n.t("sidebar.tab_scope.show_all"),
                    oxideterm_gpui_ui::button::ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: oxideterm_gpui_ui::button::ButtonSize::Default,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.select_tab_scope(TabScope::All, window, cx);
                    }),
                )
                .into_any_element(),
            ),
        )
        .size_full()
        .into_any_element()
    }
}
