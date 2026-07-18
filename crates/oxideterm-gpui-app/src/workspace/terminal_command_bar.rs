use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Range;

use super::actions::TerminalBroadcastMenuPlacement;
use super::ime::WorkspaceImeTarget;
use super::terminal_git::{
    TerminalGitPanelSection, TerminalGitPathAction, TerminalGitRepositoryAction,
    terminal_git_path_action_label_key, terminal_git_repository_action_label_key,
};
use super::*;
use oxideterm_connections::LOCAL_SHELL_PRIVILEGE_CONNECTION_ID;
use oxideterm_environment::{
    CurrentDirectoryScope, CurrentDirectorySnapshot, CurrentDirectorySource, GitChangedPath,
    GitRepositoryStatus, ProjectSnapshot, ProjectTask, ProjectTaskGroup,
};
use oxideterm_gpui_ui::button::{ButtonRadius, IconButtonOptions};
use oxideterm_gpui_ui::context_menu::{
    ContextMenuActionableStyle, context_menu_event_boundary, context_menu_pointer_event_boundary,
};
use oxideterm_gpui_ui::modal::rounded_shell_child_radius;
use oxideterm_gpui_ui::text_input::{
    TextInputView, text_caret, text_input, text_input_anchor_probe,
    text_input_value_segments_with_color,
};
use oxideterm_gpui_ui::{
    ActionChipOptions, ActionChipTextTone, CommandPanelOptions, ContextChipOptions,
    EntityListRowOptions, MonospaceDatumOptions, MonospaceDatumTone, StatusPillOptions, StatusTone,
    action_chip, action_chip_foreground, command_panel, context_chip, entity_list_row,
    monospace_datum, status_pill,
};
use oxideterm_terminal_recording::format_recording_elapsed;

pub(in crate::workspace) mod completion;

mod bar;
mod context;
mod git;
mod privilege;

const TERMINAL_BROADCAST_MENU_WIDTH: f32 = 260.0;
const TERMINAL_CWD_MENU_WIDTH: f32 = 520.0;
const TERMINAL_CWD_MENU_MAX_HEIGHT: f32 = 420.0;
const TERMINAL_CWD_MENU_MARGIN: f32 = 12.0;
const TERMINAL_GIT_BRANCH_MENU_WIDTH: f32 = 720.0;
const TERMINAL_GIT_BRANCH_MENU_BODY_MAX_HEIGHT: f32 = 520.0;
const TERMINAL_GIT_BRANCH_MENU_MARGIN: f32 = 12.0;
const TERMINAL_PROJECT_MENU_WIDTH: f32 = 640.0;
const TERMINAL_PROJECT_MENU_BODY_MAX_HEIGHT: f32 = 420.0;
const TERMINAL_PROJECT_MENU_MARGIN: f32 = 12.0;
const TERMINAL_COMMAND_CONTEXT_CHIP_MAX_WIDTH: f32 = 260.0; // Keep context chips compact beside command-bar actions.
const TERMINAL_COMMAND_PROJECT_CHIP_MAX_WIDTH: f32 = 240.0; // Project labels are shorter than cwd/git labels in Tauri.
const TERMINAL_COMMAND_INPUT_LINE_HEIGHT: f32 = 20.0;
const TERMINAL_COMMAND_INPUT_MIN_HEIGHT: f32 = 24.0;
const TERMINAL_COMMAND_INPUT_MAX_VISIBLE_LINES: usize = 6;
const PRIVILEGE_PROMPT_DEBUG_ENV: &str = "OXIDETERM_PRIVILEGE_DEBUG";

#[derive(Clone, Copy, Debug)]
struct TerminalCommandInputLine<'a> {
    text: &'a str,
    utf16_start: usize,
    utf16_end: usize,
}

impl TerminalCommandInputLine<'_> {
    fn utf16_len(&self) -> usize {
        self.utf16_end.saturating_sub(self.utf16_start)
    }
}

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

fn tab_kind_allows_privilege_prompt_helper(tab_kind: &TabKind) -> bool {
    // Local shells use an app-level scope. SSH terminals are allowed only after
    // active_privilege_scope_credentials resolves the active terminal through
    // the node ownership maps, never through host/title/runtime heuristics.
    matches!(tab_kind, TabKind::LocalTerminal | TabKind::SshTerminal)
}

fn log_privilege_prompt_helper(args: std::fmt::Arguments<'_>) {
    if std::env::var_os(PRIVILEGE_PROMPT_DEBUG_ENV).is_some() {
        eprintln!("[oxideterm:privilege] {args}");
    }
}

