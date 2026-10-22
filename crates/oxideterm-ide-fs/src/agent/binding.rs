use super::*;
use oxideterm_terminal::ShellChannel;

pub(super) struct RemoteBinding {
    pub id: String,
    pub owner_id: u64,
    pub terminal: bool,
    pub ssh_shell: bool,
    pub owns_transport: bool,
    pub audit: Option<oxideterm_audit::AuditContext>,
    pub session: Arc<AgentSession>,
}

impl NodeAgentIdeFileSystem {
    /// The returned owner cannot resolve paths through the node's SFTP or exec channels.
    pub async fn from_shell_channel(
        &self,
        channel: ShellChannel,
        access: RemoteAccess,
        progress: Option<watch::Sender<AgentInstallProgress>>,
    ) -> Result<(Self, String), IdeFileError> {
        let id = format!("shell:{}", channel.token);
        let audit = channel.audit_context();
        let session = Self::start_channel(IdeChannel::Terminal(channel), access, progress).await?;
        let cwd = session
            .transport
            .call("sys/cwd", serde_json::json!({}))
            .await
            .map_err(ide_error_from_agent_error)?;
        let cwd = cwd
            .as_str()
            .filter(|path| path.starts_with('/'))
            .ok_or_else(|| {
                IdeFileError::new(IdeFileErrorKind::Other, "Invalid shell working directory")
            })?
            .to_owned();
        let mut fs = self.scoped_owner();
        fs.mode = NodeAgentMode::Enabled;
        fs.access = access;
        fs.binding = Some(Arc::new(RemoteBinding {
            id,
            owner_id: fs.owner_id,
            terminal: true,
            ssh_shell: false,
            owns_transport: true,
            audit,
            session,
        }));
        Ok((fs, cwd))
    }

    async fn start_channel(
        channel: IdeChannel,
        access: RemoteAccess,
        progress: Option<watch::Sender<AgentInstallProgress>>,
    ) -> Result<Arc<AgentSession>, IdeFileError> {
        let transport = match access {
            RemoteAccess::Agent => AgentTransport::from_shell(channel, progress).await,
            RemoteAccess::Commands => AgentTransport::commands_terminal(channel).await,
            _ => {
                return Err(IdeFileError::new(
                    IdeFileErrorKind::Unsupported,
                    "Select commands or agent for a shell channel",
                ));
            }
        }
        .map_err(ide_error_from_agent_error)?;
        let info = handshake_agent(&transport)
            .await
            .map_err(ide_error_from_agent_error)?;
        Ok(Arc::new(AgentSession::new(transport, info)))
    }

    pub(super) fn file_audit_context(
        &self,
        node_id: &NodeId,
    ) -> Option<oxideterm_audit::AuditContext> {
        if let Some(binding) = &self.binding {
            return (binding.id == node_id.0)
                .then(|| binding.audit.clone())
                .flatten();
        }
        self.router.audit_context(node_id)
    }

    pub fn terminal_session_id(&self) -> Option<&str> {
        self.binding
            .as_ref()
            .filter(|s| s.terminal)
            .map(|s| s.id.as_str())
    }

    pub(super) async fn bound_session(&self, id: &str) -> Result<Arc<AgentSession>, IdeFileError> {
        let binding = self
            .binding
            .as_ref()
            .filter(|binding| binding.id == id && binding.session.is_alive())
            .ok_or_else(|| {
                IdeFileError::new(
                    IdeFileErrorKind::Disconnected,
                    "Remote IDE session is unavailable",
                )
            })?;
        if let Some(scope) = &self.scope {
            scope
                .acquire(&NodeId::new(id))
                .await
                .map_err(crate::node_sftp::map_route_error)?;
        }
        Ok(binding.session.clone())
    }

    pub(super) async fn rpc_stat(&self, id: &str, path: &str) -> Result<StatResult, IdeFileError> {
        let stat = self
            .bound_session(id)
            .await?
            .stat(path)
            .await
            .map_err(ide_error_from_agent_error)?;
        if !stat.exists {
            return Err(IdeFileError::new(
                IdeFileErrorKind::NotFound,
                "File does not exist",
            ));
        }
        Ok(stat)
    }
}

