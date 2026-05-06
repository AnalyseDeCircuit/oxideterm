mod draft;
mod keychain;
mod ssh_config;
mod ssh_keys;
mod store;

pub use draft::{
    ConnectionAuthDraft, ConnectionAuthDraftKind, ConnectionDraft, IMPORTED_GROUP, ProxyHopDraft,
    SSH_CONFIG_TAG, save_request_from_draft, saved_auth_from_draft, saved_connection_from_ssh_host,
};
pub use ssh_config::{
    SshBatchImportResult, SshConfigHost, SshConfigImportError, default_ssh_config_path,
    list_ssh_config_hosts, resolve_ssh_config_alias,
};
pub use ssh_keys::{SshKeyInfo, list_available_ssh_keys};
pub use store::{
    AuthType, ConnectionInfo, ConnectionOptions, ConnectionStore, ConnectionStoreData,
    SaveConnectionRequest, SavedAuth, SavedConnection, SavedProxyHop, validate_group_name,
};
