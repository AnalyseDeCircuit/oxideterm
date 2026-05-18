impl WorkspaceApp {
    fn terminal_command_context_cwd(&self, cx: &mut Context<Self>) -> Option<String> {
        let inferred = self
            .active_pane_id()
            .and_then(|pane_id| self.panes.get(&pane_id))
            .map(|pane| pane.read(cx).visible_text_snapshot())
            .and_then(|text| infer_ai_cwd(&text));
        inferred.or_else(|| {
            self.active_tab()
                .is_some_and(|tab| tab.kind == TabKind::LocalTerminal)
                .then(|| {
                    std::env::current_dir()
                        .ok()
                        .map(|path| path.to_string_lossy().to_string())
                })
                .flatten()
        })
    }

    fn terminal_command_path_suggestions(
        &self,
        parsed: &TerminalShellParseResult,
        active_arg_type: TerminalFigArgType,
        cx: &mut Context<Self>,
    ) -> Vec<TerminalCommandSuggestion> {
        if !parsed.reliable
            || !should_run_terminal_path_provider(&parsed.current_token, active_arg_type)
        {
            return Vec::new();
        }
        let cwd = self.terminal_command_context_cwd(cx);
        let Some(parts) = normalize_terminal_path_token(&parsed.current_token, cwd.as_deref())
        else {
            return Vec::new();
        };
        let is_remote = self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::SshTerminal);
        if is_remote {
            let Some(node_id) = self.terminal_command_active_node_id() else {
                return Vec::new();
            };
            let cache_key = terminal_path_cache_key(
                "remote",
                Some(node_id.0.as_str()),
                cwd.as_deref(),
                &parts.directory,
            );
            if let Some(entries) = get_cached_terminal_path_entries(&cache_key) {
                return terminal_path_entries_to_suggestions(
                    entries,
                    &parts,
                    parsed,
                    active_arg_type,
                );
            }
            self.spawn_terminal_remote_path_completion(cache_key, node_id, parts.directory, cx);
            return Vec::new();
        }

        let cache_key = terminal_path_cache_key("local", None, cwd.as_deref(), &parts.directory);
        let entries = if let Some(entries) = get_cached_terminal_path_entries(&cache_key) {
            entries
        } else {
            let Ok(entries) = std::fs::read_dir(&parts.directory) else {
                return Vec::new();
            };
            let entries = entries
                .filter_map(Result::ok)
                .filter_map(|entry| {
                    let metadata = std::fs::symlink_metadata(entry.path()).ok()?;
                    Some(TerminalPathEntry {
                        name: entry.file_name().to_string_lossy().to_string(),
                        path: entry.path().to_string_lossy().to_string(),
                        is_directory: metadata.is_dir(),
                    })
                })
                .collect::<Vec<_>>();
            put_cached_terminal_path_entries(cache_key, entries.clone());
            entries
        };
        terminal_path_entries_to_suggestions(entries, &parts, parsed, active_arg_type)
    }

    fn terminal_command_active_node_id(&self) -> Option<NodeId> {
        let tab = self.active_tab()?;
        let pane_id = tab.active_pane_id?;
        let session_id = tab.root_pane.as_ref()?.session_id_for_pane(pane_id)?;
        self.terminal_ssh_nodes.get(&session_id).cloned()
    }

    fn spawn_terminal_remote_path_completion(
        &self,
        cache_key: String,
        node_id: NodeId,
        directory: String,
        cx: &mut Context<Self>,
    ) {
        if !mark_terminal_path_request_pending(cache_key.clone()) {
            return;
        }
        let node_router = self.node_router.clone();
        let runtime = self.forwarding_runtime.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let cache_key_for_task = cache_key.clone();
        runtime.spawn(async move {
            let result = tokio::time::timeout(std::time::Duration::from_millis(800), async {
                let shared = node_router
                    .acquire_sftp(&node_id)
                    .await
                    .map_err(|error| error.to_string())?;
                let sftp = shared.lock().await;
                let (_, entries) = sftp
                    .list_dir_with_cwd(
                        &directory,
                        Some(ListFilter {
                            show_hidden: true,
                            pattern: None,
                            sort: SortOrder::Name,
                        }),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                Ok::<Vec<TerminalPathEntry>, String>(
                    entries
                        .into_iter()
                        .map(|entry| TerminalPathEntry {
                            name: entry.name,
                            path: entry.path,
                            is_directory: entry.file_type == RemotePathFileType::Directory,
                        })
                        .collect(),
                )
            })
            .await
            .ok()
            .and_then(Result::ok);
            let _ = tx.send(result);
            clear_terminal_path_request_pending(&cache_key_for_task);
        });
        cx.spawn(async move |weak, cx| {
            let result = loop {
                match rx.try_recv() {
                    Ok(result) => break result,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        Timer::after(Duration::from_millis(16)).await;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break None,
                }
            };
            if let Some(entries) = result {
                put_cached_terminal_path_entries(cache_key, entries);
                let _ = weak.update(cx, |_this, cx| cx.notify());
            }
        })
        .detach();
    }
}

