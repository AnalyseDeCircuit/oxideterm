// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud sync settings form draft state.
//!
//! The GPUI app owns dialogs and async jobs. This module owns the editable form
//! model and the mapping between `SettingsInput` identities and draft fields.

use std::fmt;

use oxideterm_cloud_sync::{AuthMode, BackendType, CloudSyncSettings, ConflictStrategy};
use zeroize::{Zeroize, Zeroizing};

use crate::SettingsInput;

pub struct CloudSyncFormDraft {
    pub backend_type: BackendType,
    pub auth_mode: AuthMode,
    pub endpoint: String,
    pub namespace: String,
    pub s3_bucket: String,
    pub s3_region: String,
    pub git_repository: String,
    pub git_branch: String,
    pub github_oauth_client_id: String,
    pub microsoft_oauth_client_id: String,
    pub google_oauth_client_id: String,
    pub auto_upload_enabled: bool,
    pub auto_upload_interval_mins: String,
    pub default_conflict_strategy: ConflictStrategy,
    pub token: String,
    pub git_token: String,
    pub basic_username: String,
    pub basic_password: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: String,
    pub sync_password: String,
    pub token_touched: bool,
    pub git_token_touched: bool,
    pub basic_username_touched: bool,
    pub basic_password_touched: bool,
    pub access_key_id_touched: bool,
    pub secret_access_key_touched: bool,
    pub session_token_touched: bool,
    pub sync_password_touched: bool,
}

/// Owns secret edits after they cross the UI/backend boundary.
///
/// Values are moved out of [`CloudSyncFormDraft`] and are zeroized when this
/// handoff is dropped. The backend may return the handoff to the form after a
/// synchronous persistence failure without cloning any secret.
pub struct CloudSyncSecretDraftHandoff {
    pub token: Option<Zeroizing<String>>,
    pub git_token: Option<Zeroizing<String>>,
    pub basic_username: Option<Zeroizing<String>>,
    pub basic_password: Option<Zeroizing<String>>,
    pub access_key_id: Option<Zeroizing<String>>,
    pub secret_access_key: Option<Zeroizing<String>>,
    pub session_token: Option<Zeroizing<String>>,
    pub sync_password: Option<Zeroizing<String>>,
}

impl fmt::Debug for CloudSyncFormDraft {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Form drafts keep editable secret text in UI-owned Strings; Debug must
        // describe state without exposing credentials copied from input fields.
        formatter
            .debug_struct("CloudSyncFormDraft")
            .field("backend_type", &self.backend_type)
            .field("auth_mode", &self.auth_mode)
            .field("endpoint", &self.endpoint)
            .field("namespace", &self.namespace)
            .field("s3_bucket", &self.s3_bucket)
            .field("s3_region", &self.s3_region)
            .field("git_repository", &self.git_repository)
            .field("git_branch", &self.git_branch)
            .field("github_oauth_client_id", &self.github_oauth_client_id)
            .field("microsoft_oauth_client_id", &self.microsoft_oauth_client_id)
            .field("google_oauth_client_id", &self.google_oauth_client_id)
            .field("auto_upload_enabled", &self.auto_upload_enabled)
            .field("auto_upload_interval_mins", &self.auto_upload_interval_mins)
            .field("default_conflict_strategy", &self.default_conflict_strategy)
            .field("token", &redacted_if_present(&self.token))
            .field("git_token", &redacted_if_present(&self.git_token))
            .field("basic_username", &redacted_if_present(&self.basic_username))
            .field("basic_password", &redacted_if_present(&self.basic_password))
            .field("access_key_id", &redacted_if_present(&self.access_key_id))
            .field(
                "secret_access_key",
                &redacted_if_present(&self.secret_access_key),
            )
            .field("session_token", &redacted_if_present(&self.session_token))
            .field("sync_password", &redacted_if_present(&self.sync_password))
            .field("token_touched", &self.token_touched)
            .field("git_token_touched", &self.git_token_touched)
            .field("basic_username_touched", &self.basic_username_touched)
            .field("basic_password_touched", &self.basic_password_touched)
            .field("access_key_id_touched", &self.access_key_id_touched)
            .field("secret_access_key_touched", &self.secret_access_key_touched)
            .field("session_token_touched", &self.session_token_touched)
            .field("sync_password_touched", &self.sync_password_touched)
            .finish()
    }
}

