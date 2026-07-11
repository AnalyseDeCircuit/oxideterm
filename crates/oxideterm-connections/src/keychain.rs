use crate::SecretString;
use anyhow::{Context, Result};
use keyring::Entry;
use oxideterm_portable_runtime::keystore::{self as portable_keystore, PortableKeystoreError};
#[cfg(test)]
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
#[cfg(target_os = "macos")]
use zeroize::Zeroizing;

const SERVICE_NAME: &str = "com.oxideterm.ssh";

#[cfg(target_os = "macos")]
mod macos_user_presence {
    use security_framework::passwords::{
        AccessControlOptions, PasswordOptions, delete_generic_password_options, generic_password,
        set_generic_password_options,
    };
    use security_framework_sys::base::errSecItemNotFound;

    pub(super) enum ReadError {
        NotFound,
        Backend(security_framework::base::Error),
    }

    fn options(service: &str, account: &str) -> PasswordOptions {
        let mut options = PasswordOptions::new_generic_password(service, account);
        // The data-protection keychain makes user presence part of the item,
        // so macOS performs one Touch ID or password authentication for reads.
        options.use_protected_keychain();
        options
    }

    pub(super) fn store(service: &str, account: &str, secret: &[u8]) -> anyhow::Result<()> {
        let mut options = options(service, account);
        options.set_access_control_options(AccessControlOptions::USER_PRESENCE);
        set_generic_password_options(secret, options).map_err(anyhow::Error::new)
    }

    pub(super) fn get(service: &str, account: &str) -> Result<Vec<u8>, ReadError> {
        generic_password(options(service, account)).map_err(|error| {
            if error.code() == errSecItemNotFound {
                ReadError::NotFound
            } else {
                ReadError::Backend(error)
            }
        })
    }

