//! Optional desktop services are unavailable in a browser-only assistant. The demo
//! advertises no external models, agents, knowledge collections, tools or jobs.
use oxideterm_ai as ai;
use oxideterm_ai::{AiProviderView, ProviderModelRefresh, agent::*};
use oxideterm_ai_tasks::*;
use oxideterm_gpui_ai::{
    background::AiBackgroundHost,
    bridge::{agents::*, knowledge::*, *},
    knowledge::page::{EmbeddingJob, KnowledgeJobs},
};
use std::sync::{Arc, atomic::AtomicBool};
use zeroize::Zeroizing;
pub struct LocalOnly;
fn finished() -> HostTask {
    HostTask::new(|| {})
}
fn ready<T: 'static>(v: T) -> HostFuture<T> {
    Box::pin(std::future::ready(v))
}
impl AgentUpdates for LocalOnly {
    fn changed(&mut self) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + '_>> {
        Box::pin(std::future::ready(false))
    }
}
impl AiProviderHost for LocalOnly {
    fn prepare_chat(
        &self,
        config: oxideterm_ai::AiChatStreamConfig,
    ) -> HostFuture<Result<oxideterm_ai::AiChatStreamConfig, ProviderKeyError>> {
        ready(Ok(config))
    }
    fn prepare_summary(
        &self,
        _config: oxideterm_ai::AiChatStreamConfig,
        _messages: Vec<oxideterm_ai::AiChatMessage>,
    ) -> HostFuture<Result<PreparedSummary, ProviderKeyError>> {
        ready(Err(ProviderKeyError::Unavailable))
    }
    fn inline(
        &self,
        _config: oxideterm_ai::AiChatStreamConfig,
        _messages: Vec<oxideterm_ai::AiChatMessage>,
        deliver: Box<dyn Fn(InlineEvent) + Send>,
    ) -> HostTask {
        deliver(InlineEvent::Failed);
        finished()
    }
    fn models(
        &self,
        _provider: AiProviderView,
        reply: HostReply<Result<ProviderModelRefresh, AiModelRefreshFailure>>,
    ) {
        reply(Err(AiModelRefreshFailure::Failed("ENOTSUP".into())));
    }
    fn probe(&self, _provider: AiProviderView, _endpoint: &'static str, reply: HostReply<bool>) {
        reply(false);
    }
    fn key_status(&self, _provider_id: String, reply: HostReply<bool>) {
        reply(false);
    }
    fn update_key(
        &self,
        _provider_id: String,
        _key: Option<Zeroizing<String>>,
    ) -> HostFuture<bool> {
        ready(false)
    }
}
impl AiAcpMetadataHost for LocalOnly {
    fn probe_agent(
        &self,
        _agent: oxideterm_settings::AcpAgentConfig,
        reply: HostReply<AiAcpAgentProbeResult>,
    ) -> HostTask {
        reply(AiAcpAgentProbeResult {
            runtime_state: Default::default(),
            auth_status: Default::default(),
            last_error_kind: Some("ENOTSUP".into()),
        });
        finished()
    }
    fn discover(
        &self,
        _agent: oxideterm_settings::AcpAgentConfig,
        _cwd: Option<std::path::PathBuf>,
        reply: HostReply<Option<Vec<oxideterm_ai::AcpSessionConfigOption>>>,
    ) -> HostTask {
        reply(None);
        finished()
    }
}
impl AiMcpHost for LocalOnly {
    fn bootstrap(&self, _configs: Vec<serde_json::Value>) {}
    fn snapshots(&self) -> Vec<oxideterm_ai::McpServerStateSnapshot> {
        vec![]
    }
    fn tool_definitions(&self) -> Vec<oxideterm_ai::AiToolDefinition> {
        vec![]
    }
    fn save_token(&self, _server_id: String, _token: Zeroizing<String>) -> HostFuture<bool> {
        ready(false)
    }
    fn refresh(&self, _server_id: String) -> HostFuture<()> {
        ready(())
    }
    fn connect(&self, _config: oxideterm_ai::McpServerConfig) -> HostFuture<()> {
        ready(())
    }
    fn disconnect(&self, _server_id: String) -> HostFuture<()> {
        ready(())
    }
    fn remove(&self, _server_id: String) -> HostFuture<()> {
        ready(())
    }
}
impl AiAgentHost for LocalOnly {
    fn subscribe(&self) -> Box<dyn AgentUpdates> {
        Box::new(LocalOnly)
    }
    fn snapshots(&self, _conversation: &str) -> Vec<AgentSnapshot> {
        vec![]
    }
    fn snapshot(&self, _run: &AgentRunRef) -> Result<AgentSnapshot, AgentError> {
        Err(AgentError::StaleRun)
    }
    fn communication(&self, _run: &AgentRunRef) -> Vec<AgentMessage> {
        vec![]
    }
    fn accepts_messages(&self, _run: &AgentRunRef) -> bool {
        false
    }
    fn supplement(&self, _run: &AgentRunRef, _content: AgentText) -> Result<(), AgentError> {
        Err(AgentError::StaleRun)
    }
    fn change_queued_model(
        &self,
        _run: &AgentRunRef,
        _model: AgentModel,
    ) -> Result<(), AgentError> {
        Err(AgentError::StaleRun)
    }
    fn stop_child(&self, _run: &AgentRunRef) -> Result<(), AgentError> {
        Err(AgentError::StaleRun)
    }
    fn cancel_group(&self, _parent: &AgentRunRef) -> Result<(), AgentError> {
        Err(AgentError::StaleRun)
    }
    fn complete(&self, _run: &AgentRunRef, _result: AgentResult) -> Result<(), AgentError> {
        Err(AgentError::StaleRun)
    }
    fn finish_group(&self, _parent: &AgentRunRef) -> Result<(), AgentError> {
        Err(AgentError::StaleRun)
    }
    fn remove_conversation(&self, _conversation: &str) {}
}
impl AiKnowledgeHost for LocalOnly {
    fn list_collections(
        &self,
        _scope_filter: Option<&str>,
    ) -> Result<Vec<ai::RagCollectionResponse>, String> {
        Ok(vec![])
    }
    fn list_documents(
        &self,
        _collection_id: &str,
        _offset: Option<usize>,
        _limit: Option<usize>,
    ) -> Result<ai::RagPaginatedDocuments, String> {
        Err("ENOTSUP".into())
    }
    fn get_document_content(&self, _doc_id: &str) -> Result<String, String> {
        Err("ENOTSUP".into())
    }
    fn create_collection(
        &self,
        _request: ai::RagCreateCollectionRequest,
    ) -> Result<ai::RagCollectionResponse, String> {
        Err("ENOTSUP".into())
    }
    fn create_blank_document(
        &self,
        _request: ai::RagCreateBlankDocumentRequest,
    ) -> Result<ai::RagDocumentResponse, String> {
        Err("ENOTSUP".into())
    }
    fn delete_collection(&self, _collection_id: &str) -> Result<(), String> {
        Err("ENOTSUP".into())
    }
    fn remove_document(&self, _doc_id: &str) -> Result<(), String> {
        Err("ENOTSUP".into())
    }
    fn add_document(
        &self,
        _request: ai::RagAddDocumentRequest,
    ) -> Result<ai::RagDocumentResponse, String> {
        Err("ENOTSUP".into())
    }
    fn search(
        &self,
        _request: ai::RagSearchRequest,
    ) -> Result<Vec<ai::RagSearchResultResponse>, String> {
        Ok(vec![])
    }
    fn get_collection_stats(&self, _collection_id: &str) -> Result<ai::RagStatsResponse, String> {
        Err("ENOTSUP".into())
    }
    fn keyword_index_state(&self) -> ai::RagKeywordIndexState {
        ai::RagKeywordIndexState::Ready
    }
    fn document_semantic_index_state(
        &self,
        _doc_id: &str,
    ) -> Result<ai::RagSemanticIndexState, String> {
        Err("ENOTSUP".into())
    }
    fn save_document(
        &self,
        _doc_id: &str,
        _content: String,
        _expected_version: Option<u64>,
    ) -> Result<ai::RagDocumentSaveOutcome, KnowledgeSaveError> {
        Err(KnowledgeSaveError::Failed("ENOTSUP".into()))
    }
    fn get_document(&self, _doc_id: &str) -> Result<ai::RagDocumentContentResponse, String> {
        Err("ENOTSUP".into())
    }
    fn stats_revision(&self) -> u64 {
        0
    }
    fn rename_collection(&self, _id: &str, _name: &str) -> Result<(), String> {
        Err("ENOTSUP".into())
    }
    fn edit_metadata(
        &self,
        _id: &str,
        _name: Option<&str>,
        _collection: Option<&str>,
        _version: u64,
    ) -> Result<DocumentMetadata, String> {
        Err("ENOTSUP".into())
    }
    fn transfer(
        &self,
        _id: &str,
        _destination: String,
        _cut: bool,
        _copy_title: String,
    ) -> Result<(String, u64, bool), String> {
        Err("ENOTSUP".into())
    }
}
impl KnowledgeJobs for LocalOnly {
    fn reindex(
        &self,
        _collection: String,
        _cancel: Arc<AtomicBool>,
    ) -> HostFuture<Result<(), String>> {
        ready(Err("ENOTSUP".into()))
    }
    fn embed(&self, _collection: String, _job: EmbeddingJob) -> HostFuture<Result<(), String>> {
        ready(Err("ENOTSUP".into()))
    }
}
impl AiBackgroundHost for LocalOnly {
    fn create(
        &self,
        _spec: BackgroundTaskSpec,
    ) -> Result<BackgroundTaskId, BackgroundTaskValidationError> {
        Err(BackgroundTaskValidationError::Invalid)
    }
    fn snapshots_for_owner(&self, _conversation_id: &str) -> Vec<BackgroundTaskSnapshot> {
        vec![]
    }
    fn snapshot(&self, _id: &BackgroundTaskId) -> Option<BackgroundTaskSnapshot> {
        None
    }
    fn cancel(&self, _id: &BackgroundTaskId) -> bool {
        false
    }
    fn cancel_owner(&self, _conversation_id: &str) -> usize {
        0
    }
    fn cancel_all(&self) -> usize {
        0
    }
    fn shutdown(&self) {}
}
