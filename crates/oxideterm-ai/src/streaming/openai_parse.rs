use std::collections::BTreeMap;

use anyhow::Result;
use serde_json::Value;

use crate::AiStreamEvent;
use crate::providers::parse_provider_json;

use super::common::ParsedStreamLine;

#[cfg(test)]
pub(crate) fn parse_openai_data_line(line: &str) -> ParsedStreamLine {
    let mut accumulator = OpenAiToolAccumulator::default();
    parse_openai_data_line_with_accumulator(line, &mut accumulator)
}

#[derive(Default)]
pub(crate) struct OpenAiToolAccumulator {
    calls: BTreeMap<usize, OpenAiToolCallChunk>,
}

#[derive(Default)]
struct OpenAiToolCallChunk {
    id: String,
    name: String,
    arguments: String,
}

pub(crate) fn parse_openai_data_line_with_accumulator(
    line: &str,
    accumulator: &mut OpenAiToolAccumulator,
) -> ParsedStreamLine {
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
            collect_tool_call_delta(delta, accumulator, &mut events);
        }
        let finish_reason = json
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("finish_reason"))
            .and_then(Value::as_str);
        if matches!(finish_reason, Some("tool_calls" | "function_call")) {
            events.extend(accumulator.complete());
        }
    }
    ParsedStreamLine {
        events,
        saw_frame: true,
    }
}

fn collect_tool_call_delta(
    delta: &Value,
    accumulator: &mut OpenAiToolAccumulator,
    events: &mut Vec<AiStreamEvent>,
) {
    let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) else {
        return;
    };
    for (fallback_index, call) in calls.iter().enumerate() {
        let index = call
            .get("index")
            .and_then(Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
            .unwrap_or(fallback_index);
        let chunk = accumulator.calls.entry(index).or_default();
        if let Some(id) = call.get("id").and_then(Value::as_str) {
            chunk.id = id.to_string();
        }
        if let Some(function) = call.get("function") {
            if let Some(name) = function.get("name").and_then(Value::as_str) {
                chunk.name.push_str(name);
            }
            if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
                chunk.arguments.push_str(arguments);
            }
        }
        if !chunk.id.is_empty() || !chunk.name.is_empty() || !chunk.arguments.is_empty() {
            events.push(AiStreamEvent::ToolCall {
                id: chunk.id.clone(),
                name: chunk.name.clone(),
                arguments: chunk.arguments.clone(),
            });
        }
    }
}

impl OpenAiToolAccumulator {
    fn complete(&mut self) -> Vec<AiStreamEvent> {
        let calls = std::mem::take(&mut self.calls);
        calls
            .into_values()
            .filter(|call| !call.id.is_empty() || !call.name.is_empty())
            .map(|call| AiStreamEvent::ToolCallComplete {
                id: call.id,
                name: call.name,
                arguments: call.arguments,
            })
            .collect()
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
        if let Some(calls) = payload.get("tool_calls").and_then(Value::as_array) {
            events.extend(calls.iter().filter_map(openai_tool_call_complete_event));
        }
    }
    Ok(events)
}

fn openai_tool_call_complete_event(call: &Value) -> Option<AiStreamEvent> {
    let id = call.get("id").and_then(Value::as_str)?;
    let function = call.get("function")?;
    let name = function.get("name").and_then(Value::as_str)?;
    let arguments = function
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Some(AiStreamEvent::ToolCallComplete {
        id: id.to_string(),
        name: name.to_string(),
        arguments: arguments.to_string(),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_parser_assembles_openai_tool_call_chunks() {
        let mut accumulator = OpenAiToolAccumulator::default();
        let first = parse_openai_data_line_with_accumulator(
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call-1","type":"function","function":{"name":"run_","arguments":"{\"command\""}}]},"finish_reason":null}]}"#,
            &mut accumulator,
        );
        assert_eq!(
            first.events,
            vec![AiStreamEvent::ToolCall {
                id: "call-1".to_string(),
                name: "run_".to_string(),
                arguments: "{\"command\"".to_string(),
            }]
        );

        let second = parse_openai_data_line_with_accumulator(
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"command","arguments":":\"pwd\"}"}}]},"finish_reason":"tool_calls"}]}"#,
            &mut accumulator,
        );
        assert_eq!(
            second.events,
            vec![
                AiStreamEvent::ToolCall {
                    id: "call-1".to_string(),
                    name: "run_command".to_string(),
                    arguments: "{\"command\":\"pwd\"}".to_string(),
                },
                AiStreamEvent::ToolCallComplete {
                    id: "call-1".to_string(),
                    name: "run_command".to_string(),
                    arguments: "{\"command\":\"pwd\"}".to_string(),
                }
            ]
        );
    }

    #[test]
    fn json_parser_extracts_openai_tool_call_complete() {
        let events = parse_openai_json_events(
            r#"{"choices":[{"message":{"tool_calls":[{"id":"call-1","type":"function","function":{"name":"get_state","arguments":"{\"scope\":\"active\"}"}}]}}]}"#,
            "test",
        )
        .unwrap();
        assert_eq!(
            events,
            vec![AiStreamEvent::ToolCallComplete {
                id: "call-1".to_string(),
                name: "get_state".to_string(),
                arguments: "{\"scope\":\"active\"}".to_string(),
            }]
        );
    }
}
