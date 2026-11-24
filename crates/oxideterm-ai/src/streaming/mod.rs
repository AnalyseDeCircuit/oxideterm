use crate::stream_error_label;

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
pub(crate) use responses_parse::ResponsesStream;
#[cfg(test)]
pub(crate) use responses_payload::responses_body;
