use super::{impl_packet_for, impl_request_id, Packet, RequestId};

/// Implementation for `SSH_FXP_HANDLE`
#[derive(Debug, Serialize, Deserialize)]
pub struct Handle {
    pub id: u32,
    pub handle: String,
}

impl_request_id!(Handle);
impl_packet_for!(Handle);