fn redacted_if_present(value: &str) -> Option<&'static str> {
    (!value.is_empty()).then_some("<redacted>")
}

impl CloudSyncFormDraft {
    pub fn from_settings(settings: &CloudSyncSettings) -> Self {
        Self {
            backend_type: settings.backend_type.clone(),
            auth_mode: settings.auth_mode.clone(),
            endpoint: settings.endpoint.clone(),
            namespace: settings.namespace.clone(),
            s3_bucket: settings.s3_bucket.clone(),
            s3_region: settings.s3_region.clone(),
            git_repository: settings.git_repository.clone(),
            git_branch: settings.git_branch.clone(),
            github_oauth_client_id: settings.github_oauth_client_id.clone(),
            microsoft_oauth_client_id: settings.microsoft_oauth_client_id.clone(),
            google_oauth_client_id: settings.google_oauth_client_id.clone(),
            auto_upload_enabled: settings.auto_upload_enabled,
            auto_upload_interval_mins: settings.auto_upload_interval_mins.to_string(),
            default_conflict_strategy: settings.default_conflict_strategy.clone(),
            token: String::new(),
            git_token: String::new(),
            basic_username: String::new(),
            basic_password: String::new(),
            access_key_id: String::new(),
            secret_access_key: String::new(),
            session_token: String::new(),
            sync_password: String::new(),
            token_touched: false,
            git_token_touched: false,
            basic_username_touched: false,
            basic_password_touched: false,
            access_key_id_touched: false,
            secret_access_key_touched: false,
            session_token_touched: false,
            sync_password_touched: false,
        }
    }

    /// Merges persisted changes without disturbing unrelated or credential drafts.
    pub fn apply_changed_non_secret_settings(
        &mut self,
        previous: &CloudSyncSettings,
        current: &CloudSyncSettings,
    ) {
        if previous.backend_type != current.backend_type {
            self.backend_type = current.backend_type.clone();
        }
        if previous.auth_mode != current.auth_mode {
            self.auth_mode = current.auth_mode.clone();
        }
        macro_rules! apply_changed_string {
            ($field:ident) => {
                if previous.$field != current.$field {
                    self.$field = current.$field.clone();
                }
            };
        }
        apply_changed_string!(endpoint);
        apply_changed_string!(namespace);
        apply_changed_string!(s3_bucket);
        apply_changed_string!(s3_region);
        apply_changed_string!(git_repository);
        apply_changed_string!(git_branch);
        apply_changed_string!(github_oauth_client_id);
        apply_changed_string!(microsoft_oauth_client_id);
        apply_changed_string!(google_oauth_client_id);
        if previous.auto_upload_enabled != current.auto_upload_enabled {
            self.auto_upload_enabled = current.auto_upload_enabled;
        }
        if previous.auto_upload_interval_mins != current.auto_upload_interval_mins {
            self.auto_upload_interval_mins = current.auto_upload_interval_mins.to_string();
        }
        if previous.default_conflict_strategy != current.default_conflict_strategy {
            self.default_conflict_strategy = current.default_conflict_strategy.clone();
        }
    }

    pub fn take_secret_handoff(&mut self) -> CloudSyncSecretDraftHandoff {
        // `mem::take` transfers each edited buffer without creating another
        // allocation containing the secret.
        CloudSyncSecretDraftHandoff {
            token: take_touched_secret(&mut self.token, &mut self.token_touched),
            git_token: take_touched_secret(&mut self.git_token, &mut self.git_token_touched),
            basic_username: take_touched_secret(
                &mut self.basic_username,
                &mut self.basic_username_touched,
            ),
            basic_password: take_touched_secret(
                &mut self.basic_password,
                &mut self.basic_password_touched,
            ),
            access_key_id: take_touched_secret(
                &mut self.access_key_id,
                &mut self.access_key_id_touched,
            ),
            secret_access_key: take_touched_secret(
                &mut self.secret_access_key,
                &mut self.secret_access_key_touched,
            ),
            session_token: take_touched_secret(
                &mut self.session_token,
                &mut self.session_token_touched,
            ),
            sync_password: take_touched_secret(
                &mut self.sync_password,
                &mut self.sync_password_touched,
            ),
        }
    }

