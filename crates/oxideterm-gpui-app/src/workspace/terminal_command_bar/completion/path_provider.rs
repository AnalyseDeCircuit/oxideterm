use super::*;

impl WorkspaceApp {
    pub(super) fn terminal_command_context_cwd(
        &self,
        pane_id: Option<PaneId>,
        tab_kind: Option<&TabKind>,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let registry_cwd = pane_id
            .and_then(|pane_id| self.tab_host.read(cx).panes().get(&pane_id))
            .and_then(|pane| pane.read(cx).current_working_directory());
        let inferred = pane_id
            .and_then(|pane_id| self.tab_host.read(cx).panes().get(&pane_id))
            .map(|pane| pane.read(cx).visible_text_snapshot())
            .and_then(|text| infer_ai_cwd(&text));
        registry_cwd.or(inferred).or_else(|| {
            tab_kind
                .is_some_and(|kind| *kind == TabKind::LocalTerminal)
                .then(|| {
                    std::env::current_dir()
                        .ok()
                        .map(|path| path.to_string_lossy().to_string())
                })
                .flatten()
        })
    }

    pub(super) fn terminal_command_path_suggestions(
        &self,
        parsed: &TerminalShellParseResult,
        active_arg_type: TerminalFigArgType,
        context: &TerminalCommandContext,
        cx: &mut Context<Self>,
    ) -> Vec<TerminalCommandSuggestion> {
        if !parsed.reliable
            || !should_run_terminal_path_provider(&parsed.current_token, active_arg_type)
        {
            return Vec::new();
        }
        let Some(parts) =
            normalize_terminal_path_token(&parsed.current_token, context.cwd.as_deref())
        else {
            return Vec::new();
        };
        let provider_scope_id = context.provider_scope_id();
        if context.is_remote_terminal() {
            let Some(node_id) = context.node_id.clone() else {
                return Vec::new();
            };
            let cache_key = terminal_path_cache_key(
                "remote",
                Some(provider_scope_id.as_str()),
                context.cwd.as_deref(),
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

        let cache_key = terminal_path_cache_key(
            "local",
            Some(provider_scope_id.as_str()),
            context.cwd.as_deref(),
            &parts.directory,
        );
        let entries = if let Some(entries) = get_cached_terminal_path_entries(&cache_key) {
            entries
        } else {
            let Ok(entries) = read_local_terminal_path_entries(&parts.directory) else {
                return Vec::new();
            };
            put_cached_terminal_path_entries(cache_key, entries.clone());
            entries
        };
        terminal_path_entries_to_suggestions(entries, &parts, parsed, active_arg_type)
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
        // A single-result handoff must stay bounded so abandoned UI tasks cannot queue data.
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let cache_key_for_task = cache_key.clone();
        runtime.spawn(async move {
            let result = tokio::time::timeout(std::time::Duration::from_millis(800), async {
                let shared = node_router
                    .acquire_sftp(&node_id)
                    .await
                    .map_err(|error| error.to_string())?;
                let first_result = {
                    let sftp = shared.lock().await;
                    sftp.list_dir_with_cwd(
                        &directory,
                        Some(ListFilter {
                            show_hidden: true,
                            pattern: None,
                            sort: SortOrder::Name,
                        }),
                    )
                    .await
                };
                let (_, entries) = match first_result {
                    Ok(entries) => entries,
                    Err(error) if error.is_channel_recoverable() => {
                        // Remote completion uses the same read-only SFTP
                        // semantics as Tauri node_sftp_list_dir: rebuild a
                        // stale channel once before dropping suggestions.
                        let rebuilt = node_router
                            .invalidate_and_reacquire_sftp(&node_id)
                            .await
                            .map_err(|route_error| route_error.to_string())?;
                        let sftp = rebuilt.lock().await;
                        sftp.list_dir_with_cwd(
                            &directory,
                            Some(ListFilter {
                                show_hidden: true,
                                pattern: None,
                                sort: SortOrder::Name,
                            }),
                        )
                        .await
                        .map_err(|retry_error| retry_error.to_string())?
                    }
                    Err(error) => return Err(error.to_string()),
                };
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
