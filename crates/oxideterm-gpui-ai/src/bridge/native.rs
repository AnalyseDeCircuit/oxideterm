use std::sync::Arc;

use oxideterm_ai::{
    AiProviderKeyStore, AiProviderRefreshKeyPolicy, AiProviderView, ProviderModelRefresh,
};
use tokio::runtime::Runtime;
use zeroize::Zeroizing;

use super::{AiModelRefreshFailure, AiProviderHost, HostFuture, HostReply};

pub(crate) struct DesktopProvider {
    pub runtime: Arc<Runtime>,
    pub keys: AiProviderKeyStore,
}

impl AiProviderHost for DesktopProvider {
    fn prepare_chat(
        &self,
        mut config: oxideterm_ai::AiChatStreamConfig,
    ) -> HostFuture<Result<oxideterm_ai::AiChatStreamConfig, super::ProviderKeyError>> {
        let runtime = self.runtime.clone();
        let keys = self.keys.clone();
        Box::pin(async move {
            if let Some(id) = config.provider_id.clone() {
                let required = oxideterm_ai::provider_chat_requires_key(&config.provider_type);
                match runtime
                    .spawn_blocking(move || keys.get_provider_key(&id))
                    .await
                {
                    Ok(Ok(None)) if required => return Err(super::ProviderKeyError::Missing),
                    Ok(Ok(key)) => config.api_key = key.map(oxideterm_ai::SharedAiProviderKey::new),
                    _ if required => return Err(super::ProviderKeyError::Unavailable),
                    _ => {}
                }
            }
            Ok(config)
        })
    }
    fn prepare_summary(
        &self,
        mut config: oxideterm_ai::AiChatStreamConfig,
        messages: Vec<oxideterm_ai::AiChatMessage>,
    ) -> HostFuture<Result<super::PreparedSummary, super::ProviderKeyError>> {
        let runtime = self.runtime.clone();
        let keys = self.keys.clone();
        Box::pin(async move {
            let required = oxideterm_ai::provider_chat_requires_key(&config.provider_type);
            let result = if let Some(id) = config.provider_id.clone() {
                runtime
                    .spawn_blocking(move || keys.get_provider_key(&id))
                    .await
                    .map_err(|_| ())
                    .and_then(|r| r.map_err(|_| ()))
            } else {
                Ok(None)
            };
            match result {
                Ok(None) if required => return Err(super::ProviderKeyError::Missing),
                Err(()) if required => return Err(super::ProviderKeyError::Unavailable),
                Ok(key) => config.api_key = key.map(oxideterm_ai::SharedAiProviderKey::new),
                Err(()) => {}
            }
            let start: super::PreparedSummary = Box::new(move |reply| {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                let model = tokio_util::task::AbortOnDropHandle::new(
                    runtime.spawn(oxideterm_ai::stream_chat_completion(config, messages, tx)),
                );
                runtime
                    .spawn(async move {
                        let _model = model;
                        reply(oxideterm_ai::stream_state::collect_ai_compaction_summary(rx).await);
                    })
                    .into()
            });
            Ok(start)
        })
    }
    fn inline(
        &self,
        mut config: oxideterm_ai::AiChatStreamConfig,
        messages: Vec<oxideterm_ai::AiChatMessage>,
        deliver: Box<dyn Fn(super::InlineEvent) + Send>,
    ) -> super::HostTask {
        use super::InlineEvent;
        let keys = self.keys.clone();
        self.runtime
            .spawn(async move {
                if let Some(id) = config.provider_id.clone() {
                    let required = oxideterm_ai::provider_chat_requires_key(&config.provider_type);
                    match tokio::task::spawn_blocking(move || keys.get_provider_key(&id))
                        .await
                        .ok()
                        .and_then(Result::ok)
                    {
                        Some(key) => {
                            let has_key = key.as_ref().is_some_and(|k| !k.trim().is_empty());
                            deliver(InlineEvent::KeyStatus(has_key));
                            if required && !has_key {
                                return deliver(InlineEvent::MissingKey);
                            }
                            config.api_key = key.map(oxideterm_ai::SharedAiProviderKey::new);
                        }
                        None if required => return deliver(InlineEvent::KeyUnavailable),
                        None => {}
                    }
                }
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                let provider = oxideterm_ai::stream_chat_completion(config, messages, tx);
                let delivery = async move {
                    while let Some(event) = rx.recv().await {
                        match event {
                            oxideterm_ai::AiStreamEvent::Content(text) => {
                                deliver(InlineEvent::Content(text))
                            }
                            oxideterm_ai::AiStreamEvent::Done => return deliver(InlineEvent::Done),
                            oxideterm_ai::AiStreamEvent::Error(_) => {
                                return deliver(InlineEvent::Failed);
                            }
                            _ => {}
                        }
                    }
                };
                let _ = tokio::join!(provider, delivery);
            })
            .into()
    }
    fn models(
        &self,
        provider: AiProviderView,
        reply: HostReply<Result<ProviderModelRefresh, AiModelRefreshFailure>>,
    ) {
        let keys = self.keys.clone();
        self.runtime.spawn(async move {
            let policy = oxideterm_ai::provider_refresh_key_policy(&provider.provider_type);
            let key = match policy {
                AiProviderRefreshKeyPolicy::NoKey => None,
                AiProviderRefreshKeyPolicy::OptionalStoredKey
                | AiProviderRefreshKeyPolicy::RequiredStoredKey => {
                    let id = provider.id.clone();
                    let read =
                        tokio::task::spawn_blocking(move || keys.get_provider_key(&id)).await;
                    if matches!(policy, AiProviderRefreshKeyPolicy::OptionalStoredKey) {
                        read.ok().and_then(Result::ok).flatten()
                    } else {
                        match read
                            .map_err(|err| err.to_string())
                            .and_then(|r| r.map_err(|err| err.to_string()))
                        {
                            Ok(Some(key)) => Some(key),
                            Ok(None) => return reply(Err(AiModelRefreshFailure::MissingApiKey)),
                            Err(err) => {
                                return reply(Err(AiModelRefreshFailure::Failed(format!(
                                    "could not read the saved API key: {err}"
                                ))));
                            }
                        }
                    }
                }
            };
            reply(
                oxideterm_ai::fetch_provider_models(provider, key)
                    .await
                    .map_err(|err| {
                        AiModelRefreshFailure::Failed(crate::state::model_refresh_failure_message(
                            &err,
                        ))
                    }),
            );
        });
    }

