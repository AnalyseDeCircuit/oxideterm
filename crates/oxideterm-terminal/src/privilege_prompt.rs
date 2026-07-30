use zeroize::Zeroizing;

const MAX_PROMPT_LINE_BYTES: usize = 2_048;

/// A password prompt classified directly from decoded terminal output.
#[derive(Clone, Eq, PartialEq)]
pub enum TerminalPrivilegePrompt {
    Sudo {
        username: Option<String>,
        prompt_text: String,
    },
    Su {
        target_user: Option<String>,
        prompt_text: String,
    },
    GenericPassword {
        prompt_text: String,
    },
}

impl std::fmt::Debug for TerminalPrivilegePrompt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sudo { username, .. } => formatter
                .debug_struct("Sudo")
                .field("has_username", &username.is_some())
                .field("prompt_text", &"<redacted>")
                .finish(),
            Self::Su { target_user, .. } => formatter
                .debug_struct("Su")
                .field("has_target_user", &target_user.is_some())
                .field("prompt_text", &"<redacted>")
                .finish(),
            Self::GenericPassword { .. } => formatter
                .debug_struct("GenericPassword")
                .field("prompt_text", &"<redacted>")
                .finish(),
        }
    }
}

/// A semantic prompt transition emitted by a terminal session.
#[derive(Clone, Eq, PartialEq)]
pub enum TerminalPrivilegePromptEvent {
    Visible {
        prompt: TerminalPrivilegePrompt,
        retry: bool,
    },
    Dismissed,
}

impl std::fmt::Debug for TerminalPrivilegePromptEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Visible { prompt, retry } => formatter
                .debug_struct("Visible")
                .field("prompt", prompt)
                .field("retry", retry)
                .finish(),
            Self::Dismissed => formatter.write_str("Dismissed"),
        }
    }
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum ControlSequenceState {
    #[default]
    Ground,
    Escape,
    Csi,
    Osc,
    OscEscape,
    StringControl,
    StringControlEscape,
}

/// Incrementally recognizes password prompts without copying the full output
/// stream into the UI event queue.
pub(crate) struct TerminalPrivilegePromptStream {
    current_line: Zeroizing<String>,
    control_state: ControlSequenceState,
    pending_carriage_return: bool,
    retry_pending: bool,
    prompt_visible: bool,
    emitted_line: Option<String>,
}

impl Default for TerminalPrivilegePromptStream {
    fn default() -> Self {
        Self {
            current_line: Zeroizing::new(String::new()),
            control_state: ControlSequenceState::Ground,
            pending_carriage_return: false,
            retry_pending: false,
            prompt_visible: false,
            emitted_line: None,
        }
    }
}

impl TerminalPrivilegePromptStream {
    pub(crate) fn observe(&mut self, output: &[u8]) -> Vec<TerminalPrivilegePromptEvent> {
        let mut events = Vec::new();
        let decoded = String::from_utf8_lossy(output);
        for character in decoded.chars() {
            self.observe_character(character, &mut events);
        }
        events
    }

    fn observe_character(
        &mut self,
        character: char,
        events: &mut Vec<TerminalPrivilegePromptEvent>,
    ) {
        match self.control_state {
            ControlSequenceState::Escape => {
                self.control_state = match character {
                    '[' => ControlSequenceState::Csi,
                    ']' => ControlSequenceState::Osc,
                    'P' | '_' | '^' => ControlSequenceState::StringControl,
                    _ => ControlSequenceState::Ground,
                };
                return;
            }
            ControlSequenceState::Csi => {
                if ('@'..='~').contains(&character) {
                    self.control_state = ControlSequenceState::Ground;
                }
                return;
            }
            ControlSequenceState::Osc => {
                if matches!(character, '\x07' | '\u{9c}') {
                    self.control_state = ControlSequenceState::Ground;
                } else if character == '\x1b' {
                    self.control_state = ControlSequenceState::OscEscape;
                }
                return;
            }
            ControlSequenceState::OscEscape => {
                self.control_state = if character == '\\' {
                    ControlSequenceState::Ground
                } else {
                    ControlSequenceState::Osc
                };
                return;
            }
            ControlSequenceState::StringControl => {
                if character == '\u{9c}' {
                    self.control_state = ControlSequenceState::Ground;
                } else if character == '\x1b' {
                    self.control_state = ControlSequenceState::StringControlEscape;
                }
                return;
            }
            ControlSequenceState::StringControlEscape => {
                self.control_state = if character == '\\' {
                    ControlSequenceState::Ground
                } else {
                    ControlSequenceState::StringControl
                };
                return;
            }
            ControlSequenceState::Ground => {}
        }

        if character == '\x1b' {
            self.control_state = ControlSequenceState::Escape;
            return;
        }
        if character == '\u{9d}' {
            self.control_state = ControlSequenceState::Osc;
            return;
        }
        if matches!(character, '\u{90}' | '\u{98}' | '\u{9e}' | '\u{9f}') {
            self.control_state = ControlSequenceState::StringControl;
            return;
        }
        if character == '\r' {
            self.pending_carriage_return = true;
            return;
        }
        if character == '\n' {
            self.finish_line(events);
            self.pending_carriage_return = false;
            return;
        }
        if self.pending_carriage_return {
            // A bare carriage return means subsequent output overwrites the
            // current terminal line rather than extending it.
            self.current_line.clear();
            self.emitted_line = None;
            self.pending_carriage_return = false;
        }
        if character == '\u{8}' {
            self.current_line.pop();
            self.emitted_line = None;
            return;
        }
        if character.is_control() {
            return;
        }

        self.current_line.push(character);
        self.bound_current_line();
        self.detect_current_prompt(events);
    }

