use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn alloc_tab_id(&mut self, cx: &mut App) -> TabId {
        self.tab_host
            .update(cx, |tab_host, _| tab_host.alloc_tab_id())
    }

    pub(in crate::workspace) fn alloc_pane_id(&mut self, cx: &mut App) -> PaneId {
        self.tab_host
            .update(cx, |tab_host, _| tab_host.alloc_pane_id())
    }

    pub(in crate::workspace) fn alloc_session_id(&mut self, cx: &mut App) -> TerminalSessionId {
        self.tab_host
            .update(cx, |tab_host, _| tab_host.alloc_session_id())
    }

    /// Keeps root window focus state and Entity-owned navigation history in one write path.
    pub(in crate::workspace) fn set_main_window_active_tab(
        &mut self,
        active_tab_id: Option<TabId>,
        cx: &mut App,
    ) {
        let previous_active_tab_id = self.main_window_tabs.active_tab_id;
        let active_tab_changed = previous_active_tab_id != active_tab_id;
        self.main_window_tabs.active_tab_id = active_tab_id;
        self.tab_host.update(cx, |tab_host, _| {
            tab_host.observe_active_tab(active_tab_id);
        });
        if active_tab_changed {
            if let Some(tab_id) = previous_active_tab_id {
                self.sync_ide_surface_mount(tab_id, cx);
            }
            if let Some(tab_id) = active_tab_id {
                self.sync_ide_surface_mount(tab_id, cx);
            }
            // Host Tools owns its timer; root only pushes mount visibility changes.
            self.sync_host_tools_lifecycle(false, cx);
            self.sync_active_terminal_metadata_context(cx);
            self.sync_active_terminal_recording_elapsed_tick(cx);
            self.sync_active_privilege_prompt_inline_hint(cx);
        }
    }

    pub(in crate::workspace) fn active_tab_index(&self) -> Option<usize> {
        let active = self.main_window_tabs.active_tab_id?;
        if let Some((cached_id, cached_index)) = self.main_window_tabs.active_tab_index_cache.get()
            && cached_id == active
            && self
                .tabs
                .get(cached_index)
                .is_some_and(|tab| tab.id == active)
        {
            return Some(cached_index);
        }
        let index = self.tabs.iter().position(|tab| tab.id == active)?;
        self.main_window_tabs
            .active_tab_index_cache
            .set(Some((active, index)));
        Some(index)
    }

    pub(in crate::workspace) fn tab_index_by_id(&self, tab_id: TabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == tab_id)
    }

    pub(in crate::workspace) fn tab_by_id(&self, tab_id: TabId) -> Option<&Tab> {
        self.tab_index_by_id(tab_id)
            .and_then(|index| self.tabs.get(index))
    }

    pub(in crate::workspace) fn tab_mut_by_id(&mut self, tab_id: TabId) -> Option<&mut Tab> {
        let index = self.tab_index_by_id(tab_id)?;
        self.tabs.get_mut(index)
    }

    pub(in crate::workspace) fn active_tab(&self) -> Option<&Tab> {
        self.active_tab_index()
            .and_then(|index| self.tabs.get(index))
    }

    pub(in crate::workspace) fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        let index = self.active_tab_index()?;
        self.tabs.get_mut(index)
    }

    pub(in crate::workspace) fn active_pane_id(&self) -> Option<PaneId> {
        self.active_tab().and_then(|tab| tab.active_pane_id)
    }

    pub(in crate::workspace) fn active_pane(&self, cx: &App) -> Option<gpui::Entity<TerminalPane>> {
        self.active_pane_id()
            .and_then(|pane_id| self.tab_host.read(cx).panes().get(&pane_id).cloned())
    }

    pub(in crate::workspace) fn active_terminal_session_id(&self) -> Option<TerminalSessionId> {
        let tab = self.active_tab()?;
        let pane_id = tab.active_pane_id?;
        tab.root_pane
            .as_ref()
            .and_then(|root| root.session_id_for_pane(pane_id))
    }
}
