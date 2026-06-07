use super::{impl_packet_for, impl_request_id, Packet, RequestId};

/// Implementation for `SSH_FXP_REALPATH`
#[derive(Debug, Serialize, Deserialize)]
pub struct RealPath {
    pub id: u32,
    pub path: String,
}

impl_request_id!(RealPath);
impl_packet_for!(RealPath);
