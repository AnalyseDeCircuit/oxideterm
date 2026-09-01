#[cfg(not(target_os = "macos"))]
use anyhow::Context;
use anyhow::Result;
#[cfg(not(target_os = "macos"))]
use keyring::Entry;
#[cfg(target_os = "windows")]
use uuid::Uuid;
use zeroize::Zeroizing;

#[cfg(target_os = "windows")]
// Windows stores passwords as UTF-16 in a 2,560-byte credential blob.
// A 1 KiB UTF-8 chunk remains below that limit for every Unicode scalar.
const WINDOWS_SECRET_CHUNK_BYTES: usize = 1_024;
#[cfg(target_os = "windows")]
const WINDOWS_CHUNK_SERVICE_SUFFIX: &str = ".oxideterm-secret-chunks-v1";
#[cfg(target_os = "windows")]
const WINDOWS_CHUNK_MANIFEST_VERSION: &str = "v1";

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod macos_auth;

#[cfg(target_os = "macos")]
pub use macos_auth::authenticate_device_owner;

/// Stores application secrets in the platform credential manager.
#[derive(Clone, Debug)]
pub struct NativeSecretStore {
    service: String,
}

impl NativeSecretStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    pub fn store(&self, account: &str, secret: &str) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            return macos::store(&self.service, account, secret);
        }

        #[cfg(target_os = "windows")]
        {
            return self.store_windows(account, secret);
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        self.entry(account)?
            .set_password(secret)
            .context("failed to store secret in the OS credential manager")
    }

    pub fn get(&self, account: &str) -> Result<Option<Zeroizing<String>>> {
        #[cfg(target_os = "macos")]
        {
            return macos::get(&self.service, account);
        }

        #[cfg(target_os = "windows")]
        {
            return self.get_windows(account);
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        match self.entry(account)?.get_password() {
            Ok(secret) => Ok(Some(Zeroizing::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => {
                Err(error).context("failed to load secret from the OS credential manager")
            }
        }
    }

    /// Loads a secret through the macOS backend that preserves multiline content exactly.
    ///
    /// This mode can request application-specific Keychain authorization, so only domains
    /// whose secret format permits newlines should select it.
    pub fn get_preserving_multiline(&self, account: &str) -> Result<Option<Zeroizing<String>>> {
        #[cfg(target_os = "macos")]
        {
            return macos::get_preserving_multiline(&self.service, account);
        }

        #[cfg(not(target_os = "macos"))]
        self.get(account)
    }

    pub fn delete(&self, account: &str) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            return macos::delete(&self.service, account);
        }

        #[cfg(target_os = "windows")]
        {
            return self.delete_windows(account);
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => {
                Err(error).context("failed to delete secret from the OS credential manager")
            }
        }
    }

    pub fn exists(&self, account: &str) -> Result<bool> {
        #[cfg(target_os = "macos")]
        {
            return macos::exists(&self.service, account);
        }

        #[cfg(target_os = "windows")]
        {
            return match self.windows_chunk_manifest_state(account)? {
                WindowsChunkManifestState::Valid(_) => Ok(true),
                WindowsChunkManifestState::Missing => self.windows_direct_secret_exists(account),
                WindowsChunkManifestState::Invalid => {
                    if self.windows_direct_secret_exists(account)? {
                        Ok(true)
                    } else {
                        anyhow::bail!("invalid secret metadata in the OS credential manager")
                    }
                }
            };
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        Ok(self.get(account)?.is_some())
    }

    #[cfg(target_os = "windows")]
    fn windows_direct_secret_exists(&self, account: &str) -> Result<bool> {
        let entry = self.entry(account)?;
        if let Some(credential) = entry
            .get_credential()
            .downcast_ref::<keyring::windows::WinCredential>()
        {
            // Windows Credential Manager supports a metadata-only lookup.
            return match credential.get_credential() {
                Ok(_) => Ok(true),
                Err(keyring::Error::NoEntry) => Ok(false),
                Err(error) => Err(error).context("failed to inspect the OS credential manager"),
            };
        }
        Ok(self.get_windows_direct(account)?.is_some())
    }

    #[cfg(target_os = "windows")]
    fn get_windows_direct(&self, account: &str) -> Result<Option<Zeroizing<String>>> {
        match self.entry(account)?.get_password() {
            Ok(secret) => Ok(Some(Zeroizing::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => {
                Err(error).context("failed to load secret from the OS credential manager")
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn entry(&self, account: &str) -> Result<Entry> {
        Entry::new(&self.service, account).context("failed to open an OS credential manager entry")
    }

    #[cfg(target_os = "windows")]
    fn store_windows(&self, account: &str, secret: &str) -> Result<()> {
        match self.entry(account)?.set_password(secret) {
            Ok(()) => {
                self.delete_windows_chunks(account)?;
                Ok(())
            }
            Err(keyring::Error::TooLong(field, _))
                if field == "password encoded as UTF-16" || field == "secret" =>
            {
                self.store_windows_chunks(account, secret)
            }
            Err(error) => Err(error).context("failed to store secret in the OS credential manager"),
        }
    }

    #[cfg(target_os = "windows")]
    fn get_windows(&self, account: &str) -> Result<Option<Zeroizing<String>>> {
        let manifest = match self.windows_chunk_manifest_state(account)? {
            WindowsChunkManifestState::Missing => return self.get_windows_direct(account),
            WindowsChunkManifestState::Valid(manifest) => manifest,
            WindowsChunkManifestState::Invalid => {
                // A malformed manifest cannot identify usable chunks. Preserve
                // a valid direct credential instead of letting metadata poison it.
                if let Some(secret) = self.get_windows_direct(account)? {
                    return Ok(Some(secret));
                }
                anyhow::bail!("invalid secret metadata in the OS credential manager");
            }
        };

        let mut secret = Zeroizing::new(String::new());
        for index in 0..manifest.chunk_count {
            let chunk_account = windows_chunk_account(account, &manifest.generation, index);
            let chunk = match self.windows_chunk_entry(&chunk_account)?.get_password() {
                Ok(chunk) => Zeroizing::new(chunk),
                Err(keyring::Error::NoEntry) => {
                    anyhow::bail!("stored secret is incomplete in the OS credential manager")
                }
                Err(error) => {
                    return Err(error)
                        .context("failed to load a secret chunk from the OS credential manager");
                }
            };
            secret.push_str(chunk.as_str());
        }
        Ok(Some(secret))
    }

    #[cfg(target_os = "windows")]
    fn delete_windows(&self, account: &str) -> Result<()> {
        self.delete_windows_chunks(account)?;
        delete_entry_if_present(self.entry(account)?)
            .context("failed to delete secret from the OS credential manager")
    }

    #[cfg(target_os = "windows")]
    fn store_windows_chunks(&self, account: &str, secret: &str) -> Result<()> {
        let previous_manifest = match self.windows_chunk_manifest_state(account)? {
            WindowsChunkManifestState::Valid(manifest) => Some(manifest),
            WindowsChunkManifestState::Missing | WindowsChunkManifestState::Invalid => None,
        };
        let generation = Uuid::new_v4().simple().to_string();
        let chunks = windows_secret_chunks(secret);

        for (index, chunk) in chunks.iter().enumerate() {
            let chunk_account = windows_chunk_account(account, &generation, index);
            if let Err(error) = self
                .windows_chunk_entry(&chunk_account)?
                .set_password(chunk.as_str())
            {
                let _ = self.delete_windows_chunk_generation(account, &generation, chunks.len());
                return Err(error)
                    .context("failed to store a secret chunk in the OS credential manager");
            }
        }

        let manifest = WindowsChunkManifest {
            generation: generation.clone(),
            chunk_count: chunks.len(),
        };
        if let Err(error) = self
            .windows_chunk_entry(account)?
            .set_password(&manifest.encode())
        {
            let _ = self.delete_windows_chunk_generation(account, &generation, chunks.len());
            return Err(error)
                .context("failed to store secret metadata in the OS credential manager");
        }

        // The manifest is committed last, so readers never observe a partial
        // generation. Old chunks are removed only after the new value is live.
        delete_entry_if_present(self.entry(account)?)
            .context("failed to replace the previous OS credential manager secret")?;
        if let Some(previous) = previous_manifest {
            self.delete_windows_chunk_generation(
                account,
                &previous.generation,
                previous.chunk_count,
            )?;
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn delete_windows_chunks(&self, account: &str) -> Result<()> {
        let manifest = match self.windows_chunk_manifest_state(account)? {
            WindowsChunkManifestState::Missing => return Ok(()),
            WindowsChunkManifestState::Valid(manifest) => Some(manifest),
            WindowsChunkManifestState::Invalid => None,
        };
        // Remove the manifest even when it is malformed so a bad OxideTerm-only
        // metadata entry cannot permanently block replacement or deletion.
        delete_entry_if_present(self.windows_chunk_entry(account)?)
            .context("failed to delete secret metadata from the OS credential manager")?;
        if let Some(manifest) = manifest {
            self.delete_windows_chunk_generation(
                account,
                &manifest.generation,
                manifest.chunk_count,
            )?;
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn delete_windows_chunk_generation(
        &self,
        account: &str,
        generation: &str,
        chunk_count: usize,
    ) -> Result<()> {
        let mut first_error = None;
        for index in 0..chunk_count {
            let chunk_account = windows_chunk_account(account, generation, index);
            if let Err(error) = self
                .windows_chunk_entry(&chunk_account)
                .and_then(delete_entry_if_present)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        if let Some(error) = first_error {
            return Err(error)
                .context("failed to delete a secret chunk from the OS credential manager");
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn windows_chunk_manifest_state(&self, account: &str) -> Result<WindowsChunkManifestState> {
        match self.windows_chunk_entry(account)?.get_password() {
            Ok(value) => {
                let value = Zeroizing::new(value);
                Ok(match WindowsChunkManifest::decode(value.as_str()) {
                    Ok(manifest) => WindowsChunkManifestState::Valid(manifest),
                    Err(_) => WindowsChunkManifestState::Invalid,
                })
            }
            Err(keyring::Error::NoEntry) => Ok(WindowsChunkManifestState::Missing),
            Err(error) => {
                Err(error).context("failed to load secret metadata from the OS credential manager")
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn windows_chunk_entry(&self, account: &str) -> Result<Entry> {
        let service = format!("{}{WINDOWS_CHUNK_SERVICE_SUFFIX}", self.service);
        Entry::new(&service, account)
            .context("failed to open a secret chunk in the OS credential manager")
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
enum WindowsChunkManifestState {
    Missing,
    Valid(WindowsChunkManifest),
    Invalid,
}

#[cfg(target_os = "windows")]
#[derive(Clone, Debug)]
struct WindowsChunkManifest {
    generation: String,
    chunk_count: usize,
}

#[cfg(target_os = "windows")]
impl WindowsChunkManifest {
    fn encode(&self) -> String {
        format!(
            "{WINDOWS_CHUNK_MANIFEST_VERSION}:{}:{}",
            self.generation, self.chunk_count
        )
    }

    fn decode(value: &str) -> Result<Self> {
        let mut fields = value.split(':');
        let version = fields.next();
        let generation = fields.next();
        let chunk_count = fields.next();
        if version != Some(WINDOWS_CHUNK_MANIFEST_VERSION) || fields.next().is_some() {
            anyhow::bail!("invalid secret metadata in the OS credential manager");
        }
        let generation = generation
            .and_then(|value| Uuid::parse_str(value).ok())
            .map(|value| value.simple().to_string())
            .ok_or_else(|| anyhow::anyhow!("invalid secret metadata generation"))?;
        let chunk_count = chunk_count
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|count| *count > 0)
            .ok_or_else(|| anyhow::anyhow!("invalid secret metadata chunk count"))?;
        Ok(Self {
            generation,
            chunk_count,
        })
    }
}

#[cfg(target_os = "windows")]
fn windows_secret_chunks(secret: &str) -> Vec<Zeroizing<String>> {
    let mut chunks = Vec::new();
    let mut chunk = String::with_capacity(WINDOWS_SECRET_CHUNK_BYTES);
    for character in secret.chars() {
        if !chunk.is_empty()
            && chunk.len().saturating_add(character.len_utf8()) > WINDOWS_SECRET_CHUNK_BYTES
        {
            chunks.push(Zeroizing::new(std::mem::take(&mut chunk)));
        }
        chunk.push(character);
    }
    if !chunk.is_empty() {
        chunks.push(Zeroizing::new(chunk));
    }
    chunks
}

#[cfg(target_os = "windows")]
fn windows_chunk_account(account: &str, generation: &str, index: usize) -> String {
    format!("{account}:{generation}:{index}")
}

#[cfg(target_os = "windows")]
fn delete_entry_if_present(entry: Entry) -> Result<()> {
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.into()),
    }
}
