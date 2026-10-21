use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::actions::TerminalBroadcastMenuPlacement;
use super::ime::WorkspaceImeTarget;
use super::terminal_git::{
    TerminalGitAiCommitError, TerminalGitBranchError, TerminalGitPanelSection,
    TerminalGitPathAction, TerminalGitRepositoryAction, terminal_git_path_action_label_key,
    terminal_git_repository_action_label_key,
};
use super::*;
use oxideterm_connections::{
    LOCAL_SHELL_PRIVILEGE_CONNECTION_ID, SSH_COMMON_PRIVILEGE_SCOPE_ID,
    privilege_host_scope_matches,
};
use oxideterm_environment::{
    CurrentDirectoryScope, CurrentDirectorySnapshot, CurrentDirectorySource, GitChangedPath,
    GitRepositoryStatus, ProjectSnapshot, ProjectTask, ProjectTaskGroup,
};
use oxideterm_gpui_ui::button::{ButtonRadius, IconButtonOptions};
use oxideterm_gpui_ui::context_menu::{
    ContextMenuActionableStyle, context_menu_event_boundary, context_menu_pointer_event_boundary,
};
use oxideterm_gpui_ui::text_input::{
    TextInputView, text_input, text_input_anchor_probe, text_input_with_viewport_and_ghost_text,
};
use oxideterm_gpui_ui::{
    ActionChipOptions, ActionChipTextTone, CommandPanelOptions, ContextChipOptions,
    EntityListRowOptions, MonospaceDatumOptions, MonospaceDatumTone, StatusPillOptions, StatusTone,
    action_chip, action_chip_foreground, command_panel, command_panel_body, context_chip,
    entity_list_row, monospace_datum, status_pill,
};
use oxideterm_terminal_recording::format_recording_elapsed;

pub(in crate::workspace) mod completion;

mod bar;
mod context;
mod git;
mod highlight;
mod history;
mod privilege;
mod sender;

pub(in crate::workspace) use history::{SavedConsoleProfileKind, delete_saved_console_profile};

const TERMINAL_BROADCAST_MENU_WIDTH: f32 = 340.0;
const TERMINAL_BROADCAST_MENU_MAX_HEIGHT: f32 = 520.0;
const TERMINAL_CWD_MENU_WIDTH: f32 = 520.0;
const TERMINAL_CWD_MENU_MAX_HEIGHT: f32 = 420.0;
const TERMINAL_CWD_MENU_MARGIN: f32 = 12.0;
const TERMINAL_GIT_BRANCH_MENU_WIDTH: f32 = 460.0;
const TERMINAL_GIT_BRANCH_MENU_BODY_HEIGHT: f32 = 360.0;
const TERMINAL_GIT_BRANCH_MENU_BODY_MAX_HEIGHT: f32 = 520.0;
const TERMINAL_GIT_BRANCH_MENU_MARGIN: f32 = 12.0;
const TERMINAL_PROJECT_MENU_WIDTH: f32 = 640.0;
const TERMINAL_PROJECT_MENU_BODY_MAX_HEIGHT: f32 = 420.0;
const TERMINAL_PROJECT_MENU_MARGIN: f32 = 12.0;
const TERMINAL_COMMAND_CONTEXT_CHIP_MAX_WIDTH: f32 = 260.0; // Keep context chips compact beside command-bar actions.
const TERMINAL_COMMAND_PROJECT_CHIP_MAX_WIDTH: f32 = 240.0; // Project labels are shorter than cwd/git labels in Tauri.
const TERMINAL_COMMAND_TOOLBAR_HEIGHT: f32 = 32.0;
pub(in crate::workspace) const TERMINAL_SYNC_HEADER_HEIGHT: f32 = 26.0;
pub(in crate::workspace) const TERMINAL_SENDER_COMPACT_HEIGHT: f32 =
    oxideterm_gpui_ui::WORKSPACE_STATUS_BAR_HEIGHT;
const PRIVILEGE_PROMPT_DEBUG_ENV: &str = "OXIDETERM_PRIVILEGE_DEBUG";

