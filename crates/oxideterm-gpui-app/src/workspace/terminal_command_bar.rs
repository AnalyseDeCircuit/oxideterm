use super::actions::TerminalBroadcastMenuPlacement;
use super::ime::WorkspaceImeTarget;
use super::terminal_git::{
    TerminalGitPanelSection, TerminalGitPathAction, TerminalGitRepositoryAction,
};
use super::*;
use oxideterm_connections::LOCAL_SHELL_PRIVILEGE_CONNECTION_ID;
use oxideterm_environment::{CurrentDirectorySnapshot, GitChangedPath, GitRepositoryStatus};
use oxideterm_gpui_ui::button::{ButtonRadius, IconButtonOptions};
use oxideterm_gpui_ui::context_menu::{
    ContextMenuActionableStyle, context_menu_event_boundary, context_menu_pointer_event_boundary,
};
use oxideterm_gpui_ui::modal::rounded_shell_child_radius;
use oxideterm_gpui_ui::text_input::{
    TextInputView, text_caret, text_input, text_input_anchor_probe,
    text_input_value_segments_with_color,
};
use oxideterm_terminal_recording::format_recording_elapsed;

pub(in crate::workspace) mod completion;

const TERMINAL_BROADCAST_MENU_WIDTH: f32 = 260.0;
const TERMINAL_CWD_MENU_WIDTH: f32 = 520.0;
const TERMINAL_CWD_MENU_MAX_HEIGHT: f32 = 420.0;
const TERMINAL_CWD_MENU_MARGIN: f32 = 12.0;
const TERMINAL_GIT_BRANCH_MENU_WIDTH: f32 = 720.0;
const TERMINAL_GIT_BRANCH_MENU_BODY_MAX_HEIGHT: f32 = 520.0;
const TERMINAL_GIT_BRANCH_MENU_MARGIN: f32 = 12.0;
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

impl WorkspaceApp {
    fn terminal_command_action_button(
        &self,
        icon: LucideIcon,
        icon_color: Rgba,
        disabled: bool,
        background: Option<Rgba>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Tauri TerminalCommandBarActions uses a shared h-6/w-6 rounded-md
        // button for split, broadcast, recording, and cast controls. Keep the
        // geometry local to the terminal bar while routing activation through
        // the workspace button guard shared with FileManager/SFTP actions.
        self.workspace_icon_action_button(
            icon,
            14.0,
            icon_color,
            IconButtonOptions {
                disabled,
                background,
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(24.0, ButtonRadius::Md)
            },
            listener,
            cx,
        )
    }

