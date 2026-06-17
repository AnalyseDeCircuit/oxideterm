use std::time::{Duration, Instant};

const MAX_PROMPT_TAIL_CHARS: usize = 4_096;
const MAX_TRACKED_INPUT_CHARS: usize = 512;
const PRIVILEGE_COMMAND_CONTEXT_TTL: Duration = Duration::from_secs(15);
const PRIVILEGE_PROMPT_VISIBLE_TTL: Duration = Duration::from_secs(300);
const PRIVILEGE_PROMPT_FILLED_TTL: Duration = Duration::from_secs(8);

#[derive(Clone, Debug, Eq, PartialEq)]
enum PrivilegeCommandContext {
    Sudo,
    Su { target_user: Option<String> },
}

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivilegeInputObservation {
    Normal,
    SecretEntry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivilegePromptConfidence {
    ExplicitPrompt,
    CommandContext,
    GenericPrompt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivilegePromptSnapshot {
    pub prompt: PrivilegePromptMatch,
    pub confidence: PrivilegePromptConfidence,
    pub retry_count: u8,
}

#[derive(Clone, Debug)]
enum PrivilegePromptTrackerState {
    Idle,
    CommandCandidate {
        command_id: u64,
        context: PrivilegeCommandContext,
        observed_at: Instant,
    },
    PromptVisible {
        command_id: Option<u64>,
        prompt: PrivilegePromptMatch,
        confidence: PrivilegePromptConfidence,
        first_seen_at: Instant,
        last_seen_at: Instant,
        retry_count: u8,
    },
    Filled {
        command_id: Option<u64>,
        prompt: PrivilegePromptMatch,
        filled_at: Instant,
        retry_count: u8,
    },
    ManualEntry {
        command_id: Option<u64>,
        prompt: PrivilegePromptMatch,
        started_at: Instant,
        retry_count: u8,
    },
}

#[derive(Clone, Debug)]
pub struct PrivilegePromptTracker {
    input_line: String,
    output_tail: String,
    next_command_id: u64,
    state: PrivilegePromptTrackerState,
}

impl Default for PrivilegePromptTracker {
    fn default() -> Self {
        Self {
            input_line: String::new(),
            output_tail: String::new(),
            next_command_id: 1,
            state: PrivilegePromptTrackerState::Idle,
        }
    }
}

impl PrivilegePromptTracker {
    pub fn observe_user_input_bytes(
        &mut self,
        bytes: &[u8],
        now: Instant,
    ) -> PrivilegeInputObservation {
        if bytes.is_empty() {
            return PrivilegeInputObservation::Normal;
        }

        if self.prompt_is_waiting_for_secret(now) {
            if bytes.iter().any(|byte| *byte == 0x03 || *byte == 0x04) {
                self.reset();
                return PrivilegeInputObservation::Normal;
            }
            if bytes.iter().any(|byte| matches!(*byte, b'\r' | b'\n')) {
                self.mark_secret_filled(now);
            } else if bytes.iter().any(|byte| !byte.is_ascii_control()) {
                self.mark_manual_secret_entry(now);
            }
            self.input_line.clear();
            return PrivilegeInputObservation::SecretEntry;
        }

        for byte in bytes {
            match *byte {
                b'\r' | b'\n' => self.commit_input_line(now),
                0x03 | 0x04 => self.reset(),
                0x08 | 0x7f => {
                    self.input_line.pop();
                }
                0x15 => {
                    self.input_line.clear();
                }
                0x17 => {
                    self.trim_last_input_word();
                }
                byte if byte.is_ascii_graphic() || byte == b' ' => {
                    if self.input_line.chars().count() >= MAX_TRACKED_INPUT_CHARS {
                        let tail = tail_chars(&self.input_line, MAX_TRACKED_INPUT_CHARS - 1);
                        self.input_line = tail.to_string();
                    }
                    self.input_line.push(char::from(byte));
                }
                _ => {}
            }
        }

        PrivilegeInputObservation::Normal
    }

    pub fn observe_output_bytes(&mut self, bytes: &[u8], now: Instant) {
        if bytes.is_empty() {
            return;
        }
        let text = String::from_utf8_lossy(bytes);
        self.observe_output_text(&text, now);
    }

    pub fn observe_output_text(&mut self, text: &str, now: Instant) {
        if text.is_empty() {
            return;
        }

        // This short tail mirrors already-visible terminal output only for
        // prompt classification. It is never logged, persisted, or exposed to
        // AI/tooling, and manual secret keystrokes are excluded before this
        // tracker sees normal command input.
        self.output_tail.push_str(text);
        let trimmed = tail_chars(&self.output_tail, MAX_PROMPT_TAIL_CHARS);
        if trimmed.len() != self.output_tail.len() {
            self.output_tail = trimmed.to_string();
        }

        let retry_notice = output_contains_retry_notice(text);
        let context = self
            .active_command_candidate(now)
            .or_else(|| self.retry_prompt_context());
        if let Some((prompt, confidence)) = detect_privilege_prompt_with_context(
            &self.output_tail,
            context.as_ref().map(|(_, c)| c),
            false,
        ) {
            self.remember_visible_prompt(
                prompt,
                confidence,
                context.map(|(id, _)| id),
                retry_notice,
                now,
            );
        } else if retry_notice {
            self.increment_retry_count();
        }
    }

    pub fn mark_secret_filled(&mut self, now: Instant) {
        let filled = match &self.state {
            PrivilegePromptTrackerState::PromptVisible {
                command_id,
                prompt,
                retry_count,
                ..
            }
            | PrivilegePromptTrackerState::ManualEntry {
                command_id,
                prompt,
                retry_count,
                ..
            } => Some((*command_id, prompt.clone(), *retry_count)),
            _ => None,
        };
        if let Some((command_id, prompt, retry_count)) = filled {
            self.state = PrivilegePromptTrackerState::Filled {
                command_id,
                prompt,
                filled_at: now,
                retry_count,
            };
        };
        self.input_line.clear();
    }

    pub fn snapshot(&self, now: Instant) -> Option<PrivilegePromptSnapshot> {
        match &self.state {
            PrivilegePromptTrackerState::PromptVisible {
                prompt,
                confidence,
                last_seen_at,
                retry_count,
                ..
            } if now.saturating_duration_since(*last_seen_at) <= PRIVILEGE_PROMPT_VISIBLE_TTL => {
                Some(PrivilegePromptSnapshot {
                    prompt: prompt.clone(),
                    confidence: *confidence,
                    retry_count: *retry_count,
                })
            }
            PrivilegePromptTrackerState::Filled { filled_at, .. }
                if now.saturating_duration_since(*filled_at) <= PRIVILEGE_PROMPT_FILLED_TTL =>
            {
                None
            }
            PrivilegePromptTrackerState::ManualEntry { .. } => None,
            _ => None,
        }
    }

    pub fn suppresses_fallback_prompt_detection(&self, now: Instant) -> bool {
        match &self.state {
            PrivilegePromptTrackerState::ManualEntry { .. } => true,
            PrivilegePromptTrackerState::Filled { filled_at, .. } => {
                now.saturating_duration_since(*filled_at) <= PRIVILEGE_PROMPT_FILLED_TTL
            }
            _ => false,
        }
    }

    fn commit_input_line(&mut self, now: Instant) {
        let line = self.input_line.trim();
        if let Some(context) = detect_privilege_command(line) {
            let command_id = self.next_command_id;
            self.next_command_id = self.next_command_id.wrapping_add(1).max(1);
            self.state = PrivilegePromptTrackerState::CommandCandidate {
                command_id,
                context,
                observed_at: now,
            };
        } else if !matches!(self.state, PrivilegePromptTrackerState::Filled { .. }) {
            self.state = PrivilegePromptTrackerState::Idle;
        }
        self.input_line.clear();
    }

    fn trim_last_input_word(&mut self) {
        while self
            .input_line
            .chars()
            .last()
            .is_some_and(char::is_whitespace)
        {
            self.input_line.pop();
        }
        while self
            .input_line
            .chars()
            .last()
            .is_some_and(|ch| !ch.is_whitespace())
        {
            self.input_line.pop();
        }
    }

    fn prompt_is_waiting_for_secret(&self, now: Instant) -> bool {
        match &self.state {
            PrivilegePromptTrackerState::PromptVisible { last_seen_at, .. } => {
                now.saturating_duration_since(*last_seen_at) <= PRIVILEGE_PROMPT_VISIBLE_TTL
            }
            PrivilegePromptTrackerState::ManualEntry { started_at, .. } => {
                now.saturating_duration_since(*started_at) <= PRIVILEGE_PROMPT_VISIBLE_TTL
            }
            _ => false,
        }
    }

    fn active_command_candidate(&mut self, now: Instant) -> Option<(u64, PrivilegeCommandContext)> {
        let candidate = match &self.state {
            PrivilegePromptTrackerState::CommandCandidate {
                command_id,
                context,
                observed_at,
            } => Some((*command_id, context.clone(), *observed_at)),
            _ => None,
        }?;
        if now.saturating_duration_since(candidate.2) > PRIVILEGE_COMMAND_CONTEXT_TTL {
            self.state = PrivilegePromptTrackerState::Idle;
            return None;
        }
        Some((candidate.0, candidate.1))
    }

    fn retry_prompt_context(&self) -> Option<(u64, PrivilegeCommandContext)> {
        match &self.state {
            PrivilegePromptTrackerState::PromptVisible {
                command_id, prompt, ..
            }
            | PrivilegePromptTrackerState::Filled {
                command_id, prompt, ..
            }
            | PrivilegePromptTrackerState::ManualEntry {
                command_id, prompt, ..
            } => prompt_context(prompt).map(|context| (command_id.unwrap_or_default(), context)),
            _ => None,
        }
    }

    fn remember_visible_prompt(
        &mut self,
        prompt: PrivilegePromptMatch,
        confidence: PrivilegePromptConfidence,
        command_id: Option<u64>,
        retry_notice: bool,
        now: Instant,
    ) {
        let retry_count = self
            .current_retry_count_for(&prompt)
            .saturating_add(u8::from(retry_notice));
        let first_seen_at = match &self.state {
            PrivilegePromptTrackerState::PromptVisible {
                prompt: current,
                first_seen_at,
                ..
            } if same_prompt_kind(current, &prompt) => *first_seen_at,
            _ => now,
        };
        self.state = PrivilegePromptTrackerState::PromptVisible {
            command_id,
            prompt,
            confidence,
            first_seen_at,
            last_seen_at: now,
            retry_count,
        };
    }

    fn current_retry_count_for(&self, prompt: &PrivilegePromptMatch) -> u8 {
        match &self.state {
            PrivilegePromptTrackerState::PromptVisible {
                prompt: current,
                retry_count,
                ..
            }
            | PrivilegePromptTrackerState::Filled {
                prompt: current,
                retry_count,
                ..
            }
            | PrivilegePromptTrackerState::ManualEntry {
                prompt: current,
                retry_count,
                ..
            } if same_prompt_kind(current, prompt) => *retry_count,
            _ => 0,
        }
    }

    fn increment_retry_count(&mut self) {
        match &mut self.state {
            PrivilegePromptTrackerState::PromptVisible { retry_count, .. }
            | PrivilegePromptTrackerState::Filled { retry_count, .. }
            | PrivilegePromptTrackerState::ManualEntry { retry_count, .. } => {
                *retry_count = retry_count.saturating_add(1);
            }
            _ => {}
        }
    }

    fn mark_manual_secret_entry(&mut self, now: Instant) {
        let PrivilegePromptTrackerState::PromptVisible {
            command_id,
            prompt,
            retry_count,
            ..
        } = &self.state
        else {
            return;
        };
        self.state = PrivilegePromptTrackerState::ManualEntry {
            command_id: *command_id,
            prompt: prompt.clone(),
            started_at: now,
            retry_count: *retry_count,
        };
    }

    fn reset(&mut self) {
        self.input_line.clear();
        self.state = PrivilegePromptTrackerState::Idle;
    }
}

pub fn detect_custom_privilege_prompt(
    text: &str,
    credential_id: &str,
    prompt_patterns: &[String],
) -> Option<PrivilegePromptMatch> {
    if credential_id.trim().is_empty() {
        return None;
    }
    let line = latest_prompt_candidate_line(text)?;
    if looks_like_password_result(&line) || !line_matches_custom_patterns(&line, prompt_patterns) {
        return None;
    }
    Some(PrivilegePromptMatch::Custom {
        credential_id: credential_id.to_string(),
        prompt_text: line,
    })
}

pub fn detect_privilege_prompt(text: &str) -> Option<PrivilegePromptMatch> {
    detect_privilege_prompt_with_context(text, None, true).map(|(prompt, _)| prompt)
}

fn detect_privilege_prompt_with_context(
    text: &str,
    command_context: Option<&PrivilegeCommandContext>,
    allow_line_context: bool,
) -> Option<(PrivilegePromptMatch, PrivilegePromptConfidence)> {
    let lines = recent_prompt_candidate_lines(text);
    let line = lines.last()?.as_str();

    if looks_like_password_result(line) {
        return None;
    }

    if let Some(username) = parse_sudo_prompt(line) {
        return Some((
            PrivilegePromptMatch::Sudo {
                username,
                prompt_text: line.to_string(),
            },
            PrivilegePromptConfidence::ExplicitPrompt,
        ));
    }

    if let Some(target_user) = parse_su_prompt(line) {
        return Some((
            PrivilegePromptMatch::Su {
                target_user,
                prompt_text: line.to_string(),
            },
            PrivilegePromptConfidence::ExplicitPrompt,
        ));
    }

    if is_generic_password_prompt(line) {
        if let Some(context) = command_context.cloned().or_else(|| {
            allow_line_context
                .then(|| command_context_before_prompt(&lines))
                .flatten()
        }) {
            return Some((
                match context {
                    PrivilegeCommandContext::Sudo => PrivilegePromptMatch::Sudo {
                        username: None,
                        prompt_text: line.to_string(),
                    },
                    PrivilegeCommandContext::Su { target_user } => PrivilegePromptMatch::Su {
                        target_user,
                        prompt_text: line.to_string(),
                    },
                },
                PrivilegePromptConfidence::CommandContext,
            ));
        }
        // A bare password prompt without a nearby sudo/su command is still a
        // sensitive-input opportunity, but it must not be silently classified
        // as privilege escalation. The app layer can offer explicit choices.
        return Some((
            PrivilegePromptMatch::GenericPassword {
                prompt_text: line.to_string(),
            },
            PrivilegePromptConfidence::GenericPrompt,
        ));
    }

    None
}

fn latest_prompt_candidate_line(text: &str) -> Option<String> {
    recent_prompt_candidate_lines(text).pop()
}

fn recent_prompt_candidate_lines(text: &str) -> Vec<String> {
    let tail = tail_chars(text, MAX_PROMPT_TAIL_CHARS);
    recent_non_empty_lines(tail)
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

fn recent_non_empty_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(normalize_terminal_line)
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

fn normalize_terminal_line(line: &str) -> String {
    // Visible terminal snapshots should already be plain text, but stripping
    // CSI escapes here keeps prompt detection resilient if a future renderer
    // passes through raw decorated prompt fragments.
    let mut output = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            continue;
        }
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            let _ = chars.next();
            for control in chars.by_ref() {
                if ('@'..='~').contains(&control) {
                    break;
                }
            }
            continue;
        }
        output.push(ch);
    }
    output
}

