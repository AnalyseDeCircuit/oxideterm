fn ai_message_estimated_tokens(message: &AiChatMessage) -> usize {
    // Tauri's chat token budget only counts message.content here; tool-call
    // details are accounted separately in the context indicator.
    ai_estimated_tokens(&message.content)
}

fn ai_tool_definitions_estimated_tokens(tools: &[oxideterm_ai::AiToolDefinition]) -> usize {
    tools
        .iter()
        .map(|tool| {
            10 + ai_estimated_tokens(&tool.name)
                + ai_estimated_tokens(&tool.description)
                + ai_estimated_tokens(&tool.parameters.to_string())
        })
        .sum()
}

fn ai_summary_eligible_tokens(messages: &[&AiChatMessage]) -> usize {
    if messages.len() < 4 {
        return 0;
    }
    messages
        .iter()
        .take(messages.len().saturating_sub(3))
        .map(|message| ai_message_estimated_tokens(message))
        .sum()
}


fn normalize_ai_stream_history_for_provider(history: &mut Vec<AiChatMessage>) {
    let mut normalized = Vec::with_capacity(history.len());
    for mut message in history.drain(..) {
        match message.role {
            AiChatRole::System if is_ai_compaction_anchor(&message) => {
                if message.content.trim().is_empty() {
                    continue;
                }
                message.content = format!("Previous conversation summary:\n{}", message.content);
                message.metadata = None;
                message.tool_calls.clear();
                message.tool_call_id = None;
                message.thinking_content = None;
                normalized.push(message);
            }
            AiChatRole::System if is_runtime_ai_history_system(&message) => {
                if !message.content.trim().is_empty() {
                    normalized.push(message);
                }
            }
            AiChatRole::System => {}
            AiChatRole::User => {
                if !message.content.trim().is_empty() {
                    normalized.push(message);
                }
            }
            AiChatRole::Assistant => {
                if message.content.trim().is_empty() {
                    continue;
                }
                // Tauri replays prior turns as plain assistant text. Tool protocol
                // messages are only emitted inside the live tool loop, where every
                // assistant tool_call is immediately followed by its matching tool result.
                message.tool_calls.clear();
                message.tool_call_id = None;
                message.thinking_content = None;
                normalized.push(message);
            }
            AiChatRole::Tool => {}
        }
    }
    *history = normalized;
}

fn is_ai_compaction_anchor(message: &AiChatMessage) -> bool {
    message
        .metadata
        .as_ref()
        .is_some_and(|metadata| metadata.kind == "compaction-anchor")
}

fn is_runtime_ai_history_system(message: &AiChatMessage) -> bool {
    matches!(
        message.id.as_str(),
        "task-mode" | "current-terminal-context"
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AiStoppedAssistantTurn {
    message_id: String,
    status: &'static str,
    retained: bool,
}

fn finalize_streaming_ai_messages_on_cancel(
    conversation: &mut AiConversation,
) -> Vec<AiStoppedAssistantTurn> {
    let mut stopped_turns = Vec::new();
    let mut remove_ids = Vec::new();
    for message in &mut conversation.messages {
        if message.role != AiChatRole::Assistant || !message.is_streaming {
            continue;
        }
        let pending_calls = message
            .tool_calls
            .iter()
            .filter_map(cancel_rejected_tool_call)
            .collect::<Vec<_>>();
        for (id, name, arguments) in pending_calls {
            let result = serde_json::json!({
                "ok": false,
                "summary": "Generation was stopped.",
                "output": "Generation was stopped.",
                "error": {
                    "code": "generation_stopped",
                    "message": "Generation was stopped.",
                    "recoverable": true,
                },
                "meta": {
                    "toolName": name,
                    "durationMs": 0,
                    "verified": false,
                    "truncated": false,
                }
            });
            update_ai_tool_call_status(
                message,
                &id,
                &name,
                &arguments,
                "rejected",
                Some(result),
                None,
                Some("Generation was stopped.".to_string()),
                None,
                None,
            );
        }
        message.is_streaming = false;
        if should_retain_stopped_ai_message(message) {
            set_ai_turn_status(message, "complete");
            stopped_turns.push(AiStoppedAssistantTurn {
                message_id: message.id.clone(),
                status: "complete",
                retained: true,
            });
        } else {
            stopped_turns.push(AiStoppedAssistantTurn {
                message_id: message.id.clone(),
                status: "error",
                retained: false,
            });
            remove_ids.push(message.id.clone());
        }
    }
    if !remove_ids.is_empty() {
        let remove_ids = remove_ids.into_iter().collect::<std::collections::HashSet<_>>();
        conversation
            .messages
            .retain(|message| !remove_ids.contains(&message.id));
        conversation.message_count = conversation.messages.len();
    }
    stopped_turns
}

fn should_retain_stopped_ai_message(message: &AiChatMessage) -> bool {
    if !message.content.trim().is_empty()
        || message
            .thinking_content
            .as_deref()
            .is_some_and(|content| !content.trim().is_empty())
    {
        return true;
    }
    if !message.tool_calls.is_empty() {
        return true;
    }
    message.turn.as_ref().is_some_and(|turn| {
        turn.get("parts")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|parts| !parts.is_empty())
            || turn
                .get("toolRounds")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|rounds| !rounds.is_empty())
    })
}

