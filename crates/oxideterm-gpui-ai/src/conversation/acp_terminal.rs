use std::path::PathBuf;

use gpui::{App, Window};
use oxideterm_ai::AcpTerminalCreateSpec;

pub enum AcpTerminalStatus {
    Running,
    Exited(Option<i32>),
    Closed,
}

#[cfg(not(target_family = "wasm"))]
impl AcpTerminalStatus {
    pub(super) fn exit_code(&self) -> Option<i32> {
        match self {
            Self::Exited(code) => *code,
            _ => None,
        }
    }
}

pub struct AcpTerminalBridge<Terminal> {
    pub launch:
        Box<dyn FnOnce(AcpTerminalCreateSpec, PathBuf, &mut Window, &mut App) -> Option<Terminal>>,
    pub read_output: fn(&Terminal) -> (String, AcpTerminalStatus),
    pub status: fn(&Terminal) -> AcpTerminalStatus,
    pub stop: fn(&Terminal),
}
