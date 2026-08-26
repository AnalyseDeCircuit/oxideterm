const ENCRYPTED_CONFIG_FORMAT: &str = "oxideterm.config.encrypted";
const ENCRYPTED_CONFIG_VERSION: u32 = 1;
const ENCRYPTED_CONFIG_ALGORITHM: &str = "chacha20poly1305";
const CONFIG_ENCRYPTION_KEY_LEN: usize = 32;
const CONFIG_ENCRYPTION_NONCE_LEN: usize = 12;
const CONFIG_KEYCHAIN_SERVICE: &str = "com.oxideterm.config";
const CONFIG_KEYCHAIN_ID: &str = "local-config-master-key";
const MANAGED_SSH_KEY_SECRET_DIR: &str = "managed-ssh-key-secrets";
const MANAGED_SSH_KEY_SECRET_FILE_FORMAT: &str = "oxideterm.managed-ssh-key-secret.encrypted";
const MANAGED_SSH_KEY_SECRET_FILE_VERSION: u32 = 1;
const MANAGED_SSH_KEY_SECRET_FILE_ALGORITHM: &str = "chacha20poly1305";
const MANAGED_SSH_KEY_SECRET_NONCE_LEN: usize = 12;

use std::{
    io,
    sync::{Mutex, OnceLock},
};

use chacha20poly1305::KeyInit as _;

type ConfigEncryptionKey = zeroize::Zeroizing<[u8; CONFIG_ENCRYPTION_KEY_LEN]>;
static CONFIG_ENCRYPTION_KEY_CACHE: OnceLock<Mutex<Option<ConfigEncryptionKey>>> = OnceLock::new();

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_ATOMIC_REPLACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConnectionStoreStorageFormat {
    Missing,
    Plaintext,
    Encrypted,
}