#[derive(Clone, Debug, Eq, PartialEq)]
struct MatchedPrivilegeCredential {
    connection_id: String,
    credential_id: String,
    label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PrivilegePromptHelperState {
    connection_id: String,
    prompt: PrivilegePromptMatch,
    matches: Vec<MatchedPrivilegeCredential>,
}

pub(super) struct PrivilegeCredentialPicker {
    pane_id: PaneId,
    session_id: TerminalSessionId,
    prompt_generation: u64,
    state: PrivilegePromptHelperState,
    selected: usize,
}

pub(super) struct PendingPrivilegeTotp {
    pub(super) pane_id: PaneId,
    pub(super) session_id: TerminalSessionId,
    pub(super) totp_credential_id: String,
    pub(super) first_prompt_text: String,
    pub(super) second_factor_patterns: Vec<String>,
    pub(super) second_factor_user_hint: Option<String>,
    pub(super) expires_at: std::time::Instant,
    pub(super) require_click_to_send: bool,
}

impl PrivilegeCredentialPicker {
    fn is_current(
        &self,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        generation: u64,
        state: &PrivilegePromptHelperState,
    ) -> bool {
        self.pane_id == pane_id
            && self.session_id == session_id
            && self.prompt_generation == generation
            && self.state == *state
    }
}

fn tab_kind_allows_privilege_prompt_helper(tab_kind: &TabKind) -> bool {
    // Local shells use an app-level scope. SSH terminals are allowed only after
    // active_privilege_scope_credentials resolves the active terminal through
    // the node ownership maps, never through host/title/runtime heuristics.
    matches!(
        tab_kind,
        TabKind::LocalTerminal | TabKind::SshTerminal | TabKind::Workspace
    )
}

fn log_privilege_prompt_helper(args: std::fmt::Arguments<'_>) {
    if std::env::var_os(PRIVILEGE_PROMPT_DEBUG_ENV).is_some() {
        eprintln!("[oxideterm:privilege] {args}");
    }
}

fn terminal_git_section_icon(section: TerminalGitPanelSection) -> LucideIcon {
    match section {
        TerminalGitPanelSection::Branches => LucideIcon::GitFork,
        TerminalGitPanelSection::Changes => LucideIcon::Pencil,
        TerminalGitPanelSection::Resolve => LucideIcon::AlertTriangle,
        TerminalGitPanelSection::History => LucideIcon::History,
        TerminalGitPanelSection::More => LucideIcon::MoreVertical,
    }
}

fn terminal_git_action_icon(action: TerminalGitRepositoryAction) -> LucideIcon {
    match action {
        TerminalGitRepositoryAction::Fetch => LucideIcon::RefreshCw,
        TerminalGitRepositoryAction::FetchAll => LucideIcon::RefreshCw,
        TerminalGitRepositoryAction::Pull => LucideIcon::Download,
        TerminalGitRepositoryAction::Push
        | TerminalGitRepositoryAction::Publish
        | TerminalGitRepositoryAction::PushTags => LucideIcon::Upload,
        TerminalGitRepositoryAction::Status => LucideIcon::ListChecks,
        TerminalGitRepositoryAction::Diff | TerminalGitRepositoryAction::DiffStaged => {
            LucideIcon::FileText
        }
        TerminalGitRepositoryAction::Log
        | TerminalGitRepositoryAction::LogStat
        | TerminalGitRepositoryAction::Reflog => LucideIcon::History,
        TerminalGitRepositoryAction::Stash => LucideIcon::Archive,
        TerminalGitRepositoryAction::StashList => LucideIcon::ListTree,
        TerminalGitRepositoryAction::StashPop => LucideIcon::Inbox,
        TerminalGitRepositoryAction::StashShowLatest => LucideIcon::FileText,
        TerminalGitRepositoryAction::StashApplyLatest => LucideIcon::Inbox,
        TerminalGitRepositoryAction::StashDropLatest => LucideIcon::Trash2,
        TerminalGitRepositoryAction::StageAll => LucideIcon::Plus,
        TerminalGitRepositoryAction::UnstageAll => LucideIcon::RotateCcw,
        TerminalGitRepositoryAction::Commit
        | TerminalGitRepositoryAction::CommitVerbose
        | TerminalGitRepositoryAction::CommitSignoff => LucideIcon::CheckCircle,
        TerminalGitRepositoryAction::Amend | TerminalGitRepositoryAction::AmendNoEdit => {
            LucideIcon::Pencil
        }
        TerminalGitRepositoryAction::RebasePull
        | TerminalGitRepositoryAction::RebaseInteractive => LucideIcon::GitFork,
        TerminalGitRepositoryAction::BranchVerbose => LucideIcon::GitFork,
        TerminalGitRepositoryAction::RemoteList => LucideIcon::Network,
        TerminalGitRepositoryAction::TagList => LucideIcon::Hash,
        TerminalGitRepositoryAction::WorktreeList => LucideIcon::FolderOpen,
        TerminalGitRepositoryAction::ConflictFiles => LucideIcon::AlertTriangle,
        TerminalGitRepositoryAction::Continue(_) => LucideIcon::Check,
        TerminalGitRepositoryAction::Abort(_) => LucideIcon::X,
        TerminalGitRepositoryAction::Skip(_) => LucideIcon::ArrowRight,
    }
}

fn terminal_project_group_icon(group: ProjectTaskGroup) -> LucideIcon {
    match group {
        ProjectTaskGroup::Develop => LucideIcon::Rocket,
        ProjectTaskGroup::Test => LucideIcon::CheckCircle,
        ProjectTaskGroup::Build => LucideIcon::FileCode,
        ProjectTaskGroup::Run => LucideIcon::Play,
        ProjectTaskGroup::Docker => LucideIcon::HardDrive,
        ProjectTaskGroup::Custom => LucideIcon::ListChecks,
    }
}

fn terminal_project_group_label_key(group: ProjectTaskGroup) -> &'static str {
    match group {
        ProjectTaskGroup::Develop => "terminal.project.group_develop",
        ProjectTaskGroup::Test => "terminal.project.group_test",
        ProjectTaskGroup::Build => "terminal.project.group_build",
        ProjectTaskGroup::Run => "terminal.project.group_run",
        ProjectTaskGroup::Docker => "terminal.project.group_docker",
        ProjectTaskGroup::Custom => "terminal.project.group_custom",
    }
}

fn privilege_prompt_kind_name(prompt: &PrivilegePromptMatch) -> &'static str {
    match prompt {
        PrivilegePromptMatch::Sudo { .. } => "sudo",
        PrivilegePromptMatch::Su { .. } => "su",
        PrivilegePromptMatch::Custom { .. } => "custom",
        PrivilegePromptMatch::GenericPassword { .. } => "generic-password",
    }
}

