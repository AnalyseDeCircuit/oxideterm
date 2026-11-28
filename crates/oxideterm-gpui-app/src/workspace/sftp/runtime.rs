use super::*;
use oxideterm_environment::CurrentDirectoryScope;

fn sftp_terminal_directory_pane(
    remote: &SftpRemoteId,
    active: Option<(CurrentDirectoryScope, PaneId)>,
    recent: &HashMap<NodeId, PaneId>,
) -> Option<PaneId> {
    let SftpRemoteId::Node(node) = remote else {
        return None;
    };
    if let Some((CurrentDirectoryScope::SshNode(active_node), pane)) = active
        && active_node == node.0
    {
        return Some(pane);
    }
    recent.get(node).copied()
}

use oxideterm_gpui_sftp::{sftp_open_presentation, sidebar_sftp_target};

impl WorkspaceApp {
    fn dedicated_sftp_connection_slot(
        &self,
        node_id: &NodeId,
        route: Option<&ConnectionConsumerLease>,
    ) -> Arc<DedicatedSftpConnectionSlot> {
        let Some(route) = route else {
            return Arc::default();
        };
        let mut slots = self.dedicated_sftp_connections.lock();
        if let Some(previous) = slots.get(node_id)
            && previous.connection_id == route.connection_id()
            && let Some(slot) = previous.slot.upgrade()
        {
            return slot;
        }
        // A replacement route can use another jump transport while old requests drain.
        let slot = Arc::default();
        slots.insert(
            node_id.clone(),
            DedicatedSftpRouteSlot {
                connection_id: route.connection_id().to_owned(),
                slot: Arc::downgrade(&slot),
            },
        );
        slot
    }

    fn lease_sftp_route(&self, node_id: &NodeId) -> Option<Arc<ConnectionConsumerLease>> {
        let id = self.node_router.connection_id_for_node(node_id)?;
        if self
            .ssh_nodes
            .get(node_id)
            .is_some_and(|node| node.ssh_channel_strategy.requires_dedicated_consumers())
        {
            // The browsing transport owns the SFTP consumer; this lease only keeps its route alive.
            return self.node_router.lease_connection_handoff(&id).map(Arc::new);
        }
        self.ssh_registry
            .lease_consumer(
                &id,
                ConnectionConsumer::Sftp(uuid::Uuid::new_v4().to_string()),
            )
            .map(Arc::new)
    }

    fn bind_sftp_surface_lease(&mut self, surface: SftpSurfaceId, node_id: &NodeId) {
        if self.sftp_surface_leases.get(&surface).is_some_and(|lease| {
            &lease.node_id == node_id
                && self.node_router.connection_id_for_node(node_id).as_deref()
                    == Some(lease.route.connection_id())
        }) {
            return;
        }
        if let Some(route) = self.lease_sftp_route(node_id) {
            let lease = SftpSurfaceLease {
                node_id: node_id.clone(),
                _dedicated_slot: self.dedicated_sftp_connection_slot(node_id, Some(&route)),
                route,
            };
            self.sftp_surface_leases.insert(surface, lease);
        } else {
            self.sftp_surface_leases.remove(&surface);
        }
    }

    pub(in crate::workspace) fn rebind_sftp_surfaces_for_node(
        &mut self,
        node_id: &NodeId,
        cx: &App,
    ) {
        // Background pages continue owning the route while another tab is focused.
        let tabs: Vec<_> = self
            .sftp_tab_nodes
            .iter()
            .filter_map(|(tab, bound)| (bound == node_id).then_some(*tab))
            .collect();
        for tab in tabs {
            self.bind_sftp_surface_lease(SftpSurfaceId::Tab(tab), node_id);
        }
        let sidebar = self.sftp_view.read(cx);
        if sidebar.current_surface_id == Some(SftpSurfaceId::Sidebar)
            && sidebar
                .current_remote_id
                .as_ref()
                .and_then(SftpRemoteId::node_id)
                == Some(node_id)
        {
            self.bind_sftp_surface_lease(SftpSurfaceId::Sidebar, node_id);
        }
    }

    pub(in crate::workspace::sftp) fn sftp_remote_backend(
        &self,
        remote_id: &SftpRemoteId,
    ) -> Option<SftpRemoteBackend> {
        match remote_id {
            SftpRemoteId::Ftp(id) => self
                .ftp_sessions
                .get(id)
                .cloned()
                .map(|runtime| SftpRemoteBackend::Ftp { runtime }),
            SftpRemoteId::Node(node_id) => {
                let route = self.lease_sftp_route(node_id);
                Some(SftpRemoteBackend::Node {
                    router: self.node_router.clone(),
                    node_id: node_id.clone(),
                    channel_strategy: self
                        .ssh_nodes
                        .get(node_id)
                        .map(|node| node.ssh_channel_strategy)
                        .unwrap_or_default(),
                    prompt_handler: self.ssh_consumer_prompt_handler.clone(),
                    managed_key_resolver: self.ssh_consumer_managed_key_resolver.clone(),
                    dedicated_slot: self.dedicated_sftp_connection_slot(node_id, route.as_deref()),
                    _route_lease: route,
                })
            }
            SftpRemoteId::Standalone(profile_id) => self
                .standalone_sftp_sessions
                .get(profile_id)
                .map(|runtime| SftpRemoteBackend::Standalone {
                    handle: runtime.handle.clone(),
                }),
        }
    }

    pub(in crate::workspace::sftp) fn acquire_sftp_transfer_backend(
        &self,
        remote_id: &SftpRemoteId,
        transfer_id: &str,
    ) -> Option<(SftpRemoteBackend, Option<ConnectionConsumerLease>)> {
        match remote_id {
            SftpRemoteId::Ftp(id) => self
                .ftp_sessions
                .get(id)
                .cloned()
                .map(|runtime| (SftpRemoteBackend::Ftp { runtime }, None)),
            SftpRemoteId::Node(_) => Some((self.sftp_remote_backend(remote_id)?, None)),
            SftpRemoteId::Standalone(profile_id) => {
                let runtime = self.standalone_sftp_sessions.get(profile_id)?;
                let consumer = ConnectionConsumer::Sftp(format!(
                    "standalone-transfer:{profile_id}:{transfer_id}:{}",
                    uuid::Uuid::new_v4()
                ));
                let lease = self
                    .ssh_registry
                    .lease_consumer(&runtime.connection_id, consumer)?;
                Some((
                    SftpRemoteBackend::Standalone {
                        handle: lease.handle().clone(),
                    },
                    Some(lease),
                ))
            }
        }
    }

