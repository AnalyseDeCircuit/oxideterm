use super::*;
use oxideterm_gpui_shell::panes::{
    PaneHost, ShellPaneRequest, measure_drop_region, split_drop_target,
};
#[cfg(test)]
use oxideterm_gpui_shell::panes::{SplitDropEdge, drop_edge, preview_bounds};
pub(in crate::workspace) use oxideterm_gpui_shell::panes::{SplitDropRegion, SplitDropTarget};

impl WorkspaceApp {
    pub(in crate::workspace) fn wrap_split_drop_region(
        &self,
        tab: TabId,
        pane: Option<PaneId>,
        content: AnyElement,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let main = self
            .window_registry
            .handle_for_role(window_registry::WindowRole::Main);
        if main.is_none_or(|main| main.window_id() != window.window_handle().window_id()) {
            return content;
        }
        let regions = self.split_drop_regions.clone();
        let owner = cx.entity().downgrade();
        measure_drop_region(tab, pane, content, regions, move |window, cx| {
            if pane.is_none() {
                let _ = owner.update(cx, |host, cx| {
                    host.refresh_split_drop_after_layout(window, cx)
                });
            }
        })
    }
    fn tab_drop_target(
        &self,
        source: TabId,
        point: Point<Pixels>,
        cx: &App,
    ) -> Option<SplitDropTarget> {
        self.window_registry
            .handle_for_role(window_registry::WindowRole::Main)?;
        // A source window must not drop through its own content onto an obscured main window.
        if self.detached_tab_return_drag.is_some_and(|drag| {
            drag.tab_id == source && !drag.native_window_move && drag.source_bounds.contains(&point)
        }) {
            return None;
        }
        split_drop_target(
            self.tab_host.read(cx),
            &self.split_drop_regions.borrow(),
            source,
            point,
        )
    }

