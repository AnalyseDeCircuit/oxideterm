use super::*;
use oxideterm_environment::archive::{
    ArchiveCapabilities, ArchiveCommandError, ArchiveShell, plan_archive_command,
};
use oxideterm_gpui_editor::{
    EditorContextMenuLabels, EditorPresentation, EditorSettings, TextEditorView,
};
use oxideterm_gpui_ui::button::{ButtonOptions, ButtonSize, ButtonVariant, button_with};
use oxideterm_gpui_ui::modal::{
    modal_body, modal_container, modal_footer, modal_header, modal_overlay,
};
use oxideterm_gpui_ui::scroll::ScrollableElement;
use zeroize::Zeroizing;

mod probe;

pub(super) struct ArchiveCommandPreview {
    pane_id: PaneId,
    pane: gpui::WeakEntity<TerminalPane>,
    connection_id: Option<String>,
    cwd: Option<String>,
    local_process_cwd: bool,
    prompt_at: Option<u64>,
    path: Zeroizing<String>,
    target: String,
    shell: Option<ArchiveShell>,
    capabilities: Option<ArchiveCapabilities>,
    editor: Entity<TextEditorView>,
    summary: Option<(String, String)>,
    error: Option<&'static str>,
    loading: bool,
    choosing_shell: bool,
    job: Option<tokio::task::AbortHandle>,
    delivery: Option<Task<()>>,
}

impl Drop for ArchiveCommandPreview {
    fn drop(&mut self) {
        // The preview owns the read-only probe, including cancellation on close or replacement.
        if let Some(job) = self.job.take() {
            job.abort();
        }
    }
}

fn archive_cwd(pane: &TerminalPane, local_process: bool) -> Option<String> {
    if pane.current_working_directory_is_pending() {
        return None;
    }
    if local_process {
        if let Some(cwd) = pane.process_info().cwd {
            return Some(cwd.to_string_lossy().into_owned());
        }
        return super::terminal_cwd::terminal_cwd_snapshot_from_pane(
            oxideterm_environment::CurrentDirectoryScope::Local,
            pane,
        )
        .map(|snapshot| snapshot.path().to_string());
    }
    matches!(
        pane.current_working_directory_source(),
        Some(
            TerminalWorkingDirectorySource::ShellIntegration
                | TerminalWorkingDirectorySource::VisibleCommand
        )
    )
    .then(|| pane.current_working_directory())
    .flatten()
}

