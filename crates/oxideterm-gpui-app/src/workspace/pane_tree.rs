use super::*;

const SPLIT_HANDLE_LINE_ALPHA: u32 = 0x80;
const SPLIT_HANDLE_HOVER_BG_ALPHA: u32 = 0x12;
const SPLIT_HANDLE_ACTIVE_BG_ALPHA: u32 = 0x1f;
const SPLIT_HANDLE_ACTIVE_LINE_ALPHA: u32 = 0xcc;
const SPLIT_HANDLE_LINE_WIDTH: f32 = 1.0;
const ACTIVE_PANE_BORDER_ALPHA: u32 = 0x66;
const ACTIVE_PANE_SHADOW_ALPHA: u32 = 0x24;
const ACTIVE_PANE_SHADOW_BLUR: f32 = 10.0;

#[derive(Clone)]
pub(super) struct SplitDrag {
    tab_id: Option<TabId>,
    group_id: PaneId,
    handle_index: usize,
    direction: SplitDirection,
    start_position: gpui::Point<Pixels>,
    start_sizes: Vec<f32>,
}

impl WorkspaceApp {
    pub(super) fn register_terminal_pane(
        &mut self,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        pane: gpui::Entity<TerminalPane>,
        cx: &mut Context<Self>,
    ) {
        let subscription = cx.subscribe(
            &pane,
            move |this, _pane, event: &TerminalPaneEvent, cx| match event {
                TerminalPaneEvent::Exited { .. } => {
                    this.queue_auto_close_terminal_session(session_id, cx);
                }
            },
        );
        self.terminal_pane_subscriptions
            .insert(pane_id, subscription);
        self.panes.insert(pane_id, pane);
    }

    pub(super) fn remove_terminal_pane(
        &mut self,
        pane_id: &PaneId,
    ) -> Option<gpui::Entity<TerminalPane>> {
        self.terminal_pane_subscriptions.remove(pane_id);
        self.panes.remove(pane_id)
    }

    pub(super) fn queue_auto_close_terminal_session(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) {
        // Serial sessions report port failures through the same terminal event;
        // keep local transport panes visible so users can inspect the error
        // text and reconnect without recreating the whole tab.
        if self.serial_terminal_configs.contains_key(&session_id)
            || self.raw_tcp_terminal_configs.contains_key(&session_id)
            || self.raw_udp_terminal_configs.contains_key(&session_id)
        {
            return;
        }
        if self.pending_auto_close_terminal_sessions.insert(session_id) {
            cx.notify();
        }
    }

