// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(super) fn terminal_command_action_button(
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

    pub(super) fn terminal_command_context_chip_slot(
        max_width: f32,
        chip: AnyElement,
    ) -> AnyElement {
        // Context chips should measure to their content, then shrink only when
        // the command bar is too narrow to keep action buttons visible.
        div()
            .min_w(px(0.0))
            .max_w(px(max_width))
            .flex_initial()
            .overflow_hidden()
            .child(chip)
            .into_any_element()
    }

    pub(super) fn render_terminal_target_indicator(
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

    pub(super) fn render_terminal_cwd_chip(
        &self,
        snapshot: Option<CurrentDirectorySnapshot>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.terminal_cwd_picker.open;
        let workspace = cx.entity();
        let tooltip_id = "terminal-cwd-chip";
        let tooltip_label = terminal_cwd_chip_tooltip(
            snapshot.as_ref(),
            self.active_terminal_cwd_host(cx),
            &self.i18n,
        );
        let pending = self.active_terminal_cwd_is_pending(cx);
        let path = snapshot
            .as_ref()
            .map(|snapshot| terminal_cwd_chip_label(snapshot.path()))
            .unwrap_or_else(|| "...".to_string());
        let foreground = if active {
            rgb(theme.accent)
        } else {
            rgb(theme.text)
        };
        let icon_color = if active {
            rgb(theme.accent)
        } else {
            rgb(theme.text_muted)
        };
        select_anchor_probe(
            SelectAnchorId::TerminalCwdMenu,
            context_chip(
                &self.tokens,
                ContextChipOptions::new()
                    .max_width(TERMINAL_COMMAND_CONTEXT_CHIP_MAX_WIDTH)
                    .border_color(if active {
                        rgba((theme.accent << 8) | 0x99)
                    } else {
                        rgba((theme.border << 8) | 0x80)
                    })
                    .background_color(if active {
                        rgba((theme.accent << 8) | 0x1f)
                    } else {
                        rgba((theme.bg_hover << 8) | 0x66)
                    })
                    .text_color(foreground)
                    .hover_background_color(rgb(theme.bg_hover)),
                Some(Self::render_lucide_icon(
                    LucideIcon::Folder,
                    12.0,
                    icon_color,
                )),
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .when(pending, |this| {
                        this.italic().text_color(rgb(theme.text_muted))
                    })
                    .child(path)
                    .into_any_element(),
                Vec::new(),
            )
            .id(tooltip_id)
            .on_mouse_move(
                cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                    this.queue_workspace_tooltip(
                        tooltip_id,
                        tooltip_label.clone(),
                        f32::from(event.position.x) + 12.0,
                        f32::from(event.position.y) + 16.0,
                        cx,
                    );
                }),
            )
            .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                if !*hovered {
                    this.clear_workspace_tooltip(tooltip_id, cx);
                }
            }))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
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
            ),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    pub(super) fn render_terminal_cwd_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let left = self.terminal_cwd_picker_left();
        let bottom = if self.terminal_command_input_collapsed {
            32.0
        } else {
            64.0
        };
        let mut panel = context_menu_pointer_event_boundary(
            command_panel(
                &self.tokens,
                CommandPanelOptions::new()
                    .width(TERMINAL_CWD_MENU_WIDTH)
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
        )
        .child(self.render_terminal_cwd_search(cx));

        if let Some(path) = self.terminal_cwd_browse_path() {
            panel = panel.child(self.render_terminal_cwd_context_row(path.to_string(), cx));
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
                self.render_terminal_cwd_entry_list(visible, cx)
            }
        };

        oxideterm_gpui_ui::motion::fade_in(
            &self.tokens,
            "terminal-cwd-picker-enter",
            panel.child(body),
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        )
    }

    pub(super) fn render_terminal_cwd_entry_list(
        &self,
        visible: Vec<terminal_cwd::TerminalCwdVisibleEntry>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sync_terminal_cwd_list_state(&visible);
        let total = visible.len();
        let state = self.terminal_cwd_picker.list_state.clone();
        let spec = terminal_cwd::terminal_cwd_list_spec();
        let estimated_row_height = f32::from(spec.row_height);
        let list_height = (total as f32 * estimated_row_height)
            .clamp(estimated_row_height, TERMINAL_CWD_MENU_MAX_HEIGHT);
        let workspace = cx.entity();

        div()
            .min_h(px(0.0))
            .h(px(list_height))
            .max_h(px(TERMINAL_CWD_MENU_MAX_HEIGHT))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    let total = visible.len();
                    let Some(entry) = visible.get(index).cloned() else {
                        return div().into_any_element();
                    };
                    workspace.update(cx, move |this, cx| {
                        this.render_terminal_cwd_entry_list_item(entry, index, total, cx)
                    })
                },
            ))
            .into_any_element()
    }

    pub(super) fn render_terminal_cwd_entry_list_item(
        &self,
        entry: terminal_cwd::TerminalCwdVisibleEntry,
        index: usize,
        total: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .when(index + 1 < total, |item| item.pb(px(2.0)))
            .child(self.render_terminal_cwd_entry_row(entry, cx))
            .into_any_element()
    }

    pub(super) fn sync_terminal_cwd_list_state(
        &self,
        entries: &[terminal_cwd::TerminalCwdVisibleEntry],
    ) {
        let signatures = entries
            .iter()
            .map(terminal_cwd_entry_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.terminal_cwd_picker.list_state,
            &mut self.terminal_cwd_picker.list_cache.borrow_mut(),
            "terminal-cwd-picker",
            &signatures,
            terminal_cwd::terminal_cwd_list_spec(),
        );
    }

    pub(super) fn render_terminal_cwd_context_row(
        &self,
        path: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let display_path = path.clone();
        let switch_path = path.clone();
        let scope = self
            .terminal_cwd_picker
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.scope().clone());
        let mut trailing = vec![
            self.render_terminal_cwd_context_action(
                LucideIcon::Check,
                self.i18n.t("terminal.cwd.switch_to_directory"),
                {
                    let path = path.clone();
                    move |this, _event, window, cx| {
                        this.select_terminal_cwd_path(path.clone(), true, window, cx);
                        cx.stop_propagation();
                    }
                },
                cx,
            ),
            self.render_terminal_cwd_context_action(
                LucideIcon::Copy,
                self.i18n.t("terminal.cwd.copy_path"),
                {
                    let path = path.clone();
                    move |this, _event, _window, cx| {
                        this.copy_terminal_cwd_path(path.clone(), cx);
                        cx.stop_propagation();
                    }
                },
                cx,
            ),
        ];
        match scope {
            Some(CurrentDirectoryScope::Local) => {
                trailing.push(self.render_terminal_cwd_context_action(
                    LucideIcon::FolderOpen,
                    self.i18n.t("terminal.cwd.open_file_manager"),
                    {
                        let path = path.clone();
                        move |this, _event, window, cx| {
                            this.open_terminal_cwd_path_in_file_manager(path.clone(), window, cx);
                            cx.stop_propagation();
                        }
                    },
                    cx,
                ));
            }
            Some(CurrentDirectoryScope::SshNode(node_id)) => {
                trailing.push(self.render_terminal_cwd_context_action(
                    LucideIcon::Cloud,
                    self.i18n.t("terminal.cwd.open_sftp"),
                    {
                        let node_id = NodeId::new(node_id.clone());
                        let path = path.clone();
                        move |this, _event, window, cx| {
                            this.open_terminal_cwd_path_in_sftp(
                                node_id.clone(),
                                path.clone(),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }
                    },
                    cx,
                ));
                trailing.push(self.render_terminal_cwd_context_action(
                    LucideIcon::FileCode,
                    self.i18n.t("terminal.cwd.open_ide"),
                    {
                        let node_id = NodeId::new(node_id);
                        let path = path.clone();
                        move |this, _event, _window, cx| {
                            this.open_terminal_cwd_path_in_ide(node_id.clone(), path.clone(), cx);
                            cx.stop_propagation();
                        }
                    },
                    cx,
                ));
            }
            None => {}
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
                .font_family(settings_mono_font_family(self.settings_store.settings()))
                .text_color(rgb(theme.text))
                .child(display_path)
                .into_any_element(),
            None,
            Vec::new(),
            trailing,
        )
        .cursor_pointer()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                if event.click_count >= 2 {
                    this.select_terminal_cwd_path(switch_path.clone(), true, window, cx);
                }
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(super) fn render_terminal_cwd_context_action(
        &self,
        icon: LucideIcon,
        tooltip: String,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_tooltip_icon_button(
            icon,
            12.0,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions {
                idle_opacity: 0.72,
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(24.0, ButtonRadius::Md)
            },
            tooltip,
            "terminal-cwd-context-action",
            true,
            cx.listener(listener),
            cx.entity(),
        )
    }

    pub(super) fn render_terminal_cwd_search(&self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(super) fn render_terminal_cwd_entry_row(
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
            terminal_cwd::TerminalCwdVisibleEntryKind::File => {
                (LucideIcon::FileText, entry.name.clone(), rgb(theme.text))
            }
            terminal_cwd::TerminalCwdVisibleEntryKind::TypedPath => (
                LucideIcon::CornerDownLeft,
                self.i18n.t("terminal.cwd.go_to_path"),
                rgb(theme.accent),
            ),
        };
        let path = entry.path.clone();
        let entry_kind = entry.kind;
        let verified_directory = matches!(
            entry.kind,
            terminal_cwd::TerminalCwdVisibleEntryKind::Parent
                | terminal_cwd::TerminalCwdVisibleEntryKind::Directory
        );
        let browse_path = path.clone();
        let browse_tooltip_id = format!("terminal-cwd-enter-{browse_path}");
        let browse_tooltip_label = self.i18n.t("terminal.cwd.enter_directory");
        let browse_element_id = terminal_cwd_browse_element_id(&browse_path);
        let can_browse = matches!(
            entry.kind,
            terminal_cwd::TerminalCwdVisibleEntryKind::Parent
                | terminal_cwd::TerminalCwdVisibleEntryKind::Directory
        );
        let mut trailing = Vec::new();
        if can_browse {
            trailing.push(
                div()
                    .size(px(24.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.md))
                    .text_color(rgb(theme.text_muted))
                    .id(("terminal-cwd-browse-entry", browse_element_id))
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
                    ))
                    .into_any_element(),
            );
        }

        entity_list_row(
            &self.tokens,
            EntityListRowOptions::new().active(active).compact(),
            Some(Self::render_lucide_icon(icon, 13.0, accent)),
            div()
                .truncate()
                .text_color(if active { rgb(theme.accent) } else { accent })
                .child(label)
                .into_any_element(),
            Some(
                div()
                    .truncate()
                    .text_size(px(10.0))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_color(rgb(theme.text_muted))
                    .child(entry.path)
                    .into_any_element(),
            ),
            Vec::new(),
            trailing,
        )
        .cursor_pointer()
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
            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                this.terminal_cwd_picker.highlighted_path = Some(path.clone());
                if event.click_count >= 2 {
                    match entry_kind {
                        terminal_cwd::TerminalCwdVisibleEntryKind::File => {
                            this.insert_terminal_cwd_file_path(path.clone(), cx);
                        }
                        _ => this.select_terminal_cwd_path(
                            path.clone(),
                            verified_directory,
                            window,
                            cx,
                        ),
                    }
                } else {
                    cx.notify();
                }
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(super) fn render_terminal_cwd_message(
        &self,
        icon: LucideIcon,
        message: String,
    ) -> AnyElement {
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
            .child(if matches!(icon, LucideIcon::LoaderCircle) {
                self.render_loading_icon(
                    "terminal-cwd-loading",
                    14.0,
                    rgb(self.tokens.ui.text_muted),
                )
            } else {
                Self::render_lucide_icon(icon, 14.0, rgb(self.tokens.ui.text_muted))
            })
            .child(message)
            .into_any_element()
    }

    pub(super) fn render_terminal_git_chip(
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
        let foreground = if active {
            rgb(self.tokens.ui.accent)
        } else {
            rgba(0x86efacff)
        };
        let mut trailing: Vec<AnyElement> = Vec::new();
        if ahead > 0 {
            trailing.push(
                self.render_terminal_git_status_badge(LucideIcon::ArrowUp, ahead, rgba(0x86efacff))
                    .into_any_element(),
            );
        }
        if behind > 0 {
            trailing.push(
                self.render_terminal_git_status_badge(
                    LucideIcon::ArrowDown,
                    behind,
                    rgba(0x67e8f9ff),
                )
                .into_any_element(),
            );
        }
        if changed > 0 {
            trailing.push(
                self.render_terminal_git_status_badge(
                    LucideIcon::Pencil,
                    changed,
                    rgba(0xfbbf24ff),
                )
                .into_any_element(),
            );
        }
        if conflicts > 0 {
            trailing.push(
                self.render_terminal_git_status_badge(
                    LucideIcon::AlertTriangle,
                    conflicts,
                    rgba(0xf87171ff),
                )
                .into_any_element(),
            );
        }

        select_anchor_probe(
            SelectAnchorId::TerminalGitBranchMenu,
            context_chip(
                &self.tokens,
                ContextChipOptions::new()
                    .max_width(TERMINAL_COMMAND_CONTEXT_CHIP_MAX_WIDTH)
                    .border_color(if active {
                        rgba((self.tokens.ui.accent << 8) | 0x99)
                    } else {
                        rgba(0x22c55e4d)
                    })
                    .background_color(if active {
                        rgba((self.tokens.ui.accent << 8) | 0x1f)
                    } else {
                        rgba(0x22c55e1a)
                    })
                    .text_color(foreground)
                    .hover_background_color(rgba(0x22c55e26)),
                Some(Self::render_lucide_icon(
                    LucideIcon::GitFork,
                    12.0,
                    foreground,
                )),
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .child(label)
                    .into_any_element(),
                trailing,
            )
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
            ),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    pub(super) fn render_terminal_git_status_badge(
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

    pub(super) fn render_terminal_project_chip(
        &self,
        snapshot: ProjectSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.terminal_project_panel.open;
        let workspace = cx.entity();
        let label = snapshot.display_label();
        let task_count = snapshot.tasks().len();
        let foreground = if active {
            rgb(theme.accent)
        } else {
            rgba(0x7dd3fcff)
        };
        let mut trailing: Vec<AnyElement> = Vec::new();
        if task_count > 0 {
            trailing.push(
                div()
                    .flex_none()
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgba(0x082f4933))
                    .px(px(4.0))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .child(task_count.to_string())
                    .into_any_element(),
            );
        }

        select_anchor_probe(
            SelectAnchorId::TerminalProjectMenu,
            context_chip(
                &self.tokens,
                ContextChipOptions::new()
                    .max_width(TERMINAL_COMMAND_PROJECT_CHIP_MAX_WIDTH)
                    .border_color(if active {
                        rgba((theme.accent << 8) | 0x99)
                    } else {
                        rgba(0x38bdf84d)
                    })
                    .background_color(if active {
                        rgba((theme.accent << 8) | 0x1f)
                    } else {
                        rgba(0x38bdf81a)
                    })
                    .text_color(foreground)
                    .hover_background_color(rgba(0x38bdf826)),
                Some(Self::render_lucide_icon(
                    LucideIcon::ListChecks,
                    12.0,
                    foreground,
                )),
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .child(label)
                    .into_any_element(),
                trailing,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    if this.terminal_project_panel.open {
                        this.close_terminal_project_panel();
                        cx.notify();
                    } else {
                        this.open_terminal_project_panel(cx);
                    }
                    cx.stop_propagation();
                }),
            ),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    pub(super) fn render_terminal_project_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let left = self.terminal_project_panel_left();
        let bottom = if self.terminal_command_input_collapsed {
            32.0
        } else {
            64.0
        };
        let git_root_disagreement = self
            .active_terminal_project_snapshot(cx)
            .and_then(|project| {
                self.active_terminal_git_snapshot(cx).and_then(|git| {
                    terminal_project_git_root_disagreement(project.root_path(), &git.repo_root)
                })
            });

        let mut panel = context_menu_pointer_event_boundary(
            command_panel(
                &self.tokens,
                CommandPanelOptions::new()
                    .width(TERMINAL_PROJECT_MENU_WIDTH)
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
        )
        .child(self.render_terminal_project_search(cx));

        let body = if let Some(snapshot) = self.active_terminal_project_snapshot(cx) {
            panel = panel.child(
                self.render_terminal_project_header(&snapshot, git_root_disagreement.as_deref()),
            );
            let tasks = self.visible_terminal_project_tasks(cx);
            if tasks.is_empty() {
                self.render_terminal_project_message(
                    LucideIcon::Search,
                    self.i18n.t("terminal.project.no_tasks"),
                )
            } else {
                self.render_terminal_project_task_list(tasks, cx)
            }
        } else {
            self.render_terminal_project_message(
                LucideIcon::AlertCircle,
                self.i18n.t("terminal.project.no_project"),
            )
        };
        panel = panel.child(body);
        oxideterm_gpui_ui::motion::fade_in(
            &self.tokens,
            "terminal-project-picker-enter",
            panel,
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        )
    }

    pub(super) fn render_terminal_project_header(
        &self,
        snapshot: &ProjectSnapshot,
        git_root_disagreement: Option<&str>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let trailing = git_root_disagreement
            .map(|git_root| {
                vec![
                    div()
                        .truncate()
                        .max_w(px(260.0))
                        .font_family(settings_mono_font_family(self.settings_store.settings()))
                        .text_size(px(10.0))
                        .text_color(rgb(theme.text_muted))
                        .child(format!(
                            "{}: {git_root}",
                            self.i18n.t("terminal.project.git_root")
                        ))
                        .into_any_element(),
                ]
            })
            .unwrap_or_default();
        entity_list_row(
            &self.tokens,
            EntityListRowOptions::new().compact(),
            Some(Self::render_lucide_icon(
                LucideIcon::Folder,
                13.0,
                rgb(theme.text_muted),
            )),
            div()
                .truncate()
                .font_family(settings_mono_font_family(self.settings_store.settings()))
                .text_size(px(11.0))
                .text_color(rgb(theme.text))
                .child(snapshot.root_path().to_string())
                .into_any_element(),
            Some(
                div()
                    .truncate()
                    .text_size(px(10.0))
                    .text_color(rgb(theme.text_muted))
                    .child(snapshot.display_label())
                    .into_any_element(),
            ),
            trailing,
            Vec::new(),
        )
        .into_any_element()
    }

    pub(super) fn render_terminal_project_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::TerminalProjectSearch;
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &self.terminal_project_panel.query,
                    placeholder: self.i18n.t("terminal.project.search_tasks"),
                    focused: self.terminal_project_panel.open,
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

    pub(super) fn render_terminal_project_task_list(
        &self,
        tasks: Vec<ProjectTask>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut groups = Vec::<(ProjectTaskGroup, Vec<ProjectTask>)>::new();
        for task in tasks {
            if let Some((_, bucket)) = groups.iter_mut().find(|(group, _)| *group == task.group()) {
                bucket.push(task);
            } else {
                groups.push((task.group(), vec![task]));
            }
        }

        let mut list = div().flex().flex_col().gap(px(8.0));
        for (group, tasks) in groups {
            let mut section = div().flex().flex_col().gap(px(3.0)).child(
                div()
                    .px(px(4.0))
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .text_size(px(10.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(Self::render_lucide_icon(
                        terminal_project_group_icon(group),
                        12.0,
                        rgb(self.tokens.ui.text_muted),
                    ))
                    .child(self.i18n.t(terminal_project_group_label_key(group))),
            );
            for task in tasks {
                section = section.child(self.render_terminal_project_task_row(task, cx));
            }
            list = list.child(section);
        }

        div()
            .min_h(px(0.0))
            .max_h(px(TERMINAL_PROJECT_MENU_BODY_MAX_HEIGHT))
            .overflow_y_scrollbar()
            .child(list)
            .into_any_element()
    }

    pub(super) fn render_terminal_project_task_row(
        &self,
        task: ProjectTask,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.terminal_project_panel.highlighted_task_id.as_deref() == Some(task.id());
        let task_id = task.id().to_string();
        let task_label = task.label().to_string();
        let task_command = task.command().to_string();
        let task_source = task.source().display_name();
        let row_task = task.clone();
        entity_list_row(
            &self.tokens,
            EntityListRowOptions::new().active(active).compact(),
            Some(Self::render_lucide_icon(
                LucideIcon::FilePlay,
                13.0,
                if active {
                    rgb(theme.accent)
                } else {
                    rgb(theme.text_muted)
                },
            )),
            div()
                .truncate()
                .text_color(if active {
                    rgb(theme.accent)
                } else {
                    rgb(theme.text)
                })
                .child(task_label)
                .into_any_element(),
            Some(
                div()
                    .truncate()
                    .text_size(px(10.0))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_color(rgb(theme.text_muted))
                    .child(task_command)
                    .into_any_element(),
            ),
            vec![
                status_pill(
                    &self.tokens,
                    task_source,
                    StatusPillOptions::new(StatusTone::Neutral).compact(),
                )
                .into_any_element(),
            ],
            Vec::new(),
        )
        .cursor_pointer()
        .on_mouse_move(cx.listener({
            let task_id = task_id.clone();
            move |this, _event: &MouseMoveEvent, _window, cx| {
                if this.terminal_project_panel.highlighted_task_id.as_deref()
                    != Some(task_id.as_str())
                {
                    this.terminal_project_panel.highlighted_task_id = Some(task_id.clone());
                    cx.notify();
                }
            }
        }))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                this.terminal_project_panel.highlighted_task_id = Some(row_task.id().to_string());
                if event.click_count >= 2 {
                    this.run_terminal_project_task(row_task.clone(), cx);
                } else {
                    cx.notify();
                }
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(super) fn render_terminal_project_message(
        &self,
        icon: LucideIcon,
        message: String,
    ) -> AnyElement {
        div()
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
}
