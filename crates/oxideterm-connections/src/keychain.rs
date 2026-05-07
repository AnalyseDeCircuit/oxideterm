use anyhow::{Context, Result, bail};
use keyring::Entry;
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
    pub(crate) fn store(&self, id: &str, secret: &SecretString) -> Result<()> {
        #[cfg(test)]
        if let Some(store) = &self.test_store {
            store
                .lock()
                .map_err(|error| anyhow::anyhow!("failed to lock test keychain: {error}"))?
                .insert(id.to_string(), secret.clone());
            return Ok(());
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

        let entry = self.entry(id)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error)
                .with_context(|| format!("failed to delete password from OS keychain for {id}")),
        }
    }

    fn entry(&self, id: &str) -> Result<Entry> {
        let account = format!("{}@{}", whoami::username(), id);
        Entry::new(&self.service, &account)
            .with_context(|| format!("failed to open OS keychain entry {} for {id}", self.service))
    }
}
