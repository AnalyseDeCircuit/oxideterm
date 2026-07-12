mod acp;
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
pub mod stream_state;
mod streaming;
mod suggestions;
mod target_projection;
mod tool_protocol;
mod tool_result_protocol;
mod types;

pub use acp::{
    AcpAgentRuntime, AcpClientEvent, AcpClientEventSender, AcpHostCapabilityPolicy,
    AcpLaunchConfig, AcpLaunchConfigError, AcpPermissionOptionProjection,
    AcpPermissionRequestProjection, AcpPromptSessionOutcome, AcpRegisteredRuntimeHandle,
    AcpRuntimeHandleKey, AcpRuntimeRegistry, AcpStdioLauncher, AcpTerminalRegistry,
    acp_client_event_to_ai_stream_events, acp_launch_command_available, acp_method_not_found,
    acp_permission_cancelled_response, acp_permission_request_projection,
    acp_permission_response_for_decision, acp_session_notification_to_ai_stream_events,
    build_acp_initialize_request, build_acp_stdio_launcher, build_sdk_acp_agent,
    initialize_acp_agent, resolve_acp_read_text_file_request, resolve_acp_write_text_file_request,
    run_acp_prompt_session_events, with_acp_agent_runtime, with_acp_agent_runtime_events,
};
pub use chat::{apply_chat_request_overrides, generate_chat_title};
pub use context_sanitizer::{sanitize_api_messages_for_provider, sanitize_for_ai};
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
pub use persistence::{AiChatPersistenceStore, PersistedDiagnosticEvent, PersistedTranscriptEntry};
pub use policy::{
    AiActionRisk, AiPolicyDecision, AiPolicyDecisionKind, AiPolicySafetyMode, AiToolUsePolicy,
    denied_commands, has_denied_commands, is_command_denied, is_orchestrator_tool_name,
    orchestrator_approval_key_for_tool, orchestrator_risk_for_tool, resolve_ai_policy_decision,
};
pub use profiles::{AiExecutionBackend, resolve_ai_reasoning_effort, tool_policy_from_parts};
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
    AiDetectedIntent, AiInputTokenAtCursor, AiInputTokenType, AiParsedInput, AiParticipantDef,
    AiParticipantMatch, AiReferenceDef, AiReferenceMatch, AiSlashCommand,
    ai_autocomplete_candidates, ai_detected_intent_system_prompt, ai_help_markdown,
    ai_input_system_prompt, ai_input_token_at_cursor, apply_ai_autocomplete_candidate,
    detect_ai_intent, parse_ai_user_input, resolve_ai_participant, resolve_ai_reference,
    resolve_ai_slash_command, slash_task_system_prompt,
};
pub use stream_state::*;
pub use streaming::stream_chat_completion;
pub use suggestions::{
    AiSuggestionParseResult, ai_has_partial_suggestions_block, ai_visible_suggestion_content,
    parse_ai_suggestions,
};
pub use target_projection::{
    AiIdeTargetInput, AiRawTcpTargetInput, AiRawUdpTargetInput, AiSftpTargetInput,
    AiTargetProjection, connect_result_terminal_projection, ide_workspace_target_projection,
    opened_local_terminal_projection, raw_tcp_terminal_label, raw_tcp_terminal_metadata,
    raw_udp_terminal_label, raw_udp_terminal_metadata, sftp_target_projection,
};
pub use tool_protocol::{
    AiOrchestratorObligation, AiOrchestratorObligationMode, ai_classify_orchestrator_obligation,
    ai_orchestrator_obligation_prompt, ai_required_tool_retry_prompt,
    ai_should_retry_required_tool_round, ai_should_retry_required_tool_round_for_turn,
    ai_should_trigger_hard_deny, ai_text_contains_tauri_action_claim,
    ai_user_explicitly_requested_json,
};
pub use tool_result_protocol::{
    AI_TOOL_CONDENSE_KEEP_RECENT, AI_TOOL_CONDENSE_SUMMARY_MAX_CHARS,
    AI_TOOL_MODEL_ERROR_MESSAGE_MAX_CHARS, AI_TOOL_MODEL_ERROR_OUTPUT_MAX_CHARS,
    AI_TOOL_MODEL_OUTPUT_MAX_CHARS, AI_TOOL_MODEL_SUMMARY_MAX_CHARS, AiExecutedToolResult,
    ai_insert_execution_shortcuts_for_model, ai_insert_non_empty_model_array,
    ai_to_usable_budget_threshold, ai_tool_result_envelope_or_legacy,
    ai_tool_result_evidence_facts_for_model, ai_tool_result_model_content,
    ai_tool_result_model_error, condense_ai_tool_messages, truncate_ai_tool_result_for_model,
};
pub use types::{
    AiChatMessage, AiChatMessageMetadata, AiChatRole, AiChatState, AiChatStreamConfig,
    AiConversation, AiFollowUpSuggestion, AiMessageBranches, AiProviderTemplate, AiProviderView,
    AiStreamEvent, AiToolCall, AiToolChoice, AiToolDefinition, ModelSelectorProviderGroup,
    ModelSelectorProviderProbe, ProviderModelRefresh,
};

#[cfg(test)]
mod tests;
