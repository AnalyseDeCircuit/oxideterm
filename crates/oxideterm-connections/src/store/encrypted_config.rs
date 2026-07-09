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
const MAX_ATOMIC_TEMP_ATTEMPTS: usize = 128;

use std::{
    ffi::OsString,
    fs::OpenOptions,
    io::{self, Write},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use chacha20poly1305::KeyInit as _;

type ConfigEncryptionKey = zeroize::Zeroizing<[u8; CONFIG_ENCRYPTION_KEY_LEN]>;
static CONFIG_ENCRYPTION_KEY_CACHE: OnceLock<Mutex<Option<ConfigEncryptionKey>>> = OnceLock::new();
static ATOMIC_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "atomic write path has no file name")
    })?;
    let (temp_path, mut temp_file) = (0..MAX_ATOMIC_TEMP_ATTEMPTS)
        .find_map(|_| {
            let sequence = ATOMIC_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let mut temp_name = OsString::from(".");
            temp_name.push(file_name);
            temp_name.push(format!(".{}.{sequence}.tmp", std::process::id()));
            let temp_path = parent.join(temp_name);
            match OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp_path)
            {
                Ok(file) => Some(Ok((temp_path, file))),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(error)),
            }
        })
        .transpose()?
        .ok_or_else(|| io::Error::new(io::ErrorKind::AlreadyExists, "atomic temp path exhausted"))?;

    // Secret-bearing stores commit only after the complete temp file is durable.
    let write_result = (|| {
        temp_file.write_all(bytes)?;
        temp_file.flush()?;
        temp_file.sync_all()?;
        drop(temp_file);
        fail_before_atomic_replace_for_tests()?;
        atomic_replace_file(&temp_path, path)?;
        sync_directory(parent)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> io::Result<()> {
    fs::File::open(directory)?.sync_all()
}

#[cfg(windows)]
fn sync_directory(directory: &Path) -> io::Result<()> {
    use std::{ffi::c_void, os::windows::ffi::OsStrExt};

    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const FILE_SHARE_DELETE: u32 = 0x0000_0004;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const INVALID_HANDLE_VALUE: *mut c_void = -1isize as *mut c_void;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn CreateFileW(
            file_name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *mut c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: *mut c_void,
        ) -> *mut c_void;
        fn FlushFileBuffers(file: *mut c_void) -> i32;
        fn CloseHandle(object: *mut c_void) -> i32;
    }

    let wide_path = directory
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // FILE_FLAG_BACKUP_SEMANTICS is required to open a directory handle.
    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    let flush_result = unsafe { FlushFileBuffers(handle) };
    let flush_error = (flush_result == 0).then(io::Error::last_os_error);
    let close_result = unsafe { CloseHandle(handle) };
    if let Some(error) = flush_error {
        return Err(error);
    }
    if close_result == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
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

#[cfg(not(windows))]
fn atomic_replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replaced = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
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
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to delete {}", path.display()))
        }
    }
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
    authenticate_macos_keychain_access("OxideTerm needs to unlock your encrypted connections")?;
    let entry = config_keychain_entry()?;
    match entry.get_password() {
        // Move the backend String directly into a zeroizing owner.
        Ok(secret) => Ok(Some(zeroize::Zeroizing::new(secret))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error).context("failed to load local config key from OS keychain"),
    }
}

fn store_system_config_key_secret(secret: &str) -> Result<()> {
    // keyring's apple-native backend never places the master key in process argv.
    config_keychain_entry()?
        .set_password(secret)
        .context("failed to store local config key in OS keychain")
}

fn delete_system_config_key_secret() -> Result<()> {
    match config_keychain_entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error).context("failed to delete local config key from OS keychain"),
    }
}

#[cfg(target_os = "macos")]
fn authenticate_macos_keychain_access(reason: &str) -> Result<()> {
    use objc2::{class, msg_send};
    use objc2_foundation::{NSError, NSString};
    use std::sync::mpsc;

    const LA_POLICY_DEVICE_OWNER: i64 = 2;

    unsafe {
        let cls = class!(LAContext);
        let ctx: *mut objc2::runtime::AnyObject = msg_send![cls, alloc];
        let ctx: *mut objc2::runtime::AnyObject = msg_send![ctx, init];
        if ctx.is_null() {
            return Ok(());
        }

        let mut error_ptr: *mut NSError = std::ptr::null_mut();
        let can_auth: objc2::runtime::Bool =
            msg_send![ctx, canEvaluatePolicy: LA_POLICY_DEVICE_OWNER, error: &mut error_ptr];
        if !can_auth.as_bool() {
            return Ok(());
        }

        let reason = NSString::from_str(reason);
        let (tx, rx) = mpsc::channel::<Result<()>>();
        let block = block2::RcBlock::new(move |success: objc2::runtime::Bool, error: *mut NSError| {
            if success.as_bool() {
                let _ = tx.send(Ok(()));
            } else {
                let message = if error.is_null() {
                    "macOS authentication failed".to_string()
                } else {
                    let err = &*error;
                    let description: objc2::rc::Retained<NSString> =
                        msg_send![err, localizedDescription];
                    description.to_string()
                };
                let _ = tx.send(Err(anyhow::anyhow!(message)));
            }
        });

        // This is an app-level identity gate matching Tauri's model. Reads must
        // not rewrite the keychain item because macOS treats ACL updates as a
        // separate password-protected permission change.
        let _: () = msg_send![
            ctx,
            evaluatePolicy: LA_POLICY_DEVICE_OWNER,
            localizedReason: &*reason,
            reply: &*block
        ];

        rx.recv()
            .unwrap_or_else(|_| Err(anyhow::anyhow!("macOS authentication channel closed")))
    }
}

fn config_keychain_entry() -> Result<keyring::Entry> {
    keyring::Entry::new(CONFIG_KEYCHAIN_SERVICE, &config_keychain_account())
        .context("failed to open local config keychain entry")
}

fn config_keychain_account() -> String {
    format!("{}@{}", whoami::username(), CONFIG_KEYCHAIN_ID)
}

#[cfg(test)]
fn encode_encrypted_connection_store_data_for_tests(
    data: &ConnectionStoreData,
    key: &[u8; CONFIG_ENCRYPTION_KEY_LEN],
) -> Vec<u8> {
    let envelope = encrypt_connection_store_data(data, key).expect("test envelope encrypts");
    serde_json::to_vec_pretty(&envelope).expect("test envelope serializes")
}

#[cfg(test)]
fn decode_connection_store_data_for_tests(
    bytes: &[u8],
    key: &[u8; CONFIG_ENCRYPTION_KEY_LEN],
) -> Result<LoadedConnectionStoreData> {
    let document: serde_json::Value = serde_json::from_slice(bytes)?;
    let envelope: EncryptedConfigEnvelope = serde_json::from_value(document)?;
    Ok(LoadedConnectionStoreData {
        data: decrypt_connection_store_data(envelope, key)?,
        format: ConnectionStoreStorageFormat::Encrypted,
    })
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
    fn config_encryption_key_cache_round_trips_and_clears() {
        clear_cached_config_encryption_key();

        let key = zeroize::Zeroizing::new([7u8; CONFIG_ENCRYPTION_KEY_LEN]);
        remember_config_encryption_key(&key);

        assert_eq!(&*cached_config_encryption_key().expect("cached key"), &*key);

        clear_cached_config_encryption_key();
        assert!(cached_config_encryption_key().is_none());
    }
}
