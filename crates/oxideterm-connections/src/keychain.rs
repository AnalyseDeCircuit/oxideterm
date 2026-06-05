use anyhow::{Context, Result, bail};
use keyring::Entry;
use oxideterm_portable_runtime::keystore::{self as portable_keystore, PortableKeystoreError};
#[cfg(test)]
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
#[cfg(target_os = "macos")]
use zeroize::Zeroizing;

use crate::SecretString;

const SERVICE_NAME: &str = "com.oxideterm.ssh";

#[cfg(target_os = "macos")]
mod mac_keychain {
    use std::process::Command;

    use zeroize::{Zeroize, Zeroizing};

    pub(super) fn store(service: &str, account: &str, password: &str) -> Result<(), String> {
        let _ = Command::new("security")
            .args(["delete-generic-password", "-s", service, "-a", account])
            .output();

        let output = Command::new("security")
            .args([
                "add-generic-password",
                "-s",
                service,
                "-a",
                account,
                "-w",
                password,
                "-A",
            ])
            .output()
            .map_err(|error| format!("security CLI: {error}"))?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("security add-generic-password: {}", stderr.trim()))
        }
    }

    pub(super) fn get(service: &str, account: &str) -> Result<Zeroizing<String>, String> {
        let mut output = Command::new("security")
            .args(["find-generic-password", "-s", service, "-a", account, "-w"])
            .output()
            .map_err(|error| format!("security CLI: {error}"))?;

        if output.status.success() {
            // `security -w` returns the secret in stdout. Move it into a
            // zeroizing owner and wipe the process output buffer immediately.
            let secret = Zeroizing::new(
                String::from_utf8_lossy(&output.stdout)
                    .trim_end_matches('\n')
                    .to_string(),
            );
            output.stdout.zeroize();
            Ok(secret)
        } else {
            Err("not found".to_string())
        }
    }

    pub(super) fn delete(service: &str, account: &str) {
        let _ = Command::new("security")
            .args(["delete-generic-password", "-s", service, "-a", account])
            .output();
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ConnectionKeychain {
    service: String,
    #[cfg(target_os = "macos")]
    use_biometrics: bool,
    #[cfg(target_os = "macos")]
    biometric_reason: Option<String>,
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
            use_biometrics: false,
            #[cfg(target_os = "macos")]
            biometric_reason: None,
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
            use_biometrics: false,
            #[cfg(target_os = "macos")]
            biometric_reason: None,
            #[cfg(test)]
            test_store: Some(Arc::new(Mutex::new(HashMap::new()))),
            #[cfg(test)]
            test_max_secret_bytes: None,
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn with_macos_biometrics_reason(
        service: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            service: service.into(),
            use_biometrics: true,
            biometric_reason: Some(reason.into()),
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
            use_biometrics: false,
            #[cfg(target_os = "macos")]
            biometric_reason: None,
            test_store: Some(Arc::new(Mutex::new(HashMap::new()))),
            test_max_secret_bytes: Some(max_secret_bytes),
        }
    }

    #[cfg(target_os = "macos")]
    fn biometric_reason(&self) -> &str {
        self.biometric_reason
            .as_deref()
            .unwrap_or("OxideTerm needs to access your stored secrets")
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
                bail!("test keychain secret exceeds configured byte limit");
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
        if self.use_biometrics {
            let account = self.account(id);
            // Match Tauri's biometric keychain shape: LocalAuthentication
            // gates reads, while `security -A` avoids per-binary ACL prompts
            // when preview/dev builds write the item.
            return mac_keychain::store(&self.service, &account, secret.expose_secret())
                .map_err(anyhow::Error::msg)
                .with_context(|| format!("failed to store password in OS keychain for {id}"));
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

        #[cfg(target_os = "macos")]
        if self.use_biometrics {
            if crate::touch_id::is_biometric_available() {
                crate::touch_id::authenticate(self.biometric_reason())
                    .map_err(anyhow::Error::msg)
                    .with_context(|| format!("failed to authenticate keychain access for {id}"))?;
            }

            let account = self.account(id);
            if let Ok(secret) = mac_keychain::get(&self.service, &account) {
                return Ok(SecretString::from(secret));
            }

            let entry = self.entry(id)?;
            return match entry.get_password() {
                Ok(secret) => {
                    // Older native builds may have written this item via
                    // keyring's restrictive macOS ACL. After the one-time
                    // fallback read succeeds, migrate it to the Tauri-style
                    // biometric-compatible storage shape.
                    let secret = Zeroizing::new(secret);
                    let _ = mac_keychain::store(&self.service, &account, secret.as_str());
                    Ok(SecretString::from(secret))
                }
                Err(keyring::Error::NoEntry) => bail!("Password not saved for this connection"),
                Err(error) => Err(error)
                    .with_context(|| format!("failed to load password from OS keychain for {id}")),
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

        #[cfg(target_os = "macos")]
        if self.use_biometrics {
            let account = self.account(id);
            mac_keychain::delete(&self.service, &account);
            let entry = self.entry(id)?;
            let _ = entry.delete_credential();
            return Ok(());
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
