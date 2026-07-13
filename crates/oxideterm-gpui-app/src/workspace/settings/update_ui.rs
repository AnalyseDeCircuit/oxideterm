use super::*;

// Update state stays owned by the live workspace; this module only maps that
// state into the persistent prompt and the on-demand release-notes overlay.
const NATIVE_UPDATE_RELEASE_NOTES_WIDTH: f32 = 760.0;
const NATIVE_UPDATE_RELEASE_NOTES_HEIGHT: f32 = 720.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeUpdateNotificationAction {
    ReleaseNotes,
    Download,
    Cancel,
    Install,
    Retry,
}

impl WorkspaceApp {
    pub(in crate::workspace) fn show_native_update_notification(&mut self) {
        self.native_update_notification_presence.reopen();
        self.native_update_notification_open = true;
    }

    pub(in crate::workspace) fn dismiss_native_update_notification(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(generation) = self.native_update_notification_presence.begin_exit() else {
            return;
        };
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        if delay.is_zero() {
            self.native_update_notification_open = false;
            self.native_update_notification_presence.reopen();
            cx.notify();
            return;
        }

        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this
                    .native_update_notification_presence
                    .finish_exit(generation)
                {
                    this.native_update_notification_open = false;
                    this.native_update_notification_presence.reopen();
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn render_native_update_notification(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<ToastView> {
        if !self.native_update_notification_open
            || self.version_migration.open
            || self.onboarding.open
            || self.settings_page.legal_notice_open
            || self.native_update_release_notes_open
        {
            return None;
        }

        let (title, status_text, progress, variant) = match &self.native_update_state {
            NativeUpdateUiState::Available(_) => (
                self.i18n.t("settings_view.help.update_available"),
                None,
                None,
                ToastVariant::Default,
            ),
            NativeUpdateUiState::Downloading(status) => (
                self.i18n.t("settings_view.help.downloading"),
                status.as_ref().map(native_update_progress_hint),
                status
                    .as_ref()
                    .and_then(native_update_progress_ratio)
                    .map(|ratio| ratio * 100.0)
                    .or(Some(0.0)),
                ToastVariant::Default,
            ),
            NativeUpdateUiState::Verifying(status) => (
                self.i18n.t("settings_view.help.verifying"),
                status.as_ref().map(native_update_progress_hint),
                Some(100.0),
                ToastVariant::Default,
            ),
            NativeUpdateUiState::Downloaded(_) => (
                self.i18n.t("settings_view.help.update_downloaded"),
                None,
                None,
                ToastVariant::Success,
            ),
            NativeUpdateUiState::Installing(plan) => (
                self.i18n.t("settings_view.help.installing"),
                plan.as_ref().map(|plan| plan.summary.clone()),
                None,
                ToastVariant::Default,
            ),
            NativeUpdateUiState::InstallFinished(outcome) => {
                let (title_key, variant) = match outcome.status {
                    oxideterm_update::NativeInstallStatus::ManualActionRequired => (
                        "settings_view.help.update_downloaded",
                        ToastVariant::Warning,
                    ),
                    oxideterm_update::NativeInstallStatus::InstallerLaunched => (
                        "settings_view.help.installer_launched",
                        ToastVariant::Success,
                    ),
                    oxideterm_update::NativeInstallStatus::ReplacementScheduled => (
                        "settings_view.help.replacement_scheduled",
                        ToastVariant::Success,
                    ),
                };
                (
                    self.i18n.t(title_key),
                    Some(outcome.message.clone()),
                    None,
                    variant,
                )
            }
            NativeUpdateUiState::Error(error) => (
                self.i18n.t("settings_view.help.update_error"),
                (!error.is_empty()).then(|| error.clone()),
                None,
                ToastVariant::Error,
            ),
            NativeUpdateUiState::Idle
            | NativeUpdateUiState::Checking
            | NativeUpdateUiState::UpToDate => return None,
        };

        let description = self
            .native_update_package
            .as_ref()
            .map(|package| format!("v{} → v{}", package.current_version, package.version));
        let actions = self.render_native_update_notification_actions(cx);
        let workspace = cx.entity();

        Some(ToastView {
            id: "native-update".to_string(),
            phase: self.native_update_notification_presence.phase(),
            title,
            description,
            status_text,
            progress,
            variant,
            actions,
            close: Some(
                toast_close(&self.tokens)
                    .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                        let _ = workspace.update(cx, |this, cx| {
                            this.dismiss_native_update_notification(cx);
                        });
                        cx.stop_propagation();
                    })
                    .into_any_element(),
            ),
        })
    }

    fn render_native_update_notification_actions(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mut actions = Vec::new();
        let has_release_notes = self
            .native_update_package
            .as_ref()
            .and_then(|package| package.body.as_deref())
            .is_some_and(|body| !body.trim().is_empty());

        if has_release_notes {
            actions.push(self.native_update_notification_action(
                NativeUpdateNotificationAction::ReleaseNotes,
                self.i18n.t("settings_view.help.release_notes"),
                false,
                cx,
            ));
        }

        let primary_action = match &self.native_update_state {
            NativeUpdateUiState::Available(_) => Some((
                NativeUpdateNotificationAction::Download,
                "settings_view.help.download_update",
            )),
            NativeUpdateUiState::Downloading(_) | NativeUpdateUiState::Verifying(_) => Some((
                NativeUpdateNotificationAction::Cancel,
                "settings_view.help.cancel",
            )),
            NativeUpdateUiState::Downloaded(_) => Some((
                NativeUpdateNotificationAction::Install,
                "settings_view.help.install_update",
            )),
            NativeUpdateUiState::Error(_) => Some((
                NativeUpdateNotificationAction::Retry,
                "settings_view.help.retry",
            )),
            _ => None,
        };
        if let Some((action, label_key)) = primary_action {
            actions.push(self.native_update_notification_action(
                action,
                self.i18n.t(label_key),
                true,
                cx,
            ));
        }

        (!actions.is_empty()).then(|| {
            div()
                .flex()
                .flex_wrap()
                .gap(px(self.tokens.spacing.two))
                .children(actions)
                .into_any_element()
        })
    }

    fn native_update_notification_action(
        &self,
        action: NativeUpdateNotificationAction,
        label: String,
        primary: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        toast_action(&self.tokens, label)
            .cursor_pointer()
            .when(primary, |button| {
                button
                    .border_color(rgb(self.tokens.ui.accent))
                    .bg(rgb(self.tokens.ui.accent))
                    .text_color(rgb(self.tokens.ui.accent_text))
            })
            .when(!primary, |button| {
                button.hover(|button| button.bg(rgb(self.tokens.ui.bg_hover)))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    match action {
                        NativeUpdateNotificationAction::ReleaseNotes => {
                            this.open_native_update_release_notes(cx)
                        }
                        NativeUpdateNotificationAction::Download => this.download_native_update(cx),
                        NativeUpdateNotificationAction::Cancel => this.cancel_native_update(cx),
                        NativeUpdateNotificationAction::Install => this.install_native_update(cx),
                        NativeUpdateNotificationAction::Retry => this.check_native_update(cx),
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn open_native_update_release_notes(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let has_release_notes = self
            .native_update_package
            .as_ref()
            .and_then(|package| package.body.as_deref())
            .is_some_and(|body| !body.trim().is_empty());
        if !has_release_notes {
            return;
        }

        self.native_update_release_notes_scroll = MarkdownVirtualListScrollHandle::new();
        self.native_update_release_notes_presence.reopen();
        self.native_update_release_notes_open = true;
        cx.notify();
    }

    pub(in crate::workspace) fn close_native_update_release_notes(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(generation) = self.native_update_release_notes_presence.begin_exit() else {
            return;
        };
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        if delay.is_zero() {
            self.native_update_release_notes_open = false;
            self.native_update_release_notes_presence.reopen();
            cx.notify();
            return;
        }

        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this
                    .native_update_release_notes_presence
                    .finish_exit(generation)
                {
                    this.native_update_release_notes_open = false;
                    this.native_update_release_notes_presence.reopen();
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn handle_native_update_release_notes_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.native_update_release_notes_open
            || event.keystroke.key.as_str() != "escape"
            || event.keystroke.modifiers.platform
        {
            return false;
        }
        self.close_native_update_release_notes(cx);
        true
    }

    pub(in crate::workspace) fn render_native_update_release_notes_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let package = self.native_update_package.as_ref();
        let release_body = package
            .and_then(|package| package.body.as_deref())
            .filter(|body| !body.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| self.i18n.t("settings_view.help.no_changelog"));
        let description = package.map(|package| {
            package
                .date
                .as_ref()
                .map(|date| format!("v{} · {date}", package.version))
                .unwrap_or_else(|| format!("v{}", package.version))
        });

        let mut options = self.localized_markdown_options();
        options.base_font_size = self.tokens.metrics.ui_text_sm;
        options.block_gap = 8.0;
        let code_actions = self.markdown_mermaid_actions(cx);

        let backdrop = dismissible_dialog_backdrop().on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                this.close_native_update_release_notes(cx);
                cx.stop_propagation();
            }),
        );
        let mut header = dialog_header(&self.tokens).child(dialog_title(
            &self.tokens,
            self.i18n.t("settings_view.help.release_notes"),
        ));
        if let Some(description) = description {
            header = header.child(dialog_description(&self.tokens, description));
        }
        let form = overlay_content_boundary(
            dialog_content(&self.tokens)
                .flex()
                .flex_col()
                .w(px(NATIVE_UPDATE_RELEASE_NOTES_WIDTH))
                .max_w(relative(0.92))
                .h(px(NATIVE_UPDATE_RELEASE_NOTES_HEIGHT))
                .max_h(relative(0.90))
                .child(header)
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .p(px(16.0))
                        .bg(rgb(self.tokens.ui.bg))
                        .text_color(rgb(self.tokens.ui.text))
                        .child(markdown_virtual_with_code_actions(
                            cx.entity(),
                            "native-update-release-notes-markdown",
                            &self.tokens,
                            &release_body,
                            &options,
                            &self.native_update_release_notes_scroll,
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
                            this.close_native_update_release_notes(cx);
                        },
                        cx,
                    )),
                ),
        );
        settings_dialog_transition(
            &self.tokens,
            "native-update-release-notes-form",
            backdrop,
            form,
            self.native_update_release_notes_presence,
        )
    }
}