fn parse_sudo_prompt(line: &str) -> Option<Option<String>> {
    if strip_sudo_marker(line).is_none()
        && let Some(username) = parse_sudo_username_body(line)
    {
        return Some(username);
    }

    let body = strip_sudo_marker(line)?;
    if !is_password_prompt_text(line) {
        return None;
    }
    parse_sudo_username_body(body).or_else(|| {
        strip_prompt_colon(body)
            .is_some_and(is_password_label)
            .then_some(None)
    })
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

    let suffixes = ["的密码", "的密碼", "のパスワード", "암호"];
    for suffix in suffixes {
        if let Some(username) = prompt.strip_suffix(suffix) {
            return Some(non_empty_username(username));
        }
    }

    None
}

fn parse_su_prompt(line: &str) -> Option<Option<String>> {
    let prompt = strip_prompt_colon(line)?;
    let Some(prefix) = prompt.get(..3) else {
        return None;
    };
    if !prefix.eq_ignore_ascii_case("su:") {
        return None;
    }
    let label = prompt[3..].trim();
    is_password_label(label).then_some(None)
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
        || prompt.contains("密码")
        || prompt.contains("密碼")
        || prompt.contains("パスワード")
        || prompt.contains("암호")
}

fn is_password_label(label: &str) -> bool {
    let label = label.trim();
    let lower = label.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "password" | "passwort" | "contraseña" | "contrasena" | "senha" | "mot de passe"
    ) || matches!(label, "密码" | "密碼" | "パスワード" | "암호" | "пароль")
}

