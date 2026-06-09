impl NodeAgentIdeFileSystem {
    pub fn new(router: NodeRouter, mode: NodeAgentMode) -> Self {
        Self {
            sftp: NodeSftpIdeFileSystem::new(router.clone()),
            router,
            registry: Arc::new(AgentRegistry::default()),
            ide_sessions: Arc::new(DashMap::new()),
            mode,
            agent_statuses: Arc::new(DashMap::new()),
            latest_agent_status: Arc::new(DashMap::new()),
            watch_subscriptions: Arc::new(DashMap::new()),
            deploy_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn set_mode(&mut self, mode: NodeAgentMode) {
        self.mode = mode;
        if mode == NodeAgentMode::Disabled {
            self.agent_statuses.clear();
            self.latest_agent_status.clear();
            self.watch_subscriptions.clear();
        }
    }

    pub fn status(&self) -> AgentStatus {
        self.status_for_node(None)
    }

    pub fn status_for_node(&self, node_id: Option<&str>) -> AgentStatus {
        let Some(node_id) = node_id else {
            return AgentStatus::SftpFallback;
        };
        let Some(key) = self.latest_agent_status.get(node_id) else {
            return AgentStatus::SftpFallback;
        };
        self.agent_statuses
            .get(key.value())
            .map(|status| status.value().clone())
            .unwrap_or(AgentStatus::SftpFallback)
    }

    pub async fn deploy_agent_for_node(&self, node_id: impl Into<String>) -> AgentStatus {
        let node_id = NodeId::new(node_id.into());
        let _ = self.ensure_ide_session_for_node(&node_id).await;
        self.ensure_agent(&node_id).await
    }

    pub async fn refresh_agent_status(&self, node_id: impl Into<String>) -> AgentStatus {
        let node_id = NodeId::new(node_id.into());
        if self.mode == NodeAgentMode::Disabled {
            self.set_status_for_node(&node_id, None, AgentStatus::SftpFallback);
            return AgentStatus::SftpFallback;
        }

        let _ = self.ensure_ide_session_for_node(&node_id).await;
        let status = match self.probe_agent_status(&node_id).await {
            Ok(status) => status,
            Err(error) => AgentStatus::Failed {
                reason: error.to_string(),
            },
        };
        self.set_status_for_node(&node_id, None, status.clone());
        status
    }

    pub async fn remove_agent_for_node(
        &self,
        node_id: impl Into<String>,
    ) -> Result<(), IdeFileError> {
        let node_id = NodeId::new(node_id.into());
        self.ensure_ide_session_for_node(&node_id).await?;
        let resolved = self
            .acquire_ide_connection(&node_id)
            .await
            .map_err(crate::node_sftp::map_route_error)?;
        self.registry.remove(&resolved.connection_id).await;
        let remote_path = remote_agent_remove_path(&resolved.handle)
            .await
            .map_err(ide_error_from_agent_error)?;
        resolved
            .handle
            .run_command(
                &format!("rm -f -- {}", shell_path_arg(&remote_path)),
                Duration::from_secs(15),
                2048,
            )
            .await
            .map_err(|error| ide_error_from_agent_message(error.to_string()))?;
        self.set_status_for_node(&node_id, Some(&resolved.connection_id), AgentStatus::SftpFallback);
        Ok(())
    }

    pub async fn node_agent_read_file(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
    ) -> Result<ReadFileResult, NodeAgentRpcError> {
        let node_id = NodeId::new(node_id.into());
        let path = path.into();
        let session = self.node_agent_rpc_session(&node_id).await?;
        session
            .read_file(&path)
            .await
            .map_err(node_agent_rpc_error_from_agent)
    }

    pub async fn node_agent_write_file(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
        content: impl Into<String>,
        expect_hash: Option<&str>,
    ) -> Result<WriteFileResult, NodeAgentRpcError> {
        let node_id = NodeId::new(node_id.into());
        let path = path.into();
        let content = content.into();
        let session = self.node_agent_rpc_session(&node_id).await?;
        session
            .write_file(&path, &content, expect_hash)
            .await
            .map_err(node_agent_rpc_error_from_agent)
    }

    pub async fn node_agent_symbol_index(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
        max_files: Option<u32>,
    ) -> Result<SymbolIndexResult, NodeAgentRpcError> {
        let node_id = NodeId::new(node_id.into());
        let path = path.into();
        let session = self.node_agent_rpc_session(&node_id).await?;
        session
            .symbol_index(&path, max_files)
            .await
            .map_err(node_agent_rpc_error_from_agent)
    }

    pub async fn node_agent_symbol_complete(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
        prefix: impl Into<String>,
        limit: Option<u32>,
    ) -> Result<Vec<SymbolInfo>, NodeAgentRpcError> {
        let node_id = NodeId::new(node_id.into());
        let path = path.into();
        let prefix = prefix.into();
        let session = self.node_agent_rpc_session(&node_id).await?;
        session
            .symbol_complete(&path, &prefix, limit)
            .await
            .map_err(node_agent_rpc_error_from_agent)
    }

    pub async fn node_agent_symbol_definitions(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Vec<SymbolInfo>, NodeAgentRpcError> {
        let node_id = NodeId::new(node_id.into());
        let path = path.into();
        let name = name.into();
        let session = self.node_agent_rpc_session(&node_id).await?;
        session
            .symbol_definitions(&path, &name)
            .await
            .map_err(node_agent_rpc_error_from_agent)
    }

    pub async fn open_project(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
    ) -> Result<IdeProjectInfo, IdeFileError> {
        let node_id = node_id.into();
        self.ensure_ide_session_for_node(&NodeId::new(node_id.clone()))
            .await?;
        if self.mode == NodeAgentMode::Enabled {
            let _ = self.ensure_agent(&NodeId::new(node_id.clone())).await;
        } else {
            self.set_status_for_node(&NodeId::new(node_id.clone()), None, AgentStatus::SftpFallback);
        }
        self.sftp.open_project(node_id, path).await
    }

    pub async fn check_file(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
    ) -> Result<oxideterm_ide_core::IdeFileCheck, IdeFileError> {
        let node_id = node_id.into();
        self.ensure_ide_session_for_node(&NodeId::new(node_id.clone()))
            .await?;
        self.sftp.check_file(node_id, path).await
    }

    pub async fn batch_stat(
        &self,
        node_id: impl Into<String>,
        paths: Vec<String>,
    ) -> Result<Vec<Option<IdePathStat>>, IdeFileError> {
        let node_id = node_id.into();
        self.ensure_ide_session_for_node(&NodeId::new(node_id.clone()))
            .await?;
        self.sftp.batch_stat(node_id, paths).await
    }

    pub async fn watch_directory(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
        ignore: Vec<String>,
    ) -> Result<Option<IdeWatchSubscription>, IdeFileError> {
        if self.mode == NodeAgentMode::Disabled {
            return Ok(None);
        }
        let node_id = NodeId::new(node_id.into());
        let path = path.into();
        let resolved = self
            .acquire_ide_connection(&node_id)
            .await
            .map_err(crate::node_sftp::map_route_error)?;
        let Some(session) = self.registry.get(&resolved.connection_id) else {
            return Ok(None);
        };
        if !session.is_alive() {
            self.registry.remove_without_shutdown(&resolved.connection_id);
            self.set_status_for_node(
                &node_id,
                Some(&resolved.connection_id),
                AgentStatus::SftpFallback,
            );
            return Ok(None);
        }

        let key = IdeWatchKey::new(node_id.0.clone(), normalize_agent_watch_path(&path));
        if let Some(shared) = self.watch_subscriptions.get(&key)
            && shared.connection_id == resolved.connection_id
        {
            return Ok(Some(IdeWatchSubscription {
                rx: shared.events_tx.subscribe(),
            }));
        }

        session
            .watch_start(&path, ignore)
            .await
            .map_err(ide_error_from_agent_error)?;
        let (events_tx, _) = broadcast::channel::<IdeWatchEvent>(1024);
        let shared = Arc::new(IdeWatchShared {
            connection_id: resolved.connection_id.clone(),
            events_tx,
        });
        self.watch_subscriptions.insert(key.clone(), shared.clone());
        spawn_watch_dispatcher(key, shared.clone(), session.subscribe_watch_events());
        Ok(Some(IdeWatchSubscription {
            rx: shared.events_tx.subscribe(),
        }))
    }

    pub async fn stop_watch_directory(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
    ) -> Result<(), IdeFileError> {
        let node_id = node_id.into();
        let path = path.into();
        let key = IdeWatchKey::new(node_id.clone(), normalize_agent_watch_path(&path));
        self.watch_subscriptions.remove(&key);
        // Tauri's IDE cleanup invalidates an existing agent/watch owner; it
        // never opens a fresh node route just to unsubscribe. Keep this path
        // cleanup-only so closing a tab or disconnecting a subtree cannot
        // revive an IDE consumer after the user intentionally tore it down.
        let Some(connection_id) = self
            .ide_sessions
            .get(&node_id)
            .and_then(|entry| entry.connection_id())
        else {
            return Ok(());
        };
        let Some(session) = self.registry.get(&connection_id) else {
            return Ok(());
        };
        session
            .watch_stop(&path)
            .await
            .map_err(ide_error_from_agent_error)
    }

    pub async fn delete_item(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
        recursive: bool,
    ) -> Result<(), IdeFileError> {
        self.sftp.delete_item(node_id, path, recursive).await
    }

    pub async fn create_file(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
    ) -> Result<SavedFileVersion, IdeFileError> {
        self.sftp.create_file(node_id, path).await
    }

    pub async fn create_folder(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
    ) -> Result<(), IdeFileError> {
        self.sftp.create_folder(node_id, path).await
    }

    pub async fn rename_item(
        &self,
        node_id: impl Into<String>,
        old_path: impl Into<String>,
        new_path: impl Into<String>,
    ) -> Result<(), IdeFileError> {
        self.sftp.rename_item(node_id, old_path, new_path).await
    }

    pub async fn grep_project(
        &self,
        node_id: impl Into<String>,
        pattern: impl Into<String>,
        root_path: impl Into<String>,
        case_sensitive: bool,
        max_results: u32,
    ) -> Result<Vec<IdeSearchMatch>, IdeFileError> {
        let pattern = pattern.into();
        let root_path = root_path.into();
        self.search_project(
            node_id,
            IdeSearchQuery {
                pattern,
                root_path,
                case_sensitive,
                regex: false,
                include_globs: oxideterm_ide_core::tauri_project_search_include_globs(),
                exclude_globs: Vec::new(),
                include_hidden: false,
                max_results,
                stale_token: 0,
            },
        )
        .await
    }

    pub async fn search_project(
        &self,
        node_id: impl Into<String>,
        query: IdeSearchQuery,
    ) -> Result<Vec<IdeSearchMatch>, IdeFileError> {
        let node_id = NodeId::new(node_id.into());
        self.ensure_ide_session_for_node(&node_id).await?;
        if let Some(session) = self.agent_session(&node_id).await {
            match session
                .grep(
                    &query.pattern,
                    &query.root_path,
                    query.case_sensitive,
                    query.max_results,
                )
                .await
            {
                Ok(matches) => {
                    return Ok(matches
                        .into_iter()
                        .map(|hit| {
                            search_match_from_agent(hit, &query.pattern, query.case_sensitive)
                        })
                        .collect());
                }
                Err(error) => {
                    warn!(
                        "[ide-agent] grep via agent failed ({}), falling back to exec grep",
                        agent_error_log_label(&error)
                    );
                    self.set_status_for_node(&node_id, None, AgentStatus::SftpFallback);
                }
            }
        }

        self.grep_project_via_exec(&node_id, &query)
            .await
    }

    async fn grep_project_via_exec(
        &self,
        node_id: &NodeId,
        query: &IdeSearchQuery,
    ) -> Result<Vec<IdeSearchMatch>, IdeFileError> {
        if query.pattern.len() > 8192 {
            return Err(IdeFileError::new(
                IdeFileErrorKind::Unsupported,
                "Search query too long",
            ));
        }
        if query.regex {
            return Err(IdeFileError::new(
                IdeFileErrorKind::Unsupported,
                "Regex project search requires the remote agent",
            ));
        }
        let resolved = self
            .acquire_ide_connection(node_id)
            .await
            .map_err(crate::node_sftp::map_route_error)?;
        let escaped_query = regex_escape_for_basic_grep(&query.pattern);
        let include_patterns = grep_include_patterns(&query.include_globs);
        let command = format!(
            "cd {} && grep -rn -I {} --color=never -- -e '{}' . 2>/dev/null | head -{}",
            shell_cd_arg(&query.root_path),
            include_patterns,
            shell_single_quote(&escaped_query),
            query.max_results
        );
        let output = resolved
            .handle
            .run_command(&command, Duration::from_secs(30), 256 * 1024)
            .await
            .map_err(|error| ide_error_from_agent_message(error.to_string()))?;
        Ok(parse_grep_output(&output, &query.pattern, false))
    }

    pub fn close_ide_session(&self, node_id: &str) {
        if let Some((_, session)) = self.ide_sessions.remove(node_id) {
            session.close();
        }
    }

    pub fn close_all_ide_sessions(&self) {
        let sessions = self
            .ide_sessions
            .iter()
            .map(|entry| entry.key().clone())
            .filter_map(|node_id| self.ide_sessions.remove(&node_id).map(|(_, session)| session))
            .collect::<Vec<_>>();
        for session in sessions {
            session.close();
        }
    }

    pub fn release_ide_consumer(&self, node_id: &str) {
        self.close_ide_session(node_id);
    }

    pub fn release_all_ide_consumers(&self) {
        self.close_all_ide_sessions();
    }

    async fn ensure_ide_session_for_node(&self, node_id: &NodeId) -> Result<(), IdeFileError> {
        self.acquire_ide_connection(node_id)
            .await
            .map(|_| ())
            .map_err(crate::node_sftp::map_route_error)
    }

    async fn acquire_ide_connection(
        &self,
        node_id: &NodeId,
    ) -> Result<ResolvedConnection, RouteError> {
        let session = self.ide_session_for_node(node_id);
        session.acquire_connection().await
    }

    async fn node_agent_rpc_session(
        &self,
        node_id: &NodeId,
    ) -> Result<Arc<AgentSession>, NodeAgentRpcError> {
        if self.mode == NodeAgentMode::Disabled {
            self.set_status_for_node(node_id, None, AgentStatus::SftpFallback);
            return Err(NodeAgentRpcError::Unavailable(
                "Agent deployment is disabled".to_string(),
            ));
        }
        if self.mode == NodeAgentMode::Enabled {
            let status = self.ensure_agent(node_id).await;
            if !status.is_ready() {
                return Err(NodeAgentRpcError::Unavailable(format!(
                    "Agent is not ready: {status:?}"
                )));
            }
        }

        let resolved = self
            .acquire_ide_connection(node_id)
            .await
            .map_err(|error| NodeAgentRpcError::Unavailable(error.to_string()))?;
        let Some(session) = self.registry.get(&resolved.connection_id) else {
            self.set_status_for_node(node_id, Some(&resolved.connection_id), AgentStatus::SftpFallback);
            return Err(NodeAgentRpcError::Unavailable(
                "Agent not deployed".to_string(),
            ));
        };
        if session.is_alive() {
            self.set_status_for_node(node_id, Some(&resolved.connection_id), session.status());
            Ok(session)
        } else {
            self.registry
                .remove_without_shutdown(&resolved.connection_id);
            self.set_status_for_node(
                node_id,
                Some(&resolved.connection_id),
                AgentStatus::SftpFallback,
            );
            Err(NodeAgentRpcError::Unavailable(
                "Agent channel closed".to_string(),
            ))
        }
    }

    async fn agent_session(&self, node_id: &NodeId) -> Option<Arc<AgentSession>> {
        if self.mode == NodeAgentMode::Disabled {
            self.set_status_for_node(node_id, None, AgentStatus::SftpFallback);
            return None;
        }
        if self.mode == NodeAgentMode::Enabled {
            let _ = self.ensure_agent(node_id).await;
        }

        let resolved = self.acquire_ide_connection(node_id).await.ok()?;
        let session = self.registry.get(&resolved.connection_id)?;
        if session.is_alive() {
            self.set_status_for_node(node_id, Some(&resolved.connection_id), session.status());
            Some(session)
        } else {
            self.registry
                .remove_without_shutdown(&resolved.connection_id);
            self.set_status_for_node(
                node_id,
                Some(&resolved.connection_id),
                AgentStatus::SftpFallback,
            );
            None
        }
    }

    async fn ensure_agent(&self, node_id: &NodeId) -> AgentStatus {
        let _guard = self.deploy_lock.lock().await;
        if let Ok(resolved) = self.acquire_ide_connection(node_id).await
            && let Some(session) = self.registry.get(&resolved.connection_id)
        {
            if session.is_alive() {
                let status = session.status();
                self.set_status_for_node(node_id, Some(&resolved.connection_id), status.clone());
                return status;
            }
            self.registry.remove(&resolved.connection_id).await;
        }

        self.set_status_for_node(node_id, None, AgentStatus::Deploying);
        let status = match self.deploy_agent(node_id).await {
            Ok(status) => status,
            Err(error) => AgentStatus::Failed {
                reason: error.to_string(),
            },
        };
        self.set_status_for_node(node_id, None, status.clone());
        status
    }

    async fn deploy_agent(&self, node_id: &NodeId) -> Result<AgentStatus, AgentError> {
        let resolved = self.acquire_ide_connection(node_id).await?;
        let arch = detect_arch(&resolved.handle).await?;
        let remote_path = remote_agent_path();
        let target = arch_to_target(&arch);
        let install_state = probe_remote_install(&resolved.handle, &remote_path).await;

        match target {
            Ok(target) => {
                if !matches!(install_state, RemoteAgentInstallState::Current) {
                    let binary = resolve_agent_binary(target)?;
                    upload_agent(
                        &resolved.handle,
                        &self.router,
                        node_id,
                        &remote_path,
                        &binary,
                    )
                    .await?;
                }
            }
            Err(AgentError::UnsupportedArch(_)) => match install_state {
                RemoteAgentInstallState::Missing => {
                    return Ok(AgentStatus::ManualUploadRequired { arch, remote_path });
                }
                RemoteAgentInstallState::Current => {}
                RemoteAgentInstallState::Incompatible(version) => {
                    return Ok(AgentStatus::ManualUpdateRequired {
                        arch,
                        remote_path,
                        current_agent_version: version.version,
                        current_compatibility_version: version.compatibility_version,
                        expected_compatibility_version: CURRENT_AGENT_COMPATIBILITY_VERSION,
                    });
                }
            },
            Err(error) => {
                return Err(error);
            }
        }

        let channel = resolved
            .handle
            .open_exec_channel()
            .await
            .map_err(|error| AgentError::StartFailed(format!("Channel open failed: {error}")))?;
        let transport = AgentTransport::new(channel, &remote_path)
            .await
            .map_err(|error| AgentError::StartFailed(error.to_string()))?;
        let info = handshake_agent(&transport).await?;
        let status = AgentStatus::Ready {
            version: info.version.clone(),
            arch: info.arch.clone(),
            pid: info.pid,
        };
        self.registry.register(
            resolved.connection_id.clone(),
            AgentSession::new(transport, info),
        );
        self.set_status_for_node(node_id, Some(&resolved.connection_id), status.clone());
        Ok(status)
    }

    async fn probe_agent_status(&self, node_id: &NodeId) -> Result<AgentStatus, AgentError> {
        let resolved = self.acquire_ide_connection(node_id).await?;
        if let Some(session) = self.registry.get(&resolved.connection_id) {
            // Mirrors Tauri's `node_agent_status`: the current connection's
            // agent session is authoritative even when the channel is already
            // closed, so the UI sees a failed agent instead of an install probe
            // result from the same node.
            return Ok(session.status());
        }

        let arch = detect_arch(&resolved.handle).await?;
        let remote_path = remote_agent_path();
        let install_state = probe_remote_install(&resolved.handle, &remote_path).await;
        match arch_to_target(&arch) {
            Ok(_) => Ok(AgentStatus::NotDeployed),
            Err(AgentError::UnsupportedArch(_)) => match install_state {
                RemoteAgentInstallState::Missing => {
                    Ok(AgentStatus::ManualUploadRequired { arch, remote_path })
                }
                RemoteAgentInstallState::Current => Ok(AgentStatus::NotDeployed),
                RemoteAgentInstallState::Incompatible(version) => {
                    Ok(AgentStatus::ManualUpdateRequired {
                        arch,
                        remote_path,
                        current_agent_version: version.version,
                        current_compatibility_version: version.compatibility_version,
                        expected_compatibility_version: CURRENT_AGENT_COMPATIBILITY_VERSION,
                    })
                }
            },
            Err(error) => Err(error),
        }
    }

    fn ide_session_for_node(&self, node_id: &NodeId) -> Arc<IdeRemoteSessionInner> {
        self.ide_sessions
            .entry(node_id.0.clone())
            .or_insert_with(|| {
                Arc::new(IdeRemoteSessionInner::new(
                    node_id.clone(),
                    self.router.clone(),
                ))
            })
            .clone()
    }

    fn set_status_for_node(
        &self,
        node_id: &NodeId,
        connection_id: Option<&str>,
        status: AgentStatus,
    ) {
        let connection_id = connection_id
            .map(ToOwned::to_owned)
            .or_else(|| {
                self.ide_sessions
                    .get(&node_id.0)
                    .and_then(|session| session.connection_id())
            })
            .unwrap_or_else(|| "<unresolved>".to_string());
        let key = AgentStatusKey {
            node_id: node_id.0.clone(),
            connection_id,
        };
        self.agent_statuses.insert(key.clone(), status);
        self.latest_agent_status.insert(node_id.0.clone(), key);
    }
}

fn node_agent_rpc_error_from_agent(error: AgentError) -> NodeAgentRpcError {
    match error {
        AgentError::Rpc { code, message } if is_agent_conflict_parts(code, &message) => {
            NodeAgentRpcError::Conflict(message)
        }
        AgentError::ChannelClosed
        | AgentError::Timeout(_)
        | AgentError::Route(_)
        | AgentError::StartFailed(_)
        | AgentError::Handshake(_)
        | AgentError::UnsupportedArch(_)
        | AgentError::BinaryNotFound(_) => NodeAgentRpcError::Unavailable(error.to_string()),
        other => NodeAgentRpcError::Other(other.to_string()),
    }
}

fn search_match_from_agent(
    hit: AgentGrepMatch,
    pattern: &str,
    case_sensitive: bool,
) -> IdeSearchMatch {
    let preview = hit.text.trim().chars().take(200).collect::<String>();
    let match_start = find_match_start(&preview, pattern, case_sensitive).unwrap_or(0);
    IdeSearchMatch {
        path: hit.path,
        line: hit.line,
        column: hit.column,
        preview,
        match_start,
        match_end: match_start.saturating_add(pattern.len()),
    }
}

fn parse_grep_output(output: &str, pattern: &str, case_sensitive: bool) -> Vec<IdeSearchMatch> {
    output
        .lines()
        .filter_map(|line| {
            let (path, rest) = line.split_once(':')?;
            let (line_number, content) = rest.split_once(':')?;
            let line = line_number.parse::<u32>().ok()?;
            let path = path.strip_prefix("./").unwrap_or(path).to_string();
            let preview = content.trim().chars().take(200).collect::<String>();
            let match_start = find_match_start(&preview, pattern, case_sensitive).unwrap_or(0);
            Some(IdeSearchMatch {
                path,
                line,
                column: match_start as u32,
                preview,
                match_start,
                match_end: match_start.saturating_add(pattern.len()),
            })
        })
        .collect()
}

fn find_match_start(text: &str, pattern: &str, case_sensitive: bool) -> Option<usize> {
    if case_sensitive {
        text.find(pattern)
    } else {
        text.to_lowercase().find(&pattern.to_lowercase())
    }
}

fn regex_escape_for_basic_grep(pattern: &str) -> String {
    pattern.chars().fold(String::new(), |mut escaped, ch| {
        if matches!(
            ch,
            '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\'
        ) {
            escaped.push('\\');
        }
        escaped.push(ch);
        escaped
    })
}

fn spawn_watch_dispatcher(
    key: IdeWatchKey,
    shared: Arc<IdeWatchShared>,
    mut rx: broadcast::Receiver<AgentWatchEvent>,
) {
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            let event_path = normalize_agent_watch_path(&event.path);
            if event_path != key.path
                && !event_path.starts_with(&format!("{}/", key.path.trim_end_matches('/')))
            {
                continue;
            }
            let _ = shared.events_tx.send(IdeWatchEvent {
                path: event.path,
                kind: event.kind,
            });
        }
    });
}

