use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn insert_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        let tab_id = tab.id;
        if tab.kind == TabKind::Sftp {
            self.ensure_sftp_page(tab_id, cx);
        }
        self.register_tab_surface(&tab, cx);
        let previous_active_tab_id = self
            .tab_host
            .update(cx, |tab_host, _| tab_host.insert_and_select_main_tab(tab));
        self.apply_main_window_active_tab_change(previous_active_tab_id, Some(tab_id), cx);
    }

    pub(in crate::workspace) fn register_tab_surface(&mut self, tab: &Tab, cx: &mut App) {
        let tab_id = tab.id;
        let surface_kind =
            crate::workspace::root::helpers::tab_background_key(&tab.kind).to_string();
        let surface_label = if tab.title.trim().is_empty() {
            surface_kind.clone()
        } else {
            tab.title.clone()
        };
        let stable_surface_ref = oxideterm_ai::StableResourceRef::new(
            oxideterm_ai::StableResourceKind::AppSurface,
            surface_kind,
            Some(surface_label.clone()),
        )
        .ok();
        // Tab insertion is the mount boundary for exact focus authority. The
        // stable reference is optional because not every internal tab is openable.
        self.ai_runtime_context.update(cx, |runtime, _cx| {
            runtime.register_app_surface(tab_id, surface_label, stable_surface_ref);
        });
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
        if let Some(id) = active_tab_id.filter(|id| self.sftp_pages.contains_key(id)) {
            self.sftp_focused_surface = sftp::SftpSurfaceId::Tab(id);
        }
        self.apply_main_window_active_tab_change(
            previous_active_tab_id,
            self.active_tab_id(cx),
            cx,
        );
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

    pub(in crate::workspace) fn active_content_tab_id(&self, cx: &App) -> Option<TabId> {
        let host = self.tab_host.read(cx);
        Some(host.focused_page_id(host.active_tab_id()?))
    }

    pub(in crate::workspace) fn active_content_tab<'a>(&self, cx: &'a App) -> Option<&'a Tab> {
        self.tab_host
            .read(cx)
            .tab_by_id(self.active_content_tab_id(cx)?)
    }

    pub(in crate::workspace) fn active_pane_id(&self, cx: &App) -> Option<PaneId> {
        self.active_tab(cx).and_then(|tab| tab.active_pane_id)
    }

    pub(in crate::workspace) fn active_pane(&self, cx: &App) -> Option<gpui::Entity<TerminalPane>> {
        self.active_pane_id(cx)
            .and_then(|pane_id| self.tab_host.read(cx).panes().get(&pane_id).cloned())
    }

    pub(in crate::workspace) fn terminal_kind_for_pane(
        &self,
        pane_id: PaneId,
        cx: &App,
    ) -> Option<oxideterm_terminal::TerminalSessionKind> {
        self.tab_host
            .read(cx)
            .panes()
            .get(&pane_id)
            .map(|pane| pane.read(cx).session_kind())
    }

    pub(in crate::workspace) fn active_terminal_kind(
        &self,
        cx: &App,
    ) -> Option<oxideterm_terminal::TerminalSessionKind> {
        self.terminal_kind_for_pane(self.active_pane_id(cx)?, cx)
    }

    pub(in crate::workspace) fn terminal_pane_label(&self, pane_id: PaneId, cx: &App) -> String {
        if let Some(node) = self
            .session_id_for_pane(pane_id, cx)
            .and_then(|id| self.workspace_runtime.read(cx).ssh_terminal_node_id(id))
            .and_then(|id| self.ssh_nodes.get(&id))
        {
            return format!(
                "{} · {}@{}:{}",
                node.title, node.endpoint.username, node.endpoint.host, node.endpoint.port
            );
        }
        self.tab_host
            .read(cx)
            .panes()
            .get(&pane_id)
            .map(|pane| pane.read(cx).title().to_string())
            .unwrap_or_default()
    }

    pub(in crate::workspace) fn terminal_tab_kind_for_pane(
        &self,
        pane_id: PaneId,
        cx: &App,
    ) -> Option<TabKind> {
        use oxideterm_terminal::TerminalSessionKind;
        self.terminal_kind_for_pane(pane_id, cx)
            .map(|kind| match kind {
                TerminalSessionKind::SshPty => TabKind::SshTerminal,
                TerminalSessionKind::Mosh => TabKind::MoshTerminal,
                _ => TabKind::LocalTerminal,
            })
    }

    pub(in crate::workspace) fn terminal_tab_has_kind(
        &self,
        tab_id: TabId,
        kind: oxideterm_terminal::TerminalSessionKind,
        cx: &App,
    ) -> bool {
        let Some(root) = self
            .tab_by_id(tab_id, cx)
            .and_then(|tab| tab.root_pane.as_ref())
        else {
            return false;
        };
        let mut panes = Vec::new();
        root.collect_pane_ids(&mut panes);
        panes
            .into_iter()
            .any(|pane| self.terminal_kind_for_pane(pane, cx) == Some(kind))
    }

    pub(in crate::workspace) fn terminal_pane_for_window(
        &self,
        window: &Window,
        cx: &App,
    ) -> Option<PaneId> {
        let host = self.tab_host.read(cx);
        let tab_id = host
            .tabs()
            .iter()
            .find(|tab| {
                host.container_tab_id(tab.id) == tab.id
                    && host.detached_window_handle(tab.id).is_some_and(|handle| {
                        handle.window_id() == window.window_handle().window_id()
                    })
            })
            .map(|tab| tab.id)
            .or_else(|| {
                self.window_registry
                    .handle_for_role(window_registry::WindowRole::Main)
                    .filter(|handle| handle.window_id() == window.window_handle().window_id())
                    .and_then(|_| self.active_tab_id(cx))
            })?;
        host.tab_by_id(tab_id)?.active_pane_id
    }

    pub(in crate::workspace) fn focus_tab_terminal(
        &self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let host = self.tab_host.read(cx);
        let Some(pane) = host
            .tab_by_id(tab_id)
            .and_then(|tab| tab.active_pane_id)
            .and_then(|id| host.panes().get(&id))
            .cloned()
        else {
            return;
        };
        let Some(handle) = host.detached_window_handle(tab_id).or_else(|| {
            self.window_registry
                .handle_for_role(window_registry::WindowRole::Main)
        }) else {
            return;
        };
        if handle.window_id() == window.window_handle().window_id() {
            pane.update(cx, |pane, cx| pane.focus(window, cx));
        } else {
            let _ = handle.update(cx, |_, window, cx| {
                pane.update(cx, |pane, cx| pane.focus(window, cx));
            });
        }
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
