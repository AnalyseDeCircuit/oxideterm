use crate::{AiExecutedToolResult, AiToolCall};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub const MAX_PARALLEL_READS: usize = 4;

pub fn parallel_read_call(call: &AiToolCall) -> bool {
    super::ToolExecutionDescription::for_call(call).read_only
}

#[derive(Default)]
pub struct ProgressGuard {
    repeated: HashMap<[u8; 32], (u32, [u8; 32])>,
    stalled: bool,
}

impl ProgressGuard {
    pub fn reset(&mut self) {
        self.repeated.clear();
        self.stalled = false;
    }

    pub fn stalled(&self) -> bool {
        self.stalled
    }

    pub fn observe(&mut self, call: &AiToolCall, result: &AiExecutedToolResult) -> bool {
        // Waiting and polling can legitimately repeat even when a command is not ready yet.
        if matches!(
            call.name.as_str(),
            "observe_terminal" | "wait_terminal_output" | "get_terminal_command_status"
        ) {
            return false;
        }
        // Hashes retain no command, credential, or file content.
        if result.success && call.name != "read_resource" {
            if !parallel_read_call(call) {
                self.reset();
            }
            return false;
        }
        let mut key = Sha256::new();
        key.update(call.name.as_bytes());
        let args = zeroize::Zeroizing::new(
            serde_json::from_str::<serde_json::Value>(&call.arguments)
                .map(|value| value.to_string())
                .unwrap_or_else(|_| call.arguments.clone()),
        );
        key.update(args.as_bytes());
        let key = key.finalize().into();
        let outcome = zeroize::Zeroizing::new(if result.success {
            result.output.clone()
        } else {
            result
                .envelope
                .get("error")
                .unwrap_or(&serde_json::Value::Null)
                .to_string()
        });
        let result_key: [u8; 32] = Sha256::digest(outcome.as_bytes()).into();
        if result.success
            && self
                .repeated
                .get(&key)
                .is_none_or(|entry| entry.1 != result_key)
        {
            self.reset();
            self.repeated.insert(key, (1, result_key));
            return false;
        }
        let entry = self.repeated.entry(key).or_insert((0, result_key));
        if entry.1 != result_key {
            *entry = (0, result_key);
        }
        entry.0 += 1;
        self.stalled |= !result.success && entry.0 >= 5;
        entry.0 == 3
    }
}
