const MAX_PROMPT_TAIL_CHARS: usize = 4_096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivilegePromptMatch {
    Sudo {
        username: Option<String>,
        prompt_text: String,
    },
    Su {
        target_user: Option<String>,
        prompt_text: String,
    },
    Custom {
        credential_id: String,
        prompt_text: String,
    },
    GenericPassword {
        prompt_text: String,
    },
}

pub fn detect_privilege_prompt(text: &str) -> Option<PrivilegePromptMatch> {
    let tail = tail_chars(text, MAX_PROMPT_TAIL_CHARS);
    let line = tail
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .last()?;

    if looks_like_password_result(line) {
        return None;
    }

    if let Some(username) = parse_sudo_prompt(line) {
        return Some(PrivilegePromptMatch::Sudo {
            username,
            prompt_text: line.to_string(),
        });
    }

    if line.eq_ignore_ascii_case("su: password:") {
        return Some(PrivilegePromptMatch::Su {
            target_user: None,
            prompt_text: line.to_string(),
        });
    }

    if is_generic_password_prompt(line) {
        // Plain terminal output has no trustworthy command metadata. Treat a
        // bare password prompt as a sensitive-input opportunity, but leave
        // credential selection to explicit user clicks in the scoped UI.
        return Some(PrivilegePromptMatch::GenericPassword {
            prompt_text: line.to_string(),
        });
    }

    None
}

fn tail_chars(text: &str, max_chars: usize) -> &str {
    // Terminal buffers can be large; prompt detection only inspects the recent
    // tail, matching the Tauri helper's bounded scan.
    let start = text
        .char_indices()
        .rev()
        .nth(max_chars)
        .map(|(index, _)| index)
        .unwrap_or(0);
    &text[start..]
}

fn parse_sudo_prompt(line: &str) -> Option<Option<String>> {
    let lower = line.to_ascii_lowercase();
    if let Some(prompt) = lower
        .strip_prefix("[sudo] password for ")
        .or_else(|| lower.strip_prefix("password for "))
        && prompt.ends_with(':')
    {
        let original_prefix_len = line.len() - prompt.len();
        let username = line[original_prefix_len..line.len() - 1].trim();
        Some((!username.is_empty()).then(|| username.to_string()))
    } else {
        parse_localized_sudo_prompt(line)
    }
}

fn parse_localized_sudo_prompt(line: &str) -> Option<Option<String>> {
    let prompt = line.trim().strip_prefix("[sudo]")?.trim();
    // Ubuntu localizes sudo prompts, for example: `[sudo] deploy 的密码：`.
    // Keep this as a literal suffix parser instead of a broad password regex so
    // ordinary application password prompts do not look like sudo.
    let prompt = prompt
        .strip_suffix('：')
        .or_else(|| prompt.strip_suffix(':'))?
        .trim();
    let username = prompt.strip_suffix("的密码")?.trim();
    Some((!username.is_empty()).then(|| username.to_string()))
}

fn is_generic_password_prompt(line: &str) -> bool {
    line.eq_ignore_ascii_case("password:") || line == "密码:" || line == "密码："
}

fn looks_like_password_result(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let has_password = lower.contains("password") || line.contains('密') && line.contains('码');
    let has_result = [
        "accepted",
        "changed",
        "updated",
        "success",
        "failed",
        "incorrect",
        "denied",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    has_password && has_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_sudo_prompts_with_username() {
        assert_eq!(
            detect_privilege_prompt("sudo -k true\n[sudo] password for dominical:"),
            Some(PrivilegePromptMatch::Sudo {
                username: Some("dominical".to_string()),
                prompt_text: "[sudo] password for dominical:".to_string(),
            })
        );
    }

    #[test]
    fn detects_localized_sudo_prompts_with_username() {
        assert_eq!(
            detect_privilege_prompt("sudo yazi\n[sudo] deploy 的密码："),
            Some(PrivilegePromptMatch::Sudo {
                username: Some("deploy".to_string()),
                prompt_text: "[sudo] deploy 的密码：".to_string(),
            })
        );
    }

    #[test]
    fn detects_localized_sudo_prompt_after_retry() {
        assert_eq!(
            detect_privilege_prompt(
                "sudo yazi\n[sudo] lipsc 的密码:\n对不起，请重试。\n[sudo] lipsc 的密码:"
            ),
            Some(PrivilegePromptMatch::Sudo {
                username: Some("lipsc".to_string()),
                prompt_text: "[sudo] lipsc 的密码:".to_string(),
            })
        );
    }

    #[test]
    fn detects_su_prompts_with_explicit_prefix() {
        assert_eq!(
            detect_privilege_prompt("su - root\nsu: Password:"),
            Some(PrivilegePromptMatch::Su {
                target_user: None,
                prompt_text: "su: Password:".to_string(),
            })
        );
    }

    #[test]
    fn detects_generic_password_prompts_without_command_guessing() {
        assert_eq!(
            detect_privilege_prompt("❯ sudo yazi\nPassword:"),
            Some(PrivilegePromptMatch::GenericPassword {
                prompt_text: "Password:".to_string(),
            })
        );
        assert_eq!(
            detect_privilege_prompt("❯ sudo yazi\n密码："),
            Some(PrivilegePromptMatch::GenericPassword {
                prompt_text: "密码：".to_string(),
            })
        );
        assert_eq!(
            detect_privilege_prompt("su - root\nPassword:"),
            Some(PrivilegePromptMatch::GenericPassword {
                prompt_text: "Password:".to_string(),
            })
        );
        assert_eq!(
            detect_privilege_prompt("su - root\n密码："),
            Some(PrivilegePromptMatch::GenericPassword {
                prompt_text: "密码：".to_string(),
            })
        );
        assert_eq!(
            detect_privilege_prompt("mysql login\nPassword:"),
            Some(PrivilegePromptMatch::GenericPassword {
                prompt_text: "Password:".to_string(),
            })
        );
    }

    #[test]
    fn rejects_result_and_help_lines() {
        assert_eq!(detect_privilege_prompt("password changed"), None);
        assert_eq!(detect_privilege_prompt("error: password failed"), None);
        assert_eq!(detect_privilege_prompt("Usage: --password: value"), None);
    }
}
