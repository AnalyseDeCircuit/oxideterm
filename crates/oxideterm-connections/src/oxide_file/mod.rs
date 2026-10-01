//! Versioned `.oxide` containers and portable configuration archives.

mod container;
mod crypto;
mod error;
mod format;
mod transfer;

pub use container::{
    OxideDocumentKind, open_local_oxide_document, open_oxide_document, seal_local_oxide_document,
    seal_oxide_document,
};

pub use crypto::{
    OxideBatchDecryptionContext, OxideBatchEncryptionContext, compute_checksum,
    decrypt_oxide_archive_with_context_and_progress, decrypt_oxide_file,
    decrypt_oxide_file_with_context_and_progress, decrypt_oxide_file_with_progress, derive_key,
    encrypt_oxide_file, encrypt_oxide_file_with_context_and_progress,
    encrypt_oxide_file_with_progress,
};
pub use error::OxideFileError;
pub use format::{
    EncryptedAuth, EncryptedConnection, EncryptedForward, EncryptedManagedKeyMetadata,
    EncryptedPayload, EncryptedPluginSetting, EncryptedPortableSecret,
    EncryptedPrivilegeCredential, EncryptedProxyHop, EncryptedUpstreamProxyAuth,
    EncryptedUpstreamProxyConfig, EncryptedUpstreamProxyPolicy, FileHeader, MAGIC, NONCE_LEN,
    OxideFile, OxideMetadata, SALT_LEN, TAG_LEN, VERSION, kdf_flags,
};
pub use transfer::{
    AppSettingsSectionPreview, DecodedSyncArchiveConnections, ExportPreflightResult, ForwardDetail,
    ImportConflictStrategy, ImportPreview, ImportPreviewRecord, ImportResultEnvelope,
    OxideExportOptions, OxideForwardRecord, OxideImportOptions, apply_oxide_import,
    apply_oxide_import_with_options, apply_oxide_import_with_options_with_context_and_progress,
    apply_oxide_import_with_options_with_progress, decode_archive_sync_connections,
    export_connections_to_oxide, export_connections_to_oxide_with_context_and_progress,
    export_connections_to_oxide_with_progress, preflight_export,
    preview_oxide_app_settings_sections, preview_oxide_import,
    preview_oxide_import_with_context_and_progress, preview_oxide_import_with_options,
    preview_oxide_import_with_progress,
};
