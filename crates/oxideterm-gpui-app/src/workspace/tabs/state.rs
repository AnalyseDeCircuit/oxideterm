impl WorkspaceApp {
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

    fn pane_id_for_session(&self, session_id: TerminalSessionId) -> Option<PaneId> {
        self.tabs.iter().find_map(|tab| {
            tab.root_pane
                .as_ref()
                .and_then(|root| root.pane_id_for_session(session_id))
        })
    }

    pub(super) fn active_terminal_session_id(&self) -> Option<TerminalSessionId> {
        let tab = self.active_tab()?;
        let pane_id = tab.active_pane_id?;
        tab.root_pane
            .as_ref()
            .and_then(|root| root.session_id_for_pane(pane_id))
    }

}
