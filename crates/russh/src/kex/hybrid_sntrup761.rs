use byteorder::{BigEndian, ByteOrder};
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use curve25519_dalek::montgomery::MontgomeryPoint;
use curve25519_dalek::scalar::Scalar;
use log::debug;
use sha2::Digest;
use sntrup761::{Ciphertext, DecapsulationKey, EncapsulationKey, SharedSecret as SntrupShared};
use ssh_encoding::{Encode, Writer};

use super::{KexAlgorithm, KexAlgorithmImplementor, KexType, SharedSecret, compute_keys};
use crate::mac;
use crate::session::Exchange;
use crate::{CryptoVec, Error, cipher, msg};

const SNTRUP761_PUBLIC_KEY_SIZE: usize = sntrup761::PUBLIC_KEY_SIZE;
const SNTRUP761_CIPHERTEXT_SIZE: usize = sntrup761::CIPHERTEXT_SIZE;
const X25519_PUBLIC_KEY_SIZE: usize = 32;

pub struct Sntrup761X25519KexType {}

impl KexType for Sntrup761X25519KexType {
    fn make(&self) -> KexAlgorithm {
        Sntrup761X25519Kex {
            sntrup_secret: None,
            x25519_secret: None,
            k_sntrup: None,
            k_cl: None,
        }
        .into()
    }
}

#[doc(hidden)]
pub struct Sntrup761X25519Kex {
    sntrup_secret: Option<Box<DecapsulationKey>>,
    x25519_secret: Option<Scalar>,
    k_sntrup: Option<SntrupShared>,
    k_cl: Option<MontgomeryPoint>,
}

impl std::fmt::Debug for Sntrup761X25519Kex {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "Sntrup761X25519Kex {{ sntrup_secret: [hidden], x25519_secret: [hidden], k_sntrup: [hidden], k_cl: [hidden] }}",
        )
    }
}

impl Sntrup761X25519Kex {
    fn combined_shared_secret(&self) -> Result<Vec<u8>, Error> {
        let k_sntrup = self.k_sntrup.as_ref().ok_or(Error::KexInit)?;
        let k_cl = self.k_cl.as_ref().ok_or(Error::KexInit)?;

        let mut combined = Vec::with_capacity(sntrup761::SHARED_SECRET_SIZE + X25519_PUBLIC_KEY_SIZE);
        combined.extend_from_slice(k_sntrup.as_ref());
        combined.extend_from_slice(&k_cl.0);

        let mut hasher = sha2::Sha512::new();
        hasher.update(&combined);
        Ok(hasher.finalize().to_vec())
    }
}

impl KexAlgorithmImplementor for Sntrup761X25519Kex {
    fn skip_exchange(&self) -> bool {
        false
    }

    fn server_dh(&mut self, exchange: &mut Exchange, payload: &[u8]) -> Result<(), Error> {
        debug!("server_dh (hybrid sntrup761)");

        if payload.first() != Some(&msg::KEX_HYBRID_INIT) {
            return Err(Error::Inconsistent);
        }

        #[allow(clippy::indexing_slicing)]
        let c_init_len = BigEndian::read_u32(&payload[1..]) as usize;

        if payload.len() < 5 + c_init_len {
            return Err(Error::Inconsistent);
        }

        if c_init_len != SNTRUP761_PUBLIC_KEY_SIZE + X25519_PUBLIC_KEY_SIZE {
            return Err(Error::Kex);
        }

        #[allow(clippy::indexing_slicing)]
        let c_init = &payload[5..5 + c_init_len];

        #[allow(clippy::indexing_slicing)]
        let c_sntrup_public_bytes = &c_init[..SNTRUP761_PUBLIC_KEY_SIZE];
        #[allow(clippy::indexing_slicing)]
        let c_x25519_public_bytes = &c_init[SNTRUP761_PUBLIC_KEY_SIZE..];

        let c_sntrup_public =
            EncapsulationKey::try_from(c_sntrup_public_bytes).map_err(|_| Error::Kex)?;

        let mut c_x25519_public = MontgomeryPoint([0; 32]);
        c_x25519_public.0.copy_from_slice(c_x25519_public_bytes);

        let (s_sntrup_ciphertext, k_sntrup) =
            c_sntrup_public.encapsulate(sntrup761::rand::rng());

        let s_x25519_secret = Scalar::from_bytes_mod_order(rand::random::<[u8; 32]>());
        let s_x25519_public = (ED25519_BASEPOINT_TABLE * &s_x25519_secret).to_montgomery();
        let k_cl = s_x25519_secret * c_x25519_public;

        exchange.server_ephemeral.clear();
        exchange
            .server_ephemeral
            .extend_from_slice(s_sntrup_ciphertext.as_ref());
        exchange.server_ephemeral.extend_from_slice(&s_x25519_public.0);

        self.k_sntrup = Some(k_sntrup);
        self.k_cl = Some(k_cl);

        Ok(())
    }

