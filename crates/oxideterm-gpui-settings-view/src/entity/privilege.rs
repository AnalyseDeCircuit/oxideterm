use super::*;
use oxideterm_connections::{
    SavePrivilegeCredentialRequest, SavedPrivilegeCredential, SecretString,
};

impl SettingsWorkspaceEntity {
    pub fn privilege_prompt_type(&self) -> PrivilegePromptType {
        self.privilege_draft.prompt_type
    }

    pub fn privilege_response_mode(&self) -> PrivilegeResponseMode {
        self.privilege_draft.response_mode
    }

    pub fn privilege_snapshot(&self) -> PrivilegeCredentialSnapshot {
        PrivilegeCredentialSnapshot {
            credential_id: self.privilege_draft.credential_id.clone(),
            label: self.privilege_draft.label.clone(),
            prompt_type: self.privilege_draft.prompt_type,
            response_mode: self.privilege_draft.response_mode,
            username_hint: self.privilege_draft.username_hint.clone(),
            prompt_patterns: self.privilege_draft.prompt_patterns.clone(),
            host_patterns: self.privilege_draft.host_patterns.clone(),
            second_factor_patterns: self.privilege_draft.second_factor_patterns.clone(),
            second_factor_user_hint: self.privilege_draft.second_factor_user_hint.clone(),
            totp_credential_id: self.privilege_draft.totp_credential_id.clone(),
            enabled: self.privilege_draft.enabled,
            require_click_to_send: self.privilege_draft.require_click_to_send,
            error: self.privilege_error.clone(),
        }
    }

    pub fn privilege_layout_flags(&self) -> (Option<&str>, bool, bool, bool) {
        (
            self.privilege_scope_id.as_deref(),
            self.privilege_draft.credential_id.is_some(),
            self.privilege_editor_open,
            self.privilege_error.is_some(),
        )
    }

    pub fn set_privilege_scope(&mut self, scope_id: String, cx: &mut Context<Self>) {
        self.reset_privilege_draft_state();
        self.privilege_scope_id = Some(scope_id);
        cx.notify();
    }

    pub fn reset_privilege_draft(&mut self, cx: &mut Context<Self>) {
        self.reset_privilege_draft_state();
        cx.notify();
    }

    pub fn begin_new_privilege_credential(&mut self, cx: &mut Context<Self>) {
        self.reset_privilege_draft_state();
        self.privilege_editor_open = true;
        self.settings_focused_input = Some(SettingsInput::LocalPrivilegeLabel);
        cx.notify();
    }

    pub fn edit_privilege_credential(
        &mut self,
        scope_id: String,
        credential: SavedPrivilegeCredential,
        cx: &mut Context<Self>,
    ) {
        self.reset_privilege_draft_state();
        self.privilege_scope_id = Some(scope_id);
        self.privilege_draft.credential_id = Some(credential.id);
        self.privilege_draft.label = credential.label;
        self.privilege_draft.prompt_type = credential.prompt_type;
        self.privilege_draft.response_mode = credential.response_mode;
        self.privilege_draft.username_hint = credential.username_hint.unwrap_or_default();
        self.privilege_draft.prompt_patterns = credential.prompt_patterns.join("\n");
        self.privilege_draft.host_patterns = if credential.host_patterns.is_empty() {
            "*".to_string()
        } else {
            credential.host_patterns.join("\n")
        };
        self.privilege_draft.second_factor_patterns = credential.second_factor_patterns.join("\n");
        self.privilege_draft.second_factor_user_hint =
            credential.second_factor_user_hint.unwrap_or_default();
        self.privilege_draft.totp_credential_id = credential.totp_credential_id;
        self.privilege_draft.enabled = credential.enabled;
        self.privilege_draft.require_click_to_send = credential.require_click_to_send;
        self.privilege_editor_open = true;
        self.settings_focused_input = Some(SettingsInput::LocalPrivilegeLabel);
        cx.notify();
    }

