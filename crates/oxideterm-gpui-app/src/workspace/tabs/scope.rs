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

    pub(in crate::workspace) fn tab_scope_counts(
        &self,
        cx: &App,
    ) -> (usize, HashMap<TabAffinity, usize>) {
        let affinities = self.collect_tab_affinities(cx);
        let total = affinities.len();
        let mut counts = HashMap::new();
        for values in affinities.values() {
            for affinity in values {
                *counts.entry(affinity.clone()).or_insert(0) += 1;
            }
        }
        (total, counts)
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
        self.clear_workspace_tooltip("titlebar-show-all-tabs", cx);
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

    fn bind_tab_scope_picker_trigger(
        &self,
        trigger: gpui::Stateful<gpui::Div>,
        tooltip: String,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let workspace = cx.entity().downgrade();
        trigger
            .debug_selector(|| "titlebar-tab-scope".into())
            .role(gpui::Role::Button)
            .aria_label(tooltip.clone())
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
    }

    pub(in crate::workspace) fn render_tab_scope_trigger(
        &self,
        available_width: f32,
        text_color: u32,
        hover_bg: u32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let host = self.tab_host.read(cx);
        let filtered = *host.scope() != TabScope::All;
        let visible_count = host.main_tabs().count();
        let total_count = host.unfiltered_main_tabs().count();

        // Hide the scope trigger when the workspace is unfiltered and empty so
        // the titlebar remains a continuous drag bar over the Welcome view.
        if !filtered && total_count == 0 {
            return None;
        }

        let theme = self.tokens.ui;
        let label = self.tab_scope_label(host.scope(), cx);
        let visible = visible_count.to_string();
        let total = total_count.to_string();
        let pill_height = (self.tokens.metrics.titlebar_height - 8.0).max(20.0);
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

        if !filtered {
            let max_width = available_width.max(0.0).min(280.0);
            let hover_text = theme.text;
            let count_color = rgba((text_color << 8) | 0xc8);
            let trigger = div()
                .group("titlebar-tab-scope")
                .id("titlebar-tab-scope")
                .h(px(pill_height))
                .max_w(px(max_width))
                .min_w(px(0.0))
                .flex_none()
                .mx(px(self.tokens.spacing.two))
                .px(px(8.0))
                .rounded(px(self.tokens.radii.md))
                .bg(rgba(0x00000000))
                .overflow_hidden()
                .occlude()
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(5.0))
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(text_color))
                .hover(move |trigger| trigger.bg(rgb(hover_bg)).text_color(rgb(hover_text)))
                .child(div().flex_initial().min_w(px(0.0)).truncate().child(label))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(self.tokens.metrics.ui_text_2xs))
                        .text_color(count_color)
                        .child(total),
                )
                .child(
                    div().flex_none().child(
                        svg()
                            .path(LucideIcon::ChevronDown.path())
                            .size(px(12.0))
                            .text_color(rgb(text_color))
                            .group_hover("titlebar-tab-scope", move |icon| {
                                icon.text_color(rgb(hover_text))
                            }),
                    ),
                );

            return Some(
                self.bind_tab_scope_picker_trigger(trigger, tooltip, cx)
                    .into_any_element(),
            );
        }

        let max_width = available_width.max(0.0).min(320.0);
        let count = format!("{visible}/{total}");
        let show_all_label = self.i18n.t("sidebar.tab_scope.show_all");
        let show_all_tooltip = show_all_label.clone();
        let capsule_bg = rgba((theme.accent << 8) | if theme.is_light() { 0x14 } else { 0x1f });
        let capsule_border = rgba((theme.accent << 8) | 0x4d);
        let primary_hover_bg = rgba((theme.accent << 8) | 0x24);
        let divider_bg = rgba((theme.accent << 8) | 0x38);
        let clear_hover_bg = rgba((theme.accent << 8) | 0x2e);
        let clear_workspace = cx.entity().downgrade();
        let hover_text = theme.text;

        let primary = self.bind_tab_scope_picker_trigger(
            div()
                .id("titlebar-tab-scope")
                .h_full()
                .px(px(8.0))
                .gap(px(5.0))
                .flex()
                .items_center()
                .min_w(px(0.0))
                .flex_auto()
                .cursor_pointer()
                .hover(move |segment| segment.bg(primary_hover_bg))
                .child(div().flex_none().child(Self::render_lucide_icon(
                    LucideIcon::ListFilter,
                    12.0,
                    rgb(theme.accent),
                )))
                .child(
                    div()
                        .flex_initial()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .child(label),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(self.tokens.metrics.ui_text_2xs))
                        .text_color(rgb(theme.text_muted))
                        .child(count),
                )
                .child(div().flex_none().child(Self::render_lucide_icon(
                    LucideIcon::ChevronDown,
                    12.0,
                    rgb(theme.text_muted),
                ))),
            tooltip,
            cx,
        );

        Some(
            div()
                .id("titlebar-tab-scope-capsule")
                .debug_selector(|| "titlebar-tab-scope-capsule".into())
                .h(px(pill_height))
                .max_w(px(max_width))
                .min_w(px(0.0))
                .flex_none()
                .mx(px(self.tokens.spacing.two))
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(capsule_border)
                .bg(capsule_bg)
                .overflow_hidden()
                .occlude()
                .flex()
                .flex_row()
                .items_center()
                .child(primary)
                .child(div().w(px(1.0)).h(px(12.0)).flex_none().bg(divider_bg))
                .child(
                    div()
                        .group("titlebar-show-all-tabs")
                        .id("titlebar-show-all-tabs")
                        .debug_selector(|| "titlebar-show-all-tabs".into())
                        .role(gpui::Role::Button)
                        .aria_label(show_all_label)
                        .h_full()
                        .w(px(22.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(move |segment| segment.bg(clear_hover_bg))
                        .child(
                            svg()
                                .path(LucideIcon::X.path())
                                .size(px(12.0))
                                .text_color(rgb(theme.text_muted))
                                .group_hover("titlebar-show-all-tabs", move |icon| {
                                    icon.text_color(rgb(hover_text))
                                }),
                        )
                        .on_mouse_move(cx.listener(
                            move |this, event: &MouseMoveEvent, _window, cx| {
                                this.queue_workspace_tooltip(
                                    "titlebar-show-all-tabs",
                                    show_all_tooltip.clone(),
                                    f32::from(event.position.x) + 12.0,
                                    f32::from(event.position.y) + 16.0,
                                    cx,
                                );
                            },
                        ))
                        .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                            if !*hovered {
                                this.clear_workspace_tooltip("titlebar-show-all-tabs", cx);
                            }
                        }))
                        .on_a11y_action(gpui::AccessibleAction::Click, move |_, window, cx| {
                            let _ = clear_workspace.update(cx, |this, cx| {
                                this.clear_workspace_tooltip("titlebar-show-all-tabs", cx);
                                this.select_tab_scope(TabScope::All, window, cx);
                            });
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.clear_workspace_tooltip("titlebar-show-all-tabs", cx);
                                this.select_tab_scope(TabScope::All, window, cx);
                                cx.stop_propagation();
                            }),
                        ),
                )
                .into_any_element(),
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    fn tab_scope_titlebar_fixture(
        cx: &mut gpui::TestAppContext,
    ) -> Option<(Entity<WorkspaceApp>, &mut gpui::VisualTestContext)> {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_TAB_SCOPE_TEST_DIR";
        let Some(directory) = std::env::var_os(fixture_key) else {
            // Workspace storage is process-wide; isolate settings in a portable dir.
            let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
            let child = directory.path().join(executable.file_name().unwrap());
            std::fs::hard_link(&executable, &child).unwrap();
            std::fs::write(directory.path().join("portable"), []).unwrap();
            let output = std::process::Command::new(child)
                .arg(cx.test_function_name().unwrap())
                .arg("--nocapture")
                .env(fixture_key, directory.path())
                .env_remove("APPIMAGE")
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return None;
        };
        assert!(default_settings_path().starts_with(PathBuf::from(directory)));
        let mut settings = SettingsStore::load_from_path(default_settings_path()).unwrap();
        settings.settings_mut().ssh_config.auto_load_hosts = false;
        settings.settings_mut().onboarding_completed = true;
        settings.save().unwrap();
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            workspace.update(cx, |workspace, _| {
                workspace.tokens.motion.enabled = false;
            });
            window_shell::WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        Some((workspace, cx))
    }

    #[gpui::test]
    fn titlebar_tab_scope_progressive_states_and_inline_clear(cx: &mut gpui::TestAppContext) {
        let Some((workspace, cx)) = tab_scope_titlebar_fixture(cx) else {
            return;
        };
        cx.simulate_resize(gpui::size(px(900.0), px(700.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        // State 1: empty unfiltered workspace hides the scope trigger and keeps
        // a continuous drag bar across the titlebar center.
        assert_eq!(cx.debug_bounds("titlebar-tab-scope"), None);
        assert_eq!(cx.debug_bounds("titlebar-show-all-tabs"), None);
        let left_drag = cx.debug_bounds("workspace-titlebar-drag-left").unwrap();
        let right_drag = cx.debug_bounds("workspace-titlebar-drag-right").unwrap();
        assert!((left_drag.right() - right_drag.left()).abs() <= px(1.0));

        // State 2: unfiltered workspace with open tabs centers #titlebar-tab-scope
        // without rendering an inline clear button.
        workspace.update(cx, |workspace, cx| {
            workspace.tab_host.update(cx, |host, _| {
                for id in [1, 2] {
                    host.insert_and_select_main_tab(Tab {
                        id: TabId(id),
                        kind: TabKind::SessionManager,
                        title: format!("Tab {id}"),
                        title_source: TabTitleSource::Static,
                        root_pane: None,
                        active_pane_id: None,
                    });
                }
            });
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let scope_bounds = cx
            .debug_bounds("titlebar-tab-scope")
            .expect("unfiltered tabs must show #titlebar-tab-scope");
        assert_eq!(cx.debug_bounds("titlebar-show-all-tabs"), None);
        assert!((scope_bounds.center().x - px(450.0)).abs() <= px(2.0));
        assert!((scope_bounds.size.height - px(22.0)).abs() <= px(0.5));
        assert!(scope_bounds.size.width <= px(280.0));

        // State 3a: filtered scope with zero visible tabs must stay visible with
        // both the primary trigger and inline clear segment.
        workspace.update(cx, |workspace, cx| {
            workspace.tab_host.update(cx, |host, _| {
                host.switch_scope(TabScope::Affinity(TabAffinity::LocalTerminal));
            });
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let capsule = cx
            .debug_bounds("titlebar-tab-scope-capsule")
            .expect("filtered empty scope must keep capsule visible");
        let primary = cx
            .debug_bounds("titlebar-tab-scope")
            .expect("filtered empty scope must keep primary segment visible");
        let clear = cx
            .debug_bounds("titlebar-show-all-tabs")
            .expect("filtered empty scope must expose inline clear segment");
        assert!((capsule.center().x - px(450.0)).abs() <= px(2.0));
        assert!((capsule.size.height - px(22.0)).abs() <= px(0.5));
        assert!(capsule.size.width <= px(320.0));
        assert!((clear.size.width - px(22.0)).abs() <= px(0.5));
        assert!(primary.right() <= clear.left());

        // State 3b: long filtered scope label clamps capsule to 320px without
        // squeezing the 22px inline clear button.
        let long_node = NodeId::new("long-scope-node");
        workspace.update(cx, |workspace, cx| {
            let mut config = SshConfig::default();
            config.host = "prod.example.com".into();
            config.port = 22;
            config.username = "ops".into();
            workspace.ssh_nodes.insert(
                long_node.clone(),
                WorkspaceSshNode::new(
                    None,
                    &config,
                    "very-long-production-cluster-hostname-that-must-truncate-in-titlebar".into(),
                    Vec::new(),
                    NodeReadiness::Ready,
                ),
            );
            workspace.tab_host.update(cx, |host, _| {
                host.switch_scope(TabScope::Affinity(TabAffinity::Ssh(long_node.clone())));
            });
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let long_capsule = cx.debug_bounds("titlebar-tab-scope-capsule").unwrap();
        let long_clear = cx.debug_bounds("titlebar-show-all-tabs").unwrap();
        assert!(long_capsule.size.width <= px(320.0));
        assert!((long_clear.size.width - px(22.0)).abs() <= px(0.5));

        // Clicking inline clear resets the scope to All and returns to State 2.
        cx.simulate_mouse_down(long_clear.center(), MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));

        workspace.read_with(cx, |workspace, cx| {
            assert_eq!(*workspace.tab_host.read(cx).scope(), TabScope::All);
        });
        assert!(cx.debug_bounds("titlebar-tab-scope").is_some());
        assert_eq!(cx.debug_bounds("titlebar-show-all-tabs"), None);
    }
}
