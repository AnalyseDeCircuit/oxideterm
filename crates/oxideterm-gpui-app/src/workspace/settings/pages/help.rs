use super::*;

pub(in crate::workspace) const HELP_LEGAL_MARKDOWN: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../LEGAL.md"));
const HELP_THIRD_PARTY_MARKDOWN: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../THIRD_PARTY_NOTICES.md"
));

pub(in crate::workspace) const HELP_LEGAL_NOTICE_WIDTH: f32 = 760.0;
pub(in crate::workspace) const HELP_LEGAL_NOTICE_HEIGHT: f32 = 720.0;

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_help_section(
        &mut self,
        section: SettingsSectionId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use oxideterm_gpui_settings_view::help_page::{HelpPage, HelpRequest};

        let owner = cx.entity().downgrade();
        let dispatch = Rc::new(move |request, cx: &mut App| {
            let _ = owner.update(cx, |this, cx| match request {
                HelpRequest::Check => this.check_native_update(cx),
                HelpRequest::Download => this.download_native_update(cx),
                HelpRequest::Install => this.install_native_update(cx),
                HelpRequest::Cancel => this.cancel_native_update(cx),
                HelpRequest::ReleaseNotes => this.open_native_update_release_notes(cx),
                HelpRequest::Legal => this.open_help_legal_notice(cx),
                HelpRequest::ThirdParty => {
                    this.settings_legal_notice_scroll = MarkdownVirtualListScrollHandle::new();
                    this.overlay.update(cx, |overlay, cx| {
                        overlay.open_confirm(WorkspaceOverlayConfirmKind::ThirdPartyNotices, cx);
                    });
                }
                HelpRequest::Logs => this.open_help_log_directory(cx),
                HelpRequest::Url(url) => this.open_help_url(url, cx),
            });
        });
        let year = chrono::Local::now().format("%Y").to_string();
        self.with_settings_controls(cx, |controls, cx| {
            HelpPage {
                controls,
                state: &self.settings_workspace,
                year: &year,
                dispatch,
            }
            .render(section, cx)
        })
    }

    pub(in crate::workspace) fn open_help_legal_notice(&mut self, cx: &mut Context<Self>) {
        self.settings_legal_notice_scroll = MarkdownVirtualListScrollHandle::new();
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
        let mut options = self.localized_markdown_options();
        options.base_font_size = self.tokens.metrics.ui_text_sm;
        options.block_gap = 8.0;
        let code_actions = self.markdown_mermaid_actions(cx);

        let backdrop = dismissible_dialog_backdrop().on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                this.close_help_legal_notice(cx);
                cx.stop_propagation();
            }),
        );
        let form = overlay_content_boundary(
            dialog_content(&self.tokens)
                .flex()
                .flex_col()
                .w(px(HELP_LEGAL_NOTICE_WIDTH))
                .max_w(relative(0.92))
                .h(px(HELP_LEGAL_NOTICE_HEIGHT))
                .max_h(relative(0.90))
                .child(
                    dialog_header(&self.tokens)
                        .child(dialog_title(
                            &self.tokens,
                            self.i18n.t(if third_party {
                                "settings_view.help.third_party_notices"
                            } else {
                                "settings_view.help.disclaimer"
                            }),
                        ))
                        .child(dialog_description(
                            &self.tokens,
                            self.i18n.t(if third_party {
                                "settings_view.help.third_party_notices_description"
                            } else {
                                "settings_view.help.legal_notice_description"
                            }),
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .p(px(16.0))
                        .bg(rgb(self.tokens.ui.bg))
                        .text_color(rgb(self.tokens.ui.text))
                        .child(markdown_virtual_with_code_actions(
                            "settings-help-legal-notice-markdown",
                            &self.tokens,
                            if third_party {
                                HELP_THIRD_PARTY_MARKDOWN
                            } else {
                                HELP_LEGAL_MARKDOWN
                            },
                            &options,
                            &self.settings_legal_notice_scroll,
                            &code_actions,
                        )),
                )
                .child(
                    dialog_footer(&self.tokens).child(self.standard_footer_action_button(
                        self.i18n.t("settings_view.help.legal_notice_close"),
                        ButtonVariant::Secondary,
                        ConfirmDialogAction::Cancel,
                        false,
                        |this, _event, _window, cx| {
                            this.close_help_legal_notice(cx);
                        },
                        cx,
                    )),
                ),
        );
        settings_dialog_transition(
            &self.tokens,
            "help-legal-notice-form",
            backdrop,
            form,
            snapshot.phase,
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
