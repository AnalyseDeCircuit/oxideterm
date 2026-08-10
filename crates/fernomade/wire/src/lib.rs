// SPDX-License-Identifier: Apache-2.0

//! Clean-room wire messages and authenticated-plaintext fragmentation.

mod fragment;
mod message;

pub use fragment::{
    Fragment, FragmentAccumulator, FragmentError, FragmentHeader, MAX_FRAGMENT_BODY_BYTES,
};
pub use message::{
    ByteRun, Instruction, InstructionBatch, Marker, MessageError, SESSION_CONTROL_CAPABILITY,
    SESSION_CREATE_REQUEST, SESSION_CREATED, SESSION_LIST_REQUEST, SESSION_LIST_RESPONSE,
    SESSION_SWITCH_REQUEST, SESSION_SWITCHED, SessionControl, StateUpdate, ViewportSize,
    decode_compressed_update, encode_compressed_update,
};