    fn refresh_split_drop_after_layout(&mut self, window: &Window, cx: &mut Context<Self>) {
        let source = self
            .main_window_tabs
            .read(cx)
            .state
            .drag
            .as_ref()
            .filter(|drag| drag.active)
            .map(|drag| {
                (
                    drag.tab_id,
                    gpui::point(px(drag.current_x), px(drag.current_y)) + window.bounds().origin,
                )
            })
            .or_else(|| {
                self.detached_tab_return_drag
                    .filter(|drag| drag.active)
                    .map(|drag| {
                        (
                            drag.tab_id,
                            gpui::point(px(drag.current_screen_x), px(drag.current_screen_y)),
                        )
                    })
            });
        let Some((source, point)) = source else {
            return;
        };
        let next = self.tab_drop_target(source, point, cx);
        let over_content = self.split_drop_regions.borrow().iter().any(|region| {
            region.tab != source
                && self.active_tab_id(cx) == Some(region.tab)
                && region.bounds.contains(&point)
        });
        let mut changed = self.split_drop_target != next;
        self.split_drop_target = next;
        if over_content {
            self.main_window_tabs.update(cx, |strip, cx| {
                if let Some(drag) = strip.state.drag.as_mut() {
                    changed |= drag.mode != TabDragMode::Content;
                    drag.mode = TabDragMode::Content;
                    if changed {
                        cx.notify()
                    }
                }
            });
        }
        if changed {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn update_tab_split_destination(
        &mut self,
        drag: &mut TabDragState,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let point = event.position + window.bounds().origin;
        let over_strip = self
            .main_window_tabbar_drop_bounds
            .is_some_and(|bounds| bounds.contains(&point));
        if over_strip {
            let bounds = self.main_window_tabbar_drop_bounds.unwrap();
            let x = f32::from(point.x - bounds.origin.x)
                + f32::from(
                    -self
                        .main_window_tabs
                        .read(cx)
                        .state
                        .scroll_handle
                        .offset()
                        .x,
                );
            let layout = self.tab_strip_layout(window, cx);
            let hovered = self.main_tabs(cx).find_map(|(_, tab)| {
                layout
                    .bounds(tab.id)
                    .filter(|bounds| bounds.contains(&x))
                    .map(|_| tab.id)
            });
            if let Some(tab) = hovered.filter(|tab| *tab != drag.tab_id) {
                drag.destination_tab = Some(tab);
                if self.active_tab_id(cx) != Some(tab) {
                    self.set_active_tab(tab, window, cx);
                }
            }
        } else if self.active_tab_id(cx) == Some(drag.tab_id)
            && let Some(tab) = drag
                .destination_tab
                .filter(|id| self.tab_by_id(*id, cx).is_some())
        {
            self.set_active_tab(tab, window, cx);
        }
        self.split_drop_target = self.tab_drop_target(drag.tab_id, point, cx);
        // Inside another tab's content, an invalid edge is cancellation, never implicit detach.
        !over_strip
            && self.split_drop_regions.borrow().iter().any(|region| {
                region.tab != drag.tab_id
                    && self.active_tab_id(cx) == Some(region.tab)
                    && region.bounds.contains(&point)
            })
    }

    pub(in crate::workspace) fn update_detached_split_destination(
        &mut self,
        source: TabId,
        point: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let next = self.tab_drop_target(source, point, cx);
        if self.split_drop_target != next {
            self.split_drop_target = next;
            cx.notify();
        }
    }

    pub(in crate::workspace) fn finish_tab_split_drop(
        &mut self,
        source: TabId,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // Revalidate at release: a destination can close or reach its pane limit while dragging.
        let target = self.tab_drop_target(source, point, cx);
        self.split_drop_target = None;
        let Some(target) = target else {
            return false;
        };
        self.pane_request(ShellPaneRequest::Drop { source, target }, window, cx)
    }

    pub(in crate::workspace) fn cancel_tab_merge_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let main_drag = self.main_window_tabs.update(cx, |strip, cx| {
            let drag = strip.state.drag.take();
            if drag.is_some() {
                cx.notify()
            }
            drag
        });
        let active = main_drag.is_some() | self.detached_tab_return_drag.take().is_some();
        self.split_drop_target = None;
        if let Some(drag) = main_drag
            && self.tab_by_id(drag.tab_id, cx).is_some()
            && !self.tab_host.read(cx).is_outside_main_window(drag.tab_id)
        {
            self.set_main_window_active_tab(Some(drag.tab_id), cx);
            self.sync_active_tab_surface(cx);
        }
        if active {
            cx.notify();
        }
        active
    }

    pub(in crate::workspace) fn render_detached_tab_drag_capture(
        &self,
        tab: TabId,
        cx: &Context<Self>,
    ) -> AnyElement {
        // Per-frame capture listeners keep receiving a release outside the source element.
        let owner = cx.entity().downgrade();
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                window.set_window_cursor_style(CursorStyle::ClosedHand);
                let move_owner = owner.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase != gpui::DispatchPhase::Capture {
                        return;
                    }
                    let handled = move_owner
                        .update(cx, |workspace, cx| {
                            if !workspace
                                .detached_tab_return_drag
                                .is_some_and(|drag| drag.tab_id == tab)
                            {
                                return false;
                            }
                            workspace.update_detached_tab_return_drag(tab, event, window, cx);
                            true
                        })
                        .unwrap_or(false);
                    if handled {
                        cx.stop_propagation();
                    }
                });
                let up_owner = owner.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if phase != gpui::DispatchPhase::Capture || event.button != MouseButton::Left {
                        return;
                    }
                    let handled = up_owner
                        .update(cx, |workspace, cx| {
                            if !workspace
                                .detached_tab_return_drag
                                .is_some_and(|drag| drag.tab_id == tab)
                            {
                                return false;
                            }
                            workspace.finish_detached_tab_return_drag(tab, event, window, cx);
                            true
                        })
                        .unwrap_or(false);
                    if handled {
                        cx.stop_propagation();
                    }
                });
            },
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    pub(in crate::workspace) fn render_split_drop_preview(
        &self,
        window: &Window,
    ) -> Option<AnyElement> {
        Some(self.split_drop_target?.preview(window, &self.tokens))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_edges_use_screen_bounds_and_leave_the_center_inactive() {
        let bounds = Bounds::new(
            gpui::point(px(200.0), px(100.0)),
            gpui::size(px(800.0), px(400.0)),
        );
        for (x, y, expected) in [
            (210.0, 300.0, Some(SplitDropEdge::Left)),
            (990.0, 300.0, Some(SplitDropEdge::Right)),
            (600.0, 110.0, Some(SplitDropEdge::Top)),
            (600.0, 490.0, Some(SplitDropEdge::Bottom)),
            (600.0, 300.0, None),
            (199.0, 300.0, None),
        ] {
            assert_eq!(drop_edge(bounds, gpui::point(px(x), px(y))), expected);
        }
        assert_eq!(
            preview_bounds(bounds, SplitDropEdge::Right),
            Bounds::new(
                gpui::point(px(600.0), px(100.0)),
                gpui::size(px(400.0), px(400.0))
            )
        );
        assert_eq!(
            preview_bounds(bounds, SplitDropEdge::Top),
            Bounds::new(
                gpui::point(px(200.0), px(100.0)),
                gpui::size(px(800.0), px(200.0))
            )
        );
    }
}
