mod catalog;
mod discovery;
mod discovery_http;
mod discovery_models;
mod selector;

pub use catalog::{
    AI_PROVIDER_TEMPLATES, active_model_or_provider_default, active_provider_view,
    first_provider_default_model, generated_provider_id, new_provider_from_template, provider_id,
    provider_string, provider_template_by_type, provider_view, provider_views, update_provider,
};
pub use discovery::fetch_provider_models;
pub(crate) use discovery::{
    ANTHROPIC_VERSION, api_key_required_ref, looks_like_html_response,
    openai_compatible_candidates, parse_provider_json, url_encode_component,
};
#[cfg(test)]
pub(crate) use discovery::{parse_provider_context_windows, parse_provider_models};
pub use selector::{
    check_model_selector_provider_online, is_local_provider_url, model_selector_display_name,
    model_selector_truncated_label, model_selector_visible_provider_groups,
    resolve_model_selector_provider_probe,
};
