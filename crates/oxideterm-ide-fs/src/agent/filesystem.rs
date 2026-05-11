impl NodeAgentIdeFileSystem {
    pub fn new(router: NodeRouter, mode: NodeAgentMode) -> Self {
        Self {
            sftp: NodeSftpIdeFileSystem::new(router.clone()),
            router,
            registry: Arc::new(AgentRegistry::default()),
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
        self.ensure_agent(&NodeId::new(node_id.into())).await
    }

    pub async fn refresh_agent_status(&self, node_id: impl Into<String>) -> AgentStatus {
        if self.mode == NodeAgentMode::Disabled {
            self.set_status(AgentStatus::SftpFallback);
            return AgentStatus::SftpFallback;
        }

        let node_id = NodeId::new(node_id.into());
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
        let resolved = self
            .router
            .resolve_connection(&node_id)
            .await
            .map_err(|error| IdeFileError::new(IdeFileErrorKind::Other, error.to_string()))?;
        self.registry.remove(&resolved.connection_id).await;
        let remote_path = remote_agent_path(&resolved.handle)
            .await
            .map_err(|error| IdeFileError::new(IdeFileErrorKind::Other, error.to_string()))?;
        resolved
            .handle
            .run_command(
                &format!("rm -f -- '{}'", shell_single_quote(&remote_path)),
                Duration::from_secs(15),
                2048,
            )
            .await
            .map_err(|error| IdeFileError::new(IdeFileErrorKind::Other, error.to_string()))?;
        self.set_status(AgentStatus::SftpFallback);
        Ok(())
    }

    pub async fn open_project(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
    ) -> Result<IdeProjectInfo, IdeFileError> {
        let node_id = node_id.into();
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
        self.sftp.check_file(node_id, path).await
    }

    pub async fn batch_stat(
        &self,
        node_id: impl Into<String>,
        paths: Vec<String>,
    ) -> Result<Vec<Option<IdePathStat>>, IdeFileError> {
        self.sftp.batch_stat(node_id, paths).await
    }

    async fn agent_session(&self, node_id: &NodeId) -> Option<Arc<AgentSession>> {
        if self.mode == NodeAgentMode::Disabled {
            self.set_status(AgentStatus::SftpFallback);
            return None;
        }
        if self.mode == NodeAgentMode::Enabled {
            let _ = self.ensure_agent(node_id).await;
        }

        let resolved = self.router.resolve_connection(node_id).await.ok()?;
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
        if let Ok(resolved) = self.router.resolve_connection(node_id).await
            && let Some(session) = self.registry.get(&resolved.connection_id)
            && session.is_alive()
        {
            let status = session.status();
            self.set_status(status.clone());
            return status;
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
        let resolved = self.router.resolve_connection(node_id).await?;
        let arch = detect_arch(&resolved.handle).await?;
        let remote_path = remote_agent_path(&resolved.handle).await?;
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

        let channel = resolved.handle.open_exec_channel().await?;
        let transport = AgentTransport::new(channel, &remote_path).await?;
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
        let resolved = self.router.resolve_connection(node_id).await?;
        if let Some(session) = self.registry.get(&resolved.connection_id) {
            if session.is_alive() {
                return Ok(session.status());
            }
            self.registry
                .remove_without_shutdown(&resolved.connection_id);
        }

        let arch = detect_arch(&resolved.handle).await?;
        let remote_path = remote_agent_path(&resolved.handle).await?;
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
            if let Some(session) = self.agent_session(&node_id).await {
                match session.read_file(&path).await {
                    Ok(result) => return Ok(ide_file_data_from_agent(result)),
                    Err(error) => {
                        warn!("[ide-agent] read via agent failed, falling back to SFTP: {error}");
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
                        warn!("[ide-agent] stat via agent failed, falling back to SFTP: {error}");
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
                            "[ide-agent] directory listing via agent failed, falling back to SFTP: {error}"
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
            if mode == WriteMode::CreateNew {
                return self
                    .sftp
                    .write_file(location, text, expected_version, mode)
                    .await;
            }

            if let Some(session) = self.agent_session(&node_id).await {
                let expect_hash = expected_version.and_then(|version| version.etag.as_deref());
                match session.write_file(&path, text, expect_hash).await {
                    Ok(result) => return Ok(version_from_agent_write(&result)),
                    Err(AgentError::Rpc { code, message })
                        if is_agent_conflict_parts(code, &message) =>
                    {
                        return Err(IdeFileError::new(IdeFileErrorKind::Conflict, message));
                    }
                    Err(error) => {
                        self.set_status(AgentStatus::Failed {
                            reason: error.to_string(),
                        });
                        return Err(map_agent_error(error));
                    }
                }
            }

            self.sftp
                .write_file(location, text, expected_version, mode)
                .await
        })
    }
}
