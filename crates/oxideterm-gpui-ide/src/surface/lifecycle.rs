impl IdeSurface {
    pub fn new(
        #[cfg(not(target_family = "wasm"))] fs: NodeAgentIdeFileSystem,
        #[cfg(target_family = "wasm")] fs: std::rc::Rc<dyn PortableIdeHost>,
        tokens: ThemeTokens,
        labels: IdeLabels,
        runtime_settings: IdeRuntimeSettings,
        #[cfg(not(target_family = "wasm"))] backend_runtime: Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) -> Self {
        // Node surfaces own independent scopes on the shared transport. A shell
        // surface adopts the exclusive PTY lease owner; later scoped clients
        // cannot shut down that owner's channel when they release themselves.
        #[cfg(not(target_family = "wasm"))]
        let fs = if fs.is_bound() { fs } else { fs.scoped_owner() };
        Self {
            workspace: IdeWorkspace::new(),
            tab_close_batch: None,
            fs,
            tokens,
            labels,
            runtime_settings,
            focus_handle: cx.focus_handle(),
            modal_focus_handle: cx.focus_handle(),
            #[cfg(not(target_family = "wasm"))]
            backend_runtime,
            #[cfg(not(target_family = "wasm"))]
            desktop: DesktopSession::default(),
            load_state: IdeLoadState::Empty,
            node_id: None,
            root_path: None,
            git_branch: None,
            tree_width: IDE_TREE_DEFAULT_WIDTH,
            tree_resize: None,
            tree_resize_activation: None,
            generation: 0,
            editors: HashMap::new(),
            markdown_previews: HashMap::new(),
            loading_paths: HashSet::new(),
            loading_file_tabs: HashSet::new(),
            saving_tabs: HashSet::new(),
            pending_saves: HashMap::new(),
            file_open_errors: HashMap::new(),
            save_after_close: None,
            conflict_state: None,
            pending_restore_files: Vec::new(),
            pending_restore_dirty_contents: BTreeMap::new(),
            pending_restore_file_formats: BTreeMap::new(),
            pending_reconnect_restore_node_id: None,
            pending_reconnect_restore_files_remaining: 0,
            last_error: None,
            folder_picker: FolderPickerState::default(),
            folder_switch_confirm_open: false,
            tree_rows_cache: None,
            tree_scroll_handle: UniformListScrollHandle::new(),
            tab_scroll_handle: ScrollHandle::new(),
            search: ProjectSearchState::default(),
            quick_open: None,
            quick_open_focus: None,
            search_cache: HashMap::new(),
            search_cache_order: Vec::new(),
            pending_search_queries: HashMap::new(),
            pending_editor_reveals: BTreeMap::new(),
            tab_context_menu: None,
            tree_context_menu: None,
            tree_name_input: None,
            delete_confirm: None,
            tree_clipboard: None,
            tab_drag: None,
            file_format_menu: None,
            file_format_bounds: [None; 2],
            mount: IdeSurfaceMount::default(),
        }
    }

    pub fn load_state(&self) -> &IdeLoadState {
        &self.load_state
    }

    pub fn set_labels(&mut self, labels: IdeLabels, cx: &mut Context<Self>) {
        self.labels = labels;
        cx.notify();
    }

    pub fn mount(&self) -> IdeSurfaceMount {
        self.mount
    }

    pub fn set_mount(&mut self, mount: IdeSurfaceMount, cx: &mut Context<Self>) {
        if self.mount == mount {
            return;
        }
        #[cfg(not(target_family = "wasm"))]
        let was_visible = self.mount.is_visible();
        self.finish_tree_resize(cx);
        self.reset_quick_open();
        self.mount = mount;
        self.sync_tab_render_caches(cx);
        #[cfg(not(target_family = "wasm"))]
        if mount.is_visible() {
            if !was_visible {
                // Mount visibility controls only page sampling. User operations
                // and node ownership continue independently while hidden.
                self.resume_agent_sampling(cx);
            }
        } else {
            self.cancel_agent_sampling();
            self.stop_agent_watch(cx);
        }
    }

    pub fn set_visual_and_runtime_settings(
        &mut self,
        tokens: ThemeTokens,
        runtime_settings: IdeRuntimeSettings,
        cx: &mut Context<Self>,
    ) {
        #[cfg(not(target_family = "wasm"))]
        let (previous_agent_mode, runtime_settings) = {
            let mut settings = runtime_settings;
            if self.fs.is_bound() {
                settings.agent_mode = if self.fs.access() == oxideterm_ide_fs::RemoteAccess::Sftp {
                    NodeAgentMode::Disabled
                } else {
                    NodeAgentMode::Enabled
                };
            }
            (self.runtime_settings.agent_mode, settings)
        };
        self.tokens = tokens;
        self.runtime_settings = runtime_settings;
        #[cfg(not(target_family = "wasm"))]
        {
            let mode = self.runtime_settings.agent_mode;
            self.fs.set_mode(mode);
            if mode == NodeAgentMode::Disabled {
                self.cancel_agent_sampling();
                self.stop_agent_watch(cx);
            } else if previous_agent_mode == NodeAgentMode::Disabled && self.mount.is_visible() {
                self.resume_agent_sampling(cx);
            }
        }
        for editor in self.editors.values() {
            apply_editor_runtime_settings(editor, self.tokens, &self.runtime_settings, cx);
        }
        cx.notify();
    }

    pub fn snapshot(&mut self, cx: &mut Context<Self>) -> Option<WorkspaceSnapshot> {
        self.sync_all_editors(cx);
        self.workspace.snapshot().ok()
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn reconnect_snapshot(&mut self, cx: &mut Context<Self>) -> Option<ReconnectIdeSnapshot> {
        if self.fs.terminal_session_id().is_some() {
            return None;
        }
        self.sync_all_editors(cx);
        let snapshot = self.workspace.snapshot().ok()?;
        let (connection_id, project_path) = match &snapshot.project.root {
            IdeLocation::Remote { node_id, path } => (node_id.clone(), path.clone()),
            IdeLocation::Local { .. } => return None,
        };
        let tab_paths = snapshot
            .tabs
            .iter()
            .filter_map(|tab| match &tab.location {
                IdeLocation::Remote { path, .. } => Some(path.clone()),
                IdeLocation::Local { .. } => None,
            })
            .collect::<Vec<_>>();
        let dirty_contents = snapshot
            .buffers
            .iter()
            .filter(|buffer| {
                buffer.revision != buffer.saved_revision
                    || buffer.text != buffer.saved_text
                    || buffer.format != buffer.saved_format
            })
            .filter_map(|buffer| match &buffer.location {
                IdeLocation::Remote { path, .. } => Some((path.clone(), buffer.text.to_string())),
                IdeLocation::Local { .. } => None,
            })
            .collect::<BTreeMap<_, _>>();

        Some(ReconnectIdeSnapshot {
            project_path,
            tab_paths,
            connection_id,
            dirty_contents,
            file_formats: snapshot
                .buffers
                .iter()
                .filter_map(|buffer| {
                    let IdeLocation::Remote { path, .. } = &buffer.location else {
                        return None;
                    };
                    Some((
                        path.clone(),
                        oxideterm_ssh::ReconnectIdeFileFormat {
                            encoding: buffer.format.encoding.clone(),
                            has_bom: buffer.format.has_bom,
                            line_ending: buffer.format.line_ending.label().into(),
                        },
                    ))
                })
                .collect(),
        })
    }

    pub fn open_remote_project(
        &mut self,
        node_id: impl Into<String>,
        root_path: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let node_id = node_id.into();
        let root_path = root_path.into();
        self.reset_quick_open();
        #[cfg(not(target_family = "wasm"))]
        {
            self.cancel_agent_sampling();
            self.cancel_tree_transfer();
            if let Some(previous_node_id) = self.node_id.clone()
                && previous_node_id != node_id
            {
                self.stop_agent_watch(cx);
                self.release_ide_node_after_watch_stop(previous_node_id);
            } else if self.root_path.as_deref() != Some(root_path.as_str()) {
                self.stop_agent_watch(cx);
            }
        }
        if self.pending_restore_files.is_empty() {
            self.pending_restore_dirty_contents.clear();
            self.pending_restore_file_formats.clear();
        }
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.node_id = Some(node_id.clone());
        self.root_path = Some(root_path.clone());
        self.git_branch = None;
        self.load_state = IdeLoadState::Loading;
        self.tab_close_batch = None;
        self.last_error = None;
        self.conflict_state = None;
        self.loading_paths.clear();
        self.loading_file_tabs.clear();
        self.saving_tabs.clear();
        self.pending_saves.clear();
        self.file_open_errors.clear();
        self.tree_name_input = None;
        self.delete_confirm = None;
        self.tree_clipboard = None;
        #[cfg(not(target_family = "wasm"))]
        {
            self.desktop.agent_action = None;
        }
        self.editors.clear();
        self.markdown_previews.clear();
        self.workspace = IdeWorkspace::new();
        cx.notify();

        let fs = self.fs.clone();
        let jobs = self.file_jobs();
        let request = FileRequest::capture(
            self.pending_reconnect_restore_node_id
                .is_none()
                .then_some(FileOrigin::User),
        );
        cx.spawn(async move |weak, cx| {
            let result = jobs
                .run(async move {
                    request
                        .run(open_project_with_root_listing(fs, node_id, root_path))
                        .await
                })
                .await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(result) => this.apply_project_open(result, cx),
                    Err(error) => {
                        let message = error.message;
                        this.load_state = IdeLoadState::Error(message.clone());
                        if this.is_shell_ide() {
                            cx.emit(IdeSurfaceEvent::ShellOpenFailed);
                        }
                        if let Some(reconnect_node_id) =
                            this.pending_reconnect_restore_node_id.take()
                        {
                            cx.emit(IdeSurfaceEvent::ReconnectRestoreProjectFailed {
                                reconnect_node_id,
                                message,
                            });
                        }
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    pub fn open_remote_project_with_files(
        &mut self,
        node_id: impl Into<String>,
        root_path: impl Into<String>,
        file_paths: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        self.pending_restore_files = file_paths;
        self.pending_restore_dirty_contents.clear();
        self.pending_restore_file_formats.clear();
        self.open_remote_project(node_id, root_path, cx);
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn release_remote_session(&mut self, cx: &mut Context<Self>) {
        self.reset_quick_open();
        self.cancel_tree_transfer();
        self.cancel_backend_binding();
        self.cancel_agent_sampling();
        self.stop_agent_watch(cx);
        self.clear_search_cache();
        self.search.generation = self.search.generation.wrapping_add(1);
        self.search.searching = false;
        self.pending_search_queries.clear();
        self.pending_reconnect_restore_node_id = None;
        self.pending_reconnect_restore_files_remaining = 0;
        if let Some(node_id) = self.node_id.take() {
            self.release_ide_node_after_watch_stop(node_id);
        }
    }

    #[cfg(not(target_family = "wasm"))]
    fn release_ide_node_after_watch_stop(&mut self, node_id: String) {
        if self.desktop.agent_watch_stop_in_flight {
            // Keep the node consumer until watch/stop has used the existing
            // agent session; releasing first would turn cleanup into a no-op.
            if let Some(release_queue) = self.desktop.agent_watch_stop_release_queue.as_ref() {
                release_queue.request_release(&self.fs, node_id);
            } else {
                self.fs.release_ide_session_for_node(&node_id);
            }
        } else {
            self.fs.release_ide_session_for_node(&node_id);
        }
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn mark_connection_interrupted(&mut self, cx: &mut Context<Self>) {
        self.reset_quick_open();
        self.cancel_tree_transfer();
        self.cancel_agent_sampling();
        self.stop_agent_watch(cx);
        self.clear_search_cache();
        self.search.generation = self.search.generation.wrapping_add(1);
        self.search.searching = false;
        self.pending_search_queries.clear();
        self.pending_reconnect_restore_node_id = None;
        self.pending_reconnect_restore_files_remaining = 0;
        if let Some(node_id) = self.node_id.clone() {
            self.release_ide_node_after_watch_stop(node_id);
        }
        if matches!(self.load_state, IdeLoadState::Ready) {
            self.load_state = IdeLoadState::Disconnected;
            cx.notify();
        }
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn restore_reconnect_snapshot(
        &mut self,
        snapshot: ReconnectIdeSnapshot,
        reconnect_node_id: String,
        cx: &mut Context<Self>,
    ) -> bool {
        self.restore_reconnect_with_access(snapshot, reconnect_node_id, self.fs.access(), cx)
    }

    /// Pending pickers and failed opens do not represent an opened project.
    pub fn has_open_project(&self) -> bool {
        self.root_path.is_some()
            && matches!(
                self.load_state,
                IdeLoadState::Ready | IdeLoadState::Disconnected
            )
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn remote_access(&self) -> oxideterm_ide_fs::RemoteAccess {
        self.fs.access()
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn restore_reconnect_with_access(
        &mut self,
        snapshot: ReconnectIdeSnapshot,
        reconnect_node_id: String,
        access: oxideterm_ide_fs::RemoteAccess,
        cx: &mut Context<Self>,
    ) -> bool {
        if access == oxideterm_ide_fs::RemoteAccess::Automatic {
            return self.apply_reconnect_snapshot(snapshot, reconnect_node_id, cx);
        }
        self.load_state = IdeLoadState::Loading;
        let fs = self.fs.clone();
        let runtime = self.backend_runtime.clone();
        let node = snapshot.connection_id.clone();
        self.cancel_backend_binding();
        let generation = self.generation;
        let progress = self.track_agent_install(access, cx);
        let job = runtime.spawn(async move { fs.bind_node(&node, access, progress).await });
        self.desktop.backend_binding_abort = Some(job.abort_handle());
        cx.spawn(async move |weak, cx| {
            let result = await_ide_backend(job).await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.desktop.backend_binding_abort = None;
                this.desktop.agent_install_task = None;
                this.desktop.agent_install_progress = None;
                match result {
                    Ok(fs) => {
                        this.runtime_settings.agent_mode =
                            if fs.access() == oxideterm_ide_fs::RemoteAccess::Sftp {
                                NodeAgentMode::Disabled
                            } else {
                                NodeAgentMode::Enabled
                            };
                        this.fs = fs;
                        this.apply_reconnect_snapshot(snapshot, reconnect_node_id, cx);
                    }
                    Err(error) => {
                        this.load_state = IdeLoadState::Error(error.message.clone());
                        cx.emit(IdeSurfaceEvent::ReconnectRestoreProjectFailed {
                            reconnect_node_id,
                            message: error.message,
                        });
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        cx.notify();
        true
    }

    #[cfg(not(target_family = "wasm"))]
    fn apply_reconnect_snapshot(
        &mut self,
        snapshot: ReconnectIdeSnapshot,
        reconnect_node_id: String,
        cx: &mut Context<Self>,
    ) -> bool {
        self.sync_all_editors(cx);
        let same_project_open = self.root_path.as_deref() == Some(snapshot.project_path.as_str())
            && self.node_id.as_deref() == Some(snapshot.connection_id.as_str());

        if self.root_path.is_some() && !same_project_open {
            return false;
        }

        self.pending_restore_dirty_contents = snapshot.dirty_contents;
        self.pending_restore_file_formats = snapshot
            .file_formats
            .into_iter()
            .filter_map(|(path, format)| {
                let line_ending = match format.line_ending.as_str() {
                    "LF" => LineEnding::Lf,
                    "CRLF" => LineEnding::CrLf,
                    "CR" => LineEnding::Cr,
                    _ => return None,
                };
                Some((
                    path,
                    TextFileFormat {
                        encoding: format.encoding,
                        has_bom: format.has_bom,
                        line_ending,
                    },
                ))
            })
            .collect();
        if same_project_open {
            self.load_state = IdeLoadState::Ready;
            self.last_error = None;
            self.resume_agent_sampling(cx);
            for path in snapshot.tab_paths {
                self.open_remote_file(
                    IdeLocation::remote(snapshot.connection_id.clone(), path),
                    None,
                    cx,
                );
            }
            cx.notify();
        } else {
            self.pending_reconnect_restore_node_id = Some(reconnect_node_id);
            self.pending_restore_files = snapshot.tab_paths;
            self.open_remote_project(snapshot.connection_id, snapshot.project_path, cx);
        }
        true
    }

    pub fn open_remote_folder_picker_for_node(
        &mut self,
        node_id: impl Into<String>,
        initial_path: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let node_id = node_id.into();
        let initial_path = normalize_remote_path(&initial_path.into());
        self.node_id = Some(node_id.clone());
        self.folder_picker.open = true;
        self.folder_picker.node_id = Some(node_id.clone());
        self.folder_picker.path_input_focused = true;
        self.load_folder_picker_path(node_id, initial_path, cx);
    }
}

#[cfg(not(target_family = "wasm"))]
impl Drop for IdeSurface {
    fn drop(&mut self) {
        self.cancel_tree_transfer();
        self.cancel_backend_binding();
        if let Some(abort_handle) = self.desktop.agent_sampling_backend_abort.take() {
            abort_handle.abort();
        }
        if let Some(abort_handle) = self.desktop.agent_watch_backend_abort.take() {
            abort_handle.abort();
        }
        // GPUI can drop an IDE surface during workspace teardown without a
        // `Context`. Release only this surface's node because the file-system
        // registry is shared with other IDE surfaces and AI consumers.
        if let Some(node_id) = self.node_id.take() {
            if let Some(release_queue) = self.desktop.agent_watch_stop_release_queue.as_ref() {
                // The detached Tokio stop task owns the final remote cleanup
                // and consumer release even when no GPUI update can run.
                release_queue.request_release(&self.fs, node_id);
            } else {
                self.fs.release_ide_session_for_node(&node_id);
            }
        }
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod lifecycle_tests {
    use super::*;
    use gpui::TestAppContext;
    use oxideterm_ssh::{
        ConnectionConsumer, ConnectionState, NodeId, NodeRouter, SshConfig, SshConnectionHandle,
        SshConnectionRegistry,
    };
    use oxideterm_theme::default_tokens;

    fn bind_active_node(
        registry: &SshConnectionRegistry,
        router: &NodeRouter,
        node_id: &str,
        host: &str,
    ) -> (NodeId, SshConnectionHandle) {
        let node_id = NodeId::new(node_id);
        let config = SshConfig::password(host, 22, "ide-user", "pw");
        router.upsert_node(node_id.clone(), config.clone());
        let handle = registry.acquire(config, ConnectionConsumer::NodeRouter(node_id.0.clone()));
        handle.set_physical(Arc::new(()));
        registry.mark_state(handle.connection_id(), ConnectionState::Active);
        router
            .bind_connection(&node_id, handle.connection_id().to_string())
            .expect("bind active IDE test node");
        (node_id, handle)
    }

    fn has_ide_consumer(handle: &SshConnectionHandle, node_id: &str) -> bool {
        ide_consumer_count(handle, node_id) > 0
    }

    fn ide_consumer_count(handle: &SshConnectionHandle, node_id: &str) -> usize {
        let session_prefix = format!("{node_id}:");
        handle
            .info()
            .consumers
            .iter()
            .filter(|consumer| {
                matches!(
                    consumer,
                    ConnectionConsumer::Ide(consumer_id)
                        if consumer_id == node_id || consumer_id.starts_with(&session_prefix)
                )
            })
            .count()
    }

    fn test_surface(cx: &mut TestAppContext) -> Entity<IdeSurface> {
        let router = NodeRouter::new(SshConnectionRegistry::default());
        let fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Ask);
        let backend_runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build IDE visibility test runtime"),
        );
        cx.new(move |cx| {
            IdeSurface::new(
                fs,
                default_tokens(),
                IdeLabels::default(),
                IdeRuntimeSettings::default(),
                backend_runtime,
                cx,
            )
        })
    }

    #[gpui::test]
    fn agent_install_updates_show_received_bytes_and_cancel_with_the_binding(
        cx: &mut TestAppContext,
    ) {
        use oxideterm_ide_fs::{AgentInstallProgress, RemoteAccess};
        let surface = test_surface(cx);
        let sender = surface.update(cx, |surface, cx| {
            surface.load_state = IdeLoadState::Loading;
            surface
                .track_agent_install(RemoteAccess::Agent, cx)
                .unwrap()
        });
        for (state, text, percent) in [
            (AgentInstallProgress::Checking, "Checking agent...", None),
            (
                AgentInstallProgress::Uploading {
                    transferred: 1024,
                    total: 2048,
                },
                "Installing agent: 50% (1 / 2 KiB)",
                Some(50.0),
            ),
            (AgentInstallProgress::Starting, "Starting agent…", None),
        ] {
            sender.send_replace(state);
            cx.run_until_parked();
            let shown = surface.update(cx, |surface, _| {
                surface
                    .labels
                    .agent_install_status(surface.desktop.agent_install_progress.unwrap())
            });
            assert_eq!(shown, (text.into(), percent));
        }
        surface.update(cx, |surface, _| surface.cancel_backend_binding());
        sender.send_replace(AgentInstallProgress::Starting);
        cx.run_until_parked();
        surface.update(cx, |surface, _| {
            assert!(surface.desktop.agent_install_progress.is_none())
        });
    }
    #[gpui::test]
    fn releasing_surface_cancels_started_backend_preparation(cx: &mut TestAppContext) {
        let surface = test_surface(cx);
        let runtime = surface.update(cx, |surface, _| surface.backend_runtime.clone());
        let (started, ready) = tokio::sync::oneshot::channel();
        let job = runtime.spawn(async move {
            let _ = started.send(());
            std::future::pending::<()>().await;
        });
        runtime.block_on(ready).unwrap();
        surface.update(cx, |surface, cx| {
            surface.desktop.backend_binding_abort = Some(job.abort_handle());
            surface.release_remote_session(cx);
        });
        assert!(runtime.block_on(job).unwrap_err().is_cancelled());
    }

    fn configure_ready_surface(surface: &mut IdeSurface, cx: &mut Context<IdeSurface>) {
        surface.node_id = Some("visibility-node".to_string());
        surface.root_path = Some("/srv/app".to_string());
        surface.load_state = IdeLoadState::Ready;
        surface.set_mount(IdeSurfaceMount::MainWindow, cx);
    }

    #[gpui::test]
    fn hidden_mount_stops_sampling_watch_and_watch_reads_without_releasing_node(
        cx: &mut TestAppContext,
    ) {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node_id = "visibility-hidden-node";
        let (_node_id, handle) = bind_active_node(&registry, &router, node_id, "hidden-host");
        let fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Ask);
        let backend_runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build IDE hidden visibility test runtime"),
        );
        let surface = cx.new({
            let fs = fs;
            let backend_runtime = backend_runtime.clone();
            move |cx| {
                IdeSurface::new(
                    fs,
                    default_tokens(),
                    IdeLabels::default(),
                    IdeRuntimeSettings::default(),
                    backend_runtime,
                    cx,
                )
            }
        });
        let surface_fs = surface.read_with(cx, |surface, _cx| surface.fs.clone());
        backend_runtime.block_on(async {
            let _ = surface_fs.deploy_agent_for_node(node_id).await;
        });
        assert_eq!(ide_consumer_count(&handle, node_id), 1);

        surface.update(cx, |surface, cx| {
            surface.node_id = Some(node_id.to_string());
            surface.root_path = Some("/srv/app".to_string());
            surface.load_state = IdeLoadState::Ready;
            surface.set_mount(IdeSurfaceMount::MainWindow, cx);
            surface.schedule_next_agent_status_poll(cx);
            surface.desktop.agent_watch_task = Some(cx.spawn(async move |_weak, _cx| {
                std::future::pending::<()>().await;
            }));
            surface.schedule_agent_watch_retry(cx);
            surface.desktop.watched_root_path = Some("/srv/app".to_string());

            surface.set_mount(IdeSurfaceMount::Hidden, cx);
            surface.refresh_tree_for_watch_path("/srv/app/src/main.rs".to_string(), cx);

            assert!(surface.desktop.agent_poll_task.is_none());
            assert!(surface.desktop.agent_sampling_refresh_task.is_none());
            assert!(surface.desktop.agent_watch_task.is_none());
            assert!(surface.desktop.agent_watch_retry_task.is_none());
            assert!(surface.loading_paths.is_empty());
        });
        assert_eq!(ide_consumer_count(&handle, node_id), 1);
    }

    #[gpui::test]
    fn detached_window_mount_resumes_sampling(cx: &mut TestAppContext) {
        let surface = test_surface(cx);

        surface.update(cx, |surface, cx| {
            configure_ready_surface(surface, cx);
            surface.set_mount(IdeSurfaceMount::Hidden, cx);
            assert!(surface.desktop.agent_sampling_refresh_task.is_none());

            surface.set_mount(IdeSurfaceMount::DetachedWindow, cx);

            assert_eq!(surface.mount(), IdeSurfaceMount::DetachedWindow);
            assert!(surface.desktop.agent_sampling_refresh_task.is_some());
            assert_eq!(
                surface.desktop.agent_refresh_origin,
                Some(AgentStatusRefreshOrigin::VisibilitySampling)
            );
        });
    }

    #[gpui::test]
    fn switching_between_visible_mounts_does_not_restart_sampling(cx: &mut TestAppContext) {
        let surface = test_surface(cx);

        surface.update(cx, |surface, cx| {
            configure_ready_surface(surface, cx);
            surface.cancel_agent_sampling();
            surface.schedule_next_agent_status_poll(cx);
            let poll_generation = surface.desktop.agent_poll_generation;

            surface.set_mount(IdeSurfaceMount::DetachedWindow, cx);

            assert_eq!(surface.desktop.agent_poll_generation, poll_generation);
            assert!(surface.desktop.agent_poll_task.is_some());
            assert!(surface.desktop.agent_sampling_refresh_task.is_none());
        });
    }

    #[gpui::test]
    fn hidden_surface_keeps_user_agent_refresh_completion_owned(cx: &mut TestAppContext) {
        let surface = test_surface(cx);

        surface.update(cx, |surface, cx| {
            configure_ready_surface(surface, cx);
            surface.set_mount(IdeSurfaceMount::Hidden, cx);

            surface.refresh_agent_status(AgentStatusRefreshOrigin::UserAction, cx);

            assert_eq!(surface.desktop.agent_action, Some(AgentActionKind::Refresh));
            assert_eq!(
                surface.desktop.agent_refresh_origin,
                Some(AgentStatusRefreshOrigin::UserAction)
            );
            assert!(surface.desktop.agent_poll_task.is_none());
            assert!(surface.desktop.agent_sampling_refresh_task.is_none());
        });
    }

    #[gpui::test]
    fn hidden_surface_applies_project_and_disconnect_lifecycle_without_sampling(
        cx: &mut TestAppContext,
    ) {
        let surface = test_surface(cx);

        surface.update(cx, |surface, cx| {
            surface.set_mount(IdeSurfaceMount::Hidden, cx);
            surface.apply_project_open(
                ProjectOpenResult {
                    node_id: "visibility-node".to_string(),
                    root: IdeLocation::remote("visibility-node", "/srv/app"),
                    title: "app".to_string(),
                    git_branch: None,
                    children: Vec::new(),
                },
                cx,
            );

            assert_eq!(surface.load_state, IdeLoadState::Ready);
            assert!(surface.desktop.agent_poll_task.is_none());
            assert!(surface.desktop.agent_sampling_refresh_task.is_none());
            assert!(surface.desktop.agent_watch_task.is_none());

            surface.mark_connection_interrupted(cx);

            assert_eq!(surface.load_state, IdeLoadState::Disconnected);
            assert_eq!(surface.mount(), IdeSurfaceMount::Hidden);
            assert!(surface.desktop.agent_poll_task.is_none());
            assert!(surface.desktop.agent_watch_task.is_none());
        });
    }

    #[gpui::test]
    fn releasing_one_surface_preserves_other_node_consumer(cx: &mut TestAppContext) {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let (first_node, first_handle) =
            bind_active_node(&registry, &router, "surface-node-first", "first-host");
        let (second_node, second_handle) =
            bind_active_node(&registry, &router, "surface-node-second", "second-host");
        let fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled);
        let backend_runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build IDE test runtime"),
        );

        let first_surface = cx.new({
            let fs = fs.clone();
            let backend_runtime = backend_runtime.clone();
            let first_node_id = first_node.0.clone();
            move |cx| {
                let mut surface = IdeSurface::new(
                    fs,
                    default_tokens(),
                    IdeLabels::default(),
                    IdeRuntimeSettings::default(),
                    backend_runtime,
                    cx,
                );
                surface.node_id = Some(first_node_id);
                surface
            }
        });
        let second_surface = cx.new({
            let fs = fs;
            let backend_runtime = backend_runtime.clone();
            let second_node_id = second_node.0.clone();
            move |cx| {
                let mut surface = IdeSurface::new(
                    fs,
                    default_tokens(),
                    IdeLabels::default(),
                    IdeRuntimeSettings::default(),
                    backend_runtime,
                    cx,
                );
                surface.node_id = Some(second_node_id);
                surface
            }
        });
        let first_surface_fs = first_surface.read_with(cx, |surface, _cx| surface.fs.clone());
        let second_surface_fs = second_surface.read_with(cx, |surface, _cx| surface.fs.clone());
        // Each surface-owned scope acquires only its node lease before the fake
        // SSH transport rejects the agent probe.
        backend_runtime.block_on(async {
            let _ = first_surface_fs
                .deploy_agent_for_node(first_node.0.clone())
                .await;
            let _ = second_surface_fs
                .deploy_agent_for_node(second_node.0.clone())
                .await;
        });
        assert!(has_ide_consumer(&first_handle, &first_node.0));
        assert!(has_ide_consumer(&second_handle, &second_node.0));

        first_surface.update(cx, |surface, cx| surface.release_remote_session(cx));

        assert!(!has_ide_consumer(&first_handle, &first_node.0));
        assert!(has_ide_consumer(&second_handle, &second_node.0));
    }

    #[gpui::test]
    fn stopping_agent_watch_cancels_tasks_and_orders_same_path_restart(cx: &mut TestAppContext) {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry);
        let fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled);
        let backend_runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build IDE test runtime"),
        );
        let surface = cx.new(move |cx| {
            IdeSurface::new(
                fs,
                default_tokens(),
                IdeLabels::default(),
                IdeRuntimeSettings::default(),
                backend_runtime,
                cx,
            )
        });

        surface.update(cx, |surface, cx| {
            surface.set_mount(IdeSurfaceMount::MainWindow, cx);
            surface.desktop.agent_watch_task = Some(cx.spawn(async move |_weak, _cx| {
                std::future::pending::<()>().await;
            }));
            surface.schedule_agent_watch_retry(cx);
            surface.desktop.watched_root_path = Some("/srv/app".to_string());
            surface.root_path = Some("/srv/app".to_string());
            surface.node_id = Some("node-watch".to_string());
            assert!(surface.desktop.agent_watch_task.is_some());
            assert!(surface.desktop.agent_watch_retry_task.is_some());

            surface.stop_agent_watch(cx);

            assert!(surface.desktop.agent_watch_task.is_none());
            assert!(surface.desktop.agent_watch_retry_task.is_none());
            assert!(surface.desktop.watched_root_path.is_none());
            assert!(surface.desktop.agent_watch_stop_task.is_some());
            assert!(surface.desktop.agent_watch_stop_in_flight);
            surface.release_ide_node_after_watch_stop("node-watch".to_string());
            assert!(surface.desktop.agent_watch_stop_release_queue.is_some());

            surface.start_agent_watch_if_ready(cx);

            assert!(surface.desktop.agent_watch_restart_requested);
            assert!(surface.desktop.agent_watch_task.is_none());
        });
    }

    #[gpui::test]
    fn releasing_surface_preserves_same_node_ai_owner(cx: &mut TestAppContext) {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let shared_node_id = "surface-ai-shared";
        let (_node_id, handle) =
            bind_active_node(&registry, &router, shared_node_id, "shared-host");
        let ai_fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled);
        let backend_runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build IDE test runtime"),
        );
        let surface = cx.new({
            let ai_fs = ai_fs.clone();
            let backend_runtime = backend_runtime.clone();
            let shared_node_id = shared_node_id.to_string();
            move |cx| {
                let mut surface = IdeSurface::new(
                    ai_fs,
                    default_tokens(),
                    IdeLabels::default(),
                    IdeRuntimeSettings::default(),
                    backend_runtime,
                    cx,
                );
                surface.node_id = Some(shared_node_id);
                surface
            }
        });
        let surface_fs = surface.read_with(cx, |surface, _cx| surface.fs.clone());

        backend_runtime.block_on(async {
            let _ = ai_fs.deploy_agent_for_node(shared_node_id).await;
            let _ = surface_fs.deploy_agent_for_node(shared_node_id).await;
        });
        assert_eq!(ide_consumer_count(&handle, shared_node_id), 2);

        surface.update(cx, |surface, cx| surface.release_remote_session(cx));

        assert_eq!(ide_consumer_count(&handle, shared_node_id), 1);
        ai_fs.release_ide_session_for_node(shared_node_id);
        assert_eq!(ide_consumer_count(&handle, shared_node_id), 0);
    }

    #[test]
    fn background_stop_completion_releases_owner_without_gpui_update() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node_id = "surface-drop-during-stop";
        let (_node_id, handle) = bind_active_node(&registry, &router, node_id, "drop-host");
        let fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled);
        let backend_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build IDE test runtime");
        backend_runtime.block_on(async {
            let _ = fs.deploy_agent_for_node(node_id).await;
        });
        assert_eq!(ide_consumer_count(&handle, node_id), 1);

        let release_queue = IdeWatchStopReleaseQueue::default();
        release_queue.request_release(&fs, node_id.to_string());
        assert_eq!(ide_consumer_count(&handle, node_id), 1);

        // This is the backend completion path used when the GPUI weak update
        // cannot run because the surface was released during watch/stop.
        release_queue.finish_stop(&fs);
        assert_eq!(ide_consumer_count(&handle, node_id), 0);
    }

    fn open_close_test_tabs<const N: usize>(
        surface: &mut IdeSurface,
        paths: [&str; N],
    ) -> [EditorTabId; N] {
        surface.node_id = Some("test".into());
        surface.root_path = Some("/work".into());
        surface.load_state = IdeLoadState::Ready;
        surface
            .workspace
            .open_project(IdeLocation::remote("test", "/work"), "work");
        paths.map(|path| {
            surface
                .workspace
                .open_file(
                    IdeLocation::remote("test", path),
                    "original",
                    SavedFileVersion::default(),
                )
                .unwrap();
            surface.workspace.active_tab().unwrap()
        })
    }

    #[gpui::test]
    fn file_tab_batch_close_preserves_pins_and_the_context_tab(cx: &mut TestAppContext) {
        let surface = test_surface(cx);
        let mut events = cx.events(&surface);
        surface.update(cx, |surface, cx| {
            let [pinned, keep, _active] =
                open_close_test_tabs(surface, ["/work/pinned", "/work/keep", "/work/active"]);
            surface.workspace.toggle_tab_pin(pinned).unwrap();
            assert!(surface.can_close_file_tabs(Some(keep)));
            surface.close_file_tabs(Some(keep), cx);
            assert_eq!(surface.open_file_paths(), ["/work/pinned", "/work/keep"]);
            assert!(!surface.can_close_file_tabs(Some(keep)));

            surface.close_file_tabs(None, cx);
            assert_eq!(surface.open_file_paths(), ["/work/pinned"]);
            assert!(!surface.can_close_file_tabs(None));
            surface.workspace.toggle_tab_pin(pinned).unwrap();
            surface.close_file_tabs(None, cx);
            assert!(surface.open_file_paths().is_empty());
            assert_eq!(
                surface.workspace.project_root(),
                Some(&IdeLocation::remote("test", "/work"))
            );
        });
        while let Ok(event) = events.try_recv() {
            assert_ne!(event, IdeSurfaceEvent::ShellReturnReady);
        }
    }

    #[gpui::test]
    fn file_tab_batch_close_prompts_in_order_and_stops_on_cancel(cx: &mut TestAppContext) {
        let surface = test_surface(cx);
        surface.update(cx, |surface, cx| {
            let [first, second, _later] =
                open_close_test_tabs(surface, ["/work/first", "/work/second", "/work/later"]);
            for (tab, text) in [(first, "edit one"), (second, "edit two")] {
                surface.workspace.replace_buffer_text(tab, text).unwrap();
            }

            surface.close_file_tabs(None, cx);
            assert_eq!(surface.workspace.pending_close().unwrap().tab_id, first);
            surface.resolve_dirty_close(DirtyCloseDecision::Discard, cx);
            assert_eq!(surface.workspace.pending_close().unwrap().tab_id, second);
            surface.load_state = IdeLoadState::Disconnected;
            surface.resolve_dirty_close(DirtyCloseDecision::Save, cx);
            assert_eq!(surface.workspace.pending_close().unwrap().tab_id, second);
            surface.resolve_dirty_close(DirtyCloseDecision::Cancel, cx);
            assert!(surface.workspace.pending_close().is_none());
            assert_eq!(surface.open_file_paths(), ["/work/second", "/work/later"]);
            assert_eq!(
                surface.workspace.buffer(second).unwrap().text.to_string(),
                "edit two"
            );
        });
    }

    #[gpui::test]
    fn shell_return_cancel_keeps_dirty_text_and_discard_closes_it(cx: &mut TestAppContext) {
        let surface = test_surface(cx);
        surface.update(cx, |surface, cx| {
            surface
                .workspace
                .open_project(IdeLocation::remote("shell:test", "/work"), "container");
            surface
                .workspace
                .open_file(
                    IdeLocation::remote("shell:test", "/work/config"),
                    "original",
                    SavedFileVersion::default(),
                )
                .unwrap();
            let tab = surface.workspace.tabs()[0].id;
            surface
                .workspace
                .replace_buffer_text(tab, "unsaved container edit")
                .unwrap();
            let (cancel, receiver) = tokio::sync::watch::channel(false);
            surface.desktop.transfer = Some(TreeTransfer {
                label: surface.labels.upload.clone(),
                percent: 50,
                cancel,
            });
            surface.tab_close_batch = Some(TabCloseBatch::ReturnToTerminal);
            surface.cancel_tree_transfer();
            surface.advance_tab_close_batch(cx);
            assert!(*receiver.borrow());
            assert!(surface.workspace.pending_close().is_none());
            surface.complete_tree_transfer(
                surface.generation,
                None,
                Err(IdeFileError::new(
                    IdeFileErrorKind::Cancelled,
                    "transfer cancelled",
                )),
                cx,
            );
            assert!(surface.last_error.is_none());
            assert_eq!(surface.workspace.pending_close().unwrap().tab_id, tab);
            surface.resolve_dirty_close(DirtyCloseDecision::Cancel, cx);
            surface.advance_tab_close_batch(cx);
            assert_eq!(
                surface.workspace.buffer(tab).unwrap().text.to_string(),
                "unsaved container edit"
            );
            assert!(!surface.shell_return_requested());
            let (cancel, _receiver) = tokio::sync::watch::channel(false);
            surface.desktop.transfer = Some(TreeTransfer {
                label: surface.labels.download.clone(),
                percent: 90,
                cancel,
            });
            surface.tab_close_batch = Some(TabCloseBatch::ReturnToTerminal);
            surface.cancel_tree_transfer();
            surface.complete_tree_transfer(
                surface.generation,
                None,
                Err(IdeFileError::new(
                    IdeFileErrorKind::PermissionDenied,
                    "cannot install downloaded file",
                )),
                cx,
            );
            assert_eq!(
                surface.last_error.as_deref(),
                Some("cannot install downloaded file")
            );
            surface.resolve_dirty_close(DirtyCloseDecision::Discard, cx);
            surface.advance_tab_close_batch(cx);
            assert!(surface.workspace.tabs().is_empty());
            assert!(!surface.shell_return_requested());
        });
    }
}

impl IdeSurface {
    pub fn is_shell_ide(&self) -> bool {
        #[cfg(not(target_family = "wasm"))]
        {
            self.fs.terminal_session_id().is_some()
        }
        #[cfg(target_family = "wasm")]
        {
            false
        }
    }

    pub fn request_shell_return(&mut self, cx: &mut Context<Self>) {
        if !self.is_shell_ide() {
            return;
        }
        self.tab_close_batch = Some(TabCloseBatch::ReturnToTerminal);
        self.reset_quick_open();
        #[cfg(not(target_family = "wasm"))]
        self.cancel_tree_transfer();
        self.advance_tab_close_batch(cx);
        cx.notify();
    }

    fn shell_return_requested(&self) -> bool {
        matches!(self.tab_close_batch, Some(TabCloseBatch::ReturnToTerminal))
    }

    fn file_tabs_to_close(&self, keep: Option<EditorTabId>) -> impl Iterator<Item = EditorTabId> {
        self.workspace
            .tabs()
            .iter()
            .filter(move |tab| !tab.is_pinned && Some(tab.id) != keep)
            .map(|tab| tab.id)
    }

    fn can_close_file_tabs(&self, keep: Option<EditorTabId>) -> bool {
        self.tab_close_batch.is_none()
            && self.workspace.pending_close().is_none()
            && keep.is_none_or(|tab| self.workspace.buffer(tab).is_some())
            && self.file_tabs_to_close(keep).next().is_some()
    }

    fn close_file_tabs(&mut self, keep: Option<EditorTabId>, cx: &mut Context<Self>) {
        if self.can_close_file_tabs(keep) {
            // A file opened while a save or confirmation is pending belongs to a later action.
            let targets = self.file_tabs_to_close(keep).collect();
            self.tab_close_batch = Some(TabCloseBatch::Files(targets));
            self.advance_tab_close_batch(cx);
        }
        cx.notify();
    }

    fn advance_tab_close_batch(&mut self, cx: &mut Context<Self>) {
        while self.workspace.pending_close().is_none() {
            let next = match &self.tab_close_batch {
                None => return,
                Some(TabCloseBatch::Files(targets)) => self
                    .file_tabs_to_close(None)
                    .find(|tab| targets.contains(tab)),
                Some(TabCloseBatch::ReturnToTerminal) => {
                    #[cfg(not(target_family = "wasm"))]
                    if self.desktop.transfer.is_some() {
                        return;
                    }
                    if !self.saving_tabs.is_empty() {
                        return;
                    }
                    self.workspace.tabs().first().map(|tab| tab.id)
                }
            };
            let Some(tab) = next else {
                if let Some(TabCloseBatch::ReturnToTerminal) = self.tab_close_batch.take() {
                    cx.emit(IdeSurfaceEvent::ShellReturnReady);
                }
                return;
            };
            if self.saving_tabs.contains(&tab) || !self.close_tab(tab, cx) {
                return;
            }
        }
    }
}

#[cfg(not(target_family = "wasm"))]
impl IdeSurface {
    fn resume_backend_selection(&mut self, cx: &mut Context<Self>) {
        if self.desktop.access_request.is_some() {
            self.load_state = IdeLoadState::Empty;
            cx.notify();
        }
    }

    pub fn request_backend_selection(
        &mut self,
        node: String,
        path: String,
        cx: &mut Context<Self>,
    ) {
        self.node_id = Some(node.clone());
        self.desktop.access_request = Some((node, path));
        cx.notify();
    }

    fn select_ssh_access(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let generation = self.generation;
        let prompt = oxideterm_gpui_ui::choice_dialog::choice_dialog(
            self.tokens,
            oxideterm_gpui_ui::choice_dialog::ChoiceDialog {
                title: self.labels.access_modes[1].clone(),
                description: Some(self.labels.agent_choice_hint.clone()),
                choices: self.labels.agent_choices.clone(),
                cancel: self.labels.cancel.clone(),
                confirm: self.labels.confirm.clone(),
            },
            window,
            cx,
        );
        self.load_state = IdeLoadState::Loading;
        cx.spawn(async move |weak, cx| {
            let choice = prompt.await.unwrap_or(2);
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation || this.desktop.access_request.is_none() {
                    return;
                }
                match choice {
                    0 => this.select_access(oxideterm_ide_fs::RemoteAccess::Commands, cx),
                    1 => this.select_access(oxideterm_ide_fs::RemoteAccess::Agent, cx),
                    _ => this.resume_backend_selection(cx),
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub fn select_access(
        &mut self,
        access: oxideterm_ide_fs::RemoteAccess,
        cx: &mut Context<Self>,
    ) {
        let Some((node, path)) = self.desktop.access_request.clone() else {
            return;
        };
        self.load_state = IdeLoadState::Loading;
        let fs = self.fs.clone();
        let runtime = self.backend_runtime.clone();
        self.cancel_backend_binding();
        let generation = self.generation;
        let node_for_job = node.clone();
        let progress = self.track_agent_install(access, cx);
        let job = runtime.spawn(async move { fs.bind_node(&node_for_job, access, progress).await });
        self.desktop.backend_binding_abort = Some(job.abort_handle());
        cx.spawn(async move |weak, cx| {
            let result = await_ide_backend(job).await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.desktop.backend_binding_abort = None;
                this.desktop.agent_install_task = None;
                this.desktop.agent_install_progress = None;
                match result {
                    Ok(fs) => {
                        this.runtime_settings.agent_mode =
                            if fs.access() == oxideterm_ide_fs::RemoteAccess::Sftp {
                                NodeAgentMode::Disabled
                            } else {
                                NodeAgentMode::Enabled
                            };
                        this.fs = fs;
                        this.desktop.access_request = None;
                        this.load_state = IdeLoadState::Empty;
                        this.open_remote_folder_picker_for_node(node, path, cx);
                    }
                    Err(error) => {
                        this.load_state = IdeLoadState::Empty;
                        this.last_error = Some(if access == oxideterm_ide_fs::RemoteAccess::Sftp {
                            this.labels.sftp_unavailable.clone()
                        } else if error.kind == IdeFileErrorKind::Unsupported {
                            this.labels.backend_requirements.clone()
                        } else {
                            this.labels.ssh_unavailable.clone()
                        });
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }
}

#[cfg(not(target_family = "wasm"))]
impl IdeSurface {
    fn track_agent_install(
        &mut self,
        access: oxideterm_ide_fs::RemoteAccess,
        cx: &mut Context<Self>,
    ) -> Option<tokio::sync::watch::Sender<oxideterm_ide_fs::AgentInstallProgress>> {
        if access != oxideterm_ide_fs::RemoteAccess::Agent {
            return None;
        }
        let initial = oxideterm_ide_fs::AgentInstallProgress::Checking;
        let (sender, mut updates) = tokio::sync::watch::channel(initial);
        self.desktop.agent_install_progress = Some(initial);
        let generation = self.generation;
        // The binding owner cancels this observer together with its startup job.
        self.desktop.agent_install_task = Some(cx.spawn(async move |weak, cx| {
            while updates.changed().await.is_ok() {
                let progress = *updates.borrow_and_update();
                let Ok(current) = weak.update(cx, |this, cx| {
                    if this.generation != generation {
                        return false;
                    }
                    this.desktop.agent_install_progress = Some(progress);
                    cx.notify();
                    true
                }) else {
                    break;
                };
                if !current {
                    break;
                }
            }
        }));
        Some(sender)
    }
    fn cancel_backend_binding(&mut self) {
        self.desktop.agent_install_task = None;
        self.desktop.agent_install_progress = None;
        self.generation = self.generation.wrapping_add(1);
        if let Some(abort) = self.desktop.backend_binding_abort.take() {
            abort.abort();
        }
    }
}
