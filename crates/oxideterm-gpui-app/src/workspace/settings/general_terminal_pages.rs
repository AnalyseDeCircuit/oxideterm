use super::*;
#[cfg(not(target_os = "macos"))]
use oxideterm_gpui_settings_view::entity::LaunchAtLoginError;
use oxideterm_gpui_settings_view::general_page::{GeneralPage, GeneralRequest};

use oxideterm_gpui_settings_view::command_specs::{
    CommandSpecsBindings, CommandSpecsDialog, CommandSpecsFrame, TerminalCommandSpecsAction,
};
use oxideterm_gpui_settings_view::components::SettingsWindowRequest;
use zeroize::Zeroizing;

impl WorkspaceApp {
    fn with_general_page<R>(
        &self,
        section: SettingsSectionId,
        cx: &mut Context<Self>,
        render: impl FnOnce(GeneralPage<'_>, &App) -> R,
    ) -> R {
        let owner = cx.entity().downgrade();
        let dispatch = Rc::new(move |request, window: &mut Window, cx: &mut App| {
            let _ = owner.update(cx, |app, cx| match request {
                GeneralRequest::Cli(operation) => app.start_cli_companion_operation(operation, cx),
                GeneralRequest::OpenLoginItems => {
                    #[cfg(target_os = "macos")]
                    {
                        let result =
                            oxideterm_gpui_platform::autostart::open_login_items_settings()
                                .map_err(|error| error.to_string());
                        app.settings_workspace.update(cx, |settings, cx| {
                            settings.finish_launch_at_login_settings_handoff(result, cx)
                        });
                    }
                }
                GeneralRequest::SetLaunchAtLogin(enabled) => {
                    #[cfg(not(target_os = "macos"))]
                    if !app
                        .settings_workspace
                        .read(cx)
                        .launch_at_login_snapshot()
                        .pending
                    {
                        app.set_launch_at_login_enabled(enabled, cx);
                    }
                    #[cfg(target_os = "macos")]
                    let _ = enabled;
                }
                GeneralRequest::PickDataDirectory => app.pick_settings_data_directory(cx),
                GeneralRequest::ResetDataDirectory => {
                    app.settings_workspace.update(cx, |settings, cx| {
                        settings.open_data_directory_reset_confirm(cx)
                    });
                    app.reset_standard_confirm_focus();
                    cx.notify();
                }
                GeneralRequest::ClearCommandHistory => {
                    app.open_command_history_clear_confirm(true, cx)
                }
                GeneralRequest::OpenCommandSpecs => {
                    app.open_terminal_command_specs_editor(window, cx)
                }
            });
        });
        let focus_handoff = (section == SettingsSectionId::TerminalCommandBarFocusHandoff)
            .then(|| self.focus_handoff_commands_row(self.settings_store.settings(), cx));
        let (command_specs_path, command_specs_summary) =
            if section == SettingsSectionId::TerminalCommandBarQuickCommands {
                (
                    super::terminal_command_bar::completion::terminal_command_specs_path(
                        self.settings_store.path(),
                    )
                    .display()
                    .to_string(),
                    self.terminal_command_specs_summary(),
                )
            } else {
                (String::new(), String::new())
            };
        self.with_settings_controls(cx, |controls, cx| {
            render(
                GeneralPage {
                    controls,
                    state: &self.settings_workspace,
                    data_dir: (section == SettingsSectionId::GeneralDataDirectory)
                        .then(|| self.settings_data_directory_info()),
                    history_failed: self.terminal_history_store.failed(),
                    focus_handoff,
                    command_specs_path,
                    command_specs_summary,
                    dispatch,
                },
                cx,
            )
        })
    }

    pub(in crate::workspace) fn settings_general_section(
        &self,
        section: SettingsSectionId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if section == SettingsSectionId::GeneralAppLock {
            return self.render_app_lock_settings_card(cx);
        }
        self.with_general_page(section, cx, |page, cx| {
            page.settings_general_section(section, cx)
        })
    }

    #[cfg(not(target_os = "macos"))]
    pub(in crate::workspace) fn refresh_launch_at_login_status(&mut self, cx: &mut Context<Self>) {
        let runtime = self.forwarding_runtime.clone();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_launch_at_login_operation(
                async move {
                    let result = runtime
                        .spawn_blocking(oxideterm_gpui_platform::autostart::is_enabled)
                        .await
                        .map_err(|error| {
                            LaunchAtLoginError::TaskFailed(error.to_string().into())
                        })?;
                    result.map_err(|error| {
                        LaunchAtLoginError::OperationFailed(error.to_string().into())
                    })
                },
                cx,
            );
        });
    }

    #[cfg(not(target_os = "macos"))]
    pub(in crate::workspace) fn set_launch_at_login_enabled(
        &mut self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let runtime = self.forwarding_runtime.clone();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_launch_at_login_operation(
                async move {
                    let result = runtime
                        .spawn_blocking(move || {
                            oxideterm_gpui_platform::autostart::set_enabled(enabled)?;
                            let actual = oxideterm_gpui_platform::autostart::is_enabled()?;
                            if actual != enabled {
                                return Err(std::io::Error::other(
                                    "the operating system did not retain the startup setting",
                                ));
                            }
                            Ok(actual)
                        })
                        .await
                        .map_err(|error| {
                            LaunchAtLoginError::TaskFailed(error.to_string().into())
                        })?;
                    result.map_err(|error| {
                        if error.kind() == std::io::ErrorKind::PermissionDenied {
                            LaunchAtLoginError::ApprovalRequired
                        } else {
                            LaunchAtLoginError::OperationFailed(error.to_string().into())
                        }
                    })
                },
                cx,
            );
        });
    }

    pub(in crate::workspace) fn settings_data_directory_info(
        &self,
    ) -> oxideterm_settings::DataDirectoryInfo {
        oxideterm_settings::data_directory_info().unwrap_or_else(|_| {
            let path = self
                .settings_store
                .path()
                .parent()
                .unwrap_or_else(|| self.settings_store.path())
                .to_path_buf();
            oxideterm_settings::DataDirectoryInfo {
                default_path: path.clone(),
                path,
                is_custom: false,
                is_portable: false,
                can_change: false,
            }
        })
    }

    pub(in crate::workspace) fn pick_settings_data_directory(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.general.select_data_directory"),
            )),
        });
        let selection = async move {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return None;
            };
            paths.into_iter().next()
        };
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_data_directory_picker(selection, cx);
        });
    }

    pub(in crate::workspace) fn cancel_settings_data_directory_confirm(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.clear_standard_confirm_focus();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.begin_data_directory_confirm_exit(false, delay, cx);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn confirm_settings_data_directory(&mut self, cx: &mut Context<Self>) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.clear_standard_confirm_focus();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.begin_data_directory_confirm_exit(true, delay, cx);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn render_settings_data_directory_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let settings_workspace = self.settings_workspace.read(cx);
        let confirm = settings_workspace.data_directory_confirm()?;
        let (title_key, description) = match confirm {
            DataDirectoryConfirm::Conflict { files_found, .. } => (
                "settings_view.general.data_directory_conflict",
                self.i18n
                    .t("settings_view.general.data_directory_conflict_detail")
                    .replace("{{files}}", &files_found.join(", ")),
            ),
            DataDirectoryConfirm::Reset => (
                "settings_view.general.reset_data_directory",
                self.i18n
                    .t("settings_view.general.reset_data_directory_confirm"),
            ),
        };
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                &self.tokens,
                "settings-data-directory-confirm-motion",
                settings_workspace.data_directory_confirm_phase(),
                ConfirmDialogView {
                    variant: ConfirmDialogVariant::Default,
                    title: div().child(self.i18n.t(title_key)).into_any_element(),
                    description: Some(div().child(description).into_any_element()),
                    cancel_label: div()
                        .child(self.i18n.t("common.actions.cancel"))
                        .into_any_element(),
                    confirm_label: div()
                        .child(self.i18n.t("common.actions.confirm"))
                        .into_any_element(),
                },
                self.standard_confirm_focus(),
                cx.listener(|this, _event, _window, cx| {
                    this.cancel_settings_data_directory_confirm(cx);
                    cx.stop_propagation();
                }),
                cx.listener(|this, _event, _window, cx| {
                    this.confirm_settings_data_directory(cx);
                    cx.stop_propagation();
                }),
            ),
        )
    }

    pub(in crate::workspace) fn settings_terminal_section(
        &self,
        section: SettingsSectionId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use oxideterm_gpui_settings_view::terminal_page::TerminalSettingsSection;
        let settings = self.settings_store.settings();
        let section = match section {
            SettingsSectionId::TerminalDisplayFont => TerminalSettingsSection::Font {
                custom_cjk: self.settings_workspace.read(cx).terminal_cjk_font_custom,
                preview: Self::settings_terminal_preview_font(settings, cx),
            },
            SettingsSectionId::TerminalDisplayBehavior => TerminalSettingsSection::DisplayBehavior,
            SettingsSectionId::TerminalDisplayCursor => TerminalSettingsSection::Cursor,
            SettingsSectionId::TerminalDisplayCommandMarks => TerminalSettingsSection::CommandMarks,
            SettingsSectionId::TerminalDisplayBuffer => TerminalSettingsSection::Buffer,
            SettingsSectionId::TerminalInputClipboard => TerminalSettingsSection::Clipboard,
            SettingsSectionId::TerminalInputMouseLinks => TerminalSettingsSection::MouseLinks,
            SettingsSectionId::TerminalInputKeyboard => TerminalSettingsSection::Keyboard,
            SettingsSectionId::TerminalTransferInBand => TerminalSettingsSection::InBandTransfer,
            SettingsSectionId::TerminalTransferXmodem => TerminalSettingsSection::Xmodem,
            SettingsSectionId::TerminalLoggingSession => TerminalSettingsSection::SessionLogging {
                default_directory: settings::terminal_session_log_root_directory(
                    self.settings_store.path(),
                    None,
                )
                .to_string_lossy()
                .into_owned(),
            },
            SettingsSectionId::TerminalLocalShell
            | SettingsSectionId::TerminalLocalShellProfile
            | SettingsSectionId::TerminalLocalPrivilegeCredentials => {
                return self.settings_local_section(section, cx);
            }
            SettingsSectionId::TerminalCommandBarControls
            | SettingsSectionId::TerminalCommandBarFocusHandoff
            | SettingsSectionId::TerminalCommandBarQuickCommands
            | SettingsSectionId::TerminalAwarenessControls => {
                return self.with_general_page(section, cx, |page, cx| {
                    page.terminal_awareness_section(section, cx)
                });
            }
            SettingsSectionId::TerminalAwarenessShellIntegration => {
                return self.remote_shell_integration_card(cx);
            }
            SettingsSectionId::TerminalAwarenessTriggers => {
                return self.terminal_triggers_settings_card(cx);
            }
            SettingsSectionId::TerminalHighlightSelection => {
                return self.highlight_selection_card(settings, cx);
            }
            SettingsSectionId::TerminalHighlightSemantic => {
                return self.highlight_semantic_card(settings, cx);
            }
            SettingsSectionId::TerminalHighlightRules => {
                return self.highlight_rules_card(settings, cx);
            }
            _ => return div().into_any_element(),
        };
        self.with_settings_controls(cx, |controls, cx| {
            controls.terminal_settings_section(section, cx)
        })
    }

    pub(in crate::workspace) fn pick_terminal_session_log_directory(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n
                    .t("settings_view.terminal.session_log_select_directory"),
            )),
        });
        cx.spawn(async move |workspace, cx| {
            let selected_directory = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let Some(selected_directory) = selected_directory else {
                return;
            };
            let directory = selected_directory.to_string_lossy().to_string();
            let _ = workspace.update(cx, |this, cx| {
                this.edit_settings(
                    |settings| settings.terminal.session_log.directory = Some(directory),
                    cx,
                );
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn focus_handoff_commands_row(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = SettingsInput::TerminalCommandBarFocusHandoff;
        let focused = self.focused_settings_input == Some(input);
        let value = if focused {
            self.settings_input_draft.clone()
        } else {
            self.current_settings_input_value(input, cx)
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let theme = self.tokens.ui;
        let line_height = input.textarea_line_height();
        let mut command_editor = div()
            .w_full()
            .min_h(px(44.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if focused {
                rgba((theme.accent << 8) | 0x99)
            } else {
                rgb(theme.border)
            })
            .bg(rgb(theme.bg))
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(0.0))
            .cursor(CursorStyle::IBeam)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(line_height))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_color(rgb(theme.text))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input, cx);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle, cx);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                }),
            );

        if value.is_empty() {
            command_editor = self.render_settings_multiline_textarea_lines(
                command_editor,
                target,
                "aider, custom-tui",
                true,
                line_height,
                cx,
            );
        } else {
            command_editor = self.render_settings_multiline_textarea_lines(
                command_editor,
                target,
                &value,
                false,
                line_height,
                cx,
            );
        }

        if let Some(marked) = self.marked_text_for_target(target, cx) {
            command_editor = command_editor.child(
                div()
                    .underline()
                    .text_color(rgb(theme.text))
                    .child(marked.to_string()),
            );
        }

        let control = text_input_anchor_probe(
            target.anchor_id(),
            command_editor,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        );

        let control = self.settings_multiline_input_viewport(target, control.into_any_element());

        let mut presets = div().w_full().flex().flex_row().flex_wrap().gap(px(8.0));
        for command in RECOMMENDED_FOCUS_HANDOFF_COMMANDS {
            let enabled = settings
                .terminal
                .command_bar
                .focus_handoff_commands
                .iter()
                .any(|candidate| candidate == command);
            let command_name = (*command).to_string();
            let theme = self.tokens.ui;
            let chip = div()
                .h(px(32.0))
                .px(px(12.0))
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(if enabled {
                    rgba((theme.accent << 8) | 0x99)
                } else {
                    rgba((theme.border << 8) | 0x99)
                })
                .bg(if enabled {
                    rgba((theme.accent << 8) | 0x20)
                } else {
                    rgba(0x00000000)
                })
                .flex()
                .items_center()
                .gap(px(6.0))
                .cursor_pointer()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .font_family(settings_mono_font_family(self.settings_store.settings()))
                .text_color(if enabled {
                    rgb(theme.accent)
                } else {
                    rgb(theme.text_muted)
                })
                .hover(move |style| {
                    style
                        .border_color(rgba((theme.accent << 8) | 0x88))
                        .bg(rgb(theme.bg_hover))
                        .text_color(rgb(theme.text))
                })
                .when(enabled, |chip| chip.child("✓"))
                .child(command_name.clone())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if let Some(input) = this.focused_settings_input.take() {
                            this.clear_settings_input_draft(input);
                        }
                        let command_name = command_name.clone();
                        this.edit_settings(
                            move |settings| {
                                let commands =
                                    &mut settings.terminal.command_bar.focus_handoff_commands;
                                if let Some(index) = commands
                                    .iter()
                                    .position(|candidate| candidate == &command_name)
                                {
                                    commands.remove(index);
                                } else {
                                    commands.push(command_name);
                                }
                            },
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                );
            presets = presets.child(chip);
        }

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                self.card_subsection_header(
                    self.i18n
                        .t("settings_view.terminal.command_bar_focus_handoff_presets"),
                    Some(
                        self.i18n
                            .t("settings_view.terminal.command_bar_focus_handoff_presets_hint"),
                    ),
                ),
            )
            .child(presets)
            .child(self.card_separator())
            .child(
                self.card_subsection_header(
                    self.i18n
                        .t("settings_view.terminal.command_bar_focus_handoff_advanced"),
                    Some(
                        self.i18n
                            .t("settings_view.terminal.command_bar_focus_handoff_advanced_hint"),
                    ),
                ),
            )
            .child(control)
            .into_any_element()
    }

    pub(in crate::workspace) fn mount_command_specs_dialog(
        settings: &Entity<SettingsWorkspaceEntity>,
        cx: &mut Context<Self>,
    ) -> Entity<CommandSpecsDialog> {
        let owner = cx.entity().downgrade();
        let bindings = CommandSpecsBindings {
            frame: Rc::new({
                let owner = owner.clone();
                move |cx| {
                    let owner = owner.upgrade()?;
                    let app = owner.read(cx);
                    Some(CommandSpecsFrame {
                        tokens: app.tokens,
                        i18n: app.i18n.clone(),
                        mono_font: settings_mono_font_family(app.settings_store.settings()),
                        scroll: app
                            .selectable_text_scroll_handle("terminal-command-specs-editor-scroll"),
                    })
                }
            }),
            editor: Rc::new({
                let owner = owner.clone();
                move |value, cx| {
                    let Some(owner) = owner.upgrade() else {
                        return div().into_any_element();
                    };
                    owner
                        .read(cx)
                        .terminal_command_specs_editor_control(value, owner, cx)
                }
            }),
            dispatch: Rc::new(move |action, window, cx| {
                let _ = owner.update(cx, |app, cx| {
                    app.handle_terminal_command_specs_action(action, window, cx)
                });
            }),
        };
        cx.new(|cx| CommandSpecsDialog::new(settings, bindings, cx))
    }

    fn terminal_command_specs_editor_control(
        &self,
        value: &str,
        owner: Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        let input = SettingsInput::TerminalCommandSpecsJson;
        let focused = self
            .settings_workspace
            .read(cx)
            .settings_entity_focused_input()
            == Some(input);
        let target = WorkspaceImeTarget::Settings(input);
        let theme = self.tokens.ui;
        let line_height = input.textarea_line_height();
        let mut textarea = div()
            .w_full()
            .min_h(px(520.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if focused {
                rgba((theme.accent << 8) | 0x99)
            } else {
                rgb(theme.border)
            })
            .bg(rgb(theme.bg))
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(0.0))
            .cursor(CursorStyle::IBeam)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(line_height))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_color(rgb(theme.text));

        textarea = self.render_settings_multiline_textarea_lines(
            textarea,
            target,
            value,
            false,
            line_height,
            cx,
        );
        if let Some(marked) = self.marked_text_for_target(target, cx) {
            textarea = textarea.child(
                div()
                    .underline()
                    .text_color(rgb(theme.text))
                    .child(marked.to_string()),
            );
        }
        let dispatch = Self::settings_window_dispatch_for(owner.downgrade());
        let select = dispatch.clone();
        let textarea = textarea
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                select(
                    SettingsWindowRequest::BeginInputSelection(input, event.clone()),
                    window,
                    cx,
                );
                cx.stop_propagation();
            })
            .on_mouse_move(move |event, window, cx| {
                dispatch(
                    SettingsWindowRequest::DragInputSelection(event.clone()),
                    window,
                    cx,
                )
            });
        let anchors = self.text_input_anchors.clone();
        let control = text_input_anchor_probe(target.anchor_id(), textarea, move |anchor, _, _| {
            anchors.update(anchor)
        });
        self.settings_multiline_input_viewport(target, control.into_any_element())
    }

    pub(in crate::workspace) fn open_terminal_command_specs_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = self.terminal_command_specs_editor_initial_value();
        self.prepare_modal_interaction_boundary(cx);
        let path = super::terminal_command_bar::completion::terminal_command_specs_path(
            self.settings_store.path(),
        );
        let summary = self.terminal_command_specs_summary();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.open_command_specs(path.display().to_string(), summary, value, cx);
        });
        self.show_active_input_caret(cx);
        window.focus(&self.focus_handle, cx);
    }

    pub(in crate::workspace) fn close_terminal_command_specs_editor(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self
            .settings_workspace
            .update(cx, |settings, cx| settings.close_command_specs(cx))
        {
            self.clear_ime_selection();
            self.ime_marked_text = None;
        }
    }

    pub(in crate::workspace) fn render_settings_multiline_textarea_lines(
        &self,
        mut textarea: Div,
        target: WorkspaceImeTarget,
        value: &str,
        placeholder: bool,
        line_height: f32,
        cx: &App,
    ) -> Div {
        // The IME maps physical lines, so visual wrapping must not add lines.
        // The viewport scrolls this anchored content rather than the anchor itself.
        textarea = textarea
            .w_auto()
            .min_w_full()
            .flex_shrink_0()
            .whitespace_nowrap();
        let selection = self.ime_selected_range_for_target(target, cx);
        let theme = self.tokens.ui;
        for (line_range, line_text) in settings_multiline_line_ranges(value) {
            let (selection_range, caret_offset) =
                settings_multiline_line_selection(selection.as_ref(), &line_range);
            // Browser textareas hit-test contiguous line boxes. Keep the
            // manually rendered GPUI lines at the same height used by IME
            // y-to-line mapping so pointer selection cannot drift vertically.
            let mut line = div()
                .h(px(line_height))
                .min_h(px(line_height))
                .flex_shrink_0()
                .whitespace_nowrap();
            if placeholder {
                // Browser placeholder text is not part of the editable value;
                // keep it muted and do not feed it through selection segments.
                line = line
                    .text_color(rgb(theme.text_muted))
                    .child(line_text.as_str().to_string());
            } else {
                // Tauri uses a real textarea, so caret/selection sit inside the
                // current visual line. Native renders line elements manually and
                // must split the shared UTF-16 IME selection per line.
                line = line.child(text_input_value_segments(
                    &self.tokens,
                    &line_text,
                    false,
                    selection_range,
                    caret_offset,
                    self.input_caret.visible(),
                ));
            }
            textarea = textarea.child(line);
        }
        textarea
    }

    pub(in crate::workspace) fn settings_multiline_input_viewport(
        &self,
        target: WorkspaceImeTarget,
        content: AnyElement,
    ) -> AnyElement {
        let scroll =
            self.selectable_text_scroll_handle(format!("multiline-input-{}", target.anchor_id().0));
        let document =
            target == WorkspaceImeTarget::Settings(SettingsInput::TerminalCommandSpecsJson);
        div()
            .relative()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .id(("settings-multiline-viewport", target.anchor_id().0))
                    .w_full()
                    .min_w_0()
                    .when(!document, |area| area.max_h(px(240.0)))
                    .flex()
                    .items_start()
                    .overflow_scroll()
                    .track_scroll(&scroll)
                    .child(content),
            )
            .child(
                oxideterm_gpui_ui::Scrollbar::new(&scroll)
                    .axis(oxideterm_gpui_ui::ScrollbarAxis::Both),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn load_terminal_command_specs_editor_value(&self) -> String {
        let path = super::terminal_command_bar::completion::terminal_command_specs_path(
            self.settings_store.path(),
        );
        std::fs::read_to_string(path).unwrap_or_default()
    }

    pub(in crate::workspace) fn terminal_command_specs_editor_initial_value(&self) -> String {
        super::terminal_command_bar::completion::terminal_command_specs_editor_initial_json(
            &self.load_terminal_command_specs_editor_value(),
        )
    }

    pub(in crate::workspace) fn terminal_command_specs_summary(&self) -> String {
        let built_in_count =
            super::terminal_command_bar::completion::built_in_terminal_fig_specs().len();
        let custom_count = super::terminal_command_bar::completion::user_terminal_fig_specs_count(
            self.settings_store.path(),
        );
        self.i18n
            .t("settings_view.terminal.command_specs_summary")
            .replace("{builtIn}", &built_in_count.to_string())
            .replace("{custom}", &custom_count.to_string())
    }

    pub(in crate::workspace) fn handle_terminal_command_specs_action(
        &mut self,
        action: TerminalCommandSpecsAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            TerminalCommandSpecsAction::Close => self.close_terminal_command_specs_editor(cx),
            TerminalCommandSpecsAction::Example => {
                let example =
                    super::terminal_command_bar::completion::terminal_command_specs_example_json();
                self.set_terminal_command_specs_draft(Zeroizing::new(example), None, true, cx);
                window.focus(&self.focus_handle, cx);
            }
            TerminalCommandSpecsAction::Format => {
                let Some(value) = self
                    .settings_workspace
                    .read(cx)
                    .settings_entity_input_value(SettingsInput::TerminalCommandSpecsJson)
                else {
                    return;
                };
                match super::terminal_command_bar::completion::normalize_terminal_command_specs_json(
                    value,
                ) {
                    Ok(pretty) => {
                        self.set_terminal_command_specs_draft(
                            Zeroizing::new(pretty),
                            None,
                            true,
                            cx,
                        );
                        window.focus(&self.focus_handle, cx);
                    }
                    Err(error) => self.push_ai_settings_toast(
                        format!(
                            "{} {}",
                            self.i18n.t("settings_view.terminal.command_specs_invalid"),
                            error
                        ),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                }
            }
            TerminalCommandSpecsAction::Save => {
                let Some(value) = self
                    .settings_workspace
                    .read(cx)
                    .settings_entity_input_value(SettingsInput::TerminalCommandSpecsJson)
                else {
                    return;
                };
                match super::terminal_command_bar::completion::normalize_terminal_command_specs_json(
                    value,
                ) {
                    Ok(pretty) => {
                        let pretty = Zeroizing::new(pretty);
                        let path =
                            super::terminal_command_bar::completion::terminal_command_specs_path(
                                self.settings_store.path(),
                            );
                        let result = path
                            .parent()
                            .map(std::fs::create_dir_all)
                            .transpose()
                            .and_then(|_| std::fs::write(&path, pretty.as_bytes()));
                        match result {
                            Ok(()) => {
                                let summary = self.terminal_command_specs_summary();
                                self.set_terminal_command_specs_draft(
                                    pretty,
                                    Some(summary),
                                    false,
                                    cx,
                                );
                                self.push_ai_settings_toast(
                                    self.i18n.t("settings_view.terminal.command_specs_saved"),
                                    TerminalNoticeVariant::Success,
                                    cx,
                                );
                                cx.notify();
                            }
                            Err(error) => self.push_ai_settings_toast(
                                error.to_string(),
                                TerminalNoticeVariant::Error,
                                cx,
                            ),
                        }
                    }
                    Err(error) => self.push_ai_settings_toast(
                        format!(
                            "{} {}",
                            self.i18n.t("settings_view.terminal.command_specs_invalid"),
                            error
                        ),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                }
            }
        }
    }

    fn set_terminal_command_specs_draft(
        &mut self,
        value: Zeroizing<String>,
        summary: Option<String>,
        focus: bool,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.update_command_specs(value, summary, focus, cx)
        });
        if focus {
            self.clear_ime_selection();
            self.ime_marked_text = None;
            self.show_active_input_caret(cx);
        }
    }
}
