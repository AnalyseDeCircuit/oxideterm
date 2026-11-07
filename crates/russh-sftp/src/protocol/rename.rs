use super::{impl_packet_for, impl_request_id, Packet, RequestId};

/// Implementation for `SSH_FXP_RENAME`
#[derive(Debug, Serialize, Deserialize)]
pub struct Rename {
    pub id: u32,
    #[serde(with = "crate::path")]
    pub oldpath: String,
    #[serde(with = "crate::path")]
    pub newpath: String,
}

impl_request_id!(Rename);
impl_packet_for!(Rename);
