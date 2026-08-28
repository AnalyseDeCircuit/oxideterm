// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only
// Hallmark · component: source-control workbench · genre: modern-minimal
// Hallmark · pre-emit critique: P5 H5 E5 S5 R5 V4

use super::*;

#[derive(Clone, Copy)]
enum TerminalGitPathGroup {
    Conflict,
    Staged,
    Worktree,
}

impl TerminalGitPathGroup {
    fn label_key(self) -> &'static str {
        match self {
            Self::Conflict => "terminal.git.group_conflicts",
            Self::Staged => "terminal.git.group_staged",
            Self::Worktree => "terminal.git.group_modified",
        }
    }

    fn icon(self) -> LucideIcon {
        match self {
            Self::Conflict => LucideIcon::AlertTriangle,
            Self::Staged => LucideIcon::CheckCircle,
            Self::Worktree => LucideIcon::Pencil,
        }
    }

    fn color(self) -> Rgba {
        match self {
            Self::Conflict => rgba(0xf87171ff),
            Self::Staged => rgba(0x86efacff),
            Self::Worktree => rgba(0xfbbf24ff),
        }
    }

    fn bulk_action(self) -> Option<TerminalGitRepositoryAction> {
        match self {
            Self::Conflict => None,
            Self::Staged => Some(TerminalGitRepositoryAction::UnstageAll),
            Self::Worktree => Some(TerminalGitRepositoryAction::StageAll),
        }
    }
}

#[derive(Clone, Copy)]
enum TerminalGitPathState {
    Conflict,
    Staged,
    Modified,
    Untracked,
}

impl TerminalGitPathState {
    fn from_path(path: &GitChangedPath, group: TerminalGitPathGroup) -> Self {
        match group {
            TerminalGitPathGroup::Conflict => Self::Conflict,
            TerminalGitPathGroup::Staged => Self::Staged,
            TerminalGitPathGroup::Worktree if path.untracked() => Self::Untracked,
            TerminalGitPathGroup::Worktree => Self::Modified,
        }
    }

    fn label_key(self) -> &'static str {
        match self {
            Self::Conflict => "terminal.git.path_state_conflict",
            Self::Staged => "terminal.git.path_state_staged",
            Self::Modified => "terminal.git.path_state_modified",
            Self::Untracked => "terminal.git.path_state_untracked",
        }
    }

    fn color(self) -> Rgba {
        match self {
            Self::Conflict => rgba(0xf87171ff),
            Self::Staged => rgba(0x86efacff),
            Self::Modified => rgba(0xfbbf24ff),
            Self::Untracked => rgba(0x67e8f9ff),
        }
    }

    fn primary_action(self) -> TerminalGitPathAction {
        match self {
            Self::Staged => TerminalGitPathAction::DiffStaged,
            Self::Modified => TerminalGitPathAction::Diff,
            Self::Conflict | Self::Untracked => TerminalGitPathAction::Open,
        }
    }

    fn row_actions(self) -> &'static [TerminalGitPathAction] {
        match self {
            Self::Conflict => &[
                TerminalGitPathAction::Ours,
                TerminalGitPathAction::Theirs,
                TerminalGitPathAction::Stage,
            ],
            Self::Staged => &[TerminalGitPathAction::Unstage],
            Self::Modified | Self::Untracked => &[TerminalGitPathAction::Stage],
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct TerminalGitPathLabel<'a> {
    file_name: &'a str,
    parent_path: Option<&'a str>,
}

fn terminal_git_path_label(path: &str) -> TerminalGitPathLabel<'_> {
    let path_without_trailing_separator = path.trim_end_matches('/').trim_end_matches('\\');
    let normalized_path = if path_without_trailing_separator.is_empty() {
        path
    } else {
        path_without_trailing_separator
    };
    let separator_index = match (normalized_path.rfind('/'), normalized_path.rfind('\\')) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(index), None) | (None, Some(index)) => Some(index),
        (None, None) => None,
    };

    match separator_index {
        Some(index) if index + 1 < normalized_path.len() => TerminalGitPathLabel {
            file_name: &normalized_path[index + 1..],
            parent_path: (index > 0).then_some(&normalized_path[..index]),
        },
        _ => TerminalGitPathLabel {
            file_name: normalized_path,
            parent_path: None,
        },
    }
}

