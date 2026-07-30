use std::time::{Duration, Instant};
use zeroize::Zeroizing;

const MAX_PROMPT_TAIL_CHARS: usize = 4_096;
const MAX_TRACKED_INPUT_CHARS: usize = 512;
const PRIVILEGE_COMMAND_CONTEXT_TTL: Duration = Duration::from_secs(15);
const OPAQUE_COMMAND_OBSERVATION_TTL: Duration = Duration::from_secs(5);
const PRIVILEGE_PROMPT_VISIBLE_TTL: Duration = Duration::from_secs(300);
const PRIVILEGE_PROMPT_FILLED_TTL: Duration = Duration::from_secs(8);
const PRIVILEGE_PROMPT_DEBUG_ENV: &str = "OXIDETERM_PRIVILEGE_DEBUG";

fn log_privilege_prompt_tracker(args: std::fmt::Arguments<'_>) {
    if std::env::var_os(PRIVILEGE_PROMPT_DEBUG_ENV).is_some() {
        eprintln!("[oxideterm:privilege] {args}");
    }
}

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
    OpaqueCommandSubmission {
        observed_at: Instant,
    },
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

fn privilege_command_context_name(context: &PrivilegeCommandContext) -> &'static str {
    match context {
        PrivilegeCommandContext::Sudo => "sudo",
        PrivilegeCommandContext::Su { .. } => "su",
    }
}

fn privilege_prompt_match_name(prompt: &PrivilegePromptMatch) -> &'static str {
    match prompt {
        PrivilegePromptMatch::Sudo { .. } => "sudo",
        PrivilegePromptMatch::Su { .. } => "su",
        PrivilegePromptMatch::Custom { .. } => "custom",
        PrivilegePromptMatch::GenericPassword { .. } => "generic-password",
    }
}

fn privilege_prompt_tracker_state_name(state: &PrivilegePromptTrackerState) -> &'static str {
    match state {
        PrivilegePromptTrackerState::Idle => "idle",
        PrivilegePromptTrackerState::OpaqueCommandSubmission { .. } => "opaque-command-submission",
        PrivilegePromptTrackerState::CommandCandidate { .. } => "command-candidate",
        PrivilegePromptTrackerState::PromptVisible { .. } => "prompt-visible",
        PrivilegePromptTrackerState::Filled { .. } => "filled",
        PrivilegePromptTrackerState::ManualEntry { .. } => "manual-entry",
    }
}

#[derive(Clone)]
pub struct PrivilegePromptTracker {
    input_line: Zeroizing<String>,
    input_line_opaque: bool,
    output_tail: Zeroizing<String>,
    next_command_id: u64,
    state: PrivilegePromptTrackerState,
    state_generation: u64,
}

impl Default for PrivilegePromptTracker {
    fn default() -> Self {
        Self {
            input_line: Zeroizing::new(String::new()),
            input_line_opaque: false,
            output_tail: Zeroizing::new(String::new()),
            next_command_id: 1,
            state: PrivilegePromptTrackerState::Idle,
            state_generation: 0,
        }
    }
}

impl PrivilegePromptTracker {
    pub(crate) fn state_generation(&self) -> u64 {
        self.state_generation
    }

    pub(crate) fn requires_output_observation(&self, now: Instant) -> bool {
        self.next_expiry_deadline()
            .is_some_and(|deadline| now < deadline)
    }

    pub(crate) fn next_expiry_deadline(&self) -> Option<Instant> {
        match &self.state {
            PrivilegePromptTrackerState::OpaqueCommandSubmission { observed_at } => {
                observed_at.checked_add(OPAQUE_COMMAND_OBSERVATION_TTL)
            }
            PrivilegePromptTrackerState::CommandCandidate { observed_at, .. } => {
                observed_at.checked_add(PRIVILEGE_COMMAND_CONTEXT_TTL)
            }
            PrivilegePromptTrackerState::PromptVisible { last_seen_at, .. } => {
                last_seen_at.checked_add(PRIVILEGE_PROMPT_VISIBLE_TTL)
            }
            PrivilegePromptTrackerState::Filled { filled_at, .. } => {
                filled_at.checked_add(PRIVILEGE_PROMPT_FILLED_TTL)
            }
            PrivilegePromptTrackerState::ManualEntry { started_at, .. } => {
                started_at.checked_add(PRIVILEGE_PROMPT_VISIBLE_TTL)
            }
            PrivilegePromptTrackerState::Idle => None,
        }
    }

