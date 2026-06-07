use super::{impl_packet_for, impl_request_id, Packet, RequestId};

/// Implementation for `SSH_FXP_STAT`
#[derive(Debug, Serialize, Deserialize)]
pub struct Stat {
    pub id: u32,
    pub path: String,
}

impl_request_id!(Stat);
impl_packet_for!(Stat);
