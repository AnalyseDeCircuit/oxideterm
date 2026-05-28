const ENCRYPTED_CONFIG_FORMAT: &str = "oxideterm.config.encrypted";
const ENCRYPTED_CONFIG_VERSION: u32 = 1;
const ENCRYPTED_CONFIG_ALGORITHM: &str = "chacha20poly1305";
const CONFIG_ENCRYPTION_KEY_LEN: usize = 32;
const CONFIG_ENCRYPTION_NONCE_LEN: usize = 12;
const CONFIG_KEYCHAIN_SERVICE: &str = "com.oxideterm.config";
const CONFIG_KEYCHAIN_ID: &str = "local-config-master-key";
#[cfg(target_os = "macos")]
const MACOS_KEYCHAIN_COMMAND_TIMEOUT_SECS: u64 = 30;

use chacha20poly1305::KeyInit as _;

type ConfigEncryptionKey = zeroize::Zeroizing<[u8; CONFIG_ENCRYPTION_KEY_LEN]>;

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
        return Ok(LoadedConnectionStoreData {
            data,
            format: ConnectionStoreStorageFormat::Encrypted,
        });
    }

    Ok(LoadedConnectionStoreData {
        data: serde_json::from_value(document).context("failed to parse plaintext connections")?,
        format: ConnectionStoreStorageFormat::Plaintext,
    })
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
    let secret = match load_config_key_secret()? {
        Some(secret) => secret,
        None => return Ok(None),
    };
    Ok(Some(decode_config_encryption_key(secret.as_str())?))
}

fn get_or_create_config_encryption_key() -> Result<(ConfigEncryptionKey, bool)> {
    if let Some(key) = load_config_encryption_key()? {
        return Ok((key, false));
    }

    let mut key = zeroize::Zeroizing::new([0u8; CONFIG_ENCRYPTION_KEY_LEN]);
    let mut rng = rand::rngs::OsRng;
    rand::RngCore::fill_bytes(&mut rng, &mut key[..]);
    store_config_key_secret(&encode_config_encryption_key(&*key)?)?;
    Ok((key, true))
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
    if oxideterm_portable_runtime::is_portable_mode()
        .context("failed to determine portable mode")?
    {
        return oxideterm_portable_runtime::keystore::delete_secret(
            CONFIG_KEYCHAIN_SERVICE,
            CONFIG_KEYCHAIN_ID,
        )
        .context("failed to delete local config key");
    }

    delete_system_config_key_secret()
}

#[cfg(target_os = "macos")]
fn load_system_config_key_secret() -> Result<Option<zeroize::Zeroizing<String>>> {
    authenticate_macos_keychain_access("OxideTerm needs to unlock your encrypted connections")?;
    // Tauri stores the local config key in the macOS generic password store.
    // Calling `security` directly avoids keyring backends that can block a
    // headless CLI while preserving the same service/account lookup.
    let output = run_macos_security_command(
        vec![
            "find-generic-password".to_string(),
            "-s".to_string(),
            CONFIG_KEYCHAIN_SERVICE.to_string(),
            "-a".to_string(),
            config_keychain_account(),
            "-w".to_string(),
        ],
        "lookup local config key",
    )?;
    if output.status.success() {
        let secret = String::from_utf8(output.stdout)
            .context("local config key from macOS keychain is not UTF-8")?;
        let secret = zeroize::Zeroizing::new(secret.trim_end_matches(['\r', '\n']).to_string());
        let _ = store_system_config_key_secret(secret.as_str());
        return Ok(Some(secret));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("could not be found") {
        return Ok(None);
    }
    Err(anyhow::anyhow!(
        "failed to load local config key from macOS keychain: {}",
        stderr.trim()
    ))
}

#[cfg(not(target_os = "macos"))]
fn load_system_config_key_secret() -> Result<Option<zeroize::Zeroizing<String>>> {
    let entry = config_keychain_entry()?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(zeroize::Zeroizing::new(secret))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error).context("failed to load local config key from OS keychain"),
    }
}

#[cfg(target_os = "macos")]
fn store_system_config_key_secret(secret: &str) -> Result<()> {
    let output = run_macos_security_command(
        vec![
            "add-generic-password".to_string(),
            "-U".to_string(),
            "-s".to_string(),
            CONFIG_KEYCHAIN_SERVICE.to_string(),
            "-a".to_string(),
            config_keychain_account(),
            "-w".to_string(),
            secret.to_string(),
            "-A".to_string(),
        ],
        "store local config key",
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "failed to store local config key in macOS keychain"
        ))
    }
}

#[cfg(not(target_os = "macos"))]
fn store_system_config_key_secret(secret: &str) -> Result<()> {
    config_keychain_entry()?
        .set_password(secret)
        .context("failed to store local config key in OS keychain")
}

#[cfg(target_os = "macos")]
fn delete_system_config_key_secret() -> Result<()> {
    let output = run_macos_security_command(
        vec![
            "delete-generic-password".to_string(),
            "-s".to_string(),
            CONFIG_KEYCHAIN_SERVICE.to_string(),
            "-a".to_string(),
            config_keychain_account(),
        ],
        "delete local config key",
    )?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("could not be found") {
        return Ok(());
    }
    Err(anyhow::anyhow!(
        "failed to delete local config key from macOS keychain: {}",
        stderr.trim()
    ))
}

#[cfg(target_os = "macos")]
fn run_macos_security_command(
    args: Vec<String>,
    action: &str,
) -> Result<std::process::Output> {
    let mut child = std::process::Command::new("security")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to run macOS security command to {action}"))?;
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(MACOS_KEYCHAIN_COMMAND_TIMEOUT_SECS);
    loop {
        if child
            .try_wait()
            .with_context(|| format!("failed to poll macOS security command to {action}"))?
            .is_some()
        {
            return child
                .wait_with_output()
                .with_context(|| format!("failed to collect macOS security output to {action}"));
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!(
                "macOS keychain authorization timed out while trying to {action}; approve the keychain prompt and retry"
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
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

        // This is an app-level identity gate matching Tauri's model. The
        // keychain item itself is migrated to `security -A` after a successful
        // read so future access does not depend on per-binary ACL prompts.
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

#[cfg(not(target_os = "macos"))]
fn delete_system_config_key_secret() -> Result<()> {
    match config_keychain_entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error).context("failed to delete local config key from OS keychain"),
    }
}

#[cfg(not(target_os = "macos"))]
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
