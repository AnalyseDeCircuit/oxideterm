// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Arc;

use oxideterm_connections::{ConnectionStore, SavedAuth, SecretString};
use oxideterm_ssh::{AuthMethod, ManagedKeyResolver, SshTransportError};

pub fn auth_method_from_saved_auth(
    store: &ConnectionStore,
    auth: &SavedAuth,
) -> Option<AuthMethod> {
    Some(match auth {
        SavedAuth::Password {
            empty_password: true,
            ..
        } => AuthMethod::password(""),
        SavedAuth::Password {
            plaintext_password: Some(password),
            ..
        } => AuthMethod::password_secret(password.clone().into_zeroizing()),
        SavedAuth::Password {
            keychain_id: Some(_),
            ..
        } => {
            // A retained reference does not prove the credential exists in this device's store.
            // Use the runtime password prompt so its consent and successful-auth save path apply.
            match store.get_saved_auth_password_optional(auth).ok()? {
                Some(password) => AuthMethod::password_secret(password.into_zeroizing()),
                None => AuthMethod::password_prompt(),
            }
        }
        SavedAuth::Password {
            keychain_id: None,
            plaintext_password: None,
            ..
        } => AuthMethod::password_prompt(),
        SavedAuth::Key {
            key_path,
            plaintext_passphrase,
            ..
        } => AuthMethod::key_secret(
            key_path.clone(),
            plaintext_passphrase
                .clone()
                .or_else(|| store.get_saved_auth_passphrase(auth).ok().flatten())
                .map(SecretString::into_zeroizing),
        ),
        SavedAuth::Certificate {
            key_path,
            cert_path,
            plaintext_passphrase,
            ..
        } => AuthMethod::certificate_secret(
            key_path.clone(),
            cert_path.clone(),
            plaintext_passphrase
                .clone()
                .or_else(|| store.get_saved_auth_passphrase(auth).ok().flatten())
                .map(SecretString::into_zeroizing),
        ),
        SavedAuth::ManagedKey {
            key_id,
            passphrase_keychain_id,
            ..
        } => AuthMethod::managed_key_secret(
            key_id.clone(),
            passphrase_keychain_id
                .as_ref()
                .and_then(|_| store.get_saved_auth_passphrase(auth).ok().flatten())
                .map(SecretString::into_zeroizing),
        ),
        // Keyboard-interactive prompts are collected by the runtime prompt handler.
        SavedAuth::KeyboardInteractive => AuthMethod::KeyboardInteractive,
        SavedAuth::Agent => AuthMethod::Agent,
        SavedAuth::KerberosPreferred {
            server_identity,
            delegate_credentials,
            fallback,
        } => AuthMethod::kerberos_preferred(
            auth_method_from_saved_auth(store, fallback)?,
            server_identity.clone(),
            *delegate_credentials,
        ),
    })
}

pub fn managed_key_resolver_from_store(store: &ConnectionStore) -> ManagedKeyResolver {
    let path = store.path().to_path_buf();
    Arc::new(move |key_id| {
        let store = ConnectionStore::load_read_only(&path)
            .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?;
        store
            .resolve_managed_ssh_key_private_key(key_id)
            .and_then(|private_key| {
                Ok(oxideterm_ssh::ManagedKeyMaterial {
                    private_key: private_key.into_zeroizing(),
                    certificate: store.managed_ssh_key_metadata(key_id)?.certificate,
                })
            })
            .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))
    })
}