    fn probe(&self, provider: AiProviderView, endpoint: &'static str, reply: HostReply<bool>) {
        let keys = self.keys.clone();
        self.runtime.spawn(async move {
            let online = if provider.provider_type == "openai_compatible" {
                let key = tokio::task::spawn_blocking(move || keys.get_provider_key(&provider.id))
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .flatten();
                oxideterm_ai::check_openai_compatible_model_selector_provider_online(
                    &provider.base_url,
                    key,
                )
                .await
            } else {
                oxideterm_ai::check_model_selector_provider_online(&provider.base_url, endpoint)
                    .await
            };
            reply(online);
        });
    }

    fn key_status(&self, provider_id: String, reply: HostReply<bool>) {
        let keys = self.keys.clone();
        self.runtime.spawn(async move {
            reply(
                tokio::task::spawn_blocking(move || keys.has_provider_key(&provider_id))
                    .await
                    .unwrap_or(false),
            );
        });
    }

    fn update_key(&self, provider_id: String, key: Option<Zeroizing<String>>) -> HostFuture<bool> {
        let runtime = self.runtime.clone();
        let keys = self.keys.clone();
        Box::pin(async move {
            runtime
                .spawn_blocking(move || match key {
                    Some(key) => keys.store_provider_key(&provider_id, key),
                    None => keys.delete_provider_key(&provider_id),
                })
                .await
                .is_ok_and(|result| result.is_ok())
        })
    }
}

