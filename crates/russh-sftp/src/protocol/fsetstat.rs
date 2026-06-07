use super::{impl_packet_for, impl_request_id, FileAttributes, Packet, RequestId};

/// Implementation for `SSH_FXP_FSETSTAT`
#[derive(Debug, Serialize, Deserialize)]
pub struct FSetStat {
    pub id: u32,
    pub handle: String,
    pub attrs: FileAttributes,
}

impl_request_id!(FSetStat);
impl_packet_for!(FSetStat);
