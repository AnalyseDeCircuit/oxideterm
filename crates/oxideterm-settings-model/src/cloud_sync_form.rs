// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud sync settings form draft state.
//!
//! The GPUI app owns dialogs and async jobs. This module owns the editable form
//! model and the mapping between `SettingsInput` identities and draft fields.

use oxideterm_cloud_sync::{AuthMode, BackendType, CloudSyncSettings, ConflictStrategy};

use crate::SettingsInput;

#[derive(Clone, Debug)]
pub struct CloudSyncFormDraft {
    pub backend_type: BackendType,
    pub auth_mode: AuthMode,
    pub endpoint: String,
    pub namespace: String,
    pub s3_bucket: String,
    pub s3_region: String,
    pub git_repository: String,
    pub git_branch: String,
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
}

pub fn cloud_sync_form_input_value(
    form: &CloudSyncFormDraft,
    input: SettingsInput,
) -> Option<String> {
    match input {
        SettingsInput::CloudSyncEndpoint => Some(form.endpoint.clone()),
        SettingsInput::CloudSyncNamespace => Some(form.namespace.clone()),
        SettingsInput::CloudSyncS3Bucket => Some(form.s3_bucket.clone()),
        SettingsInput::CloudSyncS3Region => Some(form.s3_region.clone()),
        SettingsInput::CloudSyncGitRepository => Some(form.git_repository.clone()),
        SettingsInput::CloudSyncGitBranch => Some(form.git_branch.clone()),
        SettingsInput::CloudSyncToken => Some(form.token.clone()),
        SettingsInput::CloudSyncGitToken => Some(form.git_token.clone()),
        SettingsInput::CloudSyncBasicUsername => Some(form.basic_username.clone()),
        SettingsInput::CloudSyncBasicPassword => Some(form.basic_password.clone()),
        SettingsInput::CloudSyncAccessKeyId => Some(form.access_key_id.clone()),
        SettingsInput::CloudSyncSecretAccessKey => Some(form.secret_access_key.clone()),
        SettingsInput::CloudSyncSessionToken => Some(form.session_token.clone()),
        SettingsInput::CloudSyncSyncPassword => Some(form.sync_password.clone()),
        SettingsInput::CloudSyncAutoUploadInterval => Some(form.auto_upload_interval_mins.clone()),
        _ => None,
    }
}

pub fn apply_cloud_sync_form_input_draft(
    form: &mut CloudSyncFormDraft,
    input: SettingsInput,
    draft: &str,
) -> bool {
    match input {
        SettingsInput::CloudSyncEndpoint => form.endpoint = draft.to_string(),
        SettingsInput::CloudSyncNamespace => form.namespace = draft.to_string(),
        SettingsInput::CloudSyncS3Bucket => form.s3_bucket = draft.to_string(),
        SettingsInput::CloudSyncS3Region => form.s3_region = draft.to_string(),
        SettingsInput::CloudSyncGitRepository => form.git_repository = draft.to_string(),
        SettingsInput::CloudSyncGitBranch => form.git_branch = draft.to_string(),
        SettingsInput::CloudSyncAutoUploadInterval => {
            form.auto_upload_interval_mins = draft.to_string();
        }
        SettingsInput::CloudSyncToken => {
            // Secret fields track whether the user explicitly edited them so
            // save can preserve untouched keychain values.
            form.token = draft.to_string();
            form.token_touched = true;
        }
        SettingsInput::CloudSyncGitToken => {
            form.git_token = draft.to_string();
            form.git_token_touched = true;
        }
        SettingsInput::CloudSyncBasicUsername => {
            form.basic_username = draft.to_string();
            form.basic_username_touched = true;
        }
        SettingsInput::CloudSyncBasicPassword => {
            form.basic_password = draft.to_string();
            form.basic_password_touched = true;
        }
        SettingsInput::CloudSyncAccessKeyId => {
            form.access_key_id = draft.to_string();
            form.access_key_id_touched = true;
        }
        SettingsInput::CloudSyncSecretAccessKey => {
            form.secret_access_key = draft.to_string();
            form.secret_access_key_touched = true;
        }
        SettingsInput::CloudSyncSessionToken => {
            form.session_token = draft.to_string();
            form.session_token_touched = true;
        }
        SettingsInput::CloudSyncSyncPassword => {
            form.sync_password = draft.to_string();
            form.sync_password_touched = true;
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_sync_secret_input_marks_field_touched() {
        let settings = CloudSyncSettings::default();
        let mut draft = CloudSyncFormDraft::from_settings(&settings);

        assert!(apply_cloud_sync_form_input_draft(
            &mut draft,
            SettingsInput::CloudSyncToken,
            "token"
        ));

        assert_eq!(
            cloud_sync_form_input_value(&draft, SettingsInput::CloudSyncToken).as_deref(),
            Some("token")
        );
        assert!(draft.token_touched);
    }
}
