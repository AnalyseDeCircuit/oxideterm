use super::*;
use oxideterm_gpui_settings_view::entity::SettingsCompletion;
use oxideterm_gpui_settings_view::native_update::{NativeUpdateAction, NativeUpdateCheckKind, NativeUpdateEnvironment};

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
                self.settings_views.release_notes_scroll = MarkdownVirtualListScrollHandle::new();
                cx.notify();
            }
            SettingsWorkspaceEvent::ShowNativeUpdateNotification => {}
            SettingsWorkspaceEvent::RequestAutomaticNativeUpdateCheck => {
                self.apply_native_update_action(NativeUpdateAction::Check(NativeUpdateCheckKind::Automatic), cx);
            }
            SettingsWorkspaceEvent::RequestQuitAfterNativeUpdate => {
                self.schedule_native_update_quit(cx);
            }
            SettingsWorkspaceEvent::DataDirectoryConfirmOpened => {
                self.reset_standard_confirm_focus();
                cx.notify();
            }
            _ => {
                let completions = settings.update(cx, |settings, _cx| {
                    settings.take_completions(*event, self.settings_store.path(), &self.i18n)
                });
                for completion in completions {
                    match completion {
                        SettingsCompletion::Edit(edit) => self.edit_settings(edit, cx),
                        SettingsCompletion::SaveTheme(save) => {
                            let mut message = None;
                            self.edit_settings(|settings| message = save(settings), cx);
                            if let Some(message) = message {
                                self.send_settings_notice(message, TerminalNoticeVariant::Success, cx);
                            }
                        }
                        SettingsCompletion::ApplyKeybindings(overrides) => {
                            self.apply_settings_keybinding_import(overrides, cx);
                        }
                        SettingsCompletion::Notice(message, variant) => {
                            let variant = match variant {
                                SettingsWorkspaceToast::Success => TerminalNoticeVariant::Success,
                                SettingsWorkspaceToast::Warning => TerminalNoticeVariant::Warning,
                                SettingsWorkspaceToast::Error => TerminalNoticeVariant::Error,
                            };
                            self.send_settings_notice(message, variant, cx);
                        }
                        SettingsCompletion::RefreshPortableStatus => self.refresh_portable_settings_snapshot(true, cx),
                    }
                }
            }
        }
    }

    fn apply_settings_keybinding_import(
        &mut self, overrides: serde_json::Map<String, serde_json::Value>, cx: &mut Context<Self>,
    ) {
        use crate::keybindings::{ACTION_DEFINITIONS, KeybindingSide, effective_combo, runtime_rebind_key_bindings};
        let side = KeybindingSide::current();
        let previous = &self.settings_store.settings().keybindings.overrides;
        let bindings = ACTION_DEFINITIONS.iter().flat_map(|definition| {
            let old = effective_combo(definition, previous, side);
            let new = effective_combo(definition, &overrides, side);
            runtime_rebind_key_bindings(&definition.id, old.as_ref(), new.as_ref())
        }).collect::<Vec<_>>();
        self.edit_settings(|settings| settings.keybindings.overrides = overrides, cx);
        Self::apply_runtime_key_bindings(bindings, cx);
    }

    pub(in crate::workspace) fn check_native_update(&mut self, cx: &mut Context<Self>) {
        self.apply_native_update_action(NativeUpdateAction::Check(NativeUpdateCheckKind::Manual), cx);
    }

    pub(in crate::workspace) fn schedule_automatic_native_update_check(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.schedule_automatic_native_update_check(cx);
        });
    }

    pub(in crate::workspace) fn apply_native_update_action(
        &self,
        action: NativeUpdateAction,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.apply_native_update_action(
                action,
                NativeUpdateEnvironment {
                    general: &self.settings_store.settings().general,
                    settings_path: self.settings_store.path(),
                    current_version: env!("CARGO_PKG_VERSION"),
                    runtime: self.forwarding_runtime.clone(),
                },
                &self.i18n,
                cx,
            );
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

    pub(in crate::workspace) fn native_update_is_portable(&self, cx: &App) -> bool {
        self.settings_workspace.read(cx).native_update_is_portable()
    }
}