    pub(crate) fn expire_at(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.next_expiry_deadline() else {
            return false;
        };
        if now < deadline {
            return false;
        }
        self.state = PrivilegePromptTrackerState::Idle;
        self.advance_state_generation();
        true
    }

    fn advance_state_generation(&mut self) {
        self.state_generation = self.state_generation.wrapping_add(1);
    }

    pub fn observe_user_input_bytes(
        &mut self,
        bytes: &[u8],
        now: Instant,
    ) -> PrivilegeInputObservation {
        if bytes.is_empty() {
            return PrivilegeInputObservation::Normal;
        }
        let normalized_input = normalize_privilege_input_bytes(bytes);
        let input_bytes = normalized_input.bytes.as_slice();
        self.input_line_opaque |= normalized_input.invalidates_reconstruction;

        if self.prompt_is_waiting_for_secret(now) {
            if input_bytes
                .iter()
                .any(|byte| *byte == 0x03 || *byte == 0x04)
            {
                self.reset();
                return PrivilegeInputObservation::Normal;
            }
            if input_bytes
                .iter()
                .any(|byte| matches!(*byte, b'\r' | b'\n'))
            {
                self.mark_secret_filled(now);
            } else if input_bytes.iter().any(|byte| !byte.is_ascii_control()) {
                self.mark_manual_secret_entry(now);
            }
            self.input_line.clear();
            self.input_line_opaque = false;
            return PrivilegeInputObservation::SecretEntry;
        }

        for byte in input_bytes {
            match *byte {
                b'\r' | b'\n' => self.commit_input_line(now),
                0x03 | 0x04 => self.reset(),
                0x08 | 0x7f => {
                    self.input_line.pop();
                }
                0x15 => {
                    self.input_line.clear();
                    // Ctrl+U clears the remote line too, restoring a reliable
                    // frontend reconstruction after history navigation.
                    self.input_line_opaque = false;
                }
                0x17 => {
                    self.trim_last_input_word();
                }
                byte if byte.is_ascii_graphic() || byte == b' ' => {
                    if self.input_line.chars().count() >= MAX_TRACKED_INPUT_CHARS {
                        let tail = tail_chars(&self.input_line, MAX_TRACKED_INPUT_CHARS - 1);
                        self.input_line = Zeroizing::new(tail.to_string());
                    }
                    self.input_line.push(char::from(byte));
                }
                _ => {
                    // Completion, history search, and other line-editor
                    // controls can replace text without exposing the result.
                    self.input_line_opaque = true;
                }
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
            self.output_tail = Zeroizing::new(trimmed.to_string());
        }

        let retry_notice = output_contains_retry_notice(text);
        let context = self
            .active_command_candidate(now)
            .or_else(|| self.retry_prompt_context());
        let had_context = context.is_some();
        let output_moved_past_prompt = output_advances_past_prompt(text);
        if let Some((prompt, confidence)) = detect_privilege_prompt_with_context(
            &self.output_tail,
            context.as_ref().map(|(_, c)| c),
            false,
        ) {
            log_privilege_prompt_tracker(format_args!(
                "tracker output: prompt detected prompt_kind={} confidence={:?} retry_notice={} had_context={} output_chars={} output_moved_past_prompt={}",
                privilege_prompt_match_name(&prompt),
                confidence,
                retry_notice,
                had_context,
                text.chars().count(),
                output_moved_past_prompt
            ));
            self.remember_visible_prompt(
                prompt,
                confidence,
                context.map(|(id, _)| id),
                retry_notice,
                now,
            );
        } else if retry_notice {
            log_privilege_prompt_tracker(format_args!(
                "tracker output: retry notice without prompt state={}",
                privilege_prompt_tracker_state_name(&self.state)
            ));
            self.increment_retry_count();
        } else if self.prompt_is_waiting_for_secret(now) && output_moved_past_prompt {
            // Once output advances to a new line without a prompt, a later
            // Enter must not send the saved secret to a different reader.
            log_privilege_prompt_tracker(format_args!(
                "tracker output: reset because output advanced past prompt state={}",
                privilege_prompt_tracker_state_name(&self.state)
            ));
            self.reset();
        }
    }

    pub fn observe_submitted_command(&mut self, command: &str, now: Instant) {
        let line = command.trim();
        if line.is_empty() {
            return;
        }
        self.commit_command_line(line, now);
        self.input_line.clear();
        self.input_line_opaque = false;
    }

    pub(crate) fn observe_reconstructed_submitted_command(&mut self, command: &str, now: Instant) {
        let line = command.trim();
        if line.is_empty() {
            return;
        }
        if matches!(
            self.state,
            PrivilegePromptTrackerState::OpaqueCommandSubmission { .. }
        ) && detect_privilege_command(line).is_none()
        {
            // A shell-owned history edit can leave a lossy frontend tracker
            // with escape-sequence fragments. Such a reconstruction may prove
            // sudo/su context, but it cannot disprove the opaque submission.
            log_privilege_prompt_tracker(format_args!(
                "tracker input: non-privilege reconstruction ignored during opaque submission"
            ));
            self.input_line.clear();
            self.input_line_opaque = false;
            return;
        }
        self.observe_submitted_command(line, now);
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
            log_privilege_prompt_tracker(format_args!(
                "tracker input: secret filled prompt_kind={} retry_count={}",
                privilege_prompt_match_name(&prompt),
                retry_count
            ));
            self.state = PrivilegePromptTrackerState::Filled {
                command_id,
                prompt,
                filled_at: now,
                retry_count,
            };
            // The accepted prompt must not be rediscovered when a full-screen
            // program starts with cursor-control bytes but no printable line.
            self.output_tail.clear();
            self.advance_state_generation();
        };
        self.input_line.clear();
        self.input_line_opaque = false;
    }

