impl IdeSurface {
    pub fn new(
        router: NodeRouter,
        tokens: ThemeTokens,
        labels: IdeLabels,
        runtime_settings: IdeRuntimeSettings,
        backend_runtime: Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            workspace: IdeWorkspace::new(),
            fs: NodeAgentIdeFileSystem::new(router, runtime_settings.agent_mode),
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
            search: ProjectSearchState::default(),
            search_cache: HashMap::new(),
            search_cache_order: Vec::new(),
            pending_search_queries: BTreeMap::new(),
            tab_context_menu: None,
            tree_context_menu: None,
            tree_name_input: None,
            delete_confirm: None,
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
            self.fs.release_ide_consumer(&previous_node_id);
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
            self.fs.release_ide_consumer(&node_id);
        }
        self.fs.release_all_ide_consumers();
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
            self.fs.release_ide_consumer(node_id);
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
        // `Context`. Mirror Tauri's closeProject/invalidateAgentCache ownership
        // boundary by releasing every NodeRouter IDE consumer here as a final
        // guard against shutdown-time orphan node liveness.
        self.fs.release_all_ide_consumers();
    }
}