    fn render_terminal_target_indicator(
        &self,
        target_label: String,
        is_local_terminal: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let tooltip_id = "terminal-command-target-indicator";
        let icon = if is_local_terminal {
            LucideIcon::Terminal
        } else {
            LucideIcon::Server
        };

        div()
            .h(px(20.0))
            .min_w(px(28.0))
            .flex_none()
            .px(px(6.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x66))
            .bg(rgba((theme.bg_hover << 8) | 0x4d))
            .text_color(rgb(theme.text_muted))
            .id(tooltip_id)
            .on_mouse_move({
                let title = target_label.clone();
                cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                    this.queue_workspace_tooltip(
                        tooltip_id,
                        title.clone(),
                        f32::from(event.position.x) + 12.0,
                        f32::from(event.position.y) + 16.0,
                        cx,
                    );
                })
            })
            .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                if !*hovered {
                    this.clear_workspace_tooltip(tooltip_id, cx);
                }
            }))
            .child(Self::render_lucide_icon(icon, 12.0, rgb(theme.text_muted)))
            .into_any_element()
    }

    fn render_terminal_cwd_chip(
        &self,
        snapshot: Option<CurrentDirectorySnapshot>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.terminal_cwd_picker.open;
        let workspace = cx.entity();
        let path = snapshot
            .as_ref()
            .map(|snapshot| snapshot.path().to_string())
            .unwrap_or_else(|| "...".to_string());
        select_anchor_probe(
            SelectAnchorId::TerminalCwdMenu,
            div()
                .h(px(20.0))
                .max_w(px(260.0))
                .flex_none()
                .px(px(6.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(if active {
                    rgba((theme.accent << 8) | 0x99)
                } else {
                    rgba((theme.border << 8) | 0x80)
                })
                .bg(if active {
                    rgba((theme.accent << 8) | 0x1f)
                } else {
                    rgba((theme.bg_hover << 8) | 0x66)
                })
                .text_size(px(11.0))
                .font_family(settings_mono_font_family(self.settings_store.settings()))
                .text_color(if active {
                    rgb(theme.accent)
                } else {
                    rgb(theme.text)
                })
                .cursor_pointer()
                .hover(move |style| style.bg(rgb(theme.bg_hover)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        if this.terminal_cwd_picker.open {
                            this.close_terminal_cwd_picker();
                        } else {
                            this.open_terminal_cwd_picker(cx);
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(Self::render_lucide_icon(
                    LucideIcon::Folder,
                    12.0,
                    if active {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.text_muted)
                    },
                ))
                .child(div().min_w(px(0.0)).truncate().child(path)),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn render_terminal_cwd_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let left = self.terminal_cwd_picker_left();
        let bottom = if self.terminal_command_input_collapsed {
            32.0
        } else {
            64.0
        };
        let mut panel = context_menu_pointer_event_boundary(
            div()
                .absolute()
                .bottom(px(bottom))
                .left(px(left))
                .w(px(TERMINAL_CWD_MENU_WIDTH))
                .max_w(relative(0.96))
                .overflow_hidden()
                .rounded(px(self.tokens.radii.lg))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgba((theme.bg_elevated << 8) | 0xf5))
                .shadow_lg()
                .occlude()
                .p(px(8.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .text_size(px(12.0))
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
                    cx.stop_propagation();
                }),
        )
        .child(self.render_terminal_cwd_search(cx));

        if let Some(path) = self.terminal_cwd_browse_path() {
            panel = panel.child(self.render_terminal_cwd_context_row(path.to_string()));
        }

        let body = if self.terminal_cwd_picker.loading {
            self.render_terminal_cwd_message(
                LucideIcon::LoaderCircle,
                self.i18n.t("terminal.cwd.loading"),
            )
        } else if let Some(error) = self.terminal_cwd_picker.error.clone() {
            self.render_terminal_cwd_message(LucideIcon::AlertCircle, error)
        } else {
            let visible = self.visible_terminal_cwd_entries();
            if visible.is_empty() {
                self.render_terminal_cwd_message(
                    LucideIcon::Search,
                    self.i18n.t("terminal.cwd.no_directories"),
                )
            } else {
                let mut list = div().flex().flex_col().gap(px(2.0));
                for entry in visible {
                    list = list.child(self.render_terminal_cwd_entry_row(entry, cx));
                }
                div()
                    .min_h(px(0.0))
                    .max_h(px(TERMINAL_CWD_MENU_MAX_HEIGHT))
                    .overflow_y_scrollbar()
                    .child(list)
                    .into_any_element()
            }
        };

        panel.child(body).into_any_element()
    }

    fn render_terminal_cwd_context_row(&self, path: String) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x66))
            .bg(rgba((theme.bg_panel << 8) | 0x70))
            .px(px(10.0))
            .py(px(6.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(Self::render_lucide_icon(
                LucideIcon::FolderOpen,
                14.0,
                rgb(theme.text_muted),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_color(rgb(theme.text))
                    .child(path),
            )
            .into_any_element()
    }

    fn render_terminal_cwd_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::TerminalCwdSearch;
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &self.terminal_cwd_picker.query,
                    placeholder: self.i18n.t("terminal.cwd.search_directories"),
                    focused: self.terminal_cwd_picker.open,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .h(px(32.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle);
                    this.ime_marked_text = None;
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(
                |this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                },
            )),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn render_terminal_cwd_entry_row(
        &self,
        entry: terminal_cwd::TerminalCwdVisibleEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.terminal_cwd_picker.highlighted_path.as_deref() == Some(&entry.path);
        let (icon, label, accent) = match entry.kind {
            terminal_cwd::TerminalCwdVisibleEntryKind::Parent => (
                LucideIcon::ArrowUp,
                self.i18n.t("terminal.cwd.parent_directory"),
                rgb(theme.text_muted),
            ),
            terminal_cwd::TerminalCwdVisibleEntryKind::Directory => {
                (LucideIcon::Folder, entry.name.clone(), rgb(theme.text))
            }
            terminal_cwd::TerminalCwdVisibleEntryKind::TypedPath => (
                LucideIcon::CornerDownLeft,
                self.i18n.t("terminal.cwd.go_to_path"),
                rgb(theme.accent),
            ),
        };
        let path = entry.path.clone();
        let browse_path = path.clone();
        let browse_tooltip_id = format!("terminal-cwd-enter-{browse_path}");
        let browse_tooltip_label = self.i18n.t("terminal.cwd.enter_directory");
        let can_browse = matches!(
            entry.kind,
            terminal_cwd::TerminalCwdVisibleEntryKind::Parent
                | terminal_cwd::TerminalCwdVisibleEntryKind::Directory
        );
        div()
            .min_h(px(34.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .bg(if active {
                rgba((theme.accent << 8) | 0x26)
            } else {
                rgba(0x00000000)
            })
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(theme.bg_hover)))
            .on_mouse_move(cx.listener({
                let path = path.clone();
                move |this, _event: &gpui::MouseMoveEvent, _window, cx| {
                    if this.terminal_cwd_picker.highlighted_path.as_deref() != Some(&path) {
                        this.terminal_cwd_picker.highlighted_path = Some(path.clone());
                        cx.notify();
                    }
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                    this.terminal_cwd_picker.highlighted_path = Some(path.clone());
                    if event.click_count >= 2 {
                        this.select_terminal_cwd_path(path.clone(), cx);
                    } else {
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            )
            .child(Self::render_lucide_icon(icon, 13.0, accent))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .truncate()
                            .text_color(if active { rgb(theme.accent) } else { accent })
                            .child(label),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(10.0))
                            .font_family(settings_mono_font_family(self.settings_store.settings()))
                            .text_color(rgb(theme.text_muted))
                            .child(entry.path),
                    ),
            )
            .when(can_browse, |row| {
                row.child(
                    div()
                        .size(px(24.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(self.tokens.radii.md))
                        .text_color(rgb(theme.text_muted))
                        .id(browse_tooltip_id.clone())
                        .hover(move |style| style.bg(rgba((theme.bg_hover << 8) | 0xb3)))
                        .on_mouse_move({
                            let tooltip_id = browse_tooltip_id.clone();
                            let tooltip_label = browse_tooltip_label.clone();
                            cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                                this.queue_workspace_tooltip(
                                    tooltip_id.clone(),
                                    tooltip_label.clone(),
                                    f32::from(event.position.x) + 12.0,
                                    f32::from(event.position.y) + 16.0,
                                    cx,
                                );
                            })
                        })
                        .on_hover(cx.listener({
                            let tooltip_id = browse_tooltip_id.clone();
                            move |this, hovered: &bool, _window, cx| {
                                if !*hovered {
                                    this.clear_workspace_tooltip(&tooltip_id, cx);
                                }
                            }
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.enter_terminal_cwd_directory(browse_path.clone(), cx);
                                this.clear_workspace_tooltip(&browse_tooltip_id, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .child(Self::render_lucide_icon(
                            LucideIcon::ChevronRight,
                            13.0,
                            rgb(theme.text_muted),
                        )),
                )
            })
            .into_any_element()
    }

    fn render_terminal_cwd_message(&self, icon: LucideIcon, message: String) -> AnyElement {
        div()
            .min_h(px(72.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
            .bg(rgba((self.tokens.ui.bg_panel << 8) | 0x4d))
            .p(px(10.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text_muted),
            ))
            .child(message)
            .into_any_element()
    }

    fn render_terminal_git_chip(
        &self,
        snapshot: oxideterm_environment::GitRepositorySnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = if snapshot.branch.is_detached() {
            format!("detached {}", snapshot.branch.display_text())
        } else {
            snapshot.branch.display_text().to_string()
        };
        let status = &snapshot.status;
        let ahead = status.ahead();
        let behind = status.behind();
        let conflicts = status.conflicts();
        let changed = status
            .staged()
            .saturating_add(status.modified())
            .saturating_add(status.untracked());
        let workspace = cx.entity();
        let active = self.terminal_git_branch_picker.open;

        select_anchor_probe(
            SelectAnchorId::TerminalGitBranchMenu,
            div()
                .h(px(20.0))
                .max_w(px(260.0))
                .flex_none()
                .px(px(6.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(if active {
                    rgba((self.tokens.ui.accent << 8) | 0x99)
                } else {
                    rgba(0x22c55e4d)
                })
                .bg(if active {
                    rgba((self.tokens.ui.accent << 8) | 0x1f)
                } else {
                    rgba(0x22c55e1a)
                })
                .text_size(px(11.0))
                .text_color(if active {
                    rgb(self.tokens.ui.accent)
                } else {
                    rgba(0x86efacff)
                })
                .cursor_pointer()
                .hover(|style| style.bg(rgba(0x22c55e26)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        if this.terminal_git_branch_picker.open {
                            this.close_terminal_git_branch_picker();
                            cx.notify();
                        } else {
                            this.open_terminal_git_branch_picker(cx);
                        }
                        cx.stop_propagation();
                    }),
                )
                .child(Self::render_lucide_icon(
                    LucideIcon::GitFork,
                    12.0,
                    if active {
                        rgb(self.tokens.ui.accent)
                    } else {
                        rgba(0x86efacff)
                    },
                ))
                .child(div().min_w(px(0.0)).truncate().child(label))
                .when(ahead > 0, |chip| {
                    chip.child(self.render_terminal_git_status_badge(
                        LucideIcon::ArrowUp,
                        ahead,
                        rgba(0x86efacff),
                    ))
                })
                .when(behind > 0, |chip| {
                    chip.child(self.render_terminal_git_status_badge(
                        LucideIcon::ArrowDown,
                        behind,
                        rgba(0x67e8f9ff),
                    ))
                })
                .when(changed > 0, |chip| {
                    chip.child(self.render_terminal_git_status_badge(
                        LucideIcon::Pencil,
                        changed,
                        rgba(0xfbbf24ff),
                    ))
                })
                .when(conflicts > 0, |chip| {
                    chip.child(self.render_terminal_git_status_badge(
                        LucideIcon::AlertTriangle,
                        conflicts,
                        rgba(0xf87171ff),
                    ))
                }),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn render_terminal_git_status_badge(
        &self,
        icon: LucideIcon,
        count: u32,
        color: Rgba,
    ) -> gpui::Div {
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.0))
            .text_color(color)
            .child(Self::render_lucide_icon(icon, 10.0, color))
            .child(count.to_string())
    }

    fn render_terminal_git_branch_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let left = self.terminal_git_branch_picker_left();
        let bottom = if self.terminal_command_input_collapsed {
            32.0
        } else {
            64.0
        };
        let snapshot = self.active_terminal_git_snapshot(cx);
        let operation = snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.status.operation());
        let active_section = match (self.terminal_git_branch_picker.active_section, operation) {
            (TerminalGitPanelSection::Resolve, None) => TerminalGitPanelSection::Changes,
            (section, _) => section,
        };

        let mut panel = context_menu_pointer_event_boundary(
            div()
                .absolute()
                .bottom(px(bottom))
                .left(px(left))
                .w(px(TERMINAL_GIT_BRANCH_MENU_WIDTH))
                .max_w(relative(0.96))
                .overflow_hidden()
                .rounded(px(self.tokens.radii.lg))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgba((theme.bg_elevated << 8) | 0xf5))
                .shadow_lg()
                .occlude()
                .p(px(8.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .text_size(px(12.0))
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
                    cx.stop_propagation();
                }),
        );

        if let Some(snapshot) = snapshot.clone() {
            panel = panel.child(self.render_terminal_git_context_header(snapshot));
        }
        panel = panel.child(self.render_terminal_git_section_tabs(active_section, operation, cx));

        let section = match active_section {
            TerminalGitPanelSection::Branches => self.render_terminal_git_branches_section(cx),
            TerminalGitPanelSection::Changes => self.render_terminal_git_changes_section(cx),
            TerminalGitPanelSection::Commit => self.render_terminal_git_commit_section(cx),
            TerminalGitPanelSection::Sync => self.render_terminal_git_sync_section(cx),
            TerminalGitPanelSection::Stash => self.render_terminal_git_stash_section(cx),
            TerminalGitPanelSection::History => self.render_terminal_git_history_section(cx),
            TerminalGitPanelSection::Refs => self.render_terminal_git_refs_section(cx),
            TerminalGitPanelSection::Resolve => {
                self.render_terminal_git_resolve_section(operation, cx)
            }
        };
        panel = panel.child(
            div()
                .min_h(px(0.0))
                .max_h(px(TERMINAL_GIT_BRANCH_MENU_BODY_MAX_HEIGHT))
                .overflow_y_scrollbar()
                .child(section),
        );

        panel.into_any_element()
    }

    fn render_terminal_git_context_header(
        &self,
        snapshot: oxideterm_environment::GitRepositorySnapshot,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let branch_label = if snapshot.branch.is_detached() {
            format!("detached {}", snapshot.branch.display_text())
        } else {
            snapshot.branch.display_text().to_string()
        };
        let repo_root = snapshot.repo_root;
        let status = snapshot.status;
        let mut metrics = div()
            .flex_none()
            .max_w(px(320.0))
            .min_w(px(0.0))
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0));

        // The header is a probe result, not a command preview. It keeps the
        // popover tied to the active terminal/worktree before any action runs.
        if let Some(upstream) = status.upstream() {
            metrics = metrics
                .child(self.render_terminal_git_data_hint_with_width(upstream.to_string(), 260.0));
        }
        if status.ahead() > 0 {
            metrics = metrics.child(self.render_terminal_git_icon_count_chip(
                LucideIcon::ArrowUp,
                status.ahead(),
                rgba(0x86efacff),
            ));
        }
        if status.behind() > 0 {
            metrics = metrics.child(self.render_terminal_git_icon_count_chip(
                LucideIcon::ArrowDown,
                status.behind(),
                rgba(0x67e8f9ff),
            ));
        }
        if status.staged() > 0 {
            metrics = metrics.child(self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_staged",
                status.staged(),
                rgba(0x86efacff),
            ));
        }
        if status.modified() > 0 {
            metrics = metrics.child(self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_modified",
                status.modified(),
                rgba(0xfbbf24ff),
            ));
        }
        if status.untracked() > 0 {
            metrics = metrics.child(self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_untracked",
                status.untracked(),
                rgba(0x67e8f9ff),
            ));
        }
        if status.conflicts() > 0 {
            metrics = metrics.child(self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_conflict",
                status.conflicts(),
                rgba(0xf87171ff),
            ));
        }

        div()
            .min_h(px(42.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x66))
            .bg(rgba((theme.bg_panel << 8) | 0x70))
            .px(px(10.0))
            .py(px(6.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(Self::render_lucide_icon(
                LucideIcon::FolderOpen,
                14.0,
                rgb(theme.text_muted),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.0))
                            .font_family(self.terminal_git_mono_font())
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(repo_root),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.0))
                            .font_family(self.terminal_git_mono_font())
                            .text_color(rgb(theme.accent))
                            .child(branch_label),
                    ),
            )
            .child(metrics)
            .into_any_element()
    }

    fn render_terminal_git_section_tabs(
        &self,
        active_section: TerminalGitPanelSection,
        operation: Option<oxideterm_environment::GitOperationKind>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sections = [
            TerminalGitPanelSection::Branches,
            TerminalGitPanelSection::Changes,
            TerminalGitPanelSection::Commit,
            TerminalGitPanelSection::Sync,
            TerminalGitPanelSection::Stash,
            TerminalGitPanelSection::History,
            TerminalGitPanelSection::Refs,
        ];
        let mut tabs = div().flex().items_center().gap(px(6.0));
        for section in sections {
            tabs = tabs.child(self.render_terminal_git_section_tab(section, active_section, cx));
        }
        if operation.is_some() {
            tabs = tabs.child(self.render_terminal_git_section_tab(
                TerminalGitPanelSection::Resolve,
                active_section,
                cx,
            ));
        }
        tabs.into_any_element()
    }

    fn render_terminal_git_section_tab(
        &self,
        section: TerminalGitPanelSection,
        active_section: TerminalGitPanelSection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = section == active_section;
        let label = self.i18n.t(section.label_key());
        let icon = terminal_git_section_icon(section);
        div()
            .flex_none()
            .h(px(28.0))
            .px(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if active {
                rgba((theme.accent << 8) | 0x88)
            } else {
                rgba((theme.border << 8) | 0x66)
            })
            .bg(if active {
                rgba((theme.accent << 8) | 0x20)
            } else {
                rgba(0x00000000)
            })
            .flex()
            .items_center()
            .gap(px(6.0))
            .whitespace_nowrap()
            .text_color(if active {
                rgb(theme.accent)
            } else {
                rgb(theme.text_muted)
            })
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(theme.bg_hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.terminal_git_branch_picker.active_section = section;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(Self::render_lucide_icon(
                icon,
                12.0,
                if active {
                    rgb(theme.accent)
                } else {
                    rgb(theme.text_muted)
                },
            ))
            .child(label)
            .into_any_element()
    }

    fn render_terminal_git_branches_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let visible_branches = self.visible_terminal_git_branches();
        let query_checkout_candidate = self.terminal_git_query_checkout_candidate();
        let query_create_candidate = self.terminal_git_query_create_branch_candidate();
        let query_remote_tracking_candidate = self.terminal_git_query_remote_tracking_candidate();
        let query_rebase_candidate = self.terminal_git_query_rebase_candidate(cx);
        let loading = self.terminal_git_branch_picker.loading;
        let error = self.terminal_git_branch_picker.error.clone();

        // Branch search owns branch keyboard navigation; other sections only run explicit actions.
        let mut section = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(self.render_terminal_git_branch_search(cx));

        if loading {
            section = section.child(self.render_terminal_git_branch_message(
                LucideIcon::LoaderCircle,
                self.i18n.t("terminal.git.loading_branches"),
            ));
        } else if let Some(error) = error {
            section = section.child(self.render_terminal_git_branch_error(error));
        } else {
            let has_visible_branches = !visible_branches.is_empty();
            if let Some(branch_name) = query_checkout_candidate.clone() {
                section =
                    section.child(self.render_terminal_git_query_checkout_row(branch_name, cx));
            }
            if let Some(branch_name) = query_create_candidate.clone() {
                section = section.child(
                    self.render_terminal_git_query_create_branch_row(branch_name.clone(), cx),
                );
                section = section
                    .child(self.render_terminal_git_query_rename_branch_row(branch_name, cx));
            }
            if let Some(branch_name) = query_remote_tracking_candidate.clone() {
                section =
                    section.child(self.render_terminal_git_query_track_remote_row(branch_name, cx));
            }
            if let Some(branch_name) = query_rebase_candidate.clone() {
                section = section.child(self.render_terminal_git_query_rebase_row(branch_name, cx));
            }
            if !has_visible_branches && query_checkout_candidate.is_none() {
                section = section.child(self.render_terminal_git_branch_message(
                    LucideIcon::Search,
                    self.i18n.t("terminal.git.no_branches"),
                ));
            }
            let mut list = div()
                .max_h(px(280.0))
                .min_h(px(0.0))
                .overflow_y_scrollbar()
                .flex()
                .flex_col()
                .gap(px(2.0));
            for branch in visible_branches {
                list = list.child(self.render_terminal_git_branch_row(branch, cx));
            }
            if has_visible_branches {
                section = section.child(list);
            }
        }

        section.into_any_element()
    }

    fn render_terminal_git_changes_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let changed_paths = self
            .active_terminal_git_snapshot(cx)
            .map(|snapshot| snapshot.status.paths().to_vec())
            .unwrap_or_default();
        let mut section = div().flex().flex_col().gap(px(8.0));

        if changed_paths.is_empty() {
            section = section.child(self.render_terminal_git_clean_changes_state());
        } else {
            section = section.child(self.render_terminal_git_path_list(changed_paths, cx));
        }

        section = section.child(self.render_terminal_git_action_toolbar(
            &[
                TerminalGitRepositoryAction::StageAll,
                TerminalGitRepositoryAction::UnstageAll,
                TerminalGitRepositoryAction::Diff,
                TerminalGitRepositoryAction::DiffStaged,
                TerminalGitRepositoryAction::Status,
            ],
            cx,
        ));
        section.into_any_element()
    }

    fn render_terminal_git_clean_changes_state(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .min_h(px(96.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x66))
            .bg(rgba((theme.bg_panel << 8) | 0x4d))
            .p(px(12.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .child(Self::render_lucide_icon(
                LucideIcon::CheckCircle,
                18.0,
                rgba(0x86efacff),
            ))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(self.i18n.t("terminal.git.clean_title")),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("terminal.git.clean_description")),
            )
            .into_any_element()
    }

    fn render_terminal_git_action_toolbar(
        &self,
        actions: &[TerminalGitRepositoryAction],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut toolbar = div()
            .min_h(px(36.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x66))
            .p(px(4.0))
            .overflow_x_scrollbar()
            .on_scroll_wheel(|_, _, cx| {
                cx.stop_propagation();
            })
            .flex()
            .items_center()
            .gap(px(6.0));

        // Keep action buttons in one row; localized labels may overflow
        // horizontally, but should never wrap into a second visual row.
        for action in actions {
            toolbar = toolbar.child(self.render_terminal_git_toolbar_action_button(*action, cx));
        }
        toolbar.into_any_element()
    }

    fn render_terminal_git_toolbar_action_button(
        &self,
        action: TerminalGitRepositoryAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self.i18n.t(action.label_key());
        let icon = terminal_git_action_icon(action);
        div()
            .flex_none()
            .h(px(28.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x66))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .whitespace_nowrap()
            .text_size(px(11.0))
            .text_color(rgb(theme.text))
            .cursor_pointer()
            .hover(move |style| {
                style
                    .bg(rgb(theme.bg_hover))
                    .border_color(rgba((theme.accent << 8) | 0x66))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.run_terminal_git_repository_action(action, cx);
                    cx.stop_propagation();
                }),
            )
            .child(Self::render_lucide_icon(icon, 12.0, rgb(theme.text_muted)))
            .child(label)
            .into_any_element()
    }

    fn render_terminal_git_commit_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div().flex().flex_col().gap(px(2.0));
        list = list.child(self.render_terminal_git_ai_commit_action_row(cx));
        for action in [
            TerminalGitRepositoryAction::CommitVerbose,
            TerminalGitRepositoryAction::Commit,
            TerminalGitRepositoryAction::CommitSignoff,
            TerminalGitRepositoryAction::Amend,
            TerminalGitRepositoryAction::AmendNoEdit,
        ] {
            list = list.child(self.render_terminal_git_action_row(action, cx));
        }
        self.render_terminal_git_action_panel(list)
    }

    fn render_terminal_git_sync_section(&self, cx: &mut Context<Self>) -> AnyElement {
        self.render_terminal_git_action_section(
            &[
                TerminalGitRepositoryAction::Fetch,
                TerminalGitRepositoryAction::FetchAll,
                TerminalGitRepositoryAction::Pull,
                TerminalGitRepositoryAction::RebasePull,
                TerminalGitRepositoryAction::RebaseInteractive,
                TerminalGitRepositoryAction::Push,
                TerminalGitRepositoryAction::Publish,
                TerminalGitRepositoryAction::PushTags,
            ],
            cx,
        )
    }

    fn render_terminal_git_stash_section(&self, cx: &mut Context<Self>) -> AnyElement {
        self.render_terminal_git_action_section(
            &[
                TerminalGitRepositoryAction::Stash,
                TerminalGitRepositoryAction::StashList,
                TerminalGitRepositoryAction::StashShowLatest,
                TerminalGitRepositoryAction::StashPop,
                TerminalGitRepositoryAction::StashApplyLatest,
                TerminalGitRepositoryAction::StashDropLatest,
            ],
            cx,
        )
    }

    fn render_terminal_git_history_section(&self, cx: &mut Context<Self>) -> AnyElement {
        self.render_terminal_git_action_section(
            &[
                TerminalGitRepositoryAction::Log,
                TerminalGitRepositoryAction::LogStat,
                TerminalGitRepositoryAction::Reflog,
            ],
            cx,
        )
    }

    fn render_terminal_git_refs_section(&self, cx: &mut Context<Self>) -> AnyElement {
        self.render_terminal_git_action_section(
            &[
                TerminalGitRepositoryAction::BranchVerbose,
                TerminalGitRepositoryAction::RemoteList,
                TerminalGitRepositoryAction::TagList,
                TerminalGitRepositoryAction::WorktreeList,
            ],
            cx,
        )
    }

    fn render_terminal_git_resolve_section(
        &self,
        operation: Option<oxideterm_environment::GitOperationKind>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(operation) = operation else {
            return self.render_terminal_git_changes_section(cx);
        };
        let mut actions = vec![
            TerminalGitRepositoryAction::ConflictFiles,
            TerminalGitRepositoryAction::Continue(operation),
            TerminalGitRepositoryAction::Abort(operation),
        ];
        if operation != oxideterm_environment::GitOperationKind::Merge {
            actions.push(TerminalGitRepositoryAction::Skip(operation));
        }
        self.render_terminal_git_action_section(&actions, cx)
    }

    fn render_terminal_git_action_section(
        &self,
        actions: &[TerminalGitRepositoryAction],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div().flex().flex_col().gap(px(2.0));
        for action in actions {
            list = list.child(self.render_terminal_git_action_row(*action, cx));
        }
        self.render_terminal_git_action_panel(list)
    }

    fn render_terminal_git_action_panel(&self, list: gpui::Div) -> AnyElement {
        div()
            .max_h(px(340.0))
            .min_h(px(0.0))
            .overflow_y_scrollbar()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
            .p(px(4.0))
            .child(list)
            .into_any_element()
    }

    fn render_terminal_git_plain_panel(&self, list: gpui::Div) -> AnyElement {
        div()
            .min_h(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
            .p(px(4.0))
            .child(list)
            .into_any_element()
    }

    fn render_terminal_git_query_checkout_row(
        &self,
        branch_name: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_terminal_git_query_command_row(
            branch_name,
            "terminal.git.checkout_query",
            LucideIcon::CornerDownLeft,
            |this, cx| this.checkout_terminal_git_query(cx),
            cx,
        )
    }

    fn render_terminal_git_query_rebase_row(
        &self,
        branch_name: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_terminal_git_query_command_row(
            branch_name,
            "terminal.git.rebase_query",
            LucideIcon::GitFork,
            |this, cx| this.rebase_terminal_git_query(cx),
            cx,
        )
    }

    fn render_terminal_git_query_create_branch_row(
        &self,
        branch_name: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_terminal_git_query_command_row(
            branch_name,
            "terminal.git.create_branch_query",
            LucideIcon::Plus,
            |this, cx| this.create_terminal_git_query_branch(cx),
            cx,
        )
    }

    fn render_terminal_git_query_rename_branch_row(
        &self,
        branch_name: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_terminal_git_query_command_row(
            branch_name,
            "terminal.git.rename_branch_query",
            LucideIcon::Pencil,
            |this, cx| this.rename_terminal_git_query_branch(cx),
            cx,
        )
    }

    fn render_terminal_git_query_track_remote_row(
        &self,
        branch_name: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_terminal_git_query_command_row(
            branch_name,
            "terminal.git.track_remote_query",
            LucideIcon::Download,
            |this, cx| this.track_terminal_git_query_remote_branch(cx),
            cx,
        )
    }

    fn render_terminal_git_path_list(
        &self,
        paths: Vec<GitChangedPath>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let paths = paths.into_iter().take(80).collect::<Vec<_>>();
        let mut list = div().flex().flex_col().gap(px(2.0));
        list = self.append_terminal_git_path_group(
            list,
            "terminal.git.group_conflicts",
            LucideIcon::AlertTriangle,
            rgba(0xf87171ff),
            paths.iter().filter(|path| path.conflict()).cloned(),
            cx,
        );
        list = self.append_terminal_git_path_group(
            list,
            "terminal.git.group_staged",
            LucideIcon::CheckCircle,
            rgba(0x86efacff),
            paths.iter().filter(|path| path.staged()).cloned(),
            cx,
        );
        list = self.append_terminal_git_path_group(
            list,
            "terminal.git.group_modified",
            LucideIcon::Pencil,
            rgba(0xfbbf24ff),
            paths
                .iter()
                .filter(|path| path.modified() && !path.conflict())
                .cloned(),
            cx,
        );
        list = self.append_terminal_git_path_group(
            list,
            "terminal.git.group_untracked",
            LucideIcon::FilePlus,
            rgba(0x67e8f9ff),
            paths
                .iter()
                .filter(|path| path.untracked() && !path.conflict())
                .cloned(),
            cx,
        );
        self.render_terminal_git_plain_panel(list)
    }

    fn append_terminal_git_path_group(
        &self,
        mut list: gpui::Div,
        label_key: &'static str,
        icon: LucideIcon,
        color: Rgba,
        paths: impl Iterator<Item = GitChangedPath>,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let paths = paths.collect::<Vec<_>>();
        if paths.is_empty() {
            return list;
        }

        // Grouping mirrors source-control UIs while preserving terminal-owned
        // execution: rows describe current probe results, buttons send commands.
        list = list.child(
            div()
                .h(px(26.0))
                .px(px(8.0))
                .mt(px(4.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(11.0))
                .font_family(self.terminal_git_mono_font())
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(Self::render_lucide_icon(icon, 12.0, color))
                .child(div().flex_1().min_w(px(0.0)).child(self.i18n.t(label_key)))
                .child(self.render_terminal_git_icon_count_chip(icon, paths.len() as u32, color)),
        );
        for path in paths {
            list = list.child(self.render_terminal_git_path_row(path, cx));
        }
        list
    }

    fn render_terminal_git_path_row(
        &self,
        path: GitChangedPath,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let path_label = path.path().to_string();
        let mut row = div()
            .min_h(px(36.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(8.0))
            .py(px(4.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .hover(move |style| style.bg(rgb(theme.bg_hover)))
            .child(Self::render_lucide_icon(
                if path.conflict() {
                    LucideIcon::AlertTriangle
                } else {
                    LucideIcon::FileText
                },
                13.0,
                if path.conflict() {
                    rgba(0xf87171ff)
                } else {
                    rgb(theme.text_muted)
                },
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .font_family(self.terminal_git_mono_font())
                            .text_color(rgb(theme.text))
                            .child(path_label),
                    )
                    .when(path.original_path().is_some(), |meta| {
                        meta.child(
                            div()
                                .truncate()
                                .text_size(px(10.0))
                                .font_family(self.terminal_git_mono_font())
                                .text_color(rgb(theme.text_muted))
                                .child(path.original_path().unwrap_or_default().to_string()),
                        )
                    }),
            )
            .child(self.render_terminal_git_path_badges(&path));

        let mut actions = div().flex().items_center().gap(px(4.0));
        if path.untracked() || path.modified() || path.conflict() {
            actions = actions.child(self.render_terminal_git_path_action_button(
                TerminalGitPathAction::Stage,
                path.path().to_string(),
                cx,
            ));
        }
        if path.staged() {
            actions = actions
                .child(self.render_terminal_git_path_action_button(
                    TerminalGitPathAction::Unstage,
                    path.path().to_string(),
                    cx,
                ))
                .child(self.render_terminal_git_path_action_button(
                    TerminalGitPathAction::DiffStaged,
                    path.path().to_string(),
                    cx,
                ));
        }
        if path.needs_worktree_diff() {
            actions = actions.child(self.render_terminal_git_path_action_button(
                TerminalGitPathAction::Diff,
                path.path().to_string(),
                cx,
            ));
        }
        if path.conflict() {
            actions = actions
                .child(self.render_terminal_git_path_action_button(
                    TerminalGitPathAction::Ours,
                    path.path().to_string(),
                    cx,
                ))
                .child(self.render_terminal_git_path_action_button(
                    TerminalGitPathAction::Theirs,
                    path.path().to_string(),
                    cx,
                ));
        }
        actions = actions.child(self.render_terminal_git_path_action_button(
            TerminalGitPathAction::Open,
            path.path().to_string(),
            cx,
        ));
        row = row.child(actions);
        row.into_any_element()
    }

    fn render_terminal_git_path_badges(&self, path: &GitChangedPath) -> AnyElement {
        let mut badges = div().flex().items_center().gap(px(4.0));
        if path.staged() {
            badges = badges.child(self.render_terminal_git_path_badge(
                "terminal.git.path_state_staged",
                rgba(0x86efacff),
            ));
        }
        if path.modified() {
            badges = badges.child(self.render_terminal_git_path_badge(
                "terminal.git.path_state_modified",
                rgba(0xfbbf24ff),
            ));
        }
        if path.untracked() {
            badges = badges.child(self.render_terminal_git_path_badge(
                "terminal.git.path_state_untracked",
                rgba(0x67e8f9ff),
            ));
        }
        if path.conflict() {
            badges = badges.child(self.render_terminal_git_path_badge(
                "terminal.git.path_state_conflict",
                rgba(0xf87171ff),
            ));
        }
        badges.into_any_element()
    }

    fn render_terminal_git_path_badge(&self, label_key: &'static str, color: Rgba) -> AnyElement {
        div()
            .h(px(18.0))
            .rounded(px(self.tokens.radii.sm))
            .px(px(5.0))
            .flex()
            .items_center()
            .text_size(px(10.0))
            .text_color(color)
            .bg(rgba(0x00000026))
            .child(self.i18n.t(label_key))
            .into_any_element()
    }

    fn render_terminal_git_path_action_button(
        &self,
        action: TerminalGitPathAction,
        path: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self.i18n.t(action.label_key());
        div()
            .h(px(24.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x66))
            .px(px(6.0))
            .flex()
            .items_center()
            .text_size(px(10.0))
            .text_color(rgb(theme.text_muted))
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.run_terminal_git_path_action(action, path.clone(), cx);
                    cx.stop_propagation();
                }),
            )
            .child(label)
            .into_any_element()
    }

    fn render_terminal_git_query_command_row(
        &self,
        branch_name: String,
        label_key: &'static str,
        icon: LucideIcon,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self.i18n_replace(label_key, &[("branch", branch_name.clone())]);

        div()
            .h(px(30.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.accent << 8) | 0x66))
            .bg(rgba((theme.accent << 8) | 0x14))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_color(rgb(theme.accent))
            .cursor_pointer()
            .hover(move |style| style.bg(rgba((theme.accent << 8) | 0x24)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    action(this, cx);
                    cx.stop_propagation();
                }),
            )
            .child(Self::render_lucide_icon(icon, 13.0, rgb(theme.accent)))
            .child(div().flex_1().min_w(px(0.0)).truncate().child(label))
            .into_any_element()
    }

    fn render_terminal_git_action_row(
        &self,
        action: TerminalGitRepositoryAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self.i18n.t(action.label_key());
        let icon = terminal_git_action_icon(action);

        // Action rows intentionally render current Git state, not shell text.
        // The command remains in `TerminalGitActionPlan` so execution is still
        // visible in the active terminal after the user chooses an action.
        let mut row = div()
            .h(px(34.0))
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_color(rgb(theme.text))
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(theme.bg_hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.run_terminal_git_repository_action(action, cx);
                    cx.stop_propagation();
                }),
            )
            .child(Self::render_lucide_icon(icon, 13.0, rgb(theme.text_muted)))
            .child(div().flex_1().min_w(px(0.0)).truncate().child(label));

        if let Some(summary) = self.render_terminal_git_action_summary(action, cx) {
            row = row.child(summary);
        }

        row.into_any_element()
    }

    fn render_terminal_git_action_summary(
        &self,
        action: TerminalGitRepositoryAction,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let snapshot = self.active_terminal_git_snapshot(cx);
        let status = snapshot.as_ref().map(|snapshot| &snapshot.status);
        match action {
            TerminalGitRepositoryAction::Fetch
            | TerminalGitRepositoryAction::FetchAll
            | TerminalGitRepositoryAction::Pull
            | TerminalGitRepositoryAction::Push
            | TerminalGitRepositoryAction::Publish
            | TerminalGitRepositoryAction::PushTags
            | TerminalGitRepositoryAction::RebasePull
            | TerminalGitRepositoryAction::RebaseInteractive => {
                status.and_then(|status| self.render_terminal_git_sync_summary(status))
            }
            TerminalGitRepositoryAction::StageAll
            | TerminalGitRepositoryAction::Status
            | TerminalGitRepositoryAction::Diff
            | TerminalGitRepositoryAction::DiffStaged => {
                status.and_then(|status| self.render_terminal_git_change_count_chips(status))
            }
            TerminalGitRepositoryAction::UnstageAll
            | TerminalGitRepositoryAction::Commit
            | TerminalGitRepositoryAction::CommitVerbose
            | TerminalGitRepositoryAction::CommitSignoff
            | TerminalGitRepositoryAction::Amend
            | TerminalGitRepositoryAction::AmendNoEdit => {
                status.and_then(|status| self.render_terminal_git_staged_count_chip(status))
            }
            TerminalGitRepositoryAction::BranchVerbose => snapshot.map(|snapshot| {
                self.render_terminal_git_data_hint(snapshot.branch.display_text().to_string())
            }),
            TerminalGitRepositoryAction::WorktreeList => {
                let count = self
                    .terminal_git_branch_picker
                    .branches
                    .iter()
                    .filter(|branch| branch.worktree_path().is_some())
                    .count();
                (count > 0).then(|| {
                    self.render_terminal_git_icon_count_chip(
                        LucideIcon::FolderOpen,
                        count as u32,
                        rgb(self.tokens.ui.text_muted),
                    )
                })
            }
            TerminalGitRepositoryAction::ConflictFiles
            | TerminalGitRepositoryAction::Continue(_)
            | TerminalGitRepositoryAction::Abort(_)
            | TerminalGitRepositoryAction::Skip(_) => {
                status.and_then(|status| self.render_terminal_git_conflict_count_chip(status))
            }
            TerminalGitRepositoryAction::Log
            | TerminalGitRepositoryAction::LogStat
            | TerminalGitRepositoryAction::Reflog
            | TerminalGitRepositoryAction::Stash
            | TerminalGitRepositoryAction::StashList
            | TerminalGitRepositoryAction::StashPop
            | TerminalGitRepositoryAction::StashShowLatest
            | TerminalGitRepositoryAction::StashApplyLatest
            | TerminalGitRepositoryAction::StashDropLatest
            | TerminalGitRepositoryAction::RemoteList
            | TerminalGitRepositoryAction::TagList => None,
        }
    }

    fn render_terminal_git_sync_summary(&self, status: &GitRepositoryStatus) -> Option<AnyElement> {
        if status.upstream().is_none() && status.ahead() == 0 && status.behind() == 0 {
            return None;
        }
        let mut summary = div().flex().items_center().justify_end().gap(px(4.0));
        if let Some(upstream) = status.upstream() {
            summary = summary.child(self.render_terminal_git_data_hint(upstream.to_string()));
        }
        if status.ahead() > 0 {
            summary = summary.child(self.render_terminal_git_icon_count_chip(
                LucideIcon::ArrowUp,
                status.ahead(),
                rgba(0x86efacff),
            ));
        }
        if status.behind() > 0 {
            summary = summary.child(self.render_terminal_git_icon_count_chip(
                LucideIcon::ArrowDown,
                status.behind(),
                rgba(0x67e8f9ff),
            ));
        }
        Some(summary.into_any_element())
    }

    fn render_terminal_git_change_count_chips(
        &self,
        status: &GitRepositoryStatus,
    ) -> Option<AnyElement> {
        let mut has_result = false;
        let mut chips = div().flex().items_center().justify_end().gap(px(4.0));
        if status.staged() > 0 {
            has_result = true;
            chips = chips.child(self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_staged",
                status.staged(),
                rgba(0x86efacff),
            ));
        }
        if status.modified() > 0 {
            has_result = true;
            chips = chips.child(self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_modified",
                status.modified(),
                rgba(0xfbbf24ff),
            ));
        }
        if status.untracked() > 0 {
            has_result = true;
            chips = chips.child(self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_untracked",
                status.untracked(),
                rgba(0x67e8f9ff),
            ));
        }
        if status.conflicts() > 0 {
            has_result = true;
            chips = chips.child(self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_conflict",
                status.conflicts(),
                rgba(0xf87171ff),
            ));
        }
        has_result.then(|| chips.into_any_element())
    }

    fn render_terminal_git_staged_count_chip(
        &self,
        status: &GitRepositoryStatus,
    ) -> Option<AnyElement> {
        (status.staged() > 0).then(|| {
            self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_staged",
                status.staged(),
                rgba(0x86efacff),
            )
        })
    }

    fn render_terminal_git_conflict_count_chip(
        &self,
        status: &GitRepositoryStatus,
    ) -> Option<AnyElement> {
        (status.conflicts() > 0).then(|| {
            self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_conflict",
                status.conflicts(),
                rgba(0xf87171ff),
            )
        })
    }

    fn render_terminal_git_data_hint(&self, text: String) -> AnyElement {
        self.render_terminal_git_data_hint_with_width(text, 160.0)
    }

    fn render_terminal_git_data_hint_with_width(&self, text: String, max_width: f32) -> AnyElement {
        div()
            .max_w(px(max_width))
            .min_w(px(0.0))
            .truncate()
            .text_size(px(11.0))
            .font_family(self.terminal_git_mono_font())
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(text)
            .into_any_element()
    }

    fn terminal_git_mono_font(&self) -> gpui::SharedString {
        settings_mono_font_family(self.settings_store.settings())
    }

    fn render_terminal_git_label_count_chip(
        &self,
        label_key: &'static str,
        count: u32,
        color: Rgba,
    ) -> AnyElement {
        div()
            .h(px(18.0))
            .rounded(px(self.tokens.radii.sm))
            .px(px(5.0))
            .flex()
            .items_center()
            .gap(px(2.0))
            .text_size(px(10.0))
            .text_color(color)
            .bg(rgba(0x00000026))
            .child(self.i18n.t(label_key))
            .child(count.to_string())
            .into_any_element()
    }

    fn render_terminal_git_icon_count_chip(
        &self,
        icon: LucideIcon,
        count: u32,
        color: Rgba,
    ) -> AnyElement {
        div()
            .h(px(18.0))
            .rounded(px(self.tokens.radii.sm))
            .px(px(5.0))
            .flex()
            .items_center()
            .gap(px(2.0))
            .text_size(px(10.0))
            .text_color(color)
            .bg(rgba(0x00000026))
            .child(Self::render_lucide_icon(icon, 10.0, color))
            .child(count.to_string())
            .into_any_element()
    }

    fn render_terminal_git_ai_commit_action_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let loading = self.terminal_git_branch_picker.ai_commit_loading;
        let label = if loading {
            self.i18n.t("terminal.git.ai_commit_generating")
        } else {
            self.i18n.t("terminal.git.action_ai_commit_message")
        };
        let error = self.terminal_git_branch_picker.ai_commit_error.clone();
        let text_color = if loading {
            rgb(theme.text_muted)
        } else if error.is_some() {
            rgba(0xfca5a5ff)
        } else {
            rgb(theme.text)
        };
        let staged_summary = if error.is_none() {
            self.active_terminal_git_snapshot(cx)
                .and_then(|snapshot| self.render_terminal_git_staged_count_chip(&snapshot.status))
        } else {
            None
        };

        let mut row = div()
            .h(px(34.0))
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_color(text_color)
            .when(!loading, |button| {
                button
                    .cursor_pointer()
                    .hover(move |style| style.bg(rgb(theme.bg_hover)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.generate_terminal_git_ai_commit_message(cx);
                            cx.stop_propagation();
                        }),
                    )
            })
            .when(loading, |button| button.cursor(CursorStyle::Arrow))
            .child(Self::render_lucide_icon(
                LucideIcon::Sparkles,
                12.0,
                if loading {
                    rgb(theme.text_muted)
                } else {
                    rgb(theme.accent)
                },
            ))
            .child(div().flex_1().min_w(px(0.0)).truncate().child(label));

        if let Some(error) = error {
            row = row.child(
                div()
                    .max_w(px(210.0))
                    .truncate()
                    .text_size(px(11.0))
                    .text_color(rgba(0xfca5a5ff))
                    .child(error),
            );
        } else if let Some(summary) = staged_summary {
            row = row.child(summary);
        }

        row.into_any_element()
    }

    fn render_terminal_git_branch_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::TerminalGitBranchSearch;
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &self.terminal_git_branch_picker.query,
                    placeholder: self.i18n.t("terminal.git.search_branches"),
                    focused: self.terminal_git_branch_picker.open,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .h(px(32.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle);
                    this.ime_marked_text = None;
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(
                |this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                },
            )),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn render_terminal_git_branch_row(
        &self,
        branch: oxideterm_environment::GitBranchReference,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let branch_name = branch.name().to_string();
        let highlighted = self
            .terminal_git_branch_picker
            .highlighted_branch
            .as_deref()
            .is_some_and(|name| name == branch.name());
        let current = branch.current();
        let worktree_path = branch.worktree_path().map(str::to_string);
        let linked_worktree = worktree_path.is_some() && !current;
        let mut branch_identity = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(1.0))
            .child(
                div()
                    .truncate()
                    .font_family(self.terminal_git_mono_font())
                    .child(branch_name.clone()),
            );
        if let Some(worktree_path) = worktree_path {
            branch_identity = branch_identity.child(
                div()
                    .truncate()
                    .text_size(px(10.0))
                    .font_family(self.terminal_git_mono_font())
                    .text_color(rgb(theme.text_muted))
                    .child(worktree_path),
            );
        }

        div()
            .min_h(px(if linked_worktree { 42.0 } else { 30.0 }))
            .rounded(px(self.tokens.radii.md))
            .px(px(8.0))
            .py(px(4.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .bg(if highlighted {
                rgba((theme.accent << 8) | 0x24)
            } else {
                rgba(0x00000000)
            })
            .text_color(if current {
                rgb(theme.accent)
            } else {
                rgb(theme.text)
            })
            .hover(move |style| style.bg(rgb(theme.bg_hover)))
            .on_mouse_move(cx.listener({
                let branch_name = branch_name.clone();
                move |this, _event: &MouseMoveEvent, _window, cx| {
                    if this
                        .terminal_git_branch_picker
                        .highlighted_branch
                        .as_deref()
                        != Some(branch_name.as_str())
                    {
                        this.terminal_git_branch_picker.highlighted_branch =
                            Some(branch_name.clone());
                        cx.notify();
                    }
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let branch = branch.clone();
                    move |this, _event, _window, cx| {
                        this.select_terminal_git_branch(branch.clone(), cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .child(Self::render_lucide_icon(
                if current {
                    LucideIcon::Check
                } else if linked_worktree {
                    LucideIcon::FolderOpen
                } else {
                    LucideIcon::GitFork
                },
                13.0,
                if current || highlighted {
                    rgb(theme.accent)
                } else {
                    rgb(theme.text_muted)
                },
            ))
            .child(branch_identity)
            .into_any_element()
    }

    fn render_terminal_git_branch_message(&self, icon: LucideIcon, message: String) -> AnyElement {
        div()
            .min_h(px(56.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text_muted),
            ))
            .child(message)
            .into_any_element()
    }

    fn render_terminal_git_branch_error(&self, message: String) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba(0xef44444d))
            .bg(rgba(0xef44441a))
            .p(px(8.0))
            .text_color(rgba(0xfca5a5ff))
            .child(message)
            .into_any_element()
    }

    fn terminal_git_branch_picker_left(&self) -> f32 {
        let Some(chip) = self
            .select_anchors
            .get(&SelectAnchorId::TerminalGitBranchMenu)
        else {
            return TERMINAL_GIT_BRANCH_MENU_MARGIN;
        };
        let Some(bar) = self.select_anchors.get(&SelectAnchorId::TerminalCommandBar) else {
            return TERMINAL_GIT_BRANCH_MENU_MARGIN;
        };
        let bar_width = f32::from(bar.bounds.size.width);
        let desired = f32::from(chip.bounds.left() - bar.bounds.left());
        let max_left =
            (bar_width - TERMINAL_GIT_BRANCH_MENU_WIDTH - TERMINAL_GIT_BRANCH_MENU_MARGIN)
                .max(TERMINAL_GIT_BRANCH_MENU_MARGIN);
        desired.clamp(TERMINAL_GIT_BRANCH_MENU_MARGIN, max_left)
    }

    fn terminal_cwd_picker_left(&self) -> f32 {
        let Some(chip) = self.select_anchors.get(&SelectAnchorId::TerminalCwdMenu) else {
            return TERMINAL_CWD_MENU_MARGIN;
        };
        let Some(bar) = self.select_anchors.get(&SelectAnchorId::TerminalCommandBar) else {
            return TERMINAL_CWD_MENU_MARGIN;
        };
        let bar_width = f32::from(bar.bounds.size.width);
        let desired = f32::from(chip.bounds.left() - bar.bounds.left());
        let max_left = (bar_width - TERMINAL_CWD_MENU_WIDTH - TERMINAL_CWD_MENU_MARGIN)
            .max(TERMINAL_CWD_MENU_MARGIN);
        desired.clamp(TERMINAL_CWD_MENU_MARGIN, max_left)
    }

    fn active_privilege_scope_credentials(
        &self,
    ) -> Option<(String, Vec<SavedPrivilegeCredential>)> {
        let Some(active_tab) = self.active_tab() else {
            log_privilege_prompt_helper(format_args!("scope unavailable: no active tab"));
            return None;
        };
        match &active_tab.kind {
            TabKind::LocalTerminal => {
                if self.active_tab_has_serial_terminal() {
                    log_privilege_prompt_helper(format_args!(
                        "scope unavailable: local tab is backed by a serial terminal"
                    ));
                    return None;
                }
                // Local shell sudo/su prompts have no SavedConnection owner. Use a
                // dedicated store scope so secrets are never confused with SSH
                // connection credentials.
                let connection_id = LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string();
                let credentials = self
                    .connection_store
                    .list_privilege_credentials(&connection_id)
                    .unwrap_or_default();
                log_privilege_prompt_helper(format_args!(
                    "scope resolved: scope=local credentials_count={}",
                    credentials.len()
                ));
                Some((connection_id, credentials))
            }
            TabKind::SshTerminal => {
                let Some(session_id) = self.active_terminal_session_id() else {
                    log_privilege_prompt_helper(format_args!(
                        "scope unavailable: ssh tab has no active terminal session"
                    ));
                    return None;
                };
                let Some(node_id) = self.terminal_ssh_nodes.get(&session_id) else {
                    log_privilege_prompt_helper(format_args!(
                        "scope unavailable: ssh terminal session has no node mapping"
                    ));
                    return None;
                };
                let Some(connection_id) = self.ssh_privilege_scope_id_for_node(node_id) else {
                    log_privilege_prompt_helper(format_args!(
                        "scope unavailable: ssh node has no saved owner"
                    ));
                    return None;
                };
                if self.connection_store.get(&connection_id).is_none() {
                    log_privilege_prompt_helper(format_args!(
                        "scope unavailable: ssh saved owner is missing from connection store"
                    ));
                    return None;
                }
                let credentials = self
                    .connection_store
                    .list_privilege_credentials(&connection_id)
                    .unwrap_or_default();
                log_privilege_prompt_helper(format_args!(
                    "scope resolved: scope=ssh credentials_count={}",
                    credentials.len()
                ));
                Some((connection_id, credentials))
            }
            tab_kind => {
                log_privilege_prompt_helper(format_args!(
                    "scope unavailable: tab_kind={}",
                    tab_kind_privilege_scope_name(tab_kind)
                ));
                None
            }
        }
    }

    fn ssh_privilege_scope_id_for_node(&self, node_id: &NodeId) -> Option<String> {
        let node_saved_connection_id = self
            .ssh_nodes
            .get(node_id)
            .and_then(|node| node.saved_connection_id.as_deref());
        let node_origin = self
            .node_runtime_store
            .snapshot(node_id)
            .map(|snapshot| snapshot.origin);
        let has_origin_saved_owner = node_origin
            .as_ref()
            .and_then(NodeOrigin::saved_connection_id)
            .is_some_and(|connection_id| !connection_id.trim().is_empty());
        // SSH privilege credentials are scoped to the node owner. Do not recover
        // a saved connection by matching host/port/user/title or by using the
        // runtime connection id; reused node terminals must share this same owner.
        let scope_id = saved_ssh_privilege_scope_id(node_saved_connection_id, node_origin.as_ref());
        log_privilege_prompt_helper(format_args!(
            "ssh scope lookup: has_node_saved_owner={} has_runtime_snapshot={} has_origin_saved_owner={} resolved={}",
            node_saved_connection_id.is_some_and(|connection_id| !connection_id.trim().is_empty()),
            node_origin.is_some(),
            has_origin_saved_owner,
            scope_id.is_some()
        ));
        scope_id
    }

    fn active_privilege_prompt_state(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<PrivilegePromptHelperState> {
        let Some(active_tab) = self.active_tab() else {
            log_privilege_prompt_helper(format_args!("state unavailable: no active tab"));
            return None;
        };
        if !tab_kind_allows_privilege_prompt_helper(&active_tab.kind) {
            log_privilege_prompt_helper(format_args!(
                "state unavailable: unsupported tab_kind={}",
                tab_kind_privilege_scope_name(&active_tab.kind)
            ));
            return None;
        }
        let Some(active_pane) = self.active_pane() else {
            log_privilege_prompt_helper(format_args!("state unavailable: no active pane"));
            return None;
        };
        let pane = active_pane.read(cx);
        let visible_text = pane.privilege_prompt_text_snapshot();
        let visible_shape = privilege_prompt_text_shape(&visible_text);
        let tracked_prompt = pane
            .privilege_prompt_snapshot()
            .map(|snapshot| snapshot.prompt);
        let tracked_prompt_kind = tracked_prompt
            .as_ref()
            .map(privilege_prompt_kind_name)
            .unwrap_or("none");
        if tracked_prompt.is_none() && pane.privilege_prompt_fallback_suppressed() {
            log_privilege_prompt_helper(format_args!(
                "state unavailable: fallback suppressed visible_chars={}",
                visible_shape.chars
            ));
            return None;
        }
        // Tauri keeps the prompt state alive even when credential metadata
        // cannot be loaded. Do not let a missing credential row or transient
        // keychain/config error suppress the detected sudo/su prompt.
        let Some((connection_id, credentials)) = self.active_privilege_scope_credentials() else {
            log_privilege_prompt_helper(format_args!(
                "state unavailable: no credential scope tracked_prompt={} visible_chars={}",
                tracked_prompt_kind, visible_shape.chars
            ));
            return None;
        };
        let state = build_privilege_prompt_helper_state_with_tracked_prompt(
            connection_id,
            &credentials,
            &visible_text,
            tracked_prompt,
        );
        match &state {
            Some(state) => log_privilege_prompt_helper(format_args!(
                "state ready: prompt_kind={} matches_count={} credentials_count={} tracked_prompt={} visible_chars={}",
                privilege_prompt_kind_name(&state.prompt),
                state.matches.len(),
                credentials.len(),
                tracked_prompt_kind,
                visible_shape.chars
            )),
            None => log_privilege_prompt_helper(format_args!(
                "state unavailable: no prompt match credentials_count={} tracked_prompt={} visible_chars={} visible_lines={} has_ascii_colon={} has_fullwidth_colon={} ends_colon={} contains_sudo_marker={} starts_sudo_marker={} contains_password_word={} contains_cjk_password={} contains_escape={}",
                credentials.len(),
                tracked_prompt_kind,
                visible_shape.chars,
                visible_shape.lines,
                visible_shape.has_ascii_colon,
                visible_shape.has_fullwidth_colon,
                visible_shape.ends_with_prompt_colon,
                visible_shape.contains_sudo_marker,
                visible_shape.starts_with_sudo_marker,
                visible_shape.contains_password_word,
                visible_shape.contains_cjk_password,
                visible_shape.contains_escape
            )),
        }
        state
    }

    pub(in crate::workspace) fn sync_active_privilege_prompt_inline_hint(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(active_pane) = self.active_pane() else {
            return false;
        };
        let hint = self.active_privilege_prompt_inline_hint(cx);
        active_pane.update(cx, |pane, cx| {
            pane.set_privilege_prompt_inline_hint(hint, cx)
        })
    }

    fn active_privilege_prompt_inline_hint(&self, cx: &mut Context<Self>) -> Option<String> {
        let Some(state) = self.active_privilege_prompt_state(cx) else {
            log_privilege_prompt_helper(format_args!("hint unavailable: no prompt state"));
            return None;
        };
        let prompt_kind = privilege_prompt_kind_name(&state.prompt);
        if !privilege_prompt_state_allows_confirmed_fill(&state) {
            log_privilege_prompt_helper(format_args!(
                "hint suppressed: prompt_kind={} matches_count={}",
                prompt_kind,
                state.matches.len()
            ));
            return None;
        }
        log_privilege_prompt_helper(format_args!(
            "hint ready: prompt_kind={} matches_count=1",
            prompt_kind
        ));
        Some(self.i18n.t("terminal.privilege_helper.inline_fill_hint"))
    }

    pub(in crate::workspace) fn handle_privilege_prompt_helper_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let modifiers = event.keystroke.modifiers;
        if event.keystroke.key.as_str() != "enter"
            || modifiers.platform
            || modifiers.control
            || modifiers.alt
            || modifiers.shift
        {
            return false;
        }

        log_privilege_prompt_helper(format_args!("root enter: evaluating privilege helper"));
        let Some(state) = self.active_privilege_prompt_state(cx) else {
            log_privilege_prompt_helper(format_args!("root enter: no prompt state"));
            return false;
        };
        if !privilege_prompt_state_allows_confirmed_fill(&state) {
            log_privilege_prompt_helper(format_args!(
                "root enter: ignored match_count={}",
                state.matches.len()
            ));
            return false;
        };
        let [matched] = state.matches.as_slice() else {
            return false;
        };
        // The inline hint is the confirmation affordance: Enter is accepted only
        // when prompt detection produces exactly one scoped credential. Bare
        // `Password:` prompts therefore work for macOS sudo without guessing
        // between multiple saved sudo/su candidates.
        log_privilege_prompt_helper(format_args!(
            "root enter: filling prompt_kind={}",
            privilege_prompt_kind_name(&state.prompt)
        ));
        self.fill_privilege_prompt_match(matched.clone(), window, cx);
        true
    }

    pub(in crate::workspace) fn handle_active_privilege_prompt_submit_request(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(active_pane) = self.active_pane() else {
            return false;
        };
        let requested =
            active_pane.update(cx, |pane, _cx| pane.take_privilege_prompt_submit_request());
        if !requested {
            return false;
        }

        log_privilege_prompt_helper(format_args!(
            "terminal submit request: evaluating privilege helper"
        ));
        let Some(state) = self.active_privilege_prompt_state(cx) else {
            log_privilege_prompt_helper(format_args!("terminal submit request: no prompt state"));
            return false;
        };
        if !privilege_prompt_state_allows_confirmed_fill(&state) {
            log_privilege_prompt_helper(format_args!(
                "terminal submit request: ignored match_count={}",
                state.matches.len()
            ));
            return false;
        };
        let [matched] = state.matches.as_slice() else {
            return false;
        };
        // TerminalPane captures Enter before it is written as a plain newline;
        // Workspace still owns the secret lookup and one-shot PTY handoff.
        log_privilege_prompt_helper(format_args!(
            "terminal submit request: filling prompt_kind={}",
            privilege_prompt_kind_name(&state.prompt)
        ));
        self.fill_privilege_prompt_match(matched.clone(), window, cx);
        true
    }

    fn fill_privilege_prompt_match(
        &mut self,
        matched: MatchedPrivilegeCredential,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log_privilege_prompt_helper(format_args!("fill: loading scoped credential secret"));
        let secret = match self
            .connection_store
            .get_privilege_credential_secret(&matched.connection_id, &matched.credential_id)
        {
            Ok(secret) => secret,
            Err(error) => {
                log_privilege_prompt_helper(format_args!("fill: secret load failed"));
                self.push_command_palette_toast(
                    self.i18n.t("terminal.privilege_helper.load_failed"),
                    Some(error.to_string()),
                    TerminalNoticeVariant::Error,
                );
                cx.notify();
                return;
            }
        };
        log_privilege_prompt_helper(format_args!("fill: secret loaded"));
        // The newline-bearing buffer is the only owned cleartext copy in the
        // GPUI layer. It is zeroized after the PTY write attempt, matching the
        // Tauri click-only secret handoff without involving command history.
        let secret_line = zeroize::Zeroizing::new(format!("{}\n", secret.expose_secret()));
        let sent = self.active_pane().is_some_and(|pane| {
            pane.update(cx, |pane, cx| {
                pane.send_privilege_secret_input_bytes(secret_line.as_bytes(), cx)
            })
        });
        log_privilege_prompt_helper(format_args!("fill: write attempted sent={sent}"));
        if !sent {
            self.push_command_palette_toast(
                self.i18n.t("terminal.privilege_helper.send_failed"),
                None,
                TerminalNoticeVariant::Error,
            );
        }
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn render_terminal_surface(
        &self,
        root_pane: &PaneNode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let terminal = self.render_pane_tree(root_pane, cx);
        let recording_status = self.active_terminal_recording_status(cx);
        let recording_active = recording_status.state != TerminalRecordingState::Idle;
        if !self.settings_store.settings().terminal.command_bar.enabled {
            return div()
                .size_full()
                .relative()
                .child(terminal)
                .when(recording_active, |surface| {
                    surface.child(self.render_terminal_recording_controls(recording_status, cx))
                })
                .into_any_element();
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(terminal)
                    .when(recording_active, |surface| {
                        surface.child(self.render_terminal_recording_controls(recording_status, cx))
                    }),
            )
            .child(self.render_terminal_command_bar(cx))
            .into_any_element()
    }

    fn render_terminal_command_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        const COMMAND_BAR_BG_ALPHA: u32 = 0xf2; // Tauri bg-theme-bg/95
        const COMMAND_BAR_BORDER_ALPHA: u32 = 0xb3; // Tauri border-theme-border/70
        const COMMAND_BAR_INPUT_BORDER_ALPHA: u32 = 0x73; // Tauri border-theme-border/45
        const COMMAND_BAR_FOCUSED_BORDER_ALPHA: u32 = 0x73; // Tauri border-theme-accent/45

        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::TerminalCommandBar;
        let workspace = cx.entity();
        let input_collapsed = self.terminal_command_input_collapsed;
        let focused = self.terminal_command_bar_focused && !input_collapsed;
        let marked_text = self.marked_text_for_target(target);
        let selected_range = self.ime_selected_range_for_target(target);
        let command_is_empty = self.terminal_command_bar_draft.is_empty();
        let command_suggestions = if focused {
            self.terminal_command_bar_suggestions(false, cx)
        } else {
            Vec::new()
        };
        let ghost_text = self.terminal_command_ghost_text(&command_suggestions);
        let showing_placeholder = command_is_empty && marked_text.is_none();
        let command_text = if showing_placeholder {
            self.i18n.t("terminal.command_bar.command_placeholder")
        } else {
            self.terminal_command_bar_draft.clone()
        };
        let input_range = selected_range
            .clone()
            .filter(|_| focused && !command_is_empty && marked_text.is_none());
        let selection_range = input_range.clone().filter(|range| range.start < range.end);
        let caret_offset = input_range
            .as_ref()
            .filter(|range| range.start == range.end)
            .map(|range| range.start);
        let shows_selection = selection_range.is_some();
        let shows_positioned_caret = caret_offset.is_some() && !shows_selection;
        // The visible chip and completion providers share Tauri's target-label
        // inference so local shells that are currently inside SSH show the
        // remote identity consistently in both places.
        let target_label = self.terminal_command_active_target_label(cx);
        let cwd_snapshot = self.active_terminal_cwd_snapshot(cx);
        let cwd_supported = self.active_terminal_cwd_scope_and_pane().is_some();
        let git_snapshot = self.active_terminal_git_snapshot(cx);
        let active_pane_id = self.active_pane_id();
        let is_local_terminal = self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::LocalTerminal);
        let target_indicator_is_local =
            is_local_terminal && target_label == self.i18n.t("terminal.command_bar.local_shell");
        let can_split = self.active_tab().is_some_and(|tab| {
            tab.kind == TabKind::LocalTerminal
                && !self.active_tab_has_serial_terminal()
                && tab
                    .root_pane
                    .as_ref()
                    .is_some_and(|root| root.pane_count() < MAX_PANES_PER_TAB)
        });
        let broadcast_targets =
            self.terminal_broadcast_target_panes(active_pane_id.unwrap_or(PaneId(0)));
        let broadcast_label = if self.terminal_broadcast_enabled {
            if self.terminal_broadcast_targets.is_empty() {
                self.i18n.t("terminal.command_bar.all_targets")
            } else {
                format!("{}", broadcast_targets.len())
            }
        } else {
            String::new()
        };
        let quick_commands_enabled = self
            .settings_store
            .settings()
            .terminal
            .command_bar
            .quick_commands_enabled;
        let recording_status = self.active_terminal_recording_status(cx);
        let recording_active = recording_status.state != TerminalRecordingState::Idle;
        let timestamps_active = self.active_terminal_timestamps_enabled(cx);
        let input_toggle_tooltip_id = "terminal-command-input-toggle";
        let input_toggle_title = if input_collapsed {
            self.i18n.t("terminal.command_bar.expand_input")
        } else {
            self.i18n.t("terminal.command_bar.collapse_input")
        };

        let bar = div()
            .relative()
            .flex_none()
            .border_t_1()
            .border_color(rgba((theme.border << 8) | COMMAND_BAR_BORDER_ALPHA))
            .bg(rgba((theme.bg << 8) | COMMAND_BAR_BG_ALPHA))
            .px(px(12.0))
            .py(px(4.0))
            .shadow_lg()
            .when(
                !input_collapsed
                    && focused
                    && self.terminal_command_suggestions_open
                    && !command_suggestions.is_empty(),
                |bar| bar.child(self.render_terminal_command_suggestions(&command_suggestions, cx)),
            )
            .when(
                !input_collapsed && quick_commands_enabled && self.terminal_quick_commands_open,
                |bar| {
                    // Tauri renders QuickCommandsPopover as a child of the relative
                    // TerminalCommandBar (`absolute bottom-full right-3`). Keep the
                    // native popover on the same local coordinate owner; routing it
                    // through the root backdrop makes the existing bottom/right
                    // placement resolve against the wrong box.
                    bar.child(self.render_terminal_quick_commands_popover(cx))
                },
            )
            .when(self.terminal_git_branch_picker.open, |bar| {
                bar.child(self.render_terminal_git_branch_picker(cx))
            })
            .when(self.terminal_cwd_picker.open, |bar| {
                bar.child(self.render_terminal_cwd_picker(cx))
            })
            .child(
                div()
                    .min_h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .flex_1()
                            .min_w(px(0.0))
                            .child(
                                self.terminal_command_action_button(
                                    if input_collapsed {
                                        LucideIcon::ChevronRight
                                    } else {
                                        LucideIcon::ChevronDown
                                    },
                                    rgb(theme.text_muted),
                                    false,
                                    Some(if input_collapsed {
                                        rgba((theme.bg_hover << 8) | 0x99)
                                    } else {
                                        rgba(0x00000000)
                                    }),
                                    |this, _event, _window, cx| {
                                        this.terminal_command_input_collapsed =
                                            !this.terminal_command_input_collapsed;
                                        // Collapsing is visual-only. Keep the draft, but release
                                        // hidden input ownership so keystrokes return to the pane.
                                        if this.terminal_command_input_collapsed {
                                            this.terminal_command_bar_focused = false;
                                            this.ime_marked_text = None;
                                            this.terminal_command_suggestions_open = false;
                                            this.terminal_command_suggestion_highlighted = None;
                                            this.close_terminal_quick_commands_popover();
                                            this.close_terminal_cwd_picker();
                                        }
                                        this.clear_workspace_tooltip(input_toggle_tooltip_id, cx);
                                        cx.stop_propagation();
                                        cx.notify();
                                    },
                                    cx,
                                )
                                .id(input_toggle_tooltip_id)
                                .on_mouse_move({
                                    let title = input_toggle_title.clone();
                                    cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                                        this.queue_workspace_tooltip(
                                            input_toggle_tooltip_id,
                                            title.clone(),
                                            f32::from(event.position.x) + 12.0,
                                            f32::from(event.position.y) + 16.0,
                                            cx,
                                        );
                                    })
                                })
                                .on_hover(cx.listener(
                                    move |this, hovered: &bool, _window, cx| {
                                        if !*hovered {
                                            this.clear_workspace_tooltip(
                                                input_toggle_tooltip_id,
                                                cx,
                                            );
                                        }
                                    },
                                )),
                            )
                            .child(self.render_terminal_target_indicator(
                                target_label,
                                target_indicator_is_local,
                                cx,
                            ))
                            .when(cwd_supported, |row| {
                                row.child(self.render_terminal_cwd_chip(cwd_snapshot, cx))
                            })
                            .when_some(git_snapshot, |row, snapshot| {
                                row.child(self.render_terminal_git_chip(snapshot, cx))
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(px(4.0))
                            .when(
                                self.terminal_broadcast_enabled && !broadcast_label.is_empty(),
                                |actions| {
                                    actions.child(
                                        div()
                                            .h(px(20.0))
                                            .px(px(6.0))
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .rounded(px(self.tokens.radii.md))
                                            .border_1()
                                            .border_color(rgba(0xf973164d))
                                            .bg(rgba(0xf973161a))
                                            .text_size(px(11.0))
                                            .text_color(rgba(0xfdba74ff))
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::Radio,
                                                12.0,
                                                rgba(0xfdba74ff),
                                            ))
                                            .child(broadcast_label),
                                    )
                                },
                            )
                            .when(is_local_terminal, |actions| {
                                actions
                                    .child(self.terminal_command_action_button(
                                        LucideIcon::SplitSquareHorizontal,
                                        rgb(theme.text_muted),
                                        !can_split,
                                        None,
                                        |this, _event, window, cx| {
                                            this.split_active_pane(
                                                SplitDirection::Horizontal,
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                        cx,
                                    ))
                                    .child(self.terminal_command_action_button(
                                        LucideIcon::SplitSquareVertical,
                                        rgb(theme.text_muted),
                                        !can_split,
                                        None,
                                        |this, _event, window, cx| {
                                            this.split_active_pane(
                                                SplitDirection::Vertical,
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                        cx,
                                    ))
                            })
                            .child(select_anchor_probe(
                                SelectAnchorId::TerminalBroadcastMenu,
                                self.terminal_command_action_button(
                                    LucideIcon::Radio,
                                    if self.terminal_broadcast_enabled {
                                        rgba(0xfb923cff)
                                    } else {
                                        rgb(theme.text_muted)
                                    },
                                    false,
                                    Some(if self.terminal_broadcast_enabled {
                                        rgba(0xf9731626)
                                    } else {
                                        rgba((theme.bg_hover << 8) | 0x00)
                                    }),
                                    |this, _event, _window, cx| {
                                        this.toggle_terminal_broadcast_menu();
                                        cx.stop_propagation();
                                        cx.notify();
                                    },
                                    cx,
                                )
                                .relative(),
                                {
                                    let workspace = workspace.clone();
                                    move |anchor, _window, cx| {
                                        let _ = workspace.update(cx, |this, cx| {
                                            this.update_select_anchor(anchor, cx);
                                        });
                                    }
                                },
                            ))
                            .child(self.terminal_command_action_button(
                                LucideIcon::Search,
                                if self.search.visible {
                                    rgb(theme.accent)
                                } else {
                                    rgb(theme.text_muted)
                                },
                                false,
                                Some(if self.search.visible {
                                    rgba((theme.accent << 8) | 0x26)
                                } else {
                                    rgba(0x00000000)
                                }),
                                |this, _event, window, cx| {
                                    if this.search.visible {
                                        this.close_search(window, cx);
                                    } else {
                                        this.open_search(window, cx);
                                    }
                                    cx.stop_propagation();
                                },
                                cx,
                            ))
                            .child(self.terminal_command_action_button(
                                LucideIcon::Clock,
                                if timestamps_active {
                                    rgba(0x22d3eeff)
                                } else {
                                    rgb(theme.text_muted)
                                },
                                false,
                                Some(if timestamps_active {
                                    rgba(0x22d3ee26)
                                } else {
                                    rgba(0x00000000)
                                }),
                                |this, _event, _window, cx| {
                                    this.toggle_active_terminal_timestamps(cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            ))
                            .when(recording_active, |actions| {
                                actions.child(
                                    div()
                                        .h(px(20.0))
                                        .px(px(6.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .rounded(px(self.tokens.radii.md))
                                        .border_1()
                                        .border_color(rgba(0xef44444d))
                                        .bg(rgba(0xef44441a))
                                        .text_size(px(11.0))
                                        .text_color(rgba(0xfca5a5ff))
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::Circle,
                                            10.0,
                                            rgba(0xfca5a5ff),
                                        ))
                                        .child(format_recording_elapsed(recording_status.elapsed)),
                                )
                            })
                            .child(self.terminal_command_action_button(
                                match recording_status.state {
                                    TerminalRecordingState::Paused => LucideIcon::Play,
                                    _ => LucideIcon::Circle,
                                },
                                if recording_active {
                                    rgba(0xf87171ff)
                                } else {
                                    rgb(theme.text_muted)
                                },
                                false,
                                Some(if recording_active {
                                    rgba(0xef444426)
                                } else {
                                    rgba(0x00000000)
                                }),
                                move |this, _event, _window, cx| {
                                    match recording_status.state {
                                        TerminalRecordingState::Idle => {
                                            this.start_active_terminal_recording(cx)
                                        }
                                        TerminalRecordingState::Recording => {
                                            this.pause_active_terminal_recording(cx)
                                        }
                                        TerminalRecordingState::Paused => {
                                            this.resume_active_terminal_recording(cx)
                                        }
                                    }
                                    cx.stop_propagation();
                                },
                                cx,
                            ))
                            .when(recording_active, |actions| {
                                actions
                                    .child(self.terminal_command_action_button(
                                        LucideIcon::Square,
                                        rgba(0xf87171ff),
                                        false,
                                        None,
                                        |this, _event, _window, cx| {
                                            this.stop_active_terminal_recording(cx);
                                            cx.stop_propagation();
                                        },
                                        cx,
                                    ))
                                    .child(self.terminal_command_action_button(
                                        LucideIcon::Trash2,
                                        rgba(0xf87171ff),
                                        false,
                                        None,
                                        |this, _event, _window, cx| {
                                            this.discard_active_terminal_recording(cx);
                                            cx.stop_propagation();
                                        },
                                        cx,
                                    ))
                            })
                            .child(self.terminal_command_action_button(
                                LucideIcon::FilePlay,
                                rgb(theme.text_muted),
                                false,
                                None,
                                |this, _event, window, cx| {
                                    this.open_terminal_cast_file(window, cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            )),
                    ),
            )
            .when(!input_collapsed, |bar| {
                bar.child(
                    div()
                        .mt(px(2.0))
                        .pt(px(4.0))
                        .border_t_1()
                        .border_color(if focused {
                            rgba((theme.accent << 8) | COMMAND_BAR_FOCUSED_BORDER_ALPHA)
                        } else {
                            rgba((theme.border << 8) | COMMAND_BAR_INPUT_BORDER_ALPHA)
                        })
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .cursor_text()
                                // Tauri only focuses the command textarea when the
                                // row background or textarea area receives the
                                // pointer. Keep the quick-command button outside
                                // this hit region so its click cannot be captured
                                // by IME selection before the toggle handler runs.
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, event: &gpui::MouseDownEvent, window, cx| {
                                            this.terminal_command_bar_focused = true;
                                            this.ime_marked_text = None;
                                            window.focus(&this.focus_handle);
                                            this.begin_ime_selection_from_mouse_down(
                                                WorkspaceImeTarget::TerminalCommandBar,
                                                event,
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                    ),
                                )
                                .on_mouse_move(cx.listener(
                                    |this, event: &gpui::MouseMoveEvent, window, cx| {
                                        this.update_ime_selection_drag_from_mouse_move(
                                            event, window, cx,
                                        );
                                    },
                                ))
                                .child(Self::render_lucide_icon(
                                    LucideIcon::ChevronRight,
                                    16.0,
                                    rgb(theme.text_muted),
                                ))
                                .child(text_input_anchor_probe(
                                    target.anchor_id(),
                                    div()
                                        .h(px(24.0))
                                        .flex_1()
                                        .flex()
                                        .items_center()
                                        .overflow_hidden()
                                        .text_size(px(13.0))
                                        .font_family(settings_mono_font_family(
                                            self.settings_store.settings(),
                                        ))
                                        .text_color(if showing_placeholder {
                                            rgb(theme.text_muted)
                                        } else {
                                            rgb(theme.text)
                                        })
                                        .when(focused && showing_placeholder, |input| {
                                            input.child(text_caret(
                                                &self.tokens,
                                                self.new_connection_caret_visible,
                                            ))
                                        })
                                        // Tauri uses a real textarea, so the painted caret
                                        // follows selectionStart instead of always sitting
                                        // at the end of the value. Keep native rendering
                                        // tied to the shared IME range for click/arrow parity.
                                        .child(if showing_placeholder {
                                            div().child(command_text).into_any_element()
                                        } else {
                                            text_input_value_segments_with_color(
                                                &self.tokens,
                                                &command_text,
                                                false,
                                                selection_range,
                                                caret_offset,
                                                self.new_connection_caret_visible,
                                                Some(theme.text),
                                            )
                                            .into_any_element()
                                        })
                                        .when_some(marked_text, |input, marked| {
                                            input.child(
                                                div()
                                                    .underline()
                                                    .text_color(rgb(theme.text))
                                                    .child(marked.to_string()),
                                            )
                                        })
                                        .when(
                                            focused
                                                && !showing_placeholder
                                                && !shows_selection
                                                && !shows_positioned_caret,
                                            |input| {
                                                input.child(text_caret(
                                                    &self.tokens,
                                                    self.new_connection_caret_visible,
                                                ))
                                            },
                                        )
                                        .when_some(ghost_text, |input, ghost| {
                                            input.child(
                                                div()
                                                    .text_color(rgba(
                                                        (theme.text_muted << 8) | 0x99,
                                                    ))
                                                    .child(ghost),
                                            )
                                        }),
                                    {
                                        let workspace = workspace.clone();
                                        move |anchor, _window, cx| {
                                            let _ = workspace.update(cx, |this, cx| {
                                                this.update_text_input_anchor(anchor, cx);
                                            });
                                        }
                                    },
                                )),
                        )
                        .when(quick_commands_enabled, |input_row| {
                            input_row.child(
                                div()
                                    .size(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(self.tokens.radii.md))
                                    .cursor_pointer()
                                    .bg(if self.terminal_quick_commands_open {
                                        rgba((theme.accent << 8) | 0x1a)
                                    } else {
                                        rgba(0x00000000)
                                    })
                                    .text_color(if self.terminal_quick_commands_open {
                                        rgb(theme.accent)
                                    } else {
                                        rgb(theme.text_muted)
                                    })
                                    .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.terminal_quick_commands_open =
                                                !this.terminal_quick_commands_open;
                                            this.dismiss_terminal_broadcast_menu();
                                            this.close_terminal_cwd_picker();
                                            this.close_terminal_git_branch_picker();
                                            if !this.terminal_quick_commands_open {
                                                this.close_terminal_quick_commands_popover();
                                            }
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    )
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::Zap,
                                        14.0,
                                        if self.terminal_quick_commands_open {
                                            rgb(theme.accent)
                                        } else {
                                            rgb(theme.text_muted)
                                        },
                                    )),
                            )
                        }),
                )
            });
        select_anchor_probe(
            SelectAnchorId::TerminalCommandBar,
            bar,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn render_terminal_quick_commands_popover(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_quick_commands_popover(cx)
    }

    pub(in crate::workspace) fn render_terminal_broadcast_menu(
        &self,
        placement: TerminalBroadcastMenuPlacement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let entries = self.terminal_broadcast_entries();
        let active_pane_id = self.active_pane_id();
        let selectable = entries
            .iter()
            .filter(|(pane_id, _, _)| Some(*pane_id) != active_pane_id)
            .map(|(pane_id, _, _)| *pane_id)
            .collect::<Vec<_>>();
        let all_selected = !selectable.is_empty()
            && selectable
                .iter()
                .all(|pane_id| self.terminal_broadcast_targets.contains(pane_id));
        let anchor_left = self
            .select_anchors
            .get(&SelectAnchorId::TerminalBroadcastMenu)
            .map(|anchor| {
                // Tauri uses Radix DropdownMenuContent with `align="end"`.
                // Align to the trigger instead of the workspace root, because
                // the AI sidebar changes the root width but not the terminal
                // command-bar button's visual anchor.
                terminal_broadcast_menu_left_for_trigger_right(f32::from(anchor.bounds.right()))
            });

        let mut menu = context_menu_event_boundary({
            let menu = div()
                .absolute()
                .w(px(TERMINAL_BROADCAST_MENU_WIDTH))
                .max_h(px(320.0))
                .overflow_hidden()
                .rounded(px(self.tokens.radii.lg))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgba((theme.bg_elevated << 8) | 0xf2))
                .shadow_lg()
                .p(px(6.0))
                .text_size(px(12.0));
            if let Some(left) = anchor_left {
                menu.left(px(left))
            } else {
                menu.right(px(12.0))
            }
        })
        .child(
            div()
                .px(px(6.0))
                .py(px(4.0))
                .text_size(px(11.0))
                .text_color(rgb(theme.text_muted))
                .child(self.i18n.t("terminal.broadcast.select_targets")),
        );
        menu = match placement {
            TerminalBroadcastMenuPlacement::Bottom(offset) => menu.bottom(px(offset)),
            TerminalBroadcastMenuPlacement::Top(offset) => menu.top(px(offset)),
        };

        if entries.len() <= 1 {
            menu = menu.child(
                div()
                    .px(px(8.0))
                    .py(px(12.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("terminal.broadcast.no_targets")),
            );
        } else {
            for (pane_id, label, kind) in entries {
                let is_current = Some(pane_id) == active_pane_id;
                let checked = self.terminal_broadcast_targets.contains(&pane_id);
                let badge = match kind {
                    TabKind::LocalTerminal => self.i18n.t("terminal.typeLocal"),
                    TabKind::SshTerminal => self.i18n.t("terminal.typeSsh"),
                    _ => String::new(),
                };
                let row_color = if is_current {
                    rgb(theme.text_muted)
                } else {
                    rgb(theme.text)
                };
                let row = div()
                    .h(px(30.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(8.0))
                    .rounded(px(self.tokens.radii.md))
                    .text_color(row_color)
                    .child(if checked {
                        Self::render_lucide_icon(LucideIcon::Check, 12.0, rgba(0xfb923cff))
                    } else if is_current {
                        div()
                            .size(px(12.0))
                            .rounded_full()
                            .bg(rgba(0xfb923cff))
                            .into_any_element()
                    } else {
                        div().size(px(12.0)).into_any_element()
                    })
                    .child(div().flex_1().truncate().child(label))
                    .when(!badge.is_empty(), |row| {
                        row.child(
                            div()
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(px(self.tokens.radii.md))
                                .text_size(px(10.0))
                                .text_color(rgb(theme.text_muted))
                                .bg(rgba((theme.bg_panel << 8) | 0x99))
                                .child(badge),
                        )
                    })
                    .when(is_current, |row| {
                        row.child(
                            div()
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(px(self.tokens.radii.md))
                                .text_size(px(10.0))
                                .text_color(rgba(0xfb923cff))
                                .bg(rgba(0xf9731626))
                                .child(self.i18n.t("terminal.broadcast.current")),
                        )
                    });
                // Broadcast rows are checkbox-style menu items. Keep current
                // pane disabled through the shared menu action guard.
                let row = self.render_terminal_broadcast_menu_action(
                    row,
                    is_current,
                    false,
                    Some(rgb(theme.bg_hover)),
                    move |this, _event, _window, _cx| {
                        if this.terminal_broadcast_targets.remove(&pane_id) {
                            if this.terminal_broadcast_targets.is_empty() {
                                this.terminal_broadcast_enabled = false;
                            }
                        } else {
                            this.terminal_broadcast_targets.insert(pane_id);
                            this.terminal_broadcast_enabled = true;
                        }
                        this.keep_terminal_broadcast_menu_open();
                    },
                    cx,
                );
                menu = menu.child(row);
            }

            let select_all_disabled = selectable.is_empty();
            let select_all_label = div()
                .text_size(px(11.0))
                .text_color(rgb(theme.text_muted))
                .child(if all_selected {
                    self.i18n.t("terminal.broadcast.deselect_all")
                } else {
                    self.i18n.t("terminal.broadcast.select_all")
                });
            menu = menu.child(
                div()
                    .mt(px(4.0))
                    .pt(px(6.0))
                    .border_t_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(6.0))
                    .child(self.workspace_context_menu_persistent_styled_action(
                        select_all_label,
                        select_all_disabled,
                        false,
                        ContextMenuActionableStyle {
                            hover_background: None,
                            hover_text_color: Some(rgb(theme.accent)),
                        },
                        move |this, _event, _window, _cx| {
                            if all_selected {
                                this.terminal_broadcast_enabled = false;
                                this.terminal_broadcast_targets.clear();
                            } else {
                                this.terminal_broadcast_targets =
                                    selectable.iter().copied().collect();
                                this.terminal_broadcast_enabled =
                                    !this.terminal_broadcast_targets.is_empty();
                            }
                            this.keep_terminal_broadcast_menu_open();
                        },
                        cx,
                    ))
                    .when(self.terminal_broadcast_enabled, |footer| {
                        footer.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgba(0xfb923cff))
                                .child(self.i18n.t("terminal.broadcast.target_count")),
                        )
                    }),
            );
        }

        menu.into_any_element()
    }

    fn render_terminal_broadcast_menu_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        hover_bg: Option<gpui::Rgba>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Tauri broadcast target rows are Radix menu items with a disabled
        // current-terminal row. Keep native hover/cursor and action blocking
        // coupled to the shared context-menu guard.
        // Persistent menu rows still use one shared cx.listener wrapper so
        // toggling targets cannot re-enter WorkspaceApp during the click.
        self.workspace_context_menu_persistent_styled_action(
            item,
            disabled,
            loading,
            ContextMenuActionableStyle {
                hover_background: hover_bg,
                hover_text_color: None,
            },
            listener,
            cx,
        )
    }
}

fn terminal_broadcast_menu_left_for_trigger_right(trigger_right: f32) -> f32 {
    (trigger_right - TERMINAL_BROADCAST_MENU_WIDTH).max(12.0)
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
        let auto_route = NodeOrigin::AutoRoute {
            target_host: "db.internal".to_string(),
            route_id: "route-1".to_string(),
            hop_index: 0,
        };

        assert_eq!(saved_ssh_privilege_scope_id(None, Some(&direct)), None);
        assert_eq!(saved_ssh_privilege_scope_id(None, Some(&auto_route)), None);
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
