use super::*;
use std::sync::Weak;

#[derive(Clone, PartialEq, Eq, Hash)]
pub(in crate::workspace) struct DiscoveryOpenKey {
    window: gpui::WindowId,
    source: String,
    target: String,
}

pub(in crate::workspace) struct PendingDiscoveryOpen {
    target_generation: Weak<()>,
    owner: Weak<()>,
    node: Option<NodeId>,
}

enum OpenCandidate {
    Terminal(TerminalSessionId),
    InProgress(NodeId),
    Transport(NodeId),
}

impl WorkspaceApp {
    pub(in crate::workspace) fn open_discovery_target(
        &mut self,
        id: &str,
        target: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_discovery_open(id, target, true, window, cx);
    }

    pub(in crate::workspace) fn reopen_discovery_target(
        &mut self,
        id: &str,
        target: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_discovery_open(id, target, false, window, cx);
    }

    pub(in crate::workspace) fn reconnect_discovered_node(
        &mut self,
        node: &NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.node_connection_is_active_or_connecting(node) {
            return;
        }
        let Some(previous) = self.node_router.runtime_store().discovery_origin(node) else {
            return;
        };
        let Some(source) = self
            .discovery
            .source(&previous.source_id)
            .filter(|s| self.discovery.can_open(&s.id, &previous.target_id))
        else {
            return self.report_discovery_open_failure(DiscoveryFailure::StaleTarget, cx);
        };
        let request = Arc::new(());
        let token = self.workspace_runtime.update(cx, |runtime, _| {
            runtime.begin_discovery_reconnect(node, &request)
        });
        if let Some(token) = token {
            self.open_process_discovery_target(
                source,
                previous.target_id.clone(),
                request,
                Some((node.clone(), previous, token)),
                window,
                cx,
            );
        }
    }

    pub(in crate::workspace) fn restore_discovery_transport(
        &mut self,
        node: NodeId,
        config: SshConfig,
        cx: &mut Context<Self>,
    ) {
        if self.node_connection_is_active_or_connecting(&node)
            || self
                .workspace_runtime
                .read(cx)
                .node_has_in_flight_connection(&node)
            || !self.ssh_nodes.contains_key(&node)
        {
            return;
        }
        let snapshot = self.node_reconnect_snapshot(&node, std::slice::from_ref(&node), cx);
        let resolver =
            oxideterm_session_adapter::managed_key_resolver_from_store(&self.connection_store);
        let result = self.workspace_runtime.update(cx, |runtime, _| {
            runtime.start_node_transport_attempt(
                &node,
                resolver,
                self.i18n.clone(),
                Some((config, snapshot)),
            )
        });
        match result {
            Ok(()) => {
                self.forget_closed_ssh_session(&node);
                if let Some(record) = self.ssh_nodes.get_mut(&node) {
                    record.readiness = NodeReadiness::Connecting;
                }
                cx.notify();
            }
            Err(runtime_entity::NodeTransportStartError::Route(message)) => {
                self.report_discovery_open_error(message, cx)
            }
            Err(_) => self.report_discovery_open_failure(DiscoveryFailure::StaleTarget, cx),
        }
    }

    pub(in crate::workspace) fn prompt_reopen_discovery_target(
        &mut self,
        node_id: &NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prompt_discovery_reopen(node_id, None, window, cx);
    }

