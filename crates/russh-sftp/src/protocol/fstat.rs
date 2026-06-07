use super::{impl_packet_for, impl_request_id, Packet, RequestId};

/// Implementation for `SSH_FXP_FSTAT`
#[derive(Debug, Serialize, Deserialize)]
pub struct Fstat {
    pub id: u32,
    pub handle: String,
}

impl_request_id!(Fstat);
impl_packet_for!(Fstat);