fn terminal_command_input_lines(input: &str) -> Vec<TerminalCommandInputLine<'_>> {
    // Keep hard line breaks as the editing model so the command bar behaves
    // like a small textarea without introducing persistent scroll state.
    let mut lines = Vec::new();
    let mut byte_start = 0usize;
    let mut utf16_start = 0usize;
    let mut utf16_offset = 0usize;

    for (byte_index, ch) in input.char_indices() {
        if ch == '\n' {
            lines.push(TerminalCommandInputLine {
                text: &input[byte_start..byte_index],
                utf16_start,
                utf16_end: utf16_offset,
            });
            utf16_offset += ch.len_utf16();
            byte_start = byte_index + ch.len_utf8();
            utf16_start = utf16_offset;
        } else {
            utf16_offset += ch.len_utf16();
        }
    }

    lines.push(TerminalCommandInputLine {
        text: &input[byte_start..],
        utf16_start,
        utf16_end: utf16_offset,
    });
    lines
}

fn terminal_command_line_selection(
    line: TerminalCommandInputLine<'_>,
    selection: Option<&Range<usize>>,
) -> Option<Range<usize>> {
    selection.and_then(|selection| {
        let start = selection.start.max(line.utf16_start).min(line.utf16_end);
        let end = selection.end.max(line.utf16_start).min(line.utf16_end);
        (start < end).then_some(start - line.utf16_start..end - line.utf16_start)
    })
}

fn terminal_command_line_caret(
    line: TerminalCommandInputLine<'_>,
    caret_offset: Option<usize>,
) -> Option<usize> {
    caret_offset
        .filter(|offset| *offset >= line.utf16_start && *offset <= line.utf16_end)
        .map(|offset| {
            offset
                .saturating_sub(line.utf16_start)
                .min(line.utf16_len())
        })
}

fn terminal_command_placeholder_caret_visible(
    focused: bool,
    showing_placeholder: bool,
    line_index: usize,
) -> bool {
    // An empty textarea still owns a caret at its first visual position.
    focused && showing_placeholder && line_index == 0
}

