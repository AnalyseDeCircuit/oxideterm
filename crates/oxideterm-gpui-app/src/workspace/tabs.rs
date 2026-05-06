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
        let preferences = self.terminal_preferences_for_tab_kind(&TabKind::LocalTerminal);
        let terminal_config = self.local_terminal_config();
        let pane = cx.new(|cx| {
            TerminalPane::new_local_with_config_and_preferences(
                terminal_config,
                preferences,
                window,
                cx,
            )
            .expect("failed to initialize terminal pane")
        });

        self.panes.insert(pane_id, pane.clone());
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::LocalTerminal,
            title: self.local_terminal_tab_title(),
            title_source: TabTitleSource::Static,
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
        let preferences = self.terminal_preferences_for_tab_kind(&TabKind::SshTerminal);
        let pane = cx.new(|cx| {
            TerminalPane::new_ssh_with_preferences(session_config, preferences, window, cx)
                .expect("failed to initialize ssh terminal pane")
        });

        self.panes.insert(pane_id, pane.clone());
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::SshTerminal,
            title,
            title_source: TabTitleSource::Static,
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
                title_source: TabTitleSource::I18nKey("settings_view.title"),
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
            self.needs_active_pane_focus = self.active_tab().is_some_and(|tab| {
                matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal)
            });
            self.focus_active_pane(window, cx);
            self.reveal_active_tab(window);
            cx.notify();
        }
    }

    pub(super) fn sync_active_tab_surface(&mut self) {
        match self.active_tab().map(|tab| &tab.kind) {
            Some(TabKind::Settings) => {
                self.active_surface = ActiveSurface::Settings;
                self.active_sidebar_section = SidebarSection::Settings;
            }
            Some(TabKind::SessionManager) => {
                self.active_surface = ActiveSurface::Terminal;
                self.active_sidebar_section = SidebarSection::Connections;
            }
            _ => {
                self.active_surface = ActiveSurface::Terminal;
            }
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
        self.needs_active_pane_focus = self
            .active_tab()
            .is_some_and(|tab| matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal));
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
        self.needs_active_pane_focus = self
            .active_tab()
            .is_some_and(|tab| matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal));
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window);
        cx.notify();
    }

    pub(super) fn go_to_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get(index) {
            self.active_tab_id = Some(tab.id);
            self.sync_active_tab_surface();
            self.needs_active_pane_focus = self.active_tab().is_some_and(|tab| {
                matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal)
            });
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
            + self
                .tabs
                .iter()
                .map(|tab| self.tab_visual_width(tab))
                .sum::<f32>()
    }

    fn tabbar_max_scroll(&self, window: &Window) -> f32 {
        (self.tabbar_content_width() - self.tabbar_viewport_width(window)).max(0.0)
    }

    fn clamp_tab_scroll(&mut self, window: &Window) {
        self.tab_scroll_x = self.tab_scroll_x.clamp(0.0, self.tabbar_max_scroll(window));
    }

    pub(super) fn reveal_active_tab(&mut self, window: &Window) {
        let Some(index) = self.active_tab_index() else {
            self.clamp_tab_scroll(window);
            return;
        };
        let tab_left = self.tokens.metrics.tabbar_leading_offset
            + self
                .tabs
                .iter()
                .take(index)
                .map(|tab| self.tab_visual_width(tab))
                .sum::<f32>();
        let tab_right = tab_left + self.tab_visual_width(&self.tabs[index]);
        let viewport_width = self.tabbar_viewport_width(window);

        if tab_left < self.tab_scroll_x {
            self.tab_scroll_x = tab_left;
        } else if tab_right > self.tab_scroll_x + viewport_width {
            self.tab_scroll_x = tab_right - viewport_width;
        }
        self.clamp_tab_scroll(window);
    }

    pub(super) fn tab_display_title(&self, tab: &Tab) -> String {
        let title = match tab.title_source {
            TabTitleSource::Static => tab.title.clone(),
            TabTitleSource::I18nKey(key) => self.i18n.t(key),
        };
        if matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal) {
            let pane_count = tab.root_pane.as_ref().map_or(1, PaneNode::pane_count);
            if pane_count > 1 {
                return format!("{title} ({pane_count})");
            }
        }
        title
    }

    fn tab_visual_width(&self, tab: &Tab) -> f32 {
        let metrics = self.tokens.metrics;
        let title_width = self.tab_display_title(tab).chars().count() as f32
            * metrics.tab_font_size
            * metrics.tab_title_width_ratio;
        let fixed_width = metrics.tab_padding_x * 2.0
            + metrics.tab_icon_size
            + metrics.tab_gap * 2.0
            + metrics.tab_close_button_size;

        (title_width + fixed_width).clamp(metrics.tab_min_width, metrics.tab_max_width)
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
            let tab_width = self.tab_visual_width(tab);
            let icon = match tab.kind {
                TabKind::LocalTerminal => LucideIcon::Square,
                TabKind::SshTerminal => LucideIcon::Terminal,
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
                            .rounded(px(2.0))
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
                cx,
            ))
            .child(self.render_welcome_action_button(
                LucideIcon::Terminal,
                "layout.empty.new_local_terminal",
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
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(40.0))
            .px(px(18.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba((theme.border_strong << 8) | 0xcc))
            .bg(rgba((theme.bg_panel << 8) | 0x8c))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(theme.text))
            .cursor_pointer()
            .hover(move |button| button.bg(rgb(theme.bg_hover)))
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
                    .font_family(SharedString::from("JetBrainsMono Nerd Font"))
                    .text_size(px(11.0))
                    .line_height(px(14.0))
                    .text_color(rgb(theme.text))
                    .child(key),
            )
            .child(self.i18n.t(label_key))
            .into_any_element()
    }
}

fn shortcut_key(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("⌘{key}")
    } else {
        format!("Ctrl+{key}")
    }
}
