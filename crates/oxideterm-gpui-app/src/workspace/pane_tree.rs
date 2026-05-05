use super::*;

#[derive(Clone)]
pub(super) struct SplitDrag {
    group_id: PaneId,
    handle_index: usize,
    direction: SplitDirection,
    start_position: gpui::Point<Pixels>,
    start_sizes: Vec<f32>,
}

impl WorkspaceApp {
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
            self.panes.insert(pane_id, pane.clone());
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

        if let Some(pane) = self.panes.remove(&active_pane_id) {
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

    pub(super) fn start_split_drag(
        &mut self,
        group_id: PaneId,
        handle_index: usize,
        direction: SplitDirection,
        sizes: &[f32],
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        self.split_drag = Some(SplitDrag {
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
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
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
        if let Some(tab) = self.active_tab_mut()
            && tab
                .root_pane
                .as_mut()
                .is_some_and(|root_pane| root_pane.update_group_sizes(drag.group_id, &next_sizes))
        {
            cx.notify();
        }
    }

    pub(super) fn finish_split_drag(&mut self, cx: &mut Context<Self>) {
        if self.split_drag.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn render_pane_tree(&self, node: &PaneNode, cx: &mut Context<Self>) -> AnyElement {
        match node {
            PaneNode::Leaf { pane_id, .. } => {
                let theme = self.tokens.ui;
                let active = Some(*pane_id) == self.active_pane_id();
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
                    .border_1()
                    .border_color(if active {
                        rgb(theme.border)
                    } else {
                        rgb(theme.bg_panel)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let pane_id = *pane_id;
                            move |this, _event, window, cx| {
                                if let Some(tab) = this.active_tab_mut() {
                                    tab.active_pane_id = Some(pane_id);
                                }
                                this.needs_active_pane_focus = true;
                                this.focus_active_pane(window, cx);
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
                                    .child(self.render_pane_tree(child, cx)),
                            ),
                    );
                    if index + 1 < children.len() {
                        let group_id = *id;
                        let direction = *direction;
                        let start_sizes = sizes.clone();
                        let mut handle = div()
                            .flex_none()
                            .bg(rgb(self.tokens.ui.divider))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event, _window, cx| {
                                    this.start_split_drag(
                                        group_id,
                                        index,
                                        direction,
                                        &start_sizes,
                                        event,
                                        cx,
                                    );
                                }),
                            );
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