fn terminal_git_section_icon(section: TerminalGitPanelSection) -> LucideIcon {
    match section {
        TerminalGitPanelSection::Branches => LucideIcon::GitFork,
        TerminalGitPanelSection::Changes => LucideIcon::Pencil,
        TerminalGitPanelSection::Sync => LucideIcon::RefreshCw,
        TerminalGitPanelSection::Stash => LucideIcon::Archive,
        TerminalGitPanelSection::Resolve => LucideIcon::AlertTriangle,
        TerminalGitPanelSection::Commit => LucideIcon::CheckCircle,
        TerminalGitPanelSection::History => LucideIcon::History,
        TerminalGitPanelSection::Refs => LucideIcon::ListTree,
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
    match prompt {
        PrivilegePromptMatch::Sudo { username, .. } => {
            if !matches!(
                credential.kind,
                PrivilegeCredentialKind::SudoPassword | PrivilegeCredentialKind::CustomPrompt
            ) {
                return false;
            }
            if credential.kind == PrivilegeCredentialKind::CustomPrompt {
                return privilege_prompt_matches_custom_patterns(
                    prompt,
                    &credential.prompt_patterns,
                );
            }
            username.as_ref().is_none_or(|prompt_username| {
                credential
                    .username_hint
                    .as_ref()
                    .is_none_or(|hint| prompt_username == hint)
            })
        }
        PrivilegePromptMatch::Su { target_user, .. } => {
            if !matches!(
                credential.kind,
                PrivilegeCredentialKind::SuPassword | PrivilegeCredentialKind::CustomPrompt
            ) {
                return false;
            }
            match credential.kind {
                PrivilegeCredentialKind::SuPassword => {
                    target_user.as_ref().is_none_or(|prompt_user| {
                        credential
                            .username_hint
                            .as_ref()
                            .is_none_or(|hint| prompt_user == hint)
                    })
                }
                PrivilegeCredentialKind::CustomPrompt => {
                    privilege_prompt_matches_custom_patterns(prompt, &credential.prompt_patterns)
                }
                PrivilegeCredentialKind::SudoPassword => false,
            }
        }
        PrivilegePromptMatch::Custom { credential_id, .. } => credential.id == *credential_id,
        PrivilegePromptMatch::GenericPassword { .. } => match credential.kind {
            // Bare `Password:` carries no reliable sudo/su identity. Offer only
            // scoped, click-to-send candidates and never infer a command kind.
            PrivilegeCredentialKind::SudoPassword | PrivilegeCredentialKind::SuPassword => true,
            PrivilegeCredentialKind::CustomPrompt => {
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

#[cfg(test)]
fn build_privilege_prompt_helper_state(
    connection_id: String,
    credentials: &[SavedPrivilegeCredential],
    visible_text: &str,
) -> Option<PrivilegePromptHelperState> {
    let prompt = choose_privilege_prompt(credentials, visible_text, None)?;
    build_privilege_prompt_helper_state_from_prompt(connection_id, credentials, prompt)
}

fn build_privilege_prompt_helper_state_with_tracked_prompt(
    connection_id: String,
    credentials: &[SavedPrivilegeCredential],
    visible_text: &str,
    tracked_prompt: Option<PrivilegePromptMatch>,
) -> Option<PrivilegePromptHelperState> {
    let prompt = choose_privilege_prompt(credentials, visible_text, tracked_prompt)?;
    build_privilege_prompt_helper_state_from_prompt(connection_id, credentials, prompt)
}

fn build_privilege_prompt_helper_state_from_prompt(
    connection_id: String,
    credentials: &[SavedPrivilegeCredential],
    prompt: PrivilegePromptMatch,
) -> Option<PrivilegePromptHelperState> {
    let matches = credentials
        .iter()
        .filter(|credential| privilege_credential_matches_prompt(credential, &prompt))
        .map(|credential| MatchedPrivilegeCredential {
            connection_id: connection_id.clone(),
            credential_id: credential.id.clone(),
            label: credential.label.clone(),
        })
        .collect();
    Some(PrivilegePromptHelperState {
        connection_id,
        prompt,
        matches,
    })
}

fn privilege_prompt_state_allows_confirmed_fill(state: &PrivilegePromptHelperState) -> bool {
    // The UI confirmation boundary is the visible inline hint or the active
    // Enter press. A bare `Password:` prompt is fillable only after scoped
    // credential matching leaves one unambiguous candidate.
    state.matches.len() == 1
}

fn choose_privilege_prompt(
    credentials: &[SavedPrivilegeCredential],
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
        None => detect_custom_prompt_from_credentials(credentials, visible_text)
            .or_else(|| detect_privilege_prompt(visible_text)),
    }
}

fn detect_custom_prompt_from_credentials(
    credentials: &[SavedPrivilegeCredential],
    visible_text: &str,
) -> Option<PrivilegePromptMatch> {
    credentials.iter().find_map(|credential| {
        if !credential.enabled || credential.kind != PrivilegeCredentialKind::CustomPrompt {
            return None;
        }
        // Custom privilege prompts are user-authored fragments. They must be
        // allowed to trigger even when the prompt is not a built-in `Password:`
        // shape; otherwise the "custom" kind silently behaves like a no-op.
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

fn terminal_cwd_entry_signature(entry: &terminal_cwd::TerminalCwdVisibleEntry) -> u64 {
    // Virtual list state is index-based, so rows need a stable content signature
    // when filtering or changing directories reshuffles the visible entries.
    let mut hasher = DefaultHasher::new();
    terminal_cwd_visible_entry_kind_signature(entry.kind).hash(&mut hasher);
    entry.name.hash(&mut hasher);
    entry.path.hash(&mut hasher);
    hasher.finish()
}

fn terminal_cwd_visible_entry_kind_signature(
    kind: terminal_cwd::TerminalCwdVisibleEntryKind,
) -> u8 {
    match kind {
        terminal_cwd::TerminalCwdVisibleEntryKind::Parent => 0,
        terminal_cwd::TerminalCwdVisibleEntryKind::Directory => 1,
        terminal_cwd::TerminalCwdVisibleEntryKind::File => 2,
        terminal_cwd::TerminalCwdVisibleEntryKind::TypedPath => 3,
    }
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
mod terminal_broadcast_menu_tests {
    use super::*;

    #[test]
    fn broadcast_menu_aligns_end_to_trigger_not_workspace_root() {
        assert_eq!(terminal_broadcast_menu_left_for_trigger_right(700.0), 440.0);
    }

    #[test]
    fn broadcast_menu_keeps_left_viewport_margin_when_trigger_is_narrow() {
        assert_eq!(terminal_broadcast_menu_left_for_trigger_right(120.0), 12.0);
    }
}

#[cfg(test)]
mod terminal_command_input_tests {
    use super::*;

    #[test]
    fn focused_empty_command_input_shows_caret_before_placeholder() {
        assert!(terminal_command_placeholder_caret_visible(true, true, 0));
        assert!(!terminal_command_placeholder_caret_visible(false, true, 0));
        assert!(!terminal_command_placeholder_caret_visible(true, false, 0));
    }
}

#[cfg(test)]
mod terminal_project_git_root_tests {
    use super::*;

    #[test]
    fn project_git_root_disagreement_ignores_trailing_separators() {
        assert_eq!(
            terminal_project_git_root_disagreement("/repo/app/", "/repo/app").as_deref(),
            None
        );
    }

    #[test]
    fn project_git_root_disagreement_reports_distinct_git_root() {
        assert_eq!(
            terminal_project_git_root_disagreement("/repo/app", "/repo").as_deref(),
            Some("/repo")
        );
    }
}

#[cfg(test)]
mod privilege_prompt_helper_tests {
    use super::*;
    use chrono::Utc;

    fn saved_privilege_credential_for_connection(
        connection_id: &str,
        id: &str,
        kind: PrivilegeCredentialKind,
        username_hint: Option<&str>,
    ) -> SavedPrivilegeCredential {
        let now = Utc::now();
        SavedPrivilegeCredential {
            id: id.to_string(),
            connection_id: connection_id.to_string(),
            label: id.to_string(),
            kind,
            username_hint: username_hint.map(str::to_string),
            prompt_patterns: Vec::new(),
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
        kind: PrivilegeCredentialKind,
        username_hint: Option<&str>,
    ) -> SavedPrivilegeCredential {
        saved_privilege_credential_for_connection("conn-1", id, kind, username_hint)
    }

    fn custom_privilege_credential(id: &str, patterns: &[&str]) -> SavedPrivilegeCredential {
        let mut credential =
            saved_privilege_credential(id, PrivilegeCredentialKind::CustomPrompt, None);
        credential.prompt_patterns = patterns.iter().map(|pattern| pattern.to_string()).collect();
        credential
    }

    #[test]
    fn local_terminal_prompt_helper_is_enabled() {
        assert!(tab_kind_allows_privilege_prompt_helper(
            &TabKind::LocalTerminal
        ));
    }

    #[test]
    fn ssh_terminal_prompt_helper_is_tab_eligible() {
        assert!(tab_kind_allows_privilege_prompt_helper(
            &TabKind::SshTerminal
        ));
    }

    #[test]
    fn ssh_privilege_scope_prefers_explicit_node_saved_owner() {
        let origin = NodeOrigin::Restored {
            saved_connection_id: "restored-conn".to_string(),
        };

        assert_eq!(
            saved_ssh_privilege_scope_id(Some("node-owner"), Some(&origin)).as_deref(),
            Some("node-owner")
        );
    }

    #[test]
    fn ssh_privilege_scope_uses_restored_or_manual_preset_origin() {
        let restored = NodeOrigin::Restored {
            saved_connection_id: "restored-conn".to_string(),
        };
        let manual_preset = NodeOrigin::ManualPreset {
            saved_connection_id: "jump-chain".to_string(),
            hop_index: 1,
        };

        assert_eq!(
            saved_ssh_privilege_scope_id(None, Some(&restored)).as_deref(),
            Some("restored-conn")
        );
        assert_eq!(
            saved_ssh_privilege_scope_id(None, Some(&manual_preset)).as_deref(),
            Some("jump-chain")
        );
    }

    #[test]
    fn ssh_privilege_scope_does_not_guess_unsaved_node_owner() {
        let direct = NodeOrigin::Direct;
        let legacy_auto_route = NodeOrigin::AutoRoute {
            target_host: "db.internal".to_string(),
            route_id: "route-1".to_string(),
            hop_index: 0,
        };

        assert_eq!(saved_ssh_privilege_scope_id(None, Some(&direct)), None);
        assert_eq!(
            saved_ssh_privilege_scope_id(None, Some(&legacy_auto_route)),
            None
        );
        assert_eq!(saved_ssh_privilege_scope_id(None, None), None);
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
            saved_privilege_credential(
                "other-sudo",
                PrivilegeCredentialKind::SudoPassword,
                Some("other"),
            ),
            saved_privilege_credential(
                "matching-sudo",
                PrivilegeCredentialKind::SudoPassword,
                Some("lipsc"),
            ),
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
            saved_privilege_credential("local-sudo", PrivilegeCredentialKind::SudoPassword, None),
            saved_privilege_credential("local-su", PrivilegeCredentialKind::SuPassword, None),
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
            saved_privilege_credential(
                "root-su",
                PrivilegeCredentialKind::SuPassword,
                Some("root"),
            ),
            saved_privilege_credential(
                "postgres-su",
                PrivilegeCredentialKind::SuPassword,
                Some("postgres"),
            ),
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
        let credentials = vec![saved_privilege_credential(
            "local-sudo",
            PrivilegeCredentialKind::SudoPassword,
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
            saved_privilege_credential(
                "local-sudo",
                PrivilegeCredentialKind::SudoPassword,
                Some("dominical"),
            ),
            saved_privilege_credential("local-su", PrivilegeCredentialKind::SuPassword, None),
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
            saved_privilege_credential("local-sudo", PrivilegeCredentialKind::SudoPassword, None),
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
