use super::*;

// The settings Entity owns updater state; the workspace only maps its narrow
// render projection into window-scoped notification and release-note overlays.
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
            || self
                .overlay
                .read(cx)
                .confirm_snapshot()
                .is_some_and(|snapshot| {
                    matches!(snapshot.kind, WorkspaceOverlayConfirmKind::LegalNotice)
                })
            || self
                .overlay
                .read(cx)
                .confirm_snapshot()
                .is_some_and(|snapshot| {
                    matches!(
                        snapshot.kind,
                        WorkspaceOverlayConfirmKind::NativeUpdateReleaseNotes
                    )
                })
        {
            return None;
        }

        let update_state = self
            .settings_workspace
            .read(cx)
            .native_update_render_state();
        let (title, status_text, progress, variant) = match &update_state {
            NativeUpdateRenderState::Available { .. } => (
                self.i18n.t("settings_view.help.update_available"),
                None,
                None,
                ToastVariant::Default,
            ),
            NativeUpdateRenderState::Downloading(status) => (
                self.i18n.t("settings_view.help.downloading"),
                status.as_ref().map(native_update_progress_hint),
                status
                    .as_ref()
                    .and_then(native_update_progress_ratio)
                    .map(|ratio| ratio * 100.0)
                    .or(Some(0.0)),
                ToastVariant::Default,
            ),
            NativeUpdateRenderState::Verifying(status) => (
                self.i18n.t("settings_view.help.verifying"),
                status.as_ref().map(native_update_progress_hint),
                Some(100.0),
                ToastVariant::Default,
            ),
            NativeUpdateRenderState::Downloaded => (
                self.i18n.t("settings_view.help.update_downloaded"),
                None,
                None,
                ToastVariant::Success,
            ),
            NativeUpdateRenderState::Installing(summary) => (
                self.i18n.t("settings_view.help.installing"),
                summary.clone(),
                None,
                ToastVariant::Default,
            ),
            NativeUpdateRenderState::InstallFinished { status, message } => {
                let (title_key, variant) = match status {
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
                let status_text = if self.native_update_is_portable(cx)
                    && *status == oxideterm_update::NativeInstallStatus::ReplacementScheduled
                {
                    None
                } else {
                    Some(message.clone())
                };
                (self.i18n.t(title_key), status_text, None, variant)
            }
            NativeUpdateRenderState::Error(error) => (
                self.i18n.t("settings_view.help.update_error"),
                (!error.is_empty()).then(|| error.clone()),
                None,
                ToastVariant::Error,
            ),
            NativeUpdateRenderState::Idle
            | NativeUpdateRenderState::Checking
            | NativeUpdateRenderState::UpToDate => return None,
        };

        let description = self
            .settings_workspace
            .read(cx)
            .native_update_package_description();
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
            .settings_workspace
            .read(cx)
            .native_update_has_release_notes();

        if has_release_notes {
            actions.push(self.native_update_notification_action(
                NativeUpdateNotificationAction::ReleaseNotes,
                self.i18n.t("settings_view.help.release_notes"),
                false,
                cx,
            ));
        }

        let update_state = self
            .settings_workspace
            .read(cx)
            .native_update_render_state();
        let primary_action = match update_state {
            NativeUpdateRenderState::Available { .. } => Some((
                NativeUpdateNotificationAction::Download,
                "settings_view.help.download_update",
            )),
            NativeUpdateRenderState::Downloading(_) | NativeUpdateRenderState::Verifying(_) => {
                Some((
                    NativeUpdateNotificationAction::Cancel,
                    "settings_view.help.cancel",
                ))
            }
            NativeUpdateRenderState::Downloaded => Some((
                NativeUpdateNotificationAction::Install,
                "settings_view.help.install_update",
            )),
            NativeUpdateRenderState::Error(_) => Some((
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
            .settings_workspace
            .read(cx)
            .native_update_has_release_notes();
        if !has_release_notes {
            return;
        }

        self.native_update_release_notes_scroll = MarkdownVirtualListScrollHandle::new();
        self.overlay.update(cx, |overlay, cx| {
            overlay.open_confirm(WorkspaceOverlayConfirmKind::NativeUpdateReleaseNotes, cx);
        });
    }

    pub(in crate::workspace) fn close_native_update_release_notes(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.overlay.update(cx, |overlay, cx| {
            overlay.begin_confirm_exit(false, delay, cx);
        });
    }

    pub(in crate::workspace) fn handle_native_update_release_notes_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(snapshot) = self.overlay.read(cx).confirm_snapshot() else {
            return false;
        };
        if !matches!(
            snapshot.kind,
            WorkspaceOverlayConfirmKind::NativeUpdateReleaseNotes
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
                self.close_native_update_release_notes(cx);
                true
            }
            Some(WorkspaceOverlayConfirmKeyAction::Handled) => true,
            None => false,
        }
    }

    pub(in crate::workspace) fn render_native_update_release_notes_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(snapshot) = self.overlay.read(cx).confirm_snapshot() else {
            return div().into_any_element();
        };
        if !matches!(
            snapshot.kind,
            WorkspaceOverlayConfirmKind::NativeUpdateReleaseNotes
        ) {
            return div().into_any_element();
        }
        let release_notes = self
            .settings_workspace
            .read(cx)
            .native_update_release_notes();
        let (release_body, description) = release_notes
            .map(|release_notes| (release_notes.body, release_notes.description))
            .unwrap_or_else(|| (self.i18n.t("settings_view.help.no_changelog"), None));

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
            snapshot.phase,
        )
    }
}
