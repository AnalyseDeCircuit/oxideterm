use std::collections::HashSet;

use regex::Regex;

pub struct AiSlashCommand {
    pub name: &'static str,
    pub label_key: &'static str,
    pub description_key: &'static str,
    pub system_prompt_modifier: Option<&'static str>,
    pub client_only: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiParticipantDef {
    pub name: &'static str,
    pub label_key: &'static str,
    pub description_key: &'static str,
    pub system_prompt_modifier: &'static str,
    pub intent_hint: Option<&'static str>,
    pub preferred_target_view: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiReferenceDef {
    pub reference_type: &'static str,
    pub label_key: &'static str,
    pub description_key: &'static str,
    pub accepts_value: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiParticipantMatch {
    pub name: String,
    pub raw: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiReferenceMatch {
    pub reference_type: String,
    pub value: Option<String>,
    pub raw: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AiInputTokenType {
    Slash,
    Participant,
    Reference,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiInputTokenAtCursor {
    pub token_type: Option<AiInputTokenType>,
    pub partial: String,
    pub start: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiAutocompleteKind {
    Slash,
    Participant,
    Reference,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiAutocompleteCandidate {
    pub kind: AiAutocompleteKind,
    pub name: &'static str,
    pub description_key: &'static str,
    pub accepts_value: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiParsedInput {
    pub slash_command: Option<String>,
    pub participants: Vec<AiParticipantMatch>,
    pub references: Vec<AiReferenceMatch>,
    pub clean_text: String,
    pub raw_text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiDetectedIntent {
    pub kind: &'static str,
    pub confidence_percent: u8,
    pub system_hint: &'static str,
}

struct AiIntentPattern {
    kind: &'static str,
    confidence_percent: u8,
    system_hint: &'static str,
    predicates: &'static [fn(&str) -> bool],
}

pub const AI_SLASH_COMMANDS: &[AiSlashCommand] = &[
    AiSlashCommand {
        name: "explain",
        label_key: "ai.slash.explain",
        description_key: "ai.slash.explain_desc",
        system_prompt_modifier: Some(
            "The user wants an explanation. Be thorough and educational. Explain step-by-step what the command or output does, including any flags, options, or output fields. Provide examples where helpful.",
        ),
        client_only: false,
    },
    AiSlashCommand {
        name: "fix",
        label_key: "ai.slash.fix",
        description_key: "ai.slash.fix_desc",
        system_prompt_modifier: Some(
            "The user needs help fixing an error or problem. Diagnose the root cause step by step. Check the most common causes first. Use tools to gather diagnostic data when possible. Provide the exact fix with explanation.",
        ),
        client_only: false,
    },
    AiSlashCommand {
        name: "help",
        label_key: "ai.slash.help",
        description_key: "ai.slash.help_desc",
        system_prompt_modifier: None,
        client_only: true,
    },
    AiSlashCommand {
        name: "clear",
        label_key: "ai.slash.clear",
        description_key: "ai.slash.clear_desc",
        system_prompt_modifier: None,
        client_only: true,
    },
    AiSlashCommand {
        name: "compact",
        label_key: "ai.slash.compact",
        description_key: "ai.slash.compact_desc",
        system_prompt_modifier: None,
        client_only: true,
    },
];

pub const AI_PARTICIPANTS: &[AiParticipantDef] = &[
    AiParticipantDef {
        name: "terminal",
        label_key: "ai.participant.terminal",
        description_key: "ai.participant.terminal_desc",
        system_prompt_modifier: "The user explicitly selected the terminal domain. Prefer terminal/session targets and use terminal-oriented actions when tool use is needed.",
        intent_hint: Some("terminal"),
        preferred_target_view: Some("live_sessions"),
    },
    AiParticipantDef {
        name: "sftp",
        label_key: "ai.participant.sftp",
        description_key: "ai.participant.sftp_desc",
        system_prompt_modifier: "The user explicitly selected the SFTP domain. Prefer SFTP or file-capable remote targets and use file transfer/resource actions when tool use is needed.",
        intent_hint: Some("sftp"),
        preferred_target_view: Some("files"),
    },
    AiParticipantDef {
        name: "ide",
        label_key: "ai.participant.ide",
        description_key: "ai.participant.ide_desc",
        system_prompt_modifier: "The user explicitly selected the IDE domain. Prefer IDE workspace/file targets and use resource actions for code reading or editing.",
        intent_hint: Some("file"),
        preferred_target_view: Some("files"),
    },
    AiParticipantDef {
        name: "local",
        label_key: "ai.participant.local",
        description_key: "ai.participant.local_desc",
        system_prompt_modifier: "The user explicitly selected the local domain. Prefer local-shell targets and avoid assuming a remote SSH target unless the user names one.",
        intent_hint: Some("local"),
        preferred_target_view: Some("app_surfaces"),
    },
    AiParticipantDef {
        name: "settings",
        label_key: "ai.participant.settings",
        description_key: "ai.participant.settings_desc",
        system_prompt_modifier: "The user explicitly selected the settings domain. Prefer settings targets and use settings read/write actions rather than depending on the current settings tab.",
        intent_hint: Some("settings"),
        preferred_target_view: Some("app_surfaces"),
    },
    AiParticipantDef {
        name: "knowledge",
        label_key: "ai.participant.knowledge",
        description_key: "ai.participant.knowledge_desc",
        system_prompt_modifier: "The user explicitly selected the knowledge base domain. Prefer the rag-index target and use read_resource with resource=\"rag\" for documentation, runbook, SOP, or knowledge queries.",
        intent_hint: Some("knowledge"),
        preferred_target_view: Some("files"),
    },
];

pub const AI_REFERENCES: &[AiReferenceDef] = &[
    AiReferenceDef {
        reference_type: "buffer",
        label_key: "ai.reference.buffer",
        description_key: "ai.reference.buffer_desc",
        accepts_value: false,
    },
    AiReferenceDef {
        reference_type: "selection",
        label_key: "ai.reference.selection",
        description_key: "ai.reference.selection_desc",
        accepts_value: false,
    },
    AiReferenceDef {
        reference_type: "error",
        label_key: "ai.reference.error",
        description_key: "ai.reference.error_desc",
        accepts_value: false,
    },
    AiReferenceDef {
        reference_type: "pane",
        label_key: "ai.reference.pane",
        description_key: "ai.reference.pane_desc",
        accepts_value: true,
    },
    AiReferenceDef {
        reference_type: "cwd",
        label_key: "ai.reference.cwd",
        description_key: "ai.reference.cwd_desc",
        accepts_value: false,
    },
];

pub fn resolve_ai_slash_command(name: &str) -> Option<&'static AiSlashCommand> {
    AI_SLASH_COMMANDS
        .iter()
        .find(|command| command.name == name)
}

pub fn resolve_ai_participant(name: &str) -> Option<&'static AiParticipantDef> {
    AI_PARTICIPANTS
        .iter()
        .find(|participant| participant.name == name)
}

pub fn resolve_ai_reference(reference_type: &str) -> Option<&'static AiReferenceDef> {
    AI_REFERENCES
        .iter()
        .find(|reference| reference.reference_type == reference_type)
}

pub fn parse_ai_user_input(raw: &str) -> AiParsedInput {
    let mut text = raw;
    let mut slash_command = None;
    if let Some(rest) = text.strip_prefix('/') {
        let command_len = rest
            .chars()
            .take_while(|ch| ch.is_ascii_lowercase() || *ch == '_')
            .map(char::len_utf8)
            .sum::<usize>();
        if command_len > 0 {
            let name = &rest[..command_len];
            slash_command = Some(name.to_string());
            text = &rest[command_len..];
            text = text.trim_start();
        }
    }
    let participants = parse_participants(text);
    let references = parse_references(text);
    let mut clean_text = text.to_string();
    for participant in &participants {
        clean_text = clean_text.replacen(&participant.raw, "", 1);
    }
    for reference in &references {
        clean_text = clean_text.replacen(&reference.raw, "", 1);
    }
    AiParsedInput {
        slash_command,
        participants,
        references,
        clean_text: collapse_ai_input_whitespace(&clean_text),
        raw_text: raw.to_string(),
    }
}

pub fn detect_ai_intent(parsed: &AiParsedInput) -> AiDetectedIntent {
    if let Some(command) = parsed.slash_command.as_deref()
        && let Some(kind) = match command {
            "explain" => Some("explain"),
            "fix" => Some("troubleshoot"),
            _ => None,
        }
        && let Some(pattern) = AI_INTENT_PATTERNS
            .iter()
            .find(|pattern| pattern.kind == kind)
    {
        return AiDetectedIntent {
            kind,
            confidence_percent: 95,
            system_hint: pattern.system_hint,
        };
    }

    let text = parsed.clean_text.trim().to_lowercase();
    if text.is_empty() {
        return AiDetectedIntent {
            kind: "general",
            confidence_percent: 50,
            system_hint: "",
        };
    }

    AI_INTENT_PATTERNS
        .iter()
        .filter(|pattern| pattern.predicates.iter().any(|predicate| predicate(&text)))
        .max_by_key(|pattern| pattern.confidence_percent)
        .map(|pattern| AiDetectedIntent {
            kind: pattern.kind,
            confidence_percent: pattern.confidence_percent,
            system_hint: pattern.system_hint,
        })
        .unwrap_or(AiDetectedIntent {
            kind: "general",
            confidence_percent: 50,
            system_hint: "",
        })
}

pub fn ai_detected_intent_system_prompt(intent: &AiDetectedIntent) -> Option<String> {
    (intent.confidence_percent >= 80 && !intent.system_hint.is_empty())
        .then(|| format!("## Detected Intent\n{}", intent.system_hint))
}

const AI_INTENT_PATTERNS: &[AiIntentPattern] = &[
    AiIntentPattern {
        kind: "execute",
        confidence_percent: 85,
        system_hint: "The user wants to execute an action. Focus on providing actionable commands and confirming before executing anything destructive.",
        predicates: &[
            starts_with_execute_verb,
            contains_execute_phrase,
            contains_connection_phrase,
            starts_with_command_runner,
        ],
    },
    AiIntentPattern {
        kind: "explain",
        confidence_percent: 80,
        system_hint: "The user wants an explanation. Provide clear, educational answers with examples where helpful.",
        predicates: &[
            starts_with_explain_phrase,
            contains_explain_phrase,
            ends_with_question_mark,
        ],
    },
    AiIntentPattern {
        kind: "troubleshoot",
        confidence_percent: 90,
        system_hint: "The user is troubleshooting a problem. Analyze error messages carefully, suggest diagnostic commands, and provide step-by-step fixes.",
        predicates: &[
            starts_with_troubleshoot_phrase,
            contains_error_phrase,
            contains_problem_phrase,
            contains_common_error_phrase,
        ],
    },
    AiIntentPattern {
        kind: "create",
        confidence_percent: 85,
        system_hint: "The user wants to create or generate something. Provide complete, production-ready code or configurations.",
        predicates: &[
            starts_with_create_verb,
            starts_with_add_verb,
            contains_create_artifact,
            contains_create_phrase,
        ],
    },
    AiIntentPattern {
        kind: "explore",
        confidence_percent: 75,
        system_hint: "The user wants to discover or inspect information. Use appropriate tools to gather and present the requested data.",
        predicates: &[
            starts_with_explore_verb,
            starts_with_inspection_command,
            contains_inspection_phrase,
        ],
    },
    AiIntentPattern {
        kind: "configure",
        confidence_percent: 80,
        system_hint: "The user wants to modify settings or configuration. Identify the specific setting, explain the change, and confirm before applying.",
        predicates: &[
            starts_with_configure_verb,
            contains_settings_phrase,
            contains_toggle_phrase,
        ],
    },
];

fn starts_with_any(text: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| {
        text == *prefix
            || text
                .strip_prefix(*prefix)
                .is_some_and(|rest| rest.starts_with(char::is_whitespace))
    })
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn starts_with_execute_verb(text: &str) -> bool {
    starts_with_any(
        text,
        &[
            "run",
            "execute",
            "start",
            "stop",
            "restart",
            "kill",
            "deploy",
            "install",
            "uninstall",
        ],
    )
}

fn contains_execute_phrase(text: &str) -> bool {
    contains_any(text, &["run this", "execute this", "do this", "make it"])
}

fn contains_connection_phrase(text: &str) -> bool {
    contains_any(text, &["ssh into", "connect to", "login", "log in"])
}

fn starts_with_command_runner(text: &str) -> bool {
    starts_with_any(
        text,
        &[
            "sudo",
            "apt",
            "yum",
            "brew",
            "pip",
            "npm",
            "pnpm",
            "cargo",
            "docker",
            "kubectl",
            "systemctl",
        ],
    )
}

fn starts_with_explain_phrase(text: &str) -> bool {
    starts_with_any(
        text,
        &[
            "explain",
            "what is",
            "what are",
            "what does",
            "how does",
            "why does",
            "why is",
            "tell me about",
            "describe",
            "walk me through",
        ],
    )
}

fn contains_explain_phrase(text: &str) -> bool {
    contains_any(text, &["mean", "meaning", "purpose", "difference between"])
}

fn ends_with_question_mark(text: &str) -> bool {
    text.trim_end().ends_with('?') || text.trim_end().ends_with('？')
}

fn starts_with_troubleshoot_phrase(text: &str) -> bool {
    starts_with_any(text, &["fix", "debug", "troubleshoot", "diagnose"])
        || (text.starts_with("why") && contains_any(text, &["fail", "error", "crash", "broken"]))
}

fn contains_error_phrase(text: &str) -> bool {
    contains_any(
        text,
        &[
            "error",
            "fail",
            "failed",
            "failing",
            "failure",
            "crash",
            "crashed",
            "crashing",
            "broken",
            "not working",
            "can't",
            "cant",
            "unable",
        ],
    )
}

fn contains_problem_phrase(text: &str) -> bool {
    contains_any(
        text,
        &["issue", "problem", "bug", "wrong", "weird", "strange"],
    )
}

fn contains_common_error_phrase(text: &str) -> bool {
    contains_any(
        text,
        &[
            "permission denied",
            "connection refused",
            "timeout",
            "not found",
            "no such",
        ],
    )
}

fn starts_with_create_verb(text: &str) -> bool {
    starts_with_any(
        text,
        &[
            "create", "write", "generate", "make", "build", "set up", "setup", "init",
        ],
    )
}

fn starts_with_add_verb(text: &str) -> bool {
    starts_with_any(text, &["add", "new", "draft", "compose"])
}

fn contains_create_artifact(text: &str) -> bool {
    contains_any(
        text,
        &[
            "script",
            "config",
            "file",
            "template",
            "dockerfile",
            "makefile",
            "pipeline",
        ],
    )
}

fn contains_create_phrase(text: &str) -> bool {
    contains_any(text, &["write me", "generate a", "create a", "make a"])
}

fn starts_with_explore_verb(text: &str) -> bool {
    starts_with_any(
        text,
        &[
            "find", "search", "list", "show", "display", "get", "check", "look", "where",
        ],
    )
}

fn starts_with_inspection_command(text: &str) -> bool {
    starts_with_any(
        text,
        &[
            "ls", "cat", "grep", "find", "locate", "which", "type", "file",
        ],
    )
}

fn contains_inspection_phrase(text: &str) -> bool {
    contains_any(
        text,
        &["how many", "count", "size", "status", "info", "version"],
    )
}

fn starts_with_configure_verb(text: &str) -> bool {
    starts_with_any(
        text,
        &[
            "configure",
            "config",
            "set",
            "change",
            "modify",
            "update",
            "edit",
            "adjust",
            "tune",
        ],
    )
}

fn contains_settings_phrase(text: &str) -> bool {
    contains_any(
        text,
        &[
            "setting",
            "settings",
            "config",
            "configuration",
            "preference",
            "option",
            "parameter",
        ],
    )
}

fn contains_toggle_phrase(text: &str) -> bool {
    contains_any(
        text,
        &[
            "enable", "disable", "toggle", "switch", "turn on", "turn off",
        ],
    )
}

fn parse_participants(text: &str) -> Vec<AiParticipantMatch> {
    let mut matches = Vec::new();
    let mut seen = HashSet::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'@' {
            index += 1;
            continue;
        }
        let name_start = index + 1;
        let mut name_end = name_start;
        while name_end < bytes.len()
            && (bytes[name_end].is_ascii_lowercase() || bytes[name_end] == b'_')
        {
            name_end += 1;
        }
        if name_end > name_start {
            let name = &text[name_start..name_end];
            if resolve_ai_participant(name).is_some() && seen.insert(name.to_string()) {
                matches.push(AiParticipantMatch {
                    name: name.to_string(),
                    raw: text[index..name_end].to_string(),
                });
            }
        }
        index = name_end.max(index + 1);
    }
    matches
}

fn parse_references(text: &str) -> Vec<AiReferenceMatch> {
    let mut matches = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'#' {
            index += 1;
            continue;
        }
        let type_start = index + 1;
        let mut type_end = type_start;
        while type_end < bytes.len()
            && (bytes[type_end].is_ascii_lowercase() || bytes[type_end] == b'_')
        {
            type_end += 1;
        }
        if type_end > type_start {
            let reference_type = &text[type_start..type_end];
            if resolve_ai_reference(reference_type).is_some() {
                let mut raw_end = type_end;
                let mut value = None;
                if bytes.get(type_end) == Some(&b':')
                    && bytes
                        .get(type_end + 1)
                        .is_some_and(|byte| !byte.is_ascii_whitespace())
                {
                    raw_end = type_end + 1;
                    while raw_end < bytes.len() && !bytes[raw_end].is_ascii_whitespace() {
                        raw_end += 1;
                    }
                    value = Some(text[type_end + 1..raw_end].to_string());
                }
                matches.push(AiReferenceMatch {
                    reference_type: reference_type.to_string(),
                    value,
                    raw: text[index..raw_end].to_string(),
                });
                index = raw_end;
                continue;
            }
        }
        index = type_end.max(index + 1);
    }
    matches
}

fn collapse_ai_input_whitespace(text: &str) -> String {
    // Match Tauri's `/\s{2,}/g` cleanup: preserve single whitespace
    // characters, but collapse token-removal gaps and trim edges.
    Regex::new(r"\s{2,}")
        .map(|regex| regex.replace_all(text, " ").trim().to_string())
        .unwrap_or_else(|_| text.trim().to_string())
}

pub fn ai_input_token_at_cursor(text: &str, cursor_pos: usize) -> AiInputTokenAtCursor {
    if cursor_pos == 0 || cursor_pos > text.len() {
        return AiInputTokenAtCursor {
            token_type: None,
            partial: String::new(),
            start: cursor_pos.min(text.len()),
        };
    }
    let prefix = &text[..cursor_pos];
    let token_start = prefix
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(0);
    let token = &text[token_start..cursor_pos];
    if token.starts_with('/') && token_start == 0 {
        return AiInputTokenAtCursor {
            token_type: Some(AiInputTokenType::Slash),
            partial: token[1..].to_string(),
            start: token_start,
        };
    }
    if let Some(partial) = token.strip_prefix('@') {
        return AiInputTokenAtCursor {
            token_type: Some(AiInputTokenType::Participant),
            partial: partial.to_string(),
            start: token_start,
        };
    }
    if let Some(partial) = token.strip_prefix('#') {
        return AiInputTokenAtCursor {
            token_type: Some(AiInputTokenType::Reference),
            partial: partial.to_string(),
            start: token_start,
        };
    }
    AiInputTokenAtCursor {
        token_type: None,
        partial: String::new(),
        start: token_start,
    }
}

pub fn ai_autocomplete_candidates(text: &str, cursor_pos: usize) -> Vec<AiAutocompleteCandidate> {
    let token = ai_input_token_at_cursor(text, cursor_pos);
    let Some(token_type) = token.token_type else {
        return Vec::new();
    };
    let partial = token.partial.to_lowercase();
    match token_type {
        AiInputTokenType::Slash => AI_SLASH_COMMANDS
            .iter()
            .filter(|command| command.name.starts_with(&partial))
            .map(|command| AiAutocompleteCandidate {
                kind: AiAutocompleteKind::Slash,
                name: command.name,
                description_key: command.description_key,
                accepts_value: false,
            })
            .collect(),
        AiInputTokenType::Participant => AI_PARTICIPANTS
            .iter()
            .filter(|participant| participant.name.starts_with(&partial))
            .map(|participant| AiAutocompleteCandidate {
                kind: AiAutocompleteKind::Participant,
                name: participant.name,
                description_key: participant.description_key,
                accepts_value: false,
            })
            .collect(),
        AiInputTokenType::Reference => AI_REFERENCES
            .iter()
            .filter(|reference| reference.reference_type.starts_with(&partial))
            .map(|reference| AiAutocompleteCandidate {
                kind: AiAutocompleteKind::Reference,
                name: reference.reference_type,
                description_key: reference.description_key,
                accepts_value: reference.accepts_value,
            })
            .collect(),
    }
}

pub fn apply_ai_autocomplete_candidate(
    text: &str,
    cursor_pos: usize,
    candidate: &AiAutocompleteCandidate,
) -> String {
    let token = ai_input_token_at_cursor(text, cursor_pos);
    if token.token_type.is_none() {
        return text.to_string();
    }
    let replacement = match candidate.kind {
        AiAutocompleteKind::Slash => format!("/{} ", candidate.name),
        AiAutocompleteKind::Participant => format!("@{} ", candidate.name),
        AiAutocompleteKind::Reference if candidate.accepts_value => format!("#{}:", candidate.name),
        AiAutocompleteKind::Reference => format!("#{} ", candidate.name),
    };
    let start = token.start.min(text.len());
    let cursor_pos = cursor_pos.min(text.len());
    format!("{}{}{}", &text[..start], replacement, &text[cursor_pos..])
}

pub fn slash_task_system_prompt(command: &AiSlashCommand) -> Option<String> {
    command
        .system_prompt_modifier
        .map(|modifier| format!("## Task Mode: /{}\n{}", command.name, modifier))
}

pub fn ai_input_system_prompt(
    slash_command: Option<&AiSlashCommand>,
    participants: &[AiParticipantMatch],
) -> Option<String> {
    let mut sections = Vec::new();
    if let Some(prompt) = slash_command.and_then(slash_task_system_prompt) {
        sections.push(prompt);
    }
    let participant_lines = participants
        .iter()
        .filter_map(|participant| resolve_ai_participant(&participant.name))
        .map(|participant| {
            let routing_hint = [
                participant
                    .intent_hint
                    .map(|intent| format!("intent={intent}")),
                participant
                    .preferred_target_view
                    .map(|view| format!("preferred_target_view={view}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ");
            if routing_hint.is_empty() {
                format!(
                    "@{}: {}",
                    participant.name, participant.system_prompt_modifier
                )
            } else {
                format!(
                    "@{}: {} ({})",
                    participant.name, participant.system_prompt_modifier, routing_hint
                )
            }
        })
        .collect::<Vec<_>>();
    if !participant_lines.is_empty() {
        sections.push(format!(
            "## Active Participants\n{}",
            participant_lines.join("\n")
        ));
    }
    (!sections.is_empty()).then(|| sections.join("\n\n"))
}

pub fn ai_help_markdown(description_for_key: impl Fn(&str) -> String) -> String {
    let mut lines = vec![
        "### /help".to_string(),
        String::new(),
        "**Slash Commands**".to_string(),
    ];
    for command in AI_SLASH_COMMANDS {
        lines.push(format!(
            "- `/{}` - {}",
            command.name,
            description_for_key(command.description_key)
        ));
    }
    lines.push(String::new());
    lines.push("**@ Participants**".to_string());
    for participant in AI_PARTICIPANTS {
        lines.push(format!(
            "- `@{}` - {}",
            participant.name,
            description_for_key(participant.description_key)
        ));
    }
    lines.push(String::new());
    lines.push("**# References**".to_string());
    for reference in AI_REFERENCES {
        lines.push(format!(
            "- `#{}` - {}",
            reference.reference_type,
            description_for_key(reference.description_key)
        ));
    }
    lines.join("\n")
}
