use super::{impl_packet_for, impl_request_id, Packet, RequestId};

/// Implementation for `SSH_FXP_READLINK`
#[derive(Debug, Serialize, Deserialize)]
pub struct ReadLink {
    pub id: u32,
    pub path: String,
}

impl_request_id!(ReadLink);
impl_packet_for!(ReadLink);
