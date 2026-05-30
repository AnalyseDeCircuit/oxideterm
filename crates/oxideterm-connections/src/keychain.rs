use anyhow::{Context, Result, bail};
use keyring::Entry;
use oxideterm_portable_runtime::keystore::{self as portable_keystore, PortableKeystoreError};
#[cfg(test)]
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::SecretString;

const SERVICE_NAME: &str = "com.oxideterm.ssh";

#[derive(Clone, Debug)]
pub(crate) struct ConnectionKeychain {
    service: String,
    #[cfg(test)]
    test_store: Option<Arc<Mutex<HashMap<String, SecretString>>>>,
}

impl Default for ConnectionKeychain {
    fn default() -> Self {
        Self {
            service: SERVICE_NAME.to_string(),
            #[cfg(test)]
            test_store: Some(Arc::new(Mutex::new(HashMap::new()))),
        }
    }
}

impl ConnectionKeychain {
    pub(crate) fn with_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            #[cfg(test)]
            test_store: Some(Arc::new(Mutex::new(HashMap::new()))),
        }
    }

    pub(crate) fn store(&self, id: &str, secret: &SecretString) -> Result<()> {
        #[cfg(test)]
        if let Some(store) = &self.test_store {
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

        let entry = self.entry(id)?;
        entry
            .set_password(secret.expose_secret())
            .with_context(|| format!("failed to store password in OS keychain for {id}"))
    }

    pub(crate) fn get(&self, id: &str) -> Result<SecretString> {
        #[cfg(test)]
        if let Some(store) = &self.test_store {
            return store
                .lock()
                .map_err(|error| anyhow::anyhow!("failed to lock test keychain: {error}"))?
                .get(id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Password not saved for this connection"));
        }

        if portable_keychain_enabled()? {
            let account = self.account(id);
            return match portable_keystore::get_secret(&self.service, &account) {
                Ok(secret) => Ok(SecretString::from(secret)),
                Err(PortableKeystoreError::NotFound(_)) => {
                    bail!("Password not saved for this connection")
                }
                Err(error) => Err(error).with_context(|| {
                    format!("failed to load password from portable keystore for {id}")
                }),
            };
        }

        let entry = self.entry(id)?;
        match entry.get_password() {
            Ok(secret) => Ok(SecretString::from(secret)),
            Err(keyring::Error::NoEntry) => bail!("Password not saved for this connection"),
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