struct LoadedConnectionStoreData {
    data: ConnectionStoreData,
    format: ConnectionStoreStorageFormat,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct EncryptedConfigEnvelope {
    format: String,
    version: u32,
    algorithm: String,
    nonce: String,
    ciphertext: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct ManagedSshKeySecretEnvelope {
    format: String,
    version: u32,
    algorithm: String,
    nonce: String,
    ciphertext: String,
}

struct ManagedSshKeySecretWrite {
    created_config_key: bool,
}

fn decode_connection_store_data(bytes: &[u8]) -> Result<LoadedConnectionStoreData> {
    let document: serde_json::Value =
        serde_json::from_slice(bytes).context("failed to parse connections document")?;
    if is_encrypted_connections_document(&document) {
        let envelope: EncryptedConfigEnvelope = serde_json::from_value(document)
            .context("failed to parse encrypted connections envelope")?;
        let key = load_config_encryption_key()?.ok_or_else(|| {
            anyhow::anyhow!(
                "encrypted connections require the local config key from the OS keychain"
            )
        })?;
        let data = decrypt_connection_store_data(envelope, &*key)?;
        validate_connection_store_version(&data)?;
        return Ok(LoadedConnectionStoreData {
            data,
            format: ConnectionStoreStorageFormat::Encrypted,
        });
    }

    let data = serde_json::from_value(document).context("failed to parse plaintext connections")?;
    validate_connection_store_version(&data)?;
    Ok(LoadedConnectionStoreData {
        data,
        format: ConnectionStoreStorageFormat::Plaintext,
    })
}

fn validate_connection_store_version(data: &ConnectionStoreData) -> Result<()> {
    if data.version > CONFIG_VERSION {
        bail!(
            "connections version {} is newer than supported version {CONFIG_VERSION}",
            data.version
        );
    }
    if let Some(connection) = data
        .connections
        .iter()
        .find(|connection| connection.version > CONFIG_VERSION)
    {
        bail!(
            "connection {} uses newer version {} than supported version {CONFIG_VERSION}",
            connection.id,
            connection.version
        );
    }
    Ok(())
}

fn encode_connection_store_data(
    data: &ConnectionStoreData,
    format: ConnectionStoreStorageFormat,
) -> Result<Vec<u8>> {
    match format {
        ConnectionStoreStorageFormat::Encrypted => {
            let (key, created_key) = get_or_create_config_encryption_key()?;
            let envelope = match encrypt_connection_store_data(data, &key) {
                Ok(envelope) => envelope,
                Err(error) => {
                    if created_key {
                        rollback_created_config_key();
                    }
                    return Err(error);
                }
            };
            match serde_json::to_vec_pretty(&envelope).context("failed to serialize envelope") {
                Ok(bytes) => Ok(bytes),
                Err(error) => {
                    if created_key {
                        rollback_created_config_key();
                    }
                    Err(error)
                }
            }
        }
        ConnectionStoreStorageFormat::Missing | ConnectionStoreStorageFormat::Plaintext => {
            serde_json::to_vec_pretty(data).context("failed to serialize connections")
        }
    }
}

fn validate_managed_ssh_key_secret_id(secret_id: &str) -> Result<()> {
    let valid = !secret_id.is_empty()
        && secret_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');

    if valid {
        Ok(())
    } else {
        bail!("Invalid managed SSH key secret ID")
    }
}

fn managed_ssh_key_secret_file_path(data_dir: &Path, secret_id: &str) -> Result<PathBuf> {
    validate_managed_ssh_key_secret_id(secret_id)?;
    Ok(data_dir
        .join(MANAGED_SSH_KEY_SECRET_DIR)
        .join(format!("{secret_id}.json")))
}

fn encrypt_managed_ssh_key_secret(
    private_key: &SecretString,
    key: &[u8; CONFIG_ENCRYPTION_KEY_LEN],
) -> Result<ManagedSshKeySecretEnvelope> {
    let mut nonce = [0u8; MANAGED_SSH_KEY_SECRET_NONCE_LEN];
    let mut rng = rand::rngs::OsRng;
    rand::RngCore::fill_bytes(&mut rng, &mut nonce);

    let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(key)
        .context("failed to initialize managed SSH key secret cipher")?;
    let ciphertext = chacha20poly1305::aead::Aead::encrypt(
        &cipher,
        chacha20poly1305::Nonce::from_slice(&nonce),
        private_key.expose_secret().as_bytes(),
    )
    .map_err(|_| anyhow::anyhow!("failed to encrypt managed SSH key secret"))?;

    use base64::Engine as _;
    Ok(ManagedSshKeySecretEnvelope {
        format: MANAGED_SSH_KEY_SECRET_FILE_FORMAT.to_string(),
        version: MANAGED_SSH_KEY_SECRET_FILE_VERSION,
        algorithm: MANAGED_SSH_KEY_SECRET_FILE_ALGORITHM.to_string(),
        nonce: base64::engine::general_purpose::STANDARD.encode(nonce),
        ciphertext: base64::engine::general_purpose::STANDARD.encode(ciphertext),
    })
}

fn decrypt_managed_ssh_key_secret(
    envelope: ManagedSshKeySecretEnvelope,
    key: &[u8; CONFIG_ENCRYPTION_KEY_LEN],
) -> Result<SecretString> {
    if envelope.format != MANAGED_SSH_KEY_SECRET_FILE_FORMAT {
        bail!("invalid managed SSH key secret file format");
    }
    if envelope.version != MANAGED_SSH_KEY_SECRET_FILE_VERSION {
        bail!(
            "unsupported managed SSH key secret version {}",
            envelope.version
        );
    }
    if envelope.algorithm != MANAGED_SSH_KEY_SECRET_FILE_ALGORITHM {
        bail!(
            "unsupported managed SSH key secret algorithm {}",
            envelope.algorithm
        );
    }

    use base64::Engine as _;
    let nonce = base64::engine::general_purpose::STANDARD
        .decode(envelope.nonce)
        .context("failed to decode managed SSH key secret nonce")?;
    let nonce: [u8; MANAGED_SSH_KEY_SECRET_NONCE_LEN] = nonce
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid managed SSH key secret nonce length"))?;
    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(envelope.ciphertext)
        .context("failed to decode managed SSH key secret ciphertext")?;

    let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(key)
        .context("failed to initialize managed SSH key secret cipher")?;
    // Decrypted private-key text is zeroized after conversion into SecretString.
    let plaintext = zeroize::Zeroizing::new(
        chacha20poly1305::aead::Aead::decrypt(
            &cipher,
            chacha20poly1305::Nonce::from_slice(&nonce),
            ciphertext.as_ref(),
        )
        .map_err(|_| anyhow::anyhow!("failed to decrypt managed SSH key secret"))?,
    );
    let text = String::from_utf8(plaintext.to_vec())
        .context("managed SSH key secret is not valid UTF-8")?;
    Ok(SecretString::from(text))
}

fn write_managed_ssh_key_secret_file(
    data_dir: &Path,
    secret_id: &str,
    private_key: &SecretString,
    key: &[u8; CONFIG_ENCRYPTION_KEY_LEN],
) -> Result<()> {
    let path = managed_ssh_key_secret_file_path(data_dir, secret_id)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("invalid managed SSH key secret path"))?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    // Matches Tauri fallback behavior: large private keys are stored as local
    // ciphertext when the OS credential backend rejects long secret values.
    let envelope = encrypt_managed_ssh_key_secret(private_key, key)?;
    let bytes =
        serde_json::to_vec_pretty(&envelope).context("failed to serialize managed SSH key secret")?;
    atomic_write_file(&path, &bytes)
        .with_context(|| format!("failed to finalize {}", path.display()))
}

fn atomic_write_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    durable_write_with_before_replace(path, bytes, fail_before_atomic_replace_for_tests)
}