fn line_matches_custom_patterns(line: &str, patterns: &[String]) -> bool {
    let line = line.to_ascii_lowercase();
    patterns
        .iter()
        .map(|pattern| pattern.trim().to_ascii_lowercase())
        .any(|pattern| !pattern.is_empty() && line.contains(&pattern))
}

fn command_context_before_prompt(lines: &[String]) -> Option<PrivilegeCommandContext> {
    lines
        .iter()
        .rev()
        .skip(1)
        .take(6)
        .find_map(|line| detect_privilege_command(line))
}

fn detect_privilege_command(line: &str) -> Option<PrivilegeCommandContext> {
    let command = likely_command_segment(line);
    let words = split_shell_words_lossy(command);
    let command_index = first_command_word_index(&words)?;
    match words.get(command_index)?.as_str() {
        "sudo" => Some(PrivilegeCommandContext::Sudo),
        "su" => Some(PrivilegeCommandContext::Su {
            target_user: parse_su_target_user(&words[command_index + 1..]),
        }),
        _ => None,
    }
}

fn likely_command_segment(line: &str) -> &str {
    let markers = ["❯ ", "➜ ", "$ ", "# ", "% ", "> "];
    markers
        .iter()
        .filter_map(|marker| line.rfind(marker).map(|index| index + marker.len()))
        .max()
        .map(|index| line[index..].trim())
        .unwrap_or_else(|| line.trim())
}