impl From<tokio::task::JoinHandle<()>> for super::HostTask {
    fn from(task: tokio::task::JoinHandle<()>) -> Self {
        Self(Some(super::Cancellation::Worker(task)))
    }
}

impl From<tokio::task::AbortHandle> for super::HostTask {
    fn from(task: tokio::task::AbortHandle) -> Self {
        Self::new(move || task.abort())
    }
}
use super::{AiAcpAgentProbeResult, AiAcpMetadataHost, AiHostBridge, AiMcpHost, HostTask};
use std::rc::Rc;
pub(crate) fn desktop_bridge(
    runtime: Arc<Runtime>,
    keys: AiProviderKeyStore,
    registry: oxideterm_ai::McpRegistry,
) -> AiHostBridge {
    let provider = Rc::new(DesktopProvider {
        runtime: runtime.clone(),
        keys,
    });
    AiHostBridge {
        provider: provider.clone(),
        acp_metadata: provider,
        mcp: Rc::new(DesktopMcp { runtime, registry }),
    }
}
impl AiAcpMetadataHost for DesktopProvider {
    fn probe_agent(
        &self,
        agent: oxideterm_settings::AcpAgentConfig,
        reply: HostReply<AiAcpAgentProbeResult>,
    ) -> HostTask {
        let capability_policy = oxideterm_ai::AcpHostCapabilityPolicy {
            fs_read_text_file: agent.capability_policy.fs_read_text_file,
            fs_write_text_file: agent.capability_policy.fs_write_text_file,
            terminal: agent.capability_policy.terminal,
        };
        // Move args and env into the zeroizing launch config. They may contain
        // local agent tokens and must not be cloned for worker convenience.
        let launch_config = oxideterm_ai::AcpLaunchConfig {
            id: agent.id,
            display_name: agent.display_name,
            command: agent.command,
            args: agent.args,
            env: agent.env,
            cwd: agent.cwd.map(std::path::PathBuf::from),
        };
        self.runtime
            .spawn(async move {
                let result = match oxideterm_ai::build_acp_stdio_launcher(launch_config) {
                    Ok(launcher) => {
                        if !oxideterm_ai::acp_launch_command_available(launcher.config())
                            .unwrap_or(false)
                        {
                            ai_acp_probe_error_result("command_not_found")
                        } else {
                            match oxideterm_ai::initialize_acp_agent(
                                launcher,
                                env!("CARGO_PKG_VERSION").to_string(),
                                capability_policy,
                            )
                            .await
                            {
                                Ok(response) => {
                                    AiAcpAgentProbeResult {
                                        // Initialize advertises available auth methods; it does
                                        // not prove that the current process still needs auth.
                                        // Only an AuthRequired protocol error can establish that.
                                        runtime_state:
                                            oxideterm_settings::AcpAgentRuntimeState::Ready,
                                        auth_status: if response.auth_methods.is_empty() {
                                            oxideterm_settings::AcpAgentAuthStatus::NotRequired
                                        } else {
                                            oxideterm_settings::AcpAgentAuthStatus::Unknown
                                        },
                                        last_error_kind: None,
                                    }
                                }
                                Err(_) => ai_acp_probe_error_result("initialize"),
                            }
                        }
                    }
                    Err(_) => ai_acp_probe_error_result("config"),
                };
                reply(result);
            })
            .into()
    }
    fn discover(
        &self,
        agent: oxideterm_settings::AcpAgentConfig,
        cwd: Option<std::path::PathBuf>,
        reply: HostReply<Option<Vec<oxideterm_ai::AcpSessionConfigOption>>>,
    ) -> HostTask {
        let session_cwd =
            cwd.unwrap_or_else(|| oxideterm_ai::tool_loop::acp_session_cwd_from_agent(&agent));
        let capability_policy = oxideterm_ai::AcpHostCapabilityPolicy {
            fs_read_text_file: agent.capability_policy.fs_read_text_file,
            fs_write_text_file: agent.capability_policy.fs_write_text_file,
            terminal: agent.capability_policy.terminal,
        };
        let display_name = if agent.display_name.trim().is_empty() {
            agent.id.clone()
        } else {
            agent.display_name
        };
        // Discovery uses the same zeroizing one-shot launch config as a real
        // ACP session and moves token-bearing args/env into the worker.
        let launch_config = oxideterm_ai::AcpLaunchConfig {
            id: agent.id,
            display_name,
            command: agent.command,
            args: agent.args,
            env: agent.env,
            cwd: agent.cwd.map(std::path::PathBuf::from),
        };
        self.runtime
            .spawn(async move {
                let config_options = match oxideterm_ai::build_acp_stdio_launcher(launch_config) {
                    Ok(launcher) => oxideterm_ai::discover_acp_session_config_options(
                        launcher,
                        env!("CARGO_PKG_VERSION").to_string(),
                        capability_policy,
                        session_cwd,
                    )
                    .await
                    .ok()
                    .filter(|options| {
                        oxideterm_ai::acp_model_config_option(options)
                            .is_some_and(|option| !option.choices.is_empty())
                    }),
                    Err(_) => None,
                };
                reply(config_options);
            })
            .into()
    }
}

