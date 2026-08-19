// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

const TERMINAL_CWD_SCAN_LINES: usize = 16;

/// Infer a terminal working directory from recent visible terminal text.
///
/// This is a UX fallback for environment awareness only. Callers must still
/// resolve terminal ownership from app state rather than from prompt text.
pub fn infer_terminal_cwd_from_text(buffer: &str) -> Option<String> {
    buffer
        .lines()
        .rev()
        .take(TERMINAL_CWD_SCAN_LINES)
        .find_map(infer_terminal_cwd_from_line)
}

fn infer_terminal_cwd_from_line(line: &str) -> Option<String> {
    line.split_whitespace()
        .filter_map(clean_terminal_cwd_token)
        .find(|token| terminal_cwd_token_is_path(token))
}

fn clean_terminal_cwd_token(token: &str) -> Option<String> {
    let cleaned = token.trim_matches(|ch: char| {
        ch.is_control()
            || matches!(
                ch,
                '"' | '\'' | '`' | '[' | ']' | '(' | ')' | '{' | '}' | '<' | '>' | ',' | ';' | ':'
            )
    });
    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

fn terminal_cwd_token_is_path(token: &str) -> bool {
    token == "~"
        || token.starts_with("~/")
        || (token.starts_with('/') && token.len() > 1 && !token.starts_with("//"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_terminal_cwd_from_prompt_and_command_output() {
        // Prompt, command-output, rejection, and punctuation cases share one inference contract.
        let cases = [
            (
                "❯ cd oxideterm.cloud-sync-server\n󰊢    ~/oxideterm.cloud-sync-server   main !1\n",
                Some("~/oxideterm.cloud-sync-server"),
            ),
            (
                "❯ pwd\n/home/lipsc/oxideterm.cloud-sync-server\n❯",
                Some("/home/lipsc/oxideterm.cloud-sync-server"),
            ),
            (
                "Documentation: https://help.ubuntu.com\n❯ cd project\n",
                None,
            ),
            (
                "prompt: [~/work/project]: git status",
                Some("~/work/project"),
            ),
        ];

        for (text, expected) in cases {
            assert_eq!(infer_terminal_cwd_from_text(text).as_deref(), expected);
        }
    }
}
