use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, OsRng, Payload},
};
use sha2::{Digest, Sha256};
use std::fmt::Write;
use zeroize::Zeroizing;

// One key belongs to one frozen snapshot. Positions are authenticated, never accepted as
// client-supplied offsets, and require no per-page cache or retention of old cursor records.
pub(super) struct CursorKey(Zeroizing<[u8; 32]>);
impl CursorKey {
    pub fn new() -> Self {
        Self(Zeroizing::new(
            XChaCha20Poly1305::generate_key(&mut OsRng).into(),
        ))
    }
    pub fn encode(&self, snapshot: &str, row: usize, offset: usize) -> String {
        let mut position = [0u8; 16];
        position[..8].copy_from_slice(&(row as u64).to_be_bytes());
        position[8..].copy_from_slice(&(offset as u64).to_be_bytes());
        // Deterministic 192-bit nonces make retries byte-identical. A snapshot has a fresh
        // key and immutable positions; the same nonce always encrypts the same plaintext.
        let nonce = Sha256::new()
            .chain_update(b"history-cursor-v1")
            .chain_update(self.0.as_ref())
            .chain_update(position)
            .finalize();
        let cipher = XChaCha20Poly1305::new_from_slice(self.0.as_ref()).unwrap();
        let encrypted = cipher
            .encrypt(
                XNonce::from_slice(&nonce[..24]),
                Payload {
                    msg: &position,
                    aad: snapshot.as_bytes(),
                },
            )
            .expect("fixed-size history cursor");
        let mut token = format!("{snapshot}_");
        for byte in nonce[..24].iter().chain(&encrypted) {
            write!(&mut token, "{byte:02x}").unwrap();
        }
        token
    }
    pub fn decode(&self, snapshot: &str, token: &str) -> Option<(usize, usize)> {
        let body = token.strip_prefix(snapshot)?.strip_prefix('_')?;
        if body.len() != 112 || !body.is_ascii() {
            return None;
        }
        let mut bytes = [0u8; 56];
        for (byte, pair) in bytes.iter_mut().zip(body.as_bytes().chunks_exact(2)) {
            *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
        }
        let cipher = XChaCha20Poly1305::new_from_slice(self.0.as_ref()).ok()?;
        let position = cipher
            .decrypt(
                XNonce::from_slice(&bytes[..24]),
                Payload {
                    msg: &bytes[24..],
                    aad: snapshot.as_bytes(),
                },
            )
            .ok()?;
        let position: [u8; 16] = position.try_into().ok()?;
        Some((
            usize::try_from(u64::from_be_bytes(position[..8].try_into().ok()?)).ok()?,
            usize::try_from(u64::from_be_bytes(position[8..].try_into().ok()?)).ok()?,
        ))
    }
}