    pub(in crate::workspace::sftp) fn sftp_remote_id_for_tab(
        &self,
        tab_id: TabId,
    ) -> Option<SftpRemoteId> {
        self.sftp_tab_nodes
            .get(&tab_id)
            .cloned()
            .map(SftpRemoteId::Node)
            .or_else(|| {
                self.standalone_sftp_tabs.get(&tab_id).map(|binding| {
                    if self.ftp_sessions.contains_key(&binding.primary_endpoint_id) {
                        return SftpRemoteId::Ftp(binding.primary_endpoint_id.clone());
                    }
                    SftpRemoteId::Standalone(
                        binding
                            .secondary_endpoint_id
                            .as_ref()
                            .unwrap_or(&binding.primary_endpoint_id)
                            .clone(),
                    )
                })
            })
    }

    pub(in crate::workspace::sftp) fn sftp_pair_primary_remote_id(
        &self,
        cx: &App,
    ) -> Option<SftpRemoteId> {
        self.sftp_view().read(cx).pair_primary_remote_id.clone()
    }

    pub(in crate::workspace::sftp) fn request_sftp_remote_load(&mut self, cx: &mut Context<Self>) {
        self.sftp_view().update(cx, |sftp, cx| {
            sftp.request_remote_load();
            cx.notify();
        });
        self.maybe_start_sftp_remote_load(cx);
    }

