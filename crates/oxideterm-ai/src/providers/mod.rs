mod catalog;
#[cfg(not(target_family = "wasm"))]
mod discovery;
#[cfg(not(target_family = "wasm"))]
mod discovery_http;
#[cfg(not(target_family = "wasm"))]
mod discovery_models;
mod selector;

pub use catalog::{
    AI_PROVIDER_TEMPLATES, active_model_selection, active_provider_view, generated_provider_id,
    new_provider_from_template, provider_id, provider_string, provider_template_by_type,
    provider_view, provider_views, update_provider,
};
#[cfg(not(target_family = "wasm"))]
pub use discovery::fetch_provider_models;
#[cfg(not(target_family = "wasm"))]
pub(crate) use discovery::{
    ANTHROPIC_VERSION, api_key_required_ref, gemini_api_base_url, looks_like_html_response,
    openai_compatible_candidates, parse_provider_json, url_encode_component,
};
#[cfg(all(test, not(target_family = "wasm")))]
pub(crate) use discovery::{parse_provider_context_windows, parse_provider_models};
#[cfg(not(target_family = "wasm"))]
pub use selector::{
    check_model_selector_provider_online, check_openai_compatible_model_selector_provider_online,
};
pub use selector::{
    is_local_provider_url, model_selector_display_name, model_selector_truncated_label,
    model_selector_visible_provider_groups, resolve_model_selector_provider_probe,
};
