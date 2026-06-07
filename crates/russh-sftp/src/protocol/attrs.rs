use super::{impl_packet_for, impl_request_id, FileAttributes, Packet, RequestId};

/// Implementation for `SSH_FXP_ATTRS`
#[derive(Debug, Serialize, Deserialize)]
pub struct Attrs {
    pub id: u32,
    pub attrs: FileAttributes,
}

impl_request_id!(Attrs);
impl_packet_for!(Attrs);