    pub(super) fn delete(service: &str, account: &str) -> anyhow::Result<()> {
        match delete_generic_password_options(options(service, account)) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == errSecItemNotFound => Ok(()),
            Err(error) => Err(anyhow::Error::new(error)),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ConnectionKeychain {
    service: String,
    #[cfg(target_os = "macos")]
    require_user_presence: bool,
    #[cfg(test)]
    test_store: Option<Arc<Mutex<HashMap<String, SecretString>>>>,
    #[cfg(test)]
    test_max_secret_bytes: Option<usize>,
}

impl Default for ConnectionKeychain {
    fn default() -> Self {
        Self {
            service: SERVICE_NAME.to_string(),
            #[cfg(target_os = "macos")]
            require_user_presence: false,
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
            require_user_presence: false,
            #[cfg(test)]
            test_store: Some(Arc::new(Mutex::new(HashMap::new()))),
            #[cfg(test)]
            test_max_secret_bytes: None,
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn with_macos_user_presence(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            require_user_presence: true,
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
            require_user_presence: false,
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
            let account = self.account(id);
            return portable_keystore::store_secret(
                &self.service,
                &account,
                secret.expose_secret(),
            )
            .with_context(|| format!("failed to store password in portable keystore for {id}"));
        }

        #[cfg(target_os = "macos")]
        if self.require_user_presence {
            let account = self.account(id);
            macos_user_presence::store(&self.service, &account, secret.expose_secret().as_bytes())
                .with_context(|| {
                    format!("failed to store user-presence protected credential for {id}")
                })?;
            // Remove a legacy keyring entry after the protected copy is durable.
            let entry = self.entry(id)?;
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(error) => Err(error).with_context(|| {
                    format!("failed to remove legacy OS keychain credential for {id}")
                }),
            }?;
            return Ok(());
        }

        let entry = self.entry(id)?;
        // keyring's apple-native backend keeps the secret out of process argv.
        entry
            .set_password(secret.expose_secret())
            .with_context(|| format!("failed to store password in OS keychain for {id}"))
    }

    pub(crate) fn get(&self, id: &str) -> Result<SecretString> {
        self.get_optional(id)?
            .ok_or_else(|| anyhow::anyhow!("Password not saved for this connection"))
    }

    pub(crate) fn get_optional(&self, id: &str) -> Result<Option<SecretString>> {
        #[cfg(test)]
        if let Some(store) = &self.test_store {
            return Ok(store
                .lock()
                .map_err(|error| anyhow::anyhow!("failed to lock test keychain: {error}"))?
                .get(id)
                .cloned());
        }

        if portable_keychain_enabled()? {
            let account = self.account(id);
            return match portable_keystore::get_secret(&self.service, &account) {
                Ok(secret) => Ok(Some(SecretString::from(secret))),
                Err(PortableKeystoreError::NotFound(_)) => Ok(None),
                Err(error) => Err(error).with_context(|| {
                    format!("failed to load password from portable keystore for {id}")
                }),
            };
        }

        #[cfg(target_os = "macos")]
        if self.require_user_presence {
            let account = self.account(id);
            match macos_user_presence::get(&self.service, &account) {
                Ok(secret) => {
                    let secret = Zeroizing::new(secret);
                    let secret = std::str::from_utf8(secret.as_slice()).with_context(|| {
                        format!("stored credential is not valid UTF-8 for {id}")
                    })?;
                    return Ok(Some(SecretString::from(secret.to_owned())));
                }
                Err(macos_user_presence::ReadError::Backend(error)) => {
                    return Err(error).with_context(|| {
                        format!("failed to authenticate protected keychain access for {id}")
                    });
                }
                Err(macos_user_presence::ReadError::NotFound) => {}
            }

            // Legacy entries used an executable ACL. Reading one can require a
            // one-time password prompt after a cargo rebuild; migrate it so all
            // subsequent reads use the single system user-presence prompt.
            let entry = self.entry(id)?;
            return match entry.get_password() {
                Ok(secret) => {
                    let secret = SecretString::from(secret);
                    macos_user_presence::store(
                        &self.service,
                        &account,
                        secret.expose_secret().as_bytes(),
                    )
                    .with_context(|| format!("failed to migrate keychain credential for {id}"))?;
                    entry.delete_credential().with_context(|| {
                        format!("failed to remove migrated keychain credential for {id}")
                    })?;
                    Ok(Some(secret))
                }
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(error) => Err(error)
                    .with_context(|| format!("failed to load password from OS keychain for {id}")),
            };
        }

        let entry = self.entry(id)?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(SecretString::from(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error)
                .with_context(|| format!("failed to load password from OS keychain for {id}")),
        }
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
            let account = self.account(id);
            return portable_keystore::delete_secret(&self.service, &account).with_context(|| {
                format!("failed to delete password from portable keystore for {id}")
            });
        }

        #[cfg(target_os = "macos")]
        if self.require_user_presence {
            let account = self.account(id);
            macos_user_presence::delete(&self.service, &account).with_context(|| {
                format!("failed to delete protected keychain credential for {id}")
            })?;
            // A legacy entry may still exist if migration was interrupted.
            let entry = self.entry(id)?;
            return match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(error) => Err(error).with_context(|| {
                    format!("failed to delete legacy OS keychain credential for {id}")
                }),
            };
        }

        let entry = self.entry(id)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error)
                .with_context(|| format!("failed to delete password from OS keychain for {id}")),
        }
    }

    fn account(&self, id: &str) -> String {
        format!("{}@{}", whoami::username(), id)
    }

    fn entry(&self, id: &str) -> Result<Entry> {
        let account = self.account(id);
        Entry::new(&self.service, &account)
            .with_context(|| format!("failed to open OS keychain entry {} for {id}", self.service))
    }
}

fn portable_keychain_enabled() -> Result<bool> {
    oxideterm_portable_runtime::is_portable_mode()
        .context("failed to determine OxideTerm portable mode")
}
