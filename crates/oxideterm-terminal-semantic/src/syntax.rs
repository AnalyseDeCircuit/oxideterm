// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_editor_syntax::{LanguageId, SyntaxScope, SyntaxSession};

use crate::{SemanticClass, SemanticLineRole, SemanticShellDialect, scheme::Candidate};

pub(crate) fn shell_syntax_candidates(
    text: &str,
    role: SemanticLineRole,
    dialect: SemanticShellDialect,
) -> Vec<Candidate> {
    // Shell grammar scopes apply only to submitted or active command lines;
    // terminal output keeps using deterministic semantic rules.
    if role != SemanticLineRole::Command {
        return Vec::new();
    }
    let offset = command_source_offset(text);
    let source = &text[offset..];
    if source.trim().is_empty() {
        return Vec::new();
    }
    let language = language_for(dialect, source);
    let Ok(session) = SyntaxSession::parse(language, source) else {
        return Vec::new();
    };

    session
        .highlight_spans(source)
        .into_iter()
        .filter_map(|span| {
            let class = semantic_class_for_scope(span.scope)?;
            let start = offset + span.range.start.as_usize();
            let end = offset + span.range.end.as_usize();
            (start < end && end <= text.len())
                .then(|| Candidate::new(start..end, class, syntax_priority(class)))
        })
        .collect()
}

fn language_for(dialect: SemanticShellDialect, source: &str) -> LanguageId {
    match dialect {
        SemanticShellDialect::Bash => LanguageId::Bash,
        SemanticShellDialect::Zsh => LanguageId::Zsh,
        SemanticShellDialect::Fish => LanguageId::Fish,
        SemanticShellDialect::PowerShell => LanguageId::Powershell,
        SemanticShellDialect::Auto if looks_like_powershell(source) => LanguageId::Powershell,
        SemanticShellDialect::Auto => LanguageId::Bash,
    }
}

fn looks_like_powershell(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    lower.contains("$env:")
        || lower.contains("$_")
        || lower.contains(" -eq ")
        || lower.contains(" -ne ")
        || lower.contains("write-host")
        || lower.split_whitespace().next().is_some_and(|command| {
            [
                "get-", "set-", "new-", "remove-", "invoke-", "start-", "stop-",
            ]
            .iter()
            .any(|prefix| command.starts_with(prefix))
        })
}

fn command_source_offset(text: &str) -> usize {
    const PROMPT_MARKERS: &[&str] = &["$ ", "# ", "% ", "> ", "❯ "];
    // Ignore marker-like text after quotes or shell operators so a command such
    // as `echo "price $ 5"` is never mistaken for a second prompt.
    PROMPT_MARKERS
        .iter()
        .filter_map(|marker| {
            let index = text.find(marker)?;
            let prefix = &text[..index];
            (index <= 256 && !prefix.contains(['\'', '"', ';', '|']))
                .then_some(index + marker.len())
        })
        .min()
        .unwrap_or(0)
}

fn semantic_class_for_scope(scope: SyntaxScope) -> Option<SemanticClass> {
    match scope {
        SyntaxScope::Comment => Some(SemanticClass::Comment),
        SyntaxScope::Function => Some(SemanticClass::Command),
        SyntaxScope::Keyword | SyntaxScope::Type => Some(SemanticClass::Keyword),
        SyntaxScope::Number => Some(SemanticClass::Number),
        SyntaxScope::Operator | SyntaxScope::Punctuation => Some(SemanticClass::Operator),
        SyntaxScope::String => Some(SemanticClass::String),
        SyntaxScope::Attribute
        | SyntaxScope::Constant
        | SyntaxScope::Namespace
        | SyntaxScope::Property
        | SyntaxScope::Variable => Some(SemanticClass::Variable),
    }
}

fn syntax_priority(class: SemanticClass) -> u8 {
    match class {
        SemanticClass::Comment => 125,
        SemanticClass::String => 120,
        SemanticClass::Command => 115,
        SemanticClass::Keyword => 112,
        SemanticClass::Variable => 108,
        SemanticClass::Operator => 106,
        SemanticClass::Number => 105,
        _ => 100,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matched(text: &str, dialect: SemanticShellDialect) -> Vec<(&str, SemanticClass)> {
        shell_syntax_candidates(text, SemanticLineRole::Command, dialect)
            .into_iter()
            .map(|candidate| (&text[candidate.span.range], candidate.span.class))
            .collect()
    }

    #[test]
    fn bash_parser_colors_compound_shell_syntax_after_a_prompt() {
        let text = "user@host:~$ if [ -f \"$HOME/app.log\" ]; then echo ok; fi";
        let matches = matched(text, SemanticShellDialect::Bash);

        assert!(matches.contains(&("if", SemanticClass::Keyword)));
        assert!(matches.contains(&("\"$HOME/app.log\"", SemanticClass::String)));
        assert!(matches.contains(&("echo", SemanticClass::Command)));
        assert!(matches.contains(&(";", SemanticClass::Operator)));
    }

    #[test]
    fn bash_parser_colors_pipeline_and_redirection_operators() {
        let text = "ps aux | grep node && echo done > result.log";
        let matches = matched(text, SemanticShellDialect::Bash);

        for operator in ["|", "&&", ">"] {
            assert!(
                matches.contains(&(operator, SemanticClass::Operator)),
                "missing operator {operator:?} in {matches:?}"
            );
        }
    }

    #[test]
    fn powershell_parser_colors_cmdlets_variables_and_strings() {
        let text = "PS C:\\> Get-ChildItem $env:TEMP | Where-Object { $_.Name -eq 'logs' }";
        let matches = matched(text, SemanticShellDialect::PowerShell);

        assert!(matches.iter().any(|(value, class)| {
            *value == "Get-ChildItem" && *class == SemanticClass::Command
        }));
        assert!(matches.iter().any(|(value, class)| {
            value.contains("$env:TEMP") && *class == SemanticClass::Variable
        }));
        assert!(matches.contains(&("'logs'", SemanticClass::String)));
    }

    #[test]
    fn prompt_detection_does_not_cut_a_marker_inside_a_command_string() {
        let text = "echo \"price $ 5\"";
        let matches = matched(text, SemanticShellDialect::Bash);

        assert!(matches.contains(&("echo", SemanticClass::Command)));
        assert!(matches.contains(&("\"price $ 5\"", SemanticClass::String)));
    }
}
