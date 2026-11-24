//! Services supplied by the application that owns provider credentials and transport.

pub mod agents;
pub mod history;
pub mod knowledge;
pub mod turns;
pub(crate) type TurnSender =
    oxideterm_workspace::delivery::ActiveDeliverySender<crate::stream::PortableStreamDelivery>;

use std::{future::Future, pin::Pin};

use oxideterm_ai::{AiProviderView, ProviderModelRefresh};
use zeroize::Zeroizing;

pub type HostFuture<T> = Pin<Box<dyn Future<Output = T>>>;
pub type HostReply<T> = Box<dyn FnOnce(T) + Send>;

pub enum AiModelRefreshFailure {
    MissingApiKey,
    Failed(String),
}

enum Cancellation {
    Host(Box<dyn FnOnce()>),
    #[cfg(not(target_family = "wasm"))]
    Worker(tokio::task::JoinHandle<()>),
}
pub struct HostTask(Option<Cancellation>);
impl HostTask {
    pub fn new(cancel: impl FnOnce() + 'static) -> Self {
        Self(Some(Cancellation::Host(Box::new(cancel))))
    }
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn into_worker(mut self) -> Result<tokio::task::JoinHandle<()>, Self> {
        match self.0.take() {
            Some(Cancellation::Worker(worker)) => Ok(worker),
            other => {
                self.0 = other;
                Err(self)
            }
        }
    }
    pub fn abort(self) {
        drop(self)
    }
}
impl Drop for HostTask {
    fn drop(&mut self) {
        match self.0.take() {
            Some(Cancellation::Host(cancel)) => cancel(),
            #[cfg(not(target_family = "wasm"))]
            Some(Cancellation::Worker(worker)) => worker.abort(),
            None => {}
        }
    }
}

pub struct AiAcpAgentProbeResult {
    pub runtime_state: oxideterm_settings::AcpAgentRuntimeState,
    pub auth_status: oxideterm_settings::AcpAgentAuthStatus,
    pub last_error_kind: Option<String>,
}

pub trait AiAcpMetadataHost {
    fn probe_agent(
        &self,
        agent: oxideterm_settings::AcpAgentConfig,
        reply: HostReply<AiAcpAgentProbeResult>,
    ) -> HostTask;
    fn discover(
        &self,
        agent: oxideterm_settings::AcpAgentConfig,
        cwd: Option<std::path::PathBuf>,
        reply: HostReply<Option<Vec<oxideterm_ai::AcpSessionConfigOption>>>,
    ) -> HostTask;
}

pub struct AiHostBridge {
    pub provider: std::rc::Rc<dyn AiProviderHost>,
    pub acp_metadata: std::rc::Rc<dyn AiAcpMetadataHost>,
    pub mcp: std::rc::Rc<dyn AiMcpHost>,
}

pub trait AiMcpHost {
    fn bootstrap(&self, configs: Vec<serde_json::Value>);
    fn snapshots(&self) -> Vec<oxideterm_ai::McpServerStateSnapshot>;
    fn tool_definitions(&self) -> Vec<oxideterm_ai::AiToolDefinition>;
    fn save_token(&self, server_id: String, token: Zeroizing<String>) -> HostFuture<bool>;
    fn refresh(&self, server_id: String) -> HostFuture<()>;
    fn connect(&self, config: oxideterm_ai::McpServerConfig) -> HostFuture<()>;
    fn disconnect(&self, server_id: String) -> HostFuture<()>;
    fn remove(&self, server_id: String) -> HostFuture<()>;
}

pub enum InlineEvent {
    KeyStatus(bool),
    Content(String),
    Done,
    MissingKey,
    KeyUnavailable,
    Failed,
}

pub trait AiProviderHost {
    fn prepare_chat(
        &self,
        config: oxideterm_ai::AiChatStreamConfig,
    ) -> HostFuture<Result<oxideterm_ai::AiChatStreamConfig, ProviderKeyError>>;
    /// Preparing credentials does not contact the model; the workspace rechecks the run first.
    fn prepare_summary(
        &self,
        config: oxideterm_ai::AiChatStreamConfig,
        messages: Vec<oxideterm_ai::AiChatMessage>,
    ) -> HostFuture<Result<PreparedSummary, ProviderKeyError>>;
    fn inline(
        &self,
        config: oxideterm_ai::AiChatStreamConfig,
        messages: Vec<oxideterm_ai::AiChatMessage>,
        deliver: Box<dyn Fn(InlineEvent) + Send>,
    ) -> HostTask;
    fn models(
        &self,
        provider: AiProviderView,
        reply: HostReply<Result<ProviderModelRefresh, AiModelRefreshFailure>>,
    );
    fn probe(&self, provider: AiProviderView, endpoint: &'static str, reply: HostReply<bool>);
    fn key_status(&self, provider_id: String, reply: HostReply<bool>);
    /// Key writes start when polled, after the workspace admits the operation.
    fn update_key(&self, provider_id: String, key: Option<Zeroizing<String>>) -> HostFuture<bool>;
}

pub enum ProviderKeyError {
    Missing,
    Unavailable,
}
pub type PreparedSummary = Box<dyn FnOnce(HostReply<(String, bool)>) -> HostTask>;

#[cfg(not(target_family = "wasm"))]
pub(crate) mod native;
