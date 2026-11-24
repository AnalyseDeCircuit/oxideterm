use oxideterm_ai::tool_resources::AiTerminalCommandStatus;
use zeroize::Zeroizing;

#[cfg(not(target_family = "wasm"))]
pub use super::native::watch::*;

pub struct AiTerminalWatchRecord {
    pub id: String,
    pub command: Zeroizing<String>,
    pub status: AiTerminalCommandStatus,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub exit_code: Option<i32>,
}

pub enum AiTerminalWatchSample {
    HostClosed,
    OwnerClosed,
    Recovering,
    Current {
        buffer: Zeroizing<String>,
        alternate_screen: bool,
        waiting_for_secret: bool,
        prompt_returned: bool,
        records: Vec<AiTerminalWatchRecord>,
    },
}
