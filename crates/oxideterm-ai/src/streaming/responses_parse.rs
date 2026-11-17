use std::collections::BTreeMap;

use anyhow::{Result, bail};
use serde_json::Value;

use crate::AiStreamEvent;

use super::responses_error::provider_error;

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum TextPart {
    Message,
    Reasoning,
    Summary,
}

impl TextPart {
    fn event(self, text: &str) -> AiStreamEvent {
        match self {
            Self::Message => AiStreamEvent::Content(text.into()),
            Self::Reasoning | Self::Summary => AiStreamEvent::Thinking(text.into()),
        }
    }
}

#[derive(Default)]
pub(crate) struct ResponsesStream {
    items: BTreeMap<u64, Value>,
    text: BTreeMap<(u64, u64, TextPart), String>,
    finished: bool,
}

impl ResponsesStream {
    pub(crate) fn event(&mut self, value: Value, scope: &str) -> Result<Vec<AiStreamEvent>> {
        if self.finished {
            return Ok(Vec::new());
        }
        let mut events = Vec::new();
        let kind = value["type"].as_str().unwrap_or_default();
        let index = value["output_index"].as_u64().unwrap_or(0);
        match kind {
            "response.output_text.delta"
            | "response.refusal.delta"
            | "response.reasoning_summary_text.delta"
            | "response.reasoning_text.delta" => {
                let (channel, part) = match kind {
                    "response.reasoning_summary_text.delta" => (
                        TextPart::Summary,
                        value
                            .get("summary_index")
                            .or_else(|| value.get("content_index")),
                    ),
                    "response.reasoning_text.delta" => (
                        TextPart::Reasoning,
                        value
                            .get("content_index")
                            .or_else(|| value.get("summary_index")),
                    ),
                    _ => (TextPart::Message, value.get("content_index")),
                };
                let part = part.and_then(Value::as_u64).unwrap_or(0);
                if let Some(delta) = value["delta"].as_str() {
                    self.text
                        .entry((index, part, channel))
                        .or_default()
                        .push_str(delta);
                    events.push(channel.event(delta));
                }
            }
            "response.output_item.added" => {
                if value["item"]["type"] == "function_call" {
                    self.items.insert(index, value["item"].clone());
                }
            }
            "response.function_call_arguments.delta" => {
                if let Some(item) = self.items.get_mut(&index) {
                    let mut arguments = item["arguments"].as_str().unwrap_or_default().to_owned();
                    arguments.push_str(value["delta"].as_str().unwrap_or_default());
                    item["arguments"] = Value::String(arguments.clone());
                    if let (Some(id), Some(name)) =
                        (item["call_id"].as_str(), item["name"].as_str())
                    {
                        events.push(AiStreamEvent::ToolCall {
                            id: id.into(),
                            name: name.into(),
                            arguments,
                        });
                    }
                }
            }
            "response.output_item.done" => {
                self.complete_text(index, &value["item"], &mut events)?;
                self.items.remove(&index);
            }
            "response.completed" => {
                let response = &value["response"];
                if response["status"].as_str() != Some("completed") {
                    bail!("responses_invalid_stream");
                }
                let output = response["output"]
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("responses_invalid_stream"))?;
                for (index, item) in output.iter().enumerate() {
                    self.complete_text(index as u64, item, &mut events)?;
                    match item["type"].as_str() {
                        Some("function_call") => {
                            let field = |name| {
                                item[name]
                                    .as_str()
                                    .filter(|s| !s.is_empty())
                                    .ok_or_else(|| anyhow::anyhow!("responses_tool_context"))
                            };
                            events.push(AiStreamEvent::ToolCallComplete {
                                id: field("call_id")?.into(),
                                name: field("name")?.into(),
                                arguments: field("arguments")?.into(),
                            });
                        }
                        Some("message" | "reasoning") => {}
                        _ => bail!("responses_invalid_stream"),
                    }
                }
                events.push(AiStreamEvent::ProviderResponsePart {
                    provider_type: scope.into(),
                    part: serde_json::json!({"output":output}),
                });
                events.push(AiStreamEvent::Usage {
                    input_tokens: response["usage"]["input_tokens"].as_u64(),
                    output_tokens: response["usage"]["output_tokens"].as_u64(),
                });
                events.push(AiStreamEvent::Done);
                self.finished = true;
            }
            "response.incomplete" => {
                self.finished = true;
                let reason = match value["response"]["incomplete_details"]["reason"].as_str() {
                    Some("max_output_tokens") => "responses_incomplete_limit",
                    Some("content_filter") => "responses_incomplete_filter",
                    _ => "responses_incomplete",
                };
                if let Some(output) = value["response"]["output"].as_array() {
                    for (index, item) in output.iter().enumerate() {
                        self.complete_text(index as u64, item, &mut events)?;
                    }
                }
                events.push(AiStreamEvent::Error(reason.into()));
            }
            "response.failed" | "error" => {
                self.finished = true;
                let error = value
                    .pointer("/response/error")
                    .or_else(|| value.get("error"))
                    .unwrap_or(&value);
                // Never emit provider text, which can echo the tool result or credentials.
                let reason = provider_error(error["code"].as_str(), error["message"].as_str())
                    .unwrap_or("responses_failed");
                bail!("{reason}");
            }
            _ => {}
        }
        Ok(events)
    }

    fn complete_text(
        &mut self,
        index: u64,
        item: &Value,
        events: &mut Vec<AiStreamEvent>,
    ) -> Result<()> {
        if item["type"] == "reasoning" {
            let content = item["content"].as_array();
            if let Some(content) = content {
                self.complete_parts(index, TextPart::Reasoning, content, false, events)?;
            }
            if let Some(summary) = item["summary"].as_array() {
                self.complete_parts(index, TextPart::Summary, summary, content.is_none(), events)?;
            }
        } else if let Some(content) = item["content"].as_array() {
            self.complete_parts(index, TextPart::Message, content, false, events)?;
        }
        Ok(())
    }

    fn complete_parts(
        &mut self,
        index: u64,
        channel: TextPart,
        parts: &[Value],
        reasoning_may_be_summary: bool,
        events: &mut Vec<AiStreamEvent>,
    ) -> Result<()> {
        for (part, value) in parts.iter().enumerate() {
            let part = part as u64;
            let full = value["text"]
                .as_str()
                .or_else(|| value["refusal"].as_str())
                .unwrap_or_default();
            let key = (index, part, channel);
            let sent = self.text.get(&key).or_else(|| {
                // xAI can stream reasoning text but put the same text only in the final summary.
                self.text
                    .get(&(index, part, TextPart::Reasoning))
                    .filter(|text| reasoning_may_be_summary && full.starts_with(text.as_str()))
            });
            let suffix = match sent {
                Some(sent) => full
                    .strip_prefix(sent.as_str())
                    .ok_or_else(|| anyhow::anyhow!("responses_invalid_stream"))?,
                None => full,
            };
            if !suffix.is_empty() {
                events.push(channel.event(suffix));
            }
            self.text.insert(key, full.into());
        }
        Ok(())
    }
}
