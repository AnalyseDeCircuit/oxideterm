impl IdeSurface {
    pub fn new(
        fs: NodeAgentIdeFileSystem,
        tokens: ThemeTokens,
        labels: IdeLabels,
        runtime_settings: IdeRuntimeSettings,
        backend_runtime: Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) -> Self {
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
            agent_poll_generation: 0,
            agent_watch_generation: 0,
            watched_root_path: None,
        }
    }

    pub fn load_state(&self) -> &IdeLoadState {
        &self.load_state
    }

    pub fn set_visual_and_runtime_settings(
        &mut self,
        tokens: ThemeTokens,
        runtime_settings: IdeRuntimeSettings,
        cx: &mut Context<Self>,
    ) {
        self.tokens = tokens;
        self.runtime_settings = runtime_settings;
        self.fs.set_mode(runtime_settings.agent_mode);
        if runtime_settings.agent_mode != NodeAgentMode::Ask {
            self.agent_opt_in_open = false;
        }
        for editor in self.editors.values() {
            apply_editor_runtime_settings(editor, self.tokens, self.runtime_settings, cx);
        }
        cx.notify();
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
        if let Some(previous_node_id) = self.node_id.clone()
            && previous_node_id != node_id
        {
            self.stop_agent_watch(cx);
            self.fs
                .release_ide_session_for_node(&previous_node_id);
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
        self.stop_agent_watch(cx);
        self.clear_search_cache();
        self.search.generation = self.search.generation.wrapping_add(1);
        self.search.searching = false;
        self.pending_search_queries.clear();
        self.pending_reconnect_restore_node_id = None;
        self.pending_reconnect_restore_files_remaining = 0;
        if let Some(node_id) = self.node_id.take() {
            self.fs.release_ide_session_for_node(&node_id);
        }
    }

    pub fn mark_connection_interrupted(&mut self, cx: &mut Context<Self>) {
        self.stop_agent_watch(cx);
        self.clear_search_cache();
        self.search.generation = self.search.generation.wrapping_add(1);
        self.search.searching = false;
        self.pending_search_queries.clear();
        self.pending_reconnect_restore_node_id = None;
        self.pending_reconnect_restore_files_remaining = 0;
        if let Some(node_id) = self.node_id.as_deref() {
            self.fs.release_ide_session_for_node(node_id);
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
            self.refresh_agent_status(cx);
            self.schedule_next_agent_status_poll(cx);
            self.start_agent_watch_if_ready(cx);
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
        // GPUI can drop an IDE surface during workspace teardown without a
        // `Context`. Release only this surface's node because the file-system
        // registry is shared with other IDE surfaces and AI consumers.
        if let Some(node_id) = self.node_id.as_deref() {
            self.fs.release_ide_session_for_node(node_id);
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
        let handle = registry.acquire(
            config,
            ConnectionConsumer::NodeRouter(node_id.0.clone()),
        );
        handle.set_physical(Arc::new(()));
        registry.mark_state(handle.connection_id(), ConnectionState::Active);
        router
            .bind_connection(&node_id, handle.connection_id().to_string())
            .expect("bind active IDE test node");
        (node_id, handle)
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

        // Explicit deployment acquires each node-scoped IDE lease before the
        // fake SSH transport rejects the agent probe.
        backend_runtime.block_on(async {
            let _ = fs.deploy_agent_for_node(first_node.0.clone()).await;
            let _ = fs.deploy_agent_for_node(second_node.0.clone()).await;
        });
        assert!(first_handle
            .info()
            .consumers
            .contains(&ConnectionConsumer::Ide(first_node.0.clone())));
        assert!(second_handle
            .info()
            .consumers
            .contains(&ConnectionConsumer::Ide(second_node.0.clone())));

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
        let _second_surface = cx.new({
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

        first_surface.update(cx, |surface, cx| surface.release_remote_session(cx));

        assert!(!first_handle
            .info()
            .consumers
            .contains(&ConnectionConsumer::Ide(first_node.0)));
        assert!(second_handle
            .info()
            .consumers
            .contains(&ConnectionConsumer::Ide(second_node.0)));
    }
}
