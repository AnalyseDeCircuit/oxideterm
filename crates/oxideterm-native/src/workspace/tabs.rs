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
        let pane =
            cx.new(|cx| TerminalPane::new(window, cx).expect("failed to initialize terminal pane"));

        self.panes.insert(pane_id, pane.clone());
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::LocalTerminal,
            title: self.i18n.t("terminal.local_terminal"),
            root_pane: PaneNode::leaf(pane_id, session_id),
            active_pane_id: pane_id,
        });
        self.active_tab_id = Some(tab_id);
        self.needs_active_pane_focus = true;
        pane.read(cx).focus(window);
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
        let session_config =
            SshSessionConfig::from(config).with_registry(self.ssh_registry.clone(), consumer);
        let pane = cx.new(|cx| {
            TerminalPane::new_ssh(session_config, window, cx)
                .expect("failed to initialize ssh terminal pane")
        });

        self.panes.insert(pane_id, pane.clone());
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::SshTerminal,
            title,
            root_pane: PaneNode::leaf(pane_id, session_id),
            active_pane_id: pane_id,
        });
        self.active_tab_id = Some(tab_id);
        self.needs_active_pane_focus = true;
        pane.read(cx).focus(window);
        cx.notify();
        Ok(())
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
        self.active_tab().map(|tab| tab.active_pane_id)
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
            self.needs_active_pane_focus = true;
            self.focus_active_pane(window, cx);
            cx.notify();
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
        tab.root_pane.collect_pane_ids(&mut pane_ids);
        for pane_id in pane_ids {
            if let Some(pane) = self.panes.remove(&pane_id) {
                let _ = pane.update(cx, |pane, _cx| pane.shutdown());
            }
        }

        self.active_tab_id = if self.tabs.is_empty() {
            None
        } else {
            Some(self.tabs[index.saturating_sub(1).min(self.tabs.len() - 1)].id)
        };
        self.needs_active_pane_focus = self.active_tab_id.is_some();
        self.focus_active_pane(window, cx);
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
        self.needs_active_pane_focus = true;
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(super) fn go_to_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get(index) {
            self.active_tab_id = Some(tab.id);
            self.needs_active_pane_focus = true;
            self.focus_active_pane(window, cx);
            cx.notify();
        }
    }

    pub(super) fn render_tab_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let mut bar = div()
            .h(px(self.tokens.metrics.tabbar_height))
            .flex()
            .flex_row()
            .items_center()
            .pl(px(self.tokens.metrics.tabbar_leading_offset))
            .pr_1()
            .border_b_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_hover));

        for tab in &self.tabs {
            let tab_id = tab.id;
            let active = Some(tab_id) == self.active_tab_id;
            let title = tab.title.clone();
            let tab_label = match tab.kind {
                TabKind::LocalTerminal => format!(">_ {title}"),
                TabKind::SshTerminal => format!("⇄ {title}"),
            };
            bar = bar.child(
                div()
                    .id(("workspace-tab", tab_id.0))
                    .h_full()
                    .w(px(self.tokens.metrics.tab_width))
                    .px_2()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .border_r_1()
                    .border_color(rgb(theme.border))
                    .bg(if active {
                        rgb(theme.bg_active)
                    } else {
                        rgb(theme.bg_panel)
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
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(self.tokens.metrics.tab_font_size))
                            .child(tab_label),
                    )
                    .child(
                        div()
                            .px_1()
                            .cursor_pointer()
                            .text_size(px(self.tokens.metrics.tab_font_size))
                            .text_color(rgb(theme.text_muted))
                            .child("x")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, window, cx| {
                                    this.set_active_tab(tab_id, window, cx);
                                    this.close_active_tab(window, cx);
                                }),
                            ),
                    ),
            );
        }

        bar.child(
            div()
                .id("workspace-new-tab")
                .h(px(self.tokens.metrics.new_tab_button_height))
                .w(px(self.tokens.metrics.new_tab_button_width))
                .ml_1()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(self.tokens.radii.sm))
                .bg(rgb(theme.bg_hover))
                .text_color(rgb(theme.text_muted))
                .text_size(px(self.tokens.metrics.empty_sidebar_title_font_size))
                .cursor_pointer()
                .child("+")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        let _ = this.create_local_terminal_tab(window, cx);
                    }),
                ),
        )
        .into_any_element()
    }

    pub(super) fn render_empty_workspace(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(theme.bg))
            .child(
                div()
                    .px(px(self.tokens.metrics.empty_workspace_padding_x))
                    .py(px(self.tokens.metrics.empty_workspace_padding_y))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgb(theme.bg_hover))
                    .text_color(rgb(theme.text))
                    .cursor_pointer()
                    .child(self.i18n.t("workspace.new_local_terminal"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            let _ = this.create_local_terminal_tab(window, cx);
                        }),
                    ),
            )
            .into_any_element()
    }
}
