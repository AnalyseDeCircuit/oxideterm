mod draft;
mod keychain;
pub mod oxide_file;
mod secret;
mod ssh_config;
mod ssh_keys;
mod store;

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
    DeletedConnectionTombstone, LocalSyncMetadata, ManagedSshKeyInfo, ManagedSshKeyOrigin,
    ManagedSshKeyUsage, ProxyHopInfo, SaveConnectionRequest, SavedAuth, SavedConnection,
    SavedConnectionSyncRecord, SavedConnectionsConflictStrategy, SavedConnectionsSyncSnapshot,
    SavedProxyHop, SerialFlowControl, SerialParity, SerialProfile, validate_group_name,
};