    pub fn set_privilege_prompt_type(
        &mut self,
        prompt_type: PrivilegePromptType,
        cx: &mut Context<Self>,
    ) {
        self.privilege_draft.prompt_type = prompt_type;
        self.privilege_error = None;
        cx.notify();
    }

    pub fn set_privilege_response_mode(
        &mut self,
        response_mode: PrivilegeResponseMode,
        cx: &mut Context<Self>,
    ) {
        self.privilege_draft.response_mode = response_mode;
        if !response_mode.uses_totp() {
            self.privilege_draft.totp_credential_id = None;
        }
        if !response_mode.waits_for_totp() {
            self.privilege_draft.second_factor_patterns.clear();
            self.privilege_draft.second_factor_user_hint.clear();
        }
        self.privilege_error = None;
        cx.notify();
    }

    pub fn toggle_privilege_enabled(&mut self, cx: &mut Context<Self>) {
        self.privilege_draft.enabled = !self.privilege_draft.enabled;
        self.privilege_error = None;
        cx.notify();
    }

    pub fn toggle_privilege_auto_submit(&mut self, cx: &mut Context<Self>) {
        self.privilege_draft.require_click_to_send = !self.privilege_draft.require_click_to_send;
        self.privilege_error = None;
        cx.notify();
    }

    pub fn set_privilege_totp_credential(
        &mut self,
        credential_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.privilege_draft.totp_credential_id = credential_id;
        self.privilege_error = None;
        cx.notify();
    }

    pub fn take_privilege_save_request(
        &mut self,
        connection_id: String,
    ) -> Option<SavePrivilegeCredentialRequest> {
        let label = self.privilege_draft.label.trim().to_string();
        if label.is_empty() {
            return None;
        }
        let prompt_patterns = self
            .privilege_draft
            .prompt_patterns
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let host_patterns = self
            .privilege_draft
            .host_patterns
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let second_factor_patterns = self
            .privilege_draft
            .second_factor_patterns
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let secret = if self.privilege_draft.secret.is_empty() {
            None
        } else {
            // Move the only UI-owned secret into the store request so retries
            // cannot retain a second credential copy in the Entity.
            let secret = std::mem::replace(
                &mut self.privilege_draft.secret,
                zeroize::Zeroizing::new(String::new()),
            );
            Some(SecretString::from(secret))
        };
        self.settings_focused_input = None;
        Some(SavePrivilegeCredentialRequest {
            connection_id,
            credential_id: self.privilege_draft.credential_id.clone(),
            label,
            prompt_type: self.privilege_draft.prompt_type,
            response_mode: self.privilege_draft.response_mode,
            username_hint: optional_trimmed_privilege_value(&self.privilege_draft.username_hint),
            prompt_patterns,
            host_patterns,
            second_factor_patterns,
            second_factor_user_hint: optional_trimmed_privilege_value(
                &self.privilege_draft.second_factor_user_hint,
            ),
            totp_credential_id: self.privilege_draft.totp_credential_id.clone(),
            secret,
            enabled: self.privilege_draft.enabled,
            require_click_to_send: self.privilege_draft.require_click_to_send,
        })
    }

    pub fn finish_privilege_save(
        &mut self,
        scope_id: String,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(error) = error {
            self.privilege_error = Some(error);
        } else {
            self.reset_privilege_draft_state();
            self.privilege_scope_id = Some(scope_id);
        }
        cx.notify();
    }

    pub fn finish_privilege_delete(
        &mut self,
        scope_id: String,
        credential_id: &str,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(error) = error {
            self.privilege_error = Some(error);
        } else {
            if self.privilege_draft.credential_id.as_deref() == Some(credential_id) {
                self.reset_privilege_draft_state();
            }
            self.privilege_scope_id = Some(scope_id);
            self.privilege_error = None;
        }
        cx.notify();
    }

    fn reset_privilege_draft_state(&mut self) {
        self.privilege_draft = PrivilegeCredentialDraft::default();
        self.privilege_error = None;
        self.privilege_editor_open = false;
        if self.settings_focused_input.is_some_and(is_privilege_input) {
            self.settings_focused_input = None;
        }
    }
}

