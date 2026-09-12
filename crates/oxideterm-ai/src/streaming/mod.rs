mod anthropic;
mod common;
mod gemini;
mod openai;
mod openai_parse;
mod openai_payload;
mod responses;
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
        "openai" | "openai_compatible" | "deepseek" | "kimi" | "glm" => {
            ChatStreamProviderFamily::OpenAiCompatible
        }
        _ => ChatStreamProviderFamily::OpenAiCompatible,
    }
}

pub async fn stream_chat_completion(
    config: AiChatStreamConfig,
    messages: Vec<AiChatMessage>,
    events: tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
) {
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
            if events.send(event).is_err() || failed { return; }
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
                let error = error.to_string();
                let error = if config.api_protocol == crate::AiApiProtocol::Responses
                    && crate::stream_error_label(&error).is_none() {
                    "responses_failed".to_string()
                } else { error };
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
    let responses = config.api_protocol == crate::AiApiProtocol::Responses;
    if responses {
        if config.uses_responses() {
            responses::stream_responses(config, messages, events.clone()).await
        } else {
            Err(anyhow::anyhow!("Responses requires an OpenAI-compatible provider"))
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
    match error {
        "agent_compaction_failed" => Some("settings_view.ai.agent_compaction_failed"),
        "agent_context_full" => Some("settings_view.ai.agent_context_full"),
        "ai_stream_interrupted" => Some("settings_view.ai.stream_interrupted"),
        "ai_output_incomplete" => Some("settings_view.ai.output_incomplete"),
        "responses_failed" => Some("settings_view.ai.responses_failed"),
        "responses_incomplete_limit" => Some("settings_view.ai.responses_incomplete_limit"),
        "responses_incomplete_filter" => Some("settings_view.ai.responses_incomplete_filter"),
        "responses_incomplete" => Some("settings_view.ai.responses_incomplete"),
        "responses_disconnected" => Some("settings_view.ai.responses_disconnected"),
        _ => None,
    }
}
