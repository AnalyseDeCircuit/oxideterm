mod anthropic;
mod common;
mod gemini;
mod openai;
mod openai_parse;
mod openai_payload;

use std::time::Duration;

use crate::{AiChatMessage, AiChatStreamConfig, AiStreamEvent};

#[cfg(test)]
pub(crate) use anthropic::{anthropic_chat_messages, parse_anthropic_data_line};
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
        "openai" | "openai_compatible" | "deepseek" => ChatStreamProviderFamily::OpenAiCompatible,
        _ => ChatStreamProviderFamily::OpenAiCompatible,
    }
}

pub async fn stream_chat_completion(
    config: AiChatStreamConfig,
    messages: Vec<AiChatMessage>,
    events: tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
) {
    let result = match chat_stream_provider_family(&config.provider_type) {
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
    };

    if let Err(error) = result {
        let _ = events.send(AiStreamEvent::Error(error.to_string()));
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

    #[test]
    fn known_provider_types_keep_their_stream_family() {
        assert_eq!(
            chat_stream_provider_family("openai"),
            ChatStreamProviderFamily::OpenAiCompatible
        );
        assert_eq!(
            chat_stream_provider_family("openai_compatible"),
            ChatStreamProviderFamily::OpenAiCompatible
        );
        assert_eq!(
            chat_stream_provider_family("deepseek"),
            ChatStreamProviderFamily::OpenAiCompatible
        );
        assert_eq!(
            chat_stream_provider_family("anthropic"),
            ChatStreamProviderFamily::Anthropic
        );
        assert_eq!(
            chat_stream_provider_family("gemini"),
            ChatStreamProviderFamily::Gemini
        );
        assert_eq!(
            chat_stream_provider_family("ollama"),
            ChatStreamProviderFamily::Ollama
        );
    }
}