fn cancel_rejected_tool_call(call: &serde_json::Value) -> Option<(String, String, String)> {
    let status = call
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if matches!(status, "completed" | "error" | "rejected") {
        return None;
    }
    if call.get("result").is_some_and(|result| !result.is_null()) {
        return None;
    }
    let id = call.get("id").and_then(serde_json::Value::as_str)?;
    let name = call.get("name").and_then(serde_json::Value::as_str)?;
    let arguments = call
        .get("arguments")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    Some((id.to_string(), name.to_string(), arguments.to_string()))
}

fn ai_estimated_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let cjk_count = text
        .chars()
        .filter(|ch| {
            matches!(
                *ch as u32,
                0x4e00..=0x9fff | 0x3040..=0x309f | 0x30a0..=0x30ff | 0xac00..=0xd7af
            )
        })
        .count();
    let non_cjk_count = text.encode_utf16().count().saturating_sub(cjk_count);
    ((cjk_count as f64 * 1.5 + non_cjk_count as f64 * 0.25) * 1.15).ceil() as usize
}

fn ai_response_reserve(context_window: usize) -> usize {
    (((context_window as f64) * 0.15).floor() as usize).min(4096)
}

const AI_HISTORY_BUDGET_RATIO: f32 = 0.7;
const AI_COMPACTION_TRIGGER_THRESHOLD: f32 = 0.80;
const AI_TRANSCRIPT_LOOKUP_THRESHOLD: f32 = 0.92;
const AI_TOOL_LOOP_STOP_THRESHOLD: f32 = 0.98;
const AI_MIN_PROMPT_SAFETY_MARGIN: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AiPromptBudget {
    usable_prompt_budget: usize,
    history_budget: usize,
}

