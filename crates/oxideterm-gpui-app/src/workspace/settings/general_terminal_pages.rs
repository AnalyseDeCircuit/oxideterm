use super::*;
use oxideterm_gpui_settings_view::general_page::GeneralRequest;

use oxideterm_gpui_settings_view::command_specs::{
    CommandSpecsBindings, CommandSpecsFrame, CommandSpecsOutcome, TerminalCommandSpecsAction,
};

impl WorkspaceApp {
    pub(super) fn handle_general_settings_request(
        &mut self,
        request: GeneralRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match request {
            GeneralRequest::Cli(operation) => self.start_cli_companion_operation(operation, cx),
            GeneralRequest::OpenLoginItems => {
                #[cfg(target_os = "macos")]
                {
                    let result = oxideterm_gpui_platform::autostart::open_login_items_settings()
                        .map_err(|error| error.to_string());
                    self.settings_workspace.update(cx, |settings, cx| {
                        settings.finish_launch_at_login_settings_handoff(result, cx)
                    });
                }
            }
            GeneralRequest::SetLaunchAtLogin(enabled) => {
                #[cfg(not(target_os = "macos"))]
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.set_launch_at_login(enabled, self.forwarding_runtime.clone(), cx);
                });
                #[cfg(target_os = "macos")]
                let _ = enabled;
            }
            GeneralRequest::PickDataDirectory => self
                .settings_workspace
                .update(cx, |s, cx| s.pick_data_directory(&self.i18n, cx)),
            GeneralRequest::ResetDataDirectory => {
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.open_data_directory_reset_confirm(cx)
                });
                self.reset_standard_confirm_focus();
                cx.notify();
            }
            GeneralRequest::ClearCommandHistory => {
                self.open_command_history_clear_confirm(true, cx)
            }
            GeneralRequest::OpenCommandSpecs => self.open_terminal_command_specs_editor(window, cx),
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub(in crate::workspace) fn refresh_launch_at_login_status(&mut self, cx: &mut Context<Self>) {
        let runtime = self.forwarding_runtime.clone();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.refresh_launch_at_login(runtime, cx)
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
        let owner = cx.entity().downgrade();
        oxideterm_gpui_settings_view::general_page::render_data_directory_confirm(
            self.settings_workspace.read(cx),
            &self.tokens,
            &self.i18n,
            self.standard_confirm_focus(),
            move |confirmed, cx| {
                let _ = owner.update(cx, |app, cx| {
                    if confirmed {
                        app.confirm_settings_data_directory(cx);
                    } else {
                        app.cancel_settings_data_directory_confirm(cx);
                    }
                });
            },
        )
    }

    pub(in crate::workspace) fn pick_terminal_session_log_directory(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let selection = oxideterm_gpui_settings_view::entity::picker::pick_path(
            cx,
            self.i18n
                .t("settings_view.terminal.session_log_select_directory"),
            true,
        );
        cx.spawn(async move |workspace, cx| {
            let Some(selected_directory) = selection.await else {
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

    pub(super) fn toggle_focus_handoff_command(&mut self, name: String, cx: &mut Context<Self>) {
        if let Some(input) = self.focused_settings_input.take() {
            self.clear_settings_input_draft(input);
        }
        self.edit_settings(
            move |settings| {
                let commands = &mut settings.terminal.command_bar.focus_handoff_commands;
                if let Some(index) = commands.iter().position(|candidate| candidate == &name) {
                    commands.remove(index);
                } else {
                    commands.push(name);
                }
            },
            cx,
        );
    }

    pub(in crate::workspace) fn command_specs_bindings(cx: &Context<Self>) -> CommandSpecsBindings {
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
            controls: oxideterm_gpui_settings_view::surface::settings_controls_source(
                owner.clone(),
            ),
            scroll: Rc::new({
                let owner = owner.clone();
                move |key, cx| {
                    owner
                        .upgrade()
                        .map(|app| app.read(cx).selectable_text_scroll_handle(key))
                        .unwrap_or_default()
                }
            }),
            dispatch: Rc::new(move |action, window, cx| {
                let _ = owner.update(cx, |app, cx| {
                    app.handle_terminal_command_specs_action(action, window, cx)
                });
            }),
        };
        bindings
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
        use super::terminal_command_bar::completion::{
            normalize_terminal_command_specs_json, terminal_command_specs_example_json,
            terminal_command_specs_path,
        };
        let path = terminal_command_specs_path(self.settings_store.path());
        let result = self.settings_workspace.update(cx, |settings, cx| {
            settings.apply_command_specs_action(
                action,
                &path,
                terminal_command_specs_example_json,
                normalize_terminal_command_specs_json,
                || {
                    oxideterm_terminal::command_completion::invalidate_user_terminal_fig_specs(
                        self.settings_store.path(),
                    );
                    self.terminal_command_specs_summary()
                },
                &self.i18n,
                cx,
            )
        });
        match result {
            CommandSpecsOutcome::Closed => {
                self.clear_ime_selection();
                self.ime_marked_text = None;
            }
            CommandSpecsOutcome::Focus => {
                self.clear_ime_selection();
                self.ime_marked_text = None;
                self.show_active_input_caret(cx);
                window.focus(&self.focus_handle, cx);
            }
            CommandSpecsOutcome::Saved => self.push_ai_settings_toast(
                self.i18n.t("settings_view.terminal.command_specs_saved"),
                TerminalNoticeVariant::Success,
                cx,
            ),
            CommandSpecsOutcome::Error(error) => {
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx)
            }
            CommandSpecsOutcome::Inactive => {}
        }
    }
}