    fn client_dh(
        &mut self,
        client_ephemeral: &mut Vec<u8>,
        writer: &mut impl Writer,
    ) -> Result<(), Error> {
        let (sntrup_public, sntrup_secret) = sntrup761::generate_key(sntrup761::rand::rng());

        let x25519_secret = Scalar::from_bytes_mod_order(rand::random::<[u8; 32]>());
        let x25519_public = (ED25519_BASEPOINT_TABLE * &x25519_secret).to_montgomery();

        client_ephemeral.clear();
        client_ephemeral.extend_from_slice(sntrup_public.as_ref());
        client_ephemeral.extend_from_slice(&x25519_public.0);

        msg::KEX_HYBRID_INIT.encode(writer)?;
        client_ephemeral.as_slice().encode(writer)?;

        self.sntrup_secret = Some(Box::new(sntrup_secret));
        self.x25519_secret = Some(x25519_secret);

        Ok(())
    }

    fn compute_shared_secret(&mut self, remote_pubkey_: &[u8]) -> Result<(), Error> {
        if remote_pubkey_.len() != SNTRUP761_CIPHERTEXT_SIZE + X25519_PUBLIC_KEY_SIZE {
            return Err(Error::Kex);
        }

        #[allow(clippy::indexing_slicing)]
        let s_sntrup_ciphertext_bytes = &remote_pubkey_[..SNTRUP761_CIPHERTEXT_SIZE];
        #[allow(clippy::indexing_slicing)]
        let s_x25519_public_bytes = &remote_pubkey_[SNTRUP761_CIPHERTEXT_SIZE..];

        let s_sntrup_ciphertext =
            Ciphertext::try_from(s_sntrup_ciphertext_bytes).map_err(|_| Error::KexInit)?;

        let sntrup_secret = self.sntrup_secret.take().ok_or(Error::KexInit)?;
        let k_sntrup = sntrup_secret.decapsulate(&s_sntrup_ciphertext);

        let mut s_x25519_public = MontgomeryPoint([0; 32]);
        s_x25519_public.0.copy_from_slice(s_x25519_public_bytes);

        let x25519_secret = self.x25519_secret.take().ok_or(Error::KexInit)?;
        let k_cl = x25519_secret * s_x25519_public;

        self.k_sntrup = Some(k_sntrup);
        self.k_cl = Some(k_cl);

        Ok(())
    }

    fn shared_secret_bytes(&self) -> Option<&[u8]> {
        // The RFC-defined shared secret is the hash of both components. Keep this
        // callback compatible with the other hybrid KEX implementation by exposing
        // the classical component and deriving the real SSH secret in compute_keys.
        self.k_cl.as_ref().map(|k| k.0.as_slice())
    }

    fn compute_exchange_hash(
        &self,
        key: &[u8],
        exchange: &Exchange,
        buffer: &mut CryptoVec,
    ) -> Result<Vec<u8>, Error> {
        buffer.clear();
        exchange.client_id.encode(buffer)?;
        exchange.server_id.encode(buffer)?;
        exchange.client_kex_init.encode(buffer)?;
        exchange.server_kex_init.encode(buffer)?;

        buffer.extend(key);

        exchange.client_ephemeral.encode(buffer)?;
        exchange.server_ephemeral.encode(buffer)?;

        self.combined_shared_secret()?.as_slice().encode(buffer)?;

        let mut hasher = sha2::Sha512::new();
        hasher.update(&buffer);

        Ok(hasher.finalize().to_vec())
    }

    fn compute_keys(
        &self,
        session_id: &[u8],
        exchange_hash: &[u8],
        cipher: cipher::Name,
        remote_to_local_mac: mac::Name,
        local_to_remote_mac: mac::Name,
        is_server: bool,
    ) -> Result<super::cipher::CipherPair, Error> {
        let k = self.combined_shared_secret()?;
        let shared_secret = SharedSecret::from_string(&k)?;

        compute_keys::<sha2::Sha512>(
            Some(&shared_secret),
            session_id,
            exchange_hash,
            cipher,
            remote_to_local_mac,
            local_to_remote_mac,
            is_server,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sntrup761x25519_key_exchange_derives_matching_secrets() {
        let mut client_kex = Sntrup761X25519Kex {
            sntrup_secret: None,
            x25519_secret: None,
            k_sntrup: None,
            k_cl: None,
        };

        let mut server_kex = Sntrup761X25519Kex {
            sntrup_secret: None,
            x25519_secret: None,
            k_sntrup: None,
            k_cl: None,
        };

        let mut client_ephemeral = Vec::new();
        let mut client_payload = Vec::new();
        client_kex
            .client_dh(&mut client_ephemeral, &mut client_payload)
            .unwrap();

        assert_eq!(
            client_ephemeral.len(),
            SNTRUP761_PUBLIC_KEY_SIZE + X25519_PUBLIC_KEY_SIZE
        );

        let mut exchange = Exchange::default();
        server_kex.server_dh(&mut exchange, &client_payload).unwrap();

        assert_eq!(
            exchange.server_ephemeral.len(),
            SNTRUP761_CIPHERTEXT_SIZE + X25519_PUBLIC_KEY_SIZE
        );

        client_kex
            .compute_shared_secret(&exchange.server_ephemeral)
            .unwrap();

        assert_eq!(
            client_kex.combined_shared_secret().unwrap(),
            server_kex.combined_shared_secret().unwrap()
        );
    }

    #[test]
    fn sntrup761x25519_rejects_bad_peer_lengths() {
        let mut client_kex = Sntrup761X25519Kex {
            sntrup_secret: None,
            x25519_secret: None,
            k_sntrup: None,
            k_cl: None,
        };

        assert!(matches!(client_kex.compute_shared_secret(&[0; 32]), Err(Error::Kex)));
    }
}
