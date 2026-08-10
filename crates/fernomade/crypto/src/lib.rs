// SPDX-License-Identifier: Apache-2.0

//! Authenticated datagram envelopes derived from RFC 7253 and clean-room tests.

use aes::Aes128;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ocb3::Ocb3;
use ocb3::aead::{Aead, KeyInit, generic_array::GenericArray};
use std::fmt;
use zeroize::{Zeroize, Zeroizing};

const SESSION_KEY_BYTES: usize = 16;
const BASE64_PADDING_QUANTUM_BYTES: usize = 4;
const PACKET_PREFIX_BYTES: usize = 8;
const OCB_NONCE_BYTES: usize = 12;
const OCB_TAG_BYTES: usize = 16;
const DIRECTION_BIT: u64 = 1_u64 << 63;
const MAX_COUNTER: u64 = DIRECTION_BIT - 1;
const MINIMUM_PACKET_BYTES: usize = PACKET_PREFIX_BYTES + OCB_TAG_BYTES;

type DatagramCipher = Ocb3<Aes128>;

/// Owns a decoded session key without exposing it through formatting or cloning.
pub struct SessionKey(Zeroizing<[u8; SESSION_KEY_BYTES]>);

impl SessionKey {
    /// Decodes the Base64 key supplied by the authenticated bootstrap.
    ///
    /// # Errors
    ///
    /// Returns an error when the text is not Base64 or does not decode
    /// to the required 16-byte AES key.
    pub fn decode(value: &str) -> Result<Self, KeyError> {
        let mut padded_value = Zeroizing::new(value.to_owned());
        while padded_value.len() % BASE64_PADDING_QUANTUM_BYTES != 0 {
            padded_value.push('=');
        }
        let mut decoded = Zeroizing::new(
            STANDARD
                .decode(padded_value.as_bytes())
                .map_err(|_| KeyError::InvalidBase64)?,
        );
        if decoded.len() != SESSION_KEY_BYTES {
            return Err(KeyError::InvalidLength);
        }

        let mut bytes = Zeroizing::new([0_u8; SESSION_KEY_BYTES]);
        bytes.copy_from_slice(&decoded);
        decoded.zeroize();
        Ok(Self(bytes))
    }

    fn expose(&self) -> &[u8; SESSION_KEY_BYTES] {
        &self.0
    }
}

impl fmt::Debug for SessionKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SessionKey([REDACTED])")
    }
}

/// Identifies whether this process behaves as a protocol client or server.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerRole {
    Client,
    Server,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PacketDirection {
    ClientToServer,
    ServerToClient,
}

impl PacketDirection {
    fn for_outbound(role: PeerRole) -> Self {
        match role {
            PeerRole::Client => Self::ClientToServer,
            PeerRole::Server => Self::ServerToClient,
        }
    }

    fn for_inbound(role: PeerRole) -> Self {
        match role {
            PeerRole::Client => Self::ServerToClient,
            PeerRole::Server => Self::ClientToServer,
        }
    }

    fn encode(self, counter: u64) -> u64 {
        match self {
            Self::ClientToServer => counter,
            Self::ServerToClient => counter | DIRECTION_BIT,
        }
    }

    fn decode(value: u64) -> (Self, u64) {
        let direction = if value & DIRECTION_BIT == 0 {
            Self::ClientToServer
        } else {
            Self::ServerToClient
        };
        (direction, value & MAX_COUNTER)
    }
}

/// Couples one outbound nonce sequence with an authenticated inbound opener.
pub struct SecureChannel {
    outbound_cipher: DatagramCipher,
    inbound_cipher: DatagramCipher,
    outbound_direction: PacketDirection,
    inbound_direction: PacketDirection,
    next_counter: Option<u64>,
}

impl SecureChannel {
    /// Consumes the key so only expanded cipher state remains in the channel.
    #[must_use]
    pub fn new(role: PeerRole, key: SessionKey) -> Self {
        let outbound_cipher = DatagramCipher::new(GenericArray::from_slice(key.expose()));
        let inbound_cipher = DatagramCipher::new(GenericArray::from_slice(key.expose()));
        drop(key);
        Self {
            outbound_cipher,
            inbound_cipher,
            outbound_direction: PacketDirection::for_outbound(role),
            inbound_direction: PacketDirection::for_inbound(role),
            next_counter: Some(0),
        }
    }

