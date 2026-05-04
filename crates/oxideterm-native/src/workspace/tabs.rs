use super::*;

impl WorkspaceApp {
    pub(super) fn create_local_terminal_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let tab_id = self.alloc_tab_id();
        let pane_id = self.alloc_pane_id();
        let session_id = self.alloc_session_id();
        let preferences = self.terminal_preferences();
        let pane = cx.new(|cx| {
            TerminalPane::new_with_preferences(preferences, window, cx)
                .expect("failed to initialize terminal pane")
        });

        self.panes.insert(pane_id, pane.clone());
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::LocalTerminal,
            title: self.i18n.t("terminal.local_terminal"),
            root_pane: Some(PaneNode::leaf(pane_id, session_id)),
            active_pane_id: Some(pane_id),
        });
        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        pane.read(cx).focus(window);
        self.reveal_active_tab(window);
        cx.notify();
        Ok(())
    }

    pub(super) fn create_ssh_terminal_tab(
        &mut self,
        config: SshConfig,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let tab_id = self.alloc_tab_id();
        let pane_id = self.alloc_pane_id();
        let session_id = self.alloc_session_id();
        let node_id = NodeId::new(format!("ssh-{}", self.next_ssh_node_id));
        self.next_ssh_node_id += 1;

        self.node_router
            .upsert_node(node_id.clone(), config.clone());
        let consumer = ConnectionConsumer::Terminal(session_id.0.to_string());
        let prompt_handler =
            std::sync::Arc::new(NativeSshPromptHandler::new(self.ssh_worker_tx.clone()));
        let session_config = SshSessionConfig::from(config)
            .with_registry(self.ssh_registry.clone(), consumer)
            .with_prompt_handler(prompt_handler);
        let preferences = self.terminal_preferences();
        let pane = cx.new(|cx| {
            TerminalPane::new_ssh_with_preferences(session_config, preferences, window, cx)
                .expect("failed to initialize ssh terminal pane")
        });

        self.panes.insert(pane_id, pane.clone());
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::SshTerminal,
            title,
            root_pane: Some(PaneNode::leaf(pane_id, session_id)),
            active_pane_id: Some(pane_id),
        });
        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        pane.read(cx).focus(window);
        self.reveal_active_tab(window);
        cx.notify();
        Ok(())
    }

    pub(super) fn open_settings_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_id = if let Some(tab) = self.tabs.iter().find(|tab| tab.kind == TabKind::Settings) {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::Settings,
                title: self.i18n.t("settings_view.title"),
                root_pane: None,
                active_pane_id: None,
            });
            tab_id
        };
        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Settings;
        self.active_sidebar_section = SidebarSection::Settings;
        self.needs_active_pane_focus = false;
        if self.sidebar_collapsed {
            self.sidebar_collapsed = false;
        }
        self.persist_sidebar_settings();
        self.reveal_active_tab(window);
        cx.notify();
    }

    pub(super) fn alloc_tab_id(&mut self) -> TabId {
        let id = TabId(self.next_tab_id);
        self.next_tab_id += 1;
        id
    }

    pub(super) fn alloc_pane_id(&mut self) -> PaneId {
        let id = PaneId(self.next_pane_id);
        self.next_pane_id += 1;
        id
    }

    pub(super) fn alloc_session_id(&mut self) -> TerminalSessionId {
        let id = TerminalSessionId(self.next_session_id);
        self.next_session_id += 1;
        id
    }

    pub(super) fn active_tab_index(&self) -> Option<usize> {
        let active = self.active_tab_id?;
        self.tabs.iter().position(|tab| tab.id == active)
    }

    pub(super) fn active_tab(&self) -> Option<&Tab> {
        self.active_tab_index()
            .and_then(|index| self.tabs.get(index))
    }

    pub(super) fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        let index = self.active_tab_index()?;
        self.tabs.get_mut(index)
    }

    pub(super) fn active_pane_id(&self) -> Option<PaneId> {
        self.active_tab().and_then(|tab| tab.active_pane_id)
    }

    pub(super) fn active_pane(&self) -> Option<gpui::Entity<TerminalPane>> {
        self.active_pane_id()
            .and_then(|pane_id| self.panes.get(&pane_id).cloned())
    }

    pub(super) fn set_active_tab(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.iter().any(|tab| tab.id == tab_id) {
            self.active_tab_id = Some(tab_id);
            self.sync_active_tab_surface();
            self.needs_active_pane_focus = self.active_surface == ActiveSurface::Terminal;
            self.focus_active_pane(window, cx);
            self.reveal_active_tab(window);
            cx.notify();
        }
    }

    pub(super) fn sync_active_tab_surface(&mut self) {
        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::Settings)
        {
            self.active_surface = ActiveSurface::Settings;
            self.active_sidebar_section = SidebarSection::Settings;
        } else {
            self.active_surface = ActiveSurface::Terminal;
        }
    }

    pub(super) fn focus_active_pane(&self, window: &mut Window, cx: &App) {
        if let Some(pane) = self.active_pane() {
            pane.read(cx).focus(window);
        } else {
            window.focus(&self.focus_handle);
        }
    }

    pub(super) fn close_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.active_tab_index() else {
            return;
        };
        let tab = self.tabs.remove(index);
        let mut pane_ids = Vec::new();
        if let Some(root_pane) = &tab.root_pane {
            root_pane.collect_pane_ids(&mut pane_ids);
        }
        for pane_id in pane_ids {
            if let Some(pane) = self.panes.remove(&pane_id) {
                let _ = pane.update(cx, |pane, _cx| pane.shutdown());
            }
        }

        self.active_tab_id = if self.tabs.is_empty() {
            None
        } else {
            Some(self.tabs[index.min(self.tabs.len() - 1)].id)
        };
        self.sync_active_tab_surface();
        self.needs_active_pane_focus =
            self.active_tab_id.is_some() && self.active_surface == ActiveSurface::Terminal;
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window);
        cx.notify();
    }

    pub(super) fn next_tab(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let current = self.active_tab_index().unwrap_or(0);
        let next = if forward {
            (current + 1) % self.tabs.len()
        } else if current == 0 {
            self.tabs.len() - 1
        } else {
            current - 1
        };
        self.active_tab_id = Some(self.tabs[next].id);
        self.sync_active_tab_surface();
        self.needs_active_pane_focus = self.active_surface == ActiveSurface::Terminal;
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window);
        cx.notify();
    }

    pub(super) fn go_to_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get(index) {
            self.active_tab_id = Some(tab.id);
            self.sync_active_tab_surface();
            self.needs_active_pane_focus = self.active_surface == ActiveSurface::Terminal;
            self.focus_active_pane(window, cx);
            self.reveal_active_tab(window);
            cx.notify();
        }
    }

    fn tabbar_viewport_width(&self, window: &Window) -> f32 {
        let window_width = f32::from(window.inner_window_bounds().get_bounds().size.width);
        let sidebar_width = if self.sidebar_collapsed {
            self.tokens.metrics.activity_bar_width
        } else {
            self.sidebar_width
        };
        (window_width - sidebar_width).max(0.0)
    }

    fn tabbar_content_width(&self) -> f32 {
        self.tokens.metrics.tabbar_leading_offset
            + self.tabs.len() as f32 * self.tokens.metrics.tab_width
    }

    fn tabbar_max_scroll(&self, window: &Window) -> f32 {
        (self.tabbar_content_width() - self.tabbar_viewport_width(window)).max(0.0)
    }

    fn clamp_tab_scroll(&mut self, window: &Window) {
        self.tab_scroll_x = self.tab_scroll_x.clamp(0.0, self.tabbar_max_scroll(window));
    }

    fn reveal_active_tab(&mut self, window: &Window) {
        let Some(index) = self.active_tab_index() else {
            self.clamp_tab_scroll(window);
            return;
        };
        let tab_left = self.tokens.metrics.tabbar_leading_offset
            + index as f32 * self.tokens.metrics.tab_width;
        let tab_right = tab_left + self.tokens.metrics.tab_width;
        let viewport_width = self.tabbar_viewport_width(window);

        if tab_left < self.tab_scroll_x {
            self.tab_scroll_x = tab_left;
        } else if tab_right > self.tab_scroll_x + viewport_width {
            self.tab_scroll_x = tab_right - viewport_width;
        }
        self.clamp_tab_scroll(window);
    }

    pub(super) fn handle_tabbar_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event
            .delta
            .pixel_delta(px(self.tokens.metrics.tabbar_height));
        let horizontal = if f32::from(delta.x).abs() > f32::from(delta.y).abs() {
            f32::from(delta.x)
        } else {
            f32::from(delta.y)
        };
        if horizontal == 0.0 {
            return;
        }

        self.tab_scroll_x += horizontal;
        self.clamp_tab_scroll(window);
        cx.stop_propagation();
        cx.notify();
    }

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
            let title = tab.title.clone();
            let icon = match tab.kind {
                TabKind::LocalTerminal => LucideIcon::Square,
                TabKind::SshTerminal => LucideIcon::Terminal,
                TabKind::Settings => LucideIcon::Settings,
            };
            let tab_text = if matches!(tab.kind, TabKind::Settings) {
                self.i18n.t("settings_view.title")
            } else {
                title
            };
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
                    .w(px(self.tokens.metrics.tab_width))
                    .min_w(px(self.tokens.metrics.tab_min_width))
                    .max_w(px(self.tokens.metrics.tab_max_width))
                    .px_3()
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .border_r_1()
                    .border_color(rgb(theme.border))
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
                    .when(active, |tab| {
                        tab.child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .right_0()
                                .h(px(self.tokens.metrics.tab_active_accent_height))
                                .bg(rgb(theme.accent)),
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
                    .child(
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
                    ),
            );
        }

        bar = bar.child(tabs_row);
        bar.into_any_element()
    }

    pub(super) fn render_empty_workspace(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div().flex_1().bg(rgb(theme.bg)).into_any_element()
    }
}
