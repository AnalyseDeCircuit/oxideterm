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
            plaintext_password: Some(password),
            ..
        } => AuthMethod::password_secret(password.clone().into_zeroizing()),
        SavedAuth::Password {
            keychain_id: Some(_),
            ..
        } => {
            AuthMethod::password_secret(store.get_saved_auth_password(auth).ok()?.into_zeroizing())
        }
        SavedAuth::Password {
            keychain_id: None,
            plaintext_password: None,
        } => return None,
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
        SavedAuth::Agent => AuthMethod::Agent,
    })
}

pub fn managed_key_resolver_from_store(store: &ConnectionStore) -> ManagedKeyResolver {
    let store = store.clone();
    Arc::new(move |key_id| {
        store
            .resolve_managed_ssh_key_private_key(key_id)
            .map(SecretString::into_zeroizing)
            .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))
    })
}
