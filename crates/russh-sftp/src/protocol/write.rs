use super::{impl_packet_for, impl_request_id, Packet, RequestId};

/// Implementation for `SSH_FXP_WRITE`
#[derive(Debug, Serialize, Deserialize)]
pub struct Write {
    pub id: u32,
    pub handle: String,
    pub offset: u64,
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

impl_request_id!(Write);
impl_packet_for!(Write);
