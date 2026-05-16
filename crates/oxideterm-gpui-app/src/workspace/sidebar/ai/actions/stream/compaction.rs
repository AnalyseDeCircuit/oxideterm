fn ai_compaction_plan(messages: &[AiChatMessage], context_window: usize) -> Option<AiCompactionPlan> {
    if messages.len() < 4 {
        return None;
    }
    let total_tokens = messages
        .iter()
        .map(ai_message_estimated_tokens)
        .sum::<usize>();
    let keep_budget = ((context_window as f32) * 0.4) as usize;
    let manual_cap = ((total_tokens as f32) * 0.6) as usize;
    let budget = keep_budget.min(manual_cap).max(1);
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
        keep_start = messages.len().saturating_sub(2);
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
    let mut previous_summaries = Vec::new();
    let mut transcript = Vec::new();
    for message in messages {
        if message
            .metadata
            .as_ref()
            .is_some_and(|metadata| metadata.kind == "compaction-anchor")
        {
            previous_summaries.push(message.content.trim().to_string());
        } else {
            let role = match message.role {
                AiChatRole::User => "User",
                AiChatRole::Assistant => "Assistant",
                AiChatRole::System => "System",
                AiChatRole::Tool => "Tool",
            };
            transcript.push(format!("{role}: {}", message.content.trim()));
        }
    }
    let mut content = String::from(
        "Summarize the following conversation in a concise paragraph. Capture the key topics, questions asked, solutions provided, and any important context. Write in the same language as the conversation. Keep it under 200 words. If there is a \"[Previous Summary]\" section, integrate it into your summary.",
    );
    if !previous_summaries.is_empty() {
        content.push_str("\n\n[Previous Summary]\n");
        content.push_str(&previous_summaries.join("\n\n"));
    }
    content.push_str("\n\n[Conversation]\n");
    content.push_str(&transcript.join("\n\n"));
    vec![AiChatMessage {
        id: "compact-request".to_string(),
        role: AiChatRole::User,
        content,
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
    }]
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
        },
    ]
}