#[cfg(test)]
fn fail_before_atomic_replace_for_tests() -> io::Result<()> {
    FAIL_NEXT_ATOMIC_REPLACE.with(|fail| {
        if fail.replace(false) {
            Err(io::Error::other("injected failure before atomic replace"))
        } else {
            Ok(())
        }
    })
}

#[cfg(not(test))]
fn fail_before_atomic_replace_for_tests() -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
fn inject_atomic_replace_failure() {
    FAIL_NEXT_ATOMIC_REPLACE.with(|fail| fail.set(true));
}

fn read_managed_ssh_key_secret_file(
    data_dir: &Path,
    secret_id: &str,
    key: &[u8; CONFIG_ENCRYPTION_KEY_LEN],
) -> Result<SecretString> {
    let path = managed_ssh_key_secret_file_path(data_dir, secret_id)?;
    let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let envelope: ManagedSshKeySecretEnvelope =
        serde_json::from_slice(&bytes).context("failed to parse managed SSH key secret")?;
    decrypt_managed_ssh_key_secret(envelope, key)
}

fn delete_managed_ssh_key_secret_file(data_dir: &Path, secret_id: &str) -> Result<()> {
    let path = managed_ssh_key_secret_file_path(data_dir, secret_id)?;
    durable_remove(&path).with_context(|| format!("failed to delete {}", path.display()))
}

fn is_encrypted_connections_document(document: &serde_json::Value) -> bool {
    document.get("format").and_then(serde_json::Value::as_str) == Some(ENCRYPTED_CONFIG_FORMAT)
}

fn encrypt_connection_store_data(
    data: &ConnectionStoreData,
    key: &[u8; CONFIG_ENCRYPTION_KEY_LEN],
) -> Result<EncryptedConfigEnvelope> {
    // The serialized connection payload may contain secret-bearing auth state
    // before encryption; keep the buffer zeroized after the AEAD call returns.
    let plaintext = zeroize::Zeroizing::new(
        rmp_serde::to_vec_named(data).context("failed to encode connections payload")?,
    );
    let mut nonce = [0u8; CONFIG_ENCRYPTION_NONCE_LEN];
    let mut rng = rand::rngs::OsRng;
    rand::RngCore::fill_bytes(&mut rng, &mut nonce);

    let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(key)
        .context("failed to initialize connections cipher")?;
    let ciphertext = chacha20poly1305::aead::Aead::encrypt(
        &cipher,
        chacha20poly1305::Nonce::from_slice(&nonce),
        plaintext.as_ref(),
    )
    .map_err(|_| anyhow::anyhow!("failed to encrypt connections"))?;

    use base64::Engine as _;
    Ok(EncryptedConfigEnvelope {
        format: ENCRYPTED_CONFIG_FORMAT.to_string(),
        version: ENCRYPTED_CONFIG_VERSION,
        algorithm: ENCRYPTED_CONFIG_ALGORITHM.to_string(),
        nonce: base64::engine::general_purpose::STANDARD.encode(nonce),
        ciphertext: base64::engine::general_purpose::STANDARD.encode(ciphertext),
    })
}