    pub fn restore_secret_handoff(&mut self, handoff: CloudSyncSecretDraftHandoff) {
        // A failed synchronous keychain write returns ownership to the UI
        // draft. No secret buffer is cloned during rollback.
        restore_touched_secret(handoff.token, &mut self.token, &mut self.token_touched);
        restore_touched_secret(
            handoff.git_token,
            &mut self.git_token,
            &mut self.git_token_touched,
        );
        restore_touched_secret(
            handoff.basic_username,
            &mut self.basic_username,
            &mut self.basic_username_touched,
        );
        restore_touched_secret(
            handoff.basic_password,
            &mut self.basic_password,
            &mut self.basic_password_touched,
        );
        restore_touched_secret(
            handoff.access_key_id,
            &mut self.access_key_id,
            &mut self.access_key_id_touched,
        );
        restore_touched_secret(
            handoff.secret_access_key,
            &mut self.secret_access_key,
            &mut self.secret_access_key_touched,
        );
        restore_touched_secret(
            handoff.session_token,
            &mut self.session_token,
            &mut self.session_token_touched,
        );
        restore_touched_secret(
            handoff.sync_password,
            &mut self.sync_password,
            &mut self.sync_password_touched,
        );
    }
}

impl Drop for CloudSyncFormDraft {
    fn drop(&mut self) {
        // UI drafts are allowed to use Strings, but their backing allocations
        // must not retain credentials after the owning Entity is released.
        self.token.zeroize();
        self.git_token.zeroize();
        self.basic_username.zeroize();
        self.basic_password.zeroize();
        self.access_key_id.zeroize();
        self.secret_access_key.zeroize();
        self.session_token.zeroize();
        self.sync_password.zeroize();
    }
}

fn take_touched_secret(value: &mut String, touched: &mut bool) -> Option<Zeroizing<String>> {
    if !std::mem::take(touched) {
        return None;
    }
    Some(Zeroizing::new(std::mem::take(value)))
}

fn restore_touched_secret(
    value: Option<Zeroizing<String>>,
    draft: &mut String,
    touched: &mut bool,
) {
    let Some(mut value) = value else {
        return;
    };
    *draft = std::mem::take(&mut *value);
    *touched = true;
}

pub fn cloud_sync_form_input_value_ref(
    form: &CloudSyncFormDraft,
    input: SettingsInput,
) -> Option<&str> {
    match input {
        SettingsInput::CloudSyncEndpoint => Some(form.endpoint.as_str()),
        SettingsInput::CloudSyncNamespace => Some(form.namespace.as_str()),
        SettingsInput::CloudSyncS3Bucket => Some(form.s3_bucket.as_str()),
        SettingsInput::CloudSyncS3Region => Some(form.s3_region.as_str()),
        SettingsInput::CloudSyncGitRepository => Some(form.git_repository.as_str()),
        SettingsInput::CloudSyncGitBranch => Some(form.git_branch.as_str()),
        SettingsInput::CloudSyncGithubOauthClientId => Some(form.github_oauth_client_id.as_str()),
        SettingsInput::CloudSyncMicrosoftOauthClientId => {
            Some(form.microsoft_oauth_client_id.as_str())
        }
        SettingsInput::CloudSyncGoogleOauthClientId => Some(form.google_oauth_client_id.as_str()),
        SettingsInput::CloudSyncToken => Some(form.token.as_str()),
        SettingsInput::CloudSyncGitToken => Some(form.git_token.as_str()),
        SettingsInput::CloudSyncBasicUsername => Some(form.basic_username.as_str()),
        SettingsInput::CloudSyncBasicPassword => Some(form.basic_password.as_str()),
        SettingsInput::CloudSyncAccessKeyId => Some(form.access_key_id.as_str()),
        SettingsInput::CloudSyncSecretAccessKey => Some(form.secret_access_key.as_str()),
        SettingsInput::CloudSyncSessionToken => Some(form.session_token.as_str()),
        SettingsInput::CloudSyncSyncPassword => Some(form.sync_password.as_str()),
        SettingsInput::CloudSyncAutoUploadInterval => Some(form.auto_upload_interval_mins.as_str()),
        _ => None,
    }
}