    fn finish_line(&mut self, events: &mut Vec<TerminalPrivilegePromptEvent>) {
        let normalized_line = self.current_line.trim();
        let line_has_prompt = detect_terminal_privilege_prompt(normalized_line).is_some();
        let line_has_retry = looks_like_retry_notice(normalized_line);
        if line_has_retry && self.prompt_visible {
            self.retry_pending = true;
        } else if self.prompt_visible && !line_has_prompt && !normalized_line.is_empty() {
            self.prompt_visible = false;
            self.retry_pending = false;
            events.push(TerminalPrivilegePromptEvent::Dismissed);
        }
        self.current_line.clear();
        self.emitted_line = None;
    }

    fn detect_current_prompt(&mut self, events: &mut Vec<TerminalPrivilegePromptEvent>) {
        let normalized_line = self.current_line.trim();
        if self.emitted_line.as_deref() == Some(normalized_line) {
            return;
        }
        let Some(prompt) = detect_terminal_privilege_prompt(normalized_line) else {
            return;
        };
        self.emitted_line = Some(normalized_line.to_string());
        self.prompt_visible = true;
        events.push(TerminalPrivilegePromptEvent::Visible {
            prompt,
            retry: std::mem::take(&mut self.retry_pending),
        });
    }

    fn bound_current_line(&mut self) {
        if self.current_line.len() <= MAX_PROMPT_LINE_BYTES {
            return;
        }
        let mut start = self.current_line.len() - MAX_PROMPT_LINE_BYTES;
        while !self.current_line.is_char_boundary(start) {
            start += 1;
        }
        self.current_line.drain(..start);
        self.emitted_line = None;
    }
}

/// Classifies a normalized terminal line as a standard password prompt.
pub fn detect_terminal_privilege_prompt(line: &str) -> Option<TerminalPrivilegePrompt> {
    let line = line.trim();
    if line.is_empty() || looks_like_password_result(line) {
        return None;
    }
    if let Some(username) = parse_sudo_prompt(line) {
        return Some(TerminalPrivilegePrompt::Sudo {
            username,
            prompt_text: line.to_string(),
        });
    }
    if let Some(target_user) = parse_su_prompt(line) {
        return Some(TerminalPrivilegePrompt::Su {
            target_user,
            prompt_text: line.to_string(),
        });
    }
    is_generic_password_prompt(line).then(|| TerminalPrivilegePrompt::GenericPassword {
        prompt_text: line.to_string(),
    })
}

fn parse_sudo_prompt(line: &str) -> Option<Option<String>> {
    if strip_sudo_marker(line).is_none()
        && let Some(username) = parse_sudo_username_body(line)
    {
        return Some(username);
    }

    let body = strip_sudo_marker(line)?;
    let prompt_body = strip_prompt_colon(body)?;
    if prompt_body.is_empty() || !is_password_prompt_text(line) {
        return None;
    }
    parse_sudo_username_body(body).or_else(|| is_password_label(prompt_body).then_some(None))
}

fn strip_sudo_marker(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !trimmed.to_ascii_lowercase().starts_with("[sudo") {
        return None;
    }
    let marker_end = trimmed.find(']')?;
    Some(trimmed[marker_end + 1..].trim())
}

fn parse_sudo_username_body(line: &str) -> Option<Option<String>> {
    let prompt = strip_prompt_colon(line)?;
    let prefixes = [
        "password for ",
        "passwort für ",
        "passwort fuer ",
        "contraseña para ",
        "contrasena para ",
        "senha para ",
        "mot de passe de ",
        "mot de passe pour ",
        "password di ",
        "пароль для ",
    ];
    for prefix in prefixes {
        if let Some(username) = strip_prefix_ascii_case_insensitive(prompt, prefix) {
            return Some(non_empty_username(username));
        }
    }

    for suffix in ["のパスワード", "암호"] {
        if let Some(username) = prompt.strip_suffix(suffix) {
            return Some(non_empty_username(username));
        }
    }
    parse_cjk_possessive_password_body(prompt)
}

fn parse_su_prompt(line: &str) -> Option<Option<String>> {
    let prompt = strip_prompt_colon(line)?;
    let prefix = prompt.get(..3)?;
    if !prefix.eq_ignore_ascii_case("su:") {
        return None;
    }
    is_password_label(prompt[3..].trim()).then_some(None)
}