    pub(in crate::workspace) fn request_sftp_pair_primary_load(&mut self, cx: &mut Context<Self>) {
        let Some((surface_id, remote_id, path, view_generation, delivery)) = ({
            let sftp = self.sftp_view().read(cx);
            match (sftp.current_surface_id, sftp.pair_primary_remote_id.clone()) {
                (Some(surface_id), Some(remote_id)) => Some((
                    surface_id,
                    remote_id,
                    sftp.local_path.clone(),
                    sftp.view_generation,
                    sftp.worker_sender(),
                )),
                _ => None,
            }
        }) else {
            return;
        };
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            return;
        };
        self.sftp_view().update(cx, |sftp, cx| {
            sftp.pair_primary_loading = true;
            cx.notify();
        });
        self.forwarding_runtime.spawn(async move {
            let result = load_remote_sftp_listing(backend, &path).await;
            let _ = delivery.send(SftpWorkerResult::PairPrimaryList {
                surface_id,
                remote_id,
                view_generation,
                path,
                result,
            });
        });
    }

    /// Resolve the terminal pane that a terminal-directory action should target.
    ///
    /// This intentionally does not consult the cwd-awareness setting: reading the
    /// terminal cwd is gated by that setting, writing a `cd` command is not.
    pub(in crate::workspace::sftp) fn sftp_terminal_target_pane(&self, cx: &App) -> Option<PaneId> {
        let remote = self.sftp_view().read(cx).current_remote_id.as_ref()?;
        let SftpRemoteId::Node(node) = remote else {
            return None;
        };
        // A detached browser must not mistake the main window's selected tab for its focus.
        let active = match self.sftp_surface_id() {
            SftpSurfaceId::Sidebar => self.active_terminal_cwd_scope_and_pane(cx),
            SftpSurfaceId::Tab(_) => None,
        };
        let pane_id = sftp_terminal_directory_pane(
            remote,
            active,
            &self.terminal.read(cx).last_ssh_terminal_panes,
        )?;
        let session = self.session_id_for_pane(pane_id, cx)?;
        // Recheck the live registration: a closed or replaced pane cannot select another server.
        if self
            .workspace_runtime
            .read(cx)
            .ssh_terminal_node_id(session)
            .as_ref()
            != Some(node)
        {
            return None;
        }
        Some(pane_id)
    }

    pub(in crate::workspace::sftp) fn sftp_terminal_directory(&self, cx: &App) -> Option<String> {
        if !self.terminal_current_directory_awareness_enabled() {
            return None;
        }
        let pane_id = self.sftp_terminal_target_pane(cx)?;
        let SftpRemoteId::Node(node) = self.sftp_view().read(cx).current_remote_id.clone()? else {
            return None;
        };
        let host = self.tab_host.read(cx);
        let pane = host.panes().get(&pane_id)?.read(cx);
        super::super::terminal_cwd::terminal_cwd_snapshot_from_pane(
            CurrentDirectoryScope::ssh_node(node.0.clone()),
            pane,
        )
        .map(|snapshot| snapshot.path().to_string())
    }

    /// Send the visible SFTP directory to the matching SSH terminal as a visible `cd`.
    pub(in crate::workspace::sftp) fn sync_sftp_directory_to_terminal(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if path.is_empty() {
            return;
        }
        let Some(pane_id) = self.sftp_terminal_target_pane(cx) else {
            return;
        };
        let Some(command) = oxideterm_environment::current_directory_cd_command(&path) else {
            return;
        };
        let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() else {
            return;
        };
        if !pane.read(cx).can_sync_working_directory() {
            self.push_sftp_toast(
                self.i18n.t("sftp.toast.sync_terminal_busy"),
                None,
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        }
        // The SFTP path bar may still look focused after the terminal takes over.
        self.sftp_view().update(cx, |sftp, cx| {
            sftp.clear_input_focus(cx);
            sftp.dismiss_context_menu(cx);
        });
        let sent = pane.update(cx, |pane, cx| {
            if !pane.send_command_line(&command, cx) {
                return false;
            }
            // The command mark remains authoritative for success/rollback.
            pane.set_pending_current_working_directory_from_terminal_action(
                path.clone(),
                command.clone(),
                cx,
            );
            true
        });
        if !sent {
            self.push_sftp_toast(
                self.i18n.t("sftp.toast.sync_terminal_failed"),
                None,
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        }
        let tab_id = self
            .tabs(cx)
            .iter()
            .find(|tab| {
                tab.root_pane
                    .as_ref()
                    .is_some_and(|root| root.contains_pane(pane_id))
            })
            .map(|tab| tab.id);
        let Some(tab_id) = tab_id else {
            return;
        };
        self.tab_host
            .update(cx, |host, _| host.set_active_pane(Some(tab_id), pane_id));
        if !self.tab_host.read(cx).is_outside_main_window(tab_id) {
            self.set_main_window_active_tab(Some(tab_id), cx);
            self.focus_active_pane(window, cx);
        }
        cx.notify();
    }

    pub(in crate::workspace::sftp) fn reveal_sftp_terminal_directory(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.sftp_terminal_directory(cx) else {
            return;
        };
        self.sftp_view().update(cx, |sftp, cx| {
            sftp.clear_input_focus(cx);
            sftp.dismiss_context_menu(cx);
            sftp.active_pane = SftpPane::Remote;
        });
        self.ime_marked_text = None;
        self.clear_ime_selection();
        self.set_sftp_path(SftpPane::Remote, path, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn open_sftp_tab(
        &mut self,
        node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let remote_path = self.active_ssh_terminal_cwd_path_for_node(&node_id, cx);
        self.open_sftp_with_preference(node_id, remote_path, window, cx);
    }

    fn open_sftp_with_preference(
        &mut self,
        node_id: NodeId,
        remote_path: Option<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.node_allows_session_tools(&node_id) {
            return;
        }
        match self.sftp_presentation_for_node(
            &node_id,
            self.settings_store.settings().sftp.presentation,
            cx,
        ) {
            oxideterm_settings::SftpPresentationPreference::Ask => {
                self.sftp_presentation_request = Some(SftpPresentationRequest {
                    node_id,
                    remote_path,
                });
                self.prepare_modal_interaction_boundary(cx);
                cx.notify();
            }
            oxideterm_settings::SftpPresentationPreference::Tab => {
                self.open_sftp_tab_surface(node_id, remote_path, cx);
            }
            oxideterm_settings::SftpPresentationPreference::Sidebar => {
                self.open_sftp_sidebar_surface(node_id, remote_path, cx);
            }
        }
    }

    fn sftp_presentation_for_node(
        &self,
        node_id: &NodeId,
        preferred: oxideterm_settings::SftpPresentationPreference,
        cx: &App,
    ) -> oxideterm_settings::SftpPresentationPreference {
        sftp_open_presentation(
            preferred,
            self.sftp_tab_nodes
                .values()
                .any(|existing| existing == node_id),
            self.active_tab(cx)
                .is_some_and(|tab| tab.kind == TabKind::Sftp),
        )
    }

    pub(in crate::workspace) fn open_sftp_tab_surface(
        &mut self,
        node_id: NodeId,
        initial_remote_path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if !self.node_allows_session_tools(&node_id) {
            return;
        }
        let node_title = self
            .ssh_nodes
            .get(&node_id)
            .map(|node| node.title.clone())
            .unwrap_or_else(|| node_id.0.clone());
        let title = format!("{} · {}", self.i18n.t("sidebar.panels.sftp"), node_title);
        let tab_id = if let Some((tab_id, _)) = self
            .sftp_tab_nodes
            .iter()
            .find(|(_, existing_node_id)| *existing_node_id == &node_id)
        {
            *tab_id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::Sftp,
                    title,
                    title_source: TabTitleSource::Static,
                    root_pane: None,
                    active_pane_id: None,
                },
                cx,
            );
            self.sftp_tab_nodes.insert(tab_id, node_id.clone());
            tab_id
        };

        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        let _scope = self.enter_sftp_surface(SftpSurfaceId::Tab(tab_id));
        self.sftp_focused_surface = SftpSurfaceId::Tab(tab_id);
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.active_ssh_node_id = Some(node_id.clone());
        self.activate_sftp_view_for_node(tab_id, &node_id, cx);
        if let Some(path) = initial_remote_path.filter(|path| !path.trim().is_empty()) {
            // SFTP keeps its own remembered path, but an explicit open from an
            // active SSH terminal can use that pane cwd as the initial folder.
            self.set_sftp_path(SftpPane::Remote, path, cx);
        }
        // Opening the SFTP surface mirrors Tauri's createTab path: it does
        // not start SSH. The SFTP worker consumes an already-connected node
        // and reports the router's not-connected error when the node is down.
        self.request_sftp_remote_load(cx);
        cx.notify();
    }

    /// Opens the dedicated independent-SFTP surface without consulting sidebar preferences.
    pub(in crate::workspace) fn open_standalone_sftp_tab_surface(
        &mut self,
        endpoint_id: String,
        endpoint_title: String,
        initial_remote_path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let ftp = self.ftp_sessions.get(&endpoint_id);
        let label = ftp.map(|runtime| {
            if runtime.options.security == oxideterm_ftp::Security::ExplicitTls {
                "FTPS"
            } else {
                "FTP"
            }
        });
        let title = format!(
            "{} · {}",
            label
                .map(str::to_owned)
                .unwrap_or_else(|| self.i18n.t("sidebar.panels.sftp")),
            endpoint_title
        );
        let remote_id = if ftp.is_some() {
            SftpRemoteId::Ftp(endpoint_id.clone())
        } else {
            SftpRemoteId::Standalone(endpoint_id.clone())
        };
        let tab_id = if let Some((tab_id, _)) = self
            .standalone_sftp_tabs
            .iter()
            .find(|(_, binding)| binding.primary_endpoint_id == endpoint_id)
        {
            *tab_id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::Sftp,
                    title,
                    title_source: TabTitleSource::Static,
                    root_pane: None,
                    active_pane_id: None,
                },
                cx,
            );
            self.standalone_sftp_tabs.insert(
                tab_id,
                StandaloneSftpTabBinding {
                    primary_endpoint_id: endpoint_id.clone(),
                    secondary_endpoint_id: None,
                    secondary_initial_remote_path: None,
                },
            );
            tab_id
        };

        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        let _scope = self.enter_sftp_surface(SftpSurfaceId::Tab(tab_id));
        self.sftp_focused_surface = SftpSurfaceId::Tab(tab_id);
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.active_ssh_node_id = None;
        self.sftp_view().update(cx, |sftp, cx| {
            sftp.activate_view(SftpSurfaceId::Tab(tab_id), remote_id);
            cx.notify();
        });
        if let Some(path) = initial_remote_path.filter(|path| !path.trim().is_empty()) {
            self.set_sftp_path(SftpPane::Remote, path, cx);
        }
        self.request_sftp_remote_load(cx);
        cx.notify();
    }

    /// Opens one tab that owns two independent standalone SFTP endpoint consumers.
    pub(in crate::workspace) fn open_standalone_sftp_pair_tab_surface(
        &mut self,
        primary_endpoint_id: String,
        secondary_endpoint_id: String,
        endpoint_title: String,
        primary_initial_remote_path: Option<String>,
        secondary_initial_remote_path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let title = format!(
            "{} · {}",
            self.i18n.t("sidebar.panels.sftp"),
            endpoint_title
        );
        let tab_id = if let Some((tab_id, _)) =
            self.standalone_sftp_tabs.iter().find(|(_, binding)| {
                binding.primary_endpoint_id == primary_endpoint_id
                    && binding.secondary_endpoint_id.as_deref()
                        == Some(secondary_endpoint_id.as_str())
            }) {
            *tab_id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::Sftp,
                    title,
                    title_source: TabTitleSource::Static,
                    root_pane: None,
                    active_pane_id: None,
                },
                cx,
            );
            self.standalone_sftp_tabs.insert(
                tab_id,
                StandaloneSftpTabBinding {
                    primary_endpoint_id: primary_endpoint_id.clone(),
                    secondary_endpoint_id: Some(secondary_endpoint_id.clone()),
                    secondary_initial_remote_path: secondary_initial_remote_path.clone(),
                },
            );
            tab_id
        };

        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        let _scope = self.enter_sftp_surface(SftpSurfaceId::Tab(tab_id));
        self.sftp_focused_surface = SftpSurfaceId::Tab(tab_id);
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.active_ssh_node_id = None;
        self.sftp_view().update(cx, |sftp, cx| {
            sftp.activate_pair_view(
                SftpSurfaceId::Tab(tab_id),
                SftpRemoteId::Standalone(primary_endpoint_id),
                SftpRemoteId::Standalone(secondary_endpoint_id),
                primary_initial_remote_path,
            );
            cx.notify();
        });
        if let Some(path) = secondary_initial_remote_path.filter(|path| !path.trim().is_empty()) {
            self.set_sftp_path(SftpPane::Remote, path, cx);
        }
        self.request_sftp_pair_primary_load(cx);
        self.request_sftp_remote_load(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn open_sftp_tab_at_remote_path(
        &mut self,
        node_id: NodeId,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let remote_path = (!path.trim().is_empty()).then_some(path);
        self.open_sftp_with_preference(node_id, remote_path, window, cx);
    }

    pub(in crate::workspace) fn choose_sftp_presentation(
        &mut self,
        preference: oxideterm_settings::SftpPresentationPreference,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self.sftp_presentation_request.take() else {
            return;
        };
        self.edit_settings(|settings| settings.sftp.presentation = preference, cx);
        match self.sftp_presentation_for_node(&request.node_id, preference, cx) {
            oxideterm_settings::SftpPresentationPreference::Ask => {}
            oxideterm_settings::SftpPresentationPreference::Tab => {
                self.open_sftp_tab_surface(request.node_id, request.remote_path, cx);
            }
            oxideterm_settings::SftpPresentationPreference::Sidebar => {
                self.open_sftp_sidebar_surface(request.node_id, request.remote_path, cx);
            }
        }
    }

    pub(in crate::workspace) fn open_sftp_sidebar_surface(
        &mut self,
        node_id: NodeId,
        remote_path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if !self.node_allows_session_tools(&node_id) {
            return;
        }
        let _scope = self.enter_sftp_surface(SftpSurfaceId::Sidebar);
        // An explicit request for another server stays visible until the user enables following.
        self.embedded_sftp_pinned = self.active_ssh_terminal_node_id(cx).as_ref() != Some(&node_id);
        self.bind_sftp_surface_lease(SftpSurfaceId::Sidebar, &node_id);
        self.embedded_sftp_node_id = Some(node_id.clone());
        self.active_ssh_node_id = Some(node_id.clone());
        self.expanded_ssh_nodes.insert(node_id.clone());
        self.sftp_view().update(cx, |sftp, cx| {
            sftp.activate_view(SftpSurfaceId::Sidebar, SftpRemoteId::Node(node_id));
            cx.notify();
        });
        if let Some(path) = remote_path.filter(|path| !path.trim().is_empty()) {
            self.set_sftp_path(SftpPane::Remote, path, cx);
        }
        // Showing a navigator that hosts the embedded SFTP surface is the
        // visibility boundary that starts the pending node-backed directory
        // request exactly once. Keep the Session Manager panel when it is the
        // visible one instead of jumping to Active Sessions.
        if self.sidebar_hosts_embedded_sftp() {
            if self.rails.left.collapsed() {
                self.set_sidebar_collapsed_with_motion(false, cx);
            }
            self.activate_embedded_sftp_sidebar_if_visible(cx);
        } else {
            self.set_sidebar_section(SidebarSection::Sessions, cx);
        }
        // The sidebar is a consumer of the node-owned SFTP channel. Hiding it
        // never releases or disconnects the physical SSH node.
        cx.notify();
    }

    pub(in crate::workspace) fn close_embedded_sftp_for_node(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) -> bool {
        let _scope = self.enter_sftp_surface(SftpSurfaceId::Sidebar);
        if self.embedded_sftp_node_id.as_ref() != Some(node_id) {
            return false;
        }

        self.sftp_surface_leases.remove(&SftpSurfaceId::Sidebar);
        self.embedded_sftp_node_id = None;
        self.embedded_sftp_pinned = false;
        self.embedded_sftp_sidebar_resizing = false;
        if self
            .sftp_presentation_request
            .as_ref()
            .is_some_and(|request| &request.node_id == node_id)
        {
            self.sftp_presentation_request = None;
        }
        let deactivated = self.sftp_view().update(cx, |sftp, cx| {
            sftp.deactivate_view(
                SftpSurfaceId::Sidebar,
                &SftpRemoteId::Node(node_id.clone()),
                cx,
            )
        });
        if deactivated {
            self.ime_marked_text = None;
        }
        // Removing the embedded view releases only its presentation identity.
        // NodeRouter and any transfers, forwards, or sibling terminals retain
        // their independent ownership until their own lifecycle ends.
        cx.notify();
        true
    }

    pub(in crate::workspace) fn embedded_sftp_target(&self, cx: &App) -> Option<NodeId> {
        sidebar_sftp_target(
            self.embedded_sftp_node_id.as_ref(),
            self.embedded_sftp_pinned,
            self.active_ssh_terminal_node_id(cx),
        )
        .filter(|node| self.node_allows_session_tools(node))
    }

    pub(in crate::workspace) fn embedded_sftp_sidebar_dialog_view<'a>(
        &self,
        cx: &'a App,
    ) -> Option<&'a SftpWorkspaceEntity> {
        let sftp = self.sftp_view.read(cx);
        (self.sftp_sidebar_can_mount_dialog(cx) && sftp.dialog_is_open()).then_some(sftp)
    }

    pub(in crate::workspace) fn sftp_sidebar_can_mount_dialog(&self, cx: &App) -> bool {
        let sftp = self.sftp_view.read(cx);
        !self.rails.left.collapsed()
            && !self.settings_store.settings().sidebar_ui.zen_mode
            && self.sidebar_hosts_embedded_sftp()
            && self.embedded_sftp_node_id.is_some()
            && sftp.current_surface_id == Some(SftpSurfaceId::Sidebar)
            && self
                .active_content_tab(cx)
                .is_none_or(|tab| tab.kind != TabKind::Sftp)
    }

    pub(in crate::workspace) fn activate_embedded_sftp_sidebar_if_visible(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let _scope = self.enter_sftp_surface(SftpSurfaceId::Sidebar);
        if self.rails.left.collapsed()
            || !self.sidebar_hosts_embedded_sftp()
            || self
                .active_tab(cx)
                .is_some_and(|tab| tab.kind == TabKind::Sftp)
        {
            return;
        }
        let target = self.embedded_sftp_target(cx);
        let previous = {
            let sftp = self.sftp_view().read(cx);
            (sftp.current_surface_id == Some(SftpSurfaceId::Sidebar))
                .then(|| sftp.current_remote_id.clone())
                .flatten()
        };
        if let Some(previous) = previous.filter(|previous| {
            Some(previous)
                != target
                    .as_ref()
                    .map(|node| SftpRemoteId::Node(node.clone()))
                    .as_ref()
        }) {
            // Retire pending dialogs and selections with the old target. Transfers
            // retain their own remote identity and node consumer.
            self.sftp_view().update(cx, |sftp, cx| {
                sftp.deactivate_view(SftpSurfaceId::Sidebar, &previous, cx);
            });
            self.ime_marked_text = None;
        }
        let Some(node_id) = target else {
            self.sftp_surface_leases.remove(&SftpSurfaceId::Sidebar);
            return;
        };
        self.bind_sftp_surface_lease(SftpSurfaceId::Sidebar, &node_id);
        self.embedded_sftp_node_id = Some(node_id.clone());
        let already_active = {
            let sftp = self.sftp_view().read(cx);
            sftp.current_surface_id == Some(SftpSurfaceId::Sidebar)
                && sftp.current_remote_id.as_ref() == Some(&SftpRemoteId::Node(node_id.clone()))
        };
        if !already_active {
            self.sftp_view().update(cx, |sftp, cx| {
                sftp.activate_view(SftpSurfaceId::Sidebar, SftpRemoteId::Node(node_id));
                cx.notify();
            });
        }
        // Hidden views may already be active while retaining a queued load;
        // every visibility transition must wake that pending request.
        self.maybe_start_sftp_remote_load(cx);
    }

    pub(in crate::workspace) fn activate_sftp_view_for_node(
        &mut self,
        tab_id: TabId,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) {
        self.bind_sftp_surface_lease(SftpSurfaceId::Tab(tab_id), node_id);
        self.ensure_sftp_page(tab_id, cx);
        let _scope = self.enter_sftp_surface(SftpSurfaceId::Tab(tab_id));
        self.sftp_view().update(cx, |sftp, cx| {
            sftp.activate_view(
                SftpSurfaceId::Tab(tab_id),
                SftpRemoteId::Node(node_id.clone()),
            );
            cx.notify();
        });
        self.maybe_start_sftp_remote_load(cx);
    }

    pub(in crate::workspace) fn maybe_start_sftp_remote_load(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let (surface_id, remote_id) = {
            let sftp = self.sftp_view().read(cx);
            let Some(surface_id) = sftp.current_surface_id else {
                return false;
            };
            let Some(remote_id) = sftp.current_remote_id.clone() else {
                return false;
            };
            (surface_id, remote_id)
        };
        if !self.sftp_surface_is_visible(surface_id, &remote_id, cx) {
            return false;
        }
        let Some(request) = self.sftp_view().update(cx, |state, _| {
            state.start_remote_load(surface_id, &remote_id)
        }) else {
            return false;
        };
        self.spawn_sftp_remote_load(request, self.sftp_view().read(cx).worker_sender());
        true
    }

    fn spawn_sftp_remote_load(
        &self,
        request: SftpRemoteLoadRequest,
        tx: delivery::ActiveDeliverySender<SftpWorkerResult>,
    ) {
        let SftpRemoteLoadRequest {
            surface_id,
            remote_id,
            path,
            view_generation,
            audit_request,
        } = request;
        let session_id = format!("{}:sftp", remote_id.storage_key());
        let runtime = self.forwarding_runtime.clone();
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            if let Some(context) = audit_request {
                context
                    .operation(
                        oxideterm_audit::AuditCategory::File,
                        "file_browse",
                        Some(&path),
                    )
                    .finish(
                        oxideterm_audit::AuditOutcome::Failed,
                        oxideterm_audit::AuditEvidence::Lifecycle,
                        None,
                        None,
                    );
            }
            let _ = tx.send(SftpWorkerResult::RemoteList {
                surface_id,
                remote_id,
                view_generation,
                session_id,
                path,
                result: Err("SFTP endpoint is no longer available".to_string()),
            });
            return;
        };
        if !matches!(&backend, SftpRemoteBackend::Ftp { .. }) {
            let owner_backend = backend.clone();
            runtime.spawn(async move {
                // The visible surface creates one shared SFTP channel through its explicit owner.
                let _ = owner_backend.acquire_sftp().await;
            });
        }
        runtime.spawn(async move {
            let audit_context = audit_request.map(|request| {
                let owner = match &backend {
                    SftpRemoteBackend::Node {
                        router, node_id, ..
                    } => router.audit_context(node_id),
                    SftpRemoteBackend::Standalone { handle } => handle.audit_context(),
                    SftpRemoteBackend::Ftp { runtime } => {
                        Some(runtime.endpoint_audit_context(request.clone()))
                    }
                };
                owner.map_or(request.clone(), |owner| owner.with_request(&request))
            });
            let result = oxideterm_sftp::audit_browser_listing(
                &path,
                audit_context,
                load_remote_sftp_listing(backend, &path),
            )
            .await;
            let _ = tx.send(SftpWorkerResult::RemoteList {
                surface_id,
                remote_id,
                view_generation,
                session_id,
                path,
                result,
            });
        });
    }

    fn spawn_sftp_tree_children_load(
        &self,
        surface_id: SftpSurfaceId,
        remote_id: SftpRemoteId,
        view_generation: u64,
        tree_generation: u64,
        path: String,
        tx: delivery::ActiveDeliverySender<SftpWorkerResult>,
    ) {
        let runtime = self.forwarding_runtime.clone();
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            let _ = tx.send(SftpWorkerResult::TreeChildren {
                surface_id,
                remote_id,
                view_generation,
                tree_generation,
                path,
                result: Err("SFTP endpoint is no longer available".to_string()),
            });
            return;
        };
        runtime.spawn(async move {
            // Observational listing: expanding a node must not move the node's
            // SFTP ready cwd to the expanded directory.
            let result = load_remote_sftp_completion_listing(backend, &path).await;
            let _ = tx.send(SftpWorkerResult::TreeChildren {
                surface_id,
                remote_id,
                view_generation,
                tree_generation,
                path,
                result,
            });
        });
    }

    pub(in crate::workspace) fn handle_sftp_worker_effects(
        &mut self,
        effect_batch: &SftpWorkspaceEffects,
        cx: &mut Context<Self>,
    ) {
        let delivery = effect_batch.delivery();
        for effect in effect_batch.take() {
            match effect {
                SftpWorkspaceEffect::BindSession {
                    remote_id,
                    session_id,
                    cwd,
                } => {
                    if let Some(node_id) = remote_id.node_id()
                        && let Ok(event) =
                            self.node_router
                                .bind_sftp_session(node_id, session_id, Some(cwd))
                    {
                        // Binding only reports node-owned readiness. It never
                        // grants SFTP authority to start or stop the SSH link.
                        self.emit_node_event(event);
                    }
                }
                SftpWorkspaceEffect::LoadBackgroundTransfers { remote_id } => {
                    self.spawn_sftp_background_transfer_load_with_sender(
                        remote_id,
                        delivery.clone(),
                    );
                }
                SftpWorkspaceEffect::LoadIncompleteTransfers { remote_id } => {
                    self.spawn_sftp_incomplete_load_with_sender(remote_id, delivery.clone());
                }
                SftpWorkspaceEffect::RemoteLoadPending {
                    surface_id,
                    remote_id,
                } => {
                    if self.sftp_surface_is_visible(surface_id, &remote_id, cx) {
                        let _ = delivery.send(SftpWorkerResult::StartRemoteLoad {
                            surface_id,
                            remote_id,
                        });
                    }
                }
                SftpWorkspaceEffect::StartRemoteLoad(request) => {
                    self.spawn_sftp_remote_load(request, delivery.clone());
                }
                SftpWorkspaceEffect::TreeChildrenLoadFailed { path, error } => {
                    self.push_sftp_toast(
                        self.i18n.t("sftp.toast.load_more_failed"),
                        Some(format!("{path}: {error}")),
                        TerminalNoticeVariant::Error,
                        cx,
                    );
                }
                SftpWorkspaceEffect::StartTreeChildrenLoad {
                    surface_id,
                    remote_id,
                    view_generation,
                    tree_generation,
                    path,
                } => {
                    self.spawn_sftp_tree_children_load(
                        surface_id,
                        remote_id,
                        view_generation,
                        tree_generation,
                        path,
                        delivery.clone(),
                    );
                }
                SftpWorkspaceEffect::TransferFinishedForReconnect {
                    remote_id,
                    transfer_id,
                    success,
                } => {
                    if let Some(node_id) = remote_id.node_id() {
                        self.on_sftp_transfer_finished_for_reconnect(
                            node_id,
                            &transfer_id,
                            success,
                            cx,
                        );
                    }
                }
                SftpWorkspaceEffect::TransferBatchCompleted(batch) => {
                    if let Some(notice) = batch.notice(&self.i18n) {
                        self.push_workspace_notice(notice, cx);
                    }
                }
                SftpWorkspaceEffect::StartTransfer(launch) => {
                    self.spawn_sftp_transfer_launch_with_sender(launch, delivery.clone());
                }
                SftpWorkspaceEffect::Toast {
                    title,
                    description,
                    variant,
                } => {
                    self.push_sftp_toast(title, description, variant, cx);
                }
                SftpWorkspaceEffect::ReloadPairPrimaryDirectory => {
                    self.request_sftp_pair_primary_load(cx);
                }
            }
        }
    }

    pub(in crate::workspace) fn request_visible_sftp_remote_load(
        &self,
        surface_id: SftpSurfaceId,
        remote_id: SftpRemoteId,
        delivery: delivery::ActiveDeliverySender<SftpWorkerResult>,
        cx: &App,
    ) {
        if self.sftp_surface_is_visible(surface_id, &remote_id, cx) {
            let _ = delivery.send(SftpWorkerResult::StartRemoteLoad {
                surface_id,
                remote_id,
            });
        }
    }

    fn sftp_surface_is_visible(
        &self,
        surface_id: SftpSurfaceId,
        remote_id: &SftpRemoteId,
        cx: &App,
    ) -> bool {
        match surface_id {
            SftpSurfaceId::Tab(tab_id) => {
                self.tab_host.read(cx).surface_is_visible(tab_id)
                    && self
                        .tabs(cx)
                        .iter()
                        .any(|tab| tab.id == tab_id && tab.kind == TabKind::Sftp)
                    && self.sftp_remote_id_for_tab(tab_id).as_ref() == Some(remote_id)
            }
            SftpSurfaceId::Sidebar => {
                let Some(node_id) = remote_id.node_id() else {
                    return false;
                };
                !self.rails.left.collapsed()
                    && self.sidebar_hosts_embedded_sftp()
                    && !self
                        .active_tab(cx)
                        .is_some_and(|tab| tab.kind == TabKind::Sftp)
                    && self.embedded_sftp_target(cx).as_ref() == Some(node_id)
            }
        }
    }

    pub(in crate::workspace) fn visible_sftp_remote_id(&self, cx: &App) -> Option<SftpRemoteId> {
        let sftp = self.sftp_view().read(cx);
        let surface_id = sftp.current_surface_id?;
        let remote_id = sftp.current_remote_id.clone()?;
        self.sftp_surface_is_visible(surface_id, &remote_id, cx)
            .then_some(remote_id)
    }

    pub(in crate::workspace::sftp) fn visible_sftp_node_id(&self, cx: &App) -> Option<NodeId> {
        self.visible_sftp_remote_id(cx)?.node_id().cloned()
    }

    pub(in crate::workspace) fn apply_sftp_ready_event(
        &mut self,
        node_id: &NodeId,
        ready: bool,
        cwd: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let surfaces = std::iter::once(SftpSurfaceId::Sidebar)
            .chain(self.sftp_pages.keys().copied().map(SftpSurfaceId::Tab))
            .collect::<Vec<_>>();
        for surface in surfaces {
            let _scope = self.enter_sftp_surface(surface);
            self.sftp_view().update(cx, |sftp, cx| {
                if sftp.current_remote_id.as_ref() != Some(&SftpRemoteId::Node(node_id.clone())) {
                    return;
                }
                sftp.apply_router_sftp_ready(ready, cwd.clone());
                cx.notify();
            });
        }
    }
}

