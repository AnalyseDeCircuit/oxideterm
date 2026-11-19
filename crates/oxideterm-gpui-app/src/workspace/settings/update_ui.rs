use super::*;
use oxideterm_gpui_settings_view::native_update::NativeUpdateAction;
use oxideterm_gpui_settings_view::update_view::{
    NativeUpdateNoticeBindings, NativeUpdateNoticeFrame, NativeUpdateNotificationAction,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn native_update_notice_bindings(
        cx: &Context<Self>,
    ) -> NativeUpdateNoticeBindings {
        let owner = cx.entity().downgrade();
        NativeUpdateNoticeBindings {
            frame: Rc::new({
                let owner = owner.clone();
                move |cx| {
                    let owner = owner.upgrade()?;
                    let app = owner.read(cx);
                    if app.version_migration.open
                        || app.onboarding.open
                        || app
                            .overlay
                            .read(cx)
                            .confirm_snapshot()
                            .is_some_and(|snapshot| {
                                matches!(
                                    snapshot.kind,
                                    WorkspaceOverlayConfirmKind::LegalNotice
                                        | WorkspaceOverlayConfirmKind::ThirdPartyNotices
                                        | WorkspaceOverlayConfirmKind::NativeUpdateReleaseNotes
                                )
                            })
                    {
                        return None;
                    }
                    Some(NativeUpdateNoticeFrame {
                        tokens: app.tokens,
                        i18n: app.i18n.clone(),
                        portable: app.native_update_is_portable(cx),
                    })
                }
            }),
            dispatch: Rc::new(move |action, cx| {
                let _ = owner.update(cx, |app, cx| match action {
                    NativeUpdateNotificationAction::Dismiss => {}
                    NativeUpdateNotificationAction::ReleaseNotes => {
                        app.open_native_update_release_notes(cx)
                    }
                    NativeUpdateNotificationAction::Download => {
                        app.apply_native_update_action(NativeUpdateAction::Download, cx)
                    }
                    NativeUpdateNotificationAction::Cancel => {
                        app.apply_native_update_action(NativeUpdateAction::Cancel, cx)
                    }
                    NativeUpdateNotificationAction::Install => {
                        app.apply_native_update_action(NativeUpdateAction::Install, cx)
                    }
                    NativeUpdateNotificationAction::Retry => app.check_native_update(cx),
                });
            }),
        }
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

        self.settings_views.release_notes_scroll = MarkdownVirtualListScrollHandle::new();
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
        let owner = cx.entity().downgrade();
        let frame = oxideterm_gpui_settings_view::help_page::SettingsMarkdownDialog {
            tokens: &self.tokens,
            i18n: &self.i18n,
            phase: snapshot.phase,
            focus: self.standard_confirm_focus(),
            options: self.localized_markdown_options(),
            actions: self.markdown_mermaid_actions(cx),
        };
        frame.release_notes(
            self.settings_workspace.read(cx),
            &self.settings_views.release_notes_scroll,
            move |cx| {
                let _ = owner.update(cx, |app, cx| {
                    app.clear_standard_confirm_focus();
                    app.close_native_update_release_notes(cx);
                });
            },
        )
    }
}
