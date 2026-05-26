//! Tauri-compatible `.oxide` import/export codec.
//!
//! This module intentionally preserves the existing binary format instead of
//! introducing a native-only variant.

mod crypto;
mod error;
mod format;
mod transfer;

pub use crypto::{
    compute_checksum, decrypt_oxide_file, decrypt_oxide_file_with_progress, derive_key,
    encrypt_oxide_file, encrypt_oxide_file_with_progress,
};
pub use error::OxideFileError;
pub use format::{
    EncryptedAuth, EncryptedConnection, EncryptedForward, EncryptedPayload, EncryptedPluginSetting,
    EncryptedPortableSecret, EncryptedProxyHop, FileHeader, MAGIC, NONCE_LEN, OxideFile,
    OxideMetadata, SALT_LEN, TAG_LEN, VERSION, kdf_flags,
};
pub use transfer::{
    AppSettingsSectionPreview, ExportPreflightResult, ForwardDetail, ImportConflictStrategy,
    ImportPreview, ImportPreviewRecord, ImportResultEnvelope, OxideExportOptions,
    OxideForwardRecord, OxideImportOptions, apply_oxide_import, apply_oxide_import_with_options,
    apply_oxide_import_with_options_with_progress, export_connections_to_oxide,
    export_connections_to_oxide_with_progress, preflight_export, preview_oxide_import,
    preview_oxide_import_with_options, preview_oxide_import_with_progress,
};
