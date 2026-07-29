pub(super) enum SshSessionConnection {
    New(SshConfig),
    Existing { connection_id: String },
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
    use oxideterm_ssh::SshConfig;

    #[test]
    fn post_connect_command_trims_and_adds_enter_like_tauri() {
        assert_eq!(
            normalize_post_connect_command(Some("  cd /srv/app  ")).unwrap(),
            Some(b"cd /srv/app\r".to_vec())
        );
    }

    #[test]
    fn post_connect_command_converts_multiline_to_enter_keys_like_tauri() {
        assert_eq!(
            normalize_post_connect_command(Some("cd /srv/app\nls")).unwrap(),
            Some(b"cd /srv/app\rls\r".to_vec())
        );
    }

    #[test]
    fn post_connect_command_ignores_blank_values_like_tauri() {
        assert_eq!(normalize_post_connect_command(Some("   ")).unwrap(), None);
        assert_eq!(normalize_post_connect_command(None).unwrap(), None);
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
    fn runtime_handle_is_optional_and_injectable() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        assert!(
            SshSessionConfig::new("example.com", 22, "alice")
                .runtime_handle
                .is_none()
        );
        assert!(
            SshSessionConfig::new("example.com", 22, "alice")
                .with_runtime_handle(runtime.handle().clone())
                .runtime_handle
                .is_some()
        );
    }

    #[test]
    fn existing_connection_config_retains_only_safe_terminal_metadata() {
        let config =
            SshSessionConfig::for_existing_connection("connection-1", "host", 22, "alice");

        assert!(matches!(
            config.connection.as_ref(),
            Some(super::SshSessionConnection::Existing { .. })
        ));
        assert_eq!(config.host(), "host");
        assert_eq!(config.port(), 22);
        assert_eq!(config.username(), "alice");
        assert!(!format!("{config:?}").contains("connection-1"));
    }
}

impl std::fmt::Debug for SshSessionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let connection_kind = match self.connection.as_ref() {
            Some(SshSessionConnection::New(_)) => "new",
            Some(SshSessionConnection::Existing { .. }) => "existing",
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
