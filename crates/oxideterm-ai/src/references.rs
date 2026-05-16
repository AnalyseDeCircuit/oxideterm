use crate::AiReferenceMatch;

const ERROR_CONTEXT_BEFORE_LINES: usize = 15;
const ERROR_CONTEXT_AFTER_LINES: usize = 5;

pub fn ai_reference_label(reference: &AiReferenceMatch) -> String {
    reference
        .value
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("#{}:{value}", reference.reference_type))
        .unwrap_or_else(|| format!("#{}", reference.reference_type))
}

pub fn ai_reference_context_block(reference: &AiReferenceMatch, content: &str) -> Option<String> {
    let content = content.trim();
    if content.is_empty() {
        return None;
    }
    Some(format!(
        "--- {} ---\n{}",
        ai_reference_label(reference),
        content
    ))
}

pub fn current_terminal_context_system_message(context: &str) -> String {
    format!("Current terminal context:\n```\n{}\n```", context.trim())
}

pub fn extract_ai_error_context(buffer: &str) -> Option<String> {
    let lines = buffer.lines().collect::<Vec<_>>();
    let error_index = lines.iter().rposition(|line| line_looks_like_error(line))?;
    let start = error_index.saturating_sub(ERROR_CONTEXT_BEFORE_LINES);
    let end = (error_index + ERROR_CONTEXT_AFTER_LINES + 1).min(lines.len());
    let context = lines[start..end].join("\n");
    (!context.trim().is_empty()).then_some(context)
}

pub fn infer_ai_cwd(buffer: &str) -> Option<String> {
    buffer.lines().rev().take(10).find_map(infer_cwd_from_line)
}

fn line_looks_like_error(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "error",
        "failed",
        "fatal",
        "exception",
        "panic",
        "denied",
        "not found",
        "no such",
        "cannot",
        "unable",
        "segfault",
        "traceback",
        "command not found",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn infer_cwd_from_line(line: &str) -> Option<String> {
    let after_colon = line.split_once(':')?.1;
    for token in after_colon.split_whitespace() {
        let cleaned =
            token.trim_matches(|ch: char| matches!(ch, '$' | '#' | '>' | ')' | ']' | '"' | '\''));
        if cleaned.starts_with('/') || cleaned.starts_with('~') {
            return Some(cleaned.to_string());
        }
    }
    None
}
