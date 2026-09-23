use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SplitDropEdge {
    Left,
    Right,
    Top,
    Bottom,
}

impl SplitDropEdge {
    fn placement(self) -> (SplitDirection, bool) {
        match self {
            Self::Left => (SplitDirection::Horizontal, true),
            Self::Right => (SplitDirection::Horizontal, false),
            Self::Top => (SplitDirection::Vertical, true),
            Self::Bottom => (SplitDirection::Vertical, false),
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::workspace) struct SplitDropRegion {
    tab: TabId,
    pane: Option<PaneId>,
    bounds: Bounds<Pixels>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::workspace) struct SplitDropTarget {
    tab: TabId,
    pane: Option<PaneId>,
    edge: SplitDropEdge,
    bounds: Bounds<Pixels>,
}

fn drop_edge(bounds: Bounds<Pixels>, point: Point<Pixels>) -> Option<SplitDropEdge> {
    if !bounds.contains(&point) || bounds.size.width <= px(0.0) || bounds.size.height <= px(0.0) {
        return None;
    }
    let x = f32::from(point.x - bounds.origin.x) / f32::from(bounds.size.width);
    let y = f32::from(point.y - bounds.origin.y) / f32::from(bounds.size.height);
    [
        (x, SplitDropEdge::Left),
        (1.0 - x, SplitDropEdge::Right),
        (y, SplitDropEdge::Top),
        (1.0 - y, SplitDropEdge::Bottom),
    ]
    .into_iter()
    .filter(|(distance, _)| *distance <= 0.25)
    .min_by(|a, b| a.0.total_cmp(&b.0))
    .map(|(_, edge)| edge)
}

fn preview_bounds(mut bounds: Bounds<Pixels>, edge: SplitDropEdge) -> Bounds<Pixels> {
    match edge {
        SplitDropEdge::Left | SplitDropEdge::Right => {
            bounds.size.width /= 2.0;
            if edge == SplitDropEdge::Right {
                bounds.origin.x += bounds.size.width;
            }
        }
        SplitDropEdge::Top | SplitDropEdge::Bottom => {
            bounds.size.height /= 2.0;
            if edge == SplitDropEdge::Bottom {
                bounds.origin.y += bounds.size.height;
            }
        }
    }
    bounds
}

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
        div()
            .size_full()
            .child(content)
            .on_children_prepainted(move |bounds, window, cx| {
                if let Some(bounds) = bounds.first() {
                    // Native windows report pointer positions locally; drop regions use screen coordinates.
                    regions.borrow_mut().push(SplitDropRegion {
                        tab,
                        pane,
                        bounds: Bounds::new(bounds.origin + window.bounds().origin, bounds.size),
                    });
                    if pane.is_none() {
                        // The content wrapper runs after every leaf; partial geometry must not clear a valid preview.
                        let _ = owner.update(cx, |workspace, cx| {
                            workspace.refresh_split_drop_after_layout(window, cx)
                        });
                    }
                }
            })
            .into_any_element()
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
        let host = self.tab_host.read(cx);
        self.split_drop_regions.borrow().iter().find_map(|region| {
            if !host.can_receive_tab_drop(source, region.tab) {
                return None;
            }
            let tab = host.tab_by_id(region.tab)?;
            if region.pane.is_none() && tab.root_pane.is_some() {
                return None;
            }
            if region.pane.is_some_and(|pane| {
                tab.root_pane
                    .as_ref()
                    .is_none_or(|root| !root.contains_pane(pane))
            }) {
                return None;
            }
            Some(SplitDropTarget {
                tab: region.tab,
                pane: region.pane,
                edge: drop_edge(region.bounds, point)?,
                bounds: region.bounds,
            })
        })
    }

    fn refresh_split_drop_after_layout(&mut self, window: &Window, cx: &mut Context<Self>) {
        let source = self
            .main_window_tabs
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
        if over_content && let Some(drag) = self.main_window_tabs.drag.as_mut() {
            changed |= drag.mode != TabDragMode::Content;
            drag.mode = TabDragMode::Content;
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
            let mut x = f32::from(point.x - bounds.origin.x)
                + f32::from(-self.main_window_tabs.scroll_handle.offset().x)
                - self.tokens.metrics.tabbar_leading_offset;
            let host = self.tab_host.read(cx);
            let hovered = self
                .tabs(cx)
                .iter()
                .filter(|tab| !host.is_outside_main_window(tab.id))
                .find_map(|tab| {
                    let width = self.tab_visual_width(tab);
                    let hit = x >= 0.0 && x < width;
                    x -= width;
                    hit.then_some(tab.id)
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
        if self.tab_host.read(cx).is_detached(source) {
            self.return_detached_tab_to_main(source, window, cx);
        }
        let (direction, before) = target.edge.placement();
        self.combine_tabs_at(
            source,
            target.tab,
            target.pane,
            direction,
            before,
            window,
            cx,
        )
    }

    pub(in crate::workspace) fn cancel_tab_merge_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let main_drag = self.main_window_tabs.drag.take();
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

    pub(in crate::workspace) fn render_tab_drag_capture(
        &self,
        detached: Option<TabId>,
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
                            if let Some(tab) = detached {
                                if !workspace
                                    .detached_tab_return_drag
                                    .is_some_and(|drag| drag.tab_id == tab)
                                {
                                    return false;
                                }
                                workspace.update_detached_tab_return_drag(tab, event, window, cx);
                            } else {
                                if workspace.main_window_tabs.drag.is_none() {
                                    return false;
                                }
                                workspace.update_tab_drag(event, window, cx);
                            }
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
                            if let Some(tab) = detached {
                                if !workspace
                                    .detached_tab_return_drag
                                    .is_some_and(|drag| drag.tab_id == tab)
                                {
                                    return false;
                                }
                                workspace.finish_detached_tab_return_drag(tab, event, window, cx);
                            } else {
                                if workspace.main_window_tabs.drag.is_none() {
                                    return false;
                                }
                                workspace.finish_tab_drag(event, window, cx);
                            }
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
        let target = self.split_drop_target?;
        let mut bounds = preview_bounds(target.bounds, target.edge);
        bounds.origin -= window.bounds().origin;
        Some(
            div()
                .absolute()
                .left(bounds.origin.x)
                .top(bounds.origin.y)
                .w(bounds.size.width)
                .h(bounds.size.height)
                .bg(rgba((self.tokens.ui.accent << 8) | 0x30))
                .border_2()
                .border_color(rgb(self.tokens.ui.accent))
                .rounded(px(self.tokens.radii.md))
                .into_any_element(),
        )
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
