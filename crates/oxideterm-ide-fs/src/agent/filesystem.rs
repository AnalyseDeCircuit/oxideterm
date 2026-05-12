impl NodeAgentIdeFileSystem {
    pub fn new(router: NodeRouter, mode: NodeAgentMode) -> Self {
        Self {
            sftp: NodeSftpIdeFileSystem::new(router.clone()),
            router,
            registry: Arc::new(AgentRegistry::default()),
            ide_consumers: Arc::new(DashMap::new()),
            mode,
            status: Arc::new(RwLock::new(AgentStatus::SftpFallback)),
            deploy_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn set_mode(&mut self, mode: NodeAgentMode) {
        self.mode = mode;
        if mode == NodeAgentMode::Disabled {
            self.set_status(AgentStatus::SftpFallback);
        }
    }

    pub fn status(&self) -> AgentStatus {
        self.status
            .read()
            .map(|status| status.clone())
            .unwrap_or(AgentStatus::SftpFallback)
    }

    pub async fn deploy_agent_for_node(&self, node_id: impl Into<String>) -> AgentStatus {
        let node_id = NodeId::new(node_id.into());
        let _ = self.ensure_ide_session_for_node(&node_id).await;
        self.ensure_agent(&node_id).await
    }

    pub async fn refresh_agent_status(&self, node_id: impl Into<String>) -> AgentStatus {
        if self.mode == NodeAgentMode::Disabled {
            self.set_status(AgentStatus::SftpFallback);
            return AgentStatus::SftpFallback;
        }

        let node_id = NodeId::new(node_id.into());
        let _ = self.ensure_ide_session_for_node(&node_id).await;
        let status = match self.probe_agent_status(&node_id).await {
            Ok(status) => status,
            Err(error) => AgentStatus::Failed {
                reason: error.to_string(),
            },
        };
        self.set_status(status.clone());
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
        self.set_status(AgentStatus::SftpFallback);
        Ok(())
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
            self.set_status(AgentStatus::SftpFallback);
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
    ) -> Result<Option<mpsc::Receiver<AgentWatchEvent>>, IdeFileError> {
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
            self.set_status(AgentStatus::SftpFallback);
            return Ok(None);
        }

        session
            .watch_start(&path, ignore)
            .await
            .map_err(ide_error_from_agent_error)?;
        Ok(session.take_watch_rx().await)
    }

    pub async fn stop_watch_directory(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
    ) -> Result<(), IdeFileError> {
        let node_id = node_id.into();
        let path = path.into();
        // Tauri's IDE cleanup invalidates an existing agent/watch owner; it
        // never opens a fresh node route just to unsubscribe. Keep this path
        // cleanup-only so closing a tab or disconnecting a subtree cannot
        // revive an IDE consumer after the user intentionally tore it down.
        let Some(lease) = self.ide_consumers.get(&node_id).map(|entry| entry.clone()) else {
            return Ok(());
        };
        let Some(session) = self.registry.get(&lease.connection_id) else {
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
        let node_id = NodeId::new(node_id.into());
        let pattern = pattern.into();
        let root_path = root_path.into();
        self.ensure_ide_session_for_node(&node_id).await?;
        if let Some(session) = self.agent_session(&node_id).await {
            match session
                .grep(&pattern, &root_path, case_sensitive, max_results)
                .await
            {
                Ok(matches) => {
                    return Ok(matches
                        .into_iter()
                        .map(|hit| search_match_from_agent(hit, &pattern, case_sensitive))
                        .collect());
                }
                Err(error) => {
                    warn!(
                        "[ide-agent] grep via agent failed ({}), falling back to exec grep",
                        agent_error_log_label(&error)
                    );
                    self.set_status(AgentStatus::SftpFallback);
                }
            }
        }

        self.grep_project_via_exec(&node_id, &pattern, &root_path, max_results)
            .await
    }

    async fn grep_project_via_exec(
        &self,
        node_id: &NodeId,
        pattern: &str,
        root_path: &str,
        max_results: u32,
    ) -> Result<Vec<IdeSearchMatch>, IdeFileError> {
        if pattern.len() > 8192 {
            return Err(IdeFileError::new(
                IdeFileErrorKind::Unsupported,
                "Search query too long",
            ));
        }
        let resolved = self
            .acquire_ide_connection(node_id)
            .await
            .map_err(crate::node_sftp::map_route_error)?;
        let escaped_query = regex_escape_for_basic_grep(pattern);
        let command = format!(
            "cd {} && grep -rn -I {} --color=never -- -e '{}' . 2>/dev/null | head -{}",
            shell_cd_arg(root_path),
            grep_include_patterns(),
            shell_single_quote(&escaped_query),
            max_results
        );
        let output = resolved
            .handle
            .run_command(&command, Duration::from_secs(30), 256 * 1024)
            .await
            .map_err(|error| ide_error_from_agent_message(error.to_string()))?;
        Ok(parse_grep_output(&output, pattern, false))
    }

    pub fn release_ide_consumer(&self, node_id: &str) {
        if let Some((_, lease)) = self.ide_consumers.remove(node_id) {
            self.router
                .release_consumer(&lease.connection_id, &lease.consumer);
        }
    }

    pub fn release_all_ide_consumers(&self) {
        let leases = self
            .ide_consumers
            .iter()
            .map(|entry| entry.key().clone())
            .filter_map(|node_id| self.ide_consumers.remove(&node_id).map(|(_, lease)| lease))
            .collect::<Vec<_>>();
        for lease in leases {
            self.router
                .release_consumer(&lease.connection_id, &lease.consumer);
        }
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
        let consumer = ConnectionConsumer::Ide(node_id.0.clone());
        let resolved = self
            .router
            .acquire_connection_wait(node_id, consumer.clone(), Duration::from_secs(15))
            .await?;
        let lease = IdeConnectionLease {
            connection_id: resolved.connection_id.clone(),
            consumer,
        };
        if let Some(existing) = self.ide_consumers.insert(node_id.0.clone(), lease.clone())
            && existing != lease
        {
            self.router
                .release_consumer(&existing.connection_id, &existing.consumer);
        }
        Ok(resolved)
    }

    async fn agent_session(&self, node_id: &NodeId) -> Option<Arc<AgentSession>> {
        if self.mode == NodeAgentMode::Disabled {
            self.set_status(AgentStatus::SftpFallback);
            return None;
        }
        if self.mode == NodeAgentMode::Enabled {
            let _ = self.ensure_agent(node_id).await;
        }

        let resolved = self.acquire_ide_connection(node_id).await.ok()?;
        let session = self.registry.get(&resolved.connection_id)?;
        if session.is_alive() {
            self.set_status(session.status());
            Some(session)
        } else {
            self.registry
                .remove_without_shutdown(&resolved.connection_id);
            self.set_status(AgentStatus::SftpFallback);
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
                self.set_status(status.clone());
                return status;
            }
            self.registry.remove(&resolved.connection_id).await;
        }

        self.set_status(AgentStatus::Deploying);
        let status = match self.deploy_agent(node_id).await {
            Ok(status) => status,
            Err(error) => AgentStatus::Failed {
                reason: error.to_string(),
            },
        };
        self.set_status(status.clone());
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
        self.registry
            .register(resolved.connection_id, AgentSession::new(transport, info));
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

    fn set_status(&self, status: AgentStatus) {
        if let Ok(mut current) = self.status.write() {
            *current = status;
        }
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

fn grep_include_patterns() -> &'static str {
    "--include='*.ts' --include='*.tsx' --include='*.js' --include='*.jsx' --include='*.json' --include='*.rs' --include='*.toml' --include='*.md' --include='*.txt' --include='*.py' --include='*.go' --include='*.java' --include='*.c' --include='*.cpp' --include='*.h' --include='*.css' --include='*.scss' --include='*.html' --include='*.vue' --include='*.svelte' --include='*.yaml' --include='*.yml' --include='*.sh' --include='*.bash'"
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
                        self.set_status(AgentStatus::SftpFallback);
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
                        self.set_status(AgentStatus::SftpFallback);
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
                        self.set_status(AgentStatus::SftpFallback);
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
                        self.set_status(AgentStatus::SftpFallback);
                    }
                }
            }

            self.sftp
                .write_file(location, text, expected_version, mode)
                .await
        })
    }
}
