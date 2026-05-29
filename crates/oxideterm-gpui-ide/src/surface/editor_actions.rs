impl IdeSurface {
    pub fn project_root_path(&self) -> Option<String> {
        self.root_path.clone()
    }

    pub fn active_editor_tab_id(&self) -> Option<String> {
        // AI target discovery follows Tauri's IDE store shape, where tabId is
        // the active editor tab id rather than the outer application tab id.
        self.workspace.active_tab().map(|tab_id| tab_id.0.to_string())
    }

    pub fn open_file_paths(&self) -> Vec<String> {
        self.workspace
            .tabs()
            .iter()
            .filter_map(|tab| match &tab.location {
                IdeLocation::Remote { path, .. } => Some(path.clone()),
                IdeLocation::Local { .. } => None,
            })
            .collect()
    }

    pub fn ai_context_snapshot(&self) -> Option<IdeAiContextSnapshot> {
        let project_root = self.root_path.clone()?;
        let project_name = Path::new(&project_root)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(project_root.as_str())
            .to_string();
        let active_tab_id = self.workspace.active_tab();
        let active_tab = active_tab_id
            .and_then(|tab_id| self.workspace.tabs().iter().find(|tab| tab.id == tab_id));
        let active_file = active_tab.and_then(|tab| match &tab.location {
            IdeLocation::Remote { path, .. } => Some(path.clone()),
            IdeLocation::Local { .. } => None,
        });
        let open_tab_paths = self.open_file_paths();
        let buffer = active_tab_id.and_then(|tab_id| self.workspace.buffer(tab_id));
        let is_dirty = buffer.is_some_and(|buffer| buffer.is_dirty());
        let active_language = active_tab
            .and_then(|tab| buffer.and_then(|buffer| language_for_location(&tab.location, &buffer.text)))
            .map(|language| format!("{language:?}"));
        let (code_snippet, snippet_start_line) = buffer
            .map(|buffer| ai_code_snippet_around_start(&buffer.text))
            .unwrap_or((None, 1));
        Some(IdeAiContextSnapshot {
            project_root,
            project_name,
            git_branch: self.git_branch.clone(),
            active_file,
            active_language,
            is_dirty,
            open_tab_count: self.workspace.tabs().len(),
            open_tab_paths,
            code_snippet,
            snippet_start_line,
        })
    }

    pub fn plugin_snapshot(&self) -> Option<IdePluginSnapshot> {
        let snapshot = self.workspace.snapshot().ok()?;
        let (node_id, root_path) = match &snapshot.project.root {
            IdeLocation::Remote { node_id, path } => (node_id.clone(), path.clone()),
            IdeLocation::Local { .. } => return None,
        };
        let open_files = snapshot
            .tabs
            .iter()
            .map(|tab| {
                // Tauri exposes tab metadata only; native keeps file text inside
                // buffers and projects just language/dirty state into this API.
                let buffer = snapshot
                    .buffers
                    .iter()
                    .find(|buffer| buffer.tab_id == tab.id);
                let language = buffer
                    .and_then(|buffer| language_for_location(&tab.location, &buffer.text))
                    .map(|language| format!("{language:?}"))
                    .unwrap_or_default();
                IdePluginFileSnapshot {
                    path: ide_plugin_file_path(&tab.location),
                    name: tab.title.clone(),
                    language,
                    is_dirty: buffer.is_some_and(|buffer| {
                        buffer.revision != buffer.saved_revision || buffer.text != buffer.saved_text
                    }),
                    is_active: snapshot.active_tab == Some(tab.id),
                    is_pinned: tab.is_pinned,
                }
            })
            .collect::<Vec<_>>();
        let active_file = open_files.iter().find(|file| file.is_active).cloned();
        Some(IdePluginSnapshot {
            project: IdePluginProjectSnapshot {
                node_id,
                root_path,
                name: snapshot.project.title,
                is_git_repo: self.git_branch.is_some(),
                git_branch: self.git_branch.clone(),
            },
            open_files,
            active_file,
        })
    }

    pub fn retry_open_project(&mut self, cx: &mut Context<Self>) {
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        let Some(root_path) = self.root_path.clone() else {
            return;
        };
        self.open_remote_project(node_id, root_path, cx);
    }

    pub fn refresh_project_tree_root(&mut self, cx: &mut Context<Self>) {
        if !self.ensure_remote_actions_ready(cx) {
            return;
        }
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        let Some(root_path) = self.root_path.clone() else {
            return;
        };
        self.load_directory(IdeLocation::remote(node_id, root_path), cx);
    }

    pub fn restore_snapshot(&mut self, snapshot: WorkspaceSnapshot, cx: &mut Context<Self>) {
        let node_id = match &snapshot.project.root {
            IdeLocation::Remote { node_id, .. } => node_id.clone(),
            IdeLocation::Local { .. } => return,
        };
        let root_path = match &snapshot.project.root {
            IdeLocation::Remote { path, .. } => path.clone(),
            IdeLocation::Local { .. } => return,
        };
        let buffers = snapshot.buffers.clone();
        let result = self.workspace.restore_snapshot(snapshot);
        if !matches!(
            result,
            oxideterm_ide_core::RestoreSnapshotResult::Restored { .. }
        ) {
            return;
        }

        self.node_id = Some(node_id);
        self.root_path = Some(root_path);
        self.load_state = IdeLoadState::Ready;
        self.editors.clear();
        self.loading_file_tabs.clear();
        for buffer in buffers {
            self.create_editor(buffer.tab_id, &buffer.location, buffer.text, cx);
        }
        self.refresh_agent_status(cx);
        self.schedule_next_agent_status_poll(cx);
        self.start_agent_watch_if_ready(cx);
        cx.notify();
    }

    fn apply_project_open(&mut self, result: ProjectOpenResult, cx: &mut Context<Self>) {
        let root = result.root.clone();
        self.workspace.open_project(root.clone(), result.title);
        let _ = self.workspace.set_tree_expanded(&root, true);
        let _ = self.workspace.set_tree_children(root, result.children);
        self.node_id = Some(result.node_id);
        self.git_branch = result.git_branch;
        self.load_state = IdeLoadState::Ready;
        self.agent_opt_in_open = self.runtime_settings.agent_mode == NodeAgentMode::Ask;
        self.clear_search_cache();
        self.refresh_agent_status(cx);
        self.schedule_next_agent_status_poll(cx);
        self.start_agent_watch_if_ready(cx);
        cx.emit(IdeSurfaceEvent::ProjectOpened);
        let node_id = self.node_id.clone();
        let pending_restore_files = std::mem::take(&mut self.pending_restore_files);
        if self.pending_reconnect_restore_node_id.is_some() {
            self.pending_reconnect_restore_files_remaining = pending_restore_files.len();
        }
        for path in pending_restore_files {
            if let Some(node_id) = node_id.clone() {
                self.open_remote_file(IdeLocation::remote(node_id, path), cx);
            }
        }
        self.finish_pending_reconnect_file_restore_if_needed(cx);
        cx.notify();
    }

    fn load_directory(&mut self, directory: IdeLocation, cx: &mut Context<Self>) {
        if !self.ensure_remote_actions_ready(cx) {
            return;
        }
        let key = directory.stable_key();
        if self.loading_paths.contains(&key) {
            return;
        }
        self.loading_paths.insert(key.clone());
        let fs = self.fs.clone();
        let generation = self.generation;
        let backend_runtime = self.backend_runtime.clone();
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let directory_for_task = directory.clone();
            let result = await_ide_backend(backend_runtime.spawn(async move {
                fs.list_dir(&directory_for_task)
                    .await
                    .map(sort_tree_entries)
            }))
            .await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading_paths.remove(&key);
                match result {
                    Ok(children) => {
                        let _ = this.workspace.set_tree_expanded(&directory, true);
                        let _ = this.workspace.set_tree_children(directory, children);
                    }
                    Err(error) => this.last_error = Some(error.message),
                }
                if this.pending_reconnect_restore_files_remaining > 0 {
                    this.pending_reconnect_restore_files_remaining -= 1;
                }
                this.finish_pending_reconnect_file_restore_if_needed(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn finish_pending_reconnect_file_restore_if_needed(&mut self, cx: &mut Context<Self>) {
        if self.pending_reconnect_restore_files_remaining > 0 {
            return;
        }
        let Some(reconnect_node_id) = self.pending_reconnect_restore_node_id.take() else {
            return;
        };
        // Tauri's restore-ide phase completes after the project tree is open
        // and every captured tab has either reopened or failed individually.
        cx.emit(IdeSurfaceEvent::ReconnectRestoreProjectOpened { reconnect_node_id });
    }

    fn refresh_tree_for_watch_path(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        let Some(root_path) = self.root_path.clone() else {
            return;
        };
        let path = normalize_remote_path(&path);
        let root_path = normalize_remote_path(&root_path);
        if path != root_path && !path.starts_with(&format!("{}/", root_path.trim_end_matches('/')))
        {
            return;
        }
        self.load_directory(IdeLocation::remote(node_id, path), cx);
    }

    fn start_agent_watch_if_ready(&mut self, cx: &mut Context<Self>) {
        if !matches!(
            self.fs.status_for_node(self.node_id.as_deref()),
            AgentStatus::Ready { .. }
        ) {
            self.stop_agent_watch(cx);
            return;
        }
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        let Some(root_path) = self.root_path.clone() else {
            return;
        };
        let root_path = normalize_remote_path(&root_path);
        if self.watched_root_path.as_deref() == Some(root_path.as_str()) {
            return;
        }

        self.stop_agent_watch(cx);
        self.agent_watch_generation = self.agent_watch_generation.wrapping_add(1);
        let generation = self.agent_watch_generation;
        self.watched_root_path = Some(root_path.clone());
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        cx.spawn(async move |weak, cx| {
            let watch = await_ide_backend(backend_runtime.spawn({
                let node_id = node_id.clone();
                let root_path = root_path.clone();
                async move { fs.watch_directory(node_id, root_path, Vec::new()).await }
            }))
            .await;

            match watch {
                Ok(Some(mut subscription)) => {
                    while let Some(event) = subscription.recv().await {
                        let refresh_path = watch_refresh_path(&root_path, &event.path);
                        let should_continue = weak
                            .update(cx, |this, cx| {
                                if this.agent_watch_generation != generation {
                                    return false;
                                }
                                this.clear_search_cache();
                                this.refresh_tree_for_watch_path(refresh_path, cx);
                                true
                            })
                            .unwrap_or(false);
                        if !should_continue {
                            return;
                        }
                    }
                }
                Ok(None) | Err(_) => {}
            }

            let _ = weak.update(cx, |this, cx| {
                if this.agent_watch_generation == generation
                    && this.watched_root_path.as_deref() == Some(root_path.as_str())
                {
                    this.schedule_agent_watch_retry(cx);
                }
            });
        })
        .detach();
    }

    fn schedule_agent_watch_retry(&mut self, cx: &mut Context<Self>) {
        let generation = self.agent_watch_generation;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_secs(IDE_AGENT_WATCH_RETRY_SECS)).await;
            let _ = weak.update(cx, |this, cx| {
                if this.agent_watch_generation == generation {
                    this.watched_root_path = None;
                    this.start_agent_watch_if_ready(cx);
                }
            });
        })
        .detach();
    }

    fn stop_agent_watch(&mut self, cx: &mut Context<Self>) {
        self.agent_watch_generation = self.agent_watch_generation.wrapping_add(1);
        let Some(root_path) = self.watched_root_path.take() else {
            return;
        };
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        cx.spawn(async move |_weak, _cx| {
            let _ = backend_runtime
                .spawn(async move { fs.stop_watch_directory(node_id, root_path).await })
                .await;
        })
        .detach();
    }

    fn open_project_search(&mut self, cx: &mut Context<Self>) {
        if !self.ensure_remote_actions_ready(cx) {
            return;
        }
        self.search.open = true;
        self.search.error = None;
        if !self.search.query.trim().is_empty() && self.search.results.is_empty() {
            self.schedule_project_search(cx);
        }
        cx.notify();
    }

    fn close_project_search(&mut self, cx: &mut Context<Self>) {
        self.search.open = false;
        self.search.generation = self.search.generation.wrapping_add(1);
        cx.notify();
    }

    fn clear_search_cache(&mut self) {
        self.search_cache.clear();
        self.search_cache_order.clear();
    }

    fn schedule_project_search(&mut self, cx: &mut Context<Self>) {
        let query = self.search.query.clone();
        self.search.generation = self.search.generation.wrapping_add(1);
        let generation = self.search.generation;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(IDE_SEARCH_DEBOUNCE_MS)).await;
            let _ = weak.update(cx, |this, cx| {
                if this.search.generation == generation && this.search.query == query {
                    this.run_project_search(cx);
                }
            });
        })
        .detach();
    }

    fn run_project_search(&mut self, cx: &mut Context<Self>) {
        if !self.ensure_remote_actions_ready(cx) {
            self.search.searching = false;
            cx.notify();
            return;
        }
        let query = self.search.query.trim().to_string();
        if query.is_empty() {
            self.search.results.clear();
            self.search.expanded_paths.clear();
            self.search.error = None;
            self.search.truncated = false;
            self.search.searching = false;
            cx.notify();
            return;
        }
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        let Some(root_path) = self.root_path.clone() else {
            return;
        };
        let search_query = IdeSearchQuery::tauri_literal_project_search(
            query.clone(),
            normalize_remote_path(&root_path),
            IDE_SEARCH_MAX_RESULTS,
            self.search.generation,
        );
        let cache_key = search_query.cache_key();
        if let Some(entry) = self.search_cache.get(&cache_key)
            && entry.timestamp.elapsed() < Duration::from_secs(IDE_SEARCH_CACHE_TTL_SECS)
        {
            self.search.results = entry.results.clone();
            self.search.expanded_paths = self
                .search
                .results
                .iter()
                .map(|group| group.path.clone())
                .collect();
            self.search.truncated = entry.truncated;
            self.search.error = None;
            self.search.searching = false;
            cx.notify();
            return;
        }

        self.search.searching = true;
        self.search.error = None;
        self.search.generation = self.search.generation.wrapping_add(1);
        let generation = self.search.generation;
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let result = await_ide_backend(backend_runtime.spawn({
                let search_query = search_query.clone();
                async move { fs.search_project(node_id, search_query).await.map(group_search_matches) }
            }))
            .await;
            let _ = weak.update(cx, |this, cx| {
                if this.search.generation != generation {
                    return;
                }
                this.search.searching = false;
                match result {
                    Ok(results) => {
                        let truncated = results
                            .iter()
                            .map(|group| group.matches.len())
                            .sum::<usize>()
                            >= IDE_SEARCH_MAX_RESULTS as usize;
                        this.search.expanded_paths =
                            results.iter().map(|group| group.path.clone()).collect();
                        this.search.results = results.clone();
                        this.search.truncated = truncated;
                        this.search.error = None;
                        this.put_search_cache(cache_key, results, truncated);
                    }
                    Err(error) => {
                        this.search.error = Some(error.message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn put_search_cache(
        &mut self,
        key: String,
        results: Vec<SearchResultGroup>,
        truncated: bool,
    ) {
        self.search_cache_order.retain(|existing| existing != &key);
        self.search_cache_order.push(key.clone());
        self.search_cache.insert(
            key,
            SearchCacheEntry {
                results,
                timestamp: Instant::now(),
                truncated,
            },
        );
        while self.search_cache_order.len() > IDE_SEARCH_CACHE_MAX_ENTRIES {
            if let Some(oldest) = self.search_cache_order.first().cloned() {
                self.search_cache_order.remove(0);
                self.search_cache.remove(&oldest);
            } else {
                break;
            }
        }
    }

    fn handle_project_search_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if !self.search.open {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.close_project_search(cx),
            "enter" => self.run_project_search(cx),
            "backspace" => {
                self.search.query.pop();
                self.schedule_project_search(cx);
                cx.notify();
            }
            _ => {
                if let Some(text) = event.keystroke.key_char.as_deref()
                    && !text.is_empty()
                    && !text.chars().any(char::is_control)
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                {
                    self.search.query.push_str(text);
                    self.schedule_project_search(cx);
                    cx.notify();
                }
            }
        }
        cx.stop_propagation();
    }

    fn open_search_match(&mut self, hit: IdeSearchMatch, cx: &mut Context<Self>) {
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        let Some(root_path) = self.root_path.clone() else {
            return;
        };
        let path = resolve_search_match_path(&root_path, &hit.path);
        self.pending_search_queries
            .insert(path.clone(), self.search.query.clone());
        self.open_remote_file(IdeLocation::remote(node_id, path), cx);
    }

    fn apply_pending_search_query_for_location(
        &mut self,
        location: &IdeLocation,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = remote_path(location).map(ToOwned::to_owned) else {
            return;
        };
        let Some(query) = self.pending_search_queries.remove(&path) else {
            return;
        };
        let Some(tab_id) = self
            .workspace
            .tabs()
            .iter()
            .find(|tab| tab.location == *location)
            .map(|tab| tab.id)
        else {
            return;
        };
        if let Some(editor) = self.editors.get(&tab_id) {
            editor.update(cx, |editor, cx| editor.set_find_query(query, cx));
        }
    }

    fn open_tree_entry(&mut self, entry: FileTreeEntry, cx: &mut Context<Self>) {
        let _ = self
            .workspace
            .select_tree_entry(Some(entry.location.clone()));
        if !self.ensure_remote_actions_ready(cx) {
            return;
        }
        match entry.kind {
            FileKind::Directory => {
                if self.workspace.file_tree().is_expanded(&entry.location) {
                    let _ = self.workspace.set_tree_expanded(&entry.location, false);
                    cx.notify();
                } else {
                    self.load_directory(entry.location, cx);
                }
            }
            FileKind::File | FileKind::Symlink | FileKind::Other => {
                self.open_remote_file(entry.location, cx);
            }
        }
    }

    fn open_remote_file(&mut self, location: IdeLocation, cx: &mut Context<Self>) {
        if !self.ensure_remote_actions_ready(cx) {
            return;
        }
        if let Some(tab_id) = self
            .workspace
            .tabs()
            .iter()
            .find(|tab| tab.location == location)
            .map(|tab| tab.id)
        {
            let _ = self.workspace.set_active_tab(tab_id);
            self.apply_pending_reconnect_dirty_for_tab(tab_id, cx);
            self.apply_pending_search_query_for_location(&location, cx);
            cx.notify();
            return;
        }
        let key = location.stable_key();
        if self.loading_paths.contains(&key) {
            return;
        }
        let tab_id = match self.workspace.open_file(
            location.clone(),
            String::new(),
            SavedFileVersion::unknown(),
        ) {
            Ok(oxideterm_ide_core::OpenFileOutcome::Opened(tab_id))
            | Ok(oxideterm_ide_core::OpenFileOutcome::Reused(tab_id)) => tab_id,
            Err(error) => {
                self.last_error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        self.loading_file_tabs.insert(tab_id);
        self.loading_paths.insert(key.clone());
        let fs = self.fs.clone();
        let generation = self.generation;
        let backend_runtime = self.backend_runtime.clone();
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let result = await_ide_backend(backend_runtime.spawn({
                let location = location.clone();
                async move { open_text_file(fs, location).await }
            }))
            .await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading_paths.remove(&key);
                this.loading_file_tabs.remove(&tab_id);
                match result {
                    Ok(result) => {
                        if this
                            .workspace
                            .buffer(tab_id)
                            .is_none_or(|current| current.location != result.location)
                        {
                            cx.notify();
                            return;
                        }
                        let dirty_text = remote_path(&result.location)
                            .and_then(|path| this.pending_restore_dirty_contents.remove(path));
                        let _ = this
                            .workspace
                            .replace_buffer_text(tab_id, result.text.clone());
                        let _ = this.workspace.mark_saved(tab_id, result.version);
                        this.create_editor(tab_id, &result.location, result.text, cx);
                        if let Some(dirty_text) = dirty_text {
                            this.apply_reconnect_dirty_text(tab_id, dirty_text, cx);
                        }
                        this.apply_pending_search_query_for_location(&result.location, cx);
                    }
                    Err(error) => {
                        if this
                            .workspace
                            .buffer(tab_id)
                            .is_some_and(|current| current.location == location)
                        {
                            let _ = this.workspace.request_close_tab(tab_id);
                            this.editors.remove(&tab_id);
                        }
                        this.last_error = Some(error.message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn create_editor(
        &mut self,
        tab_id: EditorTabId,
        location: &IdeLocation,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let tokens = self.tokens;
        let runtime_settings = self.runtime_settings;
        let language = language_for_location(location, &text);
        let surface = cx.entity();
        let editor = cx.new(|cx| {
            let mut editor = TextEditorView::new(text, &tokens, cx);
            editor.apply_ide_runtime_settings(
                &tokens,
                runtime_settings.editor_font_size,
                runtime_settings.editor_line_height,
                runtime_settings.word_wrap,
                runtime_settings.background_active,
                cx,
            );
            editor.set_language(language, cx);
            editor.set_on_save(Box::new(move |text, _window, cx| {
                let text = text.to_string();
                let _ = surface.update(cx, |surface, cx| {
                    surface.save_tab_with_text(tab_id, text, cx);
                });
                Ok(())
            }));
            editor
        });
        self.editors.insert(tab_id, editor);
    }

    fn apply_pending_reconnect_dirty_for_tab(
        &mut self,
        tab_id: EditorTabId,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self
            .workspace
            .buffer(tab_id)
            .and_then(|buffer| remote_path(&buffer.location).map(ToOwned::to_owned))
        else {
            return;
        };
        let Some(dirty_text) = self.pending_restore_dirty_contents.remove(&path) else {
            return;
        };
        self.apply_reconnect_dirty_text(tab_id, dirty_text, cx);
    }

    fn apply_reconnect_dirty_text(
        &mut self,
        tab_id: EditorTabId,
        dirty_text: String,
        cx: &mut Context<Self>,
    ) {
        let Some(buffer) = self.workspace.buffer(tab_id).cloned() else {
            return;
        };
        if buffer.is_dirty() || dirty_text == buffer.saved_text {
            return;
        }

        // Tauri only writes snapshot dirtyContents back into clean tabs. Native
        // keeps the same user-intent rule so edits made after the snapshot win.
        let _ = self
            .workspace
            .replace_buffer_text(tab_id, dirty_text.clone());
        if let Some(editor) = self.editors.get(&tab_id) {
            editor.update(cx, |editor, cx| {
                editor.replace_text_external(dirty_text, cx);
            });
        }
    }

    fn activate_tab(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let previous = self.workspace.active_tab();
        if previous == Some(tab_id) {
            return;
        }
        // Tauri auto-saves the previously active dirty tab when activeTabId
        // changes. Window-blur save-all still needs a GPUI focus-loss hook.
        if self.runtime_settings.auto_save
            && let Some(previous_tab_id) = previous
            && self.is_tab_dirty(previous_tab_id, cx)
            && !self.saving_tabs.contains(&previous_tab_id)
        {
            self.save_tab(previous_tab_id, cx);
        }
        let _ = self.workspace.set_active_tab(tab_id);
        cx.notify();
    }

    fn close_tab(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        self.sync_editor_to_workspace(tab_id, cx);
        match self.workspace.request_close_tab(tab_id) {
            Ok(None) => {
                self.editors.remove(&tab_id);
                self.loading_file_tabs.remove(&tab_id);
                if self
                    .conflict_state
                    .as_ref()
                    .is_some_and(|conflict| conflict.tab_id == tab_id)
                {
                    self.conflict_state = None;
                }
                cx.notify();
            }
            Ok(Some(_)) => cx.notify(),
            Err(error) => {
                self.last_error = Some(error.to_string());
                cx.notify();
            }
        }
    }

    fn request_delete_tree_item(
        &mut self,
        location: IdeLocation,
        name: String,
        is_directory: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.ensure_remote_actions_ready(cx) {
            return;
        }
        let affected = self.workspace.affected_tabs_under(&location);
        let unsaved_tab_count = affected
            .iter()
            .filter(|tab_id| self.is_tab_dirty(**tab_id, cx))
            .count();
        self.delete_confirm = Some(DeleteConfirmState {
            location,
            name,
            is_directory,
            affected_tab_count: affected.len(),
            unsaved_tab_count,
            deleting: false,
        });
        cx.notify();
    }

    fn request_tree_name_input(
        &mut self,
        kind: TreeNameInputKind,
        location: IdeLocation,
        name: String,
        is_directory: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.ensure_remote_actions_ready(cx) {
            return;
        }
        let parent_path = match &location {
            IdeLocation::Remote { path, .. } => match kind {
                TreeNameInputKind::NewFile | TreeNameInputKind::NewFolder if is_directory => {
                    path.clone()
                }
                TreeNameInputKind::NewFile | TreeNameInputKind::NewFolder => {
                    parent_remote_path(path)
                }
                TreeNameInputKind::Rename => parent_remote_path(path),
            },
            IdeLocation::Local { .. } => return,
        };
        let value = if kind == TreeNameInputKind::Rename {
            name.clone()
        } else {
            String::new()
        };
        self.tree_name_input = Some(TreeNameInputState {
            kind,
            target: location,
            parent_path,
            original_name: (kind == TreeNameInputKind::Rename).then_some(name),
            value,
            error: None,
            submitting: false,
        });
        cx.notify();
    }

    fn cancel_tree_name_input(&mut self, cx: &mut Context<Self>) {
        self.tree_name_input = None;
        cx.notify();
    }

    fn submit_tree_name_input(&mut self, cx: &mut Context<Self>) {
        if !self.ensure_remote_actions_ready(cx) {
            return;
        }
        let Some(mut input) = self.tree_name_input.clone() else {
            return;
        };
        if input.submitting {
            return;
        }
        let name = input.value.trim().to_string();
        if let Some(error) = validate_file_name(&name) {
            input.error = Some(error);
            self.tree_name_input = Some(input);
            cx.notify();
            return;
        }
        let node_id = match &input.target {
            IdeLocation::Remote { node_id, .. } => node_id.clone(),
            IdeLocation::Local { .. } => return,
        };
        let new_path = join_remote_child(&input.parent_path, &name);
        let old_location = input.target.clone();
        let parent_path = input.parent_path.clone();
        let fs = self.fs.clone();
        let generation = self.generation;
        let backend_runtime = self.backend_runtime.clone();
        input.submitting = true;
        input.error = None;
        self.tree_name_input = Some(input.clone());
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let node_id_for_task = node_id.clone();
            let new_path_for_task = new_path.clone();
            let old_location_for_task = old_location.clone();
            let result = await_ide_backend(backend_runtime.spawn({
                async move {
                    match input.kind {
                        TreeNameInputKind::NewFile => {
                            fs.create_file(node_id_for_task, new_path_for_task.clone())
                                .await
                                .map(|_| ())
                        }
                        TreeNameInputKind::NewFolder => {
                            fs.create_folder(node_id_for_task, new_path_for_task).await
                        }
                        TreeNameInputKind::Rename => {
                            if let IdeLocation::Remote { path: old_path, .. } = &old_location_for_task {
                                if normalize_remote_path(old_path)
                                    == normalize_remote_path(&new_path_for_task)
                                {
                                    return Ok(());
                                }
                                fs.rename_item(
                                    node_id_for_task,
                                    old_path.clone(),
                                    new_path_for_task.clone(),
                                )
                                .await
                            } else {
                                Ok(())
                            }
                        }
                    }
                }
            }))
            .await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(()) => {
                        if input.kind == TreeNameInputKind::Rename {
                            let new_location = IdeLocation::remote(node_id.clone(), new_path.clone());
                            if let Err(error) =
                                this.workspace.rename_tabs_under(&old_location, &new_location)
                            {
                                this.last_error = Some(error.to_string());
                            }
                        }
                        this.clear_search_cache();
                        this.tree_name_input = None;
                        this.load_directory(IdeLocation::remote(node_id.clone(), parent_path), cx);
                        if input.kind == TreeNameInputKind::NewFile {
                            this.open_remote_file(IdeLocation::remote(node_id, new_path), cx);
                        }
                        this.start_agent_watch_if_ready(cx);
                    }
                    Err(error) => {
                        let mut next = input.clone();
                        next.submitting = false;
                        next.error = Some(error.message);
                        this.tree_name_input = Some(next);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn handle_tree_name_input_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.tree_name_input.is_none() {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.cancel_tree_name_input(cx),
            "enter" => self.submit_tree_name_input(cx),
            "backspace" => {
                if let Some(input) = self.tree_name_input.as_mut()
                    && !input.submitting
                {
                    input.value.pop();
                    input.error = validate_file_name(input.value.trim());
                    cx.notify();
                }
            }
            _ => {
                if let Some(text) = event.keystroke.key_char.as_deref()
                    && let Some(input) = self.tree_name_input.as_mut()
                    && !input.submitting
                    && !text.is_empty()
                    && !text.chars().any(char::is_control)
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                {
                    input.value.push_str(text);
                    input.error = validate_file_name(input.value.trim());
                    cx.notify();
                }
            }
        }
        cx.stop_propagation();
    }

    fn cancel_delete_tree_item(&mut self, cx: &mut Context<Self>) {
        self.delete_confirm = None;
        cx.notify();
    }

    fn confirm_delete_tree_item(&mut self, cx: &mut Context<Self>) {
        if !self.ensure_remote_actions_ready(cx) {
            return;
        }
        let Some(mut confirm) = self.delete_confirm.clone() else {
            return;
        };
        if confirm.deleting || confirm.unsaved_tab_count > 0 {
            return;
        }
        let (node_id, path) = match &confirm.location {
            IdeLocation::Remote { node_id, path } => (node_id.clone(), path.clone()),
            IdeLocation::Local { .. } => return,
        };
        self.sync_all_editors(cx);
        match self.workspace.close_clean_tabs_under(&confirm.location) {
            Ok(closed_tabs) => {
                for tab_id in closed_tabs {
                    self.editors.remove(&tab_id);
                }
            }
            Err(error) => {
                self.last_error = Some(error.to_string());
                self.delete_confirm = None;
                cx.notify();
                return;
            }
        }
        confirm.deleting = true;
        self.delete_confirm = Some(confirm.clone());
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        let parent_path = parent_remote_path(&path);
        let root_path = self.root_path.clone();
        let generation = self.generation;
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let result = await_ide_backend(backend_runtime.spawn(async move {
                fs.delete_item(node_id, path, confirm.is_directory).await
            }))
            .await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(()) => {
                        this.clear_search_cache();
                        this.delete_confirm = None;
                        if let Some(node_id) = this.node_id.clone() {
                            this.load_directory(IdeLocation::remote(node_id, parent_path), cx);
                        }
                        if root_path.as_deref() == this.root_path.as_deref() {
                            this.start_agent_watch_if_ready(cx);
                        }
                    }
                    Err(error) => {
                        this.delete_confirm = None;
                        this.last_error = Some(error.message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn toggle_tab_pin(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        if let Err(error) = self.workspace.toggle_tab_pin(tab_id) {
            self.last_error = Some(error.to_string());
        }
        cx.notify();
    }

    fn start_tab_drag(&mut self, tab_id: EditorTabId, position: Point<Pixels>) {
        self.tab_drag = Some(TabDrag {
            tab_id,
            start_position: position,
            over_tab_id: tab_id,
            activated: false,
        });
    }

    fn update_tab_drag(
        &mut self,
        target_tab_id: EditorTabId,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(mut drag) = self.tab_drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let distance = f32::from(event.position.x - drag.start_position.x).abs();
        if !drag.activated && distance < IDE_TAB_REORDER_ACTIVATION_PX {
            return;
        }
        drag.activated = true;
        drag.over_tab_id = target_tab_id;
        self.tab_drag = Some(drag);
        cx.notify();
    }

    fn finish_tab_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.tab_drag.take() {
            if drag.activated
                && drag.tab_id != drag.over_tab_id
                && let Some(target_index) = self
                    .workspace
                    .tabs()
                    .iter()
                    .position(|tab| tab.id == drag.over_tab_id)
            {
                let _ = self.workspace.move_tab_to_index(drag.tab_id, target_index);
            }
            cx.notify();
        }
    }

    fn resolve_dirty_close(&mut self, decision: DirtyCloseDecision, cx: &mut Context<Self>) {
        let Some(request) = self.workspace.pending_close().cloned() else {
            return;
        };
        match decision {
            DirtyCloseDecision::Save => {
                self.save_after_close = Some(request.id);
                self.save_tab(request.tab_id, cx);
            }
            DirtyCloseDecision::Discard | DirtyCloseDecision::Cancel => {
                let closing_tab = request.tab_id;
                let resolved = self.workspace.resolve_dirty_close(request.id, decision);
                if matches!(resolved, Ok(None)) && decision == DirtyCloseDecision::Discard {
                    self.editors.remove(&closing_tab);
                    if self
                        .conflict_state
                        .as_ref()
                        .is_some_and(|conflict| conflict.tab_id == closing_tab)
                    {
                        self.conflict_state = None;
                    }
                }
                cx.notify();
            }
        }
    }

    fn save_tab(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        self.sync_editor_to_workspace(tab_id, cx);
        self.save_tab_current(tab_id, cx);
    }

    fn save_tab_with_text(
        &mut self,
        tab_id: EditorTabId,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let _ = self.workspace.replace_buffer_text(tab_id, text);
        self.save_tab_current(tab_id, cx);
    }

    fn save_tab_current(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let close_request = self.save_after_close.take();
        if !self.ensure_remote_actions_ready(cx) {
            return;
        }
        if self.loading_file_tabs.contains(&tab_id) {
            return;
        }
        let Some(buffer) = self.workspace.buffer(tab_id).cloned() else {
            return;
        };
        let title = self
            .workspace
            .tabs()
            .iter()
            .find(|tab| tab.id == tab_id)
            .map(|tab| tab.title.clone())
            .unwrap_or_else(|| buffer.location.display_name());
        let local_mtime = buffer.version.modified_millis.map(|millis| millis / 1000);
        if self.saving_tabs.contains(&tab_id) {
            return;
        }
        self.saving_tabs.insert(tab_id);
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        let generation = self.generation;
        let saved_location = buffer.location.clone();
        let saved_text = buffer.text.clone();
        let saved_revision = buffer.revision;
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let mode = if fs.capabilities().atomic_write {
                WriteMode::AtomicReplace
            } else {
                WriteMode::CreateOrReplace
            };
            let result = backend_runtime
                .spawn(async move {
                    match fs
                        .write_file(&buffer.location, &buffer.text, Some(&buffer.version), mode)
                        .await
                    {
                        Ok(version) => Ok(version),
                        Err(error) if error.kind == IdeFileErrorKind::Conflict => {
                            let remote_version = fs
                                .stat(&buffer.location)
                                .await
                                .ok()
                                .map(|stat| stat.version);
                            Err((error, remote_version))
                        }
                        Err(error) => Err((error, None)),
                    }
                })
                .await
                .unwrap_or_else(|error| {
                    Err((
                        IdeFileError::new(
                            IdeFileErrorKind::Other,
                            format!("IDE backend task failed: {error}"),
                        ),
                        None,
                    ))
                });
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.saving_tabs.remove(&tab_id);
                if this
                    .workspace
                    .buffer(tab_id)
                    .is_none_or(|current| current.location != saved_location)
                {
                    return;
                }
                match result {
                    Ok(version) => {
                        this.clear_search_cache();
                        let clean_after_save = if let Some(request_id) = close_request {
                            this.workspace
                                .complete_dirty_close_after_save_at_revision(
                                    request_id,
                                    saved_text,
                                    saved_revision,
                                    version.clone(),
                                )
                                .unwrap_or(false)
                        } else {
                            this.workspace
                                .complete_save_at_revision(
                                    tab_id,
                                    saved_text,
                                    saved_revision,
                                    version,
                                )
                                .unwrap_or(false)
                        };
                        if close_request.is_some() && clean_after_save {
                            this.editors.remove(&tab_id);
                        } else if clean_after_save
                            && let Some(editor) = this.editors.get(&tab_id)
                        {
                            editor.update(cx, |editor, cx| editor.mark_saved_external(cx));
                        }
                    }
                    Err((error, remote_version)) if error.kind == IdeFileErrorKind::Conflict => {
                        this.conflict_state = Some(ConflictState {
                            tab_id,
                            title,
                            local_mtime,
                            remote_mtime: remote_version
                                .and_then(|version| version.modified_millis)
                                .map(|millis| millis / 1000),
                            close_request,
                        });
                        if let Some(editor) = this.editors.get(&tab_id) {
                            editor.update(cx, |editor, cx| {
                                editor.mark_save_failed_external(
                                    this.labels.conflict_title.clone(),
                                    cx,
                                )
                            });
                        }
                    }
                    Err((error, _)) => {
                        let message = format!("{}: {}", this.labels.save_failed, error.message);
                        this.last_error = Some(message.clone());
                        if let Some(editor) = this.editors.get(&tab_id) {
                            editor.update(cx, |editor, cx| {
                                editor.mark_save_failed_external(message, cx)
                            });
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn clear_conflict(&mut self, cx: &mut Context<Self>) {
        self.conflict_state = None;
        cx.notify();
    }

    fn overwrite_conflict(&mut self, cx: &mut Context<Self>) {
        let Some(conflict) = self.conflict_state.clone() else {
            return;
        };
        if !self.ensure_remote_actions_ready(cx) {
            return;
        }
        let Some(buffer) = self.workspace.buffer(conflict.tab_id).cloned() else {
            self.conflict_state = None;
            cx.notify();
            return;
        };
        if self.saving_tabs.contains(&conflict.tab_id) {
            return;
        }
        self.saving_tabs.insert(conflict.tab_id);
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        let generation = self.generation;
        let conflict_location = buffer.location.clone();
        let saved_text = buffer.text.clone();
        let saved_revision = buffer.revision;
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let mode = if fs.capabilities().atomic_write {
                WriteMode::AtomicReplace
            } else {
                WriteMode::CreateOrReplace
            };
            let result = await_ide_backend(backend_runtime.spawn(async move {
                // Tauri resolveConflict('overwrite') force-saves without the
                // agent hash / SFTP mtime expectation after the user confirms.
                fs.write_file(&buffer.location, &buffer.text, None, mode)
                    .await
            }))
            .await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.saving_tabs.remove(&conflict.tab_id);
                if this
                    .workspace
                    .buffer(conflict.tab_id)
                    .is_none_or(|current| current.location != conflict_location)
                {
                    return;
                }
                match result {
                    Ok(version) => {
                        this.clear_search_cache();
                        this.conflict_state = None;
                        let clean_after_save = if let Some(request_id) = conflict.close_request {
                            this.workspace
                                .complete_dirty_close_after_save_at_revision(
                                    request_id,
                                    saved_text,
                                    saved_revision,
                                    version.clone(),
                                )
                                .unwrap_or(false)
                        } else {
                            this.workspace
                                .complete_save_at_revision(
                                    conflict.tab_id,
                                    saved_text,
                                    saved_revision,
                                    version,
                                )
                                .unwrap_or(false)
                        };
                        if conflict.close_request.is_some() && clean_after_save {
                            this.editors.remove(&conflict.tab_id);
                        } else if clean_after_save
                            && let Some(editor) = this.editors.get(&conflict.tab_id)
                        {
                            editor.update(cx, |editor, cx| editor.mark_saved_external(cx));
                        }
                    }
                    Err(error) => {
                        let message = format!("{}: {}", this.labels.save_failed, error.message);
                        this.last_error = Some(message.clone());
                        if let Some(editor) = this.editors.get(&conflict.tab_id) {
                            editor.update(cx, |editor, cx| {
                                editor.mark_save_failed_external(message, cx)
                            });
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn reload_conflict(&mut self, cx: &mut Context<Self>) {
        let Some(conflict) = self.conflict_state.clone() else {
            return;
        };
        if !self.ensure_remote_actions_ready(cx) {
            return;
        }
        let Some(buffer) = self.workspace.buffer(conflict.tab_id).cloned() else {
            self.conflict_state = None;
            cx.notify();
            return;
        };
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        let generation = self.generation;
        let conflict_location = buffer.location.clone();
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let result = await_ide_backend(
                backend_runtime.spawn(async move { fs.read_file(&buffer.location).await }),
            )
            .await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation
                    || this.workspace.buffer(conflict.tab_id).is_none_or(|current| {
                        current.location != conflict_location
                    })
                {
                    return;
                }
                match result {
                    Ok(data) => {
                        this.conflict_state = None;
                        let _ = this
                            .workspace
                            .replace_buffer_text(conflict.tab_id, data.text.clone());
                        let _ = this.workspace.mark_saved(conflict.tab_id, data.version);
                        if let Some(editor) = this.editors.get(&conflict.tab_id) {
                            editor.update(cx, |editor, cx| {
                                editor.replace_text_external(data.text, cx);
                                editor.mark_saved_external(cx);
                            });
                        }
                    }
                    Err(error) => {
                        this.last_error =
                            Some(format!("{}: {}", this.labels.open_failed, error.message));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn sync_editor_to_workspace(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let Some(editor) = self.editors.get(&tab_id) else {
            return;
        };
        let text = editor.read(cx).buffer().text();
        let _ = self.workspace.replace_buffer_text(tab_id, text);
    }

    fn sync_all_editors(&mut self, cx: &mut Context<Self>) {
        let tab_ids = self.editors.keys().copied().collect::<Vec<_>>();
        for tab_id in tab_ids {
            self.sync_editor_to_workspace(tab_id, cx);
        }
    }

    fn active_editor(&self) -> Option<Entity<TextEditorView>> {
        if self
            .workspace
            .active_tab()
            .is_some_and(|tab_id| self.loading_file_tabs.contains(&tab_id))
        {
            return None;
        }
        self.workspace
            .active_tab()
            .and_then(|tab_id| self.editors.get(&tab_id).cloned())
    }

    fn is_tab_dirty(&self, tab_id: EditorTabId, cx: &mut Context<Self>) -> bool {
        self.editors
            .get(&tab_id)
            .map(|editor| editor.read(cx).buffer().is_dirty())
            .or_else(|| self.workspace.buffer(tab_id).map(|buffer| buffer.is_dirty()))
            .unwrap_or(false)
    }

    fn remote_actions_ready(&self) -> bool {
        matches!(self.load_state, IdeLoadState::Ready)
    }

    fn remote_action_unavailable_message(&self) -> String {
        match &self.load_state {
            IdeLoadState::Empty => self.labels.no_project.clone(),
            IdeLoadState::Loading => self.labels.loading_project.clone(),
            IdeLoadState::Ready => String::new(),
            IdeLoadState::Error(message) => {
                format!("{}: {message}", self.labels.open_failed)
            }
            IdeLoadState::Disconnected => self.labels.disconnected_overlay.clone(),
        }
    }

    fn ensure_remote_actions_ready(&mut self, cx: &mut Context<Self>) -> bool {
        if self.remote_actions_ready() {
            return true;
        }
        self.last_error = Some(self.remote_action_unavailable_message());
        cx.notify();
        false
    }
}

fn ai_code_snippet_around_start(text: &str) -> (Option<String>, usize) {
    const MAX_LINES: usize = 21;
    const MAX_CHARS: usize = 4000;
    let snippet = text.lines().take(MAX_LINES).collect::<Vec<_>>().join("\n");
    if snippet.trim().is_empty() {
        return (None, 1);
    }
    if snippet.len() <= MAX_CHARS {
        return (Some(snippet), 1);
    }
    let end = snippet
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= MAX_CHARS)
        .last()
        .unwrap_or(0);
    (Some(format!("{}\n... (truncated)", &snippet[..end])), 1)
}

fn ide_plugin_file_path(location: &IdeLocation) -> String {
    match location {
        IdeLocation::Remote { path, .. } => path.clone(),
        IdeLocation::Local { path } => path.to_string_lossy().into_owned(),
    }
}
