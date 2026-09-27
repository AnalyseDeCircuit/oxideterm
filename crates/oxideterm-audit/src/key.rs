use crate::AuditError;
use base64::{Engine, engine::general_purpose::STANDARD};
use oxideterm_portable_runtime::{is_portable_mode, keystore};
use oxideterm_secret_store::NativeSecretStore;
use rand::{RngCore, rngs::OsRng};
use zeroize::Zeroizing;

const SERVICE: &str = "com.oxideterm.audit";

/// The caller serializes first-key creation with the database write transaction.
pub trait AuditKeyProvider {
    fn load(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, AuditError>;
    fn create(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, AuditError>;
}

pub struct PlatformAuditKeyProvider;

impl AuditKeyProvider for PlatformAuditKeyProvider {
    fn load(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
        let encoded = if is_portable_mode().map_err(|_| AuditError::KeyUnavailable)? {
            keystore::get_secret(SERVICE, id).map_err(|_| AuditError::KeyUnavailable)?
        } else {
            NativeSecretStore::new(SERVICE)
                .get(id)
                .map_err(|_| AuditError::KeyUnavailable)?
                .ok_or(AuditError::KeyUnavailable)?
        };
        let bytes = Zeroizing::new(
            STANDARD
                .decode(encoded.as_bytes())
                .map_err(|_| AuditError::KeyUnavailable)?,
        );
        if bytes.len() != 32 {
            return Err(AuditError::KeyUnavailable);
        }
        Ok(bytes)
    }

    fn create(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
        let mut key = Zeroizing::new(vec![0; 32]);
        OsRng
            .try_fill_bytes(&mut key)
            .map_err(|_| AuditError::KeyUnavailable)?;
        let encoded = Zeroizing::new(STANDARD.encode(&*key));
        if is_portable_mode().map_err(|_| AuditError::KeyUnavailable)? {
            keystore::store_secret(SERVICE, id, &encoded)
                .map_err(|_| AuditError::KeyUnavailable)?;
        } else {
            NativeSecretStore::new(SERVICE)
                .store(id, &encoded)
                .map_err(|_| AuditError::KeyUnavailable)?;
        }
        Ok(key)
    }
}