fn split_shell_words_lossy(command: &str) -> Vec<String> {
    // This is intentionally not a shell parser. We only need enough structure
    // to recognize a just-entered `sudo`/`su` command before a generic password
    // prompt, without copying arbitrary command text into secret handling.
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in command.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        if matches!(ch, '\'' | '"') {
            quote = Some(ch);
            continue;
        }
        if ch.is_whitespace() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn first_command_word_index(words: &[String]) -> Option<usize> {
    let mut index = 0;
    while index < words.len() {
        let word =
            words[index].trim_matches(|ch: char| matches!(ch, '❯' | '➜' | '$' | '#' | '%' | '>'));
        if word.is_empty() {
            index += 1;
            continue;
        }
        if matches!(word, "env" | "command") || looks_like_shell_assignment(word) {
            index += 1;
            continue;
        }
        return Some(index);
    }
    None
}

fn looks_like_shell_assignment(word: &str) -> bool {
    let Some((name, _value)) = word.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn parse_su_target_user(words: &[String]) -> Option<String> {
    let mut index = 0;
    while index < words.len() {
        let word = words[index].as_str();
        match word {
            "-" | "-l" | "--login" | "-m" | "-p" | "--preserve-environment" => {
                index += 1;
            }
            "-c" | "--command" | "-s" | "--shell" => {
                index += 2;
            }
            _ if word.starts_with('-') => {
                index += 1;
            }
            _ => return non_empty_username(word),
        }
    }
    Some("root".to_string())
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

fn output_contains_retry_notice(text: &str) -> bool {
    recent_non_empty_lines(text)
        .iter()
        .any(|line| looks_like_retry_notice(line))
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

fn same_prompt_kind(left: &PrivilegePromptMatch, right: &PrivilegePromptMatch) -> bool {
    matches!(
        (left, right),
        (
            PrivilegePromptMatch::Sudo { .. },
            PrivilegePromptMatch::Sudo { .. }
        ) | (
            PrivilegePromptMatch::Su { .. },
            PrivilegePromptMatch::Su { .. }
        ) | (
            PrivilegePromptMatch::GenericPassword { .. },
            PrivilegePromptMatch::GenericPassword { .. }
        )
    ) || matches!(
        (left, right),
        (
            PrivilegePromptMatch::Custom {
                credential_id: left_id,
                ..
            },
            PrivilegePromptMatch::Custom {
                credential_id: right_id,
                ..
            }
        ) if left_id == right_id
    )
}

fn prompt_context(prompt: &PrivilegePromptMatch) -> Option<PrivilegeCommandContext> {
    match prompt {
        PrivilegePromptMatch::Sudo { .. } => Some(PrivilegeCommandContext::Sudo),
        PrivilegePromptMatch::Su { target_user, .. } => Some(PrivilegeCommandContext::Su {
            target_user: target_user.clone(),
        }),
        PrivilegePromptMatch::Custom { .. } | PrivilegePromptMatch::GenericPassword { .. } => None,
    }
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
    fn detects_sudo_rs_authenticate_prompt_without_username() {
        assert_eq!(
            detect_privilege_prompt("sudo true\n[sudo: authenticate] Password:"),
            Some(PrivilegePromptMatch::Sudo {
                username: None,
                prompt_text: "[sudo: authenticate] Password:".to_string(),
            })
        );
    }

    #[test]
    fn detects_traditional_chinese_sudo_prompts_with_username() {
        assert_eq!(
            detect_privilege_prompt("sudo true\n[sudo] dominical 的密碼："),
            Some(PrivilegePromptMatch::Sudo {
                username: Some("dominical".to_string()),
                prompt_text: "[sudo] dominical 的密碼：".to_string(),
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
    fn classifies_generic_password_prompt_after_sudo_command() {
        assert_eq!(
            detect_privilege_prompt("❯ sudo yazi\nPassword:"),
            Some(PrivilegePromptMatch::Sudo {
                username: None,
                prompt_text: "Password:".to_string(),
            })
        );
        assert_eq!(
            detect_privilege_prompt("❯ sudo yazi\n密码："),
            Some(PrivilegePromptMatch::Sudo {
                username: None,
                prompt_text: "密码：".to_string(),
            })
        );
    }

    #[test]
    fn classifies_generic_password_prompt_after_su_command() {
        assert_eq!(
            detect_privilege_prompt("su - root\nPassword:"),
            Some(PrivilegePromptMatch::Su {
                target_user: Some("root".to_string()),
                prompt_text: "Password:".to_string(),
            })
        );
        assert_eq!(
            detect_privilege_prompt("su postgres\n密码："),
            Some(PrivilegePromptMatch::Su {
                target_user: Some("postgres".to_string()),
                prompt_text: "密码：".to_string(),
            })
        );
    }

    #[test]
    fn keeps_plain_application_password_prompts_generic() {
        assert_eq!(
            detect_privilege_prompt("mysql login\nPassword:"),
            Some(PrivilegePromptMatch::GenericPassword {
                prompt_text: "Password:".to_string(),
            })
        );
        assert_eq!(
            detect_privilege_prompt("vault unlock\n密碼："),
            Some(PrivilegePromptMatch::GenericPassword {
                prompt_text: "密碼：".to_string(),
            })
        );
    }

    #[test]
    fn detects_custom_prompt_patterns_without_password_label() {
        assert_eq!(
            detect_custom_privilege_prompt(
                "deploy-tool unlock\nEnter deployment approval token >",
                "custom-1",
                &["approval token".to_string()],
            ),
            Some(PrivilegePromptMatch::Custom {
                credential_id: "custom-1".to_string(),
                prompt_text: "Enter deployment approval token >".to_string(),
            })
        );
        assert_eq!(
            detect_privilege_prompt("deploy-tool unlock\nEnter deployment approval token >"),
            None
        );
    }

    #[test]
    fn custom_prompt_patterns_ignore_password_result_lines() {
        assert_eq!(
            detect_custom_privilege_prompt(
                "password updated successfully",
                "custom-1",
                &["password updated".to_string()],
            ),
            None
        );
    }

    #[test]
    fn rejects_result_and_help_lines() {
        assert_eq!(detect_privilege_prompt("password changed"), None);
        assert_eq!(detect_privilege_prompt("error: password failed"), None);
        assert_eq!(detect_privilege_prompt("Usage: --password: value"), None);
    }

    #[test]
    fn tracker_classifies_generic_prompt_after_observed_sudo_command() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        assert_eq!(
            tracker.observe_user_input_bytes(b"sudo systemctl restart nginx\r", start),
            PrivilegeInputObservation::Normal
        );
        tracker.observe_output_text("Password:", start + Duration::from_millis(40));

        assert_eq!(
            tracker.snapshot(start + Duration::from_millis(40)),
            Some(PrivilegePromptSnapshot {
                prompt: PrivilegePromptMatch::Sudo {
                    username: None,
                    prompt_text: "Password:".to_string(),
                },
                confidence: PrivilegePromptConfidence::CommandContext,
                retry_count: 0,
            })
        );
    }

    #[test]
    fn tracker_keeps_plain_password_prompt_generic_without_command_context() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_output_text("Password:", start);

        assert_eq!(
            tracker.snapshot(start),
            Some(PrivilegePromptSnapshot {
                prompt: PrivilegePromptMatch::GenericPassword {
                    prompt_text: "Password:".to_string(),
                },
                confidence: PrivilegePromptConfidence::GenericPrompt,
                retry_count: 0,
            })
        );
    }

    #[test]
    fn tracker_expires_stale_command_context_before_generic_prompt() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_user_input_bytes(b"sudo id\r", start);
        tracker.observe_output_text("Password:", start + PRIVILEGE_COMMAND_CONTEXT_TTL * 2);

        assert_eq!(
            tracker.snapshot(start + PRIVILEGE_COMMAND_CONTEXT_TTL * 2),
            Some(PrivilegePromptSnapshot {
                prompt: PrivilegePromptMatch::GenericPassword {
                    prompt_text: "Password:".to_string(),
                },
                confidence: PrivilegePromptConfidence::GenericPrompt,
                retry_count: 0,
            })
        );
    }

    #[test]
    fn tracker_does_not_recover_expired_context_from_output_tail() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_user_input_bytes(b"sudo id\r", start);
        tracker.observe_output_text("sudo id\n", start + Duration::from_millis(5));
        tracker.observe_output_text("Password:", start + PRIVILEGE_COMMAND_CONTEXT_TTL * 2);

        assert_eq!(
            tracker.snapshot(start + PRIVILEGE_COMMAND_CONTEXT_TTL * 2),
            Some(PrivilegePromptSnapshot {
                prompt: PrivilegePromptMatch::GenericPassword {
                    prompt_text: "Password:".to_string(),
                },
                confidence: PrivilegePromptConfidence::GenericPrompt,
                retry_count: 0,
            })
        );
    }

    #[test]
    fn tracker_marks_manual_input_at_prompt_as_secret_entry() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_user_input_bytes(b"sudo true\r", start);
        tracker.observe_output_text("Password:", start + Duration::from_millis(10));

        assert_eq!(
            tracker.observe_user_input_bytes(b"not-for-history", start + Duration::from_millis(20)),
            PrivilegeInputObservation::SecretEntry
        );
        assert_eq!(tracker.snapshot(start + Duration::from_millis(20)), None);
        assert!(tracker.suppresses_fallback_prompt_detection(start + Duration::from_millis(20)));

        assert_eq!(
            tracker.observe_user_input_bytes(b"\r", start + Duration::from_millis(30)),
            PrivilegeInputObservation::SecretEntry
        );
        assert_eq!(tracker.snapshot(start + Duration::from_millis(30)), None);
    }

    #[test]
    fn tracker_reopens_prompt_after_retry_notice() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_user_input_bytes(b"sudo true\r", start);
        tracker.observe_output_text("Password:", start + Duration::from_millis(10));
        tracker.mark_secret_filled(start + Duration::from_millis(20));
        tracker.observe_output_text(
            "Sorry, try again.\nPassword:",
            start + Duration::from_millis(30),
        );

        assert_eq!(
            tracker.snapshot(start + Duration::from_millis(30)),
            Some(PrivilegePromptSnapshot {
                prompt: PrivilegePromptMatch::Sudo {
                    username: None,
                    prompt_text: "Password:".to_string(),
                },
                confidence: PrivilegePromptConfidence::CommandContext,
                retry_count: 1,
            })
        );
    }
}
