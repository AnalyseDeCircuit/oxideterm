pub(super) enum SshSessionConnection {
    New(SshConfig),
    Existing {
        connection_id: String,
        // The outer option distinguishes registry inheritance from explicit disablement.
        x11_forwarding_override: Option<Option<X11ForwardPolicy>>,
    },
    Dedicated {
        config: SshConfig,
        parent_connection_id: Option<String>,
    },
}

pub struct SshSessionConfig {
    connection: Option<SshSessionConnection>,
    host: String,
    port: u16,
    username: String,
    registry: Option<SshConnectionRegistry>,
    consumer: Option<ConnectionConsumer>,
    prompt_handler: Option<Arc<dyn SshPromptHandler>>,
    managed_key_resolver: Option<ManagedKeyResolver>,
    trzsz_policy: Option<TrzszTransferPolicy>,
    runtime_handle: Option<tokio::runtime::Handle>,
    defer_pty_until_resize: bool,
    post_connect_command: Option<String>,
}

const POST_CONNECT_COMMAND_MAX_BYTES: usize = 8192;

impl SshSessionConfig {
    pub fn new(host: impl Into<String>, port: u16, username: impl Into<String>) -> Self {
        Self::from(SshConfig::password(host, port, username, ""))
    }

    pub fn for_existing_connection(
        connection_id: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
    ) -> Self {
        Self {
            connection: Some(SshSessionConnection::Existing {
                connection_id: connection_id.into(),
                x11_forwarding_override: None,
            }),
            host: host.into(),
            port,
            username: username.into(),
            registry: None,
            consumer: None,
            prompt_handler: None,
            managed_key_resolver: None,
            trzsz_policy: None,
            runtime_handle: None,
            defer_pty_until_resize: false,
            post_connect_command: None,
        }
    }

