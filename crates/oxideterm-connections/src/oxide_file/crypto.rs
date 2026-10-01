use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce, aead::Aead};
use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{ConnectionOptions, ConnectionTerminalOptions, ConnectionX11ForwardingOptions};

use super::{
    EncryptedAuth, EncryptedConnection, EncryptedForward, EncryptedPayload,
    EncryptedPrivilegeCredential, EncryptedProxyHop, EncryptedUpstreamProxyPolicy, NONCE_LEN,
    OxideFile, OxideFileError, OxideMetadata, SALT_LEN, TAG_LEN, kdf_flags,
};

struct KdfParams {
    memory_cost: u32,
    iterations: u32,
    parallelism: u32,
}

/// Owns one password-derived key for a batch of independently authenticated
/// `.oxide` files. Every file still receives a fresh nonce, while the expensive
/// Argon2id derivation is paid only once for the batch.
pub struct OxideBatchEncryptionContext {
    pub(super) salt: [u8; SALT_LEN],
    pub(super) key: Zeroizing<[u8; 32]>,
    pub(super) kdf_version: u32,
}

struct CachedOxideDecryptionKey {
    salt: [u8; SALT_LEN],
    key: Zeroizing<[u8; 32]>,
    kdf_version: u32,
}

/// Reuses a zeroizing derived key while consecutive `.oxide` files share the
/// same salt and KDF version. Older batches with per-file salts remain readable
/// and simply derive a replacement key when the salt changes.
pub struct OxideBatchDecryptionContext {
    password: Zeroizing<String>,
    cached: Option<CachedOxideDecryptionKey>,
}

impl OxideBatchEncryptionContext {
    pub fn new(password: &str) -> Result<Self, OxideFileError> {
        if password.len() < 6 {
            return Err(OxideFileError::PasswordTooShort);
        }
        let mut salt = [0u8; SALT_LEN];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        let kdf_version = kdf_flags::CURRENT_KDF;
        let key = derive_key(password, &salt, kdf_version)?;
        Ok(Self {
            salt,
            key,
            kdf_version,
        })
    }
}

impl OxideBatchDecryptionContext {
    pub fn new(password: &str) -> Result<Self, OxideFileError> {
        if password.len() < 6 {
            return Err(OxideFileError::PasswordTooShort);
        }
        Ok(Self {
            password: Zeroizing::new(password.to_string()),
            cached: None,
        })
    }

    pub(super) fn key_for(
        &mut self,
        salt: [u8; SALT_LEN],
        kdf_version: u32,
    ) -> Result<&[u8; 32], OxideFileError> {
        if !self
            .cached
            .as_ref()
            .is_some_and(|cached| cached.salt == salt && cached.kdf_version == kdf_version)
        {
            self.cached = Some(CachedOxideDecryptionKey {
                key: derive_key(self.password.as_str(), &salt, kdf_version)?,
                salt,
                kdf_version,
            });
        }
        Ok(&self.cached.as_ref().ok_or(OxideFileError::CryptoError)?.key)
    }
}

impl KdfParams {
    fn for_version(version: u32) -> Result<Self, OxideFileError> {
        match version {
            kdf_flags::KDF_V1 | 0 => Ok(Self {
                memory_cost: 262_144,
                iterations: 4,
                parallelism: 4,
            }),
            kdf_flags::KDF_V2 => Ok(Self {
                memory_cost: 524_288,
                iterations: 6,
                parallelism: 4,
            }),
            other => Err(OxideFileError::UnsupportedKdfVersion(other)),
        }
    }
}

pub fn derive_key(
    password: &str,
    salt: &[u8],
    kdf_version: u32,
) -> Result<Zeroizing<[u8; 32]>, OxideFileError> {
    let kdf = KdfParams::for_version(kdf_version)?;
    let params = Params::new(kdf.memory_cost, kdf.iterations, kdf.parallelism, Some(32))
        .map_err(|_| OxideFileError::CryptoError)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut *key)
        .map_err(|_| OxideFileError::CryptoError)?;
    Ok(key)
}

pub fn encrypt_oxide_file(
    payload: &EncryptedPayload,
    password: &str,
    metadata: OxideMetadata,
) -> Result<OxideFile, OxideFileError> {
    encrypt_oxide_file_with_progress(payload, password, metadata, |_| {})
}

