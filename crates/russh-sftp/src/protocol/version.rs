use std::collections::HashMap;

use super::{impl_packet_for, Packet, VERSION};

/// Implementation for `SSH_FXP_VERSION`
#[derive(Debug, Serialize, Deserialize)]
pub struct Version {
    pub version: u32,
    pub extensions: HashMap<String, String>,
}

impl_packet_for!(Version);

impl Version {
    pub fn new() -> Self {
        Self {
            version: VERSION,
            extensions: HashMap::new(),
        }
    }
}

impl Default for Version {
    fn default() -> Self {
        Self::new()
    }
}