/// Moves an editable Cloud Sync value into the active IME adapter.
///
/// Secret fields must use this handoff when they receive focus so the form and
/// the root input adapter never own duplicate credential buffers.
pub fn take_cloud_sync_form_input_value(
    form: &mut CloudSyncFormDraft,
    input: SettingsInput,
) -> Option<String> {
    match input {
        SettingsInput::CloudSyncEndpoint => Some(std::mem::take(&mut form.endpoint)),
        SettingsInput::CloudSyncNamespace => Some(std::mem::take(&mut form.namespace)),
        SettingsInput::CloudSyncS3Bucket => Some(std::mem::take(&mut form.s3_bucket)),
        SettingsInput::CloudSyncS3Region => Some(std::mem::take(&mut form.s3_region)),
        SettingsInput::CloudSyncGitRepository => Some(std::mem::take(&mut form.git_repository)),
        SettingsInput::CloudSyncGitBranch => Some(std::mem::take(&mut form.git_branch)),
        SettingsInput::CloudSyncGithubOauthClientId => {
            Some(std::mem::take(&mut form.github_oauth_client_id))
        }
        SettingsInput::CloudSyncMicrosoftOauthClientId => {
            Some(std::mem::take(&mut form.microsoft_oauth_client_id))
        }
        SettingsInput::CloudSyncGoogleOauthClientId => {
            Some(std::mem::take(&mut form.google_oauth_client_id))
        }
        SettingsInput::CloudSyncToken => Some(std::mem::take(&mut form.token)),
        SettingsInput::CloudSyncGitToken => Some(std::mem::take(&mut form.git_token)),
        SettingsInput::CloudSyncBasicUsername => Some(std::mem::take(&mut form.basic_username)),
        SettingsInput::CloudSyncBasicPassword => Some(std::mem::take(&mut form.basic_password)),
        SettingsInput::CloudSyncAccessKeyId => Some(std::mem::take(&mut form.access_key_id)),
        SettingsInput::CloudSyncSecretAccessKey => {
            Some(std::mem::take(&mut form.secret_access_key))
        }
        SettingsInput::CloudSyncSessionToken => Some(std::mem::take(&mut form.session_token)),
        SettingsInput::CloudSyncSyncPassword => Some(std::mem::take(&mut form.sync_password)),
        SettingsInput::CloudSyncAutoUploadInterval => {
            Some(std::mem::take(&mut form.auto_upload_interval_mins))
        }
        _ => None,
    }
}

