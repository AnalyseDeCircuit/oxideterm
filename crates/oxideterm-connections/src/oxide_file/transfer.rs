use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::store::{ImportedManagedSshKey, ManagedSshKey, ManagedSshKeyOrigin};
use crate::{
    AuthType, CONFIG_VERSION, ConnectionOptions, ConnectionStore, SavedAuth, SavedConnection,
    SavedProxyHop, SecretString,
};

use super::{
    EncryptedAuth, EncryptedConnection, EncryptedForward, EncryptedManagedKeyMetadata,
    EncryptedPayload, EncryptedPluginSetting, EncryptedPortableSecret, EncryptedProxyHop,
    OxideFile, OxideFileError, OxideMetadata, compute_checksum, decrypt_oxide_file_with_progress,
    encrypt_oxide_file, encrypt_oxide_file_with_progress,
};

const EMBEDDED_KEY_MAX_BYTES: u64 = 1_048_576;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OxideForwardRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub connection_id: String,
    pub forward_type: String,
    pub bind_address: String,
    pub bind_port: u16,
    pub target_host: String,
    pub target_port: u16,
    pub description: Option<String>,
    pub auto_start: bool,
}

#[derive(Debug, Clone)]
pub struct OxideExportOptions {
    pub description: Option<String>,
    pub embed_keys: bool,
    pub include_passwords: bool,
    pub include_key_passphrases: bool,
    pub include_managed_keys: bool,
    pub include_managed_key_passphrases: bool,
    pub app_settings_json: Option<String>,
    pub quick_commands_json: Option<String>,
    pub plugin_settings: Vec<EncryptedPluginSetting>,
    pub portable_secrets: Vec<EncryptedPortableSecret>,
    pub forwards: Vec<OxideForwardRecord>,
}

impl Default for OxideExportOptions {
    fn default() -> Self {
        Self {
            description: None,
            embed_keys: false,
            include_passwords: false,
            include_key_passphrases: true,
            include_managed_keys: true,
            include_managed_key_passphrases: false,
            app_settings_json: None,
            quick_commands_json: None,
            plugin_settings: Vec::new(),
            portable_secrets: Vec::new(),
            forwards: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OxideImportOptions {
    pub selected_names: Option<Vec<String>>,
    pub selected_forward_ids: Option<Vec<String>>,
    pub conflict_strategy: ImportConflictStrategy,
    pub import_forwards: bool,
    pub import_portable_secrets: bool,
    /// Restore managed-key metadata instead of extracting managed keys as plain imported key files.
    pub restore_managed_keys: bool,
    /// Store managed-key passphrases from the encrypted archive when callers explicitly opt in.
    pub restore_managed_key_passphrases: bool,
}

impl Default for OxideImportOptions {
    fn default() -> Self {
        Self {
            selected_names: None,
            selected_forward_ids: None,
            conflict_strategy: ImportConflictStrategy::Rename,
            import_forwards: true,
            import_portable_secrets: false,
            restore_managed_keys: true,
            restore_managed_key_passphrases: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPreflightResult {
    pub total_connections: usize,
    pub missing_keys: Vec<(String, String)>,
    pub connections_with_keys: usize,
    pub connections_with_passwords: usize,
    pub connections_with_agent: usize,
    pub key_passphrase_count: usize,
    pub managed_key_count: usize,
    pub managed_key_passphrase_count: usize,
    pub blocked_managed_key_connections: Vec<String>,
    pub total_key_bytes: u64,
    pub can_export: bool,
    pub portable_secret_count: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ImportConflictStrategy {
    Rename,
    Skip,
    Replace,
    Merge,
}

impl ImportConflictStrategy {
    pub fn parse(value: Option<&str>) -> Result<Self, OxideFileError> {
        match value.unwrap_or("rename") {
            "rename" => Ok(Self::Rename),
            "skip" => Ok(Self::Skip),
            "replace" => Ok(Self::Replace),
            "merge" => Ok(Self::Merge),
            other => Err(OxideFileError::InvalidFormat(format!(
                "Unsupported conflict strategy: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub total_connections: usize,
    pub unchanged: Vec<String>,
    pub will_rename: Vec<(String, String)>,
    pub will_skip: Vec<String>,
    pub will_replace: Vec<String>,
    pub will_merge: Vec<String>,
    pub has_embedded_keys: bool,
    pub total_forwards: usize,
    pub has_app_settings: bool,
    pub has_quick_commands: bool,
    pub quick_commands_count: usize,
    pub quick_command_categories_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_settings_format: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub app_settings_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub app_settings_preview: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub app_settings_sections: Vec<AppSettingsSectionPreview>,
    pub plugin_settings_count: usize,
    pub portable_secret_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub app_settings_section_ids: Vec<String>,
    pub app_settings_contains_local_terminal_env_vars: bool,
    pub plugin_settings_by_plugin: HashMap<String, usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forward_details: Vec<ForwardDetail>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<ImportPreviewRecord>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardDetail {
    pub owner_connection_name: String,
    pub direction: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsSectionPreview {
    pub id: String,
    pub field_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub field_values: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub contains_env_vars: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewRecord {
    pub resource: String,
    pub name: String,
    pub action: String,
    pub reason_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_connection_id: Option<String>,
    pub forward_count: usize,
    pub has_embedded_keys: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResultEnvelope {
    pub imported: usize,
    pub skipped: usize,
    pub merged: usize,
    pub replaced: usize,
    pub renamed: usize,
    pub errors: Vec<String>,
    pub renames: Vec<(String, String)>,
    pub imported_forwards: usize,
    pub skipped_forwards: usize,
    pub imported_portable_secrets: usize,
    pub skipped_portable_secrets: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_settings_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quick_commands_json: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugin_settings: Vec<EncryptedPluginSetting>,
    #[serde(skip)]
    pub forward_records: Vec<OxideForwardRecord>,
    #[serde(skip)]
    pub forward_replace_owner_ids: Vec<String>,
    #[serde(skip)]
    pub forward_merge_owner_ids: Vec<String>,
    #[serde(skip)]
    pub portable_secrets: Vec<EncryptedPortableSecret>,
}

#[derive(Debug, Clone)]
enum PlannedImportAction {
    Import,
    Rename(String),
    Skip,
    Replace(String),
    Merge(String),
}

include!("transfer/common.rs");
include!("transfer/export.rs");
include!("transfer/preview.rs");
include!("transfer/import.rs");
include!("transfer/app_settings.rs");
include!("transfer/planning.rs");
include!("transfer/tests.rs");
