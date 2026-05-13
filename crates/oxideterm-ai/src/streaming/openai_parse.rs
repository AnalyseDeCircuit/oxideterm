use anyhow::Result;
use serde_json::Value;

use crate::AiStreamEvent;
use crate::providers::parse_provider_json;

use super::common::ParsedStreamLine;

pub(crate) fn parse_openai_data_line(line: &str) -> ParsedStreamLine {
    let Some(data) = line.strip_prefix("data: ") else {
        return ParsedStreamLine {
            events: Vec::new(),
            saw_frame: false,
        };
    };
    let data = data.trim();
    if data == "[DONE]" {
        return ParsedStreamLine {
            events: vec![AiStreamEvent::Done],
            saw_frame: true,
        };
    }

    let mut events = Vec::new();
    if let Ok(json) = serde_json::from_str::<Value>(data) {
        let delta = json
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("delta"));
        if let Some(delta) = delta {
            if let Some(reasoning) = delta
                .get("reasoning_content")
                .or_else(|| delta.get("reasoning"))
                .and_then(Value::as_str)
                .filter(|reasoning| !reasoning.is_empty())
            {
                events.push(AiStreamEvent::Thinking(reasoning.to_string()));
            }
            if let Some(content) = delta
                .get("content")
                .and_then(Value::as_str)
                .filter(|content| !content.is_empty())
            {
                events.push(AiStreamEvent::Content(content.to_string()));
            }
        }
    }
    ParsedStreamLine {
        events,
        saw_frame: true,
    }
}

pub(crate) fn parse_openai_json_events(body: &str, context: &str) -> Result<Vec<AiStreamEvent>> {
    let json = parse_provider_json(body, context)?;
    let payload = json
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message").or_else(|| choice.get("delta")));
    let mut events = Vec::new();
    if let Some(payload) = payload {
        if let Some(reasoning) = payload
            .get("reasoning_content")
            .or_else(|| payload.get("reasoning"))
            .and_then(Value::as_str)
            .filter(|reasoning| !reasoning.is_empty())
        {
            events.push(AiStreamEvent::Thinking(reasoning.to_string()));
        }
        if let Some(content) = payload
            .get("content")
            .and_then(Value::as_str)
            .filter(|content| !content.is_empty())
        {
            events.push(AiStreamEvent::Content(content.to_string()));
        }
    }
    Ok(events)
}

pub(crate) fn parse_openai_error(status: u16, body: &str) -> String {
    let mut fallback = format!("API error: {status}");
    if let Ok(json) = serde_json::from_str::<Value>(body) {
        if let Some(message) = json
            .get("error")
            .and_then(|error| error.get("message"))
            .or_else(|| json.get("message"))
            .and_then(Value::as_str)
        {
            return message.to_string();
        }
    } else if !body.is_empty() {
        fallback = body.chars().take(200).collect();
    }
    fallback
}

pub(crate) fn parse_ollama_error(status: u16, body: &str) -> String {
    if status == 0 || body.contains("ECONNREFUSED") {
        return "Cannot connect to Ollama. Make sure Ollama is running (ollama serve).".to_string();
    }
    let mut fallback = format!("Ollama error: {status}");
    if let Ok(json) = serde_json::from_str::<Value>(body) {
        if let Some(message) = json.get("error").and_then(Value::as_str).or_else(|| {
            json.get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
        }) {
            return message.to_string();
        }
    } else if !body.is_empty() {
        fallback = body.chars().take(200).collect();
    }
    fallback
}
