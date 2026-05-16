mod chat;
mod context_sanitizer;
mod context_window;
mod key_store;
mod mcp;
mod orchestrator;
mod persistence;
mod policy;
mod profiles;
mod provider_embeddings;
mod providers;
mod rag;
mod references;
mod settings;
mod slash;
mod streaming;
mod types;

pub use chat::{apply_chat_request_overrides, generate_chat_title};
pub use context_sanitizer::sanitize_for_ai;
pub use context_window::{
    ContextWindowSource, DEFAULT_CONTEXT_WINDOW, ModelContextWindowInfo,
    extract_context_window_from_model_name, model_context_window, model_context_window_info,
};
pub use key_store::AiProviderKeyStore;
pub use mcp::{
    McpAuthHeaderMode, McpCallToolResult, McpRegistry, McpResource, McpResourceContent,
    McpServerConfig, McpServerStateSnapshot, McpTransport, is_mcp_tool_name, mcp_resource_output,
    mcp_tool_output,
};
pub use orchestrator::orchestrator_tool_definitions;
pub use persistence::AiChatPersistenceStore;
pub use policy::{
    AiActionRisk, AiPolicyDecision, AiPolicyDecisionKind, AiPolicySafetyMode, AiToolUsePolicy,
    denied_commands, has_denied_commands, is_command_denied, is_orchestrator_tool_name,
    orchestrator_approval_key_for_tool, orchestrator_risk_for_tool, resolve_ai_policy_decision,
};
pub use profiles::{
    ResolvedAiExecutionProfile, resolve_ai_execution_profile, resolve_ai_reasoning_effort,
    tool_policy_from_parts,
};
pub use provider_embeddings::{
    AiChatEmbeddingApiKeyDecision, AiEmbeddingMode, AiEmbeddingProviderReason,
    ResolvedAiEmbeddingProvider, ai_embedding_requires_api_key, ai_provider_supports_embeddings,
    embed_texts, resolve_ai_embedding_provider, resolve_chat_embedding_api_key,
};
pub use providers::{
    AI_PROVIDER_TEMPLATES, active_model_or_provider_default, active_provider_view,
    check_model_selector_provider_online, fetch_provider_models, first_provider_default_model,
    generated_provider_id, is_local_provider_url, model_selector_display_name,
    model_selector_truncated_label, model_selector_visible_provider_groups,
    new_provider_from_template, provider_id, provider_string, provider_template_by_type,
    provider_view, provider_views, resolve_model_selector_provider_probe, update_provider,
};
pub use rag::{
    AddDocumentRequest as RagAddDocumentRequest, CollectionResponse as RagCollectionResponse,
    CreateBlankDocumentRequest as RagCreateBlankDocumentRequest,
    CreateCollectionRequest as RagCreateCollectionRequest, DocScope,
    DocScopeRequest as RagDocScopeRequest, DocumentResponse as RagDocumentResponse,
    EmbeddingInputRequest as RagEmbeddingInputRequest, PaginatedDocuments as RagPaginatedDocuments,
    PendingEmbeddingResponse as RagPendingEmbeddingResponse, RagStore,
    SearchRequest as RagSearchRequest, SearchResultResponse as RagSearchResultResponse,
    StatsResponse as RagStatsResponse, StoreEmbeddingsRequest as RagStoreEmbeddingsRequest,
    rag_add_document, rag_create_blank_document, rag_create_collection, rag_delete_collection,
    rag_get_collection_stats, rag_get_document_content, rag_get_pending_embeddings,
    rag_list_collections, rag_list_documents, rag_reindex_collection,
    rag_reindex_collection_with_progress, rag_remove_document, rag_search, rag_store_embeddings,
    rag_update_document,
};
pub use references::{
    ai_reference_context_block, ai_reference_label, current_terminal_context_system_message,
    extract_ai_error_context, infer_ai_cwd,
};
pub use settings::{
    AiProviderKeyDisplayState, AiProviderRefreshKeyPolicy, add_provider_from_template,
    apply_provider_model_refresh, model_max_response_tokens, provider_chat_requires_key,
    provider_key_display_state, provider_refresh_key_policy, remove_provider_at,
    remove_provider_at_with_scoped_settings, select_provider_model, set_active_provider_selection,
    set_provider_default_model, take_provider_key_secret,
};
pub use slash::{
    AI_PARTICIPANTS, AI_REFERENCES, AI_SLASH_COMMANDS, AiAutocompleteCandidate, AiAutocompleteKind,
    AiInputTokenAtCursor, AiInputTokenType, AiParsedInput, AiParticipantDef, AiParticipantMatch,
    AiReferenceDef, AiReferenceMatch, AiSlashCommand, ai_autocomplete_candidates, ai_help_markdown,
    ai_input_system_prompt, ai_input_token_at_cursor, apply_ai_autocomplete_candidate,
    parse_ai_user_input, resolve_ai_participant, resolve_ai_reference, resolve_ai_slash_command,
    slash_task_system_prompt,
};
pub use streaming::stream_chat_completion;
pub use types::{
    AiChatMessage, AiChatMessageMetadata, AiChatRole, AiChatState, AiChatStreamConfig,
    AiConversation, AiMessageBranches, AiProviderTemplate, AiProviderView, AiStreamEvent,
    AiToolCall, AiToolChoice, AiToolDefinition, ModelSelectorProviderGroup,
    ModelSelectorProviderProbe, ProviderModelRefresh,
};

#[cfg(test)]
mod tests;
