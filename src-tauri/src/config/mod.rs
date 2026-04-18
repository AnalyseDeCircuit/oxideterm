// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Configuration Management Module
//!
//! Handles persistent storage of connection configurations, SSH config import,
//! and secure credential storage via system keychain.
//!
//! Credential storage:
//! - SSH passwords & passphrases: `com.oxideterm.ssh` keychain service
//! - AI provider API keys: `com.oxideterm.ai` keychain service (since v1.6.0)
//! - Legacy XOR vault files (`ai_keys/*.vault`) are auto-migrated on first access

pub mod keychain;
pub mod portable;
pub mod portable_keystore;
pub mod ssh_config;
pub mod storage;
pub mod types;
pub mod vault;

#[cfg(target_os = "macos")]
pub mod touch_id;

pub use keychain::{Keychain, KeychainError};
pub use portable::{
    PortableActivationKind, PortableBootstrapStatus, PortableError, PortableHostKind,
    PortableInfo, acquire_portable_instance_lock, initialize_portable_runtime, is_portable_mode,
    portable_aware_app_data_dir, portable_bootstrap_status, portable_can_launch_full_app,
    portable_data_dir, portable_info, portable_instance_lock_path,
    set_portable_bootstrap_status,
};
pub use portable_keystore::PortableKeystoreError;
pub use ssh_config::{
    ResolvedProxyJumpHost, ResolvedSshConfigHost, SshConfigError, SshConfigHost,
    default_ssh_config_path, load_ssh_config_content, parse_ssh_config, resolve_ssh_config_host,
    resolve_ssh_config_host_content,
};
pub use storage::{
    BootstrapConfig, CONFIG_ENCRYPTION_KEY_LEN, ConfigStorage, ConfigStorageFormat, LoadedConfig,
    StorageError, config_dir, connections_file, default_dir, get_data_dir_info,
    save_bootstrap_config,
};
pub use types::{
    CONFIG_VERSION, ConfigFile, ConnectionOptions, ProxyHopConfig, SavedAuth, SavedConnection,
};
pub use vault::{AiProviderVault, AiVault, VaultError};
