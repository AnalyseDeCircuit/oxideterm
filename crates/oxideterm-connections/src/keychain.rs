use anyhow::{Context, Result, bail};
use keyring::Entry;

const SERVICE_NAME: &str = "com.oxideterm.ssh";

#[derive(Clone, Debug)]
pub(crate) struct ConnectionKeychain {
    service: String,
}

impl Default for ConnectionKeychain {
    fn default() -> Self {
        Self {
            service: SERVICE_NAME.to_string(),
        }
    }
}

impl ConnectionKeychain {
    pub(crate) fn store(&self, id: &str, secret: &str) -> Result<()> {
        let entry = self.entry(id)?;
        entry
            .set_password(secret)
            .with_context(|| format!("failed to store password in OS keychain for {id}"))
    }

    pub(crate) fn get(&self, id: &str) -> Result<String> {
        let entry = self.entry(id)?;
        match entry.get_password() {
            Ok(secret) => Ok(secret),
            Err(keyring::Error::NoEntry) => bail!("Password not saved for this connection"),
            Err(error) => Err(error)
                .with_context(|| format!("failed to load password from OS keychain for {id}")),
        }
    }

    pub(crate) fn delete(&self, id: &str) -> Result<()> {
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
