// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync confirm dialog event bridge.

use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_cloud_sync_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.cloud_sync_confirm.is_none()
            || event.keystroke.modifiers.platform
            || event.keystroke.modifiers.control
        {
            return false;
        }

        match browser_behavior::modal_footer_key_action(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            &CONFIRM_DIALOG_FOOTER_ACTIONS,
            self.cloud_sync_confirm_focused_action,
            ConfirmDialogAction::Cancel,
        ) {
            Some(browser_behavior::ModalFooterKeyAction::Cancel) => {
                self.cancel_cloud_sync_confirm();
                cx.notify();
                true
            }
            Some(browser_behavior::ModalFooterKeyAction::Focus(action)) => {
                self.cloud_sync_confirm_focused_action = Some(action);
                cx.notify();
                true
            }
            Some(browser_behavior::ModalFooterKeyAction::Activate(action)) => {
                match action {
                    ConfirmDialogAction::Cancel => self.cancel_cloud_sync_confirm(),
                    ConfirmDialogAction::Confirm => self.confirm_cloud_sync_confirm(cx),
                }
                cx.notify();
                true
            }
            None => false,
        }
    }

    pub(in crate::workspace) fn render_cloud_sync_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(confirm) = self.cloud_sync_confirm.clone() else {
            return div().into_any_element();
        };
        let copy = cloud_sync_confirm_copy_spec(&confirm);
        let description = match copy.description {
            CloudSyncConfirmDescription::None => None,
            CloudSyncConfirmDescription::ClearSecret { label } => Some(self.i18n_replace(
                "plugin.cloud_sync.confirm.clear_secret_description",
                &[("label", label)],
            )),
            CloudSyncConfirmDescription::RestoreBackup { created_at } => Some(self.i18n_replace(
                "plugin.cloud_sync.confirm.restore_backup_description",
                &[("createdAt", created_at)],
            )),
            CloudSyncConfirmDescription::EnableSensitiveSync => Some(
                self.i18n
                    .t("plugin.cloud_sync.confirm.enable_sensitive_sync_description"),
            ),
        };
        confirm_dialog_with_focus(
            &self.tokens,
            ConfirmDialogView {
                variant: copy.variant,
                title: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "cloud-sync-confirm",
                        "title",
                        self.i18n.t(copy.title_key),
                        self.tokens.ui.text_heading,
                        cx,
                    ))
                    .into_any_element(),
                description: description.map(|text| {
                    div()
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "cloud-sync-confirm",
                            "description",
                            text,
                            self.tokens.ui.text_muted,
                            cx,
                        ))
                        .into_any_element()
                }),
                cancel_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "cloud-sync-confirm",
                        "cancel",
                        self.i18n.t("common.cancel"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "cloud-sync-confirm",
                        "confirm",
                        self.i18n.t(copy.confirm_label_key),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
            },
            self.cloud_sync_confirm_focused_action,
            cx.listener(
                |this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                    this.cancel_cloud_sync_confirm();
                    cx.stop_propagation();
                    cx.notify();
                },
            ),
            cx.listener(
                |this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                    this.confirm_cloud_sync_confirm(cx);
                    cx.stop_propagation();
                    cx.notify();
                },
            ),
        )
    }
}
