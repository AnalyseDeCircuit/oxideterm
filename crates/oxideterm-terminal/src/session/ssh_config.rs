pub struct SshSessionConfig {
    config: SshConfig,
    registry: Option<SshConnectionRegistry>,
    consumer: Option<ConnectionConsumer>,
    prompt_handler: Option<Arc<dyn SshPromptHandler>>,
    trzsz_policy: Option<TrzszTransferPolicy>,
}

impl SshSessionConfig {
    pub fn new(host: impl Into<String>, port: u16, username: impl Into<String>) -> Self {
        Self {
            config: SshConfig::password(host, port, username, ""),
            registry: None,
            consumer: None,
            prompt_handler: None,
            trzsz_policy: None,
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

    pub fn trzsz_policy(&self) -> Option<TrzszTransferPolicy> {
        self.trzsz_policy.clone()
    }
}

impl From<oxideterm_ssh::SshConfig> for SshSessionConfig {
    fn from(config: oxideterm_ssh::SshConfig) -> Self {
        Self {
            config,
            registry: None,
            consumer: None,
            prompt_handler: None,
            trzsz_policy: None,
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
            .finish()
    }
}
