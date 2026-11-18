use super::*;
use oxideterm_gpui_settings_view::native_update::NativeUpdateAction;

use oxideterm_gpui_settings_view::help_page::SettingsMarkdownDialog;

impl WorkspaceApp {
    pub(in crate::workspace::settings) fn handle_help_request(
        &mut self,
        request: oxideterm_gpui_settings_view::help_page::HelpRequest,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_settings_view::help_page::HelpRequest;
        match request {
            HelpRequest::Check => self.check_native_update(cx),
            HelpRequest::Download => {
                self.apply_native_update_action(NativeUpdateAction::Download, cx)
            }
            HelpRequest::Install => {
                self.apply_native_update_action(NativeUpdateAction::Install, cx)
            }
            HelpRequest::Cancel => self.apply_native_update_action(NativeUpdateAction::Cancel, cx),
            HelpRequest::ReleaseNotes => self.open_native_update_release_notes(cx),
            HelpRequest::Legal => self.open_help_legal_notice(cx),
            HelpRequest::ThirdParty => {
                self.settings_views.legal_notice_scroll = MarkdownVirtualListScrollHandle::new();
                self.overlay.update(cx, |overlay, cx| {
                    overlay.open_confirm(WorkspaceOverlayConfirmKind::ThirdPartyNotices, cx);
                });
            }
            HelpRequest::Logs => self.open_help_log_directory(cx),
            HelpRequest::Url(url) => self.open_help_url(url, cx),
        }
    }

    pub(in crate::workspace) fn open_help_legal_notice(&mut self, cx: &mut Context<Self>) {
        self.settings_views.legal_notice_scroll = MarkdownVirtualListScrollHandle::new();
        self.overlay.update(cx, |overlay, cx| {
            overlay.open_confirm(WorkspaceOverlayConfirmKind::LegalNotice, cx);
        });
    }

    pub(in crate::workspace) fn close_help_legal_notice(&mut self, cx: &mut Context<Self>) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.overlay.update(cx, |overlay, cx| {
            overlay.begin_confirm_exit(false, delay, cx);
        });
    }

    pub(in crate::workspace) fn handle_help_legal_notice_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(snapshot) = self.overlay.read(cx).confirm_snapshot() else {
            return false;
        };
        if !matches!(
            snapshot.kind,
            WorkspaceOverlayConfirmKind::LegalNotice
                | WorkspaceOverlayConfirmKind::ThirdPartyNotices
        ) {
            return false;
        }
        if snapshot.phase == oxideterm_gpui_ui::motion::ExitPhase::Exiting {
            return true;
        }
        let key_action = self.overlay.update(cx, |overlay, cx| {
            overlay.handle_confirm_key(
                event.keystroke.key.as_str(),
                event.keystroke.modifiers.shift,
                event.keystroke.modifiers.platform || event.keystroke.modifiers.control,
                cx,
            )
        });
        match key_action {
            Some(
                WorkspaceOverlayConfirmKeyAction::Cancel
                | WorkspaceOverlayConfirmKeyAction::Confirm,
            ) => {
                self.close_help_legal_notice(cx);
                true
            }
            Some(WorkspaceOverlayConfirmKeyAction::Handled) => true,
            None => false,
        }
    }

    pub(in crate::workspace) fn render_help_legal_notice_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(snapshot) = self.overlay.read(cx).confirm_snapshot() else {
            return div().into_any_element();
        };
        if !matches!(
            snapshot.kind,
            WorkspaceOverlayConfirmKind::LegalNotice
                | WorkspaceOverlayConfirmKind::ThirdPartyNotices
        ) {
            return div().into_any_element();
        }
        let third_party = matches!(
            snapshot.kind,
            WorkspaceOverlayConfirmKind::ThirdPartyNotices
        );
        let owner = cx.entity().downgrade();
        SettingsMarkdownDialog {
            tokens: &self.tokens,
            i18n: &self.i18n,
            phase: snapshot.phase,
            focus: self.standard_confirm_focus(),
            options: self.localized_markdown_options(),
            actions: self.markdown_mermaid_actions(cx),
        }
        .legal_notice(
            third_party,
            &self.settings_views.legal_notice_scroll,
            move |clear_focus, cx| {
                let _ = owner.update(cx, |app, cx| {
                    if clear_focus {
                        app.clear_standard_confirm_focus();
                    }
                    app.close_help_legal_notice(cx);
                });
            },
        )
    }

    pub(in crate::workspace) fn open_help_url(
        &mut self,
        url: &'static str,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = open_external_url(url) {
            self.push_ai_settings_toast(error.to_string(), TerminalNoticeVariant::Error, cx);
            cx.notify();
        }
    }

    pub(in crate::workspace) fn open_help_log_directory(&mut self, cx: &mut Context<Self>) {
        let log_dir = self.help_log_directory();
        let opened = std::fs::create_dir_all(&log_dir)
            .and_then(|()| open_path_external(&log_dir))
            .map_err(|error| error.to_string());
        if let Err(error) = opened {
            self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
            cx.notify();
        }
    }

    pub(in crate::workspace) fn help_log_directory(&self) -> std::path::PathBuf {
        // Tauri stores logs under the app data directory. Native settings use
        // the same data root, so derive logs beside settings.json.
        self.settings_store
            .path()
            .parent()
            .map(|parent| parent.join("logs"))
            .unwrap_or_else(|| std::path::PathBuf::from("logs"))
    }

    pub(in crate::workspace) fn language_label(&self, language: Language) -> String {
        oxideterm_gpui_settings_view::select_menu::language_label(language)
    }
}
