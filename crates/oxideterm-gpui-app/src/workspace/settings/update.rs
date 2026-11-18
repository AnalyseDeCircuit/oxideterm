use super::*;
use oxideterm_gpui_settings_view::native_update::{NativeUpdateCheckKind, NativeUpdateCheckRequest};

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_settings_workspace_event(
        &mut self,
        settings: Entity<SettingsWorkspaceEntity>,
        event: &SettingsWorkspaceEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SettingsWorkspaceEvent::ExternalStoresChanged(change) => {
                let _ = self.reload_watched_stores(*change, cx);
            }
            SettingsWorkspaceEvent::HighlightDraftChanged(input) => {
                let Some(draft) = settings.read(cx).settings_entity_input_value(*input) else {
                    return;
                };
                let mut next = self.settings_store.settings().clone();
                if matches!(
                    apply_persisted_settings_input_draft(&mut next, *input, draft),
                    SettingsInputDraftApply::Applied
                ) && &next != self.settings_store.settings()
                {
                    if let Ok(saved) = self.settings_store.replace_and_save(next) {
                        settings.update(cx, |settings, cx| {
                            settings.acknowledge_external_store_state();
                            settings.queue_highlight_preferences_refresh(cx);
                        });
                        if let Ok(value) = serde_json::to_value(saved.settings) {
                            self.emit_native_plugin_app_settings_changed(value, cx);
                        }
                    } else {
                        eprintln!("failed to save terminal highlight settings");
                    }
                }
            }
            SettingsWorkspaceEvent::RefreshTerminalHighlightPreferences => {
                self.apply_terminal_preferences_to_panes(cx);
            }
            SettingsWorkspaceEvent::ResetNativeUpdateOverlay => {
                self.native_update_notification_open = false;
                self.native_update_notification_presence.reopen();
                self.overlay.update(cx, |overlay, cx| {
                    if overlay.confirm_snapshot().is_some_and(|snapshot| {
                        matches!(
                            snapshot.kind,
                            WorkspaceOverlayConfirmKind::NativeUpdateReleaseNotes
                        )
                    }) {
                        overlay.begin_confirm_exit(false, Duration::ZERO, cx);
                    }
                });
                // A new package must not inherit an old changelog scroll position.
                self.native_update_release_notes_scroll = MarkdownVirtualListScrollHandle::new();
                cx.notify();
            }
            SettingsWorkspaceEvent::ShowNativeUpdateNotification => {
                self.show_native_update_notification();
                cx.notify();
            }
            SettingsWorkspaceEvent::ShowNativeUpdateToast(toast) => {
                let portable_replacement = self.native_update_is_portable(cx)
                    && matches!(
                        settings.read(cx).native_update_render_state(),
                        NativeUpdateRenderState::InstallFinished {
                            status: oxideterm_update::NativeInstallStatus::ReplacementScheduled,
                            ..
                        }
                    );
                let message = if portable_replacement {
                    self.i18n.t("settings_view.help.replacement_scheduled")
                } else {
                    let Some(message) =
                        settings.read(cx).native_update_message().map(str::to_owned)
                    else {
                        return;
                    };
                    message
                };
                let variant = match toast {
                    SettingsWorkspaceToast::Success => TerminalNoticeVariant::Success,
                    SettingsWorkspaceToast::Warning => TerminalNoticeVariant::Warning,
                    SettingsWorkspaceToast::Error => TerminalNoticeVariant::Error,
                };
                self.push_ai_settings_toast(message, variant, cx);
            }
            SettingsWorkspaceEvent::RequestAutomaticNativeUpdateCheck => {
                self.check_native_update_with_kind(NativeUpdateCheckKind::Automatic, cx);
            }
            SettingsWorkspaceEvent::RequestQuitAfterNativeUpdate => {
                self.schedule_native_update_quit(cx);
            }
            SettingsWorkspaceEvent::DataDirectoryConfirmOpened => {
                self.reset_standard_confirm_focus();
                cx.notify();
            }
            SettingsWorkspaceEvent::DataDirectoryOperationReady => {
                let results =
                    settings.update(cx, |settings, _cx| settings.take_data_directory_results());
                for result in results {
                    match result {
                        DataDirectoryOperationResult::Changed => {
                            self.push_ai_settings_toast(
                                self.i18n.t("settings_view.general.data_directory_changed"),
                                TerminalNoticeVariant::Success,
                                cx,
                            );
                        }
                        DataDirectoryOperationResult::Reset => {
                            self.push_ai_settings_toast(
                                self.i18n.t("settings_view.general.data_directory_reset"),
                                TerminalNoticeVariant::Success,
                                cx,
                            );
                        }
                        DataDirectoryOperationResult::Failed(error) => {
                            self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                        }
                    }
                }
                cx.notify();
            }
            SettingsWorkspaceEvent::BackgroundBlurCommitReady(system_dark, value) => {
                let value = *value;
                let system_dark = *system_dark;
                self.edit_settings(
                    |settings| {
                        let mut style = settings.terminal.background_for_scheme(system_dark);
                        style.blur = value;
                        settings
                            .terminal
                            .set_background_for_scheme(system_dark, style);
                    },
                    cx,
                );
            }
            SettingsWorkspaceEvent::BackgroundGalleryOperationReady => {
                let results = settings.update(cx, |settings, _cx| {
                    settings.take_background_gallery_results()
                });
                for result in results {
                    match result {
                        BackgroundGalleryOperationResult::Updated(system_dark, active_path) => {
                            let gallery = settings.read(cx).background_images_snapshot();
                            let settings_path = self.settings_store.path().to_path_buf();
                            self.edit_settings(
                                move |settings| {
                                    let mut style =
                                        settings.terminal.background_for_scheme(system_dark);
                                    style.image = active_path;
                                    settings
                                        .terminal
                                        .set_background_for_scheme(system_dark, style);
                                    // Gallery deletion removes references in both system modes.
                                    let keep = |path: &str| {
                                        !oxideterm_settings::is_managed_background_image(
                                            &settings_path,
                                            Path::new(path),
                                        ) || gallery.iter().any(|image| image == path)
                                    };
                                    if settings
                                        .terminal
                                        .background_image
                                        .as_deref()
                                        .is_some_and(|path| !keep(path))
                                    {
                                        settings.terminal.background_image = None;
                                    }
                                    for style in [
                                        &mut settings.terminal.system_backgrounds.light,
                                        &mut settings.terminal.system_backgrounds.dark,
                                    ]
                                    .into_iter()
                                    .flatten()
                                    {
                                        if style.image.as_deref().is_some_and(|path| !keep(path)) {
                                            style.image = None;
                                        }
                                    }
                                },
                                cx,
                            );
                        }
                        BackgroundGalleryOperationResult::Failed => {
                            self.send_settings_notice(
                                self.i18n.t("settings_view.terminal.bg_operation_failed"),
                                TerminalNoticeVariant::Error,
                                cx,
                            );
                        }
                        BackgroundGalleryOperationResult::MediaFailed(failure) => {
                            let key = match failure {
                                oxideterm_gpui_background::BackgroundFailure::Unsupported => {
                                    "bg_unsupported"
                                }
                                oxideterm_gpui_background::BackgroundFailure::ResourceExhausted => {
                                    "bg_resource_exhausted"
                                }
                                _ => "bg_decode_failed",
                            };
                            self.send_settings_notice(
                                self.i18n.t(&format!("settings_view.terminal.{key}")),
                                TerminalNoticeVariant::Error,
                                cx,
                            );
                        }
                    }
                }
                cx.notify();
            }
            SettingsWorkspaceEvent::ThemeImportReady => {
                let results =
                    settings.update(cx, |settings, _cx| settings.take_theme_import_results());
                for result in results {
                    match result {
                        ThemeImportResult::Imported {
                            target,
                            system_dark,
                            theme_id,
                            name,
                            value,
                        } => {
                            // The identifier is persisted both as the map key
                            // and as the active theme, requiring two owners.
                            let selected_theme_id = theme_id.clone();
                            self.edit_settings(
                                move |settings| {
                                    settings.custom_themes.insert(theme_id, value);
                                    target.apply_for_scheme(
                                        settings,
                                        system_dark,
                                        selected_theme_id,
                                    );
                                },
                                cx,
                            );
                            self.send_settings_notice(
                                self.i18n
                                    .t("settings_view.appearance.theme_import_success")
                                    .replace("{{name}}", &name),
                                TerminalNoticeVariant::Success,
                                cx,
                            );
                        }
                        ThemeImportResult::Failed(error) => {
                            self.send_settings_notice(
                                self.i18n
                                    .t("settings_view.appearance.theme_import_error")
                                    .replace("{{error}}", &error),
                                TerminalNoticeVariant::Error,
                                cx,
                            );
                        }
                    }
                }
                cx.notify();
            }
            SettingsWorkspaceEvent::ThemeEditorOperationReady => {
                let results =
                    settings.update(cx, |settings, _cx| settings.take_theme_editor_results());
                for result in results {
                    match result {
                        ThemeEditorOperationResult::Save(editor) => {
                            let mut saved_name = None;
                            self.edit_settings(
                                |settings| {
                                    saved_name =
                                        save_theme_editor_snapshot_to_settings(settings, &editor);
                                },
                                cx,
                            );
                            if let Some(name) = saved_name {
                                self.send_settings_notice(
                                    self.i18n
                                        .t("settings_view.appearance.theme_import_success")
                                        .replace("{{name}}", &name),
                                    TerminalNoticeVariant::Success,
                                    cx,
                                );
                            }
                        }
                        ThemeEditorOperationResult::Delete(editor) => {
                            self.edit_settings(
                                move |settings| {
                                    if let Some(theme_id) = editor.edit_theme_id.as_deref() {
                                        delete_custom_theme_from_settings(
                                            settings,
                                            theme_id,
                                            oxideterm_theme::DEFAULT_THEME.id,
                                        );
                                    }
                                },
                                cx,
                            );
                        }
                    }
                }
                cx.notify();
            }
            SettingsWorkspaceEvent::KeybindingFileOperationReady => {
                let results = settings.update(cx, |settings, _cx| {
                    settings.take_keybinding_file_operation_results()
                });
                for result in results {
                    match result {
                        KeybindingFileOperationResult::Exported => {
                            self.push_ai_settings_toast(
                                self.i18n.t("settings_view.keybindings.export_success"),
                                TerminalNoticeVariant::Success,
                                cx,
                            );
                        }
                        KeybindingFileOperationResult::ExportFailed => {
                            self.push_ai_settings_toast(
                                self.i18n.t("settings_view.keybindings.export_error"),
                                TerminalNoticeVariant::Error,
                                cx,
                            );
                        }
                        KeybindingFileOperationResult::Imported {
                            overrides: next_overrides,
                        } => {
                            let side = crate::keybindings::KeybindingSide::current();
                            let runtime_bindings = {
                                let previous_overrides =
                                    &self.settings_store.settings().keybindings.overrides;
                                crate::keybindings::ACTION_DEFINITIONS
                                    .iter()
                                    .flat_map(|definition| {
                                        let previous = crate::keybindings::effective_combo(
                                            definition,
                                            previous_overrides,
                                            side,
                                        );
                                        let next = crate::keybindings::effective_combo(
                                            definition,
                                            &next_overrides,
                                            side,
                                        );
                                        crate::keybindings::runtime_rebind_key_bindings(
                                            &definition.id,
                                            previous.as_ref(),
                                            next.as_ref(),
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            };
                            self.edit_settings(
                                move |settings| {
                                    settings.keybindings.overrides = next_overrides;
                                },
                                cx,
                            );
                            Self::apply_runtime_key_bindings(runtime_bindings, cx);
                            self.push_ai_settings_toast(
                                self.i18n.t("settings_view.keybindings.import_success"),
                                TerminalNoticeVariant::Success,
                                cx,
                            );
                        }
                        KeybindingFileOperationResult::ImportFailed => {
                            self.push_ai_settings_toast(
                                self.i18n.t("settings_view.keybindings.import_invalid"),
                                TerminalNoticeVariant::Error,
                                cx,
                            );
                        }
                    }
                }
                cx.notify();
            }
            SettingsWorkspaceEvent::PortablePasswordChangeFinished { success } => {
                if *success {
                    self.push_ai_settings_toast(
                        self.i18n
                            .t("settings_view.general.portable_password_changed"),
                        TerminalNoticeVariant::Success,
                        cx,
                    );
                    self.refresh_portable_settings_snapshot(true, cx);
                } else if let Some(error) =
                    settings.read(cx).portable_action_error().map(str::to_owned)
                {
                    self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                }
            }
            SettingsWorkspaceEvent::CliCompanionFinished { operation, success } => {
                if *operation == CliCompanionOperation::Refresh {
                    return;
                }
                if *success {
                    let message_key = match operation {
                        CliCompanionOperation::Install => "settings_view.general.cli_installed",
                        CliCompanionOperation::Uninstall => "settings_view.general.cli_uninstalled",
                        CliCompanionOperation::UninstallLegacy => {
                            "migration.cli_legacy_uninstalled"
                        }
                        CliCompanionOperation::Migrate => "migration.cli_migrated",
                        CliCompanionOperation::Refresh => unreachable!("handled above"),
                    };
                    self.push_ai_settings_toast(
                        self.i18n.t(message_key),
                        TerminalNoticeVariant::Success,
                        cx,
                    );
                } else if let Some(error) =
                    settings.read(cx).cli_companion_error().map(str::to_owned)
                {
                    self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                }
            }
        }
    }

    pub(in crate::workspace) fn check_native_update(&mut self, cx: &mut Context<Self>) {
        self.check_native_update_with_kind(NativeUpdateCheckKind::Manual, cx);
    }

    pub(in crate::workspace) fn schedule_automatic_native_update_check(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.schedule_automatic_native_update_check(cx);
        });
    }

    fn check_native_update_with_kind(
        &mut self,
        check_kind: NativeUpdateCheckKind,
        cx: &mut Context<Self>,
    ) {
        let channel = self.settings_store.settings().general.update_channel;
        let install_flavor =
            oxideterm_update::NativeInstallContext::current(self.native_update_is_portable(cx))
                .map(|context| context.install_flavor)
                .map_err(|error| error.to_string());
        let request = NativeUpdateCheckRequest {
            kind: check_kind,
            channel,
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            install_flavor,
            update_proxy: self.settings_store.settings().general.update_proxy.clone(),
            runtime: self.forwarding_runtime.clone(),
        };
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_native_update_check(request, cx);
        });
    }

    pub(in crate::workspace) fn download_native_update(&mut self, cx: &mut Context<Self>) {
        let directory = self.native_update_download_directory();
        let update_proxy = self.settings_store.settings().general.update_proxy.clone();
        let runtime = self.forwarding_runtime.clone();
        let error_fallback = self.i18n.t("settings_view.help.update_error");
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_native_update_download(
                directory,
                update_proxy,
                runtime,
                error_fallback,
                cx,
            );
        });
    }

    pub(in crate::workspace) fn install_native_update(&mut self, cx: &mut Context<Self>) {
        let context =
            oxideterm_update::NativeInstallContext::current(self.native_update_is_portable(cx))
                .map_err(|error| error.to_string());
        let cleanup_directory = self.native_update_download_directory();
        let runtime = self.forwarding_runtime.clone();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_native_update_install(context, cleanup_directory, runtime, cx);
        });
    }

    pub(in crate::workspace) fn cancel_native_update(&mut self, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.cancel_native_update(cx);
        });
    }

    pub(in crate::workspace) fn schedule_native_update_quit(&mut self, cx: &mut Context<Self>) {
        // Tauri's updater exits after platform installers that need the current
        // process out of the way. Delay one frame so the final toast/state can
        // render before GPUI begins app shutdown.
        cx.spawn(async move |weak, cx| {
            Timer::after(std::time::Duration::from_millis(750)).await;
            let _ = weak.update(cx, |workspace, cx| {
                workspace.quit_after_confirmation(cx);
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn native_update_download_directory(&self) -> std::path::PathBuf {
        self.settings_store
            .path()
            .parent()
            .map(|parent| parent.join("updates"))
            .unwrap_or_else(|| std::path::PathBuf::from("updates"))
    }

    pub(in crate::workspace) fn native_update_is_portable(&self, cx: &App) -> bool {
        // The portable runtime marker is the persisted source of truth. The
        // cached snapshot avoids repeating filesystem detection when available.
        self.settings_workspace
            .read(cx)
            .portable_mode()
            .unwrap_or_else(|| oxideterm_portable_runtime::is_portable_mode().unwrap_or(false))
    }
}
