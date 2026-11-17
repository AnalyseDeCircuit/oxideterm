use super::common::{ErrorEnvelope, read_provider_error_body};
use super::retry::TransientFailure;

pub(super) fn provider_error(code: Option<&str>, message: Option<&str>) -> Option<&'static str> {
    match code {
        Some("invalid_api_key" | "permission_denied") => Some("responses_auth"),
        Some("rate_limit_exceeded" | "insufficient_quota") => Some("responses_rate_limit"),
        Some("model_not_found") => Some("responses_model_unavailable"),
        Some("context_length_exceeded") => Some("responses_context_limit"),
        Some("server_error" | "overloaded") => Some("responses_unavailable"),
        Some("invalid_tool_call" | "invalid_function_call_output" | "missing_tool_output") => {
            Some("responses_tool_context")
        }
        _ if message.is_some_and(|message| {
            [
                "function_call_output requires call_id",
                "function_call_output requires item_reference ids matching each call_id",
                "No tool output found for function call",
                "No tool call found for function call output",
            ]
            .iter()
            .any(|prefix| message.starts_with(prefix))
        }) =>
        {
            Some("responses_tool_context")
        }
        _ => None,
    }
}

pub(super) async fn rejected_request(mut response: reqwest::Response) -> String {
    let status = response.status().as_u16();
    if matches!(status, 400 | 422) {
        // Only inspect bounded validation errors, and output a locally defined category.
        let body = read_provider_error_body(&mut response).await;
        if let Ok(envelope) = serde_json::from_slice::<ErrorEnvelope<'_>>(&body)
            && let Some(error) = envelope.error
            && let Some(reason) = provider_error(error.code.as_deref(), error.message.as_deref())
        {
            return format!("{reason}:{status}");
        }
    }
    http_error(status)
}

fn http_error(status: u16) -> String {
    let reason = match status {
        401 | 403 => "responses_auth",
        404 | 405 => "responses_endpoint",
        408 | 504 => "responses_timeout",
        429 => "responses_rate_limit",
        500..=599 => "responses_unavailable",
        _ => "responses_rejected",
    };
    format!("{reason}:{status}")
}

pub(super) fn safe_error(error: &anyhow::Error) -> String {
    if let Some(failure) = error.downcast_ref::<TransientFailure>() {
        return http_error(failure.status);
    }
    if let Some(error) = error.downcast_ref::<reqwest::Error>() {
        return if error.is_timeout() {
            "responses_timeout"
        } else {
            "responses_network"
        }
        .into();
    }
    let code = error.to_string();
    if super::stream_error_label(&code).is_some() {
        code
    } else {
        "responses_failed".into()
    }
}
