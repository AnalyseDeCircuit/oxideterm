//! Configuration Management Module
//!
//! Handles persistent storage of connection configurations, SSH config import,
//! and secure credential storage via system keychain and local vault.

pub mod keychain;
pub mod ssh_config;
pub mod storage;
pub mod types;
pub mod vault;

pub use keychain::{Keychain, KeychainError};
pub use ssh_config::{default_ssh_config_path, parse_ssh_config, SshConfigError, SshConfigHost};
pub use storage::{config_dir, connections_file, ConfigStorage, StorageError};
pub use types::{
    ConfigFile, ConnectionOptions, ProxyHopConfig, SavedAuth, SavedConnection, CONFIG_VERSION,
};
pub use vault::{AiProviderVault, AiVault, VaultError};
