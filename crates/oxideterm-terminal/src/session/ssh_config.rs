pub struct SshSessionConfig {
    config: SshConfig,
    registry: Option<SshConnectionRegistry>,
    consumer: Option<ConnectionConsumer>,
    prompt_handler: Option<Arc<dyn SshPromptHandler>>,
    trzsz_policy: Option<TrzszTransferPolicy>,
    defer_pty_until_resize: bool,
    post_connect_command: Option<String>,
}

impl SshSessionConfig {
    pub fn new(host: impl Into<String>, port: u16, username: impl Into<String>) -> Self {
        Self {
            config: SshConfig::password(host, port, username, ""),
            registry: None,
            consumer: None,
            prompt_handler: None,
            trzsz_policy: None,
            defer_pty_until_resize: false,
            post_connect_command: None,
        }
    }

    pub fn host(&self) -> &str {
        &self.config.host
    }

    pub fn port(&self) -> u16 {
        self.config.port
    }

    pub fn username(&self) -> &str {
        &self.config.username
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

    pub fn with_trzsz_policy(mut self, policy: Option<TrzszTransferPolicy>) -> Self {
        self.trzsz_policy = policy;
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
}

impl From<oxideterm_ssh::SshConfig> for SshSessionConfig {
    fn from(config: oxideterm_ssh::SshConfig) -> Self {
        Self {
            post_connect_command: config.post_connect_command.clone(),
            config,
            registry: None,
            consumer: None,
            prompt_handler: None,
            trzsz_policy: None,
            defer_pty_until_resize: false,
        }
    }
}

impl std::fmt::Debug for SshSessionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshSessionConfig")
            .field("config", &self.config)
            .field("registry", &self.registry)
            .field("consumer", &self.consumer)
            .field("prompt_handler", &self.prompt_handler.is_some())
            .field("trzsz_policy", &self.trzsz_policy)
            .field("defer_pty_until_resize", &self.defer_pty_until_resize)
            .field("post_connect_command", &self.post_connect_command.is_some())
            .finish()
    }
}
