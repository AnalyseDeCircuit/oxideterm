mod anthropic;
mod common;
mod gemini;
mod openai;
mod openai_parse;
mod openai_payload;
mod responses;
mod responses_error;
mod responses_parse;
mod responses_payload;
mod retry;

use std::time::Duration;

use crate::{AiChatMessage, AiChatStreamConfig, AiStreamEvent};

#[cfg(test)]
pub(crate) use anthropic::parse_anthropic_data_line;
#[cfg(test)]
pub(crate) use gemini::{gemini_chat_body, gemini_chat_contents, parse_gemini_data_line};
#[cfg(test)]
pub(crate) use openai_parse::parse_openai_data_line;
#[cfg(test)]
pub(crate) use openai_payload::openai_chat_messages;

const CHAT_STREAM_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChatStreamProviderFamily {
    OpenAiCompatible,
    Anthropic,
    Gemini,
    Ollama,
}

fn chat_stream_provider_family(provider_type: &str) -> ChatStreamProviderFamily {
    match provider_type {
        "ollama" => ChatStreamProviderFamily::Ollama,
        "anthropic" => ChatStreamProviderFamily::Anthropic,
        "gemini" => ChatStreamProviderFamily::Gemini,
        "openai" | "openai_compatible" | "deepseek" | "kimi" | "glm" | "xai" | "mimo" => {
            ChatStreamProviderFamily::OpenAiCompatible
        }
        _ => ChatStreamProviderFamily::OpenAiCompatible,
    }
}

pub async fn stream_chat_completion(
    config: AiChatStreamConfig,
    mut messages: Vec<AiChatMessage>,
    events: tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
) {
    crate::prepare_image_history(&mut messages, &config);
    if messages
        .iter()
        .any(|message| !crate::message_images(message).is_empty())
        && crate::model_image_input(&config.provider_type, &config.model, config.image_input)
            == Some(false)
    {
        let _ = events.send(AiStreamEvent::Error("ai_image_model_unsupported".into()));
        return;
    }
    for attempt in 0.. {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut request = Box::pin(stream_once(config.clone(), messages.clone(), sender));
        let mut delivered_output = false;
        let result = loop {
            tokio::select! {
                result = &mut request => break result,
                Some(event) = receiver.recv() => {
                    delivered_output |= !matches!(event, AiStreamEvent::Usage { .. });
                    let failed = matches!(event, AiStreamEvent::Error(_));
                    if events.send(event).is_err() || failed { return; }
                }
            }
        };
        drop(request);
        while let Ok(event) = receiver.try_recv() {
            delivered_output |= !matches!(event, AiStreamEvent::Usage { .. });
            let failed = matches!(event, AiStreamEvent::Error(_));
            if events.send(event).is_err() || failed {
                return;
            }
        }
        match result {
            Ok(()) => return,
            Err(error) => {
                if let Some(delay) = retry::retry_delay(&error, attempt, delivered_output) {
                    // This sleep belongs to the same request future; cancellation drops it with the network stream.
                    tokio::select! {
                        _ = events.closed() => return,
                        _ = tokio::time::sleep(delay) => continue,
                    }
                }
                let error = if config.api_protocol == crate::AiApiProtocol::Responses {
                    responses_error::safe_error(&error)
                } else if chat_stream_provider_family(&config.provider_type)
                    == ChatStreamProviderFamily::OpenAiCompatible
                {
                    openai::safe_chat_error(&error)
                } else {
                    error.to_string()
                };
                let _ = events.send(AiStreamEvent::Error(error));
                return;
            }
        }
    }
}