fn terminal_git_path_action_icon(action: TerminalGitPathAction) -> LucideIcon {
    match action {
        TerminalGitPathAction::Stage => LucideIcon::Plus,
        TerminalGitPathAction::Unstage => LucideIcon::RotateCcw,
        TerminalGitPathAction::Diff | TerminalGitPathAction::DiffStaged => LucideIcon::FileText,
        TerminalGitPathAction::Open => LucideIcon::ExternalLink,
        TerminalGitPathAction::Ours => LucideIcon::ChevronLeft,
        TerminalGitPathAction::Theirs => LucideIcon::ChevronRight,
    }
}

impl WorkspaceApp {
    pub(super) fn render_terminal_git_branch_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let left = self.terminal_git_branch_picker_left();
        let snapshot = self.active_terminal_git_snapshot(cx);
        let operation = snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.status.operation());
        let active_section = match (self.terminal.read(cx).git_panel_active_section(), operation) {
            (TerminalGitPanelSection::Resolve, None) => TerminalGitPanelSection::Changes,
            (section, _) => section,
        };

        let mut panel = context_menu_pointer_event_boundary(
            command_panel(
                &self.tokens,
                CommandPanelOptions::new()
                    .width(TERMINAL_GIT_BRANCH_MENU_WIDTH)
                    .max_width_ratio(0.96)
                    .terminal_owned(),
            )
            .absolute()
            // The retired command input no longer adds a second row below the toolbar.
            .bottom(px(TERMINAL_COMMAND_TOOLBAR_HEIGHT))
            .left(px(left))
            .occlude()
            .text_size(px(12.0))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
                cx.stop_propagation();
            }),
        );

        if let Some(snapshot) = snapshot {
            panel = panel.child(self.render_terminal_git_context_header(snapshot));
        }
        panel = panel.child(self.render_terminal_git_section_tabs(active_section, operation, cx));

        let section = match active_section {
            TerminalGitPanelSection::Branches => self.render_terminal_git_branches_section(cx),
            TerminalGitPanelSection::Changes => self.render_terminal_git_changes_section(cx),
            TerminalGitPanelSection::History => self.render_terminal_git_history_section(cx),
            TerminalGitPanelSection::More => self.render_terminal_git_more_section(cx),
            TerminalGitPanelSection::Resolve => {
                self.render_terminal_git_resolve_section(operation, cx)
            }
        };
        panel = panel.child(
            command_panel_body(&self.tokens)
                // GPUI scroll containers do not contribute a reliable intrinsic
                // flex height, so the panel body owns an explicit stable viewport.
                .h(px(TERMINAL_GIT_BRANCH_MENU_BODY_HEIGHT))
                .min_h(px(0.0))
                .max_h(px(TERMINAL_GIT_BRANCH_MENU_BODY_MAX_HEIGHT))
                .overflow_y_scrollbar()
                .child(section),
        );

        panel.into_any_element()
    }

    pub(super) fn render_terminal_git_context_header(
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
        let repository_name = terminal_git_path_label(&repo_root).file_name.to_string();
        let status = snapshot.status;
        let mut metrics = div()
            .flex_none()
            .max_w(px(210.0))
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
                StatusTone::Success,
            ));
        }
        if status.modified() > 0 {
            metrics = metrics.child(self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_modified",
                status.modified(),
                StatusTone::Warning,
            ));
        }
        if status.untracked() > 0 {
            metrics = metrics.child(self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_untracked",
                status.untracked(),
                StatusTone::Info,
            ));
        }
        if status.conflicts() > 0 {
            metrics = metrics.child(self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_conflict",
                status.conflicts(),
                StatusTone::Error,
            ));
        }

        // Keep repository identity on one compact line. The active terminal
        // already exposes the full CWD, so the SCM surface prioritizes the
        // repository name and branch like a dedicated source-control sidebar.
        div()
            .min_h(px(38.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x52))
            .child(Self::render_lucide_icon(
                LucideIcon::FolderOpen,
                13.0,
                rgb(theme.text_muted),
            ))
            .child(
                div()
                    .max_w(px(150.0))
                    .truncate()
                    .text_size(px(11.0))
                    .font_family(self.terminal_git_mono_font())
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(repository_name),
            )
            .child(Self::render_lucide_icon(
                LucideIcon::GitFork,
                11.0,
                rgb(theme.text_muted),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(10.0))
                    .font_family(self.terminal_git_mono_font())
                    .text_color(rgb(theme.accent))
                    .child(branch_label),
            )
            .child(metrics)
            .into_any_element()
    }

    pub(super) fn render_terminal_git_section_tabs(
        &self,
        active_section: TerminalGitPanelSection,
        operation: Option<oxideterm_environment::GitOperationKind>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sections = [
            TerminalGitPanelSection::Changes,
            TerminalGitPanelSection::Branches,
            TerminalGitPanelSection::History,
            TerminalGitPanelSection::More,
        ];
        let mut tabs = div()
            .min_h(px(36.0))
            .px(px(4.0))
            .py(px(4.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .border_b_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x40));
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

    pub(super) fn render_terminal_git_section_tab(
        &self,
        section: TerminalGitPanelSection,
        active_section: TerminalGitPanelSection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = section == active_section;
        let label = self.i18n.t(section.label_key());
        let icon = terminal_git_section_icon(section);
        let chip_options = ActionChipOptions::new()
            .active(active)
            .height(26.0)
            .padding_x(7.0)
            .font_size(10.0)
            .radius(ButtonRadius::Sm)
            .idle_text_tone(ActionChipTextTone::Muted);
        let foreground = action_chip_foreground(&self.tokens, chip_options);
        action_chip(
            &self.tokens,
            label,
            Some(Self::render_lucide_icon(icon, 12.0, foreground)),
            chip_options,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.terminal.update(cx, |terminal, _cx| {
                    terminal.set_git_panel_active_section(section);
                });
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    pub(super) fn render_terminal_git_branches_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let visible_branches = self.visible_terminal_git_branches(cx);
        let query_checkout_candidate = self.terminal_git_query_checkout_candidate(cx);
        let query_create_candidate = self.terminal_git_query_create_branch_candidate(cx);
        let query_remote_tracking_candidate = self.terminal_git_query_remote_tracking_candidate(cx);
        let query_rebase_candidate = self.terminal_git_query_rebase_candidate(cx);
        let loading = self.terminal.read(cx).git_panel_loading();
        let error = self
            .terminal
            .read(cx)
            .git_panel_error()
            .map(|error| self.terminal_git_branch_error_message(error));

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
            if let Some(branch_name) = query_create_candidate {
                section = section.child(
                    self.render_terminal_git_query_create_branch_row(branch_name.clone(), cx),
                );
                section = section
                    .child(self.render_terminal_git_query_rename_branch_row(branch_name, cx));
            }
            if let Some(branch_name) = query_remote_tracking_candidate {
                section =
                    section.child(self.render_terminal_git_query_track_remote_row(branch_name, cx));
            }
            if let Some(branch_name) = query_rebase_candidate {
                section = section.child(self.render_terminal_git_query_rebase_row(branch_name, cx));
            }
            if !has_visible_branches && query_checkout_candidate.is_none() {
                section = section.child(self.render_terminal_git_branch_message(
                    LucideIcon::Search,
                    self.i18n.t("terminal.git.no_branches"),
                ));
            }
            let mut list = div().flex().flex_col().gap(px(2.0));
            for branch in visible_branches {
                list = list.child(self.render_terminal_git_branch_row(branch, cx));
            }
            if has_visible_branches {
                section = section.child(list);
            }
        }

        section.into_any_element()
    }

    pub(super) fn render_terminal_git_changes_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let (changed_paths, staged_count) = self
            .active_terminal_git_snapshot(cx)
            .map(|snapshot| {
                let status = snapshot.status;
                (status.paths().to_vec(), status.staged())
            })
            .unwrap_or_default();
        let mut section = div().flex().flex_col().gap(px(8.0));

        section = section.child(self.render_terminal_git_commit_controls(staged_count > 0, cx));

        if changed_paths.is_empty() {
            section = section.child(self.render_terminal_git_clean_changes_state());
        } else {
            section = section.child(self.render_terminal_git_path_list(changed_paths, cx));
        }
        section.into_any_element()
    }

    fn render_terminal_git_commit_controls(
        &self,
        has_staged_changes: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let commit_label = self.i18n.t(terminal_git_repository_action_label_key(
            TerminalGitRepositoryAction::Commit,
        ));
        let commit_enabled =
            has_staged_changes && self.terminal.read(cx).git_commit_message_ready();
        let commit_options = ActionChipOptions::new()
            .active(commit_enabled)
            .disabled(!commit_enabled)
            .height(32.0)
            .radius(ButtonRadius::Sm)
            .idle_text_tone(ActionChipTextTone::Primary);
        let commit_foreground = action_chip_foreground(&self.tokens, commit_options);
        let commit_button = action_chip(
            &self.tokens,
            commit_label,
            Some(Self::render_lucide_icon(
                LucideIcon::Check,
                13.0,
                commit_foreground,
            )),
            commit_options,
        )
        .flex_1()
        .when(commit_enabled, |button| {
            button.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.commit_terminal_git_message(cx);
                    cx.stop_propagation();
                }),
            )
        });

        let message_row = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(self.render_terminal_git_commit_message_input(cx))
            .child(self.render_terminal_git_ai_commit_action_row(cx));
        let ai_error = self
            .terminal
            .read(cx)
            .git_ai_commit_error()
            .map(|error| self.terminal_git_ai_commit_error_message(error));

        div()
            .px(px(4.0))
            .pb(px(8.0))
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x52))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(message_row)
            .child(commit_button)
            .when_some(ai_error, |controls, error| {
                controls.child(
                    div()
                        .px(px(4.0))
                        .truncate()
                        .text_size(px(10.0))
                        .text_color(rgba(0xfca5a5ff))
                        .child(error),
                )
            })
            .into_any_element()
    }

    fn render_terminal_git_commit_message_input(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::TerminalGitCommitMessage;
        let selected_range = self.ime_selected_range_for_target(target, cx);
        let marked_text = self.marked_text_for_target(target, cx);
        let terminal = self.terminal.read(cx);
        self.text_input_with_workspace_ime(
            target,
            text_input(
                &self.tokens,
                TextInputView {
                    value: terminal.git_commit_message(),
                    placeholder: self.i18n.t("terminal.git.commit_message_placeholder"),
                    focused: terminal.git_panel_open()
                        && terminal.git_panel_active_section() == TerminalGitPanelSection::Changes,
                    caret_visible: self.input_caret.visible(),
                    secret: false,
                    selected_all: false,
                    selected_range,
                    marked_text,
                },
            )
            .h(px(32.0))
            .flex_1(),
            |_this, _cx| {},
            cx,
        )
        .into_any_element()
    }

    pub(super) fn render_terminal_git_clean_changes_state(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .min_h(px(96.0))
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

    pub(super) fn render_terminal_git_more_section(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.render_terminal_git_labeled_action_group(
                "terminal.git.section_sync",
                LucideIcon::RefreshCw,
                &[
                    TerminalGitRepositoryAction::Fetch,
                    TerminalGitRepositoryAction::Pull,
                    TerminalGitRepositoryAction::RebasePull,
                    TerminalGitRepositoryAction::Push,
                    TerminalGitRepositoryAction::Publish,
                    TerminalGitRepositoryAction::FetchAll,
                    TerminalGitRepositoryAction::PushTags,
                ],
                cx,
            ))
            .child(self.render_terminal_git_labeled_action_group(
                "terminal.git.section_stash",
                LucideIcon::Archive,
                &[
                    TerminalGitRepositoryAction::Stash,
                    TerminalGitRepositoryAction::StashList,
                    TerminalGitRepositoryAction::StashShowLatest,
                    TerminalGitRepositoryAction::StashPop,
                    TerminalGitRepositoryAction::StashApplyLatest,
                    TerminalGitRepositoryAction::StashDropLatest,
                ],
                cx,
            ))
            .child(self.render_terminal_git_labeled_action_group(
                "terminal.git.group_repository",
                LucideIcon::ListTree,
                &[
                    TerminalGitRepositoryAction::BranchVerbose,
                    TerminalGitRepositoryAction::RemoteList,
                    TerminalGitRepositoryAction::TagList,
                    TerminalGitRepositoryAction::WorktreeList,
                ],
                cx,
            ))
            .child(self.render_terminal_git_labeled_action_group(
                "terminal.git.group_advanced",
                LucideIcon::CheckCircle,
                &[
                    TerminalGitRepositoryAction::CommitVerbose,
                    TerminalGitRepositoryAction::CommitSignoff,
                    TerminalGitRepositoryAction::Amend,
                    TerminalGitRepositoryAction::AmendNoEdit,
                    TerminalGitRepositoryAction::RebaseInteractive,
                ],
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_terminal_git_labeled_action_group(
        &self,
        label_key: &'static str,
        icon: LucideIcon,
        actions: &[TerminalGitRepositoryAction],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut list = div().flex().flex_col().gap(px(2.0));
        for action in actions {
            list = list.child(self.render_terminal_git_action_row(*action, cx));
        }

        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .px(px(4.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(11.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_muted))
                    .child(Self::render_lucide_icon(icon, 12.0, rgb(theme.text_muted)))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.render_terminal_git_plain_panel(list))
            .into_any_element()
    }

    pub(super) fn render_terminal_git_history_section(&self, cx: &mut Context<Self>) -> AnyElement {
        self.render_terminal_git_action_section(
            &[
                TerminalGitRepositoryAction::Log,
                TerminalGitRepositoryAction::LogStat,
                TerminalGitRepositoryAction::Reflog,
            ],
            cx,
        )
    }

    pub(super) fn render_terminal_git_resolve_section(
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

    pub(super) fn render_terminal_git_action_section(
        &self,
        actions: &[TerminalGitRepositoryAction],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div().flex().flex_col().gap(px(2.0));
        for action in actions {
            list = list.child(self.render_terminal_git_action_row(*action, cx));
        }
        self.render_terminal_git_plain_panel(list)
    }

    pub(super) fn render_terminal_git_plain_panel(&self, list: gpui::Div) -> AnyElement {
        div()
            .min_h(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
            .p(px(4.0))
            .child(list)
            .into_any_element()
    }

    pub(super) fn render_terminal_git_query_checkout_row(
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

    pub(super) fn render_terminal_git_query_rebase_row(
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

    pub(super) fn render_terminal_git_query_create_branch_row(
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

    pub(super) fn render_terminal_git_query_rename_branch_row(
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

    pub(super) fn render_terminal_git_query_track_remote_row(
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

    pub(super) fn render_terminal_git_path_list(
        &self,
        paths: Vec<GitChangedPath>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let paths = paths.into_iter().take(80).collect::<Vec<_>>();
        let mut list = div().flex().flex_col().gap(px(8.0));
        list = self.append_terminal_git_path_group(
            list,
            TerminalGitPathGroup::Conflict,
            paths.iter().filter(|path| path.conflict()).cloned(),
            cx,
        );
        list = self.append_terminal_git_path_group(
            list,
            TerminalGitPathGroup::Staged,
            paths
                .iter()
                .filter(|path| path.staged() && !path.conflict())
                .cloned(),
            cx,
        );
        list = self.append_terminal_git_path_group(
            list,
            TerminalGitPathGroup::Worktree,
            paths
                .iter()
                .filter(|path| (path.modified() || path.untracked()) && !path.conflict())
                .cloned(),
            cx,
        );
        list.into_any_element()
    }

    fn append_terminal_git_path_group(
        &self,
        list: gpui::Div,
        group: TerminalGitPathGroup,
        paths: impl Iterator<Item = GitChangedPath>,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let paths = paths.collect::<Vec<_>>();
        if paths.is_empty() {
            return list;
        }

        let group_label = self.i18n.t(group.label_key());
        let mut header = div()
            .h(px(28.0))
            .px(px(6.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .border_b_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x40))
            .text_size(px(11.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child(Self::render_lucide_icon(group.icon(), 12.0, group.color()))
            .child(div().flex_1().min_w(px(0.0)).child(group_label.clone()));
        if let Some(action) = group.bulk_action() {
            header = header.child(self.render_terminal_git_repository_icon_action_button(
                action,
                group_label,
                cx,
            ));
        }
        header =
            header.child(self.render_terminal_git_group_count(paths.len() as u32, group.color()));

        // One containment level is enough: section headers own the grouping,
        // while rows stay flat and dense like a source-control sidebar.
        let mut group_rows = div().flex().flex_col();
        for path in paths {
            group_rows = group_rows.child(self.render_terminal_git_path_row(path, group, cx));
        }
        list.child(div().flex().flex_col().child(header).child(group_rows))
    }

    fn render_terminal_git_path_row(
        &self,
        path: GitChangedPath,
        group: TerminalGitPathGroup,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let path_value = path.path().to_string();
        let display_path = terminal_git_path_label(&path_value);
        let file_name = display_path.file_name.to_string();
        let parent_path = display_path.parent_path.map(str::to_string);
        let original_path = path.original_path().map(str::to_string);
        let state = TerminalGitPathState::from_path(&path, group);
        let primary_action = state.primary_action();
        let primary_path = path_value.clone();
        let mut row = div()
            .h(px(30.0))
            .px(px(6.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(theme.bg_hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.run_terminal_git_path_action(primary_action, primary_path.clone(), cx);
                    cx.stop_propagation();
                }),
            )
            .child(Self::render_lucide_icon(
                if matches!(state, TerminalGitPathState::Conflict) {
                    LucideIcon::AlertTriangle
                } else {
                    LucideIcon::FileCode
                },
                12.0,
                if matches!(state, TerminalGitPathState::Conflict) {
                    state.color()
                } else {
                    rgb(theme.text_muted)
                },
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .overflow_hidden()
                    .child(
                        div()
                            .flex_none()
                            .max_w(relative(0.62))
                            .truncate()
                            .text_size(px(11.0))
                            .font_family(self.terminal_git_mono_font())
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(file_name),
                    )
                    .when_some(parent_path, |identity, parent_path| {
                        identity.child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .truncate()
                                .text_size(px(10.0))
                                .font_family(self.terminal_git_mono_font())
                                .text_color(rgb(theme.text_muted))
                                .child(parent_path),
                        )
                    })
                    .when_some(original_path, |identity, original_path| {
                        identity.child(
                            div()
                                .flex_none()
                                .max_w(px(100.0))
                                .truncate()
                                .text_size(px(9.0))
                                .font_family(self.terminal_git_mono_font())
                                .text_color(rgb(theme.text_muted))
                                .child(format!("← {original_path}")),
                        )
                    }),
            )
            .child(self.render_terminal_git_path_state(state));

        let mut actions = div().flex().items_center().gap(px(2.0));
        for action in state.row_actions() {
            actions = actions.child(self.render_terminal_git_path_action_button(
                *action,
                path_value.clone(),
                cx,
            ));
        }
        row = row.child(actions);
        row.into_any_element()
    }

    fn render_terminal_git_path_state(&self, state: TerminalGitPathState) -> AnyElement {
        div()
            .w(px(18.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .font_family(self.terminal_git_mono_font())
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_size(px(10.0))
            .text_color(state.color())
            .child(self.i18n.t(state.label_key()))
            .into_any_element()
    }

    fn render_terminal_git_group_count(&self, count: u32, color: Rgba) -> AnyElement {
        div()
            .h(px(18.0))
            .min_w(px(18.0))
            .flex_none()
            .rounded(px(self.tokens.radii.sm))
            .px(px(5.0))
            .flex()
            .items_center()
            .justify_center()
            .font_family(self.terminal_git_mono_font())
            .text_size(px(10.0))
            .text_color(color)
            .bg(rgba(0x00000026))
            .child(count.to_string())
            .into_any_element()
    }

    fn render_terminal_git_repository_icon_action_button(
        &self,
        action: TerminalGitRepositoryAction,
        scope_label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let action_label = self
            .i18n
            .t(terminal_git_repository_action_label_key(action));
        let tooltip = format!("{action_label} · {scope_label}");
        self.workspace_tooltip_icon_button(
            terminal_git_action_icon(action),
            12.0,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions {
                idle_opacity: 0.78,
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(22.0, ButtonRadius::Sm)
            },
            tooltip,
            "terminal-git-group-action",
            true,
            cx.listener(move |this, _event, _window, cx| {
                this.run_terminal_git_repository_action(action, cx);
                cx.stop_propagation();
            }),
            cx.entity(),
        )
    }

    fn render_terminal_git_path_action_button(
        &self,
        action: TerminalGitPathAction,
        path: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let action_label = self.i18n.t(terminal_git_path_action_label_key(action));
        let tooltip = format!("{action_label} · {path}");
        self.workspace_tooltip_icon_button(
            terminal_git_path_action_icon(action),
            12.0,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions {
                idle_opacity: 0.72,
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(22.0, ButtonRadius::Sm)
            },
            tooltip,
            "terminal-git-path-action",
            true,
            cx.listener(move |this, _event, _window, cx| {
                this.run_terminal_git_path_action(action, path.clone(), cx);
                cx.stop_propagation();
            }),
            cx.entity(),
        )
    }

    pub(super) fn render_terminal_git_query_command_row(
        &self,
        branch_name: String,
        label_key: &'static str,
        icon: LucideIcon,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self.i18n_replace(label_key, &[("branch", branch_name)]);

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

    pub(super) fn render_terminal_git_action_row(
        &self,
        action: TerminalGitRepositoryAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self
            .i18n
            .t(terminal_git_repository_action_label_key(action));
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

    pub(super) fn render_terminal_git_action_summary(
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
                let count = self.terminal.read(cx).git_worktree_branch_count();
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

    pub(super) fn render_terminal_git_sync_summary(
        &self,
        status: &GitRepositoryStatus,
    ) -> Option<AnyElement> {
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

    pub(super) fn render_terminal_git_change_count_chips(
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
                StatusTone::Success,
            ));
        }
        if status.modified() > 0 {
            has_result = true;
            chips = chips.child(self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_modified",
                status.modified(),
                StatusTone::Warning,
            ));
        }
        if status.untracked() > 0 {
            has_result = true;
            chips = chips.child(self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_untracked",
                status.untracked(),
                StatusTone::Info,
            ));
        }
        if status.conflicts() > 0 {
            has_result = true;
            chips = chips.child(self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_conflict",
                status.conflicts(),
                StatusTone::Error,
            ));
        }
        has_result.then(|| chips.into_any_element())
    }

    pub(super) fn render_terminal_git_staged_count_chip(
        &self,
        status: &GitRepositoryStatus,
    ) -> Option<AnyElement> {
        (status.staged() > 0).then(|| {
            self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_staged",
                status.staged(),
                StatusTone::Success,
            )
        })
    }

    pub(super) fn render_terminal_git_conflict_count_chip(
        &self,
        status: &GitRepositoryStatus,
    ) -> Option<AnyElement> {
        (status.conflicts() > 0).then(|| {
            self.render_terminal_git_label_count_chip(
                "terminal.git.path_state_conflict",
                status.conflicts(),
                StatusTone::Error,
            )
        })
    }

    pub(super) fn render_terminal_git_data_hint(&self, text: String) -> AnyElement {
        self.render_terminal_git_data_hint_with_width(text, 160.0)
    }

    pub(super) fn render_terminal_git_data_hint_with_width(
        &self,
        text: String,
        max_width: f32,
    ) -> AnyElement {
        monospace_datum(
            &self.tokens,
            text,
            Some(self.terminal_git_mono_font()),
            MonospaceDatumOptions::new(MonospaceDatumTone::Muted).text_size(11.0),
        )
        .max_w(px(max_width))
        .into_any_element()
    }

    pub(super) fn terminal_git_mono_font(&self) -> gpui::SharedString {
        settings_mono_font_family(self.settings_store.settings())
    }

    pub(super) fn render_terminal_git_label_count_chip(
        &self,
        label_key: &'static str,
        count: u32,
        tone: StatusTone,
    ) -> AnyElement {
        status_pill(
            &self.tokens,
            format!("{} {}", self.i18n.t(label_key), count),
            StatusPillOptions::new(tone).compact(),
        )
        .into_any_element()
    }

    pub(super) fn render_terminal_git_icon_count_chip(
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

    pub(super) fn render_terminal_git_ai_commit_action_row(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let loading = self.terminal.read(cx).git_ai_commit_loading();
        let label = if loading {
            self.i18n.t("terminal.git.ai_commit_generating")
        } else {
            self.i18n.t("terminal.git.action_ai_commit_message")
        };
        let has_error = self.terminal.read(cx).git_ai_commit_error().is_some();
        let options = ActionChipOptions::new()
            .disabled(loading)
            .height(32.0)
            .radius(ButtonRadius::Sm)
            .idle_text_tone(ActionChipTextTone::Primary)
            .hover_border_accent(true);
        let foreground = if has_error {
            rgba(0xfca5a5ff)
        } else {
            action_chip_foreground(&self.tokens, options)
        };
        action_chip(
            &self.tokens,
            label,
            Some(Self::render_lucide_icon(
                LucideIcon::Sparkles,
                12.0,
                foreground,
            )),
            options,
        )
        .when(!loading, |button| {
            button.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.generate_terminal_git_ai_commit_message(cx);
                    cx.stop_propagation();
                }),
            )
        })
        .into_any_element()
    }

    pub(super) fn render_terminal_git_branch_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::TerminalGitBranchSearch;
        let selected_range = self.ime_selected_range_for_target(target, cx);
        let marked_text = self.marked_text_for_target(target, cx);
        let terminal = self.terminal.read(cx);
        self.text_input_with_workspace_ime(
            target,
            text_input(
                &self.tokens,
                TextInputView {
                    value: terminal.git_panel_query(),
                    placeholder: self.i18n.t("terminal.git.search_branches"),
                    focused: terminal.git_panel_open(),
                    caret_visible: self.input_caret.visible(),
                    secret: false,
                    selected_all: false,
                    selected_range,
                    marked_text,
                },
            )
            .h(px(32.0)),
            |_this, _cx| {},
            cx,
        )
        .into_any_element()
    }

    pub(super) fn render_terminal_git_branch_row(
        &self,
        branch: oxideterm_environment::GitBranchReference,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let branch_name = branch.name().to_string();
        let highlighted = self.terminal.read(cx).git_branch_highlighted(branch.name());
        let current = branch.current();
        let worktree_path = branch.worktree_path().map(str::to_string);
        let linked_worktree = worktree_path.is_some() && !current;
        let mut branch_identity = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(1.0))
            .child(monospace_datum(
                &self.tokens,
                branch_name.clone(),
                Some(self.terminal_git_mono_font()),
                MonospaceDatumOptions::new(if current {
                    MonospaceDatumTone::Accent
                } else {
                    MonospaceDatumTone::Primary
                }),
            ));
        if let Some(worktree_path) = worktree_path {
            branch_identity = branch_identity.child(monospace_datum(
                &self.tokens,
                worktree_path,
                Some(self.terminal_git_mono_font()),
                MonospaceDatumOptions::new(MonospaceDatumTone::Muted).text_size(10.0),
            ));
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
                let branch_name = branch_name;
                move |this, _event: &MouseMoveEvent, _window, cx| {
                    if this.terminal.update(cx, |terminal, _cx| {
                        terminal.set_git_branch_highlight(&branch_name)
                    }) {
                        cx.notify();
                    }
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let branch = branch;
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

    pub(super) fn render_terminal_git_branch_message(
        &self,
        icon: LucideIcon,
        message: String,
    ) -> AnyElement {
        div()
            .min_h(px(56.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(if matches!(icon, LucideIcon::LoaderCircle) {
                self.render_loading_icon(
                    "terminal-git-branches-loading",
                    14.0,
                    rgb(self.tokens.ui.text_muted),
                )
            } else {
                Self::render_lucide_icon(icon, 14.0, rgb(self.tokens.ui.text_muted))
            })
            .child(message)
            .into_any_element()
    }

    pub(super) fn render_terminal_git_branch_error(&self, message: String) -> AnyElement {
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

    fn terminal_git_branch_error_message(&self, error: &TerminalGitBranchError) -> String {
        match error {
            TerminalGitBranchError::NotRepository => {
                self.i18n.t("terminal.git.branch_not_repository")
            }
            TerminalGitBranchError::GitUnavailable => {
                self.i18n.t("terminal.git.branch_git_unavailable")
            }
            TerminalGitBranchError::CwdUnavailable => {
                self.i18n.t("terminal.git.branch_cwd_unavailable")
            }
            TerminalGitBranchError::NodeUnavailable => {
                self.i18n.t("terminal.git.branch_node_unavailable")
            }
            TerminalGitBranchError::Message(message) => message.clone(),
        }
    }

    fn terminal_git_ai_commit_error_message(&self, error: &TerminalGitAiCommitError) -> String {
        match error {
            TerminalGitAiCommitError::NoStagedChanges => {
                self.i18n.t("terminal.git.ai_commit_no_staged_changes")
            }
            TerminalGitAiCommitError::NotRepository => {
                self.i18n.t("terminal.git.ai_commit_not_repository")
            }
            TerminalGitAiCommitError::GitUnavailable => {
                self.i18n.t("terminal.git.ai_commit_git_unavailable")
            }
            TerminalGitAiCommitError::CwdUnavailable => {
                self.i18n.t("terminal.git.ai_commit_cwd_unavailable")
            }
            TerminalGitAiCommitError::NodeUnavailable => {
                self.i18n.t("terminal.git.ai_commit_node_unavailable")
            }
            TerminalGitAiCommitError::InvalidMessage => {
                self.i18n.t("terminal.git.ai_commit_failed")
            }
            TerminalGitAiCommitError::Message(message) => message.clone(),
        }
    }

    pub(super) fn terminal_git_branch_picker_left(&self) -> f32 {
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

    pub(super) fn terminal_cwd_picker_left(&self) -> f32 {
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

    pub(super) fn terminal_project_panel_left(&self) -> f32 {
        let Some(chip) = self
            .select_anchors
            .get(&SelectAnchorId::TerminalProjectMenu)
        else {
            return TERMINAL_PROJECT_MENU_MARGIN;
        };
        let Some(bar) = self.select_anchors.get(&SelectAnchorId::TerminalCommandBar) else {
            return TERMINAL_PROJECT_MENU_MARGIN;
        };
        let bar_width = f32::from(bar.bounds.size.width);
        let desired = f32::from(chip.bounds.left() - bar.bounds.left());
        let max_left = (bar_width - TERMINAL_PROJECT_MENU_WIDTH - TERMINAL_PROJECT_MENU_MARGIN)
            .max(TERMINAL_PROJECT_MENU_MARGIN);
        desired.clamp(TERMINAL_PROJECT_MENU_MARGIN, max_left)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_group_preserves_modified_and_untracked_status() {
        let modified =
            GitChangedPath::from_parts("src/lib.rs", None::<String>, false, true, false, false)
                .expect("modified path");
        let untracked =
            GitChangedPath::from_parts("src/new.rs", None::<String>, false, false, true, false)
                .expect("untracked path");

        assert!(matches!(
            TerminalGitPathState::from_path(&modified, TerminalGitPathGroup::Worktree),
            TerminalGitPathState::Modified
        ));
        assert!(matches!(
            TerminalGitPathState::from_path(&untracked, TerminalGitPathGroup::Worktree),
            TerminalGitPathState::Untracked
        ));
    }
}
