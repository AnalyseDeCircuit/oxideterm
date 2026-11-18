use gpui::Context;
use oxideterm_connections::{
    ConnectionStore, LOCAL_SHELL_PRIVILEGE_CONNECTION_ID, SSH_COMMON_PRIVILEGE_SCOPE_ID,
};
use oxideterm_i18n::I18n;

use super::CredentialRequest;
use crate::entity::SettingsWorkspaceEntity;

#[derive(Default)]
pub struct CredentialOutcome {
    pub store_changed: bool,
    pub close_select: bool,
    pub clear_ime_selection: bool,
}

fn active_scope_id<'a>(store: &ConnectionStore, selected: Option<&'a str>) -> &'a str {
    selected
        .filter(|id| {
            *id == SSH_COMMON_PRIVILEGE_SCOPE_ID
                || *id == LOCAL_SHELL_PRIVILEGE_CONNECTION_ID
                || store
                    .connections()
                    .iter()
                    .any(|connection| connection.id == *id)
        })
        .unwrap_or(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID)
}

impl SettingsWorkspaceEntity {
    pub fn apply_credential_request(
        &mut self,
        request: CredentialRequest,
        store: &mut ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> CredentialOutcome {
        let mut outcome = CredentialOutcome::default();
        match request {
            CredentialRequest::SelectScope(id) => {
                if active_scope_id(store, self.privilege_layout_flags().0) != id {
                    self.set_privilege_scope(id, cx);
                    outcome.close_select = true;
                }
            }
            CredentialRequest::BeginPrivilege => {
                self.begin_new_privilege_credential(cx);
                outcome.close_select = true;
            }
            CredentialRequest::EditPrivilege(scope, id) => {
                if let Ok(credentials) = store.list_privilege_credentials(&scope)
                    && let Some(credential) = credentials
                        .into_iter()
                        .find(|credential| credential.id == id)
                {
                    self.edit_privilege_credential(scope, credential, cx);
                    outcome.close_select = true;
                }
            }
            CredentialRequest::DeletePrivilege(scope, id) => {
                outcome.store_changed =
                    self.delete_settings_privilege_credential(scope, id, store, cx);
                outcome.close_select = true;
            }
            CredentialRequest::CancelPrivilege => {
                self.reset_privilege_draft(cx);
                outcome.close_select = true;
            }
            CredentialRequest::SavePrivilege => {
                outcome.store_changed = self.save_settings_privilege_credential(store, i18n, cx);
                outcome.close_select = true;
            }
            CredentialRequest::EditTotp(id) => {
                let credential = id
                    .as_deref()
                    .and_then(|id| store.totp_credentials().iter().find(|item| item.id == id));
                if id.is_none() || credential.is_some() {
                    self.edit_totp_credential(credential, store, cx);
                    outcome.clear_ime_selection = true;
                }
            }
            CredentialRequest::DeleteTotp(id) => {
                outcome.store_changed = self.delete_totp_credential_from_settings(id, store, cx);
            }
            CredentialRequest::CancelTotp => {
                self.cancel_totp_draft(cx);
                outcome.clear_ime_selection = true;
            }
            CredentialRequest::SaveTotp => {
                if self.totp_draft.is_some() {
                    outcome.store_changed = self.save_totp_draft(store, cx);
                    outcome.clear_ime_selection = true;
                }
            }
        }
        outcome
    }

    fn save_settings_privilege_credential(
        &mut self,
        store: &mut ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> bool {
        let selected_scope = self.privilege_layout_flags().0.map(str::to_string);
        let scope_id = active_scope_id(store, selected_scope.as_deref()).to_owned();
        let snapshot = self.privilege_snapshot();
        if scope_id == SSH_COMMON_PRIVILEGE_SCOPE_ID
            && !snapshot.require_click_to_send
            && snapshot.host_patterns.trim().is_empty()
        {
            let error = i18n.t("sessionManager.privilege_credentials.auto_requires_host_patterns");
            self.finish_privilege_save(scope_id, Some(error), cx);
            return false;
        }
        if snapshot.response_mode.uses_totp() && snapshot.totp_credential_id.is_none() {
            let error = i18n.t("sessionManager.privilege_credentials.two_stage_requires_totp");
            self.finish_privilege_save(scope_id, Some(error), cx);
            return false;
        }
        let Some(request) = self.take_privilege_save_request(scope_id.clone()) else {
            return false;
        };
        match store.save_privilege_credential(request) {
            Ok(_) => {
                self.finish_privilege_save(scope_id, None, cx);
                true
            }
            Err(error) => {
                self.finish_privilege_save(scope_id, Some(error.to_string()), cx);
                false
            }
        }
    }

    fn delete_settings_privilege_credential(
        &mut self,
        scope_id: String,
        credential_id: String,
        store: &mut ConnectionStore,
        cx: &mut Context<Self>,
    ) -> bool {
        match store.delete_privilege_credential(&scope_id, &credential_id) {
            Ok(_) => {
                self.finish_privilege_delete(scope_id, &credential_id, None, cx);
                true
            }
            Err(error) => {
                self.finish_privilege_delete(scope_id, &credential_id, Some(error.to_string()), cx);
                false
            }
        }
    }
}