#[derive(Debug, Clone, Copy)]
struct AiPromptBudgetInput {
    context_window: usize,
    response_reserve: usize,
    system_budget: usize,
    history_tokens: usize,
    safety_margin: Option<usize>,
    trimmable_history_tokens: Option<usize>,
    summary_eligible_tokens: Option<usize>,
    can_summarize: bool,
    can_lookup_transcript: bool,
    in_tool_loop: bool,
    auto_compact_threshold: Option<f32>,
    transcript_lookup_threshold: Option<f32>,
    tool_loop_stop_threshold: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct AiPromptBudgetDecision {
    level: u8,
    usage_ratio: f32,
    overage: usize,
}

fn compute_ai_prompt_budget(
    context_window: usize,
    response_reserve: usize,
    system_budget: usize,
    safety_margin: Option<usize>,
) -> AiPromptBudget {
    let safety_margin = safety_margin
        .unwrap_or_else(|| AI_MIN_PROMPT_SAFETY_MARGIN.max((context_window as f32 * 0.02).floor() as usize));
    let usable_prompt_budget = context_window
        .saturating_sub(response_reserve)
        .saturating_sub(safety_margin);
    AiPromptBudget {
        usable_prompt_budget,
        history_budget: usable_prompt_budget.saturating_sub(system_budget),
    }
}

fn determine_ai_compression_level(input: AiPromptBudgetInput) -> AiPromptBudgetDecision {
    let prompt_budget = compute_ai_prompt_budget(
        input.context_window,
        input.response_reserve,
        input.system_budget,
        input.safety_margin,
    );
    let total_prompt_tokens = input.system_budget.saturating_add(input.history_tokens);
    let overage = total_prompt_tokens.saturating_sub(prompt_budget.usable_prompt_budget);
    let usage_ratio = if prompt_budget.usable_prompt_budget > 0 {
        total_prompt_tokens as f32 / prompt_budget.usable_prompt_budget as f32
    } else {
        f32::INFINITY
    };
    let trimmable_history_tokens = input.trimmable_history_tokens.unwrap_or(input.history_tokens);
    let summary_eligible_tokens = input.summary_eligible_tokens.unwrap_or(input.history_tokens);
    let auto_compact_threshold = input
        .auto_compact_threshold
        .unwrap_or(AI_COMPACTION_TRIGGER_THRESHOLD);
    let transcript_lookup_threshold = input
        .transcript_lookup_threshold
        .unwrap_or(AI_TRANSCRIPT_LOOKUP_THRESHOLD);
    let tool_loop_stop_threshold = input
        .tool_loop_stop_threshold
        .unwrap_or(AI_TOOL_LOOP_STOP_THRESHOLD);

    let level = if overage == 0 {
        if input.in_tool_loop && usage_ratio >= tool_loop_stop_threshold {
            4
        } else if input.can_lookup_transcript && usage_ratio >= transcript_lookup_threshold {
            3
        } else if input.can_summarize
            && summary_eligible_tokens > 0
            && usage_ratio >= auto_compact_threshold
        {
            2
        } else {
            0
        }
    } else if trimmable_history_tokens >= overage && trimmable_history_tokens > 0 {
        1
    } else if input.can_summarize
        && summary_eligible_tokens > 0
        && usage_ratio >= auto_compact_threshold
    {
        2
    } else if input.can_lookup_transcript && usage_ratio >= transcript_lookup_threshold {
        3
    } else if input.in_tool_loop && usage_ratio >= tool_loop_stop_threshold {
        4
    } else if input.can_lookup_transcript {
        3
    } else if input.can_summarize && summary_eligible_tokens > 0 {
        2
    } else if input.in_tool_loop {
        4
    } else {
        1
    };

    AiPromptBudgetDecision {
        level,
        usage_ratio,
        overage,
    }
}

fn trim_ai_stream_history_to_budget(
    history: &mut Vec<AiChatMessage>,
    context_window: usize,
    response_reserve: usize,
) -> usize {
    if history.is_empty() {
        return 0;
    }
    let system_tokens = history
        .iter()
        .filter(|message| message.role == AiChatRole::System)
        .map(ai_message_estimated_tokens)
        .sum::<usize>();
    let regular_indices = history
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            matches!(
                message.role,
                AiChatRole::User | AiChatRole::Assistant | AiChatRole::Tool
            )
            .then_some(index)
        })
        .collect::<Vec<_>>();
    let total_regular = regular_indices.len();
    if total_regular <= 1 {
        return 0;
    }
    let budget = ((context_window as f32) * AI_HISTORY_BUDGET_RATIO)
        .floor() as usize;
    let budget = budget
        .saturating_sub(response_reserve)
        .saturating_sub(system_tokens);
    if budget == 0 {
        // Tauri keeps the most recent history message even when fixed prompt
        // overhead leaves no budget for accumulated conversation history.
        let keep_index = regular_indices[total_regular - 1];
        *history = history
            .drain(..)
            .enumerate()
            .filter_map(|(index, message)| {
                (message.role == AiChatRole::System || index == keep_index).then_some(message)
            })
            .collect();
        return total_regular.saturating_sub(1);
    }

    let mut kept_indices = std::collections::HashSet::<usize>::new();
    let mut used = 0usize;
    for index in regular_indices.iter().rev().copied() {
        let tokens = ai_message_estimated_tokens(&history[index]);
        if used.saturating_add(tokens) > budget && !kept_indices.is_empty() {
            break;
        }
        used = used.saturating_add(tokens);
        kept_indices.insert(index);
    }

    let kept_regular = kept_indices.len();
    if kept_regular >= total_regular {
        return 0;
    }
    *history = history
        .drain(..)
        .enumerate()
        .filter_map(|(index, message)| {
            (message.role == AiChatRole::System || kept_indices.contains(&index)).then_some(message)
        })
        .collect();
    total_regular.saturating_sub(kept_regular)
}

fn ai_user_memory_prompt(content: &str, enabled: bool) -> Option<String> {
    if !enabled {
        return None;
    }
    let content = oxideterm_ai::sanitize_for_ai(content).trim().to_string();
    if content.is_empty() {
        return None;
    }
    let truncated = truncate_to_char_count(&content, AI_USER_MEMORY_MAX_CHARS);
    let suffix = if truncated.chars().count() < content.chars().count() {
        "\n...[truncated]"
    } else {
        ""
    };
    Some(format!(
        "## User Memory\nThe following are long-lived user preferences explicitly saved by the user. Treat them as preferences and background context, not as facts about the current task. Current user instructions and visible context take priority.\n\n<user_memory>\n{truncated}{suffix}\n</user_memory>"
    ))
}