#[cfg(test)]
mod remote_load_state_tests {
    use super::*;

    #[test]
    fn reopening_expanded_sftp_keeps_directory_load_on_visible_tab() {
        use oxideterm_settings::SftpPresentationPreference;

        let mut sftp = SftpWorkspaceEntity::default();
        let remote = SftpRemoteId::Node(NodeId::new("reopened-server"));
        let tab = SftpSurfaceId::Tab(TabId(1));
        sftp.activate_view(SftpSurfaceId::Sidebar, remote.clone());
        sftp.activate_view(tab, remote.clone());
        // Complete the initial listing before repeating the sidebar action.
        sftp.start_remote_load(tab, &remote)
            .expect("initial tab load");
        sftp.set_remote_load_state(sftp.remote_load_state().complete());
        let presentation = sftp_open_presentation(SftpPresentationPreference::Sidebar, true, true);
        let surface = match presentation {
            SftpPresentationPreference::Tab => tab,
            SftpPresentationPreference::Sidebar => SftpSurfaceId::Sidebar,
            SftpPresentationPreference::Ask => panic!("existing tab must not ask again"),
        };
        sftp.activate_view(surface, remote.clone());

        assert!(sftp.start_remote_load(tab, &remote).is_some());
        assert!(!sftp.remote_load_pending);
        assert!(sftp.remote_load_inflight);
    }