    /// Encrypts once with the next counter and advances it after success.
    ///
    /// # Errors
    ///
    /// Returns an error if the 63-bit counter space is exhausted or the
    /// authenticated-encryption backend rejects the operation.
    pub fn seal_next(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, SealError> {
        let counter = self.next_counter.ok_or(SealError::CounterExhausted)?;
        let encoded_number = self.outbound_direction.encode(counter);
        let prefix = encoded_number.to_be_bytes();
        let nonce = make_nonce(prefix);
        let sealed = self
            .outbound_cipher
            .encrypt(GenericArray::from_slice(&nonce), plaintext)
            .map_err(|_| SealError::EncryptionFailed)?;

        self.next_counter = counter.checked_add(1).filter(|next| *next <= MAX_COUNTER);
        let mut packet = Vec::with_capacity(PACKET_PREFIX_BYTES + sealed.len());
        packet.extend_from_slice(&prefix);
        packet.extend_from_slice(&sealed);
        Ok(packet)
    }

    /// Authenticates a packet before returning any plaintext to the caller.
    ///
    /// # Errors
    ///
    /// Returns an error for undersized packets, failed authentication, or an
    /// authenticated packet sent in the opposite protocol direction.
    pub fn open(&self, packet: &[u8]) -> Result<AuthenticatedPacket, OpenError> {
        if packet.len() < MINIMUM_PACKET_BYTES {
            return Err(OpenError::PacketTooShort);
        }

        let prefix: [u8; PACKET_PREFIX_BYTES] = packet[..PACKET_PREFIX_BYTES]
            .try_into()
            .map_err(|_| OpenError::PacketTooShort)?;
        let encoded_number = u64::from_be_bytes(prefix);
        let (direction, counter) = PacketDirection::decode(encoded_number);
        let nonce = make_nonce(prefix);
        let mut plaintext = self
            .inbound_cipher
            .decrypt(
                GenericArray::from_slice(&nonce),
                &packet[PACKET_PREFIX_BYTES..],
            )
            .map_err(|_| OpenError::AuthenticationFailed)?;

        if direction != self.inbound_direction {
            plaintext.zeroize();
            return Err(OpenError::UnexpectedDirection);
        }

        Ok(AuthenticatedPacket { counter, plaintext })
    }
}

/// Plaintext is constructible only after successful authentication.
#[derive(Eq, PartialEq)]
pub struct AuthenticatedPacket {
    pub counter: u64,
    pub plaintext: Vec<u8>,
}

impl fmt::Debug for AuthenticatedPacket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticatedPacket")
            .field("counter", &self.counter)
            .field("plaintext_bytes", &self.plaintext.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyError {
    InvalidBase64,
    InvalidLength,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SealError {
    CounterExhausted,
    EncryptionFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenError {
    PacketTooShort,
    AuthenticationFailed,
    UnexpectedDirection,
}

fn make_nonce(prefix: [u8; PACKET_PREFIX_BYTES]) -> [u8; OCB_NONCE_BYTES] {
    let mut nonce = [0_u8; OCB_NONCE_BYTES];
    nonce[OCB_NONCE_BYTES - PACKET_PREFIX_BYTES..].copy_from_slice(&prefix);
    nonce
}

#[cfg(test)]
mod tests {
    use super::{
        AuthenticatedPacket, MAX_COUNTER, OpenError, PeerRole, SealError, SecureChannel, SessionKey,
    };
    use aes::Aes128;
    use ocb3::Ocb3;
    use ocb3::aead::{Aead, KeyInit, Payload, generic_array::GenericArray};

    const SYNTHETIC_KEY: &str = "AAECAwQFBgcICQoLDA0ODw";

    #[test]
    fn authenticated_packet_debug_redacts_plaintext() {
        let packet = AuthenticatedPacket {
            counter: 7,
            plaintext: b"debug-plaintext-sentinel".to_vec(),
        };
        let output = format!("{packet:?}");
        assert!(!output.contains("debug-plaintext-sentinel"));
        assert!(output.contains("plaintext_bytes: 24"));
    }

    #[test]
    fn matches_rfc_7253_vector_with_plaintext_and_aad() {
        let key = decode_hex::<16>("000102030405060708090a0b0c0d0e0f");
        let nonce = decode_hex::<12>("bbaa99887766554433221101");
        let data = decode_hex::<8>("0001020304050607");
        let expected = decode_hex::<24>("6820b3657b6f615a5725bda0d3b4eb3a257c9af1f8f03009");
        let cipher = Ocb3::<Aes128>::new(GenericArray::from_slice(&key));

        let actual = cipher
            .encrypt(
                GenericArray::from_slice(&nonce),
                Payload {
                    msg: &data,
                    aad: &data,
                },
            )
            .expect("the public RFC vector must encrypt");

        assert_eq!(actual, expected);
    }

    #[test]
    fn client_and_server_channels_round_trip_both_directions() {
        let mut client = SecureChannel::new(
            PeerRole::Client,
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );
        let mut server = SecureChannel::new(
            PeerRole::Server,
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );

        let client_packet = client.seal_next(b"client state").expect("seal must work");
        assert_eq!(&client_packet[..8], &[0; 8]);
        assert_eq!(
            server
                .open(&client_packet)
                .expect("server must authenticate")
                .plaintext,
            b"client state"
        );

        let server_packet = server.seal_next(b"server state").expect("seal must work");
        assert_eq!(&server_packet[..8], &[0x80, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            client
                .open(&server_packet)
                .expect("client must authenticate")
                .plaintext,
            b"server state"
        );
    }

    #[test]
    fn session_key_accepts_padded_and_unpadded_base64() {
        const PARTIALLY_PADDED_SYNTHETIC_KEY: &str = "AAECAwQFBgcICQoLDA0ODw=";
        const PADDED_SYNTHETIC_KEY: &str = "AAECAwQFBgcICQoLDA0ODw==";
        let mut unpadded_channel = SecureChannel::new(
            PeerRole::Client,
            SessionKey::decode(SYNTHETIC_KEY).expect("unpadded key must decode"),
        );
        let mut padded_channel = SecureChannel::new(
            PeerRole::Client,
            SessionKey::decode(PADDED_SYNTHETIC_KEY).expect("padded key must decode"),
        );
        let mut partially_padded_channel = SecureChannel::new(
            PeerRole::Client,
            SessionKey::decode(PARTIALLY_PADDED_SYNTHETIC_KEY)
                .expect("partially padded key must decode"),
        );

        let unpadded_packet = unpadded_channel
            .seal_next(b"same plaintext")
            .expect("unpadded key must seal");
        assert_eq!(
            unpadded_packet,
            padded_channel
                .seal_next(b"same plaintext")
                .expect("padded key must seal")
        );
        assert_eq!(
            unpadded_packet,
            partially_padded_channel
                .seal_next(b"same plaintext")
                .expect("partially padded key must seal")
        );
    }

    #[test]
    fn counter_is_big_endian_and_monotonic() {
        let mut client = SecureChannel::new(
            PeerRole::Client,
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );

        let first = client.seal_next(b"first").expect("seal must work");
        let second = client.seal_next(b"second").expect("seal must work");

        assert_eq!(&first[..8], &[0; 8]);
        assert_eq!(&second[..8], &[0, 0, 0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn source_counter_accepts_high_63_bit_values_then_exhausts() {
        const HIGH_COUNTER: u64 = 1_u64 << 62;

        let mut client = SecureChannel::new(
            PeerRole::Client,
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );
        let server = SecureChannel::new(
            PeerRole::Server,
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );

        // This deterministic fixture reaches the transport boundary without a socket.
        client.next_counter = Some(HIGH_COUNTER);
        let high_packet = client.seal_next(b"high counter").expect("seal must work");
        assert_eq!(
            u64::from_be_bytes(high_packet[..8].try_into().unwrap()),
            HIGH_COUNTER
        );
        assert_eq!(
            server
                .open(&high_packet)
                .expect("server must authenticate")
                .counter,
            HIGH_COUNTER
        );

        client.next_counter = Some(MAX_COUNTER);
        let final_packet = client.seal_next(b"final counter").expect("seal must work");
        assert_eq!(
            u64::from_be_bytes(final_packet[..8].try_into().unwrap()),
            MAX_COUNTER
        );
        assert_eq!(
            client.seal_next(b"counter exhausted"),
            Err(SealError::CounterExhausted)
        );
    }

    #[test]
    fn tampering_reveals_no_plaintext() {
        let mut client = SecureChannel::new(
            PeerRole::Client,
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );
        let server = SecureChannel::new(
            PeerRole::Server,
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );
        let mut packet = client
            .seal_next(b"sensitive state")
            .expect("seal must work");
        let last = packet.last_mut().expect("sealed packet has a tag");
        *last ^= 1;

        assert_eq!(server.open(&packet), Err(OpenError::AuthenticationFailed));
    }

    #[test]
    fn tampering_with_packet_prefix_fails_authentication() {
        let mut client = SecureChannel::new(
            PeerRole::Client,
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );
        let server = SecureChannel::new(
            PeerRole::Server,
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );
        let mut packet = client
            .seal_next(b"authenticated state")
            .expect("seal must work");
        packet[7] ^= 1;

        assert_eq!(server.open(&packet), Err(OpenError::AuthenticationFailed));
    }

    #[test]
    fn reflected_packet_is_rejected_after_authentication() {
        let mut client = SecureChannel::new(
            PeerRole::Client,
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );
        let packet = client.seal_next(b"state").expect("seal must work");

        assert_eq!(client.open(&packet), Err(OpenError::UnexpectedDirection));
    }

    #[test]
    fn short_packet_is_rejected() {
        let client = SecureChannel::new(
            PeerRole::Client,
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );

        assert_eq!(client.open(&[0; 23]), Err(OpenError::PacketTooShort));
    }

    fn decode_hex<const LENGTH: usize>(value: &str) -> [u8; LENGTH] {
        let mut output = [0_u8; LENGTH];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            output[index] = (nibble(pair[0]) << 4) | nibble(pair[1]);
        }
        output
    }

    fn nibble(value: u8) -> u8 {
        match value {
            b'0'..=b'9' => value - b'0',
            b'a'..=b'f' => value - b'a' + 10,
            _ => panic!("test vector contains invalid hex"),
        }
    }
}
