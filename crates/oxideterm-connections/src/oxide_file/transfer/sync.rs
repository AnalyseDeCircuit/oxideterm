// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

pub struct DecodedSyncArchiveConnections {
    pub snapshot: crate::SavedConnectionsSyncSnapshot,
    pub credentials: Vec<EncryptedPortableSecret>,
    pub managed_keys: Vec<crate::ManagedSshKeySyncRecord>,
    pub forwards: Vec<OxideForwardRecord>,
    pub privilege_credentials: Vec<crate::PrivilegeCredentialSyncRecord>,
}

/// Decode into portable records without creating files or touching protected
/// storage. The sync coordinator owns the later durable application transaction.
pub fn decode_archive_sync_connections(
    store: &ConnectionStore,
    connections: Vec<EncryptedConnection>,
) -> Result<DecodedSyncArchiveConnections, OxideFileError> {
    let mut keys = Vec::new();
    let mut records = Vec::new();
    let mut forwards = Vec::new();
    let mut ids = HashMap::new();
    let mut privilege_credentials = Vec::new();
    for mut connection in connections {
        prepare_sync_auth(&mut connection.auth, &mut keys, &mut ids)?;
        for hop in &mut connection.proxy_chain {
            prepare_sync_auth(&mut hop.auth, &mut keys, &mut ids)?;
        }
        let id = connection
            .source_connection_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let (mut record, incoming_forwards) = encrypted_connection_to_saved(
            store,
            connection,
            None,
            Some(id.clone()),
            &mut ids,
            &mut Vec::new(),
            &OxideImportOptions {
                restore_managed_key_passphrases: true,
                ..Default::default()
            },
        )?;
        if let Some(previous) = store.get(&id) {
            record.created_at = previous.created_at;
        }
        for mut metadata in record.privilege_credentials.drain(..) {
            let secret = metadata.plaintext_secret.take();
            privilege_credentials.push(crate::PrivilegeCredentialSyncRecord { metadata, secret });
        }
        records.push(record);
        forwards.extend(incoming_forwards);
    }
    let (snapshot, credentials) = store
        .snapshot_archived_connections(records)
        .map_err(|error| OxideFileError::InvalidFormat(error.to_string()))?;
    Ok(DecodedSyncArchiveConnections {
        snapshot,
        credentials,
        managed_keys: keys,
        forwards,
        privilege_credentials,
    })
}

fn prepare_sync_auth(
    auth: &mut EncryptedAuth,
    keys: &mut Vec<crate::ManagedSshKeySyncRecord>,
    ids: &mut HashMap<String, String>,
) -> Result<(), OxideFileError> {
    if let EncryptedAuth::Certificate {
        embedded_key,
        embedded_cert,
        ..
    } = auth
    {
        if embedded_key.is_some() || embedded_cert.is_some() {
            if embedded_key.is_none() || embedded_cert.is_none() {
                return Err(OxideFileError::InvalidFormat(
                    "The archived certificate requires both its certificate and private key".into(),
                ));
            }
            let EncryptedAuth::Certificate {
                key_path,
                passphrase,
                embedded_key,
                embedded_cert,
                ..
            } = std::mem::replace(auth, EncryptedAuth::Agent)
            else {
                unreachable!()
            };
            let bytes = Zeroizing::new(BASE64.decode(embedded_cert.unwrap().as_bytes()).map_err(
                |_| OxideFileError::InvalidFormat("Invalid archived certificate encoding".into()),
            )?);
            let certificate = String::from_utf8(bytes.to_vec()).map_err(|_| {
                OxideFileError::InvalidFormat("Invalid archived certificate text".into())
            })?;
            *auth = EncryptedAuth::Key {
                key_path,
                passphrase,
                embedded_key,
                managed_key: Some(EncryptedManagedKeyMetadata {
                    key_id: Uuid::new_v4().to_string(),
                    name: "Imported SSH certificate".into(),
                    certificate: Some(certificate),
                    fingerprint: None,
                    public_key: None,
                    origin: None,
                    requires_passphrase: None,
                }),
            };
        }
    }
    match auth {
        EncryptedAuth::Key {
            embedded_key: Some(encoded),
            managed_key,
            ..
        } => {
            let id = managed_key
                .as_ref()
                .map(|key| key.key_id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let name = managed_key
                .as_ref()
                .map(|key| key.name.clone())
                .unwrap_or_else(|| "Imported SSH key".into());
            let bytes = Zeroizing::new(BASE64.decode(encoded.as_bytes()).map_err(|_| {
                OxideFileError::InvalidFormat("Invalid archived key encoding".into())
            })?);
            let private = std::str::from_utf8(&bytes)
                .map_err(|_| OxideFileError::InvalidFormat("Invalid archived key text".into()))?;
            let mut record = crate::ManagedSshKeySyncRecord::from_private_key(
                id.clone(),
                name,
                SecretString::from(private),
            )
            .map_err(|error| OxideFileError::InvalidFormat(error.to_string()))?;
            record.metadata.certificate =
                managed_key.as_ref().and_then(|key| key.certificate.clone());
            record
                .validate_certificate()
                .map_err(|error| OxideFileError::InvalidFormat(error.to_string()))?;
            *managed_key = Some(EncryptedManagedKeyMetadata {
                certificate: record.metadata.certificate.clone(),
                key_id: id.clone(),
                name: record.metadata.name.clone(),
                fingerprint: Some(record.metadata.fingerprint.clone()),
                public_key: Some(record.metadata.public_key.clone()),
                origin: Some("oxide_import".into()),
                requires_passphrase: Some(record.metadata.requires_passphrase),
            });
            ids.insert(id, record.metadata.id.clone());
            if !keys.iter().any(|key| key.metadata.id == record.metadata.id) {
                keys.push(record);
            }
        }
        EncryptedAuth::KerberosPreferred { fallback, .. } => {
            prepare_sync_auth(fallback, keys, ids)?
        }
        _ => {}
    }
    Ok(())
}
