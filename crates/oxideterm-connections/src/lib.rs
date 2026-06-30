mod connection_import;
mod draft;
mod keychain;
pub mod oxide_file;
mod secret;
mod ssh_config;
mod ssh_keys;
mod store;
#[cfg(target_os = "macos")]
mod touch_id;

pub use connection_import::{
    ConnectionImportApplyRequest, ConnectionImportApplyResult, ConnectionImportDuplicateStrategy,
    ConnectionImportErrorInfo, ConnectionImportPreview, ConnectionImportSource,
    ImportedConnectionAuthType, ImportedConnectionDraft, ImportedProxyHopDraft,
    apply_connection_import, preview_connection_import,
};
pub use draft::{
    ConnectionAuthDraft, ConnectionAuthDraftKind, ConnectionDraft, IMPORTED_GROUP, ProxyHopDraft,
    SSH_CONFIG_TAG, first_available_default_key_path, save_request_from_draft,
    saved_auth_from_draft, saved_connection_from_ssh_host,
};
pub use secret::SecretString;
pub use ssh_config::{
    SshBatchImportResult, SshConfigHost, SshConfigImportError, default_ssh_config_path,
    list_ssh_config_hosts, resolve_ssh_config_alias,
};
pub use ssh_keys::{SshKeyInfo, list_available_ssh_keys};
pub use store::{
    ApplySavedConnectionsSyncOutcome, ApplySavedConnectionsSyncSnapshotResult, AuthType,
    CONFIG_VERSION, ConnectionInfo, ConnectionOptions, ConnectionStore, ConnectionStoreData,
    DeletedConnectionTombstone, GLOBAL_UPSTREAM_PROXY_PASSWORD_KEYCHAIN_ID,
    LOCAL_SHELL_PRIVILEGE_CONNECTION_ID, LocalSyncMetadata, ManagedSshKeyInfo, ManagedSshKeyOrigin,
    ManagedSshKeyUsage, PrivilegeCredentialKind, ProxyHopInfo, RawTcpDisplayMode, RawTcpLineEnding,
    RawTcpProfile, RawTcpProfilesSyncSnapshot, RawTcpSendMode, RawTcpTlsMode,
    RawTcpTlsVerification, RawUdpDisplayMode, RawUdpLineEnding, RawUdpProfile,
    RawUdpProfilesSyncSnapshot, RawUdpSendMode, SaveConnectionRequest,
    SavePrivilegeCredentialRequest, SaveRawTcpProfileRequest, SaveRawUdpProfileRequest,
    SaveSerialProfileRequest, SaveTelnetProfileRequest, SavedAuth, SavedConnection,
    SavedConnectionSyncRecord, SavedConnectionsConflictStrategy, SavedConnectionsSyncSnapshot,
    SavedPrivilegeCredential, SavedProxyHop, SavedUpstreamProxyAuth, SavedUpstreamProxyConfig,
    SavedUpstreamProxyPolicy, SavedUpstreamProxyProtocol, SerialFlowControl, SerialParity,
    SerialProfile, SerialProfilesSyncSnapshot, TelnetProfile, validate_group_name,
};
