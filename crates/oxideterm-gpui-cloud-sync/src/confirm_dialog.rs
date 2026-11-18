// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{
    AnyElement, App, Context, Empty, Entity, IntoElement, KeyDownEvent, ParentElement, Render,
    Subscription, Window, div,
};
use oxideterm_gpui_ui::{
    ConfirmDialogView,
    confirm::{
        ConfirmDialogAction, ModalFooterKeyAction, confirm_dialog_with_focus_motion,
        modal_footer_key_action,
    },
    motion::ExitPhase,
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

use crate::{
    CloudSyncConfirmDescription, cloud_sync_confirm_copy_spec, entity::CloudSyncWorkspaceEntity,
    panel::SelectableTextRole,
};

/// The host supplies presentation preferences and its existing text-selection service.
pub struct CloudSyncConfirmFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub text: Box<dyn Fn(SelectableTextRole, &'static str, String, u32) -> AnyElement>,
}

pub struct CloudSyncConfirmPortal {
    sync: Entity<CloudSyncWorkspaceEntity>,
    frame: Box<dyn Fn(&App) -> Option<CloudSyncConfirmFrame>>,
    dispatch: Box<dyn Fn(ConfirmDialogAction, &mut App)>,
    phase: Option<ExitPhase>,
    _sync_changed: Subscription,
}

impl CloudSyncConfirmPortal {
    pub fn new(
        sync: &Entity<CloudSyncWorkspaceEntity>,
        frame: impl Fn(&App) -> Option<CloudSyncConfirmFrame> + 'static,
        dispatch: impl Fn(ConfirmDialogAction, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> Self {
        fn phase(sync: &CloudSyncWorkspaceEntity) -> Option<ExitPhase> {
            sync.view
                .confirm
                .as_ref()
                .map(|_| sync.view.confirm_presence.phase())
        }
        let sync_changed = cx.observe(sync, |portal, sync, cx| {
            let next = phase(sync.read(cx));
            if portal.phase.is_some() || next.is_some() {
                portal.phase = next;
                cx.notify();
            }
        });
        Self {
            sync: sync.clone(),
            frame: Box::new(frame),
            dispatch: Box::new(dispatch),
            phase: phase(sync.read(cx)),
            _sync_changed: sync_changed,
        }
    }

    pub fn phase(&self) -> Option<ExitPhase> {
        self.phase
    }

    /// Move footer focus locally and return any activation for window dispatch.
    pub fn handle_key(
        &self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> Option<ModalFooterKeyAction<ConfirmDialogAction>> {
        let modifiers = event.keystroke.modifiers;
        let view = &self.sync.read(cx).view;
        if view.confirm.is_none()
            || view.confirm_presence.phase() != ExitPhase::Visible
            || modifiers.platform
            || modifiers.control
        {
            return None;
        }
        let action = modal_footer_key_action(
            &event.keystroke.key,
            modifiers.shift,
            &[ConfirmDialogAction::Cancel, ConfirmDialogAction::Confirm],
            view.confirm_focused_action,
            ConfirmDialogAction::Cancel,
        )?;
        if let ModalFooterKeyAction::Focus(focused) = action {
            self.sync.update(cx, |sync, cx| {
                sync.view.confirm_focused_action = Some(focused);
                cx.notify();
            });
        }
        Some(action)
    }
}

impl Render for CloudSyncConfirmPortal {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Do not register a render dependency on background sync while closed.
        if self.phase.is_none() {
            return Empty.into_any_element();
        }
        let (confirm, confirm_phase, focused_action) = {
            let cloud_sync = self.sync.read(cx);
            (
                cloud_sync.view.confirm.clone(),
                cloud_sync.view.confirm_presence.phase(),
                cloud_sync.view.confirm_focused_action,
            )
        };
        let Some((confirm, frame)) = confirm.zip((self.frame)(cx)) else {
            return Empty.into_any_element();
        };
        let copy = cloud_sync_confirm_copy_spec(&confirm);
        let description = match copy.description {
            CloudSyncConfirmDescription::ChangePassword => Some(
                frame
                    .i18n
                    .t("plugin.cloud_sync.causal.change_password_description"),
            ),
            CloudSyncConfirmDescription::None => None,
            CloudSyncConfirmDescription::ForceUpload => Some(
                frame
                    .i18n
                    .t("plugin.cloud_sync.confirm.force_upload_description"),
            ),
            CloudSyncConfirmDescription::ClearSecret { label } => Some(
                frame
                    .i18n
                    .t("plugin.cloud_sync.confirm.clear_secret_description")
                    .replace("{{label}}", &label),
            ),
            CloudSyncConfirmDescription::RestoreBackup { created_at } => Some(
                frame
                    .i18n
                    .t("plugin.cloud_sync.confirm.restore_backup_description")
                    .replace("{{createdAt}}", &created_at),
            ),
            CloudSyncConfirmDescription::DeleteBackup { created_at } => Some(
                frame
                    .i18n
                    .t("plugin.cloud_sync.confirm.delete_backup_description")
                    .replace("{{createdAt}}", &created_at),
            ),
            CloudSyncConfirmDescription::ClearBackups => Some(
                frame
                    .i18n
                    .t("plugin.cloud_sync.confirm.clear_backups_description"),
            ),
            CloudSyncConfirmDescription::ClearHistory => Some(
                frame
                    .i18n
                    .t("plugin.cloud_sync.confirm.clear_history_description"),
            ),
            CloudSyncConfirmDescription::EnableSensitiveSync => Some(
                frame
                    .i18n
                    .t("plugin.cloud_sync.confirm.enable_sensitive_sync_description"),
            ),
        };
        confirm_dialog_with_focus_motion(
            &frame.tokens,
            "cloud-sync-confirm-motion",
            confirm_phase,
            ConfirmDialogView {
                variant: copy.variant,
                title: div()
                    .child((frame.text)(
                        SelectableTextRole::NonSelectable,
                        "title",
                        frame.i18n.t(copy.title_key),
                        frame.tokens.ui.text_heading,
                    ))
                    .into_any_element(),
                description: description.map(|text| {
                    div()
                        .child((frame.text)(
                            SelectableTextRole::PlainDocument,
                            "description",
                            text,
                            frame.tokens.ui.text_muted,
                        ))
                        .into_any_element()
                }),
                cancel_label: div()
                    .child((frame.text)(
                        SelectableTextRole::NonSelectable,
                        "cancel",
                        frame.i18n.t("common.cancel"),
                        frame.tokens.ui.text,
                    ))
                    .into_any_element(),
                confirm_label: div()
                    .child((frame.text)(
                        SelectableTextRole::NonSelectable,
                        "confirm",
                        frame.i18n.t(copy.confirm_label_key),
                        frame.tokens.ui.text,
                    ))
                    .into_any_element(),
            },
            focused_action,
            cx.listener(|this, _, _, cx| {
                (this.dispatch)(ConfirmDialogAction::Cancel, cx);
                cx.stop_propagation();
            }),
            cx.listener(|this, _, _, cx| {
                (this.dispatch)(ConfirmDialogAction::Confirm, cx);
                cx.stop_propagation();
            }),
        )
    }
}
