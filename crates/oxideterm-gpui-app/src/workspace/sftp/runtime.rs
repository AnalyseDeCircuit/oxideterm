impl WorkspaceApp {
    pub(super) fn open_sftp_tab(
        &mut self,
        node_id: NodeId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::Sftp,
                title,
                title_source: TabTitleSource::Static,
                root_pane: None,
                active_pane_id: None,
            });
            self.sftp_tab_nodes.insert(tab_id, node_id.clone());
            tab_id
        };

        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.active_sidebar_section = SidebarSection::Sessions;
        self.active_ssh_node_id = Some(node_id);
        self.sftp_view.remote_load_pending = true;
        self.persist_sidebar_settings();
        cx.notify();
    }

    pub(super) fn maybe_start_sftp_remote_load(&mut self, cx: &mut Context<Self>) {
        if self.sftp_view.remote_load_inflight || !self.sftp_view.remote_load_pending {
            return;
        }
        let Some(tab_id) = self.active_tab_id else {
            return;
        };
        if self
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .is_none_or(|tab| tab.kind != TabKind::Sftp)
        {
            return;
        }
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        let path = self.sftp_view.remote_path.clone();
        self.start_sftp_remote_load(tab_id, node_id, path, cx);
    }

    fn start_sftp_remote_load(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        path: String,
        cx: &mut Context<Self>,
    ) {
        let session_id = format!("node:{}:sftp", node_id.0);
        let resolved = match self
            .node_router
            .acquire_connection(&node_id, ConnectionConsumer::Sftp(session_id.clone()))
        {
            Ok(resolved) => resolved,
            Err(error) => {
                self.sftp_view.remote_loading = false;
                self.sftp_view.remote_load_pending = false;
                self.sftp_view.remote_load_inflight = false;
                self.sftp_view.init_error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        self.sftp_connection_consumers.insert(
            session_id.clone(),
            (
                resolved.connection_id.clone(),
                ConnectionConsumer::Sftp(session_id.clone()),
            ),
        );
        self.sftp_view.remote_loading = true;
        self.sftp_view.remote_load_pending = false;
        self.sftp_view.remote_load_inflight = true;
        self.sftp_view.init_error = None;

        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        let router = self.node_router.clone();
        runtime.spawn(async move {
            let result = load_remote_sftp_listing(router, &node_id, &path).await;
            let _ = tx.send(SftpWorkerResult::RemoteList {
                tab_id,
                node_id,
                session_id,
                path,
                result,
            });
        });
        cx.notify();
    }

    pub(super) fn poll_sftp_worker_results(&mut self, cx: &mut Context<Self>) {
        let mut results = Vec::new();
        while let Ok(result) = self.sftp_worker_rx.try_recv() {
            results.push(result);
        }

        let mut changed = false;
        for result in results {
            match result {
                SftpWorkerResult::RemoteList {
                    tab_id,
                    node_id,
                    session_id,
                    path,
                    result,
                } => {
                    if Some(tab_id) == self.active_tab_id {
                        self.sftp_view.remote_load_inflight = false;
                        self.sftp_view.remote_loading = false;
                        match result {
                            Ok(listing) => {
                                let cwd = listing.cwd;
                                self.sftp_view.remote_path = cwd.clone();
                                self.sftp_view.remote_path_input = cwd.clone();
                                self.sftp_view.remote_files = listing.files;
                                self.sftp_view.remote_selected.clear();
                                self.sftp_view.remote_last_selected = None;
                                self.sftp_view.init_error = None;
                                // GPUI still carries a session id for tab/UI compatibility, but
                                // the real SFTP owner lives in ConnectionEntry via NodeRouter.
                                if let Ok(event) = self.node_router.bind_sftp_session(
                                    &node_id,
                                    session_id,
                                    Some(cwd),
                                ) {
                                    self.emit_node_event(event);
                                }
                            }
                            Err(error) => {
                                self.sftp_view.init_error = Some(format!("{}: {error}", path));
                            }
                        }
                        changed = true;
                    }
                }
                SftpWorkerResult::TransferProgress {
                    id,
                    transferred,
                    total,
                    state,
                    error,
                } => {
                    if let Some(item) = self
                        .sftp_view
                        .transfers
                        .iter_mut()
                        .find(|item| item.id == id)
                    {
                        item.transferred = transferred;
                        item.size = total.max(item.size);
                        item.state = state;
                        item.error = error;
                        changed = true;
                    }
                }
                SftpWorkerResult::TransferComplete {
                    id,
                    result,
                    refresh_remote,
                    refresh_local,
                } => {
                    if let Some(item) = self
                        .sftp_view
                        .transfers
                        .iter_mut()
                        .find(|item| item.id == id)
                    {
                        match result {
                            Ok(()) => {
                                item.transferred = item.size;
                                item.state = SftpTransferState::Completed;
                                item.error = None;
                            }
                            Err(error) => {
                                item.state = SftpTransferState::Error;
                                item.error = Some(error);
                            }
                        }
                    }
                    if refresh_remote {
                        self.sftp_view.remote_load_pending = true;
                    }
                    if refresh_local && let Ok(files) = list_local_files(&self.sftp_view.local_path)
                    {
                        self.sftp_view.local_files = files;
                    }
                    changed = true;
                }
                SftpWorkerResult::RemoteMutationComplete {
                    result,
                    refresh_remote,
                    refresh_local,
                } => {
                    if let Err(error) = result {
                        self.sftp_view.init_error = Some(error);
                    }
                    if refresh_remote {
                        self.sftp_view.remote_load_pending = true;
                    }
                    if refresh_local && let Ok(files) = list_local_files(&self.sftp_view.local_path)
                    {
                        self.sftp_view.local_files = files;
                    }
                    changed = true;
                }
                SftpWorkerResult::PreviewLoaded { path, result } => {
                    self.sftp_view.preview_loading = false;
                    self.sftp_view.preview_path = Some(path);
                    match result {
                        Ok(content) => {
                            let asset_owner =
                                PreviewAssetOwner::from_asset_content_owned_temp(&content);
                            if let Some(owner) = asset_owner.as_ref() {
                                match owner.kind() {
                                    AssetFileKind::Audio => {
                                        let _ = self.sftp_view.preview_audio.load(owner.path());
                                    }
                                    AssetFileKind::Video => {
                                        let _ = self.sftp_view.preview_video.load(owner.path());
                                    }
                                    AssetFileKind::Image
                                    | AssetFileKind::Pdf
                                    | AssetFileKind::Office => {}
                                }
                            }
                            self.sftp_view.preview_session =
                                PreviewSession::ready(content.clone(), asset_owner.clone());
                            self.sftp_view.preview_asset_owner = asset_owner;
                            self.sftp_view.preview_content = Some(content);
                            self.sftp_view.preview_error = None;
                        }
                        Err(error) => {
                            self.sftp_view.preview_content = None;
                            self.sftp_view.preview_asset_owner = None;
                            self.sftp_view.preview_session = PreviewSession::error(error.clone());
                            self.sftp_view.preview_error = Some(error);
                        }
                    }
                    changed = true;
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    pub(super) fn apply_sftp_ready_event(
        &mut self,
        node_id: &NodeId,
        ready: bool,
        cwd: Option<String>,
    ) {
        if self
            .active_tab_id
            .and_then(|tab_id| self.sftp_tab_nodes.get(&tab_id))
            != Some(node_id)
        {
            return;
        }
        self.sftp_view.remote_loading = !ready;
        if let Some(cwd) = cwd {
            self.sftp_view.remote_path = cwd.clone();
            self.sftp_view.remote_path_input = cwd;
        }
    }
}