fn tab_kind_privilege_scope_name(tab_kind: &TabKind) -> &'static str {
    match tab_kind {
        TabKind::LocalTerminal => "local-terminal",
        TabKind::SshTerminal => "ssh-terminal",
        _ => "unsupported",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PrivilegePromptTextShape {
    chars: usize,
    lines: usize,
    has_ascii_colon: bool,
    has_fullwidth_colon: bool,
    ends_with_prompt_colon: bool,
    contains_sudo_marker: bool,
    starts_with_sudo_marker: bool,
    contains_password_word: bool,
    contains_cjk_password: bool,
    contains_escape: bool,
}

fn privilege_prompt_text_shape(text: &str) -> PrivilegePromptTextShape {
    let trimmed = text.trim();
    let lower = trimmed.to_ascii_lowercase();
    let compact_cjk: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
    PrivilegePromptTextShape {
        chars: text.chars().count(),
        lines: text.lines().count(),
        has_ascii_colon: text.contains(':'),
        has_fullwidth_colon: text.contains('：'),
        ends_with_prompt_colon: trimmed.ends_with(':') || trimmed.ends_with('：'),
        contains_sudo_marker: lower.contains("[sudo"),
        starts_with_sudo_marker: lower.starts_with("[sudo"),
        contains_password_word: lower.contains("password"),
        contains_cjk_password: compact_cjk.contains("密码")
            || compact_cjk.contains("密碼")
            || compact_cjk.contains("口令"),
        contains_escape: text.contains('\x1b'),
    }
}

fn saved_ssh_privilege_scope_id(
    node_saved_connection_id: Option<&str>,
    node_origin: Option<&NodeOrigin>,
) -> Option<String> {
    node_saved_connection_id
        .map(str::trim)
        .filter(|connection_id| !connection_id.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            node_origin
                .and_then(NodeOrigin::saved_connection_id)
                .map(str::trim)
                .filter(|connection_id| !connection_id.is_empty())
                .map(ToOwned::to_owned)
        })
}

fn privilege_credential_matches_prompt(
    credential: &SavedPrivilegeCredential,
    prompt: &PrivilegePromptMatch,
) -> bool {
    if !credential.enabled {
        return false;
    }
    // Response mode is deliberately absent here: two-stage credentials must
    // match the same sudo/su/custom prompts as password-only credentials.
    match prompt {
        PrivilegePromptMatch::Sudo { username, .. } => match credential.prompt_type {
            PrivilegePromptType::Sudo => username.as_ref().is_none_or(|prompt_username| {
                credential
                    .username_hint
                    .as_ref()
                    .is_none_or(|hint| prompt_username == hint)
            }),
            PrivilegePromptType::Custom => {
                privilege_prompt_matches_custom_patterns(prompt, &credential.prompt_patterns)
            }
            PrivilegePromptType::Su => false,
        },
        PrivilegePromptMatch::Su { target_user, .. } => match credential.prompt_type {
            PrivilegePromptType::Su => target_user.as_ref().is_none_or(|prompt_user| {
                credential
                    .username_hint
                    .as_ref()
                    .is_none_or(|hint| prompt_user == hint)
            }),
            PrivilegePromptType::Custom => {
                privilege_prompt_matches_custom_patterns(prompt, &credential.prompt_patterns)
            }
            PrivilegePromptType::Sudo => false,
        },
        PrivilegePromptMatch::Custom { .. } => {
            credential.prompt_type == PrivilegePromptType::Custom
                && privilege_prompt_matches_custom_patterns(prompt, &credential.prompt_patterns)
        }
        PrivilegePromptMatch::GenericPassword { .. } => match credential.prompt_type {
            // Bare `Password:` carries no reliable sudo/su identity. Offer only
            // scoped, click-to-send candidates and never infer a command kind.
            PrivilegePromptType::Sudo | PrivilegePromptType::Su => true,
            PrivilegePromptType::Custom => {
                privilege_prompt_matches_custom_patterns(prompt, &credential.prompt_patterns)
            }
        },
    }
}

fn privilege_prompt_matches_custom_patterns(
    prompt: &PrivilegePromptMatch,
    patterns: &[String],
) -> bool {
    let prompt_text = match prompt {
        PrivilegePromptMatch::Sudo { prompt_text, .. }
        | PrivilegePromptMatch::Su { prompt_text, .. }
        | PrivilegePromptMatch::Custom { prompt_text, .. }
        | PrivilegePromptMatch::GenericPassword { prompt_text } => prompt_text,
    }
    .to_ascii_lowercase();
    patterns
        .iter()
        .map(|pattern| pattern.trim().to_ascii_lowercase())
        .any(|pattern| !pattern.is_empty() && prompt_text.contains(&pattern))
}

fn terminal_cwd_chip_label(path: &str) -> String {
    let path = path.trim();
    if path.is_empty()
        || path == "/"
        || path == "~"
        || path.ends_with(":\\")
        || path.ends_with(":/")
    {
        return path.to_string();
    }
    let separator = if path.contains('\\') { '\\' } else { '/' };
    let mut segments = path
        .split(separator)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return path.to_string();
    }
    let tail = segments.split_off(segments.len().saturating_sub(2));
    let label = tail.join(&separator.to_string());
    if path.starts_with("~/") && tail.len() == 1 {
        format!("~/{label}")
    } else {
        label
    }
}

