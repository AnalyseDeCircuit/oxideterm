use std::collections::BTreeMap;

use anyhow::{Result, bail};
use serde_json::Value;

use crate::AiStreamEvent;

#[derive(Default)]
pub(crate) struct ResponsesStream {
    items: BTreeMap<u64, Value>,
    text: BTreeMap<(u64, u64, bool), String>,
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
            | "response.reasoning_summary_text.delta" => {
                let thinking = kind == "response.reasoning_summary_text.delta";
                let part = value[if thinking {
                    "summary_index"
                } else {
                    "content_index"
                }]
                .as_u64()
                .unwrap_or(0);
                if let Some(delta) = value["delta"].as_str() {
                    self.text
                        .entry((index, part, thinking))
                        .or_default()
                        .push_str(delta);
                    events.push(if thinking {
                        AiStreamEvent::Thinking(delta.into())
                    } else {
                        AiStreamEvent::Content(delta.into())
                    });
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
                    bail!("Responses provider returned an invalid completion status");
                }
                let output = response["output"].as_array().ok_or_else(|| {
                    anyhow::anyhow!("Responses provider omitted completed output")
                })?;
                for (index, item) in output.iter().enumerate() {
                    self.complete_text(index as u64, item, &mut events)?;
                    match item["type"].as_str() {
                        Some("function_call") => {
                            let field = |name| {
                                item[name].as_str().filter(|s| !s.is_empty()).ok_or_else(|| anyhow::anyhow!("Responses provider returned an incomplete function call"))
                            };
                            events.push(AiStreamEvent::ToolCallComplete {
                                id: field("call_id")?.into(),
                                name: field("name")?.into(),
                                arguments: field("arguments")?.into(),
                            });
                        }
                        Some("message" | "reasoning") => {}
                        _ => bail!("Responses provider returned an unsupported output item"),
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
                // Remote errors can echo request content and credentials. Do not expose their body.
                bail!("Responses provider failed to generate a response");
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
        let thinking = item["type"] == "reasoning";
        if let Some(parts) = item[if thinking { "summary" } else { "content" }].as_array() {
            for (part, value) in parts.iter().enumerate() {
                let full = value["text"]
                    .as_str()
                    .or_else(|| value["refusal"].as_str())
                    .unwrap_or_default();
                let sent = self.text.entry((index, part as u64, thinking)).or_default();
                if !full.starts_with(sent.as_str()) {
                    bail!("Responses provider changed previously streamed text");
                }
                let suffix = &full[sent.len()..];
                if !suffix.is_empty() {
                    events.push(if thinking {
                        AiStreamEvent::Thinking(suffix.into())
                    } else {
                        AiStreamEvent::Content(suffix.into())
                    });
                }
                *sent = full.into();
            }
        }
        Ok(())
    }
}