fn decrypt_connection_store_data(
    envelope: EncryptedConfigEnvelope,
    key: &[u8; CONFIG_ENCRYPTION_KEY_LEN],
) -> Result<ConnectionStoreData> {
    if envelope.format != ENCRYPTED_CONFIG_FORMAT {
        bail!("invalid encrypted connections format");
    }
    if envelope.version != ENCRYPTED_CONFIG_VERSION {
        bail!("unsupported encrypted connections version {}", envelope.version);
    }
    if envelope.algorithm != ENCRYPTED_CONFIG_ALGORITHM {
        bail!(
            "unsupported encrypted connections algorithm {}",
            envelope.algorithm
        );
    }

    use base64::Engine as _;
    let nonce = base64::engine::general_purpose::STANDARD
        .decode(envelope.nonce)
        .context("failed to decode encrypted connections nonce")?;
    let nonce: [u8; CONFIG_ENCRYPTION_NONCE_LEN] = nonce
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid encrypted connections nonce length"))?;
    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(envelope.ciphertext)
        .context("failed to decode encrypted connections ciphertext")?;

    let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(key)
        .context("failed to initialize connections cipher")?;
    // Decrypted MessagePack is only held long enough for serde to rebuild the
    // saved connection model, then the temporary byte buffer is wiped.
    let plaintext = zeroize::Zeroizing::new(
        chacha20poly1305::aead::Aead::decrypt(
            &cipher,
            chacha20poly1305::Nonce::from_slice(&nonce),
            ciphertext.as_ref(),
        )
        .map_err(|_| anyhow::anyhow!("failed to decrypt connections"))?,
    );

    rmp_serde::from_slice(&plaintext).context("failed to decode connections payload")
}

fn load_config_encryption_key() -> Result<Option<ConfigEncryptionKey>> {
    if let Some(key) = cached_config_encryption_key() {
        return Ok(Some(key));
    }

    let secret = match load_config_key_secret()? {
        Some(secret) => secret,
        None => return Ok(None),
    };
    let key = decode_config_encryption_key(secret.as_str())?;
    #[cfg(target_os = "macos")]
    if !oxideterm_portable_runtime::is_portable_mode()
        .context("failed to determine portable mode")?
    {
        // Validate the key shape before replacing Preview 16 ACLs. This keeps a
        // malformed keychain value from becoming the durable migrated value.
        store_system_config_key_secret(secret.as_str())
            .context("failed to restore Preview 14 config key access")?;
    }
    remember_config_encryption_key(&key);
    Ok(Some(key))
}

fn get_or_create_config_encryption_key() -> Result<(ConfigEncryptionKey, bool)> {
    if let Some(key) = load_config_encryption_key()? {
        return Ok((key, false));
    }

    let mut key = zeroize::Zeroizing::new([0u8; CONFIG_ENCRYPTION_KEY_LEN]);
    let mut rng = rand::rngs::OsRng;
    rand::RngCore::fill_bytes(&mut rng, &mut key[..]);
    store_config_key_secret(&encode_config_encryption_key(&*key)?)?;
    remember_config_encryption_key(&key);
    Ok((key, true))
}

fn config_encryption_key_cache() -> &'static Mutex<Option<ConfigEncryptionKey>> {
    CONFIG_ENCRYPTION_KEY_CACHE.get_or_init(|| Mutex::new(None))
}

fn cached_config_encryption_key() -> Option<ConfigEncryptionKey> {
    config_encryption_key_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.clone())
}

fn remember_config_encryption_key(key: &ConfigEncryptionKey) {
    if let Ok(mut cache) = config_encryption_key_cache().lock() {
        // Keep the local config master key in memory only for this process so
        // repeated connection-store reads do not re-trigger OS authentication.
        *cache = Some(key.clone());
    }
}

fn clear_cached_config_encryption_key() {
    if let Ok(mut cache) = config_encryption_key_cache().lock() {
        *cache = None;
    }
}

fn decode_config_encryption_key(secret: &str) -> Result<ConfigEncryptionKey> {
    use base64::Engine as _;
    // The keychain stores the Tauri-compatible base64 form. Decode into a
    // zeroizing Vec first so the intermediate copy is wiped.
    let decoded = zeroize::Zeroizing::new(
        base64::engine::general_purpose::STANDARD
            .decode(secret)
            .context("failed to decode local config key")?,
    );
    let key: [u8; CONFIG_ENCRYPTION_KEY_LEN] = decoded
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid local config key length"))?;
    Ok(zeroize::Zeroizing::new(key))
}

fn encode_config_encryption_key(
    key: &[u8; CONFIG_ENCRYPTION_KEY_LEN],
) -> Result<zeroize::Zeroizing<String>> {
    use base64::Engine as _;
    Ok(zeroize::Zeroizing::new(
        base64::engine::general_purpose::STANDARD.encode(key),
    ))
}