    pub fn mark_confirmed_secret_filled(
        &mut self,
        confirmed_prompt: PrivilegePromptMatch,
        now: Instant,
    ) {
        let (command_id, prompt, retry_count) = match &self.state {
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
            } if same_prompt_kind(prompt, &confirmed_prompt) => {
                (*command_id, prompt.clone(), *retry_count)
            }
            _ => (None, confirmed_prompt, 0),
        };
        // Workspace calls this path only after it confirms a visible prompt
        // and one scoped credential. Preserve that confirmation even if the
        // decoded-output event arrived before output observation was armed.
        self.state = PrivilegePromptTrackerState::Filled {
            command_id,
            prompt,
            filled_at: now,
            retry_count,
        };
        self.output_tail.clear();
        self.input_line.clear();
        self.input_line_opaque = false;
        self.advance_state_generation();
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
        let line = Zeroizing::new(self.input_line.trim().to_string());
        if self.input_line_opaque || line.is_empty() {
            self.arm_opaque_command_submission(now);
        } else {
            self.commit_command_line(&line, now);
        }
        self.input_line.clear();
        self.input_line_opaque = false;
    }

    fn arm_opaque_command_submission(&mut self, now: Instant) {
        if !matches!(self.state, PrivilegePromptTrackerState::Idle) {
            return;
        }
        // Shell history and remote line editors can submit text that never
        // crossed the frontend input cache. Observe decoded output briefly so
        // the first prompt is not lost before the screen snapshot catches up.
        self.state = PrivilegePromptTrackerState::OpaqueCommandSubmission { observed_at: now };
        self.advance_state_generation();
        log_privilege_prompt_tracker(format_args!(
            "tracker input: opaque command submission armed"
        ));
    }

    fn commit_command_line(&mut self, line: &str, now: Instant) {
        if let Some(context) = detect_privilege_command(line) {
            let command_id = self.next_command_id;
            self.next_command_id = self.next_command_id.wrapping_add(1).max(1);
            log_privilege_prompt_tracker(format_args!(
                "tracker input: command candidate context_kind={} command_id={}",
                privilege_command_context_name(&context),
                command_id
            ));
            self.state = PrivilegePromptTrackerState::CommandCandidate {
                command_id,
                context,
                observed_at: now,
            };
            self.advance_state_generation();
        } else if !matches!(self.state, PrivilegePromptTrackerState::Filled { .. }) {
            if !matches!(self.state, PrivilegePromptTrackerState::Idle) {
                log_privilege_prompt_tracker(format_args!(
                    "tracker input: non-privilege command reset previous_state={}",
                    privilege_prompt_tracker_state_name(&self.state)
                ));
            }
            self.state = PrivilegePromptTrackerState::Idle;
            self.advance_state_generation();
        }
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
            log_privilege_prompt_tracker(format_args!(
                "tracker state: command candidate expired context_kind={}",
                privilege_command_context_name(&candidate.1)
            ));
            self.state = PrivilegePromptTrackerState::Idle;
            self.advance_state_generation();
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
        let previous_state = privilege_prompt_tracker_state_name(&self.state);
        let first_seen_at = match &self.state {
            PrivilegePromptTrackerState::PromptVisible {
                prompt: current,
                first_seen_at,
                ..
            } if same_prompt_kind(current, &prompt) => *first_seen_at,
            _ => now,
        };
        log_privilege_prompt_tracker(format_args!(
            "tracker state: prompt visible prompt_kind={} confidence={:?} retry_count={} previous_state={} has_command_id={}",
            privilege_prompt_match_name(&prompt),
            confidence,
            retry_count,
            previous_state,
            command_id.is_some()
        ));
        self.state = PrivilegePromptTrackerState::PromptVisible {
            command_id,
            prompt,
            confidence,
            first_seen_at,
            last_seen_at: now,
            retry_count,
        };
        self.advance_state_generation();
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
        let changed = match &mut self.state {
            PrivilegePromptTrackerState::PromptVisible { retry_count, .. }
            | PrivilegePromptTrackerState::Filled { retry_count, .. }
            | PrivilegePromptTrackerState::ManualEntry { retry_count, .. } => {
                *retry_count = retry_count.saturating_add(1);
                log_privilege_prompt_tracker(format_args!(
                    "tracker state: retry_count incremented value={}",
                    *retry_count
                ));
                true
            }
            _ => false,
        };
        if changed {
            self.advance_state_generation();
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
        log_privilege_prompt_tracker(format_args!(
            "tracker input: manual secret entry prompt_kind={} retry_count={}",
            privilege_prompt_match_name(prompt),
            retry_count
        ));
        self.state = PrivilegePromptTrackerState::ManualEntry {
            command_id: *command_id,
            prompt: prompt.clone(),
            started_at: now,
            retry_count: *retry_count,
        };
        self.advance_state_generation();
    }

    fn reset(&mut self) {
        let previous_state = privilege_prompt_tracker_state_name(&self.state);
        let had_input = !self.input_line.is_empty();
        self.input_line.clear();
        self.input_line_opaque = false;
        self.state = PrivilegePromptTrackerState::Idle;
        self.advance_state_generation();
        log_privilege_prompt_tracker(format_args!(
            "tracker state: reset previous_state={} had_input={}",
            previous_state, had_input
        ));
    }
}