impl WorkspaceApp {
    pub(super) fn open_archive_command(
        &mut self,
        pane_id: PaneId,
        pane: Entity<TerminalPane>,
        path: Zeroizing<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prepare_modal_interaction_boundary(cx);
        let source = pane.read(cx);
        let connection = source.ssh_connection_handle();
        let launch_shell = source.shared_session().lock().launch_shell();
        let process_probe = source.process_info_probe();
        let shell = launch_shell
            .as_ref()
            .and_then(|shell| {
                ArchiveShell::detect(&shell.id)
                    .or_else(|| shell.path.to_str().and_then(ArchiveShell::detect))
            })
            .or_else(|| {
                connection
                    .as_ref()?
                    .remote_env()?
                    .shell
                    .as_deref()
                    .and_then(ArchiveShell::detect)
            });
        let local_process_cwd = cfg!(unix)
            && launch_shell
                .as_ref()
                .is_some_and(|shell| !shell.id.starts_with("wsl"));
        let cwd = archive_cwd(source, local_process_cwd);
        let invalid_context = !source.lifecycle().is_running()
            || source.is_tmux_control_mode()
            || source.current_working_directory_is_pending()
            || (source.shell_integration_status().detected
                && source.shell_integration_status().state
                    == oxideterm_terminal::ShellIntegrationLifecycleState::Output);
        let prompt_at = source.shell_integration_status().last_seen_at;
        let target = source.title().to_string();
        let tokens = self.tokens;
        let labels = EditorContextMenuLabels {
            copy: self.i18n.t("menu.copy"),
            cut: self.i18n.t("fileManager.cut"),
            paste: self.i18n.t("menu.paste"),
            select_all: self.i18n.t("fileManager.selectAll"),
        };
        let editor = cx.new(|cx| {
            let mut editor = TextEditorView::new_verbatim(String::new(), &tokens, cx);
            editor.set_presentation(EditorPresentation::Inline, cx);
            editor.set_settings(
                EditorSettings {
                    soft_wrap: true,
                    soft_wrap_column: None,
                    indentation_markers: false,
                    highlight_special_chars: false,
                    highlight_current_line: false,
                    placeholder: None,
                    ..Default::default()
                },
                cx,
            );
            editor.set_context_menu_labels(labels);
            editor.set_read_only(true);
            editor
        });
        let preview = ArchiveCommandPreview {
            pane_id,
            pane: pane.downgrade(),
            connection_id: connection
                .as_ref()
                .map(|connection| connection.connection_id().to_string()),
            cwd,
            local_process_cwd,
            prompt_at,
            path,
            target,
            shell,
            capabilities: None,
            editor: editor.clone(),
            summary: None,
            error: invalid_context.then_some("terminal.archive.context_unavailable"),
            loading: !invalid_context,
            choosing_shell: false,
            job: None,
            delivery: None,
        };
        self.archive_command = Some(preview);
        if invalid_context {
            cx.notify();
            return;
        }
        let job =
            self.workspace_runtime
                .read(cx)
                .task_runtime()
                .spawn(probe::probe_archive_environment(
                    connection,
                    launch_shell,
                    process_probe,
                ));
        let abort = job.abort_handle();
        let editor_id = editor.entity_id();
        let delivery = cx.spawn(async move |weak, cx| {
            let outcome = job.await;
            let _ = weak.update(cx, |this, cx| {
                let Some(preview) = this.archive_command.as_mut() else {
                    return;
                };
                if preview.editor.entity_id() != editor_id {
                    return;
                }
                preview.loading = false;
                preview.job = None;
                match outcome {
                    Ok(Ok((capabilities, detected_shell, process_info))) => {
                        if let Some(info) = process_info
                            && let Some(pane) = preview.pane.upgrade()
                        {
                            pane.update(cx, |pane, _| pane.apply_process_info(info));
                            preview.cwd = archive_cwd(pane.read(cx), preview.local_process_cwd);
                        }
                        preview.shell = detected_shell.or(preview.shell);
                        preview.capabilities = Some(capabilities);
                        this.refresh_archive_command(cx);
                    }
                    _ => preview.error = Some("terminal.archive.probe_failed"),
                }
                cx.notify();
            });
        });
        if let Some(preview) = self.archive_command.as_mut() {
            preview.job = Some(abort);
            preview.delivery = Some(delivery);
        }
        window.focus(&editor.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    fn refresh_archive_command(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = self.archive_command.as_mut() else {
            return;
        };
        let Some(capabilities) = preview.capabilities.as_ref() else {
            return;
        };
        preview.summary = None;
        let result = preview
            .shell
            .ok_or("terminal.archive.choose_shell")
            .and_then(|shell| {
                plan_archive_command(&preview.path, preview.cwd.as_deref(), shell, capabilities)
                    .map_err(|error| match error {
                        ArchiveCommandError::UnsupportedFormat => "terminal.archive.unsupported",
                        ArchiveCommandError::InvalidPath => "terminal.archive.invalid_path",
                        ArchiveCommandError::DirectoryUnavailable => {
                            "terminal.archive.cwd_unavailable"
                        }
                        ArchiveCommandError::ToolUnavailable => "terminal.archive.tool_unavailable",
                    })
            });
        match result {
            Ok(plan) => {
                preview.error = None;
                preview.summary = Some((plan.archive, plan.destination));
                preview.editor.update(cx, |editor, cx| {
                    editor.replace_text_external(plan.command, cx);
                    editor.set_read_only(false);
                });
            }
            Err(key) => {
                preview.error = Some(key);
                preview.editor.update(cx, |editor, cx| {
                    editor.replace_text_external(String::new(), cx);
                    editor.set_read_only(true);
                });
            }
        }
        cx.notify();
    }

    pub(super) fn close_archive_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.archive_command = None;
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    fn fill_archive_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(preview) = self.archive_command.as_ref() else {
            return;
        };
        if preview.loading || preview.error.is_some() {
            return;
        }
        let valid = preview.pane.upgrade().is_some_and(|pane| {
            let pane = pane.read(cx);
            pane.lifecycle().is_running()
                && pane
                    .ssh_connection_handle()
                    .map(|connection| connection.connection_id().to_string())
                    == preview.connection_id
                && pane.shell_integration_status().last_seen_at == preview.prompt_at
                && archive_cwd(pane, preview.local_process_cwd) == preview.cwd
        });
        if !valid || self.active_pane_id(cx) != Some(preview.pane_id) {
            self.archive_command.as_mut().unwrap().error = Some("terminal.archive.context_changed");
            cx.notify();
            return;
        }
        let command = Zeroizing::new(preview.editor.read(cx).buffer().text());
        if command.trim().is_empty() {
            return;
        }
        let pane_id = preview.pane_id;
        // A separate draft preserves existing input and stays pinned to the clicked pane.
        let sender_id = self.terminal_command_sender.update(cx, |sender, cx| {
            let id = sender.add_document(cx);
            sender.replace_active_text(command.to_string(), cx);
            sender.toggle_selected_target(id, pane_id, cx);
            id
        });
        self.archive_command = None;
        self.close_terminal_command_overlays(cx);
        self.focus_terminal_command_sender_input(sender_id, window, cx);
        cx.notify();
    }

    pub(super) fn render_archive_command(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let preview = self.archive_command.as_ref()?;
        let mut body = div()
            .flex()
            .flex_col()
            .gap_4()
            .text_size(px(self.tokens.metrics.ui_text_sm));
        if let Some((archive, destination)) = &preview.summary {
            body = body.child(
                div().flex().flex_col().gap_2().children(
                    [
                        ("terminal.archive.source", archive),
                        ("terminal.archive.destination", destination),
                    ]
                    .into_iter()
                    .map(|(label, value)| {
                        div()
                            .flex()
                            .gap_3()
                            .child(
                                div()
                                    .w(px(80.0))
                                    .flex_shrink_0()
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(self.i18n.t(label)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .whitespace_normal()
                                    .child(value.clone()),
                            )
                    }),
                ),
            );
        }
        if preview.loading {
            body = body.child(
                div()
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("terminal.archive.probing")),
            );
        } else if preview.choosing_shell
            || preview.shell.is_none()
            || (preview.error.is_some() && preview.capabilities.is_some())
        {
            body = body.child(
                div().flex().flex_wrap().gap_2().children(
                    [
                        (ArchiveShell::Posix, "terminal.archive.shell_posix"),
                        (ArchiveShell::Fish, "terminal.archive.shell_fish"),
                        (
                            ArchiveShell::PowerShell,
                            "terminal.archive.shell_powershell",
                        ),
                        (ArchiveShell::Cmd, "terminal.archive.shell_cmd"),
                    ]
                    .into_iter()
                    .filter_map(|(shell, label)| {
                        let supported_platform =
                            preview.capabilities.as_ref().is_some_and(|caps| {
                                if caps.windows {
                                    matches!(shell, ArchiveShell::PowerShell | ArchiveShell::Cmd)
                                } else {
                                    shell != ArchiveShell::Cmd
                                }
                            });
                        if !supported_platform {
                            return None;
                        }
                        Some(
                            button_with(
                                &self.tokens,
                                self.i18n.t(label),
                                ButtonOptions {
                                    variant: if preview.shell == Some(shell) {
                                        ButtonVariant::Default
                                    } else {
                                        ButtonVariant::Outline
                                    },
                                    disabled: preview.loading || !supported_platform,
                                    size: ButtonSize::Sm,
                                    ..Default::default()
                                },
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    if let Some(preview) = this.archive_command.as_mut()
                                        && !preview.loading
                                        && supported_platform
                                        && preview.capabilities.is_some()
                                    {
                                        preview.shell = Some(shell);
                                        preview.choosing_shell = false;
                                        this.refresh_archive_command(cx);
                                    }
                                    cx.stop_propagation();
                                }),
                            ),
                        )
                    }),
                ),
            );
        }
        if let Some(error) = preview.error {
            body = body.child(
                div()
                    .text_color(rgb(self.tokens.ui.error))
                    .child(self.i18n.t(error)),
            );
        }
        if preview.summary.is_some() {
            let shell_label = match preview.shell {
                Some(ArchiveShell::Posix) => "terminal.archive.shell_posix",
                Some(ArchiveShell::Fish) => "terminal.archive.shell_fish",
                Some(ArchiveShell::PowerShell) => "terminal.archive.shell_powershell",
                Some(ArchiveShell::Cmd) => "terminal.archive.shell_cmd",
                None => "terminal.archive.choose_shell",
            };
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(self.i18n.t("terminal.archive.command"))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap_2()
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(self.i18n.t(shell_label))
                                    .child(
                                        button_with(
                                            &self.tokens,
                                            self.i18n.t("terminal.archive.change_shell"),
                                            ButtonOptions {
                                                variant: ButtonVariant::Ghost,
                                                size: ButtonSize::Sm,
                                                ..Default::default()
                                            },
                                        )
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _, _, cx| {
                                                if let Some(preview) = this.archive_command.as_mut()
                                                {
                                                    preview.choosing_shell =
                                                        !preview.choosing_shell;
                                                    cx.notify();
                                                }
                                                cx.stop_propagation();
                                            }),
                                        ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .h(px(120.0))
                            .flex_shrink_0()
                            .min_w_0()
                            .p_3()
                            .border_1()
                            .border_color(rgb(self.tokens.ui.border))
                            .rounded(px(self.tokens.radii.sm))
                            .child(preview.editor.clone()),
                    ),
            );
        }
        let disabled = preview.loading || preview.error.is_some();
        let dialog = modal_container(&self.tokens)
            .w(px(640.0_f32.min(
                (f32::from(window.viewport_size().width) - 32.0).max(0.0),
            )))
            .max_h(px(
                (f32::from(window.viewport_size().height) - 32.0).max(0.0)
            ))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(modal_header(
                &self.tokens,
                self.i18n.t("terminal.archive.title"),
                preview.target.clone(),
            ))
            .child(
                modal_body(&self.tokens)
                    .id("archive-command-body")
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(body),
            )
            .child(
                modal_footer(&self.tokens)
                    .flex_shrink_0()
                    .child(
                        button_with(
                            &self.tokens,
                            self.i18n.t("common.cancel"),
                            ButtonOptions::default(),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.close_archive_command(window, cx)
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            &self.tokens,
                            self.i18n.t("terminal.archive.fill"),
                            ButtonOptions {
                                disabled,
                                variant: ButtonVariant::Default,
                                ..Default::default()
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.fill_archive_command(window, cx)
                            }),
                        ),
                    ),
            );
        Some(modal_overlay(&self.tokens, dialog))
    }
}
