mod anthropic;
mod common;
mod gemini;
mod openai;
mod openai_parse;
mod openai_payload;

use std::time::Duration;

use anyhow::anyhow;

use crate::{AiChatMessage, AiChatStreamConfig, AiStreamEvent};

#[cfg(test)]
pub(crate) use anthropic::{anthropic_chat_messages, parse_anthropic_data_line};
#[cfg(test)]
pub(crate) use gemini::{gemini_chat_contents, parse_gemini_data_line};
#[cfg(test)]
pub(crate) use openai_parse::parse_openai_data_line;
#[cfg(test)]
pub(crate) use openai_payload::openai_chat_messages;

const CHAT_STREAM_TIMEOUT: Duration = Duration::from_secs(120);

pub async fn stream_chat_completion(
    config: AiChatStreamConfig,
    messages: Vec<AiChatMessage>,
    events: tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
) {
    let result = match config.provider_type.as_str() {
        "ollama" => openai::stream_ollama_completion(config, messages, events.clone()).await,
        "anthropic" => {
            anthropic::stream_anthropic_completion(config, messages, events.clone()).await
        }
        "gemini" => gemini::stream_gemini_completion(config, messages, events.clone()).await,
        "openai" | "openai_compatible" | "deepseek" => {
            openai::stream_openai_completion(config, messages, events.clone()).await
        }
        provider => Err(anyhow!("unsupported AI provider type: {provider}")),
    };

    if let Err(error) = result {
        let _ = events.send(AiStreamEvent::Error(error.to_string()));
    }
}
