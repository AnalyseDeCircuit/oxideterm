use crate::SecretString;
use anyhow::{Context, Result};
use oxideterm_portable_runtime::keystore::{self as portable_keystore, PortableKeystoreError};
use oxideterm_secret_store::NativeSecretStore;
#[cfg(test)]
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
const SERVICE_NAME: &str = "com.oxideterm.ssh";
const PORTABLE_ACCOUNT_PREFIX: &str = "portable:v1:";
const LEGACY_ACCOUNT_SEPARATOR: &str = "@";

#[derive(Clone, Debug)]
pub(crate) struct ConnectionKeychain {
    service: String,
    #[cfg(target_os = "macos")]
    authentication_reason: Option<String>,
    #[cfg(test)]
    test_store: Option<Arc<Mutex<HashMap<String, SecretString>>>>,
    #[cfg(test)]
    test_max_secret_bytes: Option<usize>,
}

/// Grants multiple reads after one operation-scoped authentication.
pub(crate) struct ConnectionKeychainBatchAccess<'a> {
    keychain: &'a ConnectionKeychain,
}

impl Default for ConnectionKeychain {
    fn default() -> Self {
        Self {
            service: SERVICE_NAME.to_string(),
            #[cfg(target_os = "macos")]
            authentication_reason: None,
            #[cfg(test)]
            test_store: Some(Arc::new(Mutex::new(HashMap::new()))),
            #[cfg(test)]
            test_max_secret_bytes: None,
        }
    }
}

impl ConnectionKeychain {
    pub(crate) fn with_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            #[cfg(target_os = "macos")]
            authentication_reason: None,
            #[cfg(test)]
            test_store: Some(Arc::new(Mutex::new(HashMap::new()))),
            #[cfg(test)]
            test_max_secret_bytes: None,
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn with_macos_device_owner_authentication(
        service: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            service: service.into(),
            authentication_reason: Some(reason.into()),
            #[cfg(test)]
            test_store: Some(Arc::new(Mutex::new(HashMap::new()))),
            #[cfg(test)]
            test_max_secret_bytes: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_max_secret_bytes_for_tests(
        service: impl Into<String>,
        max_secret_bytes: usize,
    ) -> Self {
        Self {
            service: service.into(),
            #[cfg(target_os = "macos")]
            authentication_reason: None,
            test_store: Some(Arc::new(Mutex::new(HashMap::new()))),
            test_max_secret_bytes: Some(max_secret_bytes),
        }
    }

    pub(crate) fn store(&self, id: &str, secret: &SecretString) -> Result<()> {
        #[cfg(test)]
        if let Some(store) = &self.test_store {
            if self
                .test_max_secret_bytes
                .is_some_and(|limit| secret.expose_secret().len() > limit)
            {
                // Tests use this to emulate OS credential backends that reject
                // large managed SSH keys, such as RSA private-key blobs.
                anyhow::bail!("test keychain secret exceeds configured byte limit");
            }
            store
                .lock()
                .map_err(|error| anyhow::anyhow!("failed to lock test keychain: {error}"))?
                .insert(id.to_string(), secret.clone());
            return Ok(());
        }

        if portable_keychain_enabled()? {
            let account = portable_account(id);
            let legacy_suffix = legacy_portable_account_suffix(id);
            return portable_keystore::store_secret_replacing_legacy_accounts(
                &self.service,
                &account,
                &legacy_suffix,
                secret.expose_secret(),
            )
            .with_context(|| format!("failed to store password in portable keystore for {id}"));
        }

        NativeSecretStore::new(&self.service)
            .store(&self.native_account(id), secret.expose_secret())
            .with_context(|| format!("failed to store password in OS keychain for {id}"))
    }

    pub(crate) fn get(&self, id: &str) -> Result<SecretString> {
        self.get_optional(id)?
            .ok_or_else(|| anyhow::anyhow!("Password not saved for this connection"))
    }

    pub(crate) fn get_optional(&self, id: &str) -> Result<Option<SecretString>> {
        self.get_optional_with_authentication(id, true)
    }

    pub(crate) fn authenticate_batch_access(&self) -> Result<ConnectionKeychainBatchAccess<'_>> {
        #[cfg(test)]
        if self.test_store.is_some() {
            return Ok(ConnectionKeychainBatchAccess { keychain: self });
        }

        #[cfg(target_os = "macos")]
        if let Some(reason) = self.authentication_reason.as_deref() {
            oxideterm_secret_store::authenticate_device_owner(reason)
                .context("failed to authenticate batch keychain access")?;
        }
        Ok(ConnectionKeychainBatchAccess { keychain: self })
    }

