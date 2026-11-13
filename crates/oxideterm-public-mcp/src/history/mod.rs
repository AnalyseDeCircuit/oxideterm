//! Client-scoped, bounded terminal observations; no terminal transport ownership.
mod cursor;
mod store;
#[cfg(test)]
mod tests;
pub use store::*;

use crate::TerminalRef;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HistoryBuffer {
    Primary,
    Alternate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HistorySelection {
    All {},
    Screen {},
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HistoryRequest {
    Snapshot {
        terminal_ref: TerminalRef,
        buffer: HistoryBuffer,
        selection: HistorySelection,
        #[serde(default)]
        slot: u8,
        after_revision: Option<String>,
    },
    Page {
        cursor: String,
    },
    Release {
        snapshot_ref: String,
    },
    ReleaseSlot {
        slot: u8,
    },
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryArgs {
    pub request: HistoryRequest,
    pub line_limit: Option<usize>,
    pub byte_limit: Option<usize>,
}
impl HistoryArgs {
    pub fn validate(&self) -> bool {
        let limits = (1..=1000).contains(&self.lines()) && (4..=256 * 1024).contains(&self.bytes());
        match &self.request {
            HistoryRequest::Snapshot { slot, .. } => limits && *slot < 2,
            HistoryRequest::Page { cursor } => limits && valid_token(cursor),
            HistoryRequest::Release { snapshot_ref } => {
                self.line_limit.is_none() && self.byte_limit.is_none() && valid_token(snapshot_ref)
            }
            HistoryRequest::ReleaseSlot { slot } => {
                *slot < 2 && self.line_limit.is_none() && self.byte_limit.is_none()
            }
        }
    }
    pub fn lines(&self) -> usize {
        self.line_limit.unwrap_or(200)
    }
    pub fn bytes(&self) -> usize {
        self.byte_limit.unwrap_or(256 * 1024)
    }
    pub fn target(&self) -> String {
        match &self.request {
            HistoryRequest::Snapshot { terminal_ref, .. } => terminal_ref.to_string(),
            HistoryRequest::Page { cursor } => cursor.clone(),
            HistoryRequest::Release { snapshot_ref } => snapshot_ref.clone(),
            HistoryRequest::ReleaseSlot { slot } => format!("history_slot_{slot}"),
        }
    }
}
fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
}