struct NormalizedPrivilegeInput {
    bytes: Zeroizing<Vec<u8>>,
    invalidates_reconstruction: bool,
}

fn normalize_privilege_input_bytes(bytes: &[u8]) -> NormalizedPrivilegeInput {
    // Terminal input may arrive through protocol escape sequences instead of
    // IME text commits. Keep only the user text/control bytes that affect the
    // privilege command context. Track editor-owned sequences separately so
    // their unseen line mutations cannot be mistaken for reliable text.
    let mut output = Zeroizing::new(Vec::with_capacity(bytes.len()));
    let mut invalidates_reconstruction = false;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\x1b' if bytes.get(index + 1) == Some(&b'[') => {
                let Some((parameters, final_byte, next_index)) = parse_csi_sequence(bytes, index)
                else {
                    invalidates_reconstruction = true;
                    break;
                };
                if let Some(byte) = trackable_byte_from_csi_u(parameters, final_byte) {
                    output.push(byte);
                } else if !is_bracketed_paste_boundary(parameters, final_byte) {
                    invalidates_reconstruction = true;
                }
                index = next_index;
            }
            b'\x1b' => {
                invalidates_reconstruction = true;
                index += 1;
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    NormalizedPrivilegeInput {
        bytes: output,
        invalidates_reconstruction,
    }
}

fn is_bracketed_paste_boundary(parameters: &[u8], final_byte: u8) -> bool {
    final_byte == b'~' && matches!(parameters, b"200" | b"201")
}

fn parse_csi_sequence(bytes: &[u8], start_index: usize) -> Option<(&[u8], u8, usize)> {
    let mut index = start_index.checked_add(2)?;
    while index < bytes.len() {
        let byte = bytes[index];
        if (b'@'..=b'~').contains(&byte) {
            return Some((&bytes[start_index + 2..index], byte, index + 1));
        }
        index += 1;
    }
    None
}

fn trackable_byte_from_csi_u(parameters: &[u8], final_byte: u8) -> Option<u8> {
    if final_byte != b'u' {
        return None;
    }
    let mut fields = parameters.split(|byte| *byte == b';');
    let key_code = parse_csi_number_field(fields.next()?)?;
    let modifier = fields.next().and_then(parse_csi_number_field).unwrap_or(1);
    if modifier > 2 {
        return None;
    }
    match key_code {
        13 => Some(b'\r'),
        127 => Some(0x7f),
        0x20..=0x7e => Some(key_code as u8),
        _ => None,
    }
}

fn parse_csi_number_field(field: &[u8]) -> Option<u32> {
    let number = field.split(|byte| *byte == b':').next()?;
    std::str::from_utf8(number).ok()?.parse().ok()
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
    // Output observation receives the decoded PTY stream before terminal
    // controls are rendered. Strip CSI styling and OSC shell-integration
    // metadata so prompt matching sees the same text as the terminal grid.
    let mut output = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            continue;
        }
        if ch == '\x1b' {
            match chars.next() {
                Some('[') => {
                    for control in chars.by_ref() {
                        if ('@'..='~').contains(&control) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(control) = chars.next() {
                        if control == '\x07' {
                            break;
                        }
                        if control == '\x1b' && chars.peek() == Some(&'\\') {
                            let _ = chars.next();
                            break;
                        }
                    }
                }
                Some(_) | None => {}
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
    let prompt_body = strip_prompt_colon(body)?;
    if prompt_body.is_empty() {
        return None;
    }
    if !is_password_prompt_text(line) {
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

    let suffixes = ["のパスワード", "암호"];
    for suffix in suffixes {
        if let Some(username) = prompt.strip_suffix(suffix) {
            return Some(non_empty_username(username));
        }
    }

    parse_cjk_possessive_password_body(prompt)
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
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
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

fn output_advances_past_prompt(text: &str) -> bool {
    text.contains('\n') || text.contains('\r')
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
    fn tracker_generation_and_expiry_follow_state_transitions() {
        let now = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();
        assert_eq!(tracker.state_generation(), 0);
        assert_eq!(tracker.next_expiry_deadline(), None);
        assert!(!tracker.requires_output_observation(now));

        tracker.observe_submitted_command("sudo true", now);
        let command_generation = tracker.state_generation();
        assert!(command_generation > 0);
        assert!(tracker.requires_output_observation(now));
        assert_eq!(
            tracker.next_expiry_deadline(),
            now.checked_add(PRIVILEGE_COMMAND_CONTEXT_TTL)
        );

        tracker.observe_output_text("[sudo] password for test:", now);
        assert!(tracker.state_generation() > command_generation);
        assert_eq!(
            tracker.next_expiry_deadline(),
            now.checked_add(PRIVILEGE_PROMPT_VISIBLE_TTL)
        );

        tracker.mark_secret_filled(now);
        assert!(tracker.requires_output_observation(now));
        assert_eq!(
            tracker.next_expiry_deadline(),
            now.checked_add(PRIVILEGE_PROMPT_FILLED_TTL)
        );
        assert!(!tracker.expire_at(now));
        assert!(tracker.expire_at(now + PRIVILEGE_PROMPT_FILLED_TTL));
        assert_eq!(tracker.next_expiry_deadline(), None);
        assert!(!tracker.requires_output_observation(now + PRIVILEGE_PROMPT_FILLED_TTL));
    }

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
    fn detects_chinese_sudo_prompt_with_kouling_label() {
        assert_eq!(
            detect_privilege_prompt("sudo true\n[sudo] deploy 的口令："),
            Some(PrivilegePromptMatch::Sudo {
                username: Some("deploy".to_string()),
                prompt_text: "[sudo] deploy 的口令：".to_string(),
            })
        );
    }

    #[test]
    fn detects_chinese_sudo_prompt_with_spaced_password_label() {
        assert_eq!(
            detect_privilege_prompt("sudo true\n[sudo] deploy 的密 码："),
            Some(PrivilegePromptMatch::Sudo {
                username: Some("deploy".to_string()),
                prompt_text: "[sudo] deploy 的密 码：".to_string(),
            })
        );
    }

    #[test]
    fn rejects_explicit_sudo_marker_without_password_label() {
        assert_eq!(
            detect_privilege_prompt("sudo true\n[sudo] deploy 的通行码："),
            None
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
    fn tracker_classifies_generic_prompt_after_split_sudo_command_input() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_user_input_bytes(b"sudo yazi", start);
        tracker.observe_user_input_bytes(b"\r", start + Duration::from_millis(10));
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
    fn tracker_observes_first_prompt_after_opaque_shell_history_submission() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        // Shell history edits the remote line without exposing its command
        // text to the frontend. Previously typed text must not make that
        // reconstructed command look reliable after history navigation.
        tracker.observe_user_input_bytes(b"echo stale", start);
        tracker.observe_user_input_bytes(b"\x1b[A", start);
        tracker.observe_user_input_bytes(b"\r", start + Duration::from_millis(10));
        tracker.observe_reconstructed_submitted_command("OA", start + Duration::from_millis(10));

        assert!(
            tracker.requires_output_observation(start + Duration::from_millis(10)),
            "lossy command reconstruction must not disable decoded output events"
        );

        tracker.observe_output_text("Password:", start + Duration::from_millis(40));

        assert_eq!(
            tracker.snapshot(start + Duration::from_millis(40)),
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
    fn tracker_trusts_shell_integration_after_opaque_submission() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_user_input_bytes(b"\x1b[A\r", start);
        tracker.observe_submitted_command("printf ok", start);

        assert!(
            !tracker.requires_output_observation(start),
            "shell integration must replace the opaque fallback with its known command"
        );
    }

    #[test]
    fn tracker_upgrades_opaque_submission_from_reconstructed_sudo() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_user_input_bytes(b"\x1b[A\r", start);
        tracker.observe_reconstructed_submitted_command("sudo true", start);
        tracker.observe_output_text("Password:", start + Duration::from_millis(10));

        assert_eq!(
            tracker.snapshot(start + Duration::from_millis(10)),
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
    fn tracker_bounds_opaque_history_output_observation() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_user_input_bytes(b"\x1b[A", start);
        assert!(
            !tracker.requires_output_observation(start),
            "history navigation alone must not enable decoded output events"
        );

        tracker.observe_user_input_bytes(b"\r", start + Duration::from_millis(10));
        assert!(tracker.requires_output_observation(start + Duration::from_millis(10)));
        assert!(!tracker.requires_output_observation(
            start + Duration::from_millis(10) + OPAQUE_COMMAND_OBSERVATION_TTL
        ));
    }

    #[test]
    fn tracker_keeps_reconstructed_non_privilege_commands_out_of_output_observation() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_user_input_bytes(b"printf ok\r", start);

        assert!(
            !tracker.requires_output_observation(start),
            "known non-privilege commands must preserve the output fast path"
        );
    }

    #[test]
    fn tracker_ctrl_u_restores_reliable_input_after_history_navigation() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_user_input_bytes(b"\x1b[A", start);
        tracker.observe_user_input_bytes(b"\x15printf ok\r", start);

        assert!(
            !tracker.requires_output_observation(start),
            "clearing the remote line must restore reconstructed input behavior"
        );
    }

    #[test]
    fn tracker_classifies_macos_password_prompt_after_submitted_sudo_command() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_submitted_command("sudo yazi", start);
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
    fn tracker_classifies_first_password_prompt_after_shell_integration_osc() {
        let start = Instant::now();
        for output_chunks in [
            &["\x1b]633;C\x07Password:"][..],
            &["\x1b]633;C\x1b\\Password:"][..],
            &["\x1b]633;C", "\x07Password:"][..],
        ] {
            let mut tracker = PrivilegePromptTracker::default();
            assert_eq!(
                tracker.observe_user_input_bytes(b"sudo vim\r", start),
                PrivilegeInputObservation::Normal
            );
            for output in output_chunks {
                tracker.observe_output_text(output, start + Duration::from_millis(40));
            }

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
    }

    #[test]
    fn tracker_classifies_generic_prompt_after_bracketed_paste_protocol_input() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_user_input_bytes(
            b"\x1b[200~sudo yazi\x1b[201~\r",
            start + Duration::from_millis(10),
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
    fn tracker_classifies_generic_prompt_after_kitty_keyboard_protocol_input() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_user_input_bytes(
            b"\x1b[115;1u\x1b[117;1u\x1b[100;1u\x1b[111;1u\x1b[32;1u\x1b[121;1u\x1b[97;1u\x1b[122;1u\x1b[105;1u\x1b[13;1u",
            start + Duration::from_millis(10),
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

    #[test]
    fn tracker_does_not_reopen_filled_prompt_on_full_screen_entry() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_user_input_bytes(b"sudo vim /etc/hosts\r", start);
        tracker.observe_output_text(
            "[sudo] password for alice:",
            start + Duration::from_millis(10),
        );
        tracker.mark_secret_filled(start + Duration::from_millis(20));
        tracker.observe_output_text(
            "\x1b[?1049h\x1b[22;0;0t\x1b[H\x1b[2J",
            start + Duration::from_millis(30),
        );

        assert_eq!(tracker.snapshot(start + Duration::from_millis(30)), None);
        assert!(tracker.suppresses_fallback_prompt_detection(start + Duration::from_millis(30)));
    }

    #[test]
    fn tracker_suppresses_fallback_after_confirmed_fill_without_output_event() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_user_input_bytes(b"sudo vim /etc/hosts\r", start);
        tracker.observe_user_input_bytes(b"\r", start + Duration::from_millis(10));
        tracker.mark_confirmed_secret_filled(
            PrivilegePromptMatch::Sudo {
                username: Some("alice".to_string()),
                prompt_text: "[sudo] password for alice:".to_string(),
            },
            start + Duration::from_millis(20),
        );

        assert_eq!(tracker.snapshot(start + Duration::from_millis(20)), None);
        assert!(tracker.suppresses_fallback_prompt_detection(start + Duration::from_millis(20)));
    }

    #[test]
    fn tracker_clears_prompt_when_output_moves_past_it() {
        let start = Instant::now();
        let mut tracker = PrivilegePromptTracker::default();

        tracker.observe_user_input_bytes(b"sudo true\r", start);
        tracker.observe_output_text(
            "[sudo] password for alice:",
            start + Duration::from_millis(10),
        );
        assert!(
            tracker
                .snapshot(start + Duration::from_millis(10))
                .is_some()
        );

        tracker.observe_output_text(
            "\r\nsudo: timed out reading password\r\nalice@host:~$ ",
            start + Duration::from_millis(20),
        );

        assert_eq!(tracker.snapshot(start + Duration::from_millis(20)), None);
    }
}
