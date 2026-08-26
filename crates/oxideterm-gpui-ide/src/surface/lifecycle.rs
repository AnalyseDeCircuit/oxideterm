impl IdeSurface {
    pub fn new(
        fs: NodeAgentIdeFileSystem,
        tokens: ThemeTokens,
        labels: IdeLabels,
        runtime_settings: IdeRuntimeSettings,
        backend_runtime: Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) -> Self {
        // A surface owns an independent client scope. Its node/session cleanup
        // must not release same-node consumers retained by AI tools or another
        // IDE surface, while all scopes still share the NodeRouter transport.
        let fs = fs.scoped_owner();
        Self {
            workspace: IdeWorkspace::new(),
            fs,
            tokens,
            labels,
            runtime_settings,
            focus_handle: cx.focus_handle(),
            backend_runtime,
            load_state: IdeLoadState::Empty,
            node_id: None,
            root_path: None,
            git_branch: None,
            tree_width: IDE_TREE_DEFAULT_WIDTH,
            generation: 0,
            editors: HashMap::new(),
            loading_paths: HashSet::new(),
            loading_file_tabs: HashSet::new(),
            saving_tabs: HashSet::new(),
            save_after_close: None,
            conflict_state: None,
            pending_restore_files: Vec::new(),
            pending_restore_dirty_contents: BTreeMap::new(),
            pending_reconnect_restore_node_id: None,
            pending_reconnect_restore_files_remaining: 0,
            last_error: None,
            folder_picker: FolderPickerState::default(),
            folder_switch_confirm_open: false,
            tree_rows_cache: None,
            tree_scroll_handle: UniformListScrollHandle::new(),
            tab_scroll_handle: ScrollHandle::new(),
            search: ProjectSearchState::default(),
            editor_search: EditorSearchState::default(),
            search_cache: HashMap::new(),
            search_cache_order: Vec::new(),
            pending_search_queries: BTreeMap::new(),
            pending_editor_reveals: BTreeMap::new(),
            tab_context_menu: None,
            tree_context_menu: None,
            tree_name_input: None,
            delete_confirm: None,
            tree_clipboard: None,
            tab_drag: None,
            agent_opt_in_open: false,
            agent_opt_in_remember: false,
            agent_status_menu: None,
            agent_status_trigger_bounds: None,
            agent_remove_confirm_open: false,
            agent_action: None,
            agent_refresh_origin: None,
            mount: IdeSurfaceMount::default(),
            agent_poll_generation: 0,
            agent_poll_task: None,
            agent_sampling_refresh_task: None,
            agent_sampling_backend_abort: None,
            agent_watch_generation: 0,
            watched_root_path: None,
            agent_watch_task: None,
            agent_watch_retry_task: None,
            agent_watch_stop_task: None,
            agent_watch_backend_abort: None,
            agent_watch_stop_in_flight: false,
            agent_watch_restart_requested: false,
            agent_watch_stop_generation: 0,
            agent_watch_stop_release_queue: None,
        }
    }

    pub fn load_state(&self) -> &IdeLoadState {
        &self.load_state
    }

    pub fn mount(&self) -> IdeSurfaceMount {
        self.mount
    }

    pub fn set_mount(&mut self, mount: IdeSurfaceMount, cx: &mut Context<Self>) {
        if self.mount == mount {
            return;
        }
        let was_visible = self.mount.is_visible();
        self.mount = mount;
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
        let previous_agent_mode = self.runtime_settings.agent_mode;
        let next_agent_mode = runtime_settings.agent_mode;
        self.tokens = tokens;
        self.runtime_settings = runtime_settings;
        self.fs.set_mode(next_agent_mode);
        if next_agent_mode == NodeAgentMode::Disabled {
            self.cancel_agent_sampling();
            self.stop_agent_watch(cx);
        } else if previous_agent_mode == NodeAgentMode::Disabled && self.mount.is_visible() {
            self.resume_agent_sampling(cx);
        }
        if next_agent_mode != NodeAgentMode::Ask {
            self.agent_opt_in_open = false;
        }
        for editor in self.editors.values() {
            apply_editor_runtime_settings(editor, self.tokens, &self.runtime_settings, cx);
        }
        cx.notify();
    }

    pub fn set_agent_mode(&mut self, agent_mode: NodeAgentMode, cx: &mut Context<Self>) {
        let mut runtime_settings = self.runtime_settings.clone();
        runtime_settings.agent_mode = agent_mode;
        self.set_visual_and_runtime_settings(self.tokens, runtime_settings, cx);
    }

    pub fn snapshot(&mut self, cx: &mut Context<Self>) -> Option<WorkspaceSnapshot> {
        self.sync_all_editors(cx);
        self.workspace.snapshot().ok()
    }

    pub fn reconnect_snapshot(&mut self, cx: &mut Context<Self>) -> Option<ReconnectIdeSnapshot> {
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
                buffer.revision != buffer.saved_revision || buffer.text != buffer.saved_text
            })
            .filter_map(|buffer| match &buffer.location {
                IdeLocation::Remote { path, .. } => Some((path.clone(), buffer.text.clone())),
                IdeLocation::Local { .. } => None,
            })
            .collect::<BTreeMap<_, _>>();

        Some(ReconnectIdeSnapshot {
            project_path,
            tab_paths,
            connection_id,
            dirty_contents,
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
        self.cancel_agent_sampling();
        if let Some(previous_node_id) = self.node_id.clone()
            && previous_node_id != node_id
        {
            self.stop_agent_watch(cx);
            self.release_ide_node_after_watch_stop(previous_node_id);
        } else if self.root_path.as_deref() != Some(root_path.as_str()) {
            self.stop_agent_watch(cx);
        }
        if self.pending_restore_files.is_empty() {
            self.pending_restore_dirty_contents.clear();
        }
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.node_id = Some(node_id.clone());
        self.root_path = Some(root_path.clone());
        self.git_branch = None;
        self.load_state = IdeLoadState::Loading;
        self.last_error = None;
        self.conflict_state = None;
        self.loading_paths.clear();
        self.loading_file_tabs.clear();
        self.saving_tabs.clear();
        self.tree_name_input = None;
        self.delete_confirm = None;
        self.tree_clipboard = None;
        self.agent_action = None;
        self.editors.clear();
        self.workspace = IdeWorkspace::new();
        cx.notify();

        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        cx.spawn(async move |weak, cx| {
            let result = await_ide_backend(backend_runtime.spawn(async move {
                open_project_with_root_listing(fs, node_id, root_path).await
            }))
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
        self.open_remote_project(node_id, root_path, cx);
    }

    pub fn release_remote_session(&mut self, cx: &mut Context<Self>) {
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

    fn release_ide_node_after_watch_stop(&mut self, node_id: String) {
        if self.agent_watch_stop_in_flight {
            // Keep the node consumer until watch/stop has used the existing
            // agent session; releasing first would turn cleanup into a no-op.
            if let Some(release_queue) = self.agent_watch_stop_release_queue.as_ref() {
                release_queue.request_release(&self.fs, node_id);
            } else {
                self.fs.release_ide_session_for_node(&node_id);
            }
        } else {
            self.fs.release_ide_session_for_node(&node_id);
        }
    }

    pub fn mark_connection_interrupted(&mut self, cx: &mut Context<Self>) {
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

    pub fn restore_reconnect_snapshot(
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
        if same_project_open {
            self.load_state = IdeLoadState::Ready;
            self.last_error = None;
            self.resume_agent_sampling(cx);
            for path in snapshot.tab_paths {
                self.open_remote_file(
                    IdeLocation::remote(snapshot.connection_id.clone(), path),
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

impl Drop for IdeSurface {
    fn drop(&mut self) {
        if let Some(abort_handle) = self.agent_sampling_backend_abort.take() {
            abort_handle.abort();
        }
        if let Some(abort_handle) = self.agent_watch_backend_abort.take() {
            abort_handle.abort();
        }
        // GPUI can drop an IDE surface during workspace teardown without a
        // `Context`. Release only this surface's node because the file-system
        // registry is shared with other IDE surfaces and AI consumers.
        if let Some(node_id) = self.node_id.take() {
            if let Some(release_queue) = self.agent_watch_stop_release_queue.as_ref() {
                // The detached Tokio stop task owns the final remote cleanup
                // and consumer release even when no GPUI update can run.
                release_queue.request_release(&self.fs, node_id);
            } else {
                self.fs.release_ide_session_for_node(&node_id);
            }
        }
    }
}

#[cfg(test)]
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

    fn configure_ready_surface(surface: &mut IdeSurface, cx: &mut Context<IdeSurface>) {
        surface.node_id = Some("visibility-node".to_string());
        surface.root_path = Some("/srv/app".to_string());
        surface.load_state = IdeLoadState::Ready;
        surface.set_mount(IdeSurfaceMount::MainWindow, cx);
    }

    #[gpui::test]
    fn main_window_mount_allows_agent_sampling(cx: &mut TestAppContext) {
        let surface = test_surface(cx);

        surface.update(cx, |surface, cx| {
            configure_ready_surface(surface, cx);
            assert_eq!(surface.mount(), IdeSurfaceMount::MainWindow);

            surface.schedule_next_agent_status_poll(cx);

            assert!(surface.agent_poll_task.is_some());
        });
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
            let fs = fs.clone();
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
            surface.agent_watch_task = Some(cx.spawn(async move |_weak, _cx| {
                std::future::pending::<()>().await;
            }));
            surface.schedule_agent_watch_retry(cx);
            surface.watched_root_path = Some("/srv/app".to_string());

            surface.set_mount(IdeSurfaceMount::Hidden, cx);
            surface.refresh_tree_for_watch_path("/srv/app/src/main.rs".to_string(), cx);

            assert!(surface.agent_poll_task.is_none());
            assert!(surface.agent_sampling_refresh_task.is_none());
            assert!(surface.agent_watch_task.is_none());
            assert!(surface.agent_watch_retry_task.is_none());
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
            assert!(surface.agent_sampling_refresh_task.is_none());

            surface.set_mount(IdeSurfaceMount::DetachedWindow, cx);

            assert_eq!(surface.mount(), IdeSurfaceMount::DetachedWindow);
            assert!(surface.agent_sampling_refresh_task.is_some());
            assert_eq!(
                surface.agent_refresh_origin,
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
            let poll_generation = surface.agent_poll_generation;

            surface.set_mount(IdeSurfaceMount::DetachedWindow, cx);

            assert_eq!(surface.agent_poll_generation, poll_generation);
            assert!(surface.agent_poll_task.is_some());
            assert!(surface.agent_sampling_refresh_task.is_none());
        });
    }

    #[gpui::test]
    fn hidden_surface_keeps_user_agent_refresh_completion_owned(cx: &mut TestAppContext) {
        let surface = test_surface(cx);

        surface.update(cx, |surface, cx| {
            configure_ready_surface(surface, cx);
            surface.set_mount(IdeSurfaceMount::Hidden, cx);

            surface.refresh_agent_status(AgentStatusRefreshOrigin::UserAction, cx);

            assert_eq!(surface.agent_action, Some(AgentActionKind::Refresh));
            assert_eq!(
                surface.agent_refresh_origin,
                Some(AgentStatusRefreshOrigin::UserAction)
            );
            assert!(surface.agent_poll_task.is_none());
            assert!(surface.agent_sampling_refresh_task.is_none());
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
            assert!(surface.agent_poll_task.is_none());
            assert!(surface.agent_sampling_refresh_task.is_none());
            assert!(surface.agent_watch_task.is_none());

            surface.mark_connection_interrupted(cx);

            assert_eq!(surface.load_state, IdeLoadState::Disconnected);
            assert_eq!(surface.mount(), IdeSurfaceMount::Hidden);
            assert!(surface.agent_poll_task.is_none());
            assert!(surface.agent_watch_task.is_none());
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
            let fs = fs.clone();
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
            surface.agent_watch_task = Some(cx.spawn(async move |_weak, _cx| {
                std::future::pending::<()>().await;
            }));
            surface.schedule_agent_watch_retry(cx);
            surface.watched_root_path = Some("/srv/app".to_string());
            surface.root_path = Some("/srv/app".to_string());
            surface.node_id = Some("node-watch".to_string());
            assert!(surface.agent_watch_task.is_some());
            assert!(surface.agent_watch_retry_task.is_some());

            surface.stop_agent_watch(cx);

            assert!(surface.agent_watch_task.is_none());
            assert!(surface.agent_watch_retry_task.is_none());
            assert!(surface.watched_root_path.is_none());
            assert!(surface.agent_watch_stop_task.is_some());
            assert!(surface.agent_watch_stop_in_flight);
            surface.release_ide_node_after_watch_stop("node-watch".to_string());
            assert!(surface.agent_watch_stop_release_queue.is_some());

            surface.start_agent_watch_if_ready(cx);

            assert!(surface.agent_watch_restart_requested);
            assert!(surface.agent_watch_task.is_none());
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
}