    pub(super) fn schedule_pending_auto_close_terminal_sessions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_auto_close_terminal_sessions.is_empty()
            || self.auto_close_terminal_sessions_scheduled
        {
            return;
        }
        self.auto_close_terminal_sessions_scheduled = true;
        let workspace = cx.entity();
        window.on_next_frame(move |window, cx| {
            let _ = workspace.update(cx, |this, cx| {
                this.auto_close_terminal_sessions_scheduled = false;
                this.drain_pending_auto_close_terminal_sessions(window, cx);
            });
        });
    }

    fn drain_pending_auto_close_terminal_sessions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session_ids: Vec<_> = self.pending_auto_close_terminal_sessions.drain().collect();
        for session_id in session_ids {
            if self.serial_terminal_configs.contains_key(&session_id)
                || self.raw_tcp_terminal_configs.contains_key(&session_id)
                || self.raw_udp_terminal_configs.contains_key(&session_id)
            {
                continue;
            }
            self.close_terminal_session(session_id, window, cx);
        }
    }

    pub(super) fn active_tab_has_serial_terminal(&self) -> bool {
        let Some(tab) = self.active_tab() else {
            return false;
        };
        let Some(root_pane) = tab.root_pane.as_ref() else {
            return false;
        };

        let mut session_ids = Vec::new();
        root_pane.collect_session_ids(&mut session_ids);
        session_ids
            .iter()
            .any(|session_id| self.serial_terminal_configs.contains_key(session_id))
    }

    pub(super) fn split_active_pane(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_index) = self.active_tab_index() else {
            return;
        };
        let Some(active_pane_id) = self.tabs[active_index].active_pane_id else {
            return;
        };
        if self.tabs[active_index]
            .root_pane
            .as_ref()
            .is_none_or(|root_pane| root_pane.pane_count() >= MAX_PANES_PER_TAB)
        {
            return;
        }

        if self.tabs[active_index].kind == TabKind::SshTerminal {
            return;
        }
        if self.active_tab_has_serial_terminal() {
            return;
        }

        let group_id = self.alloc_pane_id();
        let pane_id = self.alloc_pane_id();
        let session_id = self.alloc_session_id();
        let tab_kind = self.tabs[active_index].kind.clone();
        let preferences = self.terminal_preferences_for_tab_kind(&tab_kind);
        let local_config =
            (tab_kind == TabKind::LocalTerminal).then(|| self.local_terminal_config());
        let pane = cx.new(|cx| {
            if let Some(config) = local_config {
                TerminalPane::new_local_with_config_and_preferences(config, preferences, window, cx)
                    .expect("failed to initialize split terminal pane")
            } else {
                TerminalPane::new_with_preferences(preferences, window, cx)
                    .expect("failed to initialize split terminal pane")
            }
        });

        let tab = &mut self.tabs[active_index];
        if tab.root_pane.as_mut().is_some_and(|root_pane| {
            root_pane.split_active(active_pane_id, group_id, direction, pane_id, session_id)
        }) {
            tab.active_pane_id = Some(pane_id);
            self.register_terminal_pane(pane_id, session_id, pane.clone(), cx);
            self.needs_active_pane_focus = true;
            pane.read(cx).focus(window);
            cx.notify();
        } else {
            let _ = pane.update(cx, |pane, _cx| pane.shutdown());
        }
    }

    pub(super) fn close_active_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active_index) = self.active_tab_index() else {
            return;
        };
        let Some(active_pane_id) = self.tabs[active_index].active_pane_id else {
            return;
        };
        if self.tabs[active_index]
            .root_pane
            .as_ref()
            .is_none_or(|root_pane| root_pane.pane_count() <= 1)
        {
            return;
        }

        if let Some(session_id) = self.tabs[active_index]
            .root_pane
            .as_ref()
            .and_then(|root_pane| root_pane.session_id_for_pane(active_pane_id))
        {
            self.serial_terminal_configs.remove(&session_id);
            self.raw_tcp_terminal_configs.remove(&session_id);
            self.raw_udp_terminal_configs.remove(&session_id);
            self.unregister_ssh_terminal_session(session_id);
        }

        if let Some(pane) = self.remove_terminal_pane(&active_pane_id) {
            let _ = pane.update(cx, |pane, _cx| pane.shutdown());
        }

        let tab = &mut self.tabs[active_index];
        let Some(root_pane) = tab.root_pane.as_mut() else {
            return;
        };
        if let Some(next_active) = root_pane.close_pane(active_pane_id) {
            if let Some(replacement) = root_pane.single_child_replacement() {
                tab.root_pane = Some(replacement);
            }
            tab.active_pane_id = Some(next_active);
            self.needs_active_pane_focus = true;
            self.focus_active_pane(window, cx);
            cx.notify();
        }
    }

    pub(super) fn reset_active_tab_to_single_pane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_index) = self.active_tab_index() else {
            return;
        };
        let Some(active_pane_id) = self.tabs[active_index].active_pane_id else {
            return;
        };
        let Some(root_pane) = self.tabs[active_index].root_pane.as_ref().cloned() else {
            return;
        };
        if root_pane.pane_count() <= 1 {
            return;
        }
        let Some(active_session_id) = root_pane.session_id_for_pane(active_pane_id) else {
            return;
        };

        let mut pane_ids = Vec::new();
        root_pane.collect_pane_ids(&mut pane_ids);
        let mut session_ids = Vec::new();
        root_pane.collect_session_ids(&mut session_ids);

        for session_id in session_ids
            .into_iter()
            .filter(|session_id| *session_id != active_session_id)
        {
            self.serial_terminal_configs.remove(&session_id);
            self.raw_tcp_terminal_configs.remove(&session_id);
            self.raw_udp_terminal_configs.remove(&session_id);
            self.unregister_ssh_terminal_session(session_id);
        }
        for pane_id in pane_ids
            .into_iter()
            .filter(|pane_id| *pane_id != active_pane_id)
        {
            if let Some(pane) = self.remove_terminal_pane(&pane_id) {
                let _ = pane.update(cx, |pane, _cx| pane.shutdown());
            }
        }

        let tab = &mut self.tabs[active_index];
        tab.root_pane = Some(PaneNode::leaf(active_pane_id, active_session_id));
        tab.active_pane_id = Some(active_pane_id);
        self.needs_active_pane_focus = true;
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(super) fn start_split_drag(
        &mut self,
        tab_id: Option<TabId>,
        group_id: PaneId,
        handle_index: usize,
        direction: SplitDirection,
        sizes: &[f32],
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        self.split_drag = Some(SplitDrag {
            tab_id,
            group_id,
            handle_index,
            direction,
            start_position: event.position,
            start_sizes: sizes.to_vec(),
        });
        cx.notify();
    }

    pub(super) fn update_split_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.split_drag.clone() else {
            return;
        };
        // Splitters use root-level pointer capture. While dragging outside the
        // splitter element, the stored drag state owns motion until mouse-up.
        let viewport = window.viewport_size();
        let delta_fraction = match drag.direction {
            SplitDirection::Horizontal => {
                f32::from(event.position.x - drag.start_position.x)
                    / f32::from(viewport.width).max(1.0)
                    * 100.0
            }
            SplitDirection::Vertical => {
                f32::from(event.position.y - drag.start_position.y)
                    / f32::from(viewport.height).max(1.0)
                    * 100.0
            }
        };
        let next_sizes = adjusted_split_sizes(&drag.start_sizes, drag.handle_index, delta_fraction);
        let updated = if let Some(tab_id) = drag.tab_id {
            self.tab_mut_by_id(tab_id).is_some_and(|tab| {
                tab.root_pane.as_mut().is_some_and(|root_pane| {
                    root_pane.update_group_sizes(drag.group_id, &next_sizes)
                })
            })
        } else {
            self.active_tab_mut().is_some_and(|tab| {
                tab.root_pane.as_mut().is_some_and(|root_pane| {
                    root_pane.update_group_sizes(drag.group_id, &next_sizes)
                })
            })
        };
        if updated {
            cx.notify();
        }
    }

    pub(super) fn finish_split_drag(&mut self, cx: &mut Context<Self>) {
        if self.split_drag.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn reset_split_group_sizes(
        &mut self,
        tab_id: Option<TabId>,
        group_id: PaneId,
        cx: &mut Context<Self>,
    ) {
        let updated = if let Some(tab_id) = tab_id {
            self.tab_mut_by_id(tab_id).is_some_and(|tab| {
                tab.root_pane
                    .as_mut()
                    .is_some_and(|root_pane| root_pane.reset_group_sizes(group_id))
            })
        } else {
            self.active_tab_mut().is_some_and(|tab| {
                tab.root_pane
                    .as_mut()
                    .is_some_and(|root_pane| root_pane.reset_group_sizes(group_id))
            })
        };
        if updated {
            cx.notify();
        }
    }

    pub(super) fn render_pane_tree(&self, node: &PaneNode, cx: &mut Context<Self>) -> AnyElement {
        self.render_pane_tree_for_tab(self.main_window_tabs.active_tab_id, node, cx)
    }

    pub(super) fn render_pane_tree_for_tab(
        &self,
        tab_id: Option<TabId>,
        node: &PaneNode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_pane_id = tab_id
            .and_then(|tab_id| self.tab_by_id(tab_id))
            .and_then(|tab| tab.active_pane_id);
        let has_split_panes = if let Some(tab_id) = tab_id {
            self.tab_by_id(tab_id)
                .and_then(|tab| tab.root_pane.as_ref())
                .is_some_and(|root_pane| root_pane.pane_count() > 1)
        } else {
            self.active_tab()
                .and_then(|tab| tab.root_pane.as_ref())
                .is_some_and(|root_pane| root_pane.pane_count() > 1)
        };
        match node {
            PaneNode::Leaf { pane_id, .. } => {
                let active = Some(*pane_id) == active_pane_id;
                let Some(pane) = self.panes.get(pane_id).cloned() else {
                    return div().size_full().into_any_element();
                };
                div()
                    .id(("workspace-pane", pane_id.0))
                    .size_full()
                    .relative()
                    .min_w(px(self.tokens.metrics.min_pane_width))
                    .min_h(px(self.tokens.metrics.min_pane_height))
                    .overflow_hidden()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let pane_id = *pane_id;
                            let tab_id = tab_id;
                            move |this, _event, window, cx| {
                                if let Some(tab_id) = tab_id
                                    && let Some(tab) = this.tab_mut_by_id(tab_id)
                                {
                                    tab.active_pane_id = Some(pane_id);
                                    if !this.detached_tabs.contains(&tab_id) {
                                        this.main_window_tabs.active_tab_id = Some(tab_id);
                                    }
                                } else if let Some(tab) = this.active_tab_mut() {
                                    tab.active_pane_id = Some(pane_id);
                                }
                                if let Some(pane) = this.panes.get(&pane_id).cloned() {
                                    pane.read(cx).focus(window);
                                }
                                cx.notify();
                            }
                        }),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .child(pane),
                    )
                    .when(active && self.ai_inline_panel.open, |pane_frame| {
                        pane_frame.child(self.render_terminal_ai_inline_panel(cx))
                    })
                    .when(active && has_split_panes, |pane_frame| {
                        let accent = self.tokens.ui.accent;
                        let active_shadow = vec![gpui::BoxShadow {
                            color: gpui::Hsla::from(rgba((accent << 8) | ACTIVE_PANE_SHADOW_ALPHA)),
                            offset: gpui::point(px(0.0), px(0.0)),
                            blur_radius: px(ACTIVE_PANE_SHADOW_BLUR),
                            spread_radius: px(0.0),
                        }];
                        // This overlay is painted above the terminal content
                        // without changing pane layout or terminal grid size.
                        pane_frame.child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .right_0()
                                .bottom_0()
                                .border_1()
                                .border_color(rgba((accent << 8) | ACTIVE_PANE_BORDER_ALPHA))
                                .shadow(active_shadow),
                        )
                    })
                    .into_any_element()
            }
            PaneNode::Group {
                id,
                direction,
                children,
                sizes,
            } => {
                let sizes = balanced_sizes(sizes, children.len());
                let mut group = div()
                    .id(("workspace-pane-group", id.0))
                    .size_full()
                    .flex()
                    .overflow_hidden();
                group = match direction {
                    SplitDirection::Horizontal => group.flex_row(),
                    SplitDirection::Vertical => group.flex_col(),
                };

                for (index, child) in children.iter().enumerate() {
                    let basis = relative(sizes.get(index).copied().unwrap_or(0.0) / 100.0);
                    group = group.child(
                        div()
                            .flex_none()
                            .flex_basis(basis)
                            .relative()
                            .min_w(px(self.tokens.metrics.min_pane_width))
                            .min_h(px(self.tokens.metrics.min_pane_height))
                            .overflow_hidden()
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .right_0()
                                    .bottom_0()
                                    .child(self.render_pane_tree_for_tab(tab_id, child, cx)),
                            ),
                    );
                    if index + 1 < children.len() {
                        let group_id = *id;
                        let direction = *direction;
                        let start_sizes = sizes.clone();
                        let active_drag = self.split_drag.as_ref().is_some_and(|drag| {
                            drag.tab_id == tab_id
                                && drag.group_id == group_id
                                && drag.handle_index == index
                                && drag.direction == direction
                        });
                        let handle_bg = if active_drag {
                            rgba((self.tokens.ui.accent << 8) | SPLIT_HANDLE_ACTIVE_BG_ALPHA)
                        } else {
                            rgba(0x00000000)
                        };
                        let line_color = if active_drag {
                            rgba((self.tokens.ui.accent << 8) | SPLIT_HANDLE_ACTIVE_LINE_ALPHA)
                        } else {
                            rgba((self.tokens.ui.divider << 8) | SPLIT_HANDLE_LINE_ALPHA)
                        };
                        // Keep the drag target wide while drawing only a
                        // hairline in the center, matching common terminal and
                        // editor splitters without making the seam look heavy.
                        let line = div()
                            .absolute()
                            .bg(line_color)
                            .when(direction == SplitDirection::Horizontal, |line| {
                                line.top_0()
                                    .bottom_0()
                                    .left(px((self.tokens.metrics.split_handle_size
                                        - SPLIT_HANDLE_LINE_WIDTH)
                                        / 2.0))
                                    .w(px(SPLIT_HANDLE_LINE_WIDTH))
                            })
                            .when(direction == SplitDirection::Vertical, |line| {
                                line.left_0()
                                    .right_0()
                                    .top(px((self.tokens.metrics.split_handle_size
                                        - SPLIT_HANDLE_LINE_WIDTH)
                                        / 2.0))
                                    .h(px(SPLIT_HANDLE_LINE_WIDTH))
                            });
                        let mut handle = div()
                            .flex_none()
                            .relative()
                            .bg(handle_bg)
                            .hover({
                                let accent = self.tokens.ui.accent;
                                move |style| {
                                    style.bg(rgba((accent << 8) | SPLIT_HANDLE_HOVER_BG_ALPHA))
                                }
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                                    if event.click_count >= 2 {
                                        this.reset_split_group_sizes(tab_id, group_id, cx);
                                        return;
                                    }
                                    this.start_split_drag(
                                        tab_id,
                                        group_id,
                                        index,
                                        direction,
                                        &start_sizes,
                                        event,
                                        cx,
                                    );
                                }),
                            )
                            .child(line);
                        handle = match direction {
                            SplitDirection::Horizontal => handle
                                .w(px(self.tokens.metrics.split_handle_size))
                                .h_full()
                                .cursor(CursorStyle::ResizeColumn),
                            SplitDirection::Vertical => handle
                                .h(px(self.tokens.metrics.split_handle_size))
                                .w_full()
                                .cursor(CursorStyle::ResizeRow),
                        };
                        group = group.child(handle);
                    }
                }

                group.into_any_element()
            }
        }
    }
}