    #[test]
    fn sftp_ready_event_preserves_pending_terminal_cwd() {
        let mut sftp = SftpWorkspaceEntity::default();
        // The shared session may become ready after the terminal cwd has queued a newer load.
        sftp.remote_path = "/root/.oxideterm".to_string();
        sftp.remote_path_input = sftp.remote_path.clone();
        sftp.remote_load_pending = true;

        sftp.apply_router_sftp_ready(true, Some("/root".to_string()));

        assert_eq!(sftp.remote_path, "/root/.oxideterm");
        assert_eq!(sftp.remote_path_input, "/root/.oxideterm");
        assert!(sftp.remote_loading);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn markdown_preview_cache_releases_on_close(cx: &mut gpui::TestAppContext) {
        let remote = cx.new(SftpWorkspaceEntity::new);
        remote.update(cx, |state, cx| {
            state.set_dialog(SftpDialog::Preview {
                name: "readme.md".into(),
            });
            let owner = Arc::new(PreviewContent::Text {
                version: None,
                data: "# Remote".into(),
                mime_type: Some("text/markdown".into()),
                language: Some("markdown".into()),
                encoding: "UTF-8".into(),
                confidence: 1.0,
                has_bom: false,
            });
            let document = state.preview_markdown_cache.document(
                &owner,
                "# Remote",
                true,
                &mut state.preview_markdown_scroll,
            );
            let blocks = Arc::downgrade(&document.blocks);
            drop(document);
            state.preview_content = Some(owner);
            state.begin_dialog_exit(Duration::ZERO, cx);
            assert!(
                blocks.upgrade().is_none(),
                "closed remote preview retained its parsed document"
            );
        });
    }

    #[gpui::test]
    fn closed_preview_releases_content_and_rejects_late_delivery(cx: &mut gpui::TestAppContext) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        entity.update(cx, |sftp, cx| {
            sftp.set_dialog(SftpDialog::Preview {
                name: "sample.txt".into(),
            });
            let content = Arc::new(PreviewContent::Hex {
                data: "ab".repeat(512 * 1024),
                total_size: 512 * 1024,
                offset: 0,
                chunk_size: 512 * 1024,
                has_more: false,
            });
            let weak = Arc::downgrade(&content);
            sftp.preview_content = Some(content);
            let generation = sftp.preview_generation;
            sftp.begin_dialog_exit(Duration::ZERO, cx);
            eprintln!(
                "SFTP_PREVIEW_RETAINED_BYTES={}",
                if weak.upgrade().is_some() {
                    1024 * 1024
                } else {
                    0
                }
            );
            assert!(
                weak.upgrade().is_none(),
                "closed preview retained its payload"
            );
            assert!(!sftp.apply_preview_loaded(
                generation,
                "sample.txt".into(),
                Ok(PreviewContent::Unsupported {
                    mime_type: "text/plain".into(),
                    reason: "late completion".into(),
                }),
                None,
                cx
            ));
            assert!(sftp.preview_content.is_none());
        });
    }

