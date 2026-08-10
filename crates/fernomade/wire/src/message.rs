// SPDX-License-Identifier: Apache-2.0

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use prost::Message;
use std::io::{Read, Write};

const PROTOCOL_VERSION: u64 = 2;
const MAXIMUM_DECOMPRESSED_BYTES: usize = 16 * 1024 * 1024;
const MAXIMUM_DECOMPRESSED_BYTES_U64: u64 = 16 * 1024 * 1024;

/// Capability bit used by mosh-go for its optional session-control extension.
pub const SESSION_CONTROL_CAPABILITY: u8 = 1;
pub const SESSION_LIST_REQUEST: u32 = 1;
pub const SESSION_LIST_RESPONSE: u32 = 2;
pub const SESSION_SWITCH_REQUEST: u32 = 3;
pub const SESSION_SWITCHED: u32 = 4;
pub const SESSION_CREATE_REQUEST: u32 = 5;
pub const SESSION_CREATED: u32 = 6;

#[derive(Clone, PartialEq, Message)]
pub struct StateUpdate {
    #[prost(uint64, tag = "1")]
    pub protocol_version: u64,
    #[prost(uint64, tag = "2")]
    pub base_state: u64,
    #[prost(uint64, tag = "3")]
    pub target_state: u64,
    #[prost(uint64, tag = "4")]
    pub acknowledged_state: u64,
    #[prost(uint64, tag = "5")]
    pub discard_before: u64,
    #[prost(bytes = "vec", tag = "6")]
    pub delta: Vec<u8>,
    #[prost(bytes = "vec", tag = "7")]
    pub padding: Vec<u8>,
    #[prost(bytes = "vec", tag = "8")]
    pub capabilities: Vec<u8>,
}