    pub fn for_dedicated_connection(
        config: SshConfig,
        parent_connection_id: Option<String>,
    ) -> Self {
        // Keep the source node's transport untouched while this terminal owns
        // a separately authenticated registry entry.
        let host = config.host.clone();
        let port = config.port;
        let username = config.username.clone();
        Self {
            connection: Some(SshSessionConnection::Dedicated {
                config,
                parent_connection_id,
            }),
            host,
            port,
            username,
            registry: None,
            consumer: None,
            prompt_handler: None,
            managed_key_resolver: None,
            trzsz_policy: None,
            runtime_handle: None,
            defer_pty_until_resize: false,
            post_connect_command: None,
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn with_registry(
        mut self,
        registry: SshConnectionRegistry,
        consumer: ConnectionConsumer,
    ) -> Self {
        self.registry = Some(registry);
        self.consumer = Some(consumer);
        self
    }

    pub fn with_x11_forwarding_override(
        mut self,
        x11_forwarding: Option<X11ForwardPolicy>,
    ) -> Self {
        if let Some(SshSessionConnection::Existing {
            x11_forwarding_override,
            ..
        }) = self.connection.as_mut()
        {
            // The outer option marks an explicit per-node choice, including disabled.
            *x11_forwarding_override = Some(x11_forwarding);
        }
        self
    }

    pub fn with_prompt_handler(mut self, prompt_handler: Arc<dyn SshPromptHandler>) -> Self {
        self.prompt_handler = Some(prompt_handler);
        self
    }

    pub fn with_managed_key_resolver(mut self, resolver: ManagedKeyResolver) -> Self {
        self.managed_key_resolver = Some(resolver);
        self
    }

    pub fn with_trzsz_policy(mut self, policy: Option<TrzszTransferPolicy>) -> Self {
        self.trzsz_policy = policy;
        self
    }

    pub fn with_runtime_handle(mut self, handle: tokio::runtime::Handle) -> Self {
        self.runtime_handle = Some(handle);
        self
    }

    pub fn with_deferred_pty(mut self, defer_pty_until_resize: bool) -> Self {
        self.defer_pty_until_resize = defer_pty_until_resize;
        self
    }

    pub fn with_post_connect_command(mut self, command: Option<String>) -> Self {
        self.post_connect_command = command.and_then(|command| {
            let command = command.trim().to_string();
            (!command.is_empty()).then_some(command)
        });
        self
    }

    pub fn defer_pty_until_resize(&self) -> bool {
        self.defer_pty_until_resize
    }

    pub fn trzsz_policy(&self) -> Option<TrzszTransferPolicy> {
        self.trzsz_policy.clone()
    }

    pub fn post_connect_command(&self) -> Option<&str> {
        self.post_connect_command.as_deref()
    }

    pub fn post_connect_input(&self) -> Result<Option<Vec<u8>>, String> {
        normalize_post_connect_command(self.post_connect_command.as_deref())
    }
}

impl From<oxideterm_ssh::SshConfig> for SshSessionConfig {
    fn from(mut config: oxideterm_ssh::SshConfig) -> Self {
        let post_connect_command = config.post_connect_command.take();
        Self {
            host: config.host.clone(),
            port: config.port,
            username: config.username.clone(),
            connection: Some(SshSessionConnection::New(config)),
            registry: None,
            consumer: None,
            prompt_handler: None,
            managed_key_resolver: None,
            trzsz_policy: None,
            runtime_handle: None,
            defer_pty_until_resize: false,
            post_connect_command,
        }
    }
}

fn normalize_post_connect_command(command: Option<&str>) -> Result<Option<Vec<u8>>, String> {
    let Some(command) = command.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    // Tauri sends each logical line as an Enter key. Normalize all newline
    // variants to carriage returns before the SSH PTY receives the payload.
    let mut normalized = command.replace("\r\n", "\n").replace('\r', "\n");
    normalized = normalized.replace('\n', "\r");
    if !normalized.ends_with('\r') {
        normalized.push('\r');
    }

    let bytes = normalized.into_bytes();
    if bytes.len() > POST_CONNECT_COMMAND_MAX_BYTES {
        return Err(format!(
            "Post-connect command is too long (max {} bytes)",
            POST_CONNECT_COMMAND_MAX_BYTES
        ));
    }
    Ok(Some(bytes))
}

#[cfg(test)]
mod ssh_config_tests {
    use super::{SshSessionConfig, normalize_post_connect_command};
    use oxideterm_ssh::{SshConfig, X11ForwardPolicy};

    #[test]
    fn post_connect_command_normalization_handles_content_and_empty_values() {
        for (input, expected) in [
            (Some("  cd /srv/app  "), Some(b"cd /srv/app\r".to_vec())),
            (Some("cd /srv/app\nls"), Some(b"cd /srv/app\rls\r".to_vec())),
            (Some("   "), None),
            (None, None),
        ] {
            assert_eq!(normalize_post_connect_command(input).unwrap(), expected);
        }
    }

    #[test]
    fn post_connect_override_can_clear_saved_node_command() {
        let config = SshConfig {
            post_connect_command: Some("cd /srv/app".to_string()),
            ..SshConfig::default()
        };
        let session_config = SshSessionConfig::from(config).with_post_connect_command(None);
        assert_eq!(session_config.post_connect_command(), None);
    }

    #[test]
    fn existing_connection_config_retains_only_safe_terminal_metadata() {
        let config = SshSessionConfig::for_existing_connection(
            "connection-1",
            "host",
            22,
            "alice",
        )
        .with_x11_forwarding_override(Some(X11ForwardPolicy::trusted()));

        assert!(matches!(
            config.connection.as_ref(),
            Some(super::SshSessionConnection::Existing {
                x11_forwarding_override: Some(Some(policy)),
                ..
            }) if *policy == X11ForwardPolicy::trusted()
        ));
        assert_eq!(config.host(), "host");
        assert_eq!(config.port(), 22);
        assert_eq!(config.username(), "alice");
        assert!(!format!("{config:?}").contains("connection-1"));
    }

    #[test]
    fn dedicated_connection_retains_parent_route_without_using_existing_mode() {
        let config = SshSessionConfig::for_dedicated_connection(
            SshConfig::password("target", 22, "alice", "secret"),
            Some("parent-connection".to_string()),
        );

        assert!(matches!(
            config.connection.as_ref(),
            Some(super::SshSessionConnection::Dedicated {
                parent_connection_id: Some(parent_connection_id),
                ..
            }) if parent_connection_id == "parent-connection"
        ));
        assert_eq!(config.host(), "target");
        assert!(!format!("{config:?}").contains("secret"));
    }
}

impl std::fmt::Debug for SshSessionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let connection_kind = match self.connection.as_ref() {
            Some(SshSessionConnection::New(_)) => "new",
            Some(SshSessionConnection::Existing { .. }) => "existing",
            Some(SshSessionConnection::Dedicated { .. }) => "dedicated",
            None => "moved",
        };
        f.debug_struct("SshSessionConfig")
            .field("connection", &connection_kind)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("registry", &self.registry)
            .field("consumer", &self.consumer)
            .field("prompt_handler", &self.prompt_handler.is_some())
            .field("managed_key_resolver", &self.managed_key_resolver.is_some())
            .field("trzsz_policy", &self.trzsz_policy)
            .field("runtime_handle", &self.runtime_handle.is_some())
            .field("defer_pty_until_resize", &self.defer_pty_until_resize)
            .field("post_connect_command", &self.post_connect_command.is_some())
            .finish()
    }
}