fn load_config_key_secret() -> Result<Option<zeroize::Zeroizing<String>>> {
    if oxideterm_portable_runtime::is_portable_mode()
        .context("failed to determine portable mode")?
    {
        return match oxideterm_portable_runtime::keystore::get_secret(
            CONFIG_KEYCHAIN_SERVICE,
            CONFIG_KEYCHAIN_ID,
        ) {
            Ok(secret) => Ok(Some(secret)),
            Err(oxideterm_portable_runtime::keystore::PortableKeystoreError::NotFound(_)) => {
                Ok(None)
            }
            Err(error) => Err(error).context("failed to load local config key"),
        };
    }

    load_system_config_key_secret()
}

fn store_config_key_secret(secret: &str) -> Result<()> {
    // The local config key is the compatibility boundary with Tauri: OS stores
    // use username@id accounts, while portable mode stores the raw key id.
    if oxideterm_portable_runtime::is_portable_mode()
        .context("failed to determine portable mode")?
    {
        return oxideterm_portable_runtime::keystore::store_secret(
            CONFIG_KEYCHAIN_SERVICE,
            CONFIG_KEYCHAIN_ID,
            secret,
        )
        .context("failed to store local config key");
    }

    store_system_config_key_secret(secret)
}

fn rollback_created_config_key() {
    let _ = delete_config_key_secret();
}

fn delete_config_key_secret() -> Result<()> {
    let result = if oxideterm_portable_runtime::is_portable_mode()
        .context("failed to determine portable mode")?
    {
        oxideterm_portable_runtime::keystore::delete_secret(
            CONFIG_KEYCHAIN_SERVICE,
            CONFIG_KEYCHAIN_ID,
        )
        .context("failed to delete local config key")
    } else {
        delete_system_config_key_secret()
    };

    if result.is_ok() {
        clear_cached_config_encryption_key();
    }
    result
}

fn load_system_config_key_secret() -> Result<Option<zeroize::Zeroizing<String>>> {
    #[cfg(target_os = "macos")]
    oxideterm_secret_store::authenticate_device_owner(
        "OxideTerm needs to unlock your encrypted connections",
    )
    .context("failed to authenticate local config key access")?;

    oxideterm_secret_store::NativeSecretStore::new(CONFIG_KEYCHAIN_SERVICE)
        .get(&config_keychain_account())
        .context("failed to load local config key from OS keychain")
}

fn store_system_config_key_secret(secret: &str) -> Result<()> {
    oxideterm_secret_store::NativeSecretStore::new(CONFIG_KEYCHAIN_SERVICE)
        .store(&config_keychain_account(), secret)
        .context("failed to store local config key in OS keychain")
}

fn delete_system_config_key_secret() -> Result<()> {
    oxideterm_secret_store::NativeSecretStore::new(CONFIG_KEYCHAIN_SERVICE)
        .delete(&config_keychain_account())
        .context("failed to delete local config key from OS keychain")
}

fn config_keychain_account() -> String {
    format!("{}@{}", whoami::username(), CONFIG_KEYCHAIN_ID)
}

#[cfg(test)]
struct ConfigEncryptionKeyGuardForTests;

#[cfg(test)]
impl Drop for ConfigEncryptionKeyGuardForTests {
    fn drop(&mut self) {
        clear_cached_config_encryption_key();
    }
}

#[cfg(test)]
fn with_config_encryption_key_for_tests(
    key: [u8; CONFIG_ENCRYPTION_KEY_LEN],
) -> ConfigEncryptionKeyGuardForTests {
    clear_cached_config_encryption_key();
    // Tests inject the cached key to exercise encrypted fallback paths without
    // touching the real OS keychain or portable keystore.
    remember_config_encryption_key(&zeroize::Zeroizing::new(key));
    ConfigEncryptionKeyGuardForTests
}

#[cfg(test)]
mod encrypted_config_tests {
    use super::*;

    #[test]
    fn malformed_config_key_is_rejected_before_acl_migration() {
        let error = decode_config_encryption_key("c2hvcnQta2V5").unwrap_err();

        assert!(error.to_string().contains("invalid local config key length"));
    }

    #[test]
    fn config_encryption_key_cache_round_trips_and_clears() {
        clear_cached_config_encryption_key();

        let key = zeroize::Zeroizing::new([7u8; CONFIG_ENCRYPTION_KEY_LEN]);
        remember_config_encryption_key(&key);

        assert_eq!(&*cached_config_encryption_key().expect("cached key"), &*key);

        clear_cached_config_encryption_key();
        assert!(cached_config_encryption_key().is_none());
    }
}