impl StateUpdate {
    #[must_use]
    pub fn new(base_state: u64, target_state: u64, acknowledged_state: u64) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            base_state,
            target_state,
            acknowledged_state,
            discard_before: 0,
            delta: Vec::new(),
            padding: Vec::new(),
            capabilities: Vec::new(),
        }
    }

    /// Decodes the independently observed instruction batch in field 6.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed known fields, unsupported wire types,
    /// malformed Protobuf, or a protocol value other than 2.
    pub fn decode_instructions(&self) -> Result<InstructionBatch, MessageError> {
        validate_instruction_batch(&self.delta)?;
        InstructionBatch::decode(self.delta.as_slice()).map_err(|_| MessageError::MalformedProtobuf)
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct InstructionBatch {
    #[prost(message, repeated, tag = "1")]
    pub instructions: Vec<Instruction>,
}

impl InstructionBatch {
    #[must_use]
    pub fn encode_bytes(&self) -> Vec<u8> {
        self.encode_to_vec()
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct Instruction {
    #[prost(message, optional, tag = "2")]
    pub bytes: Option<ByteRun>,
    #[prost(message, optional, tag = "3")]
    pub viewport: Option<ViewportSize>,
    #[prost(message, optional, tag = "7")]
    pub marker: Option<Marker>,
    #[prost(message, optional, tag = "9")]
    pub session_control: Option<SessionControl>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ByteRun {
    #[prost(bytes = "vec", tag = "4")]
    pub value: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Message)]
pub struct ViewportSize {
    #[prost(uint64, tag = "5")]
    pub columns: u64,
    #[prost(uint64, tag = "6")]
    pub rows: u64,
}

#[derive(Clone, Copy, PartialEq, Message)]
pub struct Marker {
    #[prost(uint64, tag = "8")]
    pub value: u64,
}

/// Optional mosh-go session-control instruction carried in instruction field 9.
#[derive(Clone, Eq, PartialEq, Message)]
pub struct SessionControl {
    #[prost(uint32, tag = "10")]
    pub kind: u32,
    #[prost(bytes = "vec", tag = "11")]
    pub payload: Vec<u8>,
}

/// Compresses one validated state update with the observed zlib framing.
///
/// # Errors
///
/// Returns an error if Protobuf encoding or zlib finalization fails.
pub fn encode_compressed_update(update: &StateUpdate) -> Result<Vec<u8>, MessageError> {
    if update.protocol_version != PROTOCOL_VERSION {
        return Err(MessageError::UnsupportedProtocolVersion);
    }
    let mut encoded_message = Vec::with_capacity(update.encoded_len());
    update
        .encode(&mut encoded_message)
        .map_err(|_| MessageError::MalformedProtobuf)?;
    let mut compressor = ZlibEncoder::new(Vec::new(), Compression::default());
    compressor
        .write_all(&encoded_message)
        .map_err(|_| MessageError::CompressionFailed)?;
    compressor
        .finish()
        .map_err(|_| MessageError::CompressionFailed)
}

/// Decompresses a bounded zlib stream and skips unknown Protobuf fields.
///
/// # Errors
///
/// Returns an error for invalid zlib, oversized output, malformed known fields,
/// malformed Protobuf, or an unsupported protocol value.
pub fn decode_compressed_update(compressed: &[u8]) -> Result<StateUpdate, MessageError> {
    let mut decoder = ZlibDecoder::new(compressed);
    let mut encoded = Vec::new();
    decoder
        .by_ref()
        .take(MAXIMUM_DECOMPRESSED_BYTES_U64 + 1)
        .read_to_end(&mut encoded)
        .map_err(|_| MessageError::DecompressionFailed)?;
    if encoded.len() > MAXIMUM_DECOMPRESSED_BYTES {
        return Err(MessageError::DecompressedMessageTooLarge);
    }
    validate_fields(
        &encoded,
        &[
            (1, 0),
            (2, 0),
            (3, 0),
            (4, 0),
            (5, 0),
            (6, 2),
            (7, 2),
            (8, 2),
        ],
    )?;
    let update =
        StateUpdate::decode(encoded.as_slice()).map_err(|_| MessageError::MalformedProtobuf)?;
    if update.protocol_version != PROTOCOL_VERSION {
        return Err(MessageError::UnsupportedProtocolVersion);
    }
    validate_instruction_batch(&update.delta)?;
    Ok(update)
}

fn validate_instruction_batch(encoded: &[u8]) -> Result<(), MessageError> {
    for instruction in length_delimited_values(encoded, &[(1, 2)])? {
        validate_fields(instruction, &[(2, 2), (3, 2), (7, 2), (9, 2)])?;
        for (field, value) in length_delimited_fields(instruction)? {
            match field {
                2 => validate_fields(value, &[(4, 2)])?,
                3 => validate_fields(value, &[(5, 0), (6, 0)])?,
                7 => validate_fields(value, &[(8, 0)])?,
                9 => validate_fields(value, &[(10, 0), (11, 2)])?,
                _ => {}
            }
        }
    }
    Ok(())
}

fn validate_fields(encoded: &[u8], allowed: &[(u64, u8)]) -> Result<(), MessageError> {
    let mut remaining = encoded;
    while !remaining.is_empty() {
        let (key, key_bytes) = decode_varint(remaining)?;
        remaining = &remaining[key_bytes..];
        let field = key >> 3;
        let wire_type = u8::try_from(key & 7).expect("wire type fits u8");
        if let Some((_, expected_wire_type)) = allowed.iter().find(|(known, _)| *known == field) {
            if wire_type != *expected_wire_type {
                return Err(MessageError::UnsupportedWireType);
            }
        }
        remaining = skip_value(remaining, wire_type)?;
    }
    Ok(())
}

fn length_delimited_values<'a>(
    encoded: &'a [u8],
    allowed: &[(u64, u8)],
) -> Result<Vec<&'a [u8]>, MessageError> {
    validate_fields(encoded, allowed)?;
    Ok(length_delimited_fields(encoded)?
        .into_iter()
        .filter(|(field, _)| allowed.contains(&(*field, 2)))
        .map(|(_, value)| value)
        .collect())
}

fn length_delimited_fields(mut encoded: &[u8]) -> Result<Vec<(u64, &[u8])>, MessageError> {
    let mut values = Vec::new();
    while !encoded.is_empty() {
        let (key, key_bytes) = decode_varint(encoded)?;
        encoded = &encoded[key_bytes..];
        let field = key >> 3;
        let wire_type = u8::try_from(key & 7).expect("wire type fits u8");
        match wire_type {
            0 => encoded = skip_value(encoded, wire_type)?,
            1 => encoded = skip_value(encoded, wire_type)?,
            2 => {
                let (length, length_bytes) = decode_varint(encoded)?;
                encoded = &encoded[length_bytes..];
                let length =
                    usize::try_from(length).map_err(|_| MessageError::MalformedProtobuf)?;
                let (value, remainder) = encoded
                    .split_at_checked(length)
                    .ok_or(MessageError::MalformedProtobuf)?;
                values.push((field, value));
                encoded = remainder;
            }
            5 => encoded = skip_value(encoded, wire_type)?,
            _ => return Err(MessageError::UnsupportedWireType),
        }
    }
    Ok(values)
}

fn skip_value(encoded: &[u8], wire_type: u8) -> Result<&[u8], MessageError> {
    match wire_type {
        0 => {
            let (_, bytes) = decode_varint(encoded)?;
            Ok(&encoded[bytes..])
        }
        1 => encoded.get(8..).ok_or(MessageError::MalformedProtobuf),
        2 => {
            let (length, bytes) = decode_varint(encoded)?;
            let length = usize::try_from(length).map_err(|_| MessageError::MalformedProtobuf)?;
            encoded[bytes..]
                .get(length..)
                .ok_or(MessageError::MalformedProtobuf)
        }
        5 => encoded.get(4..).ok_or(MessageError::MalformedProtobuf),
        _ => Err(MessageError::UnsupportedWireType),
    }
}

fn decode_varint(encoded: &[u8]) -> Result<(u64, usize), MessageError> {
    let mut value = 0_u64;
    for (index, &byte) in encoded.iter().take(10).enumerate() {
        let payload = u64::from(byte & 0x7f);
        if index == 9 && payload > 1 {
            return Err(MessageError::MalformedProtobuf);
        }
        value |= payload << (index * 7);
        if byte & 0x80 == 0 {
            return Ok((value, index + 1));
        }
    }
    Err(MessageError::MalformedProtobuf)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageError {
    CompressionFailed,
    DecompressionFailed,
    DecompressedMessageTooLarge,
    MalformedProtobuf,
    UnsupportedProtocolVersion,
    UnsupportedWireType,
}

#[cfg(test)]
mod tests {
    use super::{
        ByteRun, Instruction, InstructionBatch, MessageError, SESSION_CONTROL_CAPABILITY,
        SessionControl, StateUpdate, ViewportSize, decode_compressed_update,
        encode_compressed_update, validate_fields, validate_instruction_batch,
    };
    use prost::Message;

    #[test]
    fn decodes_observed_initial_client_message() {
        let encoded =
            decode_hex("0802100018012000280032130a061a04285030180a0912072205657869740a3a00");
        let update = StateUpdate::decode(encoded.as_slice()).expect("observed message must decode");
        let instructions = update
            .decode_instructions()
            .expect("instructions must decode");

        assert_eq!(update.protocol_version, 2);
        assert_eq!((update.base_state, update.target_state), (0, 1));
        assert_eq!(instructions.instructions.len(), 2);
        assert_eq!(
            instructions.instructions[0].viewport,
            Some(ViewportSize {
                columns: 80,
                rows: 24,
            })
        );
        assert_eq!(
            instructions.instructions[1]
                .bytes
                .as_ref()
                .map(|bytes| bytes.value.as_slice()),
            Some(b"exit\n".as_slice())
        );
    }

    #[test]
    fn compressed_update_round_trips() {
        let batch = InstructionBatch {
            instructions: vec![Instruction {
                bytes: Some(ByteRun {
                    value: b"input".to_vec(),
                }),
                viewport: None,
                marker: None,
                session_control: None,
            }],
        };
        let mut update = StateUpdate::new(3, 4, 2);
        batch.encode(&mut update.delta).expect("batch must encode");

        let compressed = encode_compressed_update(&update).expect("compression must work");
        let decoded = decode_compressed_update(&compressed).expect("decode must work");

        assert_eq!(decoded, update);
    }

    #[test]
    fn compressed_update_preserves_shutdown_state_number() {
        let update = StateUpdate::new(7, u64::MAX, u64::MAX);
        let compressed = encode_compressed_update(&update).expect("shutdown state must encode");
        let decoded = decode_compressed_update(&compressed).expect("shutdown state must decode");
        assert_eq!(decoded.target_state, u64::MAX);
        assert_eq!(decoded.acknowledged_state, u64::MAX);
    }

    #[test]
    fn compressed_update_preserves_mosh_go_high_63_bit_state_numbers() {
        const HIGH_STATE_NUMBER: u64 = (1_u64 << 63) - 1;
        let mut update = StateUpdate::new(
            HIGH_STATE_NUMBER - 1,
            HIGH_STATE_NUMBER,
            HIGH_STATE_NUMBER - 2,
        );
        update.discard_before = HIGH_STATE_NUMBER - 3;

        let compressed = encode_compressed_update(&update).expect("high state must encode");
        let decoded = decode_compressed_update(&compressed).expect("high state must decode");

        assert_eq!(decoded, update);
    }

    #[test]
    fn mosh_go_extensions_round_trip_without_affecting_classic_fields() {
        let mut update = StateUpdate::new(3, 4, 2);
        update.padding = vec![0xde, 0xad];
        update.capabilities = vec![SESSION_CONTROL_CAPABILITY];
        update.delta = InstructionBatch {
            instructions: vec![Instruction {
                bytes: None,
                viewport: None,
                marker: None,
                session_control: Some(SessionControl {
                    kind: 3,
                    payload: b"session-name".to_vec(),
                }),
            }],
        }
        .encode_bytes();

        let compressed = encode_compressed_update(&update).expect("extensions must encode");
        let decoded = decode_compressed_update(&compressed).expect("extensions must decode");

        assert_eq!(decoded.padding, update.padding);
        assert_eq!(decoded.capabilities, vec![SESSION_CONTROL_CAPABILITY]);
        assert_eq!(
            decoded
                .decode_instructions()
                .expect("instructions must decode"),
            update
                .decode_instructions()
                .expect("source instructions must decode")
        );
        assert_eq!((decoded.base_state, decoded.target_state), (3, 4));
    }

    #[test]
    fn skips_unknown_top_level_field() {
        assert_eq!(validate_fields(&[0x40, 0x01], &[(1, 0)]), Ok(()));
    }

    #[test]
    fn skips_unknown_fixed_width_instruction_fields() {
        let encoded = [
            0x0a, 0x10, 0xa1, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0xad, 0x01, 0, 0, 0, 0,
        ];
        assert_eq!(validate_instruction_batch(&encoded), Ok(()));
    }

    #[test]
    fn rejects_wrong_wire_type_for_known_field() {
        assert_eq!(
            validate_fields(&[0x0a, 0x00], &[(1, 0)]),
            Err(MessageError::UnsupportedWireType)
        );
    }

    fn decode_hex(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| (nibble(pair[0]) << 4) | nibble(pair[1]))
            .collect()
    }

    fn nibble(value: u8) -> u8 {
        match value {
            b'0'..=b'9' => value - b'0',
            b'a'..=b'f' => value - b'a' + 10,
            _ => panic!("test vector contains invalid hex"),
        }
    }
}