fn terminal_path_entries_to_suggestions(
    mut entries: Vec<TerminalPathEntry>,
    parts: &TerminalPathParts,
    parsed: &TerminalShellParseResult,
    active_arg_type: TerminalFigArgType,
) -> Vec<TerminalCommandSuggestion> {
    let wanted_directory = active_arg_type == TerminalFigArgType::Directory;
    let wanted_file = active_arg_type == TerminalFigArgType::File;
    let quoted = parsed.current_token.quote.is_some();
    entries.retain(|entry| {
        entry
            .name
            .to_lowercase()
            .starts_with(&parts.query.to_lowercase())
            && (!wanted_directory || entry.is_directory)
            && (!wanted_file || !entry.is_directory)
    });
    entries.sort_by(|left, right| {
        right
            .is_directory
            .cmp(&left.is_directory)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    entries
        .into_iter()
        .take(16)
        .map(|entry| {
            let suffix = if entry.is_directory { "/" } else { "" };
            let insert_text = format!(
                "{}{}{}",
                parts.display_prefix,
                escape_terminal_path_for_shell(&entry.name, quoted),
                suffix
            );
            TerminalCommandSuggestion {
                kind: if entry.is_directory {
                    TerminalCommandSuggestionKind::Directory
                } else {
                    TerminalCommandSuggestionKind::File
                },
                label: format!("{}{}", entry.name, suffix),
                insert_text,
                description: Some(entry.path),
                executable: false,
                replacement: parsed.current_token.start..parsed.current_token.end,
                group_label_key: "terminal.command_bar.group_path",
                source_label_key: "terminal.command_bar.source_path",
                score: if entry.is_directory {
                    560.0 + entry.name.len() as f64
                } else {
                    540.0 + entry.name.len() as f64
                },
                risk: None,
                inline_safe: true,
            }
        })
        .collect()
}

fn terminal_path_completion_cache() -> &'static std::sync::Mutex<TerminalPathCompletionCache> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<TerminalPathCompletionCache>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(TerminalPathCompletionCache::default()))
}

fn terminal_path_cache_key(
    scope: &str,
    node_id: Option<&str>,
    cwd: Option<&str>,
    directory: &str,
) -> String {
    format!(
        "{}::{}::{}::{}",
        scope,
        node_id.unwrap_or_default(),
        cwd.unwrap_or_default(),
        directory
    )
}

fn get_cached_terminal_path_entries(cache_key: &str) -> Option<Vec<TerminalPathEntry>> {
    const CACHE_TTL_MS: u128 = 12_000;
    let mut cache = terminal_path_completion_cache().lock().ok()?;
    let Some(entry) = cache.entries.get(cache_key) else {
        return None;
    };
    if entry.created_at.elapsed().as_millis() > CACHE_TTL_MS {
        cache.entries.remove(cache_key);
        return None;
    }
    Some(entry.entries.clone())
}

fn put_cached_terminal_path_entries(cache_key: String, entries: Vec<TerminalPathEntry>) {
    if let Ok(mut cache) = terminal_path_completion_cache().lock() {
        cache.entries.insert(
            cache_key,
            TerminalPathCacheEntry {
                created_at: std::time::Instant::now(),
                entries,
            },
        );
    }
}

fn mark_terminal_path_request_pending(cache_key: String) -> bool {
    terminal_path_completion_cache()
        .lock()
        .map(|mut cache| cache.pending.insert(cache_key))
        .unwrap_or(false)
}

fn clear_terminal_path_request_pending(cache_key: &str) {
    if let Ok(mut cache) = terminal_path_completion_cache().lock() {
        cache.pending.remove(cache_key);
    }
}