pub fn encrypt_oxide_file_with_progress<F>(
    payload: &EncryptedPayload,
    password: &str,
    metadata: OxideMetadata,
    mut on_progress: F,
) -> Result<OxideFile, OxideFileError>
where
    F: FnMut(&'static str),
{
    let mut salt = [0u8; SALT_LEN];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    on_progress("generating_salt_nonce");

    let key = derive_key(password, &salt, kdf_flags::CURRENT_KDF)?;
    on_progress("deriving_key");

    encrypt_oxide_payload(
        payload,
        metadata,
        salt,
        &key,
        kdf_flags::CURRENT_KDF,
        on_progress,
    )
}

pub fn encrypt_oxide_file_with_context_and_progress<F>(
    payload: &EncryptedPayload,
    context: &OxideBatchEncryptionContext,
    metadata: OxideMetadata,
    mut on_progress: F,
) -> Result<OxideFile, OxideFileError>
where
    F: FnMut(&'static str),
{
    on_progress("generating_nonce");
    on_progress("reusing_derived_key");
    encrypt_oxide_payload(
        payload,
        metadata,
        context.salt,
        &context.key,
        context.kdf_version,
        on_progress,
    )
}

fn encrypt_oxide_payload<F>(
    payload: &EncryptedPayload,
    metadata: OxideMetadata,
    salt: [u8; SALT_LEN],
    key: &[u8; 32],
    kdf_version: u32,
    mut on_progress: F,
) -> Result<OxideFile, OxideFileError>
where
    F: FnMut(&'static str),
{
    let plaintext = Zeroizing::new(rmp_serde::to_vec_named(&(&metadata, payload))?);
    on_progress("serializing_payload");
    let archive = super::container::seal(
        super::OxideDocumentKind::Archive,
        &plaintext,
        key,
        salt,
        kdf_version,
    )?;
    let nonce = archive[21 + SALT_LEN..21 + SALT_LEN + NONCE_LEN]
        .try_into()
        .map_err(|_| OxideFileError::CryptoError)?;
    on_progress("encrypting_payload");
    on_progress("finalizing_file");
    Ok(OxideFile {
        metadata: Some(metadata),
        archive: Some(archive),
        salt,
        nonce,
        encrypted_data: Vec::new(),
        tag: [0; TAG_LEN],
        kdf_version,
    })
}

pub fn decrypt_oxide_file(
    oxide_file: &OxideFile,
    password: &str,
) -> Result<EncryptedPayload, OxideFileError> {
    decrypt_oxide_file_with_progress(oxide_file, password, |_| {})
}

pub fn decrypt_oxide_file_with_progress<F>(
    oxide_file: &OxideFile,
    password: &str,
    mut on_progress: F,
) -> Result<EncryptedPayload, OxideFileError>
where
    F: FnMut(&'static str),
{
    let key = derive_key(password, &oxide_file.salt, oxide_file.kdf_version)?;
    on_progress("deriving_key");

    decrypt_oxide_payload(oxide_file, &key, on_progress).map(|(_, payload)| payload)
}

pub fn decrypt_oxide_file_with_context_and_progress<F>(
    oxide_file: &OxideFile,
    context: &mut OxideBatchDecryptionContext,
    on_progress: F,
) -> Result<EncryptedPayload, OxideFileError>
where
    F: FnMut(&'static str),
{
    decrypt_oxide_archive_with_context_and_progress(oxide_file, context, on_progress)
        .map(|(_, payload)| payload)
}

pub fn decrypt_oxide_archive_with_context_and_progress<F>(
    oxide_file: &OxideFile,
    context: &mut OxideBatchDecryptionContext,
    mut on_progress: F,
) -> Result<(OxideMetadata, EncryptedPayload), OxideFileError>
where
    F: FnMut(&'static str),
{
    let matches_cached = context.cached.as_ref().is_some_and(|cached| {
        cached.salt == oxide_file.salt && cached.kdf_version == oxide_file.kdf_version
    });
    if matches_cached {
        on_progress("reusing_derived_key");
    } else {
        let key = derive_key(
            context.password.as_str(),
            &oxide_file.salt,
            oxide_file.kdf_version,
        )?;
        context.cached = Some(CachedOxideDecryptionKey {
            salt: oxide_file.salt,
            key,
            kdf_version: oxide_file.kdf_version,
        });
        on_progress("deriving_key");
    }
    let key = &context
        .cached
        .as_ref()
        .ok_or(OxideFileError::CryptoError)?
        .key;
    decrypt_oxide_payload(oxide_file, key, on_progress)
}

fn decrypt_oxide_payload<F>(
    oxide_file: &OxideFile,
    key: &[u8; 32],
    mut on_progress: F,
) -> Result<(OxideMetadata, EncryptedPayload), OxideFileError>
where
    F: FnMut(&'static str),
{
    if let Some(bytes) = &oxide_file.archive {
        let plaintext = super::container::open(bytes, super::OxideDocumentKind::Archive, key)?;
        on_progress("decrypting_payload");
        let (metadata, payload): (OxideMetadata, EncryptedPayload) =
            rmp_serde::from_slice(&plaintext)?;
        on_progress("deserializing_payload");
        verify_checksum(&payload)?;
        on_progress("verifying_checksum");
        return Ok((metadata, payload));
    }
    let cipher = ChaCha20Poly1305::new_from_slice(key).map_err(|_| OxideFileError::CryptoError)?;
    let mut ciphertext_with_tag = oxide_file.encrypted_data.clone();
    ciphertext_with_tag.extend_from_slice(&oxide_file.tag);

    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                Nonce::from_slice(&oxide_file.nonce),
                ciphertext_with_tag.as_ref(),
            )
            .map_err(|_| OxideFileError::DecryptionFailed)?,
    );
    on_progress("decrypting_payload");

    let payload: EncryptedPayload = rmp_serde::from_slice(&plaintext)?;
    on_progress("deserializing_payload");
    verify_checksum(&payload)?;
    on_progress("verifying_checksum");
    Ok((
        oxide_file
            .metadata
            .clone()
            .ok_or(OxideFileError::CryptoError)?,
        payload,
    ))
}

pub fn compute_checksum(payload: &EncryptedPayload) -> Result<String, OxideFileError> {
    compute_checksum_with_connection_serializer(payload, |connection| {
        rmp_serde::to_vec_named(connection).map(Zeroizing::new)
    })
}

fn compute_checksum_with_connection_serializer<F>(
    payload: &EncryptedPayload,
    mut serialize_connection: F,
) -> Result<String, OxideFileError>
where
    F: FnMut(&EncryptedConnection) -> Result<Zeroizing<Vec<u8>>, rmp_serde::encode::Error>,
{
    // Connection archives can contain authentication material, so every
    // serializer supplied here must return a buffer that is wiped on drop.
    if payload.version <= 1
        && payload.app_settings_json.is_none()
        && payload.plugin_settings.is_empty()
        && payload.portable_secrets.is_empty()
    {
        let mut hasher = Sha256::new();
        for connection in &payload.connections {
            hasher.update(serialize_connection(connection)?.as_slice());
        }
        return Ok(format!("sha256:{:x}", hasher.finalize()));
    }

    let mut hasher = Sha256::new();
    hasher.update(payload.version.to_le_bytes());
    hasher.update((payload.connections.len() as u64).to_le_bytes());
    for connection in &payload.connections {
        hasher.update(serialize_connection(connection)?.as_slice());
    }

    match &payload.app_settings_json {
        Some(json) => {
            hasher.update([1]);
            hasher.update(json.as_bytes());
        }
        None => hasher.update([0]),
    }

    hasher.update((payload.plugin_settings.len() as u64).to_le_bytes());
    for plugin_setting in &payload.plugin_settings {
        let encoded = Zeroizing::new(rmp_serde::to_vec_named(plugin_setting)?);
        hasher.update(encoded.as_slice());
    }

    hasher.update((payload.portable_secrets.len() as u64).to_le_bytes());
    for portable_secret in &payload.portable_secrets {
        let encoded = Zeroizing::new(rmp_serde::to_vec_named(portable_secret)?);
        hasher.update(encoded.as_slice());
    }

    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn compute_checksum_before_ssh_algorithms(
    payload: &EncryptedPayload,
) -> Result<String, OxideFileError> {
    compute_checksum_with_connection_serializer(payload, |connection| {
        rmp_serde::to_vec_named(&ConnectionBeforeSshAlgorithms::from(connection))
            .map(Zeroizing::new)
    })
}

fn verify_checksum(payload: &EncryptedPayload) -> Result<(), OxideFileError> {
    if compute_checksum(payload)? == payload.checksum {
        return Ok(());
    }

    let has_only_default_ssh_algorithm_preferences = payload
        .connections
        .iter()
        .all(|connection| connection.options.ssh_algorithms.is_default());
    // Archives written before SSH algorithm preferences existed authenticate
    // the same connection data without that later defaulted field.
    if has_only_default_ssh_algorithm_preferences
        && compute_checksum_before_ssh_algorithms(payload)? == payload.checksum
    {
        return Ok(());
    }

    Err(OxideFileError::ChecksumMismatch)
}

/// Reproduces the named MessagePack shape used before SSH algorithm preferences
/// were added to `ConnectionOptions`.
#[derive(Serialize)]
struct ConnectionBeforeSshAlgorithms<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    source_connection_id: Option<&'a str>,
    name: &'a str,
    group: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<&'a str>,
    host: &'a str,
    port: u16,
    username: &'a str,
    auth: &'a EncryptedAuth,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon_background_color: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<&'a str>,
    tags: &'a [String],
    options: ConnectionOptionsBeforeSshAlgorithms<'a>,
    #[serde(skip_serializing_if = "encrypted_upstream_proxy_is_global")]
    upstream_proxy: &'a EncryptedUpstreamProxyPolicy,
    #[serde(skip_serializing_if = "slice_is_empty")]
    proxy_chain: &'a [EncryptedProxyHop],
    #[serde(skip_serializing_if = "slice_is_empty")]
    forwards: &'a [EncryptedForward],
    #[serde(skip_serializing_if = "slice_is_empty")]
    privilege_credentials: &'a [EncryptedPrivilegeCredential],
}

impl<'a> From<&'a EncryptedConnection> for ConnectionBeforeSshAlgorithms<'a> {
    fn from(connection: &'a EncryptedConnection) -> Self {
        Self {
            source_connection_id: connection.source_connection_id.as_deref(),
            name: &connection.name,
            group: connection.group.as_deref(),
            notes: connection.notes.as_deref(),
            host: &connection.host,
            port: connection.port,
            username: &connection.username,
            auth: &connection.auth,
            color: connection.color.as_deref(),
            icon_background_color: connection.icon_background_color.as_deref(),
            icon: connection.icon.as_deref(),
            tags: &connection.tags,
            options: ConnectionOptionsBeforeSshAlgorithms::from(&connection.options),
            upstream_proxy: &connection.upstream_proxy,
            proxy_chain: &connection.proxy_chain,
            forwards: &connection.forwards,
            privilege_credentials: &connection.privilege_credentials,
        }
    }
}

#[derive(Serialize)]
struct ConnectionOptionsBeforeSshAlgorithms<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    connect_timeout_seconds: Option<u64>,
    keep_alive_interval: u32,
    compression: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    jump_host: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    term_type: Option<&'a str>,
    agent_forwarding: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    identity_agent: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_forwarding_socket: Option<&'a str>,
    legacy_ssh_compatibility: bool,
    #[serde(skip_serializing_if = "bool_is_false")]
    dedicated_new_terminal_connection: bool,
    #[serde(skip_serializing_if = "x11_options_are_default")]
    x11_forwarding: &'a ConnectionX11ForwardingOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    post_connect_command: Option<&'a str>,
    #[serde(skip_serializing_if = "terminal_options_are_inherited")]
    terminal: &'a ConnectionTerminalOptions,
}

impl<'a> From<&'a ConnectionOptions> for ConnectionOptionsBeforeSshAlgorithms<'a> {
    fn from(options: &'a ConnectionOptions) -> Self {
        Self {
            connect_timeout_seconds: options.connect_timeout_seconds,
            keep_alive_interval: options.keep_alive_interval,
            compression: options.compression,
            jump_host: options.jump_host.as_deref(),
            term_type: options.term_type.as_deref(),
            agent_forwarding: options.agent_forwarding,
            identity_agent: options.identity_agent.as_deref(),
            agent_forwarding_socket: options.agent_forwarding_socket.as_deref(),
            legacy_ssh_compatibility: options.legacy_ssh_compatibility,
            dedicated_new_terminal_connection: options.dedicated_new_terminal_connection,
            x11_forwarding: &options.x11_forwarding,
            post_connect_command: options.post_connect_command.as_deref(),
            terminal: &options.terminal,
        }
    }
}

fn bool_is_false(value: &bool) -> bool {
    !*value
}

fn encrypted_upstream_proxy_is_global(value: &&EncryptedUpstreamProxyPolicy) -> bool {
    value.is_use_global()
}

fn slice_is_empty<T>(value: &&[T]) -> bool {
    value.is_empty()
}

fn x11_options_are_default(value: &&ConnectionX11ForwardingOptions) -> bool {
    value.is_default()
}

fn terminal_options_are_inherited(value: &&ConnectionTerminalOptions) -> bool {
    value.inherits_application_defaults()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload_with_default_ssh_algorithms() -> EncryptedPayload {
        EncryptedPayload {
            totp_credentials: Vec::new(),
            version: 1,
            connections: vec![EncryptedConnection {
                source_connection_id: None,
                name: "Legacy host".to_string(),
                group: None,
                notes: None,
                host: "legacy.example.com".to_string(),
                port: 22,
                username: "operator".to_string(),
                auth: EncryptedAuth::Agent,
                color: None,
                icon_background_color: None,
                icon: None,
                tags: Vec::new(),
                options: ConnectionOptions::default(),
                upstream_proxy: EncryptedUpstreamProxyPolicy::UseGlobal,
                proxy_chain: Vec::new(),
                forwards: Vec::new(),
                privilege_credentials: Vec::new(),
            }],
            app_settings_json: None,
            quick_commands_json: None,
            serial_profiles_json: None,
            telnet_profiles_json: None,
            mosh_profiles_json: None,
            standalone_sftp_profiles_json: None,
            remote_desktop_profiles_json: None,
            plugin_settings: Vec::new(),
            portable_secrets: Vec::new(),
            checksum: String::new(),
        }
    }

    #[test]
    fn checksum_accepts_archives_from_before_ssh_algorithm_preferences() {
        let mut payload = payload_with_default_ssh_algorithms();
        payload.checksum = compute_checksum_before_ssh_algorithms(&payload).unwrap();

        // The regression requires the current schema checksum to differ while
        // the authenticated historical schema remains accepted.
        assert_ne!(compute_checksum(&payload).unwrap(), payload.checksum);
        assert!(verify_checksum(&payload).is_ok());

        payload.connections[0].host.push_str(".modified");
        assert!(matches!(
            verify_checksum(&payload),
            Err(OxideFileError::ChecksumMismatch)
        ));
    }

    #[test]
    fn archive_v2_hides_metadata_and_authenticates_while_reading_v1() {
        let mut payload = payload_with_default_ssh_algorithms();
        payload.checksum = compute_checksum(&payload).unwrap();
        let metadata = OxideMetadata {
            num_connections: 1,
            connection_names: vec!["Legacy host".into()],
            ..Default::default()
        };
        let salt = [0x31; SALT_LEN];
        let nonce = [0x42; NONCE_LEN];
        let key = derive_key("archive-password", &salt, kdf_flags::CURRENT_KDF).unwrap();
        let plaintext = Zeroizing::new(rmp_serde::to_vec_named(&payload).unwrap());
        let ciphertext = ChaCha20Poly1305::new_from_slice(key.as_slice())
            .unwrap()
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_slice())
            .unwrap();
        let metadata_json = serde_json::to_vec(&metadata).unwrap();
        // Build the released wire layout independently of the archive writer.
        let mut legacy = super::super::MAGIC.to_vec();
        for value in [
            1u32,
            kdf_flags::CURRENT_KDF,
            metadata_json.len() as u32,
            (ciphertext.len() - TAG_LEN) as u32,
        ] {
            legacy.extend(value.to_le_bytes());
        }
        legacy.extend(salt);
        legacy.extend(nonce);
        legacy.extend(metadata_json);
        legacy.extend(ciphertext);
        let mut decoder = OxideBatchDecryptionContext::new("archive-password").unwrap();
        let (old_metadata, old_payload) = decrypt_oxide_archive_with_context_and_progress(
            &OxideFile::from_bytes(&legacy).unwrap(),
            &mut decoder,
            |_| {},
        )
        .unwrap();
        assert_eq!(old_metadata.connection_names, ["Legacy host"]);
        assert_eq!(old_payload.connections[0].host, "legacy.example.com");
        let bytes = encrypt_oxide_file(&payload, "archive-password", metadata)
            .unwrap()
            .to_bytes()
            .unwrap();
        assert_eq!(&bytes[5..9], &2u32.to_le_bytes());
        assert!(
            !bytes
                .windows(b"Legacy host".len())
                .any(|part| part == b"Legacy host")
        );
        let file = OxideFile::from_bytes(&bytes).unwrap();
        assert!(file.metadata.is_none());
        let (metadata, payload) =
            decrypt_oxide_archive_with_context_and_progress(&file, &mut decoder, |_| {}).unwrap();
        assert_eq!(metadata.connection_names, ["Legacy host"]);
        assert_eq!(payload.connections[0].username, "operator");
        let mut damaged = bytes;
        *damaged.last_mut().unwrap() ^= 1;
        assert!(
            decrypt_oxide_archive_with_context_and_progress(
                &OxideFile::from_bytes(&damaged).unwrap(),
                &mut decoder,
                |_| {}
            )
            .is_err()
        );
    }
}