fn terminal_cwd_chip_tooltip(
    snapshot: Option<&CurrentDirectorySnapshot>,
    host: Option<String>,
    i18n: &I18n,
) -> String {
    let Some(snapshot) = snapshot else {
        return i18n.t("terminal.cwd.unavailable");
    };
    let scope = match snapshot.scope() {
        CurrentDirectoryScope::Local => i18n.t("terminal.cwd.scope_local"),
        CurrentDirectoryScope::SshNode(_) => i18n.t("terminal.cwd.scope_ssh"),
    };
    let source = match snapshot.source() {
        CurrentDirectorySource::ProcessFallback => i18n.t("terminal.cwd.source_process"),
        CurrentDirectorySource::SessionDefault => i18n.t("terminal.cwd.source_manual"),
        CurrentDirectorySource::UserAction => i18n.t("terminal.cwd.source_manual"),
        CurrentDirectorySource::VisibleText => i18n.t("terminal.cwd.source_manual"),
        CurrentDirectorySource::ShellIntegration => i18n.t("terminal.cwd.source_shell"),
    };
    let quality = match snapshot.source() {
        CurrentDirectorySource::ProcessFallback => i18n.t("terminal.cwd.quality_process"),
        CurrentDirectorySource::SessionDefault => i18n.t("terminal.cwd.quality_manual"),
        CurrentDirectorySource::UserAction => i18n.t("terminal.cwd.quality_manual"),
        CurrentDirectorySource::VisibleText => i18n.t("terminal.cwd.quality_manual"),
        CurrentDirectorySource::ShellIntegration => i18n.t("terminal.cwd.quality_cwd_only"),
    };
    let mut lines = vec![
        snapshot.path().to_string(),
        format!("{scope} · {source} · {quality}"),
    ];
    if let Some(host) = host.filter(|host| !host.trim().is_empty()) {
        lines.push(format!("{}: {host}", i18n.t("terminal.cwd.host")));
    }
    lines.join("\n")
}

/// Credential sources for one terminal scope. Precedence is resolved after
/// prompt matching so a disabled or non-matching connection-specific
/// credential cannot hide a shared credential that does match.
#[derive(Clone, Debug, Eq, PartialEq)]
struct PrivilegeCredentialScope {
    connection_id: Option<String>,
    host: String,
    exact: Vec<SavedPrivilegeCredential>,
    common: Vec<SavedPrivilegeCredential>,
}

impl PrivilegeCredentialScope {
    fn common_credential_applies(&self, credential: &SavedPrivilegeCredential) -> bool {
        !privilege_host_patterns_are_specific(&credential.host_patterns)
            || privilege_host_scope_matches(&self.host, &credential.host_patterns)
    }

    /// Prompt classification must not inspect a shared credential that its
    /// host restriction excludes from this connection.
    fn credentials(&self) -> impl Iterator<Item = &SavedPrivilegeCredential> {
        self.exact.iter().chain(
            self.common
                .iter()
                .filter(|c| self.common_credential_applies(c)),
        )
    }

    fn is_empty(&self) -> bool {
        self.exact.is_empty()
            && !self
                .common
                .iter()
                .any(|c| self.common_credential_applies(c))
    }
}

#[cfg(test)]
fn build_privilege_prompt_helper_state(
    connection_id: String,
    credentials: &[SavedPrivilegeCredential],
    visible_text: &str,
) -> Option<PrivilegePromptHelperState> {
    // Pure helper tests synthesize the semantic session event from their text
    // fixture; production code receives the equivalent tracked prompt.
    let tracked_prompt = oxideterm_gpui_terminal::detect_privilege_prompt(visible_text);
    let scope = PrivilegeCredentialScope {
        connection_id: Some(connection_id),
        host: String::new(),
        exact: credentials.to_vec(),
        common: Vec::new(),
    };
    build_privilege_prompt_helper_state_with_tracked_prompt(scope, visible_text, tracked_prompt)
}

fn build_privilege_prompt_helper_state_with_tracked_prompt(
    scope: PrivilegeCredentialScope,
    visible_text: &str,
    tracked_prompt: Option<PrivilegePromptMatch>,
) -> Option<PrivilegePromptHelperState> {
    let prompt = choose_privilege_prompt(scope.credentials(), visible_text, tracked_prompt)?;
    Some(build_privilege_prompt_helper_state_from_prompt(
        scope, prompt,
    ))
}

fn latest_visible_prompt_line(text: &str) -> Option<&str> {
    text.lines()
        .map(str::trim)
        .rev()
        .find(|line| !line.is_empty())
}

fn privilege_line_matches_patterns(line: &str, patterns: &[String]) -> bool {
    let line = line.to_ascii_lowercase();
    patterns
        .iter()
        .map(|pattern| pattern.trim().to_ascii_lowercase())
        .any(|pattern| !pattern.is_empty() && line.contains(&pattern))
}

fn privilege_prompt_text(prompt: &PrivilegePromptMatch) -> &str {
    match prompt {
        PrivilegePromptMatch::Sudo { prompt_text, .. }
        | PrivilegePromptMatch::Su { prompt_text, .. }
        | PrivilegePromptMatch::Custom { prompt_text, .. }
        | PrivilegePromptMatch::GenericPassword { prompt_text } => prompt_text,
    }
}

fn privilege_host_patterns_are_specific(patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        let pattern = pattern.trim();
        !pattern.is_empty() && pattern != "*"
    })
}