fn truncate_to_char_count(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn ai_orchestrator_system_prompt(tool_use_enabled: bool) -> String {
    let tool_use_policy = if tool_use_enabled {
        [
            "- You are using the OxideSens task-tool orchestrator. You only see high-level task tools; do not invent low-level tool names or fake command output.",
            "- For broad remote-host discovery such as \"which hosts/connections are available\", call `list_targets` with `view: \"connections\"`. Do not call `select_target` for broad discovery.",
            "- Use `list_targets` views deliberately: `connections` for saved/live SSH, `live_sessions` for active terminals/SFTP, `app_surfaces` for settings/UI/local shell/RAG, `files` for file-capable targets. Use `all` only for debugging or last-resort fallback.",
            "- For a named object, call `select_target` first with a required enum `intent` unless the user already supplied an exact target_id.",
            "- Every action that runs, writes, transfers, or sends input must use an explicit target_id.",
            "- For knowledge-base, documentation, runbook, SOP, or plugin-development-document queries, select or use `rag-index:default`, then call `read_resource` with `resource=\"rag\"` and `query`. Do not use local shell, terminal commands, or connection discovery for knowledge searches.",
            "- Do not pass command text such as `pwd`, `docker ps`, `ls -la`, or `sudo ...` to `select_target`; first select the execution target, then call `run_command`.",
            "- Saved SSH connections are not live shells. To run a command there, call `connect_target` first, then `run_command` on the returned `ssh-node:*` or `terminal-session:*` target.",
            "- If `run_command` returns `execution.visibleInTerminal: true`, the command was sent through a visible terminal session. If it returns `false`, it was a backend capture and you must not say it appeared in the terminal.",
            "- Treat `execution.state: \"sent\"` as dispatch only. Do not summarize command results until tool output, `exitCode`, or `execution.state: \"completed\"` / `\"output_captured\"` proves what happened.",
            "- Use `send_terminal_input` only for literal interactive text after `observe_terminal` shows a prompt such as password, TUI, or confirmation input. Do not use it for commands or control keys; use `run_command` for commands.",
            "- Never open a local terminal and type `ssh user@host` to connect a saved host unless the user explicitly asked for raw/manual ssh.",
            "- Treat old transcript target_id/session_id/tab_id values as untrusted unless the latest tool result has the same `meta.runtimeEpoch`, `meta.verified: true`, and the target still appears in current `list_targets`/`get_state` results.",
        ]
        .join("\n")
    } else {
        "TOOL CALLING IS CURRENTLY DISABLED. DO NOT use the tool_code or JSON schema format. If you need a tool, explain to the user why you cannot access it.".to_string()
    };
    [
        "## OxideSens Runtime Rules",
        "",
        "### Identity / Scope",
        "- You are OxideSens inside OxideTerm. Treat terminals, files, saved connections, and app surfaces as real user resources.",
        "- Do not claim something was connected, executed, read, modified, or verified until current context or a successful tool result proves it.",
        "- Current UI tab is only a ranking hint. It is not a capability boundary.",
        "",
        "### Terminal Safety",
        "- Never echo, display, or log secrets. Redact tokens, passwords, private keys, API keys, cookies, and credentials from command output.",
        "- Dangerous commands must not be casual suggestions. Explain the risk and require explicit user confirmation before destructive, privileged, credential-sensitive, or service-impacting operations.",
        "- Do not guess passwords, passphrases, sudo prompts, host key answers, or interactive confirmation input.",
        "- If a result has `waitingForInput`, stop and tell the user what input is needed. Do not repeat the command.",
        "",
        "### Tool Use Rules",
        &tool_use_policy,
        "",
        "### Command Execution Rules",
        "- Commands that may use a pager must be made non-interactive: use forms such as `git --no-pager log`, `git --no-pager diff`, `GIT_PAGER=cat`, `journalctl --no-pager`, `systemctl --no-pager`, or pipe `man`/`less`-style output through bounded commands like `col -b | head`.",
        "- If a command or tool fails, read the error carefully and adapt the next step. Do not repeat the same failing call unchanged.",
        "- Prefer bounded, inspectable commands before broad writes or deletes.",
        "",
        "### Output Handling",
        "- If tool output is truncated, sampled, or incomplete, explicitly say what part you could see and that conclusions are limited by truncation.",
        "- Do not ask the user to manually create, copy, or paste files to report results when tools can read or write them. Use tool calls or answer directly.",
    ]
    .join("\n")
}

fn ai_context_window_from_maps(
    user_context_windows: &serde_json::Map<String, serde_json::Value>,
    model_context_windows: &serde_json::Map<String, serde_json::Value>,
    provider_id: &str,
    model: &str,
) -> Option<usize> {
    usize::try_from(oxideterm_ai::model_context_window(
        model,
        model_context_windows,
        Some(provider_id),
        user_context_windows,
    ))
    .ok()
    .filter(|tokens| *tokens > 0)
}

fn ai_tool_use_policy_from_settings(
    settings: &oxideterm_settings::AiToolUseSettings,
) -> AiToolUsePolicy {
    tool_policy_from_parts(
        settings.enabled,
        settings
            .auto_approve_tools
            .iter()
            .filter_map(|(key, value)| value.as_bool().map(|enabled| (key.clone(), enabled))),
        settings.disabled_tools.clone(),
        settings.max_rounds,
        settings.max_calls_per_round,
    )
}

fn ai_reasoning_effort_value(effort: oxideterm_settings::AiReasoningEffort) -> Option<String> {
    serde_json::to_value(effort)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .map(|value| match value.as_str() {
            "none" | "minimal" => "off".to_string(),
            "xhigh" => "max".to_string(),
            other => other.to_string(),
        })
}

fn ai_conversation_message_tokens(conversation: &AiConversation) -> usize {
    conversation
        .messages
        .iter()
        .filter(|message| {
            matches!(
                message.role,
                AiChatRole::User | AiChatRole::Assistant | AiChatRole::Tool
            )
        })
        .map(ai_message_estimated_tokens)
        .sum()
}

fn ai_context_percentage(tokens: usize, max_tokens: usize) -> f32 {
    if max_tokens == 0 {
        return 0.0;
    }
    ((tokens as f32 / max_tokens as f32) * 100.0).min(100.0)
}

const AI_CONTEXT_WARNING_PERCENT: f32 = 70.0;
const AI_CONTEXT_DANGER_PERCENT: f32 = 85.0;
const AI_COMPACTION_DEFAULT_CONTEXT_WINDOW: usize = oxideterm_ai::DEFAULT_CONTEXT_WINDOW as usize;
const AI_USER_MEMORY_MAX_CHARS: usize = 4_000;
const DEFAULT_AI_SYSTEM_PROMPT: &str = r#"You are OxideSens, a terminal-aware assistant inside OxideTerm.

## Identity / Scope
- Help with shell commands, scripts, terminal output, files, connections, and OxideTerm workflows.
- Be concise, direct, and honest about what you can verify.
- Do not claim that you connected, executed, changed, read, or verified anything unless the available context or a successful tool result proves it.

## Terminal Safety
- Treat terminal actions as real operations on the user's machine or remote hosts.
- Do not present dangerous commands as casual suggestions. For destructive, privileged, credential-sensitive, or service-impacting commands, explain the risk first and require explicit user confirmation.
- Never echo, display, or log secrets. If command output contains tokens, passwords, private keys, API keys, cookies, or credentials, redact them in your response.
- Do not guess passwords, passphrases, sudo prompts, host key answers, or interactive confirmation input.

## Output Handling
- If output is incomplete, sampled, or truncated, say that your conclusion is limited to the visible output.
- If a command or tool fails, read the error, explain the likely cause, and adapt the next step. Do not repeat the same failing command unchanged.
- When commands may invoke pagers, prefer non-pager forms such as `git --no-pager ...`, `GIT_PAGER=cat`, `journalctl --no-pager`, `man ... | col -b | head`, or command-specific no-pager flags.

## Response Style
- Prefer actionable answers over long theory.
- When tools or file access are available, do not ask the user to manually copy text into files just to complete a task; use the available mechanisms or answer directly.
- Format commands and paths clearly in markdown."#;
const AI_SUGGESTIONS_INSTRUCTION: &str = r#"

## Follow-Up Suggestions

At the END of your response, optionally include 2-4 follow-up suggestions the user might want to try next. Use this exact XML format:

<suggestions>
<s icon="IconName">Short actionable suggestion text</s>
</suggestions>

Rules:
- Only include suggestions when they add value (skip for simple greetings or one-off answers)
- Keep each suggestion under 60 characters
- Use Lucide icon names: Zap, Search, Bug, FileCode, Terminal, Settings, RefreshCw, Shield, BarChart, GitBranch, Download, Upload, Eye, Wrench, Play
- Suggestions must be contextually relevant to the conversation"#;
