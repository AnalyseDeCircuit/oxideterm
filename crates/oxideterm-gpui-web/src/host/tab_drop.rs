use super::*;
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
pub(super) struct TabDrop {
    pub regions: Rc<RefCell<Vec<SplitDropRegion>>>,
    pub preview: Option<SplitDropTarget>,
}

impl Workspace {
    pub(super) fn content_drop(
        &mut self,
        event: TabContentDrop<'_>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match event {
            TabContentDrop::Probe(drag, event) => {
                let strip = self.strip.read(cx);
                let over_strip = strip.state.scroll_handle.bounds().contains(&event.position);
                if let Some(tab) = strip
                    .live_tab_at(event.position)
                    .filter(|id| *id != drag.tab_id)
                {
                    drag.destination_tab = Some(tab);
                }
                if let Some(tab) = drag.destination_tab
                    && (!over_strip || strip.live_tab_at(event.position) == Some(tab))
                    && self.layout.active_tab_id() != Some(tab)
                    && self.layout.can_combine_pages(drag.tab_id, tab)
                {
                    self.layout.select_main_tab(Some(tab));
                    self.changed(cx);
                }
                let point = event.position + w.bounds().origin;
                let regions = self.tab_drop.regions.borrow();
                let target = split_drop_target(&self.layout, &regions, drag.tab_id, point);
                if self.tab_drop.preview != target {
                    self.tab_drop.preview = target;
                    cx.notify();
                }
                !over_strip
                    && regions.iter().any(|r| {
                        r.tab != drag.tab_id
                            && Some(r.tab) == self.layout.active_tab_id()
                            && r.bounds.contains(&point)
                    })
            }
            TabContentDrop::Commit(source, point) => {
                self.tab_drop.preview = None;
                let target =
                    split_drop_target(&self.layout, &self.tab_drop.regions.borrow(), source, point);
                target.is_some_and(|target| {
                    self.pane_request(ShellPaneRequest::Drop { source, target }, w, cx)
                })
            }
        }
    }

    pub(super) fn refresh_tab_drop(&mut self, w: &Window, cx: &mut Context<Self>) {
        let drag = self.strip.read(cx).state.drag.as_ref().filter(|d| d.active);
        let next = drag.and_then(|drag| {
            split_drop_target(
                &self.layout,
                &self.tab_drop.regions.borrow(),
                drag.tab_id,
                point(px(drag.current_x), px(drag.current_y)) + w.bounds().origin,
            )
        });
        if next != self.tab_drop.preview {
            self.tab_drop.preview = next;
            cx.notify();
        }
    }

    pub(super) fn combine_tabs(
        &mut self,
        source: TabId,
        target: TabId,
        pane: Option<PaneId>,
        dir: SplitDirection,
        before: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .layout
            .combine_pages_at(source, target, pane, dir, before)
            .is_none()
        {
            return false;
        }
        if let Some(s) = self
            .active_session()
            .and_then(|sid| self.sessions.get(&sid))
        {
            w.focus(&s.view.read(cx).focus_handle(cx), cx);
        } else {
            w.focus(&self.focus, cx);
        }
        self.sync_active_scope(cx);
        self.changed(cx);
        true
    }

    pub(crate) fn pane_shortcut(
        &mut self,
        cycle: Option<bool>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.layout.active_tab() else {
            return;
        };
        let (Some(root), Some(active)) = (&tab.root_pane, tab.active_pane_id) else {
            return;
        };
        let pane = if let Some(forward) = cycle {
            let mut panes = Vec::new();
            root.collect_pane_ids(&mut panes);
            let Some(i) = panes.iter().position(|&p| p == active) else {
                return;
            };
            if panes.len() < 2 {
                return;
            }
            panes[if forward {
                (i + 1) % panes.len()
            } else {
                (i + panes.len() - 1) % panes.len()
            }]
        } else {
            active
        };
        let Some(content) = root
            .session_id_for_pane(pane)
            .map(PaneContent::Terminal)
            .or_else(|| root.page_id_for_pane(pane).map(PaneContent::Page))
        else {
            return;
        };
        let req = if cycle.is_some() {
            if matches!(content, PaneContent::Page(_)) {
                w.focus(&self.focus, cx);
            }
            ShellPaneRequest::Focus(tab.id, pane, content)
        } else {
            ShellPaneRequest::ClosePane(tab.id, pane, content)
        };
        self.pane_request(req, w, cx);
    }

    pub(crate) fn focused_tab(&self) -> Option<&Tab> {
        self.layout
            .tab_by_id(self.layout.focused_page_id(self.layout.active_tab_id()?))
    }

    pub(crate) fn activate_page(&mut self, page: TabId, cx: &App) {
        let container = self.layout.container_tab_id(page);
        self.layout.select_main_tab(Some(container));
        if page != container {
            self.layout.focus_content_page(page);
        }
        self.sync_active_scope(cx);
    }

    pub(super) fn tidy_container(&mut self, tab: TabId) {
        let Some(item) = self
            .layout
            .tab_by_id(tab)
            .filter(|t| t.kind == TabKind::Workspace)
        else {
            return;
        };
        match &item.root_pane {
            None => {
                if let Some(index) = self.layout.tab_index_by_id(tab) {
                    self.layout.remove_tab_at(index);
                }
            }
            Some(PaneNode::Page { tab_id, .. }) => {
                let page = *tab_id;
                let active = self.layout.active_tab_id() == Some(tab);
                self.layout.unembed_page(page);
                if let Some(index) = self.layout.tab_index_by_id(tab) {
                    self.layout.remove_tab_at(index);
                }
                if active {
                    self.layout.select_main_tab(Some(page));
                }
            }
            Some(PaneNode::Leaf { session_id, .. }) => {
                if let Some(s) = self.sessions.get(session_id) {
                    self.layout.restore_single_terminal_workspace_tab(
                        tab,
                        s.kind(),
                        s.title.clone(),
                    );
                }
            }
            _ => {}
        }
    }

    pub(crate) fn remove_terminal_pane(
        &mut self,
        tab: TabId,
        pane: PaneId,
        session: TerminalSessionId,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self
            .layout
            .tab_by_id(tab)
            .and_then(|t| t.root_pane.as_ref())
        else {
            return;
        };
        if root.session_id_for_pane(pane) != Some(session) {
            return;
        }
        if root.pane_count() == 1 {
            self.close(tab, cx);
        } else if self.layout.close_pane(tab, pane).is_some() {
            self.release_session(session, cx);
            self.tidy_container(tab);
            self.reconcile_scope_after_close(cx);
            self.changed(cx);
        }
    }
}
