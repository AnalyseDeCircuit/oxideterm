// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(super) fn render_terminal_git_branch_picker(&self, cx: &mut Context<Self>) -> AnyElement {
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
            command_panel(
                &self.tokens,
                CommandPanelOptions::new()
                    .width(TERMINAL_GIT_BRANCH_MENU_WIDTH)
                    .max_width_ratio(0.96)
                    .terminal_owned(),
            )
            .absolute()
            .bottom(px(bottom))
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

        // Repository sampling redraws the panel without restarting its entrance.
        oxideterm_gpui_ui::motion::fade_in(
            &self.tokens,
            "terminal-git-branch-picker-enter",
            panel,
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        )
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

        entity_list_row(
            &self.tokens,
            EntityListRowOptions::new().compact(),
            Some(Self::render_lucide_icon(
                LucideIcon::FolderOpen,
                14.0,
                rgb(theme.text_muted),
            )),
            div()
                .truncate()
                .text_size(px(12.0))
                .font_family(self.terminal_git_mono_font())
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(theme.text))
                .child(repo_root)
                .into_any_element(),
            Some(
                div()
                    .truncate()
                    .text_size(px(11.0))
                    .font_family(self.terminal_git_mono_font())
                    .text_color(rgb(theme.accent))
                    .child(branch_label)
                    .into_any_element(),
            ),
            Vec::new(),
            vec![metrics.into_any_element()],
        )
        .into_any_element()
    }

    pub(super) fn render_terminal_git_section_tabs(
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
                this.terminal_git_branch_picker.active_section = section;
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

    pub(super) fn render_terminal_git_changes_section(&self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(super) fn render_terminal_git_clean_changes_state(&self) -> AnyElement {
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

    pub(super) fn render_terminal_git_action_toolbar(
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

    pub(super) fn render_terminal_git_toolbar_action_button(
        &self,
        action: TerminalGitRepositoryAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self
            .i18n
            .t(terminal_git_repository_action_label_key(action));
        let icon = terminal_git_action_icon(action);
        let chip_options = ActionChipOptions::new()
            .idle_text_tone(ActionChipTextTone::Primary)
            .hover_border_accent(true);
        action_chip(
            &self.tokens,
            label,
            Some(Self::render_lucide_icon(icon, 12.0, rgb(theme.text_muted))),
            chip_options,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.run_terminal_git_repository_action(action, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(super) fn render_terminal_git_commit_section(&self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(super) fn render_terminal_git_sync_section(&self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(super) fn render_terminal_git_stash_section(&self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(super) fn render_terminal_git_refs_section(&self, cx: &mut Context<Self>) -> AnyElement {
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
        self.render_terminal_git_action_panel(list)
    }

    pub(super) fn render_terminal_git_action_panel(&self, list: gpui::Div) -> AnyElement {
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

    pub(super) fn append_terminal_git_path_group(
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

    pub(super) fn render_terminal_git_path_row(
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
                    .child(monospace_datum(
                        &self.tokens,
                        path_label,
                        Some(self.terminal_git_mono_font()),
                        MonospaceDatumOptions::new(MonospaceDatumTone::Primary).strong(),
                    ))
                    .when(path.original_path().is_some(), |meta| {
                        meta.child(monospace_datum(
                            &self.tokens,
                            path.original_path().unwrap_or_default().to_string(),
                            Some(self.terminal_git_mono_font()),
                            MonospaceDatumOptions::new(MonospaceDatumTone::Muted).text_size(10.0),
                        ))
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

    pub(super) fn render_terminal_git_path_badges(&self, path: &GitChangedPath) -> AnyElement {
        let mut badges = div().flex().items_center().gap(px(4.0));
        if path.staged() {
            badges = badges.child(self.render_terminal_git_path_badge(
                "terminal.git.path_state_staged",
                StatusTone::Success,
            ));
        }
        if path.modified() {
            badges = badges.child(self.render_terminal_git_path_badge(
                "terminal.git.path_state_modified",
                StatusTone::Warning,
            ));
        }
        if path.untracked() {
            badges = badges.child(self.render_terminal_git_path_badge(
                "terminal.git.path_state_untracked",
                StatusTone::Info,
            ));
        }
        if path.conflict() {
            badges = badges.child(self.render_terminal_git_path_badge(
                "terminal.git.path_state_conflict",
                StatusTone::Error,
            ));
        }
        badges.into_any_element()
    }

    pub(super) fn render_terminal_git_path_badge(
        &self,
        label_key: &'static str,
        tone: StatusTone,
    ) -> AnyElement {
        status_pill(
            &self.tokens,
            self.i18n.t(label_key),
            StatusPillOptions::new(tone).compact(),
        )
        .into_any_element()
    }

    pub(super) fn render_terminal_git_path_action_button(
        &self,
        action: TerminalGitPathAction,
        path: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.i18n.t(terminal_git_path_action_label_key(action));
        action_chip(
            &self.tokens,
            label,
            None,
            ActionChipOptions::new()
                .height(24.0)
                .padding_x(6.0)
                .font_size(10.0)
                .radius(ButtonRadius::Sm)
                .idle_text_tone(ActionChipTextTone::Muted)
                .hover_text_tone(ActionChipTextTone::Primary),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.run_terminal_git_path_action(action, path.clone(), cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
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

    pub(super) fn render_terminal_git_branch_search(&self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(super) fn render_terminal_git_branch_row(
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