impl NodeAgentIdeFileSystem {
    pub(super) async fn rpc_write(
        &self,
        id: &str,
        path: &str,
        text: &str,
        format: &oxideterm_ide_core::TextFileFormat,
        expected: Option<&SavedFileVersion>,
        mode: WriteMode,
    ) -> Result<SavedFileVersion, IdeFileError> {
        let session = self.bound_session(id).await?;
        if expected.is_some_and(|v| v.etag.is_none()) {
            return Err(IdeFileError::new(
                IdeFileErrorKind::Conflict,
                "File must be reopened before saving",
            ));
        }
        let bytes = crate::text::encode_file(text, format)?;
        let result = session
            .write_bytes(
                "fs/writeIdeFile",
                path,
                &bytes,
                expected.and_then(|version| version.etag.as_deref()),
                Some(mode == WriteMode::CreateNew),
            )
            .await
            .map_err(ide_error_from_agent_error)?;
        Ok(version_from_agent_write(&result))
    }
}

impl NodeAgentIdeFileSystem {
    pub fn access(&self) -> RemoteAccess {
        self.access
    }
    pub fn uses_ssh_shell(&self) -> bool {
        self.binding
            .as_ref()
            .is_some_and(|binding| binding.ssh_shell)
    }
    pub fn is_bound(&self) -> bool {
        self.access != RemoteAccess::Automatic
    }
    pub fn supports_symbols(&self) -> bool {
        matches!(self.access, RemoteAccess::Automatic | RemoteAccess::Agent)
    }
    pub fn supports_watching(&self) -> bool {
        self.binding
            .as_ref()
            .map_or(self.access == RemoteAccess::Automatic, |binding| {
                !binding.terminal && binding.session.supports_capability("watch")
            })
    }

    pub async fn bind_node(
        &self,
        node: &str,
        access: RemoteAccess,
        progress: Option<watch::Sender<AgentInstallProgress>>,
    ) -> Result<Self, IdeFileError> {
        let mut fs = self.scoped_owner();
        fs.binding = None;
        fs.scope = None;
        fs.access = access;
        fs.scope = Some(Arc::new(BoundNodeOwner {
            sessions: fs.ide_sessions.clone(),
            key: fs.session_key(node),
            session: fs.ide_session_for_node(&NodeId::new(node)),
        }));
        fs.mode = if access == RemoteAccess::Sftp {
            NodeAgentMode::Disabled
        } else {
            NodeAgentMode::Enabled
        };
        let node_id = NodeId::new(node);
        fs.ensure_ide_session_for_node(&node_id).await?;
        if access == RemoteAccess::Sftp {
            fs.router
                .acquire_sftp(&node_id)
                .await
                .map_err(crate::node_sftp::map_route_error)?;
            return Ok(fs);
        }
        let channel = IdeChannel::open(&fs, &node_id)
            .await
            .map_err(ide_error_from_agent_error)?;
        let ssh_shell = channel.is_ssh_shell();
        let session = Self::start_channel(channel, access, progress).await?;
        fs.binding = Some(Arc::new(RemoteBinding {
            id: node.into(),
            owner_id: fs.owner_id,
            terminal: false,
            ssh_shell,
            owns_transport: true,
            audit: self.router.audit_context(&NodeId::new(node)),
            session,
        }));
        Ok(fs)
    }
}

/// Fixed backends release their node consumer even when startup is cancelled before UI adoption.
pub(super) struct BoundNodeOwner {
    sessions: Arc<DashMap<IdeSessionKey, Arc<IdeRemoteSessionInner>>>,
    key: IdeSessionKey,
    session: Arc<IdeRemoteSessionInner>,
}
impl BoundNodeOwner {
    pub(super) fn node_id(&self) -> &str {
        &self.key.node_id
    }
    pub(super) fn scoped(&self, owner_id: u64) -> Arc<Self> {
        let key = IdeSessionKey {
            owner_id,
            node_id: self.key.node_id.clone(),
        };
        let session = self
            .sessions
            .entry(key.clone())
            .or_insert_with(|| {
                Arc::new(IdeRemoteSessionInner::new(
                    self.session.node_id.clone(),
                    self.session.router.clone(),
                ))
            })
            .clone();
        Arc::new(Self {
            sessions: self.sessions.clone(),
            key,
            session,
        })
    }
    pub(super) async fn acquire(&self, node: &NodeId) -> Result<ResolvedConnection, RouteError> {
        if node.0 != self.key.node_id {
            return Err(RouteError::NotConnected(node.0.clone()));
        }
        self.session.acquire_connection().await
    }
}
impl Drop for BoundNodeOwner {
    fn drop(&mut self) {
        self.session.close();
        self.sessions.remove(&self.key);
    }
}