    #[test]
    fn terminal_directory_target_stays_on_the_file_browsers_server() {
        let node = NodeId::new("server-a");
        let recent = HashMap::from([
            (node.clone(), PaneId(1)),
            (NodeId::new("server-b"), PaneId(2)),
        ]);
        for (remote, active, expected) in [
            (
                SftpRemoteId::Node(node.clone()),
                Some((CurrentDirectoryScope::ssh_node("server-a"), PaneId(3))),
                Some(PaneId(3)),
            ),
            (
                SftpRemoteId::Node(node.clone()),
                Some((CurrentDirectoryScope::ssh_node("server-b"), PaneId(4))),
                Some(PaneId(1)),
            ),
            (SftpRemoteId::Node(node.clone()), None, Some(PaneId(1))),
            (
                SftpRemoteId::Node(node),
                Some((CurrentDirectoryScope::Local, PaneId(5))),
                Some(PaneId(1)),
            ),
            (SftpRemoteId::Node(NodeId::new("server-c")), None, None),
            (
                SftpRemoteId::Standalone("independent".into()),
                Some((CurrentDirectoryScope::ssh_node("server-a"), PaneId(3))),
                None,
            ),
        ] {
            assert_eq!(
                sftp_terminal_directory_pane(&remote, active.clone(), &recent),
                expected,
                "remote={remote:?}, active={active:?}"
            );
        }
    }