fn is_privilege_input(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::LocalPrivilegeLabel
            | SettingsInput::LocalPrivilegeUsernameHint
            | SettingsInput::LocalPrivilegeSecret
            | SettingsInput::LocalPrivilegePromptPatterns
            | SettingsInput::LocalPrivilegeHostPatterns
            | SettingsInput::LocalPrivilegeSecondFactorPatterns
            | SettingsInput::LocalPrivilegeSecondFactorUserHint
    )
}

fn optional_trimmed_privilege_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext, TestAppContext};

    use super::*;
    use oxideterm_connections::{
        LOCAL_SHELL_PRIVILEGE_CONNECTION_ID, SSH_COMMON_PRIVILEGE_SCOPE_ID,
    };

    #[gpui::test]
    fn privilege_save_request_includes_host_group_and_second_factor(cx: &mut TestAppContext) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        settings.update(cx, |settings, cx| {
            settings.begin_new_privilege_credential(cx);
            settings.privilege_draft.label = "group sudo".to_string();
            settings.privilege_draft.prompt_type = PrivilegePromptType::Sudo;
            settings.privilege_draft.response_mode = PrivilegeResponseMode::PasswordThenTotp;
            settings.privilege_draft.host_patterns = " hktkosl* \n\ni-* ".to_string();
            settings.privilege_draft.second_factor_patterns =
                " Second Factor: \nSecond factor： ".to_string();
            settings.privilege_draft.second_factor_user_hint = " deploy ".to_string();
            settings.privilege_draft.totp_credential_id = Some("totp-1".to_string());
            settings.privilege_draft.require_click_to_send = false;

            let request = settings
                .take_privilege_save_request(SSH_COMMON_PRIVILEGE_SCOPE_ID.to_string())
                .expect("privilege save request");
            assert_eq!(
                request.host_patterns,
                vec!["hktkosl*".to_string(), "i-*".to_string()]
            );
            assert_eq!(
                request.second_factor_patterns,
                vec!["Second Factor:".to_string(), "Second factor：".to_string()]
            );
            assert_eq!(request.prompt_type, PrivilegePromptType::Sudo);
            assert_eq!(
                request.response_mode,
                PrivilegeResponseMode::PasswordThenTotp
            );
            assert_eq!(request.second_factor_user_hint.as_deref(), Some("deploy"));
            assert_eq!(request.totp_credential_id.as_deref(), Some("totp-1"));
            assert!(!request.require_click_to_send);
        });
    }

    #[gpui::test]
    fn privilege_save_request_moves_secret_and_keeps_nonsecret_retry_state(
        cx: &mut TestAppContext,
    ) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        settings.update(cx, |settings, cx| {
            settings.begin_new_privilege_credential(cx);
            assert!(settings.replace_settings_entity_input(
                SettingsInput::LocalPrivilegeLabel,
                None,
                "Local sudo",
                cx,
            ));
            assert!(settings.focus_settings_entity_input(SettingsInput::LocalPrivilegeSecret, cx));
            assert!(settings.replace_settings_entity_input(
                SettingsInput::LocalPrivilegeSecret,
                None,
                "privilege-secret",
                cx,
            ));

            let request = settings
                .take_privilege_save_request(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string())
                .expect("privilege save request");
            assert_eq!(
                request.secret.as_ref().map(SecretString::expose_secret),
                Some("privilege-secret")
            );
            assert!(settings.privilege_draft.secret.is_empty());
            assert_eq!(settings.privilege_draft.label, "Local sudo");

            settings.finish_privilege_save(
                LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string(),
                Some("retry".to_string()),
                cx,
            );
            assert_eq!(settings.privilege_error.as_deref(), Some("retry"));
            assert_eq!(settings.privilege_draft.label, "Local sudo");

            settings.finish_privilege_save(
                LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string(),
                None,
                cx,
            );
            assert!(settings.privilege_draft.label.is_empty());
            assert!(settings.privilege_draft.secret.is_empty());
            assert!(!settings.privilege_editor_open);
        });
    }
}
