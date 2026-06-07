use bytes::Bytes;

use super::{impl_packet_for, impl_request_id, Packet, RequestId};

/// Implementation for `SSH_FXP_DATA`
#[derive(Debug)]
pub struct Data {
    pub id: u32,
    pub data: Bytes,
}

impl_request_id!(Data);
impl_packet_for!(Data);