fn normalize_agent_watch_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed == "/" {
        "/".to_string()
    } else {
        trimmed.trim_end_matches('/').to_string()
    }
}

fn grep_include_patterns(globs: &[String]) -> String {
    globs
        .iter()
        .map(|glob| format!("--include={}", shell_single_quote(glob)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_cd_arg(path: &str) -> String {
    if path == "~" {
        "~".to_string()
    } else if let Some(rest) = path.strip_prefix("~/") {
        if rest.is_empty() {
            "~".to_string()
        } else {
            format!("~/'{}'", shell_single_quote(rest))
        }
    } else {
        format!("'{}'", shell_single_quote(path))
    }
}

impl AsyncIdeFileSystem for NodeAgentIdeFileSystem {
    fn capabilities(&self) -> FileSystemCapabilities {
        FileSystemCapabilities {
            atomic_write: true,
            directory_listing: true,
            conflict_detection: true,
        }
    }

    fn read_file<'a>(&'a self, location: &'a IdeLocation) -> IdeFsFuture<'a, IdeFileData> {
        Box::pin(async move {
            let (node_id, path) = remote_location(location)?;
            self.ensure_ide_session_for_node(&node_id).await?;
            if let Some(session) = self.agent_session(&node_id).await {
                match session.read_file(&path).await {
                    Ok(result) => return Ok(ide_file_data_from_agent(result)),
                    Err(error) => {
                        warn!(
                            "[ide-agent] read via agent failed ({}), falling back to SFTP",
                            agent_error_log_label(&error)
                        );
                        self.set_status_for_node(&node_id, None, AgentStatus::SftpFallback);
                    }
                }
            }
            self.sftp.read_file(location).await
        })
    }

    fn stat<'a>(&'a self, location: &'a IdeLocation) -> IdeFsFuture<'a, FileStat> {
        Box::pin(async move {
            let (node_id, path) = remote_location(location)?;
            self.ensure_ide_session_for_node(&node_id).await?;
            if let Some(session) = self.agent_session(&node_id).await {
                match session.stat(&path).await {
                    Ok(stat) if stat.exists => {
                        return Ok(FileStat {
                            version: version_from_agent_stat(&stat),
                            is_read_only: stat
                                .permissions
                                .as_deref()
                                .and_then(|raw| u32::from_str_radix(raw, 8).ok())
                                .map(|mode| mode & 0o200 == 0)
                                .unwrap_or(false),
                        });
                    }
                    Ok(_) => return Err(IdeFileError::new(IdeFileErrorKind::NotFound, path)),
                    Err(error) => {
                        warn!(
                            "[ide-agent] stat via agent failed ({}), falling back to SFTP",
                            agent_error_log_label(&error)
                        );
                        self.set_status_for_node(&node_id, None, AgentStatus::SftpFallback);
                    }
                }
            }
            self.sftp.stat(location).await
        })
    }

    fn list_dir<'a>(&'a self, location: &'a IdeLocation) -> IdeFsFuture<'a, Vec<FileTreeEntry>> {
        Box::pin(async move {
            let (node_id, path) = remote_location(location)?;
            self.ensure_ide_session_for_node(&node_id).await?;
            if let Some(session) = self.agent_session(&node_id).await {
                match session.list_dir(&path).await {
                    Ok(entries) => {
                        return Ok(entries
                            .into_iter()
                            .map(|entry| file_tree_entry_from_agent(&node_id, entry))
                            .collect());
                    }
                    Err(error) => {
                        warn!(
                            "[ide-agent] directory listing via agent failed ({}), falling back to SFTP",
                            agent_error_log_label(&error)
                        );
                        self.set_status_for_node(&node_id, None, AgentStatus::SftpFallback);
                    }
                }
            }
            self.sftp.list_dir(location).await
        })
    }

    fn write_file<'a>(
        &'a self,
        location: &'a IdeLocation,
        text: &'a str,
        expected_version: Option<&'a SavedFileVersion>,
        mode: WriteMode,
    ) -> IdeFsFuture<'a, SavedFileVersion> {
        Box::pin(async move {
            let (node_id, path) = remote_location(location)?;
            self.ensure_ide_session_for_node(&node_id).await?;
            if mode == WriteMode::CreateNew {
                return self
                    .sftp
                    .write_file(location, text, expected_version, mode)
                    .await;
            }

            let expect_hash = expected_version.and_then(|version| version.etag.as_deref());
            if should_write_via_agent(expected_version)
                && let Some(session) = self.agent_session(&node_id).await
            {
                match session.write_file(&path, text, expect_hash).await {
                    Ok(result) => return Ok(version_from_agent_write(&result)),
                    Err(AgentError::Rpc { code, message })
                        if is_agent_conflict_parts(code, &message) =>
                    {
                        return Err(IdeFileError::new(IdeFileErrorKind::Conflict, message));
                    }
                    Err(error) => {
                        warn!(
                            "[ide-agent] write via agent failed ({}), falling back to SFTP",
                            agent_error_log_label(&error)
                        );
                        self.set_status_for_node(&node_id, None, AgentStatus::SftpFallback);
                    }
                }
            }

            self.sftp
                .write_file(location, text, expected_version, mode)
                .await
        })
    }
}