    struct TestKeys;

    impl oxideterm_audit::AuditKeyProvider for TestKeys {
        fn load(
            &self,
            _: &str,
        ) -> Result<zeroize::Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
            Ok(zeroize::Zeroizing::new(vec![17; 32]))
        }

        fn create(
            &self,
            id: &str,
        ) -> Result<zeroize::Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
            self.load(id)
        }
    }

    #[test]
    fn queued_navigation_keeps_its_request_and_refresh_does_not_reuse_it() {
        let directory = tempfile::tempdir().unwrap();
        let service = oxideterm_audit::AuditService::with_key_provider(
            directory.path().join("audit.db"),
            TestKeys,
        )
        .unwrap();
        let context = oxideterm_audit::AuditContext::new(
            service.client(),
            oxideterm_audit::AuditSource::User,
        );
        let mut state = SftpWorkspaceEntity::default();
        let surface = SftpSurfaceId::Sidebar;
        let remote = SftpRemoteId::Node(NodeId::new("node-a"));
        state.activate_view(surface, remote.clone());
        state.remote_path = "/manual".into();
        state.remote_browse_request = Some((remote.clone(), "/manual".into(), context));

        let request = state.start_remote_load(surface, &remote).unwrap();
        assert_eq!(request.path, "/manual");
        assert_eq!(
            request.audit_request.unwrap().source,
            oxideterm_audit::AuditSource::User
        );

        state.remote_load_inflight = false;
        state.request_remote_load();
        let request = state.start_remote_load(surface, &remote).unwrap();
        assert_eq!(request.path, "/manual");
        assert!(request.audit_request.is_none());
    }

    #[gpui::test]
    fn accepted_root_listing_clears_tree_state(cx: &mut gpui::TestAppContext) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        entity.update(cx, |sftp, _cx| {
            sftp.current_surface_id = Some(SftpSurfaceId::Sidebar);
            sftp.current_remote_id = Some(SftpRemoteId::Node(NodeId::new("node")));
            sftp.view_generation = 1;
            sftp.remote_path = "/root".to_string();
            sftp.remote_path_input = "/root".to_string();
            sftp.remote_load_inflight = true;
            sftp.tree_expanded.insert("/root/dir".to_string());
            sftp.tree_children
                .insert("/root/dir".to_string(), Vec::new());
            sftp.tree_loading.insert("/root/dir".to_string());
            sftp.tree_pending.push_back("/root/other".to_string());
            sftp.tree_selected = Some("/root/dir".to_string());
        });
        let changed = entity.update(cx, |sftp, cx| {
            sftp.reduce_worker_result(
                SftpWorkerResult::RemoteList {
                    surface_id: SftpSurfaceId::Sidebar,
                    remote_id: SftpRemoteId::Node(NodeId::new("node")),
                    view_generation: 1,
                    session_id: "session".to_string(),
                    path: "/root".to_string(),
                    result: Ok(RemoteSftpListing {
                        cwd: "/root".to_string(),
                        files: Vec::new(),
                    }),
                },
                &mut VecDeque::new(),
                cx,
            )
        });
        assert!(changed);
        entity.read_with(cx, |sftp, _| {
            assert!(sftp.tree_expanded.is_empty());
            assert!(sftp.tree_children.is_empty());
            assert!(sftp.tree_loading.is_empty());
            assert!(sftp.tree_pending.is_empty());
            assert!(sftp.tree_selected.is_none());
        });
    }
}
