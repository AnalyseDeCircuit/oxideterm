use serde::{Deserialize, Serialize};

use super::{impl_packet_for, impl_request_id, File, Packet, RequestId};

/// Implementation for `SSH_FXP_NAME`
#[derive(Debug, Serialize, Deserialize)]
pub struct Name {
    pub id: u32,
    pub files: Vec<File>,
}

impl_request_id!(Name);
impl_packet_for!(Name);