    fn prompt_discovery_reopen(
        &mut self,
        node_id: &NodeId,
        pending: Option<(DiscoveryOpenKey, Arc<()>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(origin) = self.node_router.runtime_store().discovery_origin(node_id) else {
            return;
        };
        let title = self.ssh_nodes.get(node_id).map(|node| node.title.as_str());
        let action = self.i18n.t("settings_view.discovery.reopen");
        let cancel = self.i18n.t("common.actions.cancel");
        let generation = self.discovery.epoch(&origin.source_id).and_then(|epoch| {
            self.discovery
                .directory_target_generation(&origin.source_id, epoch, &origin.target_id)
        });
        let Some(generation) = generation else {
            self.report_discovery_open_failure(DiscoveryFailure::StaleTarget, cx);
            return;
        };
        let prompt = window.prompt(
            gpui::PromptLevel::Info,
            &action,
            title,
            &[cancel.as_str(), action.as_str()],
            cx,
        );
        cx.spawn_in(window, async move |workspace, cx| {
            if let Ok(1) = prompt.await {
                let _ = workspace.update_in(cx, |this, window, cx| {
                    if pending
                        .as_ref()
                        .is_some_and(|(key, request)| !this.owns_discovery_open(Some(key), request))
                    {
                        return;
                    }
                    // Reopening depends on the selected target, independently of the old transport.
                    if generation.strong_count() == 0 {
                        this.report_discovery_open_failure(DiscoveryFailure::StaleTarget, cx);
                        return;
                    }
                    if let Some((_, request)) = pending {
                        let Some(source) = this
                            .discovery
                            .source(&origin.source_id)
                            .filter(|s| this.discovery.can_open(&s.id, &origin.target_id))
                        else {
                            this.report_discovery_open_failure(DiscoveryFailure::StaleTarget, cx);
                            return;
                        };
                        // The new plugin instance still belongs to the directory click that opened the prompt.
                        this.open_process_discovery_target(
                            source,
                            origin.target_id,
                            request,
                            None,
                            window,
                            cx,
                        );
                    } else {
                        this.reopen_discovery_target(
                            &origin.source_id,
                            origin.target_id,
                            window,
                            cx,
                        );
                    }
                });
            }
        })
        .detach();
    }

    pub(in crate::workspace) fn cancel_discovery_open_for_node(&mut self, node: &NodeId) {
        self.pending_discovery_opens
            .retain(|_, pending| pending.node.as_ref() != Some(node));
        if let Some(origin) = self.node_router.runtime_store().discovery_origin(node) {
            origin.finish_open_request();
        }
    }

    fn directory_open_candidate(
        &self,
        source: &str,
        target: &str,
        window: &Window,
        cx: &App,
    ) -> Option<OpenCandidate> {
        let mut username = None;
        let mut best = None;
        let tabs = self.tab_host.read(cx);
        let runtime = self.workspace_runtime.read(cx);
        for (node_id, node) in &self.ssh_nodes {
            let Some(origin) = self.node_router.runtime_store().discovery_origin(node_id) else {
                continue;
            };
            let endpoint = &node.endpoint;
            if origin.source_id != source
                || origin.target_id != target
                || !origin.directory_candidate_current()
                || !origin.matches_endpoint(&endpoint.host, endpoint.port, &endpoint.username)
            {
                continue;
            }
            let sessions = runtime.ssh_terminal_session_ids_for_node(node_id);
            let local_sessions: Vec<_> = sessions
                .iter()
                .filter_map(|id| {
                    let location = tabs.terminal_location(*id)?;
                    let owner = tabs.detached_window_handle(location.tab_id).or_else(|| {
                        self.window_registry
                            .handle_for_role(window_registry::WindowRole::Main)
                    });
                    if owner.is_some_and(|owner| {
                        owner.window_id() != window.window_handle().window_id()
                    }) {
                        return None;
                    }
                    Some((*id, tabs.panes().get(&location.pane_id)?))
                })
                .collect();
            if !sessions.is_empty() && local_sessions.is_empty() {
                continue;
            }
            if username
                .replace(&endpoint.username)
                .is_some_and(|other| other != &endpoint.username)
            {
                // The directory row did not select an account. Let the provider offer its choices.
                return None;
            }
            if !origin.allows_directory_reuse() {
                continue;
            }
            let handle = self
                .node_router
                .connection_id_for_node(node_id)
                .and_then(|id| self.ssh_registry.get(&id));
            let live = handle.as_ref().is_some_and(|handle| {
                matches!(
                    handle.state(),
                    ConnectionState::Active | ConnectionState::Idle
                ) && handle.has_open_transport()
            });
            let reconnecting = runtime.has_session_connection_work(node_id);
            let (priority, candidate) = if let Some((id, _)) = local_sessions
                .iter()
                .find(|(_, pane)| live && !pane.read(cx).can_reconnect_ssh())
            {
                (0, OpenCandidate::Terminal(*id))
            } else if reconnecting {
                (
                    1,
                    local_sessions.first().map_or_else(
                        || OpenCandidate::InProgress(node_id.clone()),
                        |(id, _)| OpenCandidate::Terminal(*id),
                    ),
                )
            } else if live {
                (2, OpenCandidate::Transport(node_id.clone()))
            } else {
                continue;
            };
            let rank = (
                priority,
                self.active_ssh_node_id.as_ref() != Some(node_id),
                &node_id.0,
            );
            if best.as_ref().is_none_or(|(previous, _)| rank < *previous) {
                best = Some((rank, candidate));
            }
        }
        best.map(|(_, candidate)| candidate)
    }

    fn owns_discovery_open(&self, key: Option<&DiscoveryOpenKey>, request: &Arc<()>) -> bool {
        key.is_none_or(|key| {
            self.pending_discovery_opens
                .get(key)
                .is_some_and(|pending| pending.owner.ptr_eq(&Arc::downgrade(request)))
        })
    }

    fn start_discovery_open(
        &mut self,
        id: &str,
        target: String,
        from_directory: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = self
            .discovery
            .source(id)
            .filter(|_| self.discovery.can_open(id, &target))
        else {
            self.report_discovery_open_failure(DiscoveryFailure::StaleTarget, cx);
            return;
        };
        let Some(target_generation) = self.discovery.epoch(id).and_then(|epoch| {
            self.discovery
                .directory_target_generation(id, epoch, &target)
        }) else {
            self.report_discovery_open_failure(DiscoveryFailure::StaleTarget, cx);
            return;
        };
        let mut original = None;
        let request = Arc::new(());
        let pending_key = from_directory.then(|| DiscoveryOpenKey {
            window: window.window_handle().window_id(),
            source: id.to_owned(),
            target: target.clone(),
        });
        if let Some(key) = &pending_key {
            self.pending_discovery_opens
                .retain(|_, p| p.owner.strong_count() > 0);
            if self
                .pending_discovery_opens
                .get(key)
                .is_some_and(|p| p.target_generation.ptr_eq(&target_generation))
            {
                return;
            }
            match self.directory_open_candidate(id, &target, window, cx) {
                Some(OpenCandidate::Terminal(session)) => {
                    self.focus_terminal_session(session, window, cx);
                    return;
                }
                Some(OpenCandidate::InProgress(node)) => {
                    self.set_sidebar_section(SidebarSection::Sessions, cx);
                    self.expanded_ssh_nodes.insert(node.clone());
                    self.active_ssh_node_id = Some(node);
                    cx.notify();
                    return;
                }
                Some(OpenCandidate::Transport(node)) if self.node_allows_session_tools(&node) => {
                    if let Err(error) =
                        self.queue_ssh_terminal_tab_for_sidebar_node(node, window, cx)
                    {
                        self.report_discovery_open_error(error.to_string(), cx);
                    }
                    return;
                }
                Some(OpenCandidate::Transport(node)) => original = Some(node),
                None => {}
            }
            self.pending_discovery_opens.insert(
                key.clone(),
                PendingDiscoveryOpen {
                    target_generation: target_generation.clone(),
                    owner: Arc::downgrade(&request),
                    node: original.clone(),
                },
            );
        }
        if let Some(node) = original {
            self.prompt_discovery_reopen(&node, pending_key.map(|key| (key, request)), window, cx);
            return;
        }
        self.open_process_discovery_target(source, target, request, None, window, cx);
    }
}
