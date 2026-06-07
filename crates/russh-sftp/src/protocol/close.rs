use super::{impl_packet_for, impl_request_id, Packet, RequestId};

/// Implementation for `SSH_FXP_CLOSE`
#[derive(Debug, Serialize, Deserialize)]
pub struct Close {
    pub id: u32,
    pub handle: String,
}

impl_request_id!(Close);
impl_packet_for!(Close);
