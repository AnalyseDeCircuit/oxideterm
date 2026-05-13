pub struct AiSlashCommand {
    pub name: &'static str,
    pub label_key: &'static str,
    pub description_key: &'static str,
    pub system_prompt_modifier: Option<&'static str>,
    pub client_only: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiParsedInput {
    pub slash_command: Option<String>,
    pub clean_text: String,
    pub raw_text: String,
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

pub fn resolve_ai_slash_command(name: &str) -> Option<&'static AiSlashCommand> {
    AI_SLASH_COMMANDS
        .iter()
        .find(|command| command.name == name)
}

pub fn parse_ai_user_input(raw: &str) -> AiParsedInput {
    let trimmed_start = raw.trim_start();
    let leading_whitespace = raw.len().saturating_sub(trimmed_start.len());
    let mut slash_command = None;
    let mut clean_text = raw.to_string();
    if leading_whitespace == 0 && trimmed_start.starts_with('/') {
        let command_len = trimmed_start[1..]
            .chars()
            .take_while(|ch| ch.is_ascii_lowercase() || *ch == '_')
            .map(char::len_utf8)
            .sum::<usize>();
        if command_len > 0 {
            let name = &trimmed_start[1..1 + command_len];
            slash_command = Some(name.to_string());
            clean_text = trimmed_start[1 + command_len..].trim_start().to_string();
        }
    }
    AiParsedInput {
        slash_command,
        clean_text: clean_text.trim().to_string(),
        raw_text: raw.to_string(),
    }
}

pub fn slash_task_system_prompt(command: &AiSlashCommand) -> Option<String> {
    command
        .system_prompt_modifier
        .map(|modifier| format!("## Task Mode: /{}\n{}", command.name, modifier))
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
    lines.join("\n")
}
