use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn insert_tab(&mut self, tab: Tab, cx: &mut App) {
        let tab_id = tab.id;
        // Creation and selection are one Entity-owned transition. Root only
        // applies the cross-subsystem visibility consequences.
        let previous_active_tab_id = self
            .tab_host
            .update(cx, |tab_host, _| tab_host.insert_and_select_main_tab(tab));
        self.apply_main_window_active_tab_change(previous_active_tab_id, Some(tab_id), cx);
    }

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
        let previous_active_tab_id = self
            .tab_host
            .update(cx, |tab_host, _| tab_host.select_main_tab(active_tab_id));
        self.apply_main_window_active_tab_change(previous_active_tab_id, active_tab_id, cx);
    }

    pub(in crate::workspace) fn apply_main_window_active_tab_change(
        &mut self,
        previous_active_tab_id: Option<TabId>,
        active_tab_id: Option<TabId>,
        cx: &mut App,
    ) {
        if previous_active_tab_id != active_tab_id {
            if let Some(tab_id) = previous_active_tab_id {
                self.sync_ide_surface_mount(tab_id, cx);
                self.sync_remote_desktop_frame_visibility(tab_id, cx);
            }
            if let Some(tab_id) = active_tab_id {
                self.sync_ide_surface_mount(tab_id, cx);
                self.sync_remote_desktop_frame_visibility(tab_id, cx);
            }
            // Host Tools owns its timer; root only pushes mount visibility changes.
            self.sync_host_tools_lifecycle(false, cx);
            // Forwarding owns its sampler; root only pushes aggregate mount visibility.
            self.sync_forwarding_sampling_visibility(cx);
            // Graphics owns frame presentation; tab navigation only supplies mount visibility.
            self.sync_graphics_surface_visibility(cx);
            self.sync_active_terminal_metadata_context(cx);
            self.sync_active_terminal_recording_elapsed_tick(cx);
            self.sync_active_privilege_prompt_inline_hint(cx);
        }
    }

    pub(in crate::workspace) fn tabs<'a>(&self, cx: &'a App) -> &'a [Tab] {
        self.tab_host.read(cx).tabs()
    }

    pub(in crate::workspace) fn active_tab_id(&self, cx: &App) -> Option<TabId> {
        self.tab_host.read(cx).active_tab_id()
    }

    pub(in crate::workspace) fn active_tab_index(&self, cx: &App) -> Option<usize> {
        self.tab_host.read(cx).active_tab_index()
    }

    pub(in crate::workspace) fn tab_index_by_id(&self, tab_id: TabId, cx: &App) -> Option<usize> {
        self.tab_host.read(cx).tab_index_by_id(tab_id)
    }

    pub(in crate::workspace) fn tab_by_id<'a>(
        &self,
        tab_id: TabId,
        cx: &'a App,
    ) -> Option<&'a Tab> {
        self.tab_host.read(cx).tab_by_id(tab_id)
    }

    pub(in crate::workspace) fn active_tab<'a>(&self, cx: &'a App) -> Option<&'a Tab> {
        self.tab_host.read(cx).active_tab()
    }

    pub(in crate::workspace) fn active_pane_id(&self, cx: &App) -> Option<PaneId> {
        self.active_tab(cx).and_then(|tab| tab.active_pane_id)
    }

    pub(in crate::workspace) fn active_pane(&self, cx: &App) -> Option<gpui::Entity<TerminalPane>> {
        self.active_pane_id(cx)
            .and_then(|pane_id| self.tab_host.read(cx).panes().get(&pane_id).cloned())
    }

    pub(in crate::workspace) fn active_terminal_session_id(
        &self,
        cx: &App,
    ) -> Option<TerminalSessionId> {
        let tab = self.active_tab(cx)?;
        let pane_id = tab.active_pane_id?;
        tab.root_pane
            .as_ref()
            .and_then(|root| root.session_id_for_pane(pane_id))
    }
}
