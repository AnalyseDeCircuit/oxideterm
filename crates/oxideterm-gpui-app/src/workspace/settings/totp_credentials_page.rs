use super::*;
use oxideterm_connections::totp::{TotpCredential, TotpError, TotpParameters};

use oxideterm_gpui_settings_view::entity::TotpDraft;

impl WorkspaceApp {
    pub(super) fn edit_totp_credential(
        &mut self,
        credential: Option<TotpCredential>,
        cx: &mut Context<Self>,
    ) {
        let mut draft = TotpDraft::new(credential.as_ref());
        if let Some(credential) = credential.as_ref() {
            draft.connection_ids = self
                .connection_store
                .connections()
                .iter()
                .filter(|connection| {
                    connection.options.totp_credential_id.as_deref() == Some(&credential.id)
                })
                .map(|connection| connection.id.clone())
                .collect();
        }
        self.settings_workspace.update(cx, |settings, cx| {
            settings.totp_draft = Some(draft);
            settings.totp_error = None;
            settings.settings_focused_input = None;
            cx.notify();
        });
        self.clear_ime_selection();
    }

    pub(super) fn save_totp_draft(&mut self, cx: &mut Context<Self>) {
        let Some(mut draft) = self
            .settings_workspace
            .update(cx, |settings, _| settings.totp_draft.take())
        else {
            return;
        };
        let secret = SecretString::from(std::mem::take(&mut *draft.secret));
        let result = draft
            .period
            .parse()
            .map_err(|_| TotpError::InvalidParameters)
            .and_then(|period| {
                self.connection_store.save_totp_credential_with_connections(
                    draft.id.as_deref(),
                    &draft.name,
                    (!secret.is_empty()).then_some(&secret),
                    TotpParameters {
                        algorithm: draft.algorithm,
                        digits: draft.digits,
                        period,
                    },
                    &draft.pattern,
                    draft.enabled,
                    Some(&draft.connection_ids),
                )
            });
        if result.is_ok() {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        self.settings_workspace.update(cx, |settings, cx| {
            settings.settings_focused_input = None;
            settings.totp_error = result.err();
            if settings.totp_error.is_some() {
                draft.secret = secret.into_zeroizing();
                settings.totp_draft = Some(draft);
            }
            cx.notify();
        });
        self.clear_ime_selection();
    }

    pub(super) fn delete_totp_credential_from_settings(
        &mut self,
        id: String,
        cx: &mut Context<Self>,
    ) {
        let result = self.connection_store.delete_totp_credential(&id);
        if result.is_ok() {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        self.settings_workspace.update(cx, |settings, cx| {
            settings.totp_error = result.err();
            cx.notify();
        });
    }

    pub(super) fn cancel_totp_draft(&mut self, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.totp_draft = None;
            settings.totp_error = None;
            settings.settings_focused_input = None;
            cx.notify();
        });
        self.clear_ime_selection();
    }
}