pub(crate) fn ai_acp_probe_error_result(kind: &'static str) -> AiAcpAgentProbeResult {
    // Only stable categories cross the worker boundary; process errors may
    // include args, env values, or local authentication material.
    AiAcpAgentProbeResult {
        runtime_state: oxideterm_settings::AcpAgentRuntimeState::Error,
        auth_status: oxideterm_settings::AcpAgentAuthStatus::Unknown,
        last_error_kind: Some(kind.to_string()),
    }
}

struct DesktopMcp {
    runtime: Arc<Runtime>,
    registry: oxideterm_ai::McpRegistry,
}
impl AiMcpHost for DesktopMcp {
    fn bootstrap(&self, configs: Vec<serde_json::Value>) {
        let registry = self.registry.clone();
        self.runtime.spawn(async move {
            registry.connect_all_values(&configs).await;
        });
    }
    fn snapshots(&self) -> Vec<oxideterm_ai::McpServerStateSnapshot> {
        self.registry.snapshots()
    }
    fn tool_definitions(&self) -> Vec<oxideterm_ai::AiToolDefinition> {
        self.registry.tool_definitions()
    }
    fn save_token(&self, id: String, token: Zeroizing<String>) -> HostFuture<bool> {
        let registry = self.registry.clone();
        let runtime = self.runtime.clone();
        Box::pin(async move {
            runtime
                .spawn_blocking(move || registry.store_auth_token(&id, token))
                .await
                .is_ok_and(|r| r.is_ok())
        })
    }
    fn refresh(&self, id: String) -> HostFuture<()> {
        let registry = self.registry.clone();
        Box::pin(async move {
            let _ = registry.refresh_tools(&id).await;
        })
    }
    fn connect(&self, config: oxideterm_ai::McpServerConfig) -> HostFuture<()> {
        let registry = self.registry.clone();
        Box::pin(async move {
            registry.connect_config(config).await;
        })
    }
    fn disconnect(&self, id: String) -> HostFuture<()> {
        let registry = self.registry.clone();
        Box::pin(async move {
            registry.disconnect_server(&id).await;
        })
    }
    fn remove(&self, id: String) -> HostFuture<()> {
        let registry = self.registry.clone();
        let runtime = self.runtime.clone();
        Box::pin(async move {
            registry.disconnect_server(&id).await;
            let _ = runtime
                .spawn_blocking(move || registry.delete_auth_token(&id))
                .await;
        })
    }
}
