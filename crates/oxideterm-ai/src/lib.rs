mod chat;
mod key_store;
mod persistence;
mod providers;
mod settings;
mod slash;
mod streaming;
mod types;

pub use chat::{apply_chat_request_overrides, generate_chat_title};
pub use key_store::AiProviderKeyStore;
pub use persistence::AiChatPersistenceStore;
pub use providers::{
    AI_PROVIDER_TEMPLATES, active_model_or_provider_default, active_provider_view,
    check_model_selector_provider_online, fetch_provider_models, first_provider_default_model,
    generated_provider_id, is_local_provider_url, model_selector_display_name,
    model_selector_truncated_label, model_selector_visible_provider_groups,
    new_provider_from_template, provider_id, provider_string, provider_template_by_type,
    provider_view, provider_views, resolve_model_selector_provider_probe, update_provider,
};
pub use settings::{
    AiProviderKeyDisplayState, AiProviderRefreshKeyPolicy, add_provider_from_template,
    apply_provider_model_refresh, model_max_response_tokens, provider_chat_requires_key,
    provider_key_display_state, provider_refresh_key_policy, remove_provider_at,
    select_provider_model, set_active_provider_selection, set_provider_default_model,
    take_provider_key_secret,
};
pub use slash::{
    AI_SLASH_COMMANDS, AiParsedInput, AiSlashCommand, ai_help_markdown, parse_ai_user_input,
    resolve_ai_slash_command, slash_task_system_prompt,
};
pub use streaming::stream_chat_completion;
pub use types::{
    AiChatMessage, AiChatRole, AiChatState, AiChatStreamConfig, AiConversation, AiProviderTemplate,
    AiProviderView, AiStreamEvent, ModelSelectorProviderGroup, ModelSelectorProviderProbe,
    ProviderModelRefresh,
};

#[cfg(test)]
mod tests;
