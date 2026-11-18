use super::*;
use oxideterm_gpui_settings_view::update_view::{
    NativeUpdateNotice, NativeUpdateNotificationAction, release_notes_dialog,
};

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
                    matches!(
                        snapshot.kind,
                        WorkspaceOverlayConfirmKind::LegalNotice
                            | WorkspaceOverlayConfirmKind::ThirdPartyNotices
                    )
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

        let owner = cx.entity().downgrade();
        let dispatch = Rc::new(move |action, cx: &mut App| {
            let _ = owner.update(cx, |app, cx| match action {
                NativeUpdateNotificationAction::Dismiss => {
                    app.dismiss_native_update_notification(cx)
                }
                NativeUpdateNotificationAction::ReleaseNotes => {
                    app.open_native_update_release_notes(cx)
                }
                NativeUpdateNotificationAction::Download => app.download_native_update(cx),
                NativeUpdateNotificationAction::Cancel => app.cancel_native_update(cx),
                NativeUpdateNotificationAction::Install => app.install_native_update(cx),
                NativeUpdateNotificationAction::Retry => app.check_native_update(cx),
            });
        });
        let settings = self.settings_workspace.read(cx);
        NativeUpdateNotice {
            i18n: &self.i18n,
            tokens: &self.tokens,
            state: settings.native_update_render_state(),
            phase: self.native_update_notification_presence.phase(),
            description: settings.native_update_package_description(),
            has_release_notes: settings.native_update_has_release_notes(),
            portable: self.native_update_is_portable(cx),
            dispatch,
        }
        .render()
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

        let body = markdown_virtual_with_code_actions(
            "native-update-release-notes-markdown",
            &self.tokens,
            &release_body,
            &options,
            &self.native_update_release_notes_scroll,
            &code_actions,
        )
        .into_any_element();
        let weak = cx.entity().downgrade();
        release_notes_dialog(
            &self.tokens,
            &self.i18n,
            snapshot.phase,
            self.standard_confirm_focus(),
            description,
            body,
            move |_, _, cx| {
                let _ = weak.update(cx, |app, cx| {
                    app.clear_standard_confirm_focus();
                    app.close_native_update_release_notes(cx);
                });
                cx.stop_propagation();
            },
        )
    }
}