    fn get_optional_with_authentication(
        &self,
        id: &str,
        authenticate_device_owner: bool,
    ) -> Result<Option<SecretString>> {
        #[cfg(not(target_os = "macos"))]
        let _ = authenticate_device_owner;

        #[cfg(test)]
        if let Some(store) = &self.test_store {
            return Ok(store
                .lock()
                .map_err(|error| anyhow::anyhow!("failed to lock test keychain: {error}"))?
                .get(id)
                .cloned());
        }

        if portable_keychain_enabled()? {
            let account = portable_account(id);
            let legacy_suffix = legacy_portable_account_suffix(id);
            return match portable_keystore::get_secret_migrating_legacy_account(
                &self.service,
                &account,
                &legacy_suffix,
            ) {
                Ok(secret) => Ok(Some(SecretString::from(secret))),
                Err(PortableKeystoreError::NotFound(_)) => Ok(None),
                Err(error) => Err(error).with_context(|| {
                    format!("failed to load password from portable keystore for {id}")
                }),
            };
        }

        #[cfg(target_os = "macos")]
        if authenticate_device_owner && let Some(reason) = self.authentication_reason.as_deref() {
            oxideterm_secret_store::authenticate_device_owner(reason)
                .with_context(|| format!("failed to authenticate keychain access for {id}"))?;
        }

        NativeSecretStore::new(&self.service)
            .get_and_relax(&self.native_account(id))
            // Move the keychain result directly into its zeroizing domain owner
            // so no unmanaged String copy survives this boundary.
            .map(|secret| secret.map(SecretString::from))
            .with_context(|| format!("failed to load password from OS keychain for {id}"))
    }

    pub(crate) fn delete(&self, id: &str) -> Result<()> {
        #[cfg(test)]
        if let Some(store) = &self.test_store {
            store
                .lock()
                .map_err(|error| anyhow::anyhow!("failed to lock test keychain: {error}"))?
                .remove(id);
            return Ok(());
        }

        if portable_keychain_enabled()? {
            let account = portable_account(id);
            let legacy_suffix = legacy_portable_account_suffix(id);
            return portable_keystore::delete_secret_with_legacy_accounts(
                &self.service,
                &account,
                &legacy_suffix,
            )
            .with_context(|| format!("failed to delete password from portable keystore for {id}"));
        }

        NativeSecretStore::new(&self.service)
            .delete(&self.native_account(id))
            .with_context(|| format!("failed to delete password from OS keychain for {id}"))
    }

    fn native_account(&self, id: &str) -> String {
        format!("{}@{}", whoami::username(), id)
    }
}

impl ConnectionKeychainBatchAccess<'_> {
    pub(crate) fn get_optional(&self, id: &str) -> Result<Option<SecretString>> {
        // The guard can only be constructed after the batch authorization step.
        self.keychain.get_optional_with_authentication(id, false)
    }
}

fn portable_account(id: &str) -> String {
    // Portable Vault authentication, rather than the host OS account, owns
    // access to this stable entry across machines and system users.
    format!("{PORTABLE_ACCOUNT_PREFIX}{id}")
}

fn legacy_portable_account_suffix(id: &str) -> String {
    format!("{LEGACY_ACCOUNT_SEPARATOR}{id}")
}

fn portable_keychain_enabled() -> Result<bool> {
    oxideterm_portable_runtime::is_portable_mode()
        .context("failed to determine OxideTerm portable mode")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_account_is_independent_of_host_username() {
        assert_eq!(
            portable_account("managed-key-id"),
            "portable:v1:managed-key-id"
        );
        assert_eq!(
            legacy_portable_account_suffix("managed-key-id"),
            "@managed-key-id"
        );
    }

    #[test]
    fn batch_access_reuses_one_guard_for_multiple_secret_reads() {
        let keychain = ConnectionKeychain::with_service("com.oxideterm.test.batch");
        keychain.store("first", &SecretString::from("one")).unwrap();
        keychain
            .store("second", &SecretString::from("two"))
            .unwrap();

        let access = keychain.authenticate_batch_access().unwrap();

        assert_eq!(
            access.get_optional("first").unwrap(),
            Some(SecretString::from("one"))
        );
        assert_eq!(
            access.get_optional("second").unwrap(),
            Some(SecretString::from("two"))
        );
    }
}
