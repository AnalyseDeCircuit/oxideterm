use std::collections::HashMap;

use super::{impl_packet_for, Packet, VERSION};

/// Implementation for `SSH_FXP_INIT`
#[derive(Debug, Serialize, Deserialize)]
pub struct Init {
    pub version: u32,
    pub extensions: HashMap<String, String>,
}

impl_packet_for!(Init);

impl Init {
    pub fn new() -> Self {
        Self {
            version: VERSION,
            extensions: HashMap::new(),
        }
    }
}

impl Default for Init {
    fn default() -> Self {
        Self::new()
    }
}