fn select_privilege_candidates(
    scope: &PrivilegeCredentialScope,
    prompt: &PrivilegePromptMatch,
) -> (String, Vec<SavedPrivilegeCredential>) {
    let exact = scope
        .exact
        .iter()
        .filter(|credential| privilege_credential_matches_prompt(credential, prompt))
        .cloned()
        .collect::<Vec<_>>();
    if !exact.is_empty() {
        return (
            scope
                .connection_id
                .clone()
                .unwrap_or_else(|| SSH_COMMON_PRIVILEGE_SCOPE_ID.to_string()),
            exact,
        );
    }

    let mut host_matched = Vec::new();
    let mut generic = Vec::new();
    for credential in &scope.common {
        if !privilege_credential_matches_prompt(credential, prompt) {
            continue;
        }
        if privilege_host_patterns_are_specific(&credential.host_patterns) {
            if privilege_host_scope_matches(&scope.host, &credential.host_patterns) {
                host_matched.push(credential.clone());
            }
        } else {
            generic.push(credential.clone());
        }
    }
    if !host_matched.is_empty() {
        return (SSH_COMMON_PRIVILEGE_SCOPE_ID.to_string(), host_matched);
    }
    if generic.is_empty() {
        // Keep the terminal's own scope even when nothing matched so prompt
        // state and picker invalidation remain tied to the active session.
        return (
            scope
                .connection_id
                .clone()
                .unwrap_or_else(|| SSH_COMMON_PRIVILEGE_SCOPE_ID.to_string()),
            generic,
        );
    }
    (SSH_COMMON_PRIVILEGE_SCOPE_ID.to_string(), generic)
}

fn build_privilege_prompt_helper_state_from_prompt(
    scope: PrivilegeCredentialScope,
    prompt: PrivilegePromptMatch,
) -> PrivilegePromptHelperState {
    let (connection_id, credentials) = select_privilege_candidates(&scope, &prompt);
    let matches = credentials
        .into_iter()
        .map(|credential| MatchedPrivilegeCredential {
            connection_id: credential.connection_id,
            credential_id: credential.id,
            label: credential.label,
        })
        .collect::<Vec<_>>();
    PrivilegePromptHelperState {
        connection_id,
        prompt,
        matches,
    }
}

fn privilege_prompt_state_allows_confirmed_fill(state: &PrivilegePromptHelperState) -> bool {
    // The UI confirmation boundary is the visible inline hint or the active
    // Enter press. A bare `Password:` prompt is fillable only after scoped
    // credential matching leaves one unambiguous candidate.
    state.matches.len() == 1
}

fn choose_privilege_prompt<'a>(
    credentials: impl IntoIterator<Item = &'a SavedPrivilegeCredential>,
    visible_text: &str,
    tracked_prompt: Option<PrivilegePromptMatch>,
) -> Option<PrivilegePromptMatch> {
    match tracked_prompt {
        Some(prompt @ (PrivilegePromptMatch::Sudo { .. } | PrivilegePromptMatch::Su { .. })) => {
            Some(prompt)
        }
        Some(prompt @ PrivilegePromptMatch::GenericPassword { .. }) => {
            detect_custom_prompt_from_credentials(credentials, visible_text).or(Some(prompt))
        }
        Some(prompt @ PrivilegePromptMatch::Custom { .. }) => Some(prompt),
        // Standard prompts arrive as session-owned semantic events. The
        // viewport fallback remains only for user-authored custom patterns,
        // which cannot be classified without credential configuration.
        None => detect_custom_prompt_from_credentials(credentials, visible_text),
    }
}

fn detect_custom_prompt_from_credentials<'a>(
    credentials: impl IntoIterator<Item = &'a SavedPrivilegeCredential>,
    visible_text: &str,
) -> Option<PrivilegePromptMatch> {
    credentials.into_iter().find_map(|credential| {
        if !credential.enabled || credential.prompt_type != PrivilegePromptType::Custom {
            return None;
        }
        // Custom privilege prompts are user-authored fragments. They must be
        // allowed to trigger even when the prompt is not a built-in `Password:`
        // shape; otherwise the "custom" prompt type behaves like a no-op.
        detect_custom_privilege_prompt(visible_text, &credential.id, &credential.prompt_patterns)
    })
}

fn terminal_broadcast_menu_left_for_trigger_right(trigger_right: f32) -> f32 {
    (trigger_right - TERMINAL_BROADCAST_MENU_WIDTH).max(12.0)
}

