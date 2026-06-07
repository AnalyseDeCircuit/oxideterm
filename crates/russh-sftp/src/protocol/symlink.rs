use super::{impl_packet_for, impl_request_id, Packet, RequestId};

/// Implementation for `SSH_FXP_SYMLINK`
#[derive(Debug, Serialize, Deserialize)]
pub struct Symlink {
    pub id: u32,
    pub linkpath: String,
    pub targetpath: String,
}

impl_request_id!(Symlink);
impl_packet_for!(Symlink);
