fn ai_compaction_plan(
    messages: &[AiChatMessage],
    context_window: usize,
    silent: bool,
) -> Option<AiCompactionPlan> {
    if messages.len() < 4 {
        return None;
    }
    let total_tokens = messages
        .iter()
        .map(ai_message_estimated_tokens)
        .sum::<usize>();
    let mut budget = ((context_window as f32) * 0.4).floor() as usize;
    if !silent && total_tokens > 0 {
        budget = budget.min(((total_tokens as f32) * 0.6).floor() as usize);
    }
    let budget = budget.max(1);
    let mut keep_start = messages.len();
    let mut used = 0usize;
    for (index, message) in messages.iter().enumerate().rev() {
        let tokens = ai_message_estimated_tokens(message);
        if keep_start < messages.len() && used.saturating_add(tokens) > budget {
            break;
        }
        used = used.saturating_add(tokens);
        keep_start = index;
    }
    if keep_start < 2 {
        return None;
    }
    let compact_messages = messages[..keep_start].to_vec();
    if compact_messages.len() < 2 {
        return None;
    }
    let keep_messages = messages[keep_start..].to_vec();
    Some(AiCompactionPlan {
        compact_messages,
        keep_messages,
    })
}

fn ai_compaction_summary_messages(messages: &[AiChatMessage]) -> Vec<AiChatMessage> {
    let mut history_parts = Vec::new();
    for message in messages {
        if message
            .metadata
            .as_ref()
            .is_some_and(|metadata| metadata.kind == "compaction-anchor")
        {
            let summary = message.content.trim();
            if !summary.is_empty() {
                history_parts.push(format!("[Previous Summary]: {summary}"));
            }
        } else if matches!(message.role, AiChatRole::User | AiChatRole::Assistant) {
            let role = match message.role {
                AiChatRole::User => "User",
                AiChatRole::Assistant => "Assistant",
                _ => unreachable!(),
            };
            history_parts.push(format!("{role}: {}", message.content.trim()));
        }
    }
    vec![
        AiChatMessage {
            id: "compact-system".to_string(),
            role: AiChatRole::System,
            content: "Summarize the following conversation in a concise paragraph. Capture the key topics, questions asked, solutions provided, and any important context. Write in the same language as the conversation. Keep it under 200 words. If there is a \"[Previous Summary]\" section, integrate it into your summary.".to_string(),
            timestamp_ms: 0,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        },
        AiChatMessage {
            id: "compact-request".to_string(),
            role: AiChatRole::User,
            content: history_parts.join("\n\n"),
            timestamp_ms: 0,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        },
    ]
}

fn ai_conversation_summary_messages(messages: &[AiChatMessage]) -> Vec<AiChatMessage> {
    let history_text = messages
        .iter()
        .filter(|message| {
            matches!(
                message.role,
                AiChatRole::User | AiChatRole::Assistant | AiChatRole::Tool
            )
        })
        .map(|message| {
            let role = if message.role == AiChatRole::User {
                "User"
            } else {
                "Assistant"
            };
            format!("{role}: {}", message.content.trim())
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    vec![
        AiChatMessage {
            id: "summary-system".to_string(),
            role: AiChatRole::System,
            content: "Summarize the following conversation in a concise paragraph. Capture the key topics, questions asked, solutions provided, and any important context. Write in the same language as the conversation. Keep it under 200 words.".to_string(),
            timestamp_ms: 0,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        },
        AiChatMessage {
            id: "summary-request".to_string(),
            role: AiChatRole::User,
            content: history_text,
            timestamp_ms: 0,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        },
    ]
}

const AI_MAX_ANCHOR_SNAPSHOT: usize = 50;

fn ai_compaction_original_count(messages: &[AiChatMessage]) -> usize {
    messages
        .iter()
        .map(|message| {
            if message
                .metadata
                .as_ref()
                .is_some_and(|metadata| metadata.kind == "compaction-anchor")
            {
                message
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.original_count)
                    .unwrap_or(0)
            } else {
                1
            }
        })
        .sum()
}

fn ai_compaction_anchor_snapshot(messages: &[AiChatMessage]) -> Vec<AiChatMessage> {
    messages
        .iter()
        .filter(|message| {
            !message
                .metadata
                .as_ref()
                .is_some_and(|metadata| metadata.kind == "compaction-anchor")
        })
        .rev()
        .take(AI_MAX_ANCHOR_SNAPSHOT)
        .map(|message| {
            let mut snapshot = message.clone();
            snapshot.context = None;
            snapshot.is_streaming = false;
            snapshot.thinking_content = None;
            snapshot.metadata = None;
            snapshot.tool_call_id = None;
            snapshot.tool_calls.clear();
            snapshot.turn = None;
            snapshot.transcript_ref = None;
            snapshot.summary_ref = None;
            snapshot.branches = None;
            snapshot.suggestions.clear();
            snapshot
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn ai_latest_summary_round_id(messages: &[AiChatMessage]) -> Option<String> {
    messages.iter().rev().find_map(|message| {
        message
            .summary_ref
            .as_ref()
            .and_then(|summary_ref| summary_ref.get("roundId"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                message
                    .turn
                    .as_ref()
                    .and_then(|turn| turn.get("toolRounds"))
                    .and_then(serde_json::Value::as_array)
                    .and_then(|rounds| rounds.iter().rev().find_map(|round| {
                        round
                            .get("id")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                    }))
            })
    })
}

impl WorkspaceApp {
    fn set_ai_compaction_notice_running(&mut self, conversation_id: &str, cx: &mut Context<Self>) {
        self.ai_compaction_notice = Some(AiCompactionNotice {
            conversation_id: conversation_id.to_string(),
            phase: AiCompactionNoticePhase::Running,
            compacted_count: None,
            timestamp_ms: ai_now_ms(),
        });
        cx.notify();
    }

    fn set_ai_compaction_notice_done(
        &mut self,
        conversation_id: &str,
        compacted_count: usize,
        cx: &mut Context<Self>,
    ) {
        let timestamp_ms = ai_now_ms();
        self.ai_compaction_notice = Some(AiCompactionNotice {
            conversation_id: conversation_id.to_string(),
            phase: AiCompactionNoticePhase::Done,
            compacted_count: Some(compacted_count),
            timestamp_ms,
        });
        self.schedule_ai_compaction_notice_clear(conversation_id.to_string(), timestamp_ms, cx);
        cx.notify();
    }

    fn clear_ai_compaction_notice_for(&mut self, conversation_id: &str, cx: &mut Context<Self>) {
        if self
            .ai_compaction_notice
            .as_ref()
            .is_some_and(|notice| notice.conversation_id == conversation_id)
        {
            self.ai_compaction_notice = None;
            cx.notify();
        }
    }

    fn schedule_ai_compaction_notice_clear(
        &mut self,
        conversation_id: String,
        timestamp_ms: i64,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_secs(5)).await;
            let _ = weak.update(cx, |this, cx| {
                if this.ai_compaction_notice.as_ref().is_some_and(|notice| {
                    notice.conversation_id == conversation_id
                        && notice.phase == AiCompactionNoticePhase::Done
                        && notice.timestamp_ms == timestamp_ms
                }) {
                    this.ai_compaction_notice = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
