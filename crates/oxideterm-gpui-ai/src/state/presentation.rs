use std::{
    collections::{HashMap, HashSet},
    time::SystemTime,
};

use gpui::px;
use oxideterm_gpui_ui::virtual_list::TauriVirtualListSpec;
use oxideterm_workspace::{PaneId, TerminalSessionId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AiCompactionNoticePhase {
    Running,
    Done,
}

#[derive(Clone, Debug)]
pub struct AiCompactionNotice {
    pub conversation_id: String,
    pub phase: AiCompactionNoticePhase,
    pub compacted_count: Option<usize>,
    pub timestamp_ms: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AiChatInitializationError {
    pub message_key: &'static str,
    pub can_retry: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiChatFooterAction {
    Submit,
}

const AI_CHAT_LIST_ROW_HEIGHT_ESTIMATE: f32 = 80.0;
const AI_CHAT_LIST_VIRTUAL_OVERSCAN: usize = 8;

pub fn ai_chat_virtual_list_spec() -> TauriVirtualListSpec {
    // Tauri AI chat is a browser scroll container, while native uses GPUI List
    // for message virtualization. Keep the estimate/overscan explicit so this
    // variable-height list follows the same shared virtual-list contract as
    // tables, file panes, notifications, and event logs.
    TauriVirtualListSpec::new(
        px(AI_CHAT_LIST_ROW_HEIGHT_ESTIMATE),
        AI_CHAT_LIST_VIRTUAL_OVERSCAN,
    )
}

#[derive(Default)]
pub struct AiChatMessageSignatureCache {
    pub conversation_id: Option<String>,
    pub signatures: HashMap<String, u64>,
}

impl AiChatMessageSignatureCache {
    pub fn select_conversation(&mut self, conversation_id: &str) {
        if self.conversation_id.as_deref() == Some(conversation_id) {
            return;
        }
        self.conversation_id = Some(conversation_id.to_string());
        self.signatures.clear();
    }

    pub fn signature_for(&mut self, message_id: &str, compute: impl FnOnce() -> u64) -> u64 {
        if let Some(signature) = self.signatures.get(message_id) {
            return *signature;
        }
        let signature = compute();
        self.signatures.insert(message_id.to_string(), signature);
        signature
    }

    pub fn invalidate_message(&mut self, message_id: &str) {
        self.signatures.remove(message_id);
    }

    pub fn invalidate_all(&mut self) {
        self.signatures.clear();
    }

    pub fn needs_prune(&self, retained_count: usize) -> bool {
        self.signatures.len() > retained_count.saturating_add(32)
    }

    pub fn prune(&mut self, retained_message_ids: &HashSet<&str>) {
        self.signatures
            .retain(|message_id, _| retained_message_ids.contains(message_id.as_str()));
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiContextTokenBreakdown {
    pub system_instructions: usize,
    pub tool_definitions: usize,
    pub reserved_output: usize,
    pub messages: usize,
    pub tool_results: usize,
    pub total: usize,
    pub max_tokens: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiContextTokenBreakdownKey {
    pub conversation_id: Option<String>,
    pub conversation_fingerprint: u64,
    pub provider_id: String,
    pub model: String,
    pub max_tokens: usize,
    pub request_configuration_fingerprint: u64,
}

#[derive(Default)]
pub struct AiContextTokenBreakdownCache {
    pub key: Option<AiContextTokenBreakdownKey>,
    pub breakdown_without_draft: Option<AiContextTokenBreakdown>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiPreparedPromptUsage {
    pub conversation_id: String,
    pub last_user_message_id: Option<String>,
    pub provider_id: String,
    pub model: String,
    pub breakdown: AiContextTokenBreakdown,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AiPromptCacheStats {
    pub requests: u64,
    pub last_input_tokens: u64,
    pub last_cache_read_tokens: u64,
    pub last_cache_write_tokens: u64,
    pub total_input_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_write_tokens: u64,
}

impl AiPromptCacheStats {
    pub fn record(&mut self, input_tokens: u64, cache_read_tokens: u64, cache_write_tokens: u64) {
        if input_tokens == 0 {
            return;
        }
        let cache_read = cache_read_tokens.min(input_tokens);
        self.requests = self.requests.saturating_add(1);
        self.last_input_tokens = input_tokens;
        self.last_cache_read_tokens = cache_read;
        self.last_cache_write_tokens = cache_write_tokens;
        self.total_input_tokens = self.total_input_tokens.saturating_add(input_tokens);
        self.total_cache_read_tokens = self.total_cache_read_tokens.saturating_add(cache_read);
        self.total_cache_write_tokens = self
            .total_cache_write_tokens
            .saturating_add(cache_write_tokens);
    }

    pub fn last_hit_rate(&self) -> f32 {
        if self.last_input_tokens == 0 {
            return 0.0;
        }
        ((self.last_cache_read_tokens as f64 / self.last_input_tokens as f64) * 100.0)
            .clamp(0.0, 100.0) as f32
    }

    pub fn total_hit_rate(&self) -> f32 {
        if self.total_input_tokens == 0 {
            return 0.0;
        }
        ((self.total_cache_read_tokens as f64 / self.total_input_tokens as f64) * 100.0)
            .clamp(0.0, 100.0) as f32
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiModelSelectorScope {
    Sidebar,
    TerminalInline,
}

#[derive(Clone, Debug)]
pub struct AiToolExecutionRecord {
    pub record_id: String,
    pub conversation_id: String,
    pub assistant_message_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub argument_summary: String,
    pub resource_kind: Option<oxideterm_ai::StableResourceKind>,
    pub target_kind: Option<String>,
    pub risk: String,
    pub approval_source: Option<String>,
    pub execution_surface: String,
    pub visible_in_terminal: Option<bool>,
    pub status: String,
    pub success: Option<bool>,
    pub error_code: Option<String>,
    pub duration_ms: Option<u64>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct AiToolResultFact {
    pub fact_id: String,
    pub conversation_id: String,
    pub assistant_message_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub source_kind: String,
    pub summary: String,
    pub created_at: i64,
}

#[derive(Default)]
pub struct AiInlinePanelState {
    pub open: bool,
    pub target: Option<(PaneId, TerminalSessionId)>,
    pub prompt: String,
    pub response: String,
    pub error: Option<String>,
    pub loading: bool,
    pub copied: bool,
    pub prompt_focused: bool,
    pub has_api_key: Option<bool>,
    pub has_selection: bool,
    pub selection_context: String,
    pub generation: u64,
}

pub fn ai_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}