async fn stream_once(
    config: AiChatStreamConfig,
    messages: Vec<AiChatMessage>,
    events: tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
) -> anyhow::Result<()> {
    crate::images::validate_image_request(messages.iter().flat_map(crate::message_images))?;
    let responses = config.api_protocol == crate::AiApiProtocol::Responses;
    if responses {
        if config.uses_responses() {
            responses::stream_responses(config, messages, events.clone()).await
        } else {
            Err(anyhow::anyhow!("responses_endpoint"))
        }
    } else {
        match chat_stream_provider_family(&config.provider_type) {
            ChatStreamProviderFamily::Ollama => {
                openai::stream_ollama_completion(config, messages, events.clone()).await
            }
            ChatStreamProviderFamily::Anthropic => {
                anthropic::stream_anthropic_completion(config, messages, events.clone()).await
            }
            ChatStreamProviderFamily::Gemini => {
                gemini::stream_gemini_completion(config, messages, events.clone()).await
            }
            ChatStreamProviderFamily::OpenAiCompatible => {
                openai::stream_openai_completion(config, messages, events.clone()).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ChatStreamProviderFamily, chat_stream_provider_family};

    #[test]
    fn unknown_provider_type_falls_back_to_openai_compatible_stream() {
        assert_eq!(
            chat_stream_provider_family("custom_vendor"),
            ChatStreamProviderFamily::OpenAiCompatible
        );
        assert_eq!(
            chat_stream_provider_family(""),
            ChatStreamProviderFamily::OpenAiCompatible
        );
    }
}

#[cfg(test)]
pub(crate) use responses_parse::ResponsesStream;
#[cfg(test)]
pub(crate) use responses_payload::responses_body;

/// An allowlist of transport categories, never remote error text.
pub fn stream_error_label(error: &str) -> Option<&'static str> {
    let error = provider_http_error(error).map_or(error, |(code, _)| code);
    match error {
        "ai_image_model_unsupported" => Some("ai.images.model_unavailable"),
        "ai_image_missing" => Some("ai.images.missing"),
        "ai_image_request_limit" => Some("ai.images.request_limit"),
        "agent_compaction_failed" => Some("settings_view.ai.agent_compaction_failed"),
        "agent_context_full" => Some("settings_view.ai.agent_context_full"),
        "ai_stream_interrupted" => Some("settings_view.ai.stream_interrupted"),
        "ai_output_incomplete" => Some("settings_view.ai.output_incomplete"),
        "stream_failed" => Some("settings_view.ai.stream_failed"),
        "chat_rejected" => Some("settings_view.ai.chat_rejected"),
        "chat_endpoint" => Some("settings_view.ai.chat_endpoint"),
        "chat_timeout" => Some("settings_view.ai.chat_timeout"),
        "chat_network" => Some("settings_view.ai.chat_network"),
        "chat_balance" => Some("settings_view.ai.chat_balance"),
        "chat_thinking_context" => Some("settings_view.ai.chat_thinking_context"),
        "chat_invalid_stream" => Some("settings_view.ai.chat_invalid_stream"),
        "responses_failed" => Some("settings_view.ai.responses_failed"),
        "responses_auth" | "chat_auth" => Some("settings_view.ai.responses_auth"),
        "responses_endpoint" => Some("settings_view.ai.responses_endpoint"),
        "responses_timeout" => Some("settings_view.ai.responses_timeout"),
        "responses_rate_limit" | "chat_rate_limit" => Some("settings_view.ai.responses_rate_limit"),
        "responses_unavailable" | "chat_unavailable" => {
            Some("settings_view.ai.responses_unavailable")
        }
        "responses_rejected" => Some("settings_view.ai.responses_rejected"),
        "responses_tool_context" => Some("settings_view.ai.responses_tool_context"),
        "responses_model_unavailable" | "chat_model_unavailable" => {
            Some("settings_view.ai.responses_model_unavailable")
        }
        "responses_context_limit" | "chat_context_limit" => {
            Some("settings_view.ai.responses_context_limit")
        }
        "responses_invalid_stream" => Some("settings_view.ai.responses_invalid_stream"),
        "responses_network" => Some("settings_view.ai.responses_network"),
        "responses_incomplete_limit" => Some("settings_view.ai.responses_incomplete_limit"),
        "responses_incomplete_filter" => Some("settings_view.ai.responses_incomplete_filter"),
        "responses_incomplete" | "chat_aborted" => Some("settings_view.ai.responses_incomplete"),
        "responses_disconnected" => Some("settings_view.ai.responses_disconnected"),
        _ => None,
    }
}

fn provider_http_error(error: &str) -> Option<(&str, u16)> {
    let (code, status) = error.split_once(':')?;
    if !(code.starts_with("responses_") || code.starts_with("chat_"))
        || status.len() != 3
        || !status.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let status = status.parse().ok()?;
    (300..=599).contains(&status).then_some((code, status))
}

pub fn stream_error_http_status(error: &str) -> Option<u16> {
    stream_error_label(error)?;
    provider_http_error(error).map(|(_, status)| status)
}