/// Returns an IME-owned value to the Cloud Sync Entity without cloning it.
pub fn apply_cloud_sync_form_input_owned(
    form: &mut CloudSyncFormDraft,
    input: SettingsInput,
    draft: String,
) -> Result<(), String> {
    match input {
        SettingsInput::CloudSyncEndpoint => form.endpoint = draft,
        SettingsInput::CloudSyncNamespace => form.namespace = draft,
        SettingsInput::CloudSyncS3Bucket => form.s3_bucket = draft,
        SettingsInput::CloudSyncS3Region => form.s3_region = draft,
        SettingsInput::CloudSyncGitRepository => form.git_repository = draft,
        SettingsInput::CloudSyncGitBranch => form.git_branch = draft,
        SettingsInput::CloudSyncGithubOauthClientId => form.github_oauth_client_id = draft,
        SettingsInput::CloudSyncMicrosoftOauthClientId => form.microsoft_oauth_client_id = draft,
        SettingsInput::CloudSyncGoogleOauthClientId => form.google_oauth_client_id = draft,
        SettingsInput::CloudSyncAutoUploadInterval => form.auto_upload_interval_mins = draft,
        SettingsInput::CloudSyncToken => {
            form.token = draft;
            form.token_touched = true;
        }
        SettingsInput::CloudSyncGitToken => {
            form.git_token = draft;
            form.git_token_touched = true;
        }
        SettingsInput::CloudSyncBasicUsername => {
            form.basic_username = draft;
            form.basic_username_touched = true;
        }
        SettingsInput::CloudSyncBasicPassword => {
            form.basic_password = draft;
            form.basic_password_touched = true;
        }
        SettingsInput::CloudSyncAccessKeyId => {
            form.access_key_id = draft;
            form.access_key_id_touched = true;
        }
        SettingsInput::CloudSyncSecretAccessKey => {
            form.secret_access_key = draft;
            form.secret_access_key_touched = true;
        }
        SettingsInput::CloudSyncSessionToken => {
            form.session_token = draft;
            form.session_token_touched = true;
        }
        SettingsInput::CloudSyncSyncPassword => {
            form.sync_password = draft;
            form.sync_password_touched = true;
        }
        _ => return Err(draft),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_sync_secret_input_marks_field_touched() {
        let settings = CloudSyncSettings::default();
        let mut draft = CloudSyncFormDraft::from_settings(&settings);

        apply_cloud_sync_form_input_owned(
            &mut draft,
            SettingsInput::CloudSyncToken,
            "token".to_string(),
        )
        .expect("cloud sync token input");

        assert_eq!(
            cloud_sync_form_input_value_ref(&draft, SettingsInput::CloudSyncToken),
            Some("token")
        );
        assert!(draft.token_touched);
    }

    #[test]
    fn cloud_sync_form_debug_redacts_secret_values() {
        let mut draft = CloudSyncFormDraft::from_settings(&CloudSyncSettings::default());
        draft.token = "token-secret".to_string();
        draft.git_token = "git-secret".to_string();
        draft.basic_password = "basic-secret".to_string();
        draft.secret_access_key = "s3-secret".to_string();
        draft.session_token = "session-secret".to_string();
        draft.sync_password = "sync-secret".to_string();

        let debug = format!("{draft:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("token-secret"));
        assert!(!debug.contains("git-secret"));
        assert!(!debug.contains("basic-secret"));
        assert!(!debug.contains("s3-secret"));
        assert!(!debug.contains("session-secret"));
        assert!(!debug.contains("sync-secret"));
    }

    #[test]
    fn applying_changed_non_secret_settings_preserves_unrelated_and_secret_drafts() {
        let mut draft = CloudSyncFormDraft::from_settings(&CloudSyncSettings::default());
        draft.namespace = "unsaved-namespace".to_string();
        draft.token = "token-secret".to_string();
        draft.token_touched = true;
        let previous = CloudSyncSettings::default();
        let mut current = previous.clone();
        current.endpoint = "https://sync.example.test".to_string();

        draft.apply_changed_non_secret_settings(&previous, &current);

        assert_eq!(draft.endpoint, "https://sync.example.test");
        assert_eq!(draft.namespace, "unsaved-namespace");
        assert_eq!(draft.token, "token-secret");
        assert!(draft.token_touched);
    }

    #[test]
    fn secret_handoff_moves_values_and_can_restore_without_clone() {
        let mut draft = CloudSyncFormDraft::from_settings(&CloudSyncSettings::default());
        draft.token = "token-secret".to_string();
        draft.token_touched = true;
        let original_pointer = draft.token.as_ptr();

        let handoff = draft.take_secret_handoff();

        assert!(draft.token.is_empty());
        assert!(!draft.token_touched);
        assert_eq!(
            handoff.token.as_deref().map(String::as_str),
            Some("token-secret")
        );
        assert_eq!(
            handoff.token.as_deref().map(|value| value.as_ptr()),
            Some(original_pointer)
        );

        draft.restore_secret_handoff(handoff);

        assert_eq!(draft.token, "token-secret");
        assert!(draft.token_touched);
        assert_eq!(draft.token.as_ptr(), original_pointer);
    }

    #[test]
    fn focused_secret_moves_to_ime_adapter_and_back_without_clone() {
        let mut draft = CloudSyncFormDraft::from_settings(&CloudSyncSettings::default());
        draft.sync_password = "sync-secret".to_string();
        let original_pointer = draft.sync_password.as_ptr();

        let focused =
            take_cloud_sync_form_input_value(&mut draft, SettingsInput::CloudSyncSyncPassword)
                .expect("cloud sync input");

        assert!(draft.sync_password.is_empty());
        assert_eq!(focused.as_ptr(), original_pointer);

        apply_cloud_sync_form_input_owned(
            &mut draft,
            SettingsInput::CloudSyncSyncPassword,
            focused,
        )
        .expect("cloud sync input");

        assert_eq!(draft.sync_password, "sync-secret");
        assert_eq!(draft.sync_password.as_ptr(), original_pointer);
        assert!(draft.sync_password_touched);
    }
}