fn terminal_cwd_browse_element_id(path: &str) -> u64 {
    // GPUI ElementId supports numeric tuple keys here; hashing keeps path-based
    // row identity stable without forcing a String into the element id type.
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

fn terminal_project_git_root_disagreement(project_root: &str, git_root: &str) -> Option<String> {
    let project_root = project_root.trim();
    let git_root = git_root.trim();
    if project_root.is_empty() || git_root.is_empty() {
        return None;
    }

    // Project detection is manifest-based while Git detection is repository
    // based. Compare normalized display paths so nested packages surface their
    // distinct Git root without rewriting the project root.
    (terminal_display_path_key(project_root) != terminal_display_path_key(git_root))
        .then(|| git_root.to_string())
}

fn terminal_display_path_key(path: &str) -> String {
    let mut path = path.trim().replace('\\', "/");
    while path.len() > 1 && path.ends_with('/') {
        path.pop();
    }
    path
}

#[cfg(test)]
mod terminal_project_git_root_tests {
    use super::*;

    #[test]
    fn project_git_root_disagreement_compares_normalized_roots() {
        for (project, git, expected) in [
            ("/repo/app/", "/repo/app", None),
            ("/repo/app", "/repo", Some("/repo")),
        ] {
            assert_eq!(
                terminal_project_git_root_disagreement(project, git).as_deref(),
                expected
            );
        }
    }
}

#[cfg(test)]
mod privilege_prompt_helper_tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn second_factor_prompt_matches_only_the_latest_visible_line() {
        let text = "First Factor:\r\nSecond Factor:";
        let line = latest_visible_prompt_line(text).expect("latest line");
        assert_eq!(line, "Second Factor:");
        let patterns = vec!["Second Factor:".to_string()];
        assert!(privilege_line_matches_patterns(line, &patterns));
        assert!(!privilege_line_matches_patterns("First Factor:", &patterns));
    }

    #[test]
    fn ssh_privilege_scope_prefers_connection_then_host_group_then_generic() {
        let exact = saved_privilege_credential_for_connection(
            "conn-1",
            "exact",
            PrivilegePromptType::Sudo,
            None,
        );
        let mut group = saved_privilege_credential_for_connection(
            SSH_COMMON_PRIVILEGE_SCOPE_ID,
            "group",
            PrivilegePromptType::Sudo,
            None,
        );
        group.host_patterns = vec!["hktkosl*".into()];
        let mut other_group = group.clone();
        other_group.id = "other-group".into();
        other_group.host_patterns = vec!["i-*".into()];
        let mut generic = saved_privilege_credential_for_connection(
            SSH_COMMON_PRIVILEGE_SCOPE_ID,
            "generic",
            PrivilegePromptType::Sudo,
            None,
        );
        generic.host_patterns = vec!["*".into()];

        let prompt = PrivilegePromptMatch::Sudo {
            username: Some("deploy".into()),
            prompt_text: "[sudo] password for deploy:".into(),
        };

        let scope = PrivilegeCredentialScope {
            connection_id: Some("conn-1".into()),
            host: "hktkosl1102".into(),
            exact: vec![exact.clone()],
            common: vec![group.clone(), other_group.clone(), generic.clone()],
        };
        let (scope_id, selected) = select_privilege_candidates(&scope, &prompt);
        assert_eq!(scope_id, "conn-1");
        assert_eq!(
            selected.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["exact"]
        );

        let scope = PrivilegeCredentialScope {
            connection_id: None,
            host: "HKTKOSL1102".into(),
            exact: Vec::new(),
            common: vec![group.clone(), other_group.clone(), generic.clone()],
        };
        let (scope_id, selected) = select_privilege_candidates(&scope, &prompt);
        assert_eq!(scope_id, SSH_COMMON_PRIVILEGE_SCOPE_ID);
        assert_eq!(
            selected.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["group"]
        );

        let scope = PrivilegeCredentialScope {
            connection_id: Some("conn-1".into()),
            host: "unknown-host".into(),
            exact: Vec::new(),
            common: vec![group, other_group, generic.clone()],
        };
        let (scope_id, selected) = select_privilege_candidates(&scope, &prompt);
        assert_eq!(scope_id, SSH_COMMON_PRIVILEGE_SCOPE_ID);
        assert_eq!(
            selected.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["generic"]
        );
    }

    #[test]
    fn ssh_common_privilege_candidates_preserve_scope_and_specific_priority() {
        let common = saved_privilege_credential_for_connection(
            SSH_COMMON_PRIVILEGE_SCOPE_ID,
            "shared",
            PrivilegePromptType::Sudo,
            None,
        );
        let specific = saved_privilege_credential_for_connection(
            "conn-1",
            "shared",
            PrivilegePromptType::Sudo,
            Some("deploy"),
        );
        for (username, expected_scope) in [
            ("deploy", "conn-1"),
            ("other", SSH_COMMON_PRIVILEGE_SCOPE_ID),
        ] {
            let prompt = PrivilegePromptMatch::Sudo {
                username: Some(username.into()),
                prompt_text: format!("[sudo] password for {username}:"),
            };
            let scope = PrivilegeCredentialScope {
                connection_id: Some("conn-1".into()),
                host: "prod-01".into(),
                exact: vec![specific.clone()],
                common: vec![common.clone()],
            };
            let state =
                build_privilege_prompt_helper_state_with_tracked_prompt(scope, "", Some(prompt))
                    .unwrap();
            assert_eq!(
                state.matches,
                vec![MatchedPrivilegeCredential {
                    connection_id: expected_scope.into(),
                    credential_id: "shared".into(),
                    label: "shared".into()
                }]
            );
        }
        let mut first = saved_privilege_credential_for_connection(
            SSH_COMMON_PRIVILEGE_SCOPE_ID,
            "first",
            PrivilegePromptType::Custom,
            None,
        );
        first.prompt_patterns = vec!["approval token".into()];
        let mut second = first.clone();
        second.id = "second".into();
        let state = build_privilege_prompt_helper_state(
            "conn-1".into(),
            &[first, second],
            "Enter approval token >",
        )
        .unwrap();
        assert_eq!(
            state
                .matches
                .iter()
                .map(|candidate| candidate.credential_id.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert!(!privilege_prompt_state_allows_confirmed_fill(&state));
    }

    #[test]
    fn prompt_matching_precedes_scope_and_ignores_response_mode() {
        let mut exact_su = saved_privilege_credential("exact-su", PrivilegePromptType::Su, None);
        exact_su.response_mode = PrivilegeResponseMode::PasswordThenTotp;
        let mut disabled_exact =
            saved_privilege_credential("disabled-exact", PrivilegePromptType::Sudo, None);
        disabled_exact.enabled = false;
        let mut staged_sudo = saved_privilege_credential_for_connection(
            SSH_COMMON_PRIVILEGE_SCOPE_ID,
            "staged-sudo",
            PrivilegePromptType::Sudo,
            None,
        );
        staged_sudo.response_mode = PrivilegeResponseMode::PasswordThenTotp;
        let mut staged_custom = saved_privilege_credential_for_connection(
            SSH_COMMON_PRIVILEGE_SCOPE_ID,
            "staged-custom",
            PrivilegePromptType::Custom,
            None,
        );
        staged_custom.response_mode = PrivilegeResponseMode::PasswordThenTotp;
        staged_custom.prompt_patterns = vec!["[sudo]".into()];

        let scope = PrivilegeCredentialScope {
            connection_id: Some("conn-1".into()),
            host: "prod-01".into(),
            exact: vec![exact_su, disabled_exact],
            common: vec![staged_sudo, staged_custom],
        };
        let prompt = PrivilegePromptMatch::Sudo {
            username: Some("deploy".into()),
            prompt_text: "[sudo] password for deploy:".into(),
        };
        let state =
            build_privilege_prompt_helper_state_with_tracked_prompt(scope, "", Some(prompt))
                .unwrap();

        assert_eq!(
            state
                .matches
                .iter()
                .map(|candidate| candidate.credential_id.as_str())
                .collect::<Vec<_>>(),
            ["staged-sudo", "staged-custom"]
        );
    }

    #[test]
    fn privilege_picker_expires_when_target_prompt_or_candidates_change() {
        let state = build_privilege_prompt_helper_state(
            "conn-1".into(),
            &[saved_privilege_credential(
                "sudo",
                PrivilegePromptType::Sudo,
                None,
            )],
            "Password:",
        )
        .unwrap();
        let picker = PrivilegeCredentialPicker {
            pane_id: PaneId(1),
            session_id: TerminalSessionId(2),
            prompt_generation: 3,
            state: state.clone(),
            selected: 0,
        };
        assert!(picker.is_current(PaneId(1), TerminalSessionId(2), 3, &state));
        for (pane, session, generation) in [
            (PaneId(4), TerminalSessionId(2), 3),
            (PaneId(1), TerminalSessionId(4), 3),
            (PaneId(1), TerminalSessionId(2), 4),
        ] {
            assert!(!picker.is_current(pane, session, generation, &state));
        }
        let mut changed = state;
        changed.matches.clear();
        assert!(!picker.is_current(PaneId(1), TerminalSessionId(2), 3, &changed));
    }

    fn saved_privilege_credential_for_connection(
        connection_id: &str,
        id: &str,
        prompt_type: PrivilegePromptType,
        username_hint: Option<&str>,
    ) -> SavedPrivilegeCredential {
        let now = Utc::now();
        SavedPrivilegeCredential {
            id: id.to_string(),
            connection_id: connection_id.to_string(),
            label: id.to_string(),
            prompt_type,
            response_mode: PrivilegeResponseMode::PasswordOnly,
            username_hint: username_hint.map(str::to_string),
            prompt_patterns: Vec::new(),
            host_patterns: Vec::new(),
            second_factor_patterns: Vec::new(),
            second_factor_user_hint: None,
            totp_credential_id: None,
            keychain_id: Some(format!("privilege:v1:{connection_id}:{id}")),
            plaintext_secret: None,
            enabled: true,
            require_click_to_send: true,
            created_at: now,
            updated_at: now,
        }
    }

    fn saved_privilege_credential(
        id: &str,
        prompt_type: PrivilegePromptType,
        username_hint: Option<&str>,
    ) -> SavedPrivilegeCredential {
        saved_privilege_credential_for_connection("conn-1", id, prompt_type, username_hint)
    }

    fn local_privilege_credential(
        id: &str,
        prompt_type: PrivilegePromptType,
        username_hint: Option<&str>,
    ) -> SavedPrivilegeCredential {
        saved_privilege_credential_for_connection(
            LOCAL_SHELL_PRIVILEGE_CONNECTION_ID,
            id,
            prompt_type,
            username_hint,
        )
    }

    fn custom_privilege_credential(id: &str, patterns: &[&str]) -> SavedPrivilegeCredential {
        let mut credential = saved_privilege_credential(id, PrivilegePromptType::Custom, None);
        credential.prompt_patterns = patterns.iter().map(|pattern| pattern.to_string()).collect();
        credential
    }

    #[test]
    fn privilege_prompt_helper_is_limited_to_supported_terminal_tabs() {
        for (kind, allowed) in [
            (TabKind::LocalTerminal, true),
            (TabKind::SshTerminal, true),
            (TabKind::MoshTerminal, false),
            (TabKind::Sftp, false),
        ] {
            assert_eq!(tab_kind_allows_privilege_prompt_helper(&kind), allowed);
        }
    }

    #[test]
    fn ssh_privilege_scope_prefers_explicit_owner_and_uses_only_saved_origins() {
        let restored = NodeOrigin::Restored {
            saved_connection_id: "restored-conn".to_string(),
        };
        let manual_preset = NodeOrigin::ManualPreset {
            saved_connection_id: "jump-chain".to_string(),
            hop_index: 1,
        };

        let direct = NodeOrigin::Direct;
        let legacy_auto_route = NodeOrigin::AutoRoute {
            target_host: "db.internal".to_string(),
            route_id: "route-1".to_string(),
            hop_index: 0,
        };

        for (owner, origin, expected) in [
            (Some("node-owner"), Some(&restored), Some("node-owner")),
            (None, Some(&restored), Some("restored-conn")),
            (None, Some(&manual_preset), Some("jump-chain")),
            (None, Some(&direct), None),
            (None, Some(&legacy_auto_route), None),
            (None, None, None),
        ] {
            assert_eq!(
                saved_ssh_privilege_scope_id(owner, origin).as_deref(),
                expected,
                "owner={owner:?}, origin={origin:?}"
            );
        }
    }

    #[test]
    fn prompt_state_survives_without_loaded_credentials() {
        let state = build_privilege_prompt_helper_state(
            "conn-1".to_string(),
            &[],
            "sudo yazi\n[sudo] lipsc 的密码:",
        )
        .expect("localized sudo prompt should create a management state");

        assert_eq!(
            state,
            PrivilegePromptHelperState {
                connection_id: "conn-1".to_string(),
                prompt: PrivilegePromptMatch::Sudo {
                    username: Some("lipsc".to_string()),
                    prompt_text: "[sudo] lipsc 的密码:".to_string(),
                },
                matches: Vec::new(),
            }
        );
    }

    #[test]
    fn prompt_state_matches_enabled_username_hint() {
        let credentials = vec![
            saved_privilege_credential("other-sudo", PrivilegePromptType::Sudo, Some("other")),
            saved_privilege_credential("matching-sudo", PrivilegePromptType::Sudo, Some("lipsc")),
        ];
        let state = build_privilege_prompt_helper_state(
            "conn-1".to_string(),
            &credentials,
            "sudo yazi\n[sudo] lipsc 的密码:",
        )
        .expect("localized sudo prompt should create fill matches");

        assert_eq!(
            state.matches,
            vec![MatchedPrivilegeCredential {
                connection_id: "conn-1".to_string(),
                credential_id: "matching-sudo".to_string(),
                label: "matching-sudo".to_string(),
            }]
        );
    }

    #[test]
    fn generic_password_after_sudo_command_matches_sudo_credentials_only() {
        let credentials = vec![
            local_privilege_credential("local-sudo", PrivilegePromptType::Sudo, None),
            local_privilege_credential("local-su", PrivilegePromptType::Su, None),
        ];
        let state = build_privilege_prompt_helper_state(
            "local-shell:default".to_string(),
            &credentials,
            "❯ sudo yazi\nPassword:",
        )
        .expect("sudo command context should classify the generic password prompt");

        assert_eq!(
            state.matches,
            vec![MatchedPrivilegeCredential {
                connection_id: "local-shell:default".to_string(),
                credential_id: "local-sudo".to_string(),
                label: "local-sudo".to_string(),
            }]
        );
    }

    #[test]
    fn generic_password_after_su_command_matches_target_hint() {
        let credentials = vec![
            local_privilege_credential("root-su", PrivilegePromptType::Su, Some("root")),
            local_privilege_credential("postgres-su", PrivilegePromptType::Su, Some("postgres")),
        ];
        let state = build_privilege_prompt_helper_state(
            "local-shell:default".to_string(),
            &credentials,
            "su postgres\nPassword:",
        )
        .expect("su command context should classify the generic password prompt");

        assert_eq!(
            state.matches,
            vec![MatchedPrivilegeCredential {
                connection_id: "local-shell:default".to_string(),
                credential_id: "postgres-su".to_string(),
                label: "postgres-su".to_string(),
            }]
        );
    }

    #[test]
    fn single_generic_password_candidate_allows_confirmed_fill() {
        let credentials = vec![local_privilege_credential(
            "local-sudo",
            PrivilegePromptType::Sudo,
            None,
        )];
        let state = build_privilege_prompt_helper_state(
            "local-shell:default".to_string(),
            &credentials,
            "Password:",
        )
        .expect("bare macOS sudo prompt should create a scoped prompt state");

        assert!(matches!(
            state.prompt,
            PrivilegePromptMatch::GenericPassword { .. }
        ));
        assert!(privilege_prompt_state_allows_confirmed_fill(&state));
    }

    #[test]
    fn generic_password_prompt_offers_scoped_click_only_candidates() {
        let credentials = vec![
            local_privilege_credential("local-sudo", PrivilegePromptType::Sudo, Some("dominical")),
            local_privilege_credential("local-su", PrivilegePromptType::Su, None),
        ];
        let state = build_privilege_prompt_helper_state(
            "local-shell:default".to_string(),
            &credentials,
            "mysql login\nPassword:",
        )
        .expect("generic password prompt should create explicit-click matches");

        assert_eq!(
            state.matches,
            vec![
                MatchedPrivilegeCredential {
                    connection_id: "local-shell:default".to_string(),
                    credential_id: "local-sudo".to_string(),
                    label: "local-sudo".to_string(),
                },
                MatchedPrivilegeCredential {
                    connection_id: "local-shell:default".to_string(),
                    credential_id: "local-su".to_string(),
                    label: "local-su".to_string(),
                },
            ]
        );
        assert!(!privilege_prompt_state_allows_confirmed_fill(&state));
    }

    #[test]
    fn custom_prompt_patterns_create_prompt_state_without_password_label() {
        let credentials = vec![
            saved_privilege_credential("local-sudo", PrivilegePromptType::Sudo, None),
            custom_privilege_credential("deploy-token", &["approval token"]),
        ];
        let state = build_privilege_prompt_helper_state(
            "conn-1".to_string(),
            &credentials,
            "deploy-tool unlock\nEnter deployment approval token >",
        )
        .expect("custom privilege prompt should not depend on built-in password wording");

        assert_eq!(
            state.prompt,
            PrivilegePromptMatch::Custom {
                credential_id: "deploy-token".to_string(),
                prompt_text: "Enter deployment approval token >".to_string(),
            }
        );
        assert_eq!(
            state.matches,
            vec![MatchedPrivilegeCredential {
                connection_id: "conn-1".to_string(),
                credential_id: "deploy-token".to_string(),
                label: "deploy-token".to_string(),
            }]
        );
    }
}