fn strip_prompt_colon(line: &str) -> Option<&str> {
    line.trim()
        .strip_suffix(':')
        .or_else(|| line.trim().strip_suffix('：'))
        .map(str::trim)
}

fn strip_prefix_ascii_case_insensitive<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let candidate = text.get(..prefix.len())?;
    candidate
        .eq_ignore_ascii_case(prefix)
        .then(|| text[prefix.len()..].trim())
}

fn non_empty_username(username: &str) -> Option<String> {
    let username = username.trim();
    (!username.is_empty()).then(|| username.to_string())
}

fn is_generic_password_prompt(line: &str) -> bool {
    strip_prompt_colon(line).is_some_and(is_password_label)
}

fn is_password_prompt_text(line: &str) -> bool {
    let Some(prompt) = strip_prompt_colon(line) else {
        return false;
    };
    let lower = prompt.to_ascii_lowercase();
    lower.contains("password")
        || lower.contains("passwort")
        || lower.contains("contraseña")
        || lower.contains("contrasena")
        || lower.contains("senha")
        || lower.contains("mot de passe")
        || lower.contains("пароль")
        || contains_cjk_password_label(prompt)
        || prompt.contains("パスワード")
        || prompt.contains("암호")
}

fn is_password_label(label: &str) -> bool {
    let label = label.trim();
    let lower = label.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "password" | "passwort" | "contraseña" | "contrasena" | "senha" | "mot de passe"
    ) || is_cjk_password_label(label)
        || matches!(label, "パスワード" | "암호" | "пароль")
}

fn parse_cjk_possessive_password_body(line: &str) -> Option<Option<String>> {
    let marker = line.find('的')?;
    let username = line[..marker].trim();
    let label = line[marker + '的'.len_utf8()..].trim();
    is_cjk_password_label(label).then(|| non_empty_username(username))
}

fn contains_cjk_password_label(text: &str) -> bool {
    let compact = cjk_label_compact(text);
    compact.contains("密码") || compact.contains("密碼") || compact.contains("口令")
}

fn is_cjk_password_label(label: &str) -> bool {
    matches!(cjk_label_compact(label).as_str(), "密码" | "密碼" | "口令")
}

fn cjk_label_compact(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
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

fn looks_like_retry_notice(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "sorry",
        "try again",
        "incorrect",
        "authentication failure",
        "permission denied",
        "对不起",
        "重试",
        "再试",
        "错误",
        "失敗",
        "失败",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_detects_first_prompt_split_across_output_chunks() {
        let mut stream = TerminalPrivilegePromptStream::default();
        assert!(stream.observe(b"\x1b]633;C\x07Pass").is_empty());
        assert_eq!(
            stream.observe(b"word:"),
            vec![TerminalPrivilegePromptEvent::Visible {
                prompt: TerminalPrivilegePrompt::GenericPassword {
                    prompt_text: "Password:".to_string(),
                },
                retry: false,
            }]
        );
    }

    #[test]
    fn stream_ignores_split_shell_integration_osc_with_st_terminator() {
        let mut stream = TerminalPrivilegePromptStream::default();
        assert!(stream.observe(b"\x1b]633;C\x1b").is_empty());
        assert_eq!(
            stream.observe(b"\\Password:"),
            vec![TerminalPrivilegePromptEvent::Visible {
                prompt: TerminalPrivilegePrompt::GenericPassword {
                    prompt_text: "Password:".to_string(),
                },
                retry: false,
            }]
        );
    }

    #[test]
    fn stream_detects_history_command_prompt_without_input_observation() {
        let mut stream = TerminalPrivilegePromptStream::default();
        assert_eq!(
            stream.observe(b"[sudo] password for deploy:"),
            vec![TerminalPrivilegePromptEvent::Visible {
                prompt: TerminalPrivilegePrompt::Sudo {
                    username: Some("deploy".to_string()),
                    prompt_text: "[sudo] password for deploy:".to_string(),
                },
                retry: false,
            }]
        );
    }

    #[test]
    fn stream_reports_retry_on_repeated_prompt() {
        let mut stream = TerminalPrivilegePromptStream::default();
        let _ = stream.observe(b"Password:");
        let events = stream.observe(b"\nSorry, try again.\nPassword:");
        assert_eq!(
            events,
            vec![TerminalPrivilegePromptEvent::Visible {
                prompt: TerminalPrivilegePrompt::GenericPassword {
                    prompt_text: "Password:".to_string(),
                },
                retry: true,
            }]
        );
    }

    #[test]
    fn stream_dismisses_prompt_after_unrelated_output_line() {
        let mut stream = TerminalPrivilegePromptStream::default();
        let _ = stream.observe(b"Password:");
        assert_eq!(
            stream.observe(b"\noperation cancelled\n"),
            vec![TerminalPrivilegePromptEvent::Dismissed]
        );
    }
}
