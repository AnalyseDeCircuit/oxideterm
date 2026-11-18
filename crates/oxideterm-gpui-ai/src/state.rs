use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use gpui::{
    Context, Entity, FollowMode, ListAlignment, ListState, ScrollHandle, Subscription, Task, Timer,
    prelude::*,
};
#[cfg(test)]
use gpui::{IntoElement, ParentElement, Styled, Window, div};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_gpui_ui::{
    ConfirmDialogAction,
    confirm::{ModalFooterKeyAction, modal_footer_key_action},
    select::BrowserFocusOrigin,
    virtual_list::{
        TauriVirtualListSpec, VirtualListSignatureCache, sync_virtual_list_state_by_signatures,
        tauri_virtual_list_state,
    },
};
use oxideterm_settings_model::{
    AiMcpServerDraft, AiProviderKeyStatusDelivery, ai_mcp_auth_mode_value, ai_mcp_transport_value,
};
#[cfg(test)]
use std::sync::atomic::Ordering;

#[cfg(test)]
use crate::stream::AiStreamDeliveryEvent;
use crate::{
    knowledge::page::KnowledgePage,
    stream::{
        AiCompactionDelivery, AiCompactionDeliverySender, AiStreamDelivery, AiStreamDeliverySender,
    },
};

mod lazy;
mod presentation;
use lazy::LazyAiRagStore;
pub use presentation::*;
mod chat;
pub mod images;
pub mod sidebar_keys;
mod message_actions;
mod model;
pub use model::*;
pub use stream_apply::*;
mod stream_apply;
mod stream_records;
mod conversation_queue;
pub mod compaction;
pub mod launch;
pub mod skills;

pub mod agents;
pub mod history;
pub use crate::knowledge::page::KNOWLEDGE_DOCUMENT_PAGE_SIZE;
pub use history::HistoryStatus;

pub struct AiPendingUserQuestion {
    pub conversation_id: String,
    pub dispatch: Option<oxideterm_ai::agent::AgentDispatch>,
    pub sender: tokio::sync::oneshot::Sender<zeroize::Zeroizing<String>>,
}

impl AiPendingUserQuestion {
    fn active(&self) -> bool {
        !self.sender.is_closed()
            && self
                .dispatch
                .as_ref()
                .is_none_or(|guard| guard.check().is_ok())
    }
}

pub struct AiPendingCursorRequest {
    pub conversation_id: String,
    pub assistant_id: String,
    pub request: oxideterm_ai::CursorRequest,
    pub selections: Vec<Vec<String>>,
    pub response_tx: Option<oxideterm_ai::CursorResponseSender>,
}

impl Drop for AiPendingCursorRequest {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.selections);
        if let Some(sender) = self.response_tx.take() {
            let _ = sender.send(Ok(oxideterm_ai::CursorRequest::cancelled_response()));
        }
    }
}

/// Pending input is owned by this workspace and is never replayed from chat persistence.
pub struct AiQueuedChatTurn {
    pub id: String,
    pub images: Vec<oxideterm_ai::AiImageAttachment>,
    pub content: zeroize::Zeroizing<String>,
    pub context: Option<zeroize::Zeroizing<String>>,
    pub config: oxideterm_ai::AiChatStreamConfig,
    pub request_content: Option<zeroize::Zeroizing<String>>,
    pub task_system_prompt: Option<zeroize::Zeroizing<String>>,
    pub participant: Option<String>,
    pub skill: Option<(String, String)>,
}

pub struct AiPendingChatStream {
    pub launch_id: Option<String>,
    pub conversation_id: String,
    pub config: oxideterm_ai::AiChatStreamConfig,
    pub request_content: Option<String>,
    pub task_system_prompt: Option<String>,
    pub rag_system_prompt: Option<String>,
}

pub enum AiWorkspaceEvent {
    AcpAgentProbeDeliveryReady,
    AcpModelDiscoveryDeliveryReady,
    ChatStreamDeliveryReady,
    CompactionDeliveryReady,
    CompactionStateChanged,
    CredentialOperationReady,
    McpRuntimeChanged,
    ModelRefreshDeliveryReady,
    ProviderKeyStatusChanged,
    SelectorProviderStatusChanged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiChatPopover {
    ConversationList,
    Menu,
    Reasoning,
    Safety,
    Context,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AiChatConfirmKind {
    ClearAll,
    DeleteMessage { message_id: Arc<str> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiChatConfirmSnapshot {
    pub kind: AiChatConfirmKind,
    pub phase: oxideterm_gpui_ui::motion::ExitPhase,
    pub focused_action: Option<ConfirmDialogAction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AiChatConfirmEffect {
    ClearAll,
    DeleteMessage { message_id: Arc<str> },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiChatConfirmKeyAction {
    Cancel,
    Confirm,
    Handled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiChatConfirmOwnerKind {
    ClearAll,
    DeleteMessage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AiChatConfirmOwnerSnapshot {
    pub kind: AiChatConfirmOwnerKind,
    pub phase: oxideterm_gpui_ui::motion::ExitPhase,
}

/// Describes which workspace-owned AI surfaces can currently consume UI probes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AiWorkspaceVisibility {
    pub model_selector_surface: bool,
    pub settings_surface: bool,
}

impl AiWorkspaceVisibility {
    fn provider_status_visible(self) -> bool {
        self.model_selector_surface || self.settings_surface
    }
}

pub struct AiAcpAgentProbeIntent {
    pub agent_id: String,
    pub runtime_state: oxideterm_settings::AcpAgentRuntimeState,
    pub auth_status: oxideterm_settings::AcpAgentAuthStatus,
    pub last_error_kind: Option<String>,
}

pub struct AiAcpModelDiscoveryIntent {
    pub conversation_id: String,
    agent_id: String,
    config_options: Option<Vec<oxideterm_ai::AcpSessionConfigOption>>,
}

pub enum AiModelRefreshIntent {
    Updated {
        index: usize,
        provider_id: String,
        refresh: oxideterm_ai::ProviderModelRefresh,
    },
    MissingApiKey {
        provider_id: String,
    },
    Failed {
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiCredentialFailure {
    SaveProviderKey,
    RemoveProviderKey,
}

pub enum AiCredentialIntent {
    ProviderKeyStored { index: usize, provider_id: String },
    ProviderKeyRemoved,
    McpServerReady { config: serde_json::Value },
    McpServerSaveFailed { draft: AiMcpServerDraft },
    McpServerRemoved { server_id: String },
    Failed(AiCredentialFailure),
}

#[derive(Clone, Copy)]
enum AiProviderKeyOperation {
    Store { index: usize },
    Remove,
}

enum AiModelRefreshFailure {
    MissingApiKey,
    Failed(String),
}

fn model_refresh_failure_message(error: &anyhow::Error) -> String {
    // Preserve the proxy/TLS/HTTP classification while keeping the toast
    // bounded and single-line. Reqwest errors already drop the request URL.
    let message = format!("{error:#}").replace(['\r', '\n'], " ");
    message.chars().take(400).collect()
}

struct AiModelRefreshWorkerDelivery {
    index: usize,
    provider_id: String,
    generation: u64,
    result: Result<oxideterm_ai::ProviderModelRefresh, AiModelRefreshFailure>,
}

struct AiModelSelectorProbeDelivery {
    provider_id: String,
    generation: u64,
    online: bool,
}

struct AiAcpAgentProbeDelivery {
    epoch: u64,
    agent_id: String,
    result: AiAcpAgentProbeResult,
}

struct AiAcpAgentProbeResult {
    runtime_state: oxideterm_settings::AcpAgentRuntimeState,
    auth_status: oxideterm_settings::AcpAgentAuthStatus,
    last_error_kind: Option<String>,
}

struct AiAcpModelDiscoveryDelivery {
    epoch: u64,
    conversation_id: String,
    agent_id: String,
    config_options: Option<Vec<oxideterm_ai::AcpSessionConfigOption>>,
}

enum AiTerminalInlineDelivery {
    KeyStatus { generation: u64, has_key: bool },
    Content { generation: u64, chunk: String },
    Done { generation: u64 },
    Error { generation: u64, message: String },
}

const AI_TERMINAL_INLINE_DELIVERY_BUDGET: oxideterm_workspace::delivery::DeliveryBudget =
    oxideterm_workspace::delivery::DeliveryBudget::new(128, Duration::from_millis(4));
const AI_CHAT_STREAM_DELIVERY_BUDGET: oxideterm_workspace::delivery::DeliveryBudget =
    oxideterm_workspace::delivery::DeliveryBudget::new(256, Duration::from_millis(4));

/// Owns AI worker delivery slices as they move out of the workspace root.
pub struct AiWorkspaceEntity {
    task_runtime: Arc<tokio::runtime::Runtime>,
    loaded_skill_versions: HashMap<String, HashMap<String, String>>,
    _rag_cache_cleanup: Task<()>,
    key_store: oxideterm_ai::AiProviderKeyStore,
    model_ui: AiModelWorkspaceState,
    chat_ui: AiChatWorkspaceState,
    knowledge_window_activation_subscription: Option<Subscription>,
    visibility: AiWorkspaceVisibility,
    provider_key_operation_tasks: HashMap<String, Task<()>>,
    pending_provider_key_removals: HashSet<String>,
    credential_intents: VecDeque<AiCredentialIntent>,
    mcp_save_task: Option<Task<()>>,
    mcp_runtime_tasks: HashMap<String, Task<()>>,
    mcp_status_tick_task: Option<Task<()>>,
    chat_confirm: Option<AiChatConfirmKind>,
    chat_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    chat_confirm_focused_action: Option<ConfirmDialogAction>,
    chat_confirm_exit_task: Option<Task<()>>,
    model_refresh_generations: HashMap<String, u64>,
    refreshing_models: HashSet<String>,
    model_refresh_tx:
        oxideterm_workspace::delivery::ActiveDeliverySender<AiModelRefreshWorkerDelivery>,
    model_refresh_rx: std::sync::mpsc::Receiver<AiModelRefreshWorkerDelivery>,
    model_refresh_pending: usize,
    next_model_refresh_generation: u64,
    model_refresh_intents: VecDeque<AiModelRefreshIntent>,
    provider_key_status: HashMap<String, bool>,
    provider_key_status_pending: HashSet<String>,
    provider_key_status_tx:
        oxideterm_workspace::delivery::ActiveDeliverySender<AiProviderKeyStatusDelivery>,
    provider_key_status_rx: std::sync::mpsc::Receiver<AiProviderKeyStatusDelivery>,
    selector_provider_online: HashMap<String, bool>,
    selector_probe_generations: HashMap<String, u64>,
    selector_probe_tx:
        oxideterm_workspace::delivery::ActiveDeliverySender<AiModelSelectorProbeDelivery>,
    selector_probe_rx: std::sync::mpsc::Receiver<AiModelSelectorProbeDelivery>,
    selector_probe_pending: usize,
    next_selector_probe_generation: u64,
    acp_agent_probe_pending: HashSet<String>,
    acp_agent_epochs: HashMap<String, u64>,
    acp_probe_tasks: HashMap<String, tokio::task::AbortHandle>,
    acp_discovery_tasks: HashMap<(String, String), tokio::task::AbortHandle>,
    acp_agent_probe_tx:
        oxideterm_workspace::delivery::ActiveDeliverySender<AiAcpAgentProbeDelivery>,
    acp_agent_probe_rx: std::sync::mpsc::Receiver<AiAcpAgentProbeDelivery>,
    acp_agent_probe_intents: VecDeque<AiAcpAgentProbeIntent>,
    acp_model_options: HashMap<(String, String), Vec<oxideterm_ai::AcpSessionConfigOption>>,
    acp_model_discovery_pending: HashSet<(String, String)>,
    acp_model_discovery_tx:
        oxideterm_workspace::delivery::ActiveDeliverySender<AiAcpModelDiscoveryDelivery>,
    acp_model_discovery_rx: std::sync::mpsc::Receiver<AiAcpModelDiscoveryDelivery>,
    acp_model_discovery_intents: VecDeque<AiAcpModelDiscoveryIntent>,
    rag_store: LazyAiRagStore,
    knowledge_page: Entity<KnowledgePage>,
    terminal_inline_panel: AiInlinePanelState,
    terminal_inline_tx:
        oxideterm_workspace::delivery::ActiveDeliverySender<AiTerminalInlineDelivery>,
    terminal_inline_rx: std::sync::mpsc::Receiver<AiTerminalInlineDelivery>,
    terminal_inline_stream_task: Option<tokio::task::JoinHandle<()>>,
    chat_stream_generation: u64,
    chat_stream_runs: HashMap<u64, AiChatStreamRun>,
    pub agents: agents::AiAgentWorkspace,
    chat_stream_tx: AiStreamDeliverySender,
    chat_stream_rx: std::sync::mpsc::Receiver<AiStreamDelivery>,
    chat_stream_batch: crate::stream::AiChatDeliveryBatch,
    chat_stream_frame: Option<Rc<crate::stream::AiChatStreamFrameSource>>,
    chat_window_delivery: Option<crate::stream::AiChatWindowDelivery>,
    chat_window_ticket: Option<u64>,
    next_chat_window_ticket: u64,
    conversation_state: oxideterm_ai::AiChatState,
    pub history: history::AiHistoryState,
    chat_initialized: bool,
    chat_initialization_error: Option<AiChatInitializationError>,
    safety_modes_by_conversation: HashMap<String, oxideterm_gpui_ui::ai::AiSafetyMode>,
    loading_conversations: HashSet<String>,
    pub queued_chat_turns: HashMap<String, VecDeque<AiQueuedChatTurn>>,
    pub chat_launches: HashMap<String, String>,
    chat_drafts: HashMap<String, zeroize::Zeroizing<String>>,
    chat_image_drafts: HashMap<String, Vec<oxideterm_ai::AiImageAttachment>>,
    pub pending_image_requests: HashMap<String, AiPendingChatStream>,
    pub pending_user_questions: HashMap<(u64, String), AiPendingUserQuestion>,
    pub pending_cursor_requests: HashMap<(u64, String), AiPendingCursorRequest>,
    pending_tool_approvals: HashMap<(u64, String), tokio::sync::oneshot::Sender<bool>>,
    // Approval previews are runtime-only; conversation history never owns execution payloads.
    pub tool_approval_previews: HashMap<(u64, String), zeroize::Zeroizing<String>>,
    pending_acp_permission_choices:
        HashMap<(u64, String), tokio::sync::oneshot::Sender<Option<String>>>,
    pending_tool_candidate_selections:
        HashMap<(u64, String), tokio::sync::oneshot::Sender<Option<usize>>>,
    pending_tool_candidate_counts: HashMap<(u64, String), usize>,
    // Runtime evidence transitions are implemented beside stream application,
    // but these collections remain physically owned by this Entity.
    pub tool_execution_records: VecDeque<AiToolExecutionRecord>,
    pub tool_result_facts: VecDeque<AiToolResultFact>,
    mcp_registry: oxideterm_ai::McpRegistry,
    compaction_tx: AiCompactionDeliverySender,
    compaction_rx: std::sync::mpsc::Receiver<AiCompactionDelivery>,
    compaction_deliveries: VecDeque<AiCompactionDelivery>,
    compacting_conversations: HashSet<String>,
    compaction_notice: Option<AiCompactionNotice>,
}

struct AiChatStreamRun {
    conversation_id: String,
    assistant_id: String,
    agent: Option<oxideterm_ai::agent::AgentRunRef>,
    child: bool,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl AiWorkspaceEntity {
    /// Returns the protected provider key store without copying key material.
    pub(crate) fn task_runtime(&self) -> &Arc<tokio::runtime::Runtime> {
        &self.task_runtime
    }

    pub fn key_store(&self) -> &oxideterm_ai::AiProviderKeyStore {
        &self.key_store
    }

    /// Returns provider and model presentation state from its sole owner.
    pub fn model_ui(&self) -> &AiModelWorkspaceState {
        &self.model_ui
    }

    /// Returns chat presentation state from its sole owner.
    pub fn chat_ui(&self) -> &AiChatWorkspaceState {
        &self.chat_ui
    }

    pub fn chat_images_mut(&mut self) -> &mut Vec<oxideterm_ai::AiImageAttachment> {
        &mut self.chat_ui.images
    }

    pub fn set_chat_image_import(&mut self, task: Option<Task<()>>) {
        self.chat_ui.image_import = task;
    }

    pub fn set_chat_image_preview(&mut self, image: Option<(String, Arc<gpui::Image>)>) {
        if image.is_none() {
            self.chat_ui.image_preview_task = None;
        }
        self.chat_ui.image_preview = image;
        self.chat_ui
            .message_signature_cache
            .borrow_mut()
            .invalidate_all();
        self.chat_ui
            .message_list_state
            .remeasure_items(0..self.chat_ui.message_list_state.item_count());
    }

    pub fn set_chat_image_preview_task(&mut self, task: Task<()>) {
        self.chat_ui.image_preview_task = Some(task);
    }

    pub fn sidebar_keyboard_target_focused(&self) -> bool {
        // Keep every editable AI surface in the same root key-routing contract.
        self.chat_ui.input_focused
            || self.chat_ui.footer_focus.is_some()
            || (self.chat_ui.renaming_conversation_id.is_some()
                && self.chat_ui.renaming_conversation_focused)
            || (self.chat_ui.editing_message_id.is_some() && self.chat_ui.editing_message_focused)
            || self.model_ui.selector_search_focused
    }

    pub fn configure_chat_surface(
        &mut self,
        sidebar_width: f32,
        overlay_window_size: Option<(f32, f32)>,
    ) {
        self.chat_ui.sidebar_width = sidebar_width;
        self.chat_ui.overlay_window_size = overlay_window_size;
    }

    pub fn set_chat_draft(&mut self, draft: String) {
        self.chat_ui.draft = draft;
    }

    pub fn focus_chat_input(&mut self) {
        self.chat_ui.input_focused = true;
        self.chat_ui.footer_focus = None;
    }

    pub fn blur_chat_input(&mut self, suppress_autocomplete: bool) {
        self.chat_ui.input_focused = false;
        self.chat_ui.footer_focus = None;
        if suppress_autocomplete {
            self.chat_ui.autocomplete_suppressed = true;
        }
    }

    pub fn set_chat_footer_focus(&mut self, focus: Option<AiChatFooterAction>) {
        self.chat_ui.input_focused = focus.is_none();
        self.chat_ui.footer_focus = focus;
    }

    pub fn clear_chat_footer_focus(&mut self) {
        self.chat_ui.footer_focus = None;
    }

    pub fn set_chat_popover_open(&mut self, popover: AiChatPopover, open: bool) {
        match popover {
            AiChatPopover::ConversationList => self.chat_ui.conversation_list_open = open,
            AiChatPopover::Menu => self.chat_ui.menu_open = open,
            AiChatPopover::Reasoning => self.chat_ui.reasoning_menu_open = open,
            AiChatPopover::Safety => self.chat_ui.safety_menu_open = open,
            AiChatPopover::Context => self.chat_ui.context_popover_open = open,
        }
    }

    pub fn close_chat_popovers(&mut self) {
        self.chat_ui.conversation_list_open = false;
        self.chat_ui.menu_open = false;
        self.chat_ui.reasoning_menu_open = false;
        self.chat_ui.safety_menu_open = false;
        self.chat_ui.context_popover_open = false;
        self.clear_conversation_rename();
    }

    pub fn toggle_chat_context(&mut self) {
        self.chat_ui.include_context = !self.chat_ui.include_context;
        if !self.chat_ui.include_context {
            self.chat_ui.include_all_panes = false;
        }
    }

    pub fn toggle_chat_all_panes(&mut self) {
        self.chat_ui.include_all_panes = !self.chat_ui.include_all_panes;
    }

    pub fn replace_chat_input(
        &mut self,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
    ) -> bool {
        if !self.chat_ui.input_focused {
            return false;
        }
        replace_utf16(&mut self.chat_ui.draft, replacement_range, text);
        self.chat_ui.autocomplete_suppressed = false;
        self.chat_ui.autocomplete_index = 0;
        true
    }

    pub fn replace_message_edit(
        &mut self,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
    ) -> bool {
        if !self.chat_ui.editing_message_focused {
            return false;
        }
        replace_utf16(
            &mut self.chat_ui.editing_message_draft,
            replacement_range,
            text,
        );
        true
    }

    pub fn replace_conversation_rename(
        &mut self,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
    ) -> bool {
        if !self.chat_ui.renaming_conversation_focused {
            return false;
        }
        replace_utf16(
            &mut self.chat_ui.renaming_conversation_draft,
            replacement_range,
            text,
        );
        true
    }

    pub fn chat_autocomplete_items(
        &self,
        backend: oxideterm_settings::AiActiveBackend,
        skill_catalog: impl FnOnce() -> Vec<(String, String)>,
    ) -> Vec<oxideterm_ai::AiAutocompleteCandidate> {
        use oxideterm_ai::{AiAutocompleteCandidate, AiAutocompleteKind, ai_autocomplete_candidates, ai_input_token_at_cursor};

        if !self.chat_ui().input_focused || self.chat_ui().autocomplete_suppressed {
            return Vec::new();
        }
        let draft = &self.chat_ui().draft;
        let mut candidates = ai_autocomplete_candidates(draft, draft.len());
        let input_token = ai_input_token_at_cursor(draft, draft.len());
        if input_token.token_type != Some(oxideterm_ai::AiInputTokenType::Slash) {
            return candidates;
        }
        let partial = input_token.partial.to_lowercase();
        for (id, description) in skill_catalog() {
            if id.to_lowercase().starts_with(&partial)
                && !candidates.iter().any(|candidate| candidate.name == id)
            {
                candidates.push(AiAutocompleteCandidate {
                    kind: AiAutocompleteKind::Slash,
                    name: id,
                    description,
                    description_is_i18n_key: false,
                    accepts_value: true,
                });
            }
        }
        if backend != oxideterm_settings::AiActiveBackend::Acp {
            return candidates;
        }
        let Some(session) = self
            .conversation_state()
            .active_conversation()
            .and_then(ai_acp_session_state)
        else {
            return candidates;
        };
        for command in session.available_commands {
            let Some(name) = command.get("name").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if !name.to_lowercase().starts_with(&partial)
                || candidates.iter().any(|candidate| candidate.name == name)
            {
                continue;
            }
            candidates.push(AiAutocompleteCandidate {
                kind: AiAutocompleteKind::Slash,
                name: name.to_string(),
                description: command
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                description_is_i18n_key: false,
                accepts_value: command.get("input").is_some_and(|input| !input.is_null()),
            });
        }
        candidates
    }

    pub fn apply_chat_autocomplete(&mut self, candidate: &oxideterm_ai::AiAutocompleteCandidate) {
        self.chat_ui.draft = oxideterm_ai::apply_ai_autocomplete_candidate(
            &self.chat_ui.draft,
            self.chat_ui.draft.len(),
            candidate,
        );
        self.chat_ui.autocomplete_index = 0;
        self.chat_ui.autocomplete_suppressed = true;
    }

    pub fn move_chat_autocomplete(&mut self, delta: isize, item_count: usize) {
        if item_count == 0 {
            self.chat_ui.autocomplete_index = 0;
            return;
        }
        self.chat_ui.autocomplete_index = if delta.is_negative() {
            (self.chat_ui.autocomplete_index + item_count - 1) % item_count
        } else {
            (self.chat_ui.autocomplete_index + 1) % item_count
        };
    }

    pub fn suppress_chat_autocomplete(&mut self) {
        self.chat_ui.autocomplete_suppressed = true;
    }

    pub fn pop_chat_draft(&mut self) -> bool {
        let changed = self.chat_ui.draft.pop().is_some()
            || self.chat_ui.autocomplete_suppressed
            || self.chat_ui.autocomplete_index != 0;
        self.chat_ui.autocomplete_suppressed = false;
        self.chat_ui.autocomplete_index = 0;
        changed
    }

    pub fn pop_conversation_rename(&mut self) -> bool {
        self.chat_ui.renaming_conversation_draft.pop().is_some()
    }

    pub fn focus_conversation_rename(&mut self) {
        self.chat_ui.renaming_conversation_focused = true;
        self.chat_ui.input_focused = false;
        self.chat_ui.footer_focus = None;
        self.chat_ui.editing_message_focused = false;
    }

    pub fn begin_conversation_rename(&mut self, conversation_id: String, title: String) {
        self.chat_ui.renaming_conversation_id = Some(conversation_id);
        self.chat_ui.renaming_conversation_draft = title;
        self.focus_conversation_rename();
    }

    pub fn clear_conversation_rename(&mut self) {
        self.chat_ui.renaming_conversation_id = None;
        self.chat_ui.renaming_conversation_draft.clear();
        self.chat_ui.renaming_conversation_focused = false;
    }

    pub fn pop_message_edit(&mut self) -> bool {
        self.chat_ui.editing_message_draft.pop().is_some()
    }

    pub fn push_message_edit_newline(&mut self) {
        self.chat_ui.editing_message_draft.push('\n');
    }

    pub fn blur_message_edit(&mut self) {
        self.chat_ui.editing_message_focused = false;
    }

    pub fn focus_message_edit(&mut self) {
        self.chat_ui.editing_message_focused = true;
        self.chat_ui.input_focused = false;
    }

    pub fn begin_message_edit(&mut self, message_id: String, content: String) {
        self.chat_ui.editing_message_id = Some(message_id);
        self.chat_ui.editing_message_draft = content;
        self.chat_ui.editing_message_scroll = ScrollHandle::new();
        self.chat_ui.editing_message_caret_position.set(None);
        self.focus_message_edit();
    }

    pub fn clear_message_edit(&mut self) {
        self.chat_ui.editing_message_id = None;
        self.chat_ui.editing_message_draft.clear();
        self.chat_ui.editing_message_caret_position.set(None);
        self.chat_ui.editing_message_focused = false;
    }

    pub fn reset_chat_for_conversation_selection(&mut self) {
        self.chat_ui.image_import = None;
        self.chat_ui.image_preview = None;
        self.chat_ui.image_preview_task = None;
        self.chat_ui.conversation_list_open = false;
        self.chat_ui.menu_open = false;
        self.chat_ui.safety_menu_open = false;
        self.clear_conversation_rename();
        self.clear_message_edit();
        self.clear_chat_expansions();
        self.blur_chat_input(false);
    }

    pub fn reset_chat_after_conversation_delete(&mut self, has_conversations: bool) {
        self.clear_chat_expansions();
        self.clear_conversation_rename();
        self.chat_ui.conversation_list_open = has_conversations;
        self.chat_ui.menu_open = false;
    }

    pub fn reset_chat_for_new_conversation(&mut self) {
        self.chat_ui.images.clear();
        self.chat_ui.image_import = None;
        self.chat_ui.image_preview = None;
        self.chat_ui.image_preview_task = None;
        self.chat_ui.conversation_list_open = false;
        self.chat_ui.menu_open = false;
        self.clear_conversation_rename();
        self.chat_ui.draft.clear();
        self.chat_ui.input_focused = false;
        self.chat_ui.autocomplete_index = 0;
        self.chat_ui.autocomplete_suppressed = false;
    }

    pub fn reset_chat_message_list(&mut self) {
        self.chat_ui.message_list_state =
            tauri_virtual_list_state(0, ListAlignment::Top, ai_chat_virtual_list_spec());
        self.chat_ui
            .message_list_cache
            .replace(VirtualListSignatureCache::default());
        self.chat_ui
            .message_signature_cache
            .borrow_mut()
            .invalidate_all();
    }

    pub fn sync_chat_message_list(
        &mut self,
        conversation_id: &str,
        signatures: &[u64],
        spec: TauriVirtualListSpec,
    ) {
        let chat = &mut self.chat_ui;
        let mut cache = chat.message_list_cache.borrow_mut();
        let list_was_reset = sync_virtual_list_state_by_signatures(
            &mut chat.message_list_state,
            &mut cache,
            conversation_id,
            signatures,
            ListAlignment::Top,
            spec.overdraw(),
        );
        if list_was_reset {
            chat.message_list_state.set_follow_mode(FollowMode::Tail);
        }
    }

    pub fn clear_chat_expansions(&mut self) {
        self.chat_ui.thinking_expansion_state.clear();
        self.chat_ui.tool_call_expansion_state.clear();
    }

    pub fn remove_thinking_expansion(&mut self, message_id: &str) {
        self.chat_ui.thinking_expansion_state.remove(message_id);
    }

    pub fn clear_chat_input_after_submit(&mut self) {
        self.chat_ui.images.clear();
        self.chat_ui.image_import = None;
        self.chat_ui.image_preview = None;
        self.chat_ui.draft.clear();
        self.chat_ui.autocomplete_index = 0;
        self.chat_ui.autocomplete_suppressed = false;
        self.chat_ui.include_context = false;
        self.chat_ui.include_all_panes = false;
    }

    pub fn open_standard_chat_confirm(&mut self, kind: AiStandardConfirmKind) {
        match kind {
            AiStandardConfirmKind::Safety => {
                self.chat_ui.safety_confirm_open = true;
                self.chat_ui.safety_confirm_presence.reopen();
            }
            AiStandardConfirmKind::Summarize => {
                self.chat_ui.summarize_confirm_open = true;
                self.chat_ui.summarize_confirm_presence.reopen();
            }
        }
    }

    pub fn begin_standard_chat_confirm_exit(&mut self, kind: AiStandardConfirmKind) -> Option<u64> {
        match kind {
            AiStandardConfirmKind::Safety => self.chat_ui.safety_confirm_presence.begin_exit(),
            AiStandardConfirmKind::Summarize => {
                self.chat_ui.summarize_confirm_presence.begin_exit()
            }
        }
    }

    pub fn finish_standard_chat_confirm_exit(
        &mut self,
        kind: AiStandardConfirmKind,
        generation: u64,
    ) -> bool {
        let finished = match kind {
            AiStandardConfirmKind::Safety => {
                self.chat_ui.safety_confirm_presence.finish_exit(generation)
            }
            AiStandardConfirmKind::Summarize => self
                .chat_ui
                .summarize_confirm_presence
                .finish_exit(generation),
        };
        if finished {
            match kind {
                AiStandardConfirmKind::Safety => self.chat_ui.safety_confirm_open = false,
                AiStandardConfirmKind::Summarize => self.chat_ui.summarize_confirm_open = false,
            }
        }
        finished
    }

    pub fn set_model_switch_warning(&mut self, percentage: Option<usize>) {
        self.chat_ui.model_switch_warning_percentage = percentage;
    }

    pub fn set_prepared_prompt_usage(&mut self, usage: AiPreparedPromptUsage) {
        self.chat_ui.prepared_prompt_usage = Some(usage);
    }

    pub fn show_context_trim_notice(&mut self, count: usize) -> u64 {
        self.chat_ui.context_trim_notice_count = Some(count);
        self.chat_ui.context_trim_notice_sequence =
            self.chat_ui.context_trim_notice_sequence.saturating_add(1);
        self.chat_ui.context_trim_notice_sequence
    }

    pub fn clear_context_trim_notice(&mut self, sequence: u64) -> bool {
        if self.chat_ui.context_trim_notice_sequence != sequence {
            return false;
        }
        self.chat_ui.context_trim_notice_count = None;
        true
    }

    pub fn set_chat_sidebar_width(&mut self, width: f32) {
        self.chat_ui.sidebar_width = width;
    }

    pub fn set_chat_sidebar_resizing(&mut self, resizing: bool) {
        self.chat_ui.sidebar_resizing = resizing;
    }

    pub fn finish_chat_sidebar_resize(&mut self) -> f32 {
        self.chat_ui.sidebar_resizing = false;
        self.chat_ui.sidebar_width
    }

    pub fn replace_overlay_window_size(&mut self, size: (f32, f32)) -> Option<(f32, f32)> {
        self.chat_ui.overlay_window_size.replace(size)
    }

    /// Retains workspace-window observers for exactly the AI Entity lifetime.
    pub fn retain_window_observers(
        &mut self,
        overlay_bounds: Subscription,
        knowledge_activation: Subscription,
    ) {
        self.chat_ui.overlay_window_bounds_subscription = Some(overlay_bounds);
        self.knowledge_window_activation_subscription = Some(knowledge_activation);
    }

    fn anchor_chat_before_disclosure(&self) {
        // Manual disclosures grow below the clicked header; they are not new
        // messages and must not pull the viewport along with the list tail.
        let list = &self.chat_ui.message_list_state;
        let top = list.logical_scroll_top();
        list.pause_following_tail();
        list.scroll_to(top);
    }

    pub fn toggle_thinking_expansion(&mut self, key: String, default_expanded: bool) {
        self.anchor_chat_before_disclosure();
        let current = self
            .chat_ui
            .thinking_expansion_state
            .get(&key)
            .copied()
            .unwrap_or(default_expanded);
        self.chat_ui.thinking_expansion_state.insert(key, !current);
    }

    pub fn toggle_tool_call_expansion(&mut self, key: String) {
        self.anchor_chat_before_disclosure();
        if !self.chat_ui.tool_call_expansion_state.remove(&key) {
            self.chat_ui.tool_call_expansion_state.insert(key);
        }
    }

    pub fn model_selector_is_open(&self, scope: AiModelSelectorScope) -> bool {
        self.model_ui.selector_open && self.model_ui.selector_scope == Some(scope)
    }

    pub fn model_selector_open(&self) -> bool {
        self.model_ui.selector_open
    }

    pub fn model_selector_scope(&self) -> Option<AiModelSelectorScope> {
        self.model_ui.selector_scope
    }

    pub fn model_selector_focus_origin(&self) -> Option<BrowserFocusOrigin> {
        self.model_ui.selector_focus_origin
    }

    pub fn set_model_selector_focus_origin(&mut self, origin: Option<BrowserFocusOrigin>) {
        self.model_ui.selector_focus_origin = origin;
    }

    pub fn model_selector_search_focused(&self) -> bool {
        self.model_ui.selector_search_focused
    }

    pub fn set_model_selector_search_focused(&mut self, focused: bool) {
        self.model_ui.selector_search_focused = focused;
    }

    pub fn model_selector_search_query(&self) -> &str {
        &self.model_ui.selector_search_query
    }

    pub fn replace_model_selector_search(
        &mut self,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
    ) {
        replace_utf16(
            &mut self.model_ui.selector_search_query,
            replacement_range,
            text,
        );
        self.model_ui.selector_highlighted_model = None;
    }

    pub fn pop_model_selector_search(&mut self) -> bool {
        let changed = self.model_ui.selector_search_query.pop().is_some()
            || self.model_ui.selector_highlighted_model.take().is_some();
        changed
    }

    pub fn clear_model_selector_search(&mut self) -> bool {
        let changed = !self.model_ui.selector_search_query.is_empty()
            || self.model_ui.selector_highlighted_model.is_some();
        self.model_ui.selector_search_query.clear();
        self.model_ui.selector_highlighted_model = None;
        changed
    }

    pub fn set_model_selector_open(&mut self, scope: AiModelSelectorScope, open: bool) {
        self.model_ui.selector_open = open;
        self.model_ui.selector_scope = open.then_some(scope);
        self.model_ui.selector_search_focused = open;
        self.model_ui.selector_highlighted_model = None;
    }

    pub fn close_model_selector(&mut self) {
        self.model_ui.selector_open = false;
        self.model_ui.selector_scope = None;
        self.model_ui.selector_focus_origin = None;
        self.model_ui.selector_search_focused = false;
        self.model_ui.selector_search_query.clear();
        self.model_ui.selector_highlighted_model = None;
    }

    pub fn model_selector_provider_expanded(&self, provider_id: &str) -> bool {
        self.model_ui
            .selector_expanded_providers
            .contains(provider_id)
    }

    pub fn expand_model_selector_provider(&mut self, provider_id: String) {
        self.model_ui
            .selector_expanded_providers
            .insert(provider_id);
    }

    pub fn toggle_model_selector_provider(&mut self, provider_id: String) -> bool {
        let expanded = if self
            .model_ui
            .selector_expanded_providers
            .remove(&provider_id)
        {
            false
        } else {
            self.model_ui
                .selector_expanded_providers
                .insert(provider_id);
            true
        };
        self.model_ui.selector_highlighted_model = None;
        expanded
    }

    pub fn model_selector_highlight(&self) -> Option<&(String, String)> {
        self.model_ui.selector_highlighted_model.as_ref()
    }

    pub fn set_model_selector_highlight(&mut self, highlighted: Option<(String, String)>) -> bool {
        if self.model_ui.selector_highlighted_model == highlighted {
            return false;
        }
        self.model_ui.selector_highlighted_model = highlighted;
        true
    }

    pub fn move_model_selector_highlight(&mut self, rows: &[(String, String)], delta: isize) {
        if rows.is_empty() {
            self.model_ui.selector_highlighted_model = None;
            return;
        }
        let current = self
            .model_ui
            .selector_highlighted_model
            .as_ref()
            .and_then(|highlighted| rows.iter().position(|row| row == highlighted));
        let next = match (current, delta.is_negative()) {
            (Some(index), false) => (index + delta as usize).min(rows.len() - 1),
            (Some(index), true) => index.saturating_sub(delta.unsigned_abs()),
            (None, false) => 0,
            (None, true) => rows.len() - 1,
        };
        self.model_ui.selector_highlighted_model = rows.get(next).cloned();
    }

    pub fn set_model_selector_highlight_edge(&mut self, rows: &[(String, String)], last: bool) {
        self.model_ui.selector_highlighted_model = if last {
            rows.last().cloned()
        } else {
            rows.first().cloned()
        };
    }

    pub fn model_selector_status_signature_matches(&self, signature: u64) -> bool {
        self.model_ui.selector_status_signature == Some(signature)
    }

    pub fn set_model_selector_status_signature(&mut self, signature: Option<u64>) {
        self.model_ui.selector_status_signature = signature;
    }

    pub fn new(
        task_runtime: Arc<tokio::runtime::Runtime>,
        key_store: oxideterm_ai::AiProviderKeyStore,
        cx: &mut Context<Self>,
    ) -> Self {
        let (model_refresh_tx, model_refresh_rx) =
            oxideterm_workspace::delivery::ActiveDeliverySender::channel();
        let (provider_key_status_tx, provider_key_status_rx) =
            oxideterm_workspace::delivery::ActiveDeliverySender::channel();
        let (selector_probe_tx, selector_probe_rx) =
            oxideterm_workspace::delivery::ActiveDeliverySender::channel();
        let (acp_agent_probe_tx, acp_agent_probe_rx) =
            oxideterm_workspace::delivery::ActiveDeliverySender::channel();
        let (acp_model_discovery_tx, acp_model_discovery_rx) =
            oxideterm_workspace::delivery::ActiveDeliverySender::channel();
        let (terminal_inline_tx, terminal_inline_rx) =
            oxideterm_workspace::delivery::ActiveDeliverySender::channel();
        let (chat_stream_tx, chat_stream_rx) =
            oxideterm_workspace::delivery::ActiveDeliverySender::channel();
        let (compaction_tx, compaction_rx) =
            oxideterm_workspace::delivery::ActiveDeliverySender::channel();
        let mcp_registry = oxideterm_ai::McpRegistry::new(key_store.clone());
        let rag_store = LazyAiRagStore::default();
        let knowledge_page = cx.new(|_| {
            let store = rag_store.clone();
            KnowledgePage::new(
                Rc::new(move || store.get()),
                task_runtime.clone(),
                key_store.clone(),
            )
        });
        let mut entity = Self {
            task_runtime,
            loaded_skill_versions: HashMap::new(),
            // Entity ownership cancels cleanup; the weak handle cannot keep a closed workspace alive.
            _rag_cache_cleanup: cx.spawn(async move |weak, cx| {
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(60))
                        .await;
                    let Ok(store) = weak.update(cx, |ai, _| ai.rag_store.clone()) else {
                        break;
                    };
                    cx.background_executor()
                        .spawn(async move {
                            store.release_idle_index();
                        })
                        .await;
                }
            }),
            model_ui: AiModelWorkspaceState::new(),
            chat_ui: AiChatWorkspaceState::new(360.0, None),
            knowledge_window_activation_subscription: None,
            key_store,
            visibility: AiWorkspaceVisibility::default(),
            provider_key_operation_tasks: HashMap::new(),
            pending_provider_key_removals: HashSet::new(),
            credential_intents: VecDeque::new(),
            mcp_save_task: None,
            mcp_runtime_tasks: HashMap::new(),
            mcp_status_tick_task: None,
            chat_confirm: None,
            chat_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            chat_confirm_focused_action: None,
            chat_confirm_exit_task: None,
            model_refresh_generations: HashMap::new(),
            refreshing_models: HashSet::new(),
            model_refresh_tx,
            model_refresh_rx,
            model_refresh_pending: 0,
            next_model_refresh_generation: 0,
            model_refresh_intents: VecDeque::new(),
            provider_key_status: HashMap::new(),
            provider_key_status_pending: HashSet::new(),
            provider_key_status_tx,
            provider_key_status_rx,
            selector_provider_online: HashMap::new(),
            selector_probe_generations: HashMap::new(),
            selector_probe_tx,
            selector_probe_rx,
            selector_probe_pending: 0,
            next_selector_probe_generation: 0,
            acp_agent_probe_pending: HashSet::new(),
            acp_agent_epochs: HashMap::new(),
            acp_probe_tasks: HashMap::new(),
            acp_discovery_tasks: HashMap::new(),
            acp_agent_probe_tx,
            acp_agent_probe_rx,
            acp_agent_probe_intents: VecDeque::new(),
            acp_model_options: HashMap::new(),
            acp_model_discovery_pending: HashSet::new(),
            acp_model_discovery_tx,
            acp_model_discovery_rx,
            acp_model_discovery_intents: VecDeque::new(),
            rag_store,
            knowledge_page,
            terminal_inline_panel: AiInlinePanelState::default(),
            terminal_inline_tx,
            terminal_inline_rx,
            terminal_inline_stream_task: None,
            chat_stream_generation: 0,
            chat_stream_runs: HashMap::new(),
            agents: agents::AiAgentWorkspace::new(
                cx.default_global::<agents::AiAgentServices>().clone(),
            ),
            chat_stream_tx,
            chat_stream_rx,
            chat_stream_batch: crate::stream::AiChatDeliveryBatch::default(),
            chat_stream_frame: None,
            chat_window_delivery: None,
            chat_window_ticket: None,
            next_chat_window_ticket: 0,
            conversation_state: oxideterm_ai::AiChatState::default(),
            history: history::AiHistoryState::default(),
            chat_initialized: false,
            chat_initialization_error: None,
            safety_modes_by_conversation: HashMap::new(),
            loading_conversations: HashSet::new(),
            queued_chat_turns: HashMap::new(),
            chat_launches: HashMap::new(),
            chat_drafts: HashMap::new(),
            chat_image_drafts: HashMap::new(),
            pending_image_requests: HashMap::new(),
            pending_user_questions: HashMap::new(),
            pending_cursor_requests: HashMap::new(),
            pending_tool_approvals: HashMap::new(),
            tool_approval_previews: HashMap::new(),
            pending_acp_permission_choices: HashMap::new(),
            pending_tool_candidate_selections: HashMap::new(),
            pending_tool_candidate_counts: HashMap::new(),
            tool_execution_records: VecDeque::new(),
            tool_result_facts: VecDeque::new(),
            mcp_registry,
            compaction_tx,
            compaction_rx,
            compaction_deliveries: VecDeque::new(),
            compacting_conversations: HashSet::new(),
            compaction_notice: None,
        };
        entity.schedule_model_refresh_delivery(cx);
        entity.schedule_agent_updates(cx);
        entity.schedule_history(cx);
        entity.schedule_provider_key_status_delivery(cx);
        entity.schedule_selector_probe_delivery(cx);
        entity.schedule_acp_agent_probe_delivery(cx);
        cx.on_release(|entity, _cx| {
            for (_, task) in entity.acp_probe_tasks.drain() {
                task.abort();
            }
            for (_, task) in entity.acp_discovery_tasks.drain() {
                task.abort();
            }
        })
        .detach();
        entity.schedule_acp_model_discovery_delivery(cx);
        entity.schedule_terminal_inline_delivery(cx);
        entity.schedule_chat_stream_delivery(cx);
        entity.schedule_compaction_delivery(cx);
        entity
    }

    pub fn set_workspace_visibility(&mut self, visibility: AiWorkspaceVisibility) -> bool {
        if self.visibility == visibility {
            return false;
        }
        // Visibility controls admission of new UI-only probes. In-flight chat,
        // compaction, Knowledge, and user-triggered operations retain delivery.
        self.visibility = visibility;
        if !visibility.settings_surface {
            // Dropping the retained timer prevents a hidden MCP page from
            // generating repaint-only runtime notifications.
            self.mcp_status_tick_task.take();
        }
        true
    }

    #[cfg(test)]
    fn workspace_visibility(&self) -> AiWorkspaceVisibility {
        self.visibility
    }

    pub fn model_is_refreshing(&self, provider_id: &str) -> bool {
        self.refreshing_models.contains(provider_id)
    }

    pub fn knowledge_page(&self) -> &Entity<KnowledgePage> {
        &self.knowledge_page
    }
    pub fn conversation_state(&self) -> &oxideterm_ai::AiChatState {
        &self.conversation_state
    }

    pub fn rag_store(&self) -> Arc<oxideterm_ai::RagStore> {
        self.rag_store.get()
    }

    pub fn conversation_state_mut(&mut self) -> &mut oxideterm_ai::AiChatState {
        // Callers receive unrestricted mutable conversation access, so any
        // cached render signature may become stale after this boundary.
        self.chat_ui
            .message_signature_cache
            .borrow_mut()
            .invalidate_all();
        if let Some(id) = self.conversation_state.active_conversation_id.as_deref() {
            self.history_metadata_changed(id);
        }
        &mut self.conversation_state
    }

    pub fn update_chat_message(
        &mut self,
        conversation_id: &str,
        message_id: &str,
        update: impl FnOnce(&mut oxideterm_ai::AiChatMessage),
    ) {
        if let Some(run_id) = self.agents.message_runs.get(message_id).cloned() {
            if let Some(record) = self.agents.records.get_mut(&run_id) {
                if let Some(message) = record
                    .messages
                    .iter_mut()
                    .find(|message| message.id == message_id)
                {
                    update(message);
                    record.revision =
                        oxideterm_ai::AiChatPersistenceStore::next_projection_persist_at();
                    self.agents.dirty.borrow_mut().insert(run_id);
                }
            }
            self.history_message_changed(conversation_id, message_id);
            self.schedule_history_stream_save();
            return;
        }
        // Streaming changes one assistant message at a time. Invalidate only
        // that row so long histories are not rehashed for every token.
        self.chat_ui
            .message_signature_cache
            .borrow_mut()
            .invalidate_message(message_id);
        self.conversation_state
            .update_message(conversation_id, message_id, update);
        self.history_message_changed(conversation_id, message_id);
        self.schedule_history_stream_save();
    }

    pub fn append_chat_stream_text(
        &mut self,
        conversation: &str,
        message: &str,
        text: &str,
        thinking: bool,
    ) {
        let append = |message: &mut oxideterm_ai::AiChatMessage| {
            if thinking {
                message
                    .thinking_content
                    .get_or_insert_with(String::new)
                    .push_str(text);
            } else {
                message.content.push_str(text);
            }
            oxideterm_ai::stream_state::append_ai_turn_text_part(
                message,
                if thinking { "thinking" } else { "text" },
                text,
                thinking,
            );
        };
        if let Some(run) = self.agents.message_runs.get(message).cloned() {
            if let Some(record) = self
                .agents
                .records
                .get_mut(&run)
                .filter(|record| record.snapshot.conversation_id == conversation)
            {
                if let Some(target) = record
                    .messages
                    .iter_mut()
                    .find(|target| target.id == message)
                {
                    append(target);
                    record.revision =
                        oxideterm_ai::AiChatPersistenceStore::next_projection_persist_at();
                }
            }
            self.history_stream_text_changed(conversation, message);
            self.schedule_history_stream_save();
            return;
        }
        self.chat_ui
            .message_signature_cache
            .borrow_mut()
            .invalidate_message(message);
        self.conversation_state
            .update_message(conversation, message, append);
        self.history_stream_text_changed(conversation, message);
        self.schedule_history_stream_save();
    }

    pub fn chat_is_loading(&self) -> bool {
        self.conversation_state
            .active_conversation_id
            .as_ref()
            .is_some_and(|id| self.loading_conversations.contains(id))
    }

    pub fn set_chat_loading(&mut self, loading: bool) {
        if let Some(id) = self.conversation_state.active_conversation_id.clone() {
            self.set_conversation_loading(&id, loading);
        }
    }

    pub fn chat_initialization_error(&self) -> Option<&AiChatInitializationError> {
        self.chat_initialization_error.as_ref()
    }

    pub fn active_conversation_safety_mode(&self) -> oxideterm_gpui_ui::ai::AiSafetyMode {
        self.conversation_state
            .active_conversation_id
            .as_ref()
            .and_then(|id| self.safety_modes_by_conversation.get(id))
            .copied()
            .unwrap_or(oxideterm_gpui_ui::ai::AiSafetyMode::Default)
    }

    pub(crate) fn chat_initialized(&self) -> bool {
        self.chat_initialized
    }

    pub fn ensure_chat_initialized(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.chat_initialized {
            return;
        }
        self.chat_initialized = true;
        self.initialize_history(path, cx);
    }

    pub fn retry_chat_initialization(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.chat_initialized = true;
        self.chat_initialization_error = None;
        if self.history.store.is_some() {
            if let Some(id) = self.conversation_state.active_conversation_id.clone() {
                self.request_history_conversation(id);
            }
            self.retry_history_write();
        } else {
            self.initialize_history(path, cx);
        }
    }

    pub fn select_conversation(&mut self, id: String) {
        if !self
            .conversation_state
            .conversations
            .iter()
            .any(|item| item.id == id)
        {
            match self
                .history
                .store
                .as_ref()
                .map(|store| store.conversation_head(&id))
                .transpose()
            {
                Ok(Some(Some(head))) => {
                    self.history.branches.insert(id.clone(), head.active_branch);
                    self.conversation_state
                        .conversations
                        .push(head.conversation);
                }
                Err(_) => {
                    self.history_load_failed();
                    return;
                }
                _ => return,
            }
        }
        if self.conversation_state.active_conversation_id.as_ref() != Some(&id) {
            self.cancel_history_page_loads();
            if self.history.edit_load.take().is_some() {
                if let Some(previous) = self.conversation_state.active_conversation_id.clone() {
                    self.set_conversation_loading(&previous, false);
                }
            }
            self.clear_message_edit();
            self.stash_active_chat_draft();
            let draft = self
                .chat_drafts
                .remove(&id)
                .map(|draft| draft.to_string())
                .unwrap_or_default();
            self.set_chat_draft(draft);
            self.chat_ui.images = self.chat_image_drafts.remove(&id).unwrap_or_default();
        }
        self.close_agent_detail();
        self.history.archives.clear();
        self.load_agent_summaries(&id);
        // Selecting can reload a previously unloaded conversation with the same
        // identifier, so identity comparison alone cannot prove cached rows valid.
        self.chat_ui
            .message_signature_cache
            .borrow_mut()
            .invalidate_all();
        let previous_index = self
            .conversation_state
            .active_conversation_id
            .as_deref()
            .filter(|previous| *previous != id.as_str())
            .and_then(|previous| {
                self.conversation_state
                    .conversations
                    .iter()
                    .position(|conversation| conversation.id == previous)
            });
        if let Some(previous_index) = previous_index.filter(|index| {
            !self
                .loading_conversations
                .contains(&self.conversation_state.conversations[*index].id)
                && !self
                    .history_has_unsaved_messages(&self.conversation_state.conversations[*index].id)
        }) {
            let previous = &mut self.conversation_state.conversations[previous_index];
            previous.messages.clear();
            previous.messages_loaded = false;
            self.history.pages.remove(&previous.id);
        }
        let needs_loading = self
            .conversation_state
            .conversations
            .iter()
            .any(|conversation| conversation.id == id && !conversation.messages_loaded);
        self.conversation_state.set_active_conversation(id.clone());
        if needs_loading {
            self.request_history_conversation(id);
        }
    }

    pub fn delete_conversation(&mut self, id: &str) -> bool {
        self.history.deleted_conversations.insert(id.to_owned());
        self.history_deleted(id);
        self.queued_chat_turns.remove(id);
        self.chat_launches.remove(id);
        self.chat_drafts.remove(id);
        self.chat_image_drafts.remove(id);
        self.pending_image_requests.remove(id);
        self.agents.services.runtime.remove_conversation(id);
        let removed: HashSet<_> = self
            .agents
            .records
            .values()
            .filter(|record| record.snapshot.conversation_id == id)
            .map(|record| record.snapshot.run.run_id.clone())
            .collect();
        self.agents.records.retain(|run, _| !removed.contains(run));
        self.agents
            .message_runs
            .retain(|_, run| !removed.contains(run));
        self.remove_agent_history_views(&removed);
        self.agents
            .details_loading
            .retain(|run| !removed.contains(run));
        self.agents
            .detail_errors
            .retain(|run| !removed.contains(run));
        self.agents
            .detail_scroll
            .retain(|run, _| !removed.contains(run));
        self.agents
            .dirty
            .borrow_mut()
            .retain(|run| !removed.contains(run));
        if self
            .agents
            .detail
            .as_ref()
            .is_some_and(|run| removed.contains(run))
        {
            self.close_agent_detail();
        }
        let active_deleted = self.conversation_state.active_conversation_id.as_deref() == Some(id);
        self.conversation_state.delete_conversation(id);
        if active_deleted {
            let draft = self
                .conversation_state
                .active_conversation_id
                .as_ref()
                .and_then(|id| self.chat_drafts.remove(id))
                .map(|draft| draft.to_string())
                .unwrap_or_default();
            self.set_chat_draft(draft);
            self.chat_ui.images = self
                .conversation_state
                .active_conversation_id
                .as_ref()
                .and_then(|id| self.chat_image_drafts.remove(id))
                .unwrap_or_default();
        }
        self.safety_modes_by_conversation.remove(id);
        if self.chat_ui.renaming_conversation_id.as_deref() == Some(id) {
            self.clear_conversation_rename();
        }
        !self.conversation_state.conversations.is_empty()
    }

    pub fn show_archived_conversations(&mut self, archived: bool) {
        self.close_chat_popovers();
        if !self.conversation_list_page(archived).initialized {
            self.request_conversation_list_page(archived);
        }
        self.chat_ui.show_archived_conversations = archived;
        self.chat_ui.conversation_list_open = true;
    }

    pub fn set_conversation_archived(&mut self, id: &str, archived: bool) {
        let Some(conversation) = self
            .conversation_state
            .conversations
            .iter_mut()
            .find(|item| item.id == id)
        else {
            return;
        };
        conversation.archived = archived;
        self.history_metadata_changed(id);
        if archived && self.conversation_state.active_conversation_id.as_deref() == Some(id) {
            let next = self
                .conversation_state
                .conversations
                .iter()
                .find(|item| !item.archived)
                .map(|item| item.id.clone());
            if let Some(next) = next {
                self.select_conversation(next);
            } else {
                self.stash_active_chat_draft();
                self.cancel_history_page_loads();
                self.conversation_state.active_conversation_id = None;
                self.set_chat_draft(String::new());
            }
            self.reset_chat_for_conversation_selection();
        }
        self.persist_chat_state();
    }

    pub fn rename_conversation(&mut self, id: &str, title: String, now_ms: i64) {
        self.conversation_state
            .rename_conversation(id, title, now_ms);
        self.history_title_changed(id);
        self.clear_conversation_rename();
    }

    pub fn clear_conversations(&mut self) {
        for conversation in &self.conversation_state.conversations {
            self.history_deleted(&conversation.id);
        }
        self.conversation_state.clear_conversations();
        self.safety_modes_by_conversation.clear();
        self.clear_conversation_rename();
    }

    pub fn set_active_conversation_safety_mode(
        &mut self,
        mode: oxideterm_gpui_ui::ai::AiSafetyMode,
    ) {
        let Some(conversation_id) = self.conversation_state.active_conversation_id.as_ref() else {
            return;
        };
        if mode == oxideterm_gpui_ui::ai::AiSafetyMode::Default {
            self.safety_modes_by_conversation.remove(conversation_id);
        } else {
            // Non-default safety modes are scoped to one conversation and are
            // deliberately discarded with that conversation.
            self.safety_modes_by_conversation
                .insert(conversation_id.clone(), mode);
        }
    }

    pub fn persist_chat_state(&self) {
        self.persist_agent_records();
        self.request_history_save();
    }

    pub fn persist_transcript_entries(
        &self,
        conversation_id: String,
        entries: Vec<oxideterm_ai::PersistedTranscriptEntry>,
    ) {
        for entry in entries {
            let id = entry.id.clone();
            self.queue_history_event(
                &conversation_id,
                "transcript",
                &id,
                history::HistoryEvent::Transcript(entry),
            );
        }
    }

    pub fn persist_diagnostic_events(
        &self,
        conversation_id: String,
        events: Vec<oxideterm_ai::PersistedDiagnosticEvent>,
    ) {
        for event in events {
            let id = event.id.clone();
            self.queue_history_event(
                &conversation_id,
                "diagnostic",
                &id,
                history::HistoryEvent::Diagnostic(event),
            );
        }
    }

    pub fn cancel_chat_conversation_state(
        &mut self,
    ) -> (
        Option<String>,
        Vec<oxideterm_ai::stream_state::AiStoppedAssistantTurn>,
    ) {
        self.set_chat_loading(false);
        // Cancellation finalizes streaming messages in place rather than through
        // update_chat_message, so their list signatures must be recomputed.
        self.chat_ui
            .message_signature_cache
            .borrow_mut()
            .invalidate_all();
        let conversation_id = self.conversation_state.active_conversation_id.clone();
        let stopped_turns = self
            .conversation_state
            .active_conversation_mut()
            .map(oxideterm_ai::stream_state::finalize_streaming_ai_messages_on_cancel)
            .unwrap_or_default();
        if let Some(id) = &conversation_id {
            for turn in &stopped_turns {
                if turn.retained {
                    self.history_message_changed(id, &turn.message_id);
                } else {
                    self.history_message_deleted(id, &turn.message_id);
                }
            }
        }
        self.persist_chat_state();
        (conversation_id, stopped_turns)
    }

    pub fn next_chat_id(&mut self, _now_ms: i64) -> String {
        format!("chat-{}", uuid::Uuid::new_v4())
    }

    pub fn request_model_refresh(
        &mut self,
        index: usize,
        provider: oxideterm_ai::AiProviderView,
    ) -> bool {
        let Some(generation) = self.begin_model_refresh(&provider.id) else {
            return false;
        };
        let provider_id = provider.id.clone();
        let key_store = self.key_store.clone();
        let worker_tx = self.model_refresh_tx.clone();
        self.task_runtime.spawn(async move {
            let key_policy = oxideterm_ai::provider_refresh_key_policy(&provider.provider_type);
            let api_key = match key_policy {
                oxideterm_ai::AiProviderRefreshKeyPolicy::NoKey => None,
                oxideterm_ai::AiProviderRefreshKeyPolicy::OptionalStoredKey => {
                    tokio::task::spawn_blocking({
                        let key_store = key_store.clone();
                        let provider_id = provider_id.clone();
                        move || key_store.get_provider_key(&provider_id)
                    })
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .flatten()
                }
                oxideterm_ai::AiProviderRefreshKeyPolicy::RequiredStoredKey => {
                    match tokio::task::spawn_blocking({
                        let key_store = key_store.clone();
                        let provider_id = provider_id.clone();
                        move || key_store.get_provider_key(&provider_id)
                    })
                    .await
                    {
                        Ok(Ok(Some(key))) => Some(key),
                        Ok(Ok(None)) => {
                            let _ = worker_tx.send(AiModelRefreshWorkerDelivery {
                                index,
                                provider_id,
                                generation,
                                result: Err(AiModelRefreshFailure::MissingApiKey),
                            });
                            return;
                        }
                        Ok(Err(error)) => {
                            let _ = worker_tx.send(AiModelRefreshWorkerDelivery {
                                index,
                                provider_id,
                                generation,
                                result: Err(AiModelRefreshFailure::Failed(format!(
                                    "could not read the saved API key: {error}"
                                ))),
                            });
                            return;
                        }
                        Err(error) => {
                            let _ = worker_tx.send(AiModelRefreshWorkerDelivery {
                                index,
                                provider_id,
                                generation,
                                result: Err(AiModelRefreshFailure::Failed(format!(
                                    "could not read the saved API key: {error}"
                                ))),
                            });
                            return;
                        }
                    }
                }
            };
            let result = oxideterm_ai::fetch_provider_models(provider, api_key)
                .await
                .map_err(|error| {
                    AiModelRefreshFailure::Failed(model_refresh_failure_message(&error))
                });
            let _ = worker_tx.send(AiModelRefreshWorkerDelivery {
                index,
                provider_id,
                generation,
                result,
            });
        });
        true
    }

    pub fn take_model_refresh_intents(&mut self) -> VecDeque<AiModelRefreshIntent> {
        std::mem::take(&mut self.model_refresh_intents)
    }

    pub fn provider_has_key(&self, provider_id: &str) -> bool {
        self.provider_key_status
            .get(provider_id)
            .copied()
            .unwrap_or(false)
    }

    pub fn set_provider_key_status(&mut self, provider_id: String, has_key: bool) {
        self.provider_key_status_pending.remove(&provider_id);
        self.provider_key_status.insert(provider_id, has_key);
    }

    pub fn store_provider_key(
        &mut self,
        index: usize,
        provider_id: String,
        secret: zeroize::Zeroizing<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        let key_store = self.key_store.clone();
        let task_runtime = self.task_runtime.clone();
        let provider_id_for_store = provider_id.clone();
        let operation = async move {
            task_runtime
                .spawn_blocking(move || {
                    key_store.store_provider_key(&provider_id_for_store, secret)
                })
                .await
                .is_ok_and(|result| result.is_ok())
        };
        self.start_provider_key_operation(
            provider_id,
            AiProviderKeyOperation::Store { index },
            operation,
            cx,
        )
    }

    pub fn remove_provider_key(&mut self, provider_id: String, cx: &mut Context<Self>) -> bool {
        if self.provider_key_operation_tasks.contains_key(&provider_id) {
            // Provider removal must be serialized after an in-flight store;
            // racing delete with the keychain write could recreate the key.
            self.pending_provider_key_removals.insert(provider_id);
            return true;
        }
        let key_store = self.key_store.clone();
        let task_runtime = self.task_runtime.clone();
        let provider_id_for_delete = provider_id.clone();
        let operation = async move {
            task_runtime
                .spawn_blocking(move || key_store.delete_provider_key(&provider_id_for_delete))
                .await
                .is_ok_and(|result| result.is_ok())
        };
        self.start_provider_key_operation(
            provider_id,
            AiProviderKeyOperation::Remove,
            operation,
            cx,
        )
    }

    fn start_provider_key_operation(
        &mut self,
        provider_id: String,
        operation_kind: AiProviderKeyOperation,
        operation: impl std::future::Future<Output = bool> + 'static,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.provider_key_operation_tasks.contains_key(&provider_id) {
            return false;
        }

        // Keep completion attached to the AI owner instead of a detached
        // workspace callback that could outlive its settings surface.
        let operation_provider_id = provider_id.clone();
        let operation_task = cx.spawn(async move |entity, cx| {
            let succeeded = operation.await;
            let _ = entity.update(cx, |entity, cx| {
                entity
                    .provider_key_operation_tasks
                    .remove(&operation_provider_id);
                let queued_removal = entity
                    .pending_provider_key_removals
                    .remove(&operation_provider_id);
                let queued_removal_provider_id = operation_provider_id.clone();
                let intent = match (operation_kind, succeeded) {
                    (AiProviderKeyOperation::Store { index }, true) => {
                        entity.set_provider_key_status(operation_provider_id.clone(), true);
                        AiCredentialIntent::ProviderKeyStored {
                            index,
                            provider_id: operation_provider_id,
                        }
                    }
                    (AiProviderKeyOperation::Remove, true) => {
                        entity.set_provider_key_status(operation_provider_id, false);
                        AiCredentialIntent::ProviderKeyRemoved
                    }
                    (AiProviderKeyOperation::Store { .. }, false) => {
                        AiCredentialIntent::Failed(AiCredentialFailure::SaveProviderKey)
                    }
                    (AiProviderKeyOperation::Remove, false) => {
                        AiCredentialIntent::Failed(AiCredentialFailure::RemoveProviderKey)
                    }
                };
                entity.credential_intents.push_back(intent);
                cx.emit(AiWorkspaceEvent::CredentialOperationReady);
                if queued_removal && matches!(operation_kind, AiProviderKeyOperation::Store { .. })
                {
                    entity.remove_provider_key(queued_removal_provider_id, cx);
                }
                cx.notify();
            });
        });
        self.provider_key_operation_tasks
            .insert(provider_id, operation_task);
        true
    }

    pub fn take_credential_intents(&mut self) -> VecDeque<AiCredentialIntent> {
        std::mem::take(&mut self.credential_intents)
    }

    pub fn open_chat_confirm(&mut self, confirm: AiChatConfirmKind, cx: &mut Context<Self>) {
        // Reopen is a new generation and dropping the retained task prevents a
        // stale exit from clearing the replacement payload.
        self.chat_confirm_exit_task = None;
        self.chat_confirm = Some(confirm);
        self.chat_confirm_presence.reopen();
        self.chat_confirm_focused_action = None;
        cx.notify();
    }

    pub fn chat_confirm_snapshot(&self) -> Option<AiChatConfirmSnapshot> {
        self.chat_confirm
            .as_ref()
            .cloned()
            .map(|kind| AiChatConfirmSnapshot {
                kind,
                phase: self.chat_confirm_presence.phase(),
                focused_action: self.chat_confirm_focused_action,
            })
    }

    pub fn chat_confirm_owner_snapshot(&self) -> Option<AiChatConfirmOwnerSnapshot> {
        self.chat_confirm
            .as_ref()
            .map(|confirm| AiChatConfirmOwnerSnapshot {
                kind: match confirm {
                    AiChatConfirmKind::ClearAll => AiChatConfirmOwnerKind::ClearAll,
                    AiChatConfirmKind::DeleteMessage { .. } => {
                        AiChatConfirmOwnerKind::DeleteMessage
                    }
                },
                phase: self.chat_confirm_presence.phase(),
            })
    }

    pub fn handle_chat_confirm_key(
        &mut self,
        key: &str,
        shift: bool,
        blocked_by_primary_modifier: bool,
        cx: &mut Context<Self>,
    ) -> Option<AiChatConfirmKeyAction> {
        if blocked_by_primary_modifier
            || self.chat_confirm_presence.phase() != oxideterm_gpui_ui::motion::ExitPhase::Visible
            || self.chat_confirm.is_none()
        {
            return None;
        }
        const ACTIONS: [ConfirmDialogAction; 2] =
            [ConfirmDialogAction::Cancel, ConfirmDialogAction::Confirm];
        match modal_footer_key_action(
            key,
            shift,
            &ACTIONS,
            self.chat_confirm_focused_action,
            ConfirmDialogAction::Cancel,
        ) {
            Some(ModalFooterKeyAction::Cancel) => {
                self.chat_confirm_focused_action = None;
                Some(AiChatConfirmKeyAction::Cancel)
            }
            Some(ModalFooterKeyAction::Focus(action)) => {
                self.chat_confirm_focused_action = Some(action);
                cx.notify();
                Some(AiChatConfirmKeyAction::Handled)
            }
            Some(ModalFooterKeyAction::Activate(action)) => {
                self.chat_confirm_focused_action = None;
                Some(match action {
                    ConfirmDialogAction::Cancel => AiChatConfirmKeyAction::Cancel,
                    ConfirmDialogAction::Confirm => AiChatConfirmKeyAction::Confirm,
                })
            }
            None => None,
        }
    }

    pub fn begin_chat_confirm_exit(
        &mut self,
        confirmed: bool,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> (bool, Option<AiChatConfirmEffect>) {
        let Some(confirm) = self.chat_confirm.as_ref() else {
            return (false, None);
        };
        let Some(generation) = self.chat_confirm_presence.begin_exit() else {
            return (false, None);
        };
        self.chat_confirm_focused_action = None;
        let effect = if confirmed {
            Some(match confirm {
                AiChatConfirmKind::ClearAll => AiChatConfirmEffect::ClearAll,
                AiChatConfirmKind::DeleteMessage { message_id } => {
                    AiChatConfirmEffect::DeleteMessage {
                        message_id: message_id.clone(),
                    }
                }
            })
        } else {
            None
        };
        self.chat_confirm_exit_task = None;
        if delay.is_zero() {
            self.finish_chat_confirm_exit(generation, cx);
            return (true, effect);
        }
        self.chat_confirm_exit_task = Some(cx.spawn(async move |entity, cx| {
            Timer::after(delay).await;
            let _ = entity.update(cx, |entity, cx| {
                entity.finish_chat_confirm_exit(generation, cx);
            });
        }));
        cx.notify();
        (true, effect)
    }

    fn finish_chat_confirm_exit(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.chat_confirm_exit_task = None;
        if self.chat_confirm.is_some() && self.chat_confirm_presence.finish_exit(generation) {
            self.chat_confirm = None;
            self.chat_confirm_presence.reopen();
            self.chat_confirm_focused_action = None;
            cx.notify();
        }
    }

    pub fn start_mcp_server_add(&mut self, mut draft: AiMcpServerDraft, cx: &mut Context<Self>) {
        let server_id = format!("mcp-{}", uuid::Uuid::new_v4());
        let should_store_auth_token = !draft.auth_token.is_empty()
            && draft.auth_header_mode != oxideterm_ai::McpAuthHeaderMode::None;
        if !should_store_auth_token {
            let config = mcp_server_config_from_draft(draft, server_id);
            self.credential_intents
                .push_back(AiCredentialIntent::McpServerReady { config });
            cx.emit(AiWorkspaceEvent::CredentialOperationReady);
            cx.notify();
            return;
        }

        let token = take_mcp_auth_token(&mut draft);
        let registry = self.mcp_registry.clone();
        let task_runtime = self.task_runtime.clone();
        let keychain_server_id = server_id.clone();
        let operation = async move {
            task_runtime
                .spawn_blocking(move || registry.store_auth_token(&keychain_server_id, token))
                .await
                .is_ok_and(|result| result.is_ok())
        };
        self.start_mcp_token_save(draft, server_id, operation, cx);
    }

    fn start_mcp_token_save(
        &mut self,
        pending_draft: AiMcpServerDraft,
        server_id: String,
        operation: impl std::future::Future<Output = bool> + 'static,
        cx: &mut Context<Self>,
    ) {
        self.mcp_save_task = Some(cx.spawn(async move |entity, cx| {
            let stored = operation.await;
            let _ = entity.update(cx, |entity, cx| {
                entity.mcp_save_task = None;
                if stored {
                    // Build the persisted value only after the keychain write
                    // succeeds, avoiding an async second copy of env/header data.
                    let config = mcp_server_config_from_draft(pending_draft, server_id);
                    entity
                        .credential_intents
                        .push_back(AiCredentialIntent::McpServerReady { config });
                } else {
                    // The token was consumed by the failed keychain boundary;
                    // restoring the remaining draft requires explicit re-entry.
                    entity
                        .credential_intents
                        .push_back(AiCredentialIntent::McpServerSaveFailed {
                            draft: pending_draft,
                        });
                }
                cx.emit(AiWorkspaceEvent::CredentialOperationReady);
                cx.notify();
            });
        }));
    }

    pub fn request_mcp_status_tick(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        if !self.visibility.settings_surface || self.mcp_status_tick_task.is_some() {
            return false;
        }
        self.mcp_status_tick_task = Some(cx.spawn(async move |entity, cx| {
            Timer::after(delay).await;
            let _ = entity.update(cx, |entity, cx| {
                entity.mcp_status_tick_task = None;
                cx.emit(AiWorkspaceEvent::McpRuntimeChanged);
                cx.notify();
            });
        }));
        true
    }

    pub fn refresh_mcp_tools(&mut self, server_id: String, cx: &mut Context<Self>) -> bool {
        let registry = self.mcp_registry.clone();
        self.start_mcp_runtime_task(
            server_id.clone(),
            async move {
                let _ = registry.refresh_tools(&server_id).await;
                None
            },
            cx,
        )
    }

    pub fn set_mcp_server_connected(
        &mut self,
        config: oxideterm_ai::McpServerConfig,
        connected: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let server_id = config.id.clone();
        let registry = self.mcp_registry.clone();
        self.start_mcp_runtime_task(
            server_id,
            async move {
                if connected {
                    registry.disconnect_server(&config.id).await;
                } else {
                    registry.connect_config(config).await;
                }
                None
            },
            cx,
        )
    }

    pub fn remove_mcp_server(&mut self, server_id: String, cx: &mut Context<Self>) -> bool {
        let registry = self.mcp_registry.clone();
        let task_runtime = self.task_runtime.clone();
        self.start_mcp_runtime_task(
            server_id.clone(),
            async move {
                registry.disconnect_server(&server_id).await;
                let delete_registry = registry.clone();
                let server_id_for_delete = server_id.clone();
                let _ = task_runtime
                    .spawn_blocking(move || {
                        delete_registry.delete_auth_token(&server_id_for_delete)
                    })
                    .await;
                Some(server_id)
            },
            cx,
        )
    }

    fn start_mcp_runtime_task(
        &mut self,
        task_key: String,
        operation: impl std::future::Future<Output = Option<String>> + 'static,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.mcp_runtime_tasks.contains_key(&task_key) {
            return false;
        }
        // Retaining one task per server keeps runtime actions alive when the
        // settings page becomes hidden and rejects duplicate user actions.
        let completion_key = task_key.clone();
        self.mcp_runtime_tasks.insert(
            task_key,
            cx.spawn(async move |entity, cx| {
                let removed_server_id = operation.await;
                let _ = entity.update(cx, |entity, cx| {
                    entity.mcp_runtime_tasks.remove(&completion_key);
                    if let Some(server_id) = removed_server_id {
                        entity
                            .credential_intents
                            .push_back(AiCredentialIntent::McpServerRemoved { server_id });
                        cx.emit(AiWorkspaceEvent::CredentialOperationReady);
                    }
                    cx.emit(AiWorkspaceEvent::McpRuntimeChanged);
                    cx.notify();
                });
            }),
        );
        true
    }

    pub fn invalidate_provider_key_status(&mut self, provider_id: &str) {
        self.provider_key_status.remove(provider_id);
        self.provider_key_status_pending.remove(provider_id);
    }

    pub fn request_provider_key_statuses(
        &mut self,
        provider_ids: impl IntoIterator<Item = String>,
    ) {
        if !self.visibility.provider_status_visible() {
            return;
        }
        for provider_id in provider_ids {
            if self.provider_key_status.contains_key(&provider_id)
                || !self.provider_key_status_pending.insert(provider_id.clone())
            {
                continue;
            }
            let worker_tx = self.provider_key_status_tx.clone();
            let key_store = self.key_store.clone();
            self.task_runtime.spawn(async move {
                let provider_id_for_check = provider_id.clone();
                let has_key = tokio::task::spawn_blocking(move || {
                    key_store.has_provider_key(&provider_id_for_check)
                })
                .await
                .unwrap_or(false);
                let _ = worker_tx.send(AiProviderKeyStatusDelivery {
                    provider_id,
                    has_key,
                });
            });
        }
    }

    pub fn selector_provider_is_online(&self, provider_id: &str) -> bool {
        self.selector_provider_online
            .get(provider_id)
            .copied()
            .unwrap_or(true)
    }

    pub fn set_selector_provider_online(&mut self, provider_id: String, online: bool) {
        // Direct state transitions supersede any older network probe result.
        self.selector_probe_generations.remove(&provider_id);
        self.selector_provider_online.insert(provider_id, online);
    }

    pub fn invalidate_selector_provider_status(&mut self, provider_id: &str) {
        self.selector_probe_generations.remove(provider_id);
        self.selector_provider_online.remove(provider_id);
    }

    pub fn request_selector_provider_probe(
        &mut self,
        provider: oxideterm_ai::AiProviderView,
        endpoint: &'static str,
    ) -> bool {
        if !self.visibility.model_selector_surface {
            return false;
        }
        self.next_selector_probe_generation = self.next_selector_probe_generation.saturating_add(1);
        let generation = self.next_selector_probe_generation;
        let provider_id = provider.id.clone();
        self.selector_probe_generations
            .insert(provider_id.clone(), generation);
        self.selector_probe_pending = self.selector_probe_pending.saturating_add(1);
        let worker_tx = self.selector_probe_tx.clone();
        let key_store = self.key_store.clone();
        self.task_runtime.spawn(async move {
            let online = if provider.provider_type == "openai_compatible" {
                let provider_id_for_key = provider_id.clone();
                // OpenAI-compatible endpoints may be keyless or authenticated. Keep the optional
                // secret in a zeroizing owner for this probe and never copy it into status state.
                let api_key = tokio::task::spawn_blocking(move || {
                    key_store.get_provider_key(&provider_id_for_key)
                })
                .await
                .ok()
                .and_then(Result::ok)
                .flatten();
                oxideterm_ai::check_openai_compatible_model_selector_provider_online(
                    &provider.base_url,
                    api_key,
                )
                .await
            } else {
                oxideterm_ai::check_model_selector_provider_online(&provider.base_url, endpoint)
                    .await
            };
            let _ = worker_tx.send(AiModelSelectorProbeDelivery {
                provider_id,
                generation,
                online,
            });
        });
        true
    }

    pub fn acp_agent_probe_is_pending(&self, agent_id: &str) -> bool {
        self.acp_agent_probe_pending.contains(agent_id)
    }

    pub fn invalidate_acp_agent_metadata(&mut self, agent_id: &str) {
        *self
            .acp_agent_epochs
            .entry(agent_id.to_string())
            .or_default() += 1;
        if let Some(task) = self.acp_probe_tasks.remove(agent_id) {
            task.abort();
        }
        self.acp_discovery_tasks.retain(|(_, id), task| {
            if id == agent_id {
                task.abort();
                false
            } else {
                true
            }
        });
        self.acp_agent_probe_pending.remove(agent_id);
        self.acp_model_discovery_pending
            .retain(|(_, id)| id != agent_id);
        self.acp_model_options.retain(|(_, id), _| id != agent_id);
        self.acp_agent_probe_intents
            .retain(|intent| intent.agent_id != agent_id);
        self.acp_model_discovery_intents
            .retain(|intent| intent.agent_id != agent_id);
        self.model_ui.selector_status_signature = None;
    }

    pub fn request_acp_agent_probe(&mut self, agent: oxideterm_settings::AcpAgentConfig) -> bool {
        if !(self.visibility.settings_surface || self.visibility.model_selector_surface)
            || self.acp_agent_probe_pending.contains(&agent.id)
        {
            return false;
        }
        let agent_id = agent.id.clone();
        let capability_policy = oxideterm_ai::AcpHostCapabilityPolicy {
            fs_read_text_file: agent.capability_policy.fs_read_text_file,
            fs_write_text_file: agent.capability_policy.fs_write_text_file,
            terminal: agent.capability_policy.terminal,
        };
        // Move args and env into the zeroizing launch config. They may contain
        // local agent tokens and must not be cloned for worker convenience.
        let launch_config = oxideterm_ai::AcpLaunchConfig {
            id: agent.id,
            display_name: agent.display_name,
            command: agent.command,
            args: agent.args,
            env: agent.env,
            cwd: agent.cwd.map(std::path::PathBuf::from),
        };
        self.acp_agent_probe_pending.insert(agent_id.clone());
        let worker_tx = self.acp_agent_probe_tx.clone();
        let epoch = self
            .acp_agent_epochs
            .get(&agent_id)
            .copied()
            .unwrap_or_default();
        let task_agent_id = agent_id.clone();
        let task = self.task_runtime.spawn(async move {
            let result = match oxideterm_ai::build_acp_stdio_launcher(launch_config) {
                Ok(launcher) => {
                    if !oxideterm_ai::acp_launch_command_available(launcher.config())
                        .unwrap_or(false)
                    {
                        ai_acp_probe_error_result("command_not_found")
                    } else {
                        match oxideterm_ai::initialize_acp_agent(
                            launcher,
                            env!("CARGO_PKG_VERSION").to_string(),
                            capability_policy,
                        )
                        .await
                        {
                            Ok(response) => {
                                AiAcpAgentProbeResult {
                                    // Initialize advertises available auth methods; it does
                                    // not prove that the current process still needs auth.
                                    // Only an AuthRequired protocol error can establish that.
                                    runtime_state: oxideterm_settings::AcpAgentRuntimeState::Ready,
                                    auth_status: if response.auth_methods.is_empty() {
                                        oxideterm_settings::AcpAgentAuthStatus::NotRequired
                                    } else {
                                        oxideterm_settings::AcpAgentAuthStatus::Unknown
                                    },
                                    last_error_kind: None,
                                }
                            }
                            Err(_) => ai_acp_probe_error_result("initialize"),
                        }
                    }
                }
                Err(_) => ai_acp_probe_error_result("config"),
            };
            let _ = worker_tx.send(AiAcpAgentProbeDelivery {
                epoch,
                agent_id,
                result,
            });
        });
        self.acp_probe_tasks
            .insert(task_agent_id, task.abort_handle());
        true
    }

    pub fn take_acp_agent_probe_intents(&mut self) -> VecDeque<AiAcpAgentProbeIntent> {
        std::mem::take(&mut self.acp_agent_probe_intents)
    }

    pub fn acp_model_options(
        &self,
        conversation_id: &str,
        agent_id: &str,
    ) -> Option<Vec<oxideterm_ai::AcpSessionConfigOption>> {
        self.acp_model_options
            .get(&(conversation_id.to_string(), agent_id.to_string()))
            .cloned()
    }

    pub fn acp_model_discovery_is_pending(&self, conversation_id: &str, agent_id: &str) -> bool {
        self.acp_model_discovery_pending
            .contains(&(conversation_id.to_string(), agent_id.to_string()))
    }

    pub fn request_acp_model_discovery(
        &mut self,
        conversation_id: String,
        agent: oxideterm_settings::AcpAgentConfig,
        session_cwd: std::path::PathBuf,
    ) -> bool {
        if !self.visibility.model_selector_surface {
            return false;
        }
        let agent_id = agent.id.clone();
        let discovery_key = (conversation_id.clone(), agent_id.clone());
        if self.acp_model_options.contains_key(&discovery_key)
            || !self.acp_model_discovery_pending.insert(discovery_key)
        {
            return false;
        }
        let capability_policy = oxideterm_ai::AcpHostCapabilityPolicy {
            fs_read_text_file: agent.capability_policy.fs_read_text_file,
            fs_write_text_file: agent.capability_policy.fs_write_text_file,
            terminal: agent.capability_policy.terminal,
        };
        let display_name = if agent.display_name.trim().is_empty() {
            agent_id.clone()
        } else {
            agent.display_name
        };
        // Discovery uses the same zeroizing one-shot launch config as a real
        // ACP session and moves token-bearing args/env into the worker.
        let launch_config = oxideterm_ai::AcpLaunchConfig {
            id: agent.id,
            display_name,
            command: agent.command,
            args: agent.args,
            env: agent.env,
            cwd: agent.cwd.map(std::path::PathBuf::from),
        };
        let worker_tx = self.acp_model_discovery_tx.clone();
        let epoch = self
            .acp_agent_epochs
            .get(&agent_id)
            .copied()
            .unwrap_or_default();
        let task_key = (conversation_id.clone(), agent_id.clone());
        let task = self.task_runtime.spawn(async move {
            let config_options = match oxideterm_ai::build_acp_stdio_launcher(launch_config) {
                Ok(launcher) => oxideterm_ai::discover_acp_session_config_options(
                    launcher,
                    env!("CARGO_PKG_VERSION").to_string(),
                    capability_policy,
                    session_cwd,
                )
                .await
                .ok()
                .filter(|options| {
                    oxideterm_ai::acp_model_config_option(options)
                        .is_some_and(|option| !option.choices.is_empty())
                }),
                Err(_) => None,
            };
            let _ = worker_tx.send(AiAcpModelDiscoveryDelivery {
                epoch,
                conversation_id,
                agent_id,
                config_options,
            });
        });
        self.acp_discovery_tasks
            .insert(task_key, task.abort_handle());
        true
    }

    pub fn take_acp_model_discovery_intents(&mut self) -> VecDeque<AiAcpModelDiscoveryIntent> {
        std::mem::take(&mut self.acp_model_discovery_intents)
    }

    pub fn apply_acp_model_discovery(
        &mut self,
        intent: AiAcpModelDiscoveryIntent,
        conversation_exists: bool,
    ) {
        if let Some(options) = intent.config_options
            && conversation_exists
        {
            self.acp_model_options
                .insert((intent.conversation_id, intent.agent_id), options);
        }
    }

    pub fn terminal_inline_panel(&self) -> &AiInlinePanelState {
        &self.terminal_inline_panel
    }

    pub fn terminal_inline_panel_mut(&mut self) -> &mut AiInlinePanelState {
        &mut self.terminal_inline_panel
    }

    pub fn replace_terminal_inline_prompt(
        &mut self,
        range: Option<std::ops::Range<usize>>,
        text: &str,
    ) -> bool {
        let panel = &mut self.terminal_inline_panel;
        if !panel.prompt_focused {
            return false;
        }
        replace_utf16(&mut panel.prompt, range, text);
        panel.error = None;
        true
    }

    pub fn open_terminal_inline_panel(&mut self, selection_context: String) {
        self.abort_terminal_inline_stream();
        let panel = &mut self.terminal_inline_panel;
        panel.open = true;
        panel.target = None;
        panel.prompt.clear();
        panel.response.clear();
        panel.error = None;
        panel.loading = false;
        panel.copied = false;
        panel.prompt_focused = true;
        panel.has_api_key = None;
        panel.has_selection = !selection_context.trim().is_empty();
        panel.selection_context = selection_context;
        panel.generation = panel.generation.wrapping_add(1);
    }

    pub fn close_terminal_inline_panel(&mut self) {
        self.abort_terminal_inline_stream();
        let panel = &mut self.terminal_inline_panel;
        panel.open = false;
        panel.target = None;
        panel.prompt_focused = false;
        panel.loading = false;
        panel.error = None;
        panel.generation = panel.generation.wrapping_add(1);
    }

    pub fn terminal_inline_request_context(&self) -> Option<(String, String)> {
        let panel = &self.terminal_inline_panel;
        if panel.loading || panel.prompt.trim().is_empty() {
            return None;
        }
        Some((
            oxideterm_ai::sanitize_for_ai(&panel.prompt),
            panel.selection_context.clone(),
        ))
    }

    pub fn request_terminal_inline(
        &mut self,
        config_result: Result<oxideterm_ai::AiChatStreamConfig, String>,
        messages: Vec<oxideterm_ai::AiChatMessage>,
        api_key_not_found: String,
        failed_to_get_key: String,
        stream_failed: String,
    ) -> bool {
        if self.terminal_inline_panel.loading || self.terminal_inline_panel.prompt.trim().is_empty()
        {
            return false;
        }
        self.abort_terminal_inline_stream();
        let panel = &mut self.terminal_inline_panel;
        let generation = panel.generation.wrapping_add(1);
        panel.generation = generation;
        panel.response.clear();
        panel.error = None;
        panel.copied = false;
        panel.loading = true;
        panel.has_api_key = None;

        let mut config = match config_result {
            Ok(config) => config,
            Err(message) => {
                panel.loading = false;
                panel.error = Some(message);
                return true;
            }
        };
        let requires_key = oxideterm_ai::provider_chat_requires_key(&config.provider_type);
        let provider_id = config.provider_id.clone();
        let key_store = self.key_store.clone();
        let worker_tx = self.terminal_inline_tx.clone();
        let task = self.task_runtime.spawn(async move {
            if let Some(provider_id) = provider_id {
                let key_result =
                    tokio::task::spawn_blocking(move || key_store.get_provider_key(&provider_id))
                        .await
                        .ok()
                        .and_then(Result::ok);
                match key_result {
                    Some(api_key) => {
                        let has_key = api_key.as_ref().is_some_and(|key| !key.trim().is_empty());
                        let _ = worker_tx.send(AiTerminalInlineDelivery::KeyStatus {
                            generation,
                            has_key,
                        });
                        if requires_key && !has_key {
                            let _ = worker_tx.send(AiTerminalInlineDelivery::Error {
                                generation,
                                message: api_key_not_found,
                            });
                            return;
                        }
                        config.api_key = api_key.map(oxideterm_ai::SharedAiProviderKey::new);
                    }
                    None if requires_key => {
                        let _ = worker_tx.send(AiTerminalInlineDelivery::Error {
                            generation,
                            message: failed_to_get_key,
                        });
                        return;
                    }
                    None => {}
                }
            }

            let (stream_tx, mut stream_rx) = tokio::sync::mpsc::unbounded_channel();
            let provider_stream = oxideterm_ai::stream_chat_completion(
                config,
                oxideterm_ai::sanitize_api_messages_for_provider(messages),
                stream_tx,
            );
            let deliver_stream = async move {
                while let Some(event) = stream_rx.recv().await {
                    match event {
                        oxideterm_ai::AiStreamEvent::Content(chunk) => {
                            let _ = worker_tx
                                .send(AiTerminalInlineDelivery::Content { generation, chunk });
                        }
                        oxideterm_ai::AiStreamEvent::Done => {
                            let _ = worker_tx.send(AiTerminalInlineDelivery::Done { generation });
                            break;
                        }
                        oxideterm_ai::AiStreamEvent::Error(_) => {
                            // Provider errors may contain response bodies or request
                            // metadata, so only localized safe copy reaches the UI.
                            let _ = worker_tx.send(AiTerminalInlineDelivery::Error {
                                generation,
                                message: stream_failed,
                            });
                            break;
                        }
                        oxideterm_ai::AiStreamEvent::Thinking(_)
                        | oxideterm_ai::AiStreamEvent::Usage { .. }
                        | oxideterm_ai::AiStreamEvent::ProviderResponsePart { .. }
                        | oxideterm_ai::AiStreamEvent::ToolCall { .. }
                        | oxideterm_ai::AiStreamEvent::ToolCallComplete { .. } => {}
                    }
                }
            };
            // Keeping producer and delivery in one retained task ensures an
            // Entity abort cannot leave the provider request detached.
            let _ = tokio::join!(provider_stream, deliver_stream);
        });
        self.set_terminal_inline_stream_task(generation, task);
        true
    }

    fn set_terminal_inline_stream_task(
        &mut self,
        generation: u64,
        task: tokio::task::JoinHandle<()>,
    ) {
        if generation == self.terminal_inline_panel.generation && self.terminal_inline_panel.loading
        {
            if let Some(replaced_task) = self.terminal_inline_stream_task.replace(task) {
                replaced_task.abort();
            }
        } else {
            task.abort();
        }
    }

    fn abort_terminal_inline_stream(&mut self) {
        if let Some(task) = self.terminal_inline_stream_task.take() {
            task.abort();
        }
    }

    pub fn refresh_terminal_inline_key_status(
        &mut self,
        config_result: Result<oxideterm_ai::AiChatStreamConfig, String>,
    ) {
        let config = match config_result {
            Ok(config) => config,
            Err(_) => {
                self.terminal_inline_panel.has_api_key = Some(false);
                return;
            }
        };
        let requires_key = oxideterm_ai::provider_chat_requires_key(&config.provider_type);
        let Some(provider_id) = config.provider_id else {
            self.terminal_inline_panel.has_api_key = Some(!requires_key);
            return;
        };
        if !requires_key {
            self.terminal_inline_panel.has_api_key = Some(true);
            return;
        }
        let generation = self.terminal_inline_panel.generation;
        let key_store = self.key_store.clone();
        let worker_tx = self.terminal_inline_tx.clone();
        self.task_runtime.spawn(async move {
            // Opening the inline panel only checks presence, avoiding a secret
            // read and biometric prompt before the user submits anything.
            let has_key =
                tokio::task::spawn_blocking(move || key_store.has_provider_key(&provider_id))
                    .await
                    .unwrap_or(false);
            let _ = worker_tx.send(AiTerminalInlineDelivery::KeyStatus {
                generation,
                has_key,
            });
        });
    }

    pub fn chat_stream_generation(&self) -> u64 {
        self.conversation_state
            .active_conversation_id
            .as_deref()
            .and_then(|id| {
                self.chat_stream_runs
                    .iter()
                    .find(|(_, run)| run.conversation_id == id && !run.child)
                    .map(|(generation, _)| *generation)
            })
            .unwrap_or(self.chat_stream_generation)
    }

    pub fn is_chat_stream_generation(&self, generation: u64) -> bool {
        self.chat_stream_runs.contains_key(&generation)
    }

    pub fn chat_stream_matches(
        &self,
        generation: u64,
        conversation_id: &str,
        assistant_id: &str,
    ) -> bool {
        self.chat_stream_runs.get(&generation).is_some_and(|run| {
            run.conversation_id == conversation_id && run.assistant_id == assistant_id
        })
    }

    pub fn conversation_stream_generations(&self, conversation_id: &str) -> Vec<u64> {
        self.chat_stream_runs
            .iter()
            .filter(|(_, run)| run.conversation_id == conversation_id)
            .map(|(generation, _)| *generation)
            .collect()
    }

    pub fn take_queued_chat_turn(&mut self, id: &str) -> Option<AiQueuedChatTurn> {
        if self.conversation_is_loading(id)
            || !self
                .conversation_state
                .conversations
                .iter()
                .any(|c| c.id == id)
        {
            return None;
        }
        self.queued_chat_turns.get_mut(id)?.pop_front()
    }

    pub fn stash_active_chat_draft(&mut self) {
        if let Some(id) = self.conversation_state.active_conversation_id.clone() {
            self.chat_image_drafts
                .insert(id.clone(), std::mem::take(&mut self.chat_ui.images));
            self.chat_drafts.insert(
                id,
                zeroize::Zeroizing::new(std::mem::take(&mut self.chat_ui.draft)),
            );
        }
    }

    pub fn conversation_is_loading(&self, id: &str) -> bool {
        self.loading_conversations.contains(id)
    }

    pub fn chat_launch_matches(&self, id: &str, launch: Option<&str>) -> bool {
        self.conversation_state
            .conversations
            .iter()
            .any(|c| c.id == id)
            && match launch {
                Some(launch) => self
                    .chat_launches
                    .get(id)
                    .is_some_and(|current| current == launch),
                None => true,
            }
    }

    pub fn set_conversation_loading(&mut self, conversation_id: &str, loading: bool) {
        if loading {
            self.loading_conversations
                .insert(conversation_id.to_owned());
        } else {
            self.loading_conversations.remove(conversation_id);
        }
    }

    pub fn begin_chat_stream(
        &mut self,
        conversation_id: String,
        assistant_id: String,
    ) -> (u64, AiStreamDeliverySender) {
        self.cancel_chat_stream_for(&conversation_id);
        self.set_conversation_loading(&conversation_id, true);
        self.allocate_chat_stream(conversation_id, assistant_id, None)
    }

    pub fn allocate_chat_stream(
        &mut self,
        conversation_id: String,
        assistant_id: String,
        agent: Option<oxideterm_ai::agent::AgentRunRef>,
    ) -> (u64, AiStreamDeliverySender) {
        self.chat_stream_generation = self
            .chat_stream_generation
            .checked_add(1)
            .expect("chat stream identity exhausted");
        self.chat_stream_runs.insert(
            self.chat_stream_generation,
            AiChatStreamRun {
                conversation_id,
                assistant_id,
                child: agent.is_some(),
                agent,
                task: None,
            },
        );
        (self.chat_stream_generation, self.chat_stream_tx.clone())
    }

    pub fn enqueue_chat_stream_delivery(&self, delivery: AiStreamDelivery) -> bool {
        self.chat_stream_tx.send(delivery).is_ok()
    }

    pub fn set_chat_stream_task(&mut self, generation: u64, task: tokio::task::JoinHandle<()>) {
        if let Some(run) = self.chat_stream_runs.get_mut(&generation) {
            if let Some(replaced) = run.task.replace(task) {
                replaced.abort();
            }
        } else {
            task.abort();
        }
    }

    pub fn cancel_chat_stream(&mut self) -> u64 {
        if let Some(id) = self.conversation_state.active_conversation_id.clone() {
            self.cancel_chat_stream_for(&id);
        }
        self.chat_stream_generation
    }

    pub fn cancel_chat_stream_for(&mut self, conversation_id: &str) {
        self.pending_image_requests.remove(conversation_id);
        self.finish_compaction(conversation_id);
        self.history.compaction_loads.remove(conversation_id);
        self.history.compaction_sources.remove(conversation_id);
        self.agents
            .local_commands
            .retain(|(id, _, _), _| id != conversation_id);
        self.chat_launches.remove(conversation_id);
        let mut runs: Vec<_> = self
            .chat_stream_runs
            .values()
            .filter(|stream| stream.conversation_id == conversation_id)
            .filter_map(|stream| stream.agent.clone().map(|run| (stream.child, run)))
            .collect();
        for (child, run) in &runs {
            if !child {
                let _ = self.agents.services.runtime.cancel_group(run);
            }
        }
        runs.sort_by_key(|(child, _)| !*child);
        for generation in self.conversation_stream_generations(conversation_id) {
            self.complete_chat_stream(generation);
        }
        for (child, run) in runs {
            let _ = self.agents.services.runtime.complete(
                &run,
                oxideterm_ai::agent::AgentResult {
                    summary: oxideterm_ai::agent::AgentText::default(),
                    evidence: Vec::new(),
                    actions: Vec::new(),
                    unfinished: Vec::new(),
                    error_code: Some("agent_cancelled".into()),
                },
            );
            if !child {
                let _ = self.agents.services.runtime.finish_group(&run);
            }
        }
        for record in self
            .agents
            .records
            .values_mut()
            .filter(|record| record.snapshot.conversation_id == conversation_id)
        {
            for message in &mut record.messages {
                message.is_streaming = false;
                for call in &mut message.tool_calls {
                    if matches!(
                        call.get("status").and_then(serde_json::Value::as_str),
                        Some("running" | "pending" | "pending_user_approval" | "waiting_user")
                    ) {
                        call["status"] = serde_json::json!("cancelled");
                    }
                }
            }
        }
        self.agents.tool_leases.retain(|_, leases| {
            !leases.iter().any(|lease| {
                self.agents
                    .services
                    .runtime
                    .snapshots(conversation_id)
                    .iter()
                    .any(|snapshot| snapshot.run == lease.lease().owner)
            })
        });
        self.set_conversation_loading(conversation_id, false);
        self.agents.groups.retain(|_, group| {
            self.agents
                .services
                .runtime
                .snapshot(&group.parent)
                .is_ok_and(|snapshot| snapshot.conversation_id != conversation_id)
        });
        self.refresh_agent_records();
    }

    pub fn complete_chat_stream(&mut self, generation: u64) -> bool {
        let Some(run) = self.chat_stream_runs.remove(&generation) else {
            return false;
        };
        if let Some(task) = run.task {
            task.abort();
        }
        self.reject_run_tool_interactions(generation);
        true
    }

    fn reject_run_tool_interactions(&mut self, generation: u64) {
        self.tool_approval_previews
            .retain(|(run, _), _| *run != generation);
        self.history.answers.retain(|(run, _), task| {
            if *run == generation {
                task.abort();
                false
            } else {
                true
            }
        });
        self.pending_user_questions
            .retain(|(run, _), _| *run != generation);
        self.pending_cursor_requests
            .retain(|(run, _), _| *run != generation);
        let approvals: Vec<_> = self
            .pending_tool_approvals
            .keys()
            .filter(|(run, _)| *run == generation)
            .cloned()
            .collect();
        for key in approvals {
            if let Some(sender) = self.pending_tool_approvals.remove(&key) {
                let _ = sender.send(false);
            }
        }
        let choices: Vec<_> = self
            .pending_acp_permission_choices
            .keys()
            .filter(|(run, _)| *run == generation)
            .cloned()
            .collect();
        for key in choices {
            if let Some(sender) = self.pending_acp_permission_choices.remove(&key) {
                let _ = sender.send(None);
            }
        }
        let candidates: Vec<_> = self
            .pending_tool_candidate_selections
            .keys()
            .filter(|(run, _)| *run == generation)
            .cloned()
            .collect();
        for key in candidates {
            self.pending_tool_candidate_counts.remove(&key);
            if let Some(sender) = self.pending_tool_candidate_selections.remove(&key) {
                let _ = sender.send(None);
            }
        }
        if self
            .chat_ui
            .tool_candidate_selection
            .as_ref()
            .is_some_and(|selection| selection.generation == generation)
        {
            if let Some(selection) = self.chat_ui.tool_candidate_selection.take() {
                self.chat_ui.input_focused = selection.input_was_focused;
                self.chat_ui.footer_focus = selection.footer_focus;
            }
        }
    }

    pub fn active_user_question(&self) -> Option<(u64, String)> {
        let conversation = self
            .conversation_state()
            .active_conversation_id
            .as_deref()?;
        self.pending_user_questions
            .iter()
            .find(|((generation, _), question)| {
                question.conversation_id == conversation
                    && question.active()
                    && self.run_accepts_tools(*generation)
            })
            .map(|(key, _)| key.clone())
    }

    pub fn resolve_user_question(
        &mut self,
        generation: u64,
        id: &str,
        answer: zeroize::Zeroizing<String>,
    ) -> bool {
        if answer.trim().is_empty() || !self.run_accepts_tools(generation) {
            return false;
        }
        let key = (generation, id.to_owned());
        if !self
            .pending_user_questions
            .get(&key)
            .is_some_and(AiPendingUserQuestion::active)
        {
            return false;
        }
        let Some(question) = self.pending_user_questions.remove(&key) else {
            return false;
        };
        self.history_event(
            &question.conversation_id,
            "user_answer",
            &format!("{generation}:{id}"),
            serde_json::json!({"toolCallId":id,"answer":answer.as_str()}),
        );
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.history_barrier(sender);
        let task = self.task_runtime.spawn(async move {
            if receiver.await == Ok(true) && question.active() {
                let _ = question.sender.send(answer);
            }
        });
        if let Some(previous) = self.history.answers.insert(key, task) {
            previous.abort();
        }
        true
    }

    pub fn select_cursor_option(
        &mut self,
        generation: u64,
        id: &str,
        question_index: usize,
        option_index: usize,
    ) {
        if !self.run_accepts_tools(generation) {
            return;
        }
        let Some(pending) = self
            .pending_cursor_requests
            .get_mut(&(generation, id.into()))
        else {
            return;
        };
        let oxideterm_ai::CursorRequest::Questions(request) = &pending.request else {
            return;
        };
        let Some(question) = request.questions.get(question_index) else {
            return;
        };
        let Some(option) = question.options.get(option_index) else {
            return;
        };
        let selected = &mut pending.selections[question_index];
        if selected.contains(&option.id) {
            selected.retain(|id| id != &option.id);
        } else {
            if !question.allow_multiple {
                selected.clear();
            }
            selected.push(option.id.clone());
        }
    }

    pub fn resolve_cursor_request(&mut self, generation: u64, id: &str, accepted: bool) -> bool {
        if !self.run_accepts_tools(generation) {
            return false;
        }
        let key = (generation, id.into());
        let Some(pending) = self.pending_cursor_requests.get(&key) else {
            return false;
        };
        let Some(response) = pending.request.response(accepted, &pending.selections) else {
            return false;
        };
        let mut pending = self
            .pending_cursor_requests
            .remove(&key)
            .expect("validated pending request");
        if let Some(sender) = pending.response_tx.take() {
            let _ = sender.send(Ok(response));
        }
        true
    }

    pub fn update_tool_approval_preview(
        &mut self,
        generation: u64,
        message_id: &str,
        tool_call_id: &str,
        arguments: &str,
        status: &str,
    ) {
        let key = (generation, tool_call_id.to_owned());
        if status != "pending_user_approval" || !self.is_chat_stream_generation(generation) {
            self.tool_approval_previews.remove(&key);
            return;
        }
        let preview =
            zeroize::Zeroizing::new(oxideterm_ai::sanitize_json_text_for_persistence(arguments));
        if self.tool_approval_previews.insert(key, preview).is_none() {
            // Show what is being approved immediately, while keeping the existing collapse control.
            self.chat_ui
                .tool_call_expansion_state
                .insert(format!("{message_id}:{tool_call_id}"));
        }
    }

    pub fn register_tool_approval(
        &mut self,
        generation: u64,
        tool_call_id: String,
        sender: tokio::sync::oneshot::Sender<bool>,
    ) -> bool {
        if !self.run_accepts_tools(generation) {
            let _ = sender.send(false);
            return false;
        }
        if let Some(stale_sender) = self
            .pending_tool_approvals
            .insert((generation, tool_call_id), sender)
        {
            // A repeated protocol id supersedes the older waiter without
            // allowing two workers to consume one user decision.
            let _ = stale_sender.send(false);
        }
        true
    }

    pub fn resolve_tool_approval(
        &mut self,
        generation: u64,
        tool_call_id: &str,
        approved: bool,
    ) -> bool {
        let Some(sender) = self
            .pending_tool_approvals
            .remove(&(generation, tool_call_id.to_owned()))
        else {
            return false;
        };
        self.tool_approval_previews
            .remove(&(generation, tool_call_id.to_owned()));
        let _ = sender.send(approved);
        true
    }

    pub fn register_acp_permission_choice(
        &mut self,
        generation: u64,
        tool_call_id: String,
        sender: tokio::sync::oneshot::Sender<Option<String>>,
    ) -> bool {
        if !self.is_chat_stream_generation(generation) {
            let _ = sender.send(None);
            return false;
        }
        if let Some(stale_sender) = self
            .pending_acp_permission_choices
            .insert((generation, tool_call_id), sender)
        {
            let _ = stale_sender.send(None);
        }
        true
    }

    pub fn resolve_acp_permission_choice(
        &mut self,
        generation: u64,
        tool_call_id: &str,
        option_id: Option<String>,
    ) -> bool {
        let Some(sender) = self
            .pending_acp_permission_choices
            .remove(&(generation, tool_call_id.to_owned()))
        else {
            return false;
        };
        self.tool_approval_previews
            .remove(&(generation, tool_call_id.to_owned()));
        let _ = sender.send(option_id);
        true
    }

    fn reject_all_tool_approvals(&mut self) {
        self.tool_approval_previews.clear();
        self.pending_user_questions.clear();
        self.pending_cursor_requests.clear();
        for (_, sender) in self.pending_tool_approvals.drain() {
            let _ = sender.send(false);
        }
        for (_, sender) in self.pending_acp_permission_choices.drain() {
            let _ = sender.send(None);
        }
    }

    pub fn register_tool_candidate_selection(
        &mut self,
        generation: u64,
        tool_call_id: String,
        candidate_count: usize,
        sender: tokio::sync::oneshot::Sender<Option<usize>>,
    ) -> bool {
        if !self.run_accepts_tools(generation) || candidate_count == 0 {
            let _ = sender.send(None);
            return false;
        }
        if let Some(stale_sender) = self
            .pending_tool_candidate_selections
            .insert((generation, tool_call_id.clone()), sender)
        {
            // Repeated protocol IDs cancel the older selector before replacing it.
            let _ = stale_sender.send(None);
        }
        self.pending_tool_candidate_counts
            .insert((generation, tool_call_id.clone()), candidate_count);
        if !self.chat_stream_runs.get(&generation).is_some_and(|run| {
            !run.child
                && Some(&run.conversation_id)
                    == self.conversation_state.active_conversation_id.as_ref()
        }) || self.chat_ui.tool_candidate_selection.is_some()
        {
            return true;
        }
        let input_was_focused = self.chat_ui.input_focused;
        let footer_focus = self.chat_ui.footer_focus;
        self.chat_ui.tool_candidate_selection = Some(AiToolCandidateSelectionState {
            generation,
            tool_call_id,
            selected_index: 0,
            candidate_count,
            input_was_focused,
            footer_focus,
        });
        self.chat_ui.input_focused = false;
        self.chat_ui.footer_focus = None;
        true
    }

    pub fn move_tool_candidate_selection(&mut self, delta: isize) -> bool {
        let Some(selection) = self.chat_ui.tool_candidate_selection.as_mut() else {
            return false;
        };
        selection.selected_index = selection
            .selected_index
            .saturating_add_signed(delta)
            .min(selection.candidate_count.saturating_sub(1));
        true
    }

    pub fn resolve_tool_candidate_selection(
        &mut self,
        generation: u64,
        tool_call_id: &str,
        selected_index: Option<usize>,
    ) -> bool {
        let Some(sender) = self
            .pending_tool_candidate_selections
            .remove(&(generation, tool_call_id.to_owned()))
        else {
            return false;
        };
        let count = self
            .pending_tool_candidate_counts
            .remove(&(generation, tool_call_id.to_owned()))
            .unwrap_or(0);
        let selected_index = selected_index.filter(|index| *index < count);
        if self
            .chat_ui
            .tool_candidate_selection
            .as_ref()
            .is_some_and(|selection| {
                selection.generation == generation && selection.tool_call_id == tool_call_id
            })
        {
            let selection = self.chat_ui.tool_candidate_selection.take().unwrap();
            // Candidate selection temporarily owns keyboard routing. Restore the
            // exact composer focus state instead of always focusing the input.
            self.chat_ui.input_focused = selection.input_was_focused;
            self.chat_ui.footer_focus = selection.footer_focus;
        }
        let _ = sender.send(selected_index);
        true
    }

    fn reject_all_tool_candidate_selections(&mut self) {
        self.pending_tool_candidate_counts.clear();
        for (_, sender) in self.pending_tool_candidate_selections.drain() {
            let _ = sender.send(None);
        }
        if let Some(selection) = self.chat_ui.tool_candidate_selection.take() {
            // Stream cancellation and replacement release the selector through
            // the same focus-restoration boundary as an explicit user cancel.
            self.chat_ui.input_focused = selection.input_was_focused;
            self.chat_ui.footer_focus = selection.footer_focus;
        }
    }

    fn reject_all_tool_interactions(&mut self) {
        self.reject_all_tool_approvals();
        self.reject_all_tool_candidate_selections();
    }

    pub fn mcp_registry(&self) -> &oxideterm_ai::McpRegistry {
        &self.mcp_registry
    }

    pub fn set_chat_stream_frame(&mut self, source: Rc<crate::stream::AiChatStreamFrameSource>) {
        self.chat_stream_frame = Some(source);
    }

    // Called only once the window registry has acquired a live window.
    pub fn take_chat_window_request(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(u64, crate::stream::AiChatWindowDelivery)> {
        let ticket = self.chat_window_ticket?;
        let delivery = self.chat_window_delivery.take()?;
        if !delivery.is_current(self) {
            self.acknowledge_chat_window_request(ticket, cx);
            return None;
        }
        Some((ticket, delivery))
    }

    pub fn acknowledge_chat_window_request(&mut self, ticket: u64, cx: &mut Context<Self>) {
        if self.chat_window_ticket != Some(ticket) || self.chat_window_delivery.is_some() {
            return;
        }
        // A receipt confirms the broker handoff, not any async operation. The
        // next delivery revalidates its own run after cancellation or replacement.
        self.chat_window_ticket = None;
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = owner.update(cx, |ai, cx| ai.advance_chat_stream_batch(cx));
        });
    }

    pub fn compaction_sender(&self) -> AiCompactionDeliverySender {
        self.compaction_tx.clone()
    }

    pub fn take_compaction_deliveries(&mut self) -> VecDeque<AiCompactionDelivery> {
        std::mem::take(&mut self.compaction_deliveries)
    }

    pub fn begin_compaction(&mut self, conversation_id: &str) -> bool {
        if !self
            .compacting_conversations
            .insert(conversation_id.to_string())
        {
            return false;
        }
        self.history.compaction_generation += 1;
        self.history.compaction_runs.insert(
            conversation_id.into(),
            (self.history.compaction_generation, None),
        );
        true
    }

    pub fn compaction_in_progress(&self, conversation_id: &str) -> bool {
        self.compacting_conversations.contains(conversation_id)
    }

    pub fn finish_compaction(&mut self, conversation_id: &str) {
        self.compacting_conversations.remove(conversation_id);
        self.history.compaction_key_loads.remove(conversation_id);
        if let Some((_, Some(task))) = self.history.compaction_runs.remove(conversation_id) {
            task.abort();
        }
    }

    pub fn compaction_notice(&self) -> Option<&AiCompactionNotice> {
        self.compaction_notice.as_ref()
    }

    pub fn set_compaction_notice_running(&mut self, conversation_id: &str, cx: &mut Context<Self>) {
        self.compaction_notice = Some(AiCompactionNotice {
            conversation_id: conversation_id.to_string(),
            phase: AiCompactionNoticePhase::Running,
            compacted_count: None,
            timestamp_ms: ai_now_ms(),
        });
        cx.emit(AiWorkspaceEvent::CompactionStateChanged);
        cx.notify();
    }

    pub fn set_compaction_notice_done(
        &mut self,
        conversation_id: &str,
        compacted_count: usize,
        cx: &mut Context<Self>,
    ) {
        let timestamp_ms = ai_now_ms();
        self.compaction_notice = Some(AiCompactionNotice {
            conversation_id: conversation_id.to_string(),
            phase: AiCompactionNoticePhase::Done,
            compacted_count: Some(compacted_count),
            timestamp_ms,
        });
        let conversation_id = conversation_id.to_string();
        cx.spawn(async move |entity, cx| {
            Timer::after(Duration::from_secs(5)).await;
            let _ = entity.update(cx, |entity, cx| {
                let should_clear = entity.compaction_notice.as_ref().is_some_and(|notice| {
                    notice.conversation_id == conversation_id
                        && notice.phase == AiCompactionNoticePhase::Done
                        && notice.timestamp_ms == timestamp_ms
                });
                if should_clear {
                    entity.compaction_notice = None;
                    cx.emit(AiWorkspaceEvent::CompactionStateChanged);
                    cx.notify();
                }
            });
        })
        .detach();
        cx.emit(AiWorkspaceEvent::CompactionStateChanged);
        cx.notify();
    }

    pub fn clear_compaction_notice_for(&mut self, conversation_id: &str, cx: &mut Context<Self>) {
        let should_clear = self
            .compaction_notice
            .as_ref()
            .is_some_and(|notice| notice.conversation_id == conversation_id);
        if should_clear {
            self.compaction_notice = None;
            cx.emit(AiWorkspaceEvent::CompactionStateChanged);
            cx.notify();
        }
    }

    fn begin_model_refresh(&mut self, provider_id: &str) -> Option<u64> {
        if self.refreshing_models.contains(provider_id) {
            return None;
        }
        self.next_model_refresh_generation = self.next_model_refresh_generation.saturating_add(1);
        let generation = self.next_model_refresh_generation;
        self.model_refresh_generations
            .insert(provider_id.to_string(), generation);
        self.refreshing_models.insert(provider_id.to_string());
        self.model_refresh_pending = self.model_refresh_pending.saturating_add(1);
        Some(generation)
    }

    fn schedule_model_refresh_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.model_refresh_tx.wake();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // Releasing UI state stops only its waiter; in-flight HTTP work
            // remains owned by the workspace Tokio runtime.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |entity, cx| entity.drain_model_refresh_deliveries(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        delivery_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_model_refresh_deliveries(&mut self, cx: &mut Context<Self>) -> bool {
        let drain = oxideterm_workspace::delivery::drain_channel(
            &self.model_refresh_rx,
            oxideterm_workspace::delivery::USER_ACTION_DELIVERY_BUDGET,
        );
        for delivery in drain.items {
            self.model_refresh_pending = self.model_refresh_pending.saturating_sub(1);
            if self.model_refresh_generations.get(&delivery.provider_id)
                != Some(&delivery.generation)
            {
                continue;
            }
            self.refreshing_models.remove(&delivery.provider_id);
            let intent = match delivery.result {
                Ok(refresh) => AiModelRefreshIntent::Updated {
                    index: delivery.index,
                    provider_id: delivery.provider_id,
                    refresh,
                },
                Err(AiModelRefreshFailure::MissingApiKey) => AiModelRefreshIntent::MissingApiKey {
                    provider_id: delivery.provider_id,
                },
                Err(AiModelRefreshFailure::Failed(message)) => {
                    AiModelRefreshIntent::Failed { message }
                }
            };
            self.model_refresh_intents.push_back(intent);
        }
        if !self.model_refresh_intents.is_empty() {
            cx.emit(AiWorkspaceEvent::ModelRefreshDeliveryReady);
            cx.notify();
        }
        drain.outcome.backlog_remaining
    }

    fn schedule_provider_key_status_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.provider_key_status_tx.wake();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // Status probes expose only booleans and never own key material.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |entity, cx| entity.drain_provider_key_statuses(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        delivery_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_provider_key_statuses(&mut self, cx: &mut Context<Self>) -> bool {
        let drain = oxideterm_workspace::delivery::drain_channel(
            &self.provider_key_status_rx,
            oxideterm_workspace::delivery::USER_ACTION_DELIVERY_BUDGET,
        );
        let mut changed = false;
        for delivery in drain.items {
            // Ignore probes superseded by a save, delete, or provider removal
            // while the blocking keychain lookup was still running.
            if !self
                .provider_key_status_pending
                .remove(&delivery.provider_id)
            {
                continue;
            }
            let previous = self
                .provider_key_status
                .insert(delivery.provider_id, delivery.has_key);
            changed |= previous != Some(delivery.has_key);
        }
        if changed {
            cx.emit(AiWorkspaceEvent::ProviderKeyStatusChanged);
            cx.notify();
        }
        drain.outcome.backlog_remaining
    }

    fn schedule_selector_probe_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.selector_probe_tx.wake();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // Entity release stops only UI delivery, not the shared AI runtime.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |entity, cx| entity.drain_selector_probe_results(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        delivery_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_selector_probe_results(&mut self, cx: &mut Context<Self>) -> bool {
        let drain = oxideterm_workspace::delivery::drain_channel(
            &self.selector_probe_rx,
            oxideterm_workspace::delivery::USER_ACTION_DELIVERY_BUDGET,
        );
        let mut changed = false;
        for delivery in drain.items {
            self.selector_probe_pending = self.selector_probe_pending.saturating_sub(1);
            if self.selector_probe_generations.get(&delivery.provider_id)
                != Some(&delivery.generation)
            {
                continue;
            }
            let previous = self
                .selector_provider_online
                .insert(delivery.provider_id, delivery.online);
            changed |= previous != Some(delivery.online);
        }
        if changed {
            cx.emit(AiWorkspaceEvent::SelectorProviderStatusChanged);
            cx.notify();
        }
        drain.outcome.backlog_remaining
    }

    fn schedule_acp_agent_probe_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.acp_agent_probe_tx.wake();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // The ACP child process owns its runtime lifetime independently.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |entity, cx| entity.drain_acp_agent_probe_results(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        delivery_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_acp_agent_probe_results(&mut self, cx: &mut Context<Self>) -> bool {
        let drain = oxideterm_workspace::delivery::drain_channel(
            &self.acp_agent_probe_rx,
            oxideterm_workspace::delivery::USER_ACTION_DELIVERY_BUDGET,
        );
        for delivery in drain.items {
            if self
                .acp_agent_epochs
                .get(&delivery.agent_id)
                .copied()
                .unwrap_or_default()
                != delivery.epoch
            {
                continue;
            }
            self.acp_probe_tasks.remove(&delivery.agent_id);
            self.acp_agent_probe_pending.remove(&delivery.agent_id);
            self.acp_agent_probe_intents
                .push_back(AiAcpAgentProbeIntent {
                    agent_id: delivery.agent_id,
                    runtime_state: delivery.result.runtime_state,
                    auth_status: delivery.result.auth_status,
                    last_error_kind: delivery.result.last_error_kind,
                });
        }
        if !self.acp_agent_probe_intents.is_empty() {
            cx.emit(AiWorkspaceEvent::AcpAgentProbeDeliveryReady);
            cx.notify();
        }
        drain.outcome.backlog_remaining
    }

    fn schedule_acp_model_discovery_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.acp_model_discovery_tx.wake();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // A hidden selector must not discard a user-triggered completion.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |entity, cx| {
                            entity.drain_acp_model_discovery_results(cx)
                        })
                        .unwrap_or(false);
                    if backlog_remaining {
                        delivery_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_acp_model_discovery_results(&mut self, cx: &mut Context<Self>) -> bool {
        let drain = oxideterm_workspace::delivery::drain_channel(
            &self.acp_model_discovery_rx,
            oxideterm_workspace::delivery::USER_ACTION_DELIVERY_BUDGET,
        );
        for delivery in drain.items {
            if self
                .acp_agent_epochs
                .get(&delivery.agent_id)
                .copied()
                .unwrap_or_default()
                != delivery.epoch
            {
                continue;
            }
            self.acp_discovery_tasks
                .remove(&(delivery.conversation_id.clone(), delivery.agent_id.clone()));
            self.acp_model_discovery_pending
                .remove(&(delivery.conversation_id.clone(), delivery.agent_id.clone()));
            self.acp_model_discovery_intents
                .push_back(AiAcpModelDiscoveryIntent {
                    conversation_id: delivery.conversation_id,
                    agent_id: delivery.agent_id,
                    config_options: delivery.config_options,
                });
        }
        if !self.acp_model_discovery_intents.is_empty() {
            cx.emit(AiWorkspaceEvent::AcpModelDiscoveryDeliveryReady);
            cx.notify();
        }
        drain.outcome.backlog_remaining
    }

    fn schedule_terminal_inline_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.terminal_inline_tx.wake();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // The workspace runtime owns an in-flight provider request; entity
            // release only stops delivery into a destroyed UI owner.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |entity, cx| entity.drain_terminal_inline_results(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        delivery_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_terminal_inline_results(&mut self, cx: &mut Context<Self>) -> bool {
        let drain = oxideterm_workspace::delivery::drain_channel(
            &self.terminal_inline_rx,
            AI_TERMINAL_INLINE_DELIVERY_BUDGET,
        );
        let mut changed = false;
        let mut stream_finished = false;
        for delivery in drain.items {
            let panel = &mut self.terminal_inline_panel;
            match delivery {
                AiTerminalInlineDelivery::KeyStatus {
                    generation,
                    has_key,
                } if generation == panel.generation => {
                    panel.has_api_key = Some(has_key);
                    changed = true;
                }
                AiTerminalInlineDelivery::Content { generation, chunk }
                    if generation == panel.generation =>
                {
                    panel.response.push_str(&chunk);
                    changed = true;
                }
                AiTerminalInlineDelivery::Done { generation } if generation == panel.generation => {
                    panel.loading = false;
                    changed = true;
                    stream_finished = true;
                }
                AiTerminalInlineDelivery::Error {
                    generation,
                    message,
                } if generation == panel.generation => {
                    panel.loading = false;
                    panel.error = Some(message);
                    changed = true;
                    stream_finished = true;
                }
                _ => {}
            }
        }
        if stream_finished {
            // A terminal delivery closes the retained generation even if the
            // provider future has a few cleanup polls remaining.
            self.abort_terminal_inline_stream();
        }
        if changed {
            cx.emit(crate::inline::AiInlineResponseChanged);
            cx.notify();
        }
        drain.outcome.backlog_remaining
    }

    fn schedule_chat_stream_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.chat_stream_tx.wake();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| release_wake.stop()).detach();
        cx.spawn(async move |entity, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |entity, cx| entity.drain_chat_stream_results(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        delivery_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_chat_stream_results(&mut self, cx: &mut Context<Self>) -> bool {
        let drain = oxideterm_workspace::delivery::drain_channel(
            &self.chat_stream_rx,
            AI_CHAT_STREAM_DELIVERY_BUDGET,
        );
        self.chat_stream_batch.extend(drain.items);
        self.advance_chat_stream_batch(cx);
        drain.outcome.backlog_remaining
    }

    fn advance_chat_stream_batch(&mut self, cx: &mut Context<Self>) {
        if self.chat_window_ticket.is_some() || !self.chat_stream_batch.has_deliveries() {
            return;
        }
        let frame = self.chat_stream_frame.as_ref()
            .and_then(|source| source(cx))
            .unwrap_or_default();
        // Keep the unprocessed tail in the AI entity, not the window broker.
        let mut batch = std::mem::take(&mut self.chat_stream_batch);
        let delivery = batch.next_window_delivery(
            self, &frame.i18n, |data| frame.diagnostic(data), cx,
        );
        self.chat_stream_batch = batch;
        cx.notify();
        if let Some(delivery) = delivery {
            self.next_chat_window_ticket += 1;
            self.chat_window_ticket = Some(self.next_chat_window_ticket);
            self.chat_window_delivery = Some(delivery);
            cx.emit(AiWorkspaceEvent::ChatStreamDeliveryReady);
        }
    }

    fn schedule_compaction_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.compaction_tx.wake();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| release_wake.stop()).detach();
        cx.spawn(async move |entity, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |entity, cx| entity.drain_compaction_results(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        delivery_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_compaction_results(&mut self, cx: &mut Context<Self>) -> bool {
        let drain = oxideterm_workspace::delivery::drain_channel(
            &self.compaction_rx,
            oxideterm_workspace::delivery::USER_ACTION_DELIVERY_BUDGET,
        );
        if !drain.items.is_empty() {
            self.compaction_deliveries.extend(drain.items);
            cx.emit(AiWorkspaceEvent::CompactionDeliveryReady);
            cx.notify();
        }
        drain.outcome.backlog_remaining
    }
}

fn take_mcp_auth_token(draft: &mut AiMcpServerDraft) -> zeroize::Zeroizing<String> {
    // Move the draft allocation into the keychain boundary without creating
    // an intermediate token copy.
    zeroize::Zeroizing::new(std::mem::take(&mut draft.auth_token))
}

fn mcp_server_config_from_draft(
    mut draft: AiMcpServerDraft,
    server_id: String,
) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    object.insert("id".to_string(), serde_json::Value::String(server_id));
    object.insert(
        "name".to_string(),
        serde_json::Value::String(take_trimmed_mcp_value(&mut draft.name)),
    );
    object.insert(
        "transport".to_string(),
        serde_json::json!(ai_mcp_transport_value(draft.transport)),
    );
    let url = take_trimmed_mcp_value(&mut draft.url);
    if !url.is_empty() {
        object.insert("url".to_string(), serde_json::Value::String(url));
    }
    let command = take_trimmed_mcp_value(&mut draft.command);
    if !command.is_empty() {
        object.insert("command".to_string(), serde_json::Value::String(command));
    }
    let args_source = zeroize::Zeroizing::new(std::mem::take(&mut draft.args));
    let args = args_source
        .split_whitespace()
        .map(|argument| serde_json::Value::String(argument.to_string()))
        .collect::<Vec<_>>();
    if !args.is_empty() {
        object.insert("args".to_string(), serde_json::Value::Array(args));
    }
    if let Some(env) = take_mcp_record_value(&mut draft.env) {
        object.insert("env".to_string(), env);
    }
    let auth_header_name = take_trimmed_mcp_value(&mut draft.auth_header_name);
    if !auth_header_name.is_empty() && auth_header_name != "Authorization" {
        object.insert(
            "authHeaderName".to_string(),
            serde_json::Value::String(auth_header_name),
        );
    }
    if draft.auth_header_mode != oxideterm_ai::McpAuthHeaderMode::Bearer {
        object.insert(
            "authHeaderMode".to_string(),
            serde_json::json!(ai_mcp_auth_mode_value(draft.auth_header_mode)),
        );
    }
    if let Some(headers) = take_mcp_record_value(&mut draft.headers) {
        object.insert("headers".to_string(), headers);
    }
    object.insert("enabled".to_string(), serde_json::json!(true));
    if draft.retry_on_disconnect {
        object.insert("retryOnDisconnect".to_string(), serde_json::json!(true));
    }
    // The consumed draft is zeroized as soon as the required persistence
    // representation has been built.
    serde_json::Value::Object(object)
}

fn take_trimmed_mcp_value(value: &mut String) -> String {
    let mut owned_value = std::mem::take(value);
    let trimmed_range = {
        let trimmed = owned_value.trim();
        let start = trimmed.as_ptr() as usize - owned_value.as_ptr() as usize;
        start..start + trimmed.len()
    };
    owned_value.truncate(trimmed_range.end);
    owned_value.drain(..trimmed_range.start);
    owned_value
}

fn take_mcp_record_value(entries: &mut Vec<(String, String)>) -> Option<serde_json::Value> {
    let mut object = serde_json::Map::new();
    for (mut key, mut value) in std::mem::take(entries) {
        key = take_trimmed_mcp_value(&mut key);
        if key.is_empty() {
            zeroize::Zeroize::zeroize(&mut value);
            continue;
        }
        if let Some(previous_value) = object.get_mut(&key) {
            // Replacing a duplicate key must zeroize the superseded value
            // instead of relying on serde_json's ordinary String drop.
            if let serde_json::Value::String(previous_value) = previous_value {
                zeroize::Zeroize::zeroize(previous_value);
            }
            *previous_value = serde_json::Value::String(value);
            zeroize::Zeroize::zeroize(&mut key);
        } else {
            object.insert(key, serde_json::Value::String(value));
        }
    }
    (!object.is_empty()).then(|| serde_json::Value::Object(object))
}

fn ai_acp_probe_error_result(kind: &'static str) -> AiAcpAgentProbeResult {
    // Only stable categories cross the worker boundary; process errors may
    // include args, env values, or local authentication material.
    AiAcpAgentProbeResult {
        runtime_state: oxideterm_settings::AcpAgentRuntimeState::Error,
        auth_status: oxideterm_settings::AcpAgentAuthStatus::Unknown,
        last_error_kind: Some(kind.to_string()),
    }
}

impl Drop for AiWorkspaceEntity {
    fn drop(&mut self) {
        // Releasing the workspace is an explicit rejection boundary for every
        // protocol worker still waiting on a user decision.
        self.reject_all_tool_interactions();
        for run in self.chat_stream_runs.values().filter(|run| !run.child) {
            if let Some(agent) = &run.agent {
                let _ = self.agents.services.runtime.cancel_group(agent);
            }
        }
        for (_, run) in self.chat_stream_runs.drain() {
            if let Some(task) = run.task {
                task.abort();
            }
        }
        self.abort_terminal_inline_stream();
    }
}

impl gpui::EventEmitter<AiWorkspaceEvent> for AiWorkspaceEntity {}

/// Identifies the AI confirmation whose retained payload may finish exiting.
#[derive(Clone, Copy)]
pub enum AiStandardConfirmKind {
    Safety,
    Summarize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiToolCandidateSelectionState {
    pub generation: u64,
    pub tool_call_id: String,
    pub selected_index: usize,
    pub candidate_count: usize,
    input_was_focused: bool,
    footer_focus: Option<AiChatFooterAction>,
}

/// Owns AI chat presentation, conversation persistence, streaming, and compaction state.
pub struct AiChatWorkspaceState {
    pub sidebar_resizing: bool,
    pub sidebar_width: f32,
    pub overlay_window_size: Option<(f32, f32)>,
    pub overlay_window_bounds_subscription: Option<Subscription>,
    pub message_list_state: ListState,
    pub message_list_cache: RefCell<VirtualListSignatureCache>,
    pub message_signature_cache: RefCell<AiChatMessageSignatureCache>,
    pub context_token_cache: RefCell<AiContextTokenBreakdownCache>,
    pub prepared_prompt_usage: Option<AiPreparedPromptUsage>,
    pub conversation_list_open: bool,
    pub show_archived_conversations: bool,
    pub menu_open: bool,
    pub reasoning_menu_open: bool,
    pub safety_menu_open: bool,
    pub safety_confirm_open: bool,
    pub safety_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub summarize_confirm_open: bool,
    pub summarize_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub draft: String,
    pub images: Vec<oxideterm_ai::AiImageAttachment>,
    pub image_import: Option<Task<()>>,
    pub image_preview: Option<(String, Arc<gpui::Image>)>,
    pub image_preview_task: Option<Task<()>>,
    pub image_thumbnails: RefCell<HashMap<String, Arc<gpui::Image>>>,
    pub input_focused: bool,
    pub footer_focus: Option<AiChatFooterAction>,
    pub renaming_conversation_id: Option<String>,
    pub renaming_conversation_draft: String,
    pub renaming_conversation_focused: bool,
    pub editing_message_id: Option<String>,
    pub editing_message_draft: String,
    pub editing_message_focused: bool,
    pub editing_message_scroll: ScrollHandle,
    pub editing_message_caret_position: std::cell::Cell<Option<(usize, usize, usize)>>,
    pub thinking_expansion_state: HashMap<String, bool>,
    pub tool_call_expansion_state: HashSet<String>,
    pub tool_candidate_selection: Option<AiToolCandidateSelectionState>,
    pub autocomplete_index: usize,
    pub autocomplete_suppressed: bool,
    pub context_popover_open: bool,
    pub model_switch_warning_percentage: Option<usize>,
    pub context_trim_notice_count: Option<usize>,
    pub context_trim_notice_sequence: u64,
    pub include_context: bool,
    pub include_all_panes: bool,
}

/// Owns provider and model presentation state inside the AI Entity.
pub struct AiModelWorkspaceState {
    pub selector_open: bool,
    pub selector_scope: Option<AiModelSelectorScope>,
    pub selector_focus_origin: Option<BrowserFocusOrigin>,
    pub selector_search_focused: bool,
    pub selector_search_query: String,
    pub selector_expanded_providers: HashSet<String>,
    pub selector_highlighted_model: Option<(String, String)>,
    pub selector_status_signature: Option<u64>,
}

impl AiChatWorkspaceState {
    fn new(sidebar_width: f32, overlay_window_size: Option<(f32, f32)>) -> Self {
        Self {
            sidebar_resizing: false,
            sidebar_width,
            overlay_window_size,
            overlay_window_bounds_subscription: None,
            message_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                ai_chat_virtual_list_spec(),
            ),
            message_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            message_signature_cache: RefCell::new(AiChatMessageSignatureCache::default()),
            context_token_cache: RefCell::new(AiContextTokenBreakdownCache::default()),
            prepared_prompt_usage: None,
            conversation_list_open: false,
            show_archived_conversations: false,
            menu_open: false,
            reasoning_menu_open: false,
            safety_menu_open: false,
            safety_confirm_open: false,
            safety_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            summarize_confirm_open: false,
            summarize_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            draft: String::new(),
            images: Vec::new(),
            image_import: None,
            image_preview: None,
            image_preview_task: None,
            image_thumbnails: RefCell::new(HashMap::new()),
            input_focused: false,
            footer_focus: None,
            renaming_conversation_id: None,
            renaming_conversation_draft: String::new(),
            renaming_conversation_focused: false,
            editing_message_id: None,
            editing_message_draft: String::new(),
            editing_message_focused: false,
            editing_message_scroll: ScrollHandle::new(),
            editing_message_caret_position: std::cell::Cell::new(None),
            thinking_expansion_state: HashMap::new(),
            tool_call_expansion_state: HashSet::new(),
            tool_candidate_selection: None,
            autocomplete_index: 0,
            autocomplete_suppressed: false,
            context_popover_open: false,
            model_switch_warning_percentage: None,
            context_trim_notice_count: None,
            context_trim_notice_sequence: 0,
            include_context: false,
            include_all_panes: false,
        }
    }
}

impl AiModelWorkspaceState {
    fn new() -> Self {
        Self {
            selector_open: false,
            selector_scope: None,
            selector_focus_origin: None,
            selector_search_focused: false,
            selector_search_query: String::new(),
            selector_expanded_providers: HashSet::new(),
            selector_highlighted_model: None,
            selector_status_signature: None,
        }
    }
}

#[cfg(test)]
mod entity_tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn manual_disclosure_keeps_header_position_while_content_grows(cx: &mut TestAppContext) {
        use gpui::{FollowMode, ListAlignment, ListState, point, px, size};
        struct DisclosureViewport {
            state: ListState,
            expanded: bool,
        }
        impl gpui::Render for DisclosureViewport {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                let expanded = self.expanded;
                div().size_full().child(
                    gpui::list(self.state.clone(), move |index, _, _| {
                        div()
                            .w_full()
                            .h(px(if index == 0 {
                                400.0
                            } else if expanded {
                                500.0
                            } else {
                                100.0
                            }))
                            .into_any_element()
                    })
                    .size_full(),
                )
            }
        }
        for tool in [false, true] {
            let entity = cx.new(|cx| {
                AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
            });
            let state = ListState::new(2, ListAlignment::Top, px(0.0));
            state.set_follow_mode(FollowMode::Tail);
            entity.update(cx, |ai, _| ai.chat_ui.message_list_state = state.clone());
            let view = cx.new(|_| DisclosureViewport {
                state: state.clone(),
                expanded: false,
            });
            let window = cx.add_empty_window();
            let draw = |expanded: bool, cx: &mut gpui::VisualTestContext| {
                view.update(cx, |view, _| view.expanded = expanded);
                let view = view.clone();
                cx.draw(
                    point(px(0.0), px(0.0)),
                    size(px(300.0), px(300.0)),
                    move |_, _| view.into_any_element(),
                );
            };
            let mut window = window;
            draw(false, &mut window);
            let header_y = state.bounds_for_item(1).unwrap().top();
            assert_eq!(header_y, px(200.0));
            for expanded in [true, false] {
                entity.update(&mut *window, |ai, _| {
                    if tool {
                        ai.toggle_tool_call_expansion("message:tool".into());
                    } else {
                        ai.toggle_thinking_expansion("message:thinking".into(), false);
                    }
                });
                state.remeasure_items(1..2);
                draw(expanded, &mut window);
                assert_eq!(
                    state.bounds_for_item(1).unwrap().top(),
                    header_y,
                    "tool={tool}, expanded={expanded}"
                );
            }
        }
    }
    use std::sync::atomic::AtomicUsize;

    pub fn queued_turn(id: &str) -> AiQueuedChatTurn {
        AiQueuedChatTurn {
            images: Vec::new(),
            id: id.into(),
            content: zeroize::Zeroizing::new(id.into()),
            context: None,
            config: oxideterm_ai::AiChatStreamConfig {
                api_protocol: oxideterm_ai::AiApiProtocol::default(),
                image_input: Default::default(),
                omit_history_images: false,
                execution_backend: Default::default(),
                provider_id: Some("provider".into()),
                acp_agent_id: None,
                acp_session_id: None,
                acp_config_selection: None,
                provider_type: "openai".into(),
                base_url: String::new(),
                model: "queued-model".into(),
                api_key: None,
                max_response_tokens: None,
                reasoning_effort: None,
                safety_mode: Default::default(),
                profile_id: None,
                memory_context: None,
                memory_entry_ids: Vec::new(),
                tool_policy: Default::default(),
                tools: Vec::new(),
                tool_choice: oxideterm_ai::AiToolChoice::Auto,
            },
            request_content: None,
            task_system_prompt: None,
            participant: None,
            skill: None,
        }
    }

    #[gpui::test]
    fn quitting_flushes_history_without_a_foreground_tokio_runtime(cx: &mut TestAppContext) {
        use oxideterm_ai::{ConversationStore, HistoryWriter};
        let directory = tempfile::tempdir().unwrap();
        let store = ConversationStore::open(directory.path().join("history.redb")).unwrap();
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(
                test_worker_runtime(),
                oxideterm_ai::AiProviderKeyStore::new(),
                cx,
            )
        });
        entity.update(cx, |ai, _| {
            ai.history.writer = Some(HistoryWriter::new(store.clone()).unwrap());
            ai.history.store = Some(store.clone());
            ai.conversation_state.create_conversation(
                "quit-test".into(),
                Some("Saved at quit".into()),
                1,
                None,
            );
            ai.history_created("quit-test");
        });
        assert!(tokio::runtime::Handle::try_current().is_err());
        // Quit must wait for a real writer thread rather than exhaust simulated ticks.
        cx.background_executor.allow_parking();
        cx.background_executor.set_block_on_ticks(100_000..=100_000);
        cx.update(|app| app.shutdown());
        assert_eq!(
            store
                .conversation_head("quit-test")
                .unwrap()
                .unwrap()
                .conversation
                .title,
            "Saved at quit"
        );
    }

    #[gpui::test]
    fn conversation_lists_load_active_first_and_archived_on_demand(cx: &mut TestAppContext) {
        use oxideterm_ai::{AiChatState, ConversationStore, HistoryMutation};
        let directory = tempfile::tempdir().unwrap();
        let store = ConversationStore::open(directory.path().join("chat_history.v4.redb")).unwrap();
        let mut state = AiChatState::default();
        for index in 0..120 {
            let id = format!("chat-{index:03}");
            state.create_conversation(id, None, index, None);
            let mut conversation = state.conversations.remove(0);
            conversation.archived = index % 2 == 0;
            store
                .apply(vec![HistoryMutation::Create {
                    conversation,
                    revision: index as u64 + 1,
                }])
                .unwrap();
        }
        let runtime = test_runtime();
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(runtime.clone(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        entity.update(cx, |ai, cx| {
            ai.initialize_history(directory.path().join("chat_history.redb"), cx)
        });
        let settle = |cx: &mut TestAppContext, archived: bool| {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                runtime.block_on(tokio::task::yield_now());
                cx.run_until_parked();
                if entity.read_with(cx, |ai, _| {
                    let page = ai.conversation_list_page(archived);
                    page.initialized && !page.loading()
                }) {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "conversation list did not arrive"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        };
        settle(cx, false);
        entity.update(cx, |ai, _| {
            assert_eq!(
                ai.conversation_state
                    .conversations
                    .iter()
                    .map(|c| c.id.clone())
                    .collect::<Vec<_>>(),
                (0..50)
                    .map(|i| format!("chat-{:03}", 119 - i * 2))
                    .collect::<Vec<_>>()
            );
            assert!(!ai.conversation_list_page(true).initialized);
            assert!(!ai.conversation_list_page(true).loading());
            ai.request_conversation_list_page(false);
        });
        settle(cx, false);
        entity.update(cx, |ai, _| {
            assert_eq!(
                ai.conversation_state
                    .conversations
                    .iter()
                    .map(|c| c.id.clone())
                    .collect::<Vec<_>>(),
                (0..60)
                    .map(|i| format!("chat-{:03}", 119 - i * 2))
                    .collect::<Vec<_>>()
            );
            assert!(!ai.conversation_list_page(false).has_more);
            ai.show_archived_conversations(true);
        });
        settle(cx, true);
        entity.update(cx, |ai, _| {
            assert_eq!(
                ai.conversation_state
                    .conversations
                    .iter()
                    .filter(|c| c.archived)
                    .map(|c| c.id.clone())
                    .collect::<Vec<_>>(),
                (0..50)
                    .map(|i| format!("chat-{:03}", 118 - i * 2))
                    .collect::<Vec<_>>()
            );
            assert!(ai.conversation_list_page(true).has_more);
            ai.request_conversation_list_page(true);
        });
        settle(cx, true);
        entity.read_with(cx, |ai, _| {
            assert_eq!(
                ai.conversation_state
                    .conversations
                    .iter()
                    .filter(|c| c.archived)
                    .map(|c| c.id.clone())
                    .collect::<Vec<_>>(),
                (0..60)
                    .map(|i| format!("chat-{:03}", 118 - i * 2))
                    .collect::<Vec<_>>()
            );
            assert!(!ai.conversation_list_page(true).has_more);
        });
    }

    #[gpui::test]
    fn archiving_switches_active_chat_and_restoring_retains_draft(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        entity.update(cx, |ai, _| {
            ai.conversation_state
                .create_conversation("a".into(), None, 1, None);
            ai.conversation_state
                .create_conversation("b".into(), None, 2, None);
            ai.set_chat_draft("unfinished draft".into());
            ai.set_conversation_archived("b", true);
            assert_eq!(
                ai.conversation_state.active_conversation_id.as_deref(),
                Some("a")
            );
            assert!(
                ai.conversation_state
                    .conversations
                    .iter()
                    .find(|c| c.id == "b")
                    .unwrap()
                    .archived
            );
            ai.set_conversation_archived("a", true);
            assert_eq!(ai.conversation_state.active_conversation_id, None);
            ai.set_conversation_archived("b", false);
            ai.select_conversation("b".into());
            assert!(
                !ai.conversation_state
                    .active_conversation()
                    .unwrap()
                    .archived
            );
            assert_eq!(ai.chat_ui.draft.as_str(), "unfinished draft");
        });
    }

    #[gpui::test]
    fn message_queue_is_fifo_scoped_and_retained_on_stop(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        entity.update(cx, |ai, _| {
            ai.conversation_state
                .create_conversation("a".into(), None, 1, None);
            ai.conversation_state
                .create_conversation("b".into(), None, 2, None);
            let mut responses_turn = queued_turn("a1");
            responses_turn.config.api_protocol = oxideterm_ai::AiApiProtocol::Responses;
            ai.queued_chat_turns.insert(
                "a".into(),
                VecDeque::from([responses_turn, queued_turn("a2")]),
            );
            ai.queued_chat_turns
                .insert("b".into(), VecDeque::from([queued_turn("b1")]));
            ai.begin_chat_stream("a".into(), "assistant".into());
            assert!(ai.conversation_is_loading("a"));
            assert!(ai.take_queued_chat_turn("a").is_none());
            assert_eq!(ai.take_queued_chat_turn("b").unwrap().id, "b1");
            ai.cancel_chat_stream_for("a");
            let first = ai.take_queued_chat_turn("a").unwrap();
            assert_eq!(
                (
                    first.id.as_str(),
                    first.config.model.as_str(),
                    first.config.api_protocol
                ),
                ("a1", "queued-model", oxideterm_ai::AiApiProtocol::Responses)
            );
            assert_eq!(ai.take_queued_chat_turn("a").unwrap().id, "a2");
            assert!(ai.take_queued_chat_turn("a").is_none());
            ai.queued_chat_turns
                .insert("a".into(), VecDeque::from([queued_turn("deleted")]));
            ai.delete_conversation("a");
            assert!(!ai.queued_chat_turns.contains_key("a"));
        });
    }

    #[gpui::test]
    fn stopped_preparation_cannot_resume_after_a_new_launch_or_deletion(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        entity.update(cx, |ai, _| {
            ai.conversation_state
                .create_conversation("a".into(), None, 1, None);
            ai.chat_launches.insert("a".into(), "first".into());
            assert!(ai.chat_launch_matches("a", Some("first")));
            ai.cancel_chat_stream_for("a");
            assert!(!ai.chat_launch_matches("a", Some("first")));
            ai.chat_launches.insert("a".into(), "second".into());
            assert!(!ai.chat_launch_matches("a", Some("first")));
            assert!(ai.chat_launch_matches("a", Some("second")));
            ai.delete_conversation("a");
            assert!(!ai.chat_launch_matches("a", Some("second")));
        });
    }

    #[gpui::test]
    fn switching_conversations_preserves_each_unsent_draft(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        entity.update(cx, |ai, _| {
            ai.conversation_state
                .create_conversation("a".into(), None, 1, None);
            ai.conversation_state
                .create_conversation("b".into(), None, 2, None);
            ai.set_chat_draft("draft-b".into());
            let image = oxideterm_ai::AiImageAttachment::new(
                "screen.png".into(),
                "image/png".into(),
                200,
                100,
                b"pixel",
                b"thumb",
            )
            .unwrap();
            ai.chat_images_mut().push(image.clone());
            ai.select_conversation("a".into());
            assert!(ai.chat_ui.draft.is_empty());
            assert!(ai.chat_ui.images.is_empty());
            ai.set_chat_draft("draft-a".into());
            ai.select_conversation("b".into());
            assert_eq!(ai.chat_ui.draft, "draft-b");
            assert_eq!(ai.chat_ui.images, vec![image]);
            ai.select_conversation("a".into());
            assert_eq!(ai.chat_ui.draft, "draft-a");
            assert!(ai.chat_ui.images.is_empty());
        });
    }

    fn test_runtime() -> Arc<tokio::runtime::Runtime> {
        Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("AI entity test runtime"),
        )
    }

    fn test_worker_runtime() -> Arc<tokio::runtime::Runtime> {
        Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("AI worker lifecycle test runtime"),
        )
    }

    struct CountTaskDrop(Arc<AtomicUsize>);

    impl Drop for CountTaskDrop {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::AcqRel);
        }
    }

    fn spawn_abort_counted_task(
        runtime: &tokio::runtime::Runtime,
        drop_count: Arc<AtomicUsize>,
    ) -> tokio::task::JoinHandle<()> {
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
        let task = runtime.spawn(async move {
            let _drop_count = CountTaskDrop(drop_count);
            started_tx
                .send(())
                .expect("lifecycle test should observe task start");
            std::future::pending::<()>().await;
        });
        started_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("retained task should start");
        task
    }

    fn wait_for_lifecycle_count(counter: &AtomicUsize, expected: usize) {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while counter.load(Ordering::Acquire) != expected {
            assert!(
                std::time::Instant::now() < deadline,
                "lifecycle counter should reach the expected value"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[gpui::test]
    fn acp_plugin_change_discards_old_probe_results_and_cancels_owned_probes(
        cx: &mut TestAppContext,
    ) {
        let runtime = test_runtime();
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(runtime.clone(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let task = runtime.spawn(std::future::pending::<()>());
        entity.update(cx, |entity, cx| {
            entity
                .acp_probe_tasks
                .insert("agent".into(), task.abort_handle());
            entity.acp_agent_probe_pending.insert("agent".into());
            entity
                .acp_agent_probe_tx
                .send(AiAcpAgentProbeDelivery {
                    epoch: 0,
                    agent_id: "agent".into(),
                    result: ai_acp_probe_error_result("initialize"),
                })
                .unwrap();
            entity.invalidate_acp_agent_metadata("agent");
            entity
                .acp_agent_probe_tx
                .send(AiAcpAgentProbeDelivery {
                    epoch: 1,
                    agent_id: "agent".into(),
                    result: ai_acp_probe_error_result("config"),
                })
                .unwrap();
            entity.drain_acp_agent_probe_results(cx);
            let intents = entity.take_acp_agent_probe_intents();
            assert_eq!(
                intents
                    .iter()
                    .map(|intent| (intent.agent_id.as_str(), intent.last_error_kind.as_deref()))
                    .collect::<Vec<_>>(),
                [("agent", Some("config"))]
            );
        });
        assert!(runtime.block_on(task).unwrap_err().is_cancelled());
    }

    fn test_acp_agent(agent_id: &str) -> oxideterm_settings::AcpAgentConfig {
        oxideterm_settings::AcpAgentConfig {
            id: agent_id.to_string(),
            display_name: "Test Agent".to_string(),
            plugin_id: None,
            command: "test-agent".to_string(),
            args: Vec::new(),
            env: std::collections::BTreeMap::new(),
            cwd: None,
            enabled: true,
            auth: Default::default(),
            capability_policy: Default::default(),
            status: Default::default(),
        }
    }

    #[gpui::test]
    fn chat_confirmation_reopen_and_keyboard_actions_preserve_current_payload_once(
        cx: &mut TestAppContext,
    ) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        entity.update(cx, |entity, cx| {
            entity.open_chat_confirm(AiChatConfirmKind::ClearAll, cx);
            assert_eq!(
                entity.begin_chat_confirm_exit(false, Duration::from_secs(60), cx),
                (true, None)
            );
            assert!(entity.chat_confirm_exit_task.is_some());

            entity.open_chat_confirm(
                AiChatConfirmKind::DeleteMessage {
                    message_id: Arc::from("message-new"),
                },
                cx,
            );
            assert!(entity.chat_confirm_exit_task.is_none());
            assert_eq!(
                entity.chat_confirm_snapshot(),
                Some(AiChatConfirmSnapshot {
                    kind: AiChatConfirmKind::DeleteMessage {
                        message_id: Arc::from("message-new"),
                    },
                    phase: oxideterm_gpui_ui::motion::ExitPhase::Visible,
                    focused_action: None,
                })
            );
        });
        entity.update(cx, |entity, cx| {
            entity.open_chat_confirm(AiChatConfirmKind::ClearAll, cx);
            assert_eq!(
                entity.handle_chat_confirm_key("escape", false, false, cx),
                Some(AiChatConfirmKeyAction::Cancel)
            );
            assert_eq!(
                entity.begin_chat_confirm_exit(false, Duration::ZERO, cx),
                (true, None)
            );

            entity.open_chat_confirm(
                AiChatConfirmKind::DeleteMessage {
                    message_id: Arc::from("message-a"),
                },
                cx,
            );
            assert_eq!(
                entity.handle_chat_confirm_key("end", false, false, cx),
                Some(AiChatConfirmKeyAction::Handled)
            );
            assert_eq!(
                entity.handle_chat_confirm_key("enter", false, false, cx),
                Some(AiChatConfirmKeyAction::Confirm)
            );
            assert_eq!(
                entity.begin_chat_confirm_exit(true, Duration::ZERO, cx),
                (
                    true,
                    Some(AiChatConfirmEffect::DeleteMessage {
                        message_id: Arc::from("message-a"),
                    })
                )
            );
            assert_eq!(
                entity.begin_chat_confirm_exit(true, Duration::ZERO, cx),
                (false, None)
            );
        });
    }

    #[gpui::test]
    fn provider_key_store_failure_and_queued_removal_preserve_the_owner(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        entity.update(cx, |entity, cx| {
            assert!(entity.start_provider_key_operation(
                "provider-test".to_string(),
                AiProviderKeyOperation::Store { index: 3 },
                std::future::ready(true),
                cx,
            ));
            assert!(
                entity
                    .provider_key_operation_tasks
                    .contains_key("provider-test")
            );
        });
        cx.run_until_parked();

        entity.update(cx, |entity, _cx| {
            assert!(
                !entity
                    .provider_key_operation_tasks
                    .contains_key("provider-test")
            );
            assert!(entity.provider_has_key("provider-test"));
            let intent = entity
                .take_credential_intents()
                .pop_front()
                .expect("provider completion intent");
            assert!(matches!(
                intent,
                AiCredentialIntent::ProviderKeyStored {
                    index: 3,
                    provider_id
                } if provider_id == "provider-test"
            ));
        });

        entity.update(cx, |entity, cx| {
            assert!(entity.start_provider_key_operation(
                "provider-test".to_string(),
                AiProviderKeyOperation::Remove,
                std::future::ready(false),
                cx,
            ));
        });
        cx.run_until_parked();
        entity.update(cx, |entity, _cx| {
            let intent = entity
                .take_credential_intents()
                .pop_front()
                .expect("provider failure intent");
            assert!(matches!(
                intent,
                AiCredentialIntent::Failed(AiCredentialFailure::RemoveProviderKey)
            ));
        });

        entity.update(cx, |entity, cx| {
            assert!(entity.start_provider_key_operation(
                "provider-test".to_string(),
                AiProviderKeyOperation::Store { index: 2 },
                std::future::pending(),
                cx,
            ));
            assert!(entity.remove_provider_key("provider-test".to_string(), cx));
            assert!(
                entity
                    .pending_provider_key_removals
                    .contains("provider-test")
            );
            assert_eq!(entity.provider_key_operation_tasks.len(), 1);
        });
    }

    #[gpui::test]
    fn mcp_token_moves_once_and_failure_restores_only_non_token_draft(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let mut restore_draft = AiMcpServerDraft::default();
        restore_draft.name = "server-test".to_string();
        restore_draft.auth_token = "test-token".to_string();
        let token_allocation = restore_draft.auth_token.as_ptr();
        let token = take_mcp_auth_token(&mut restore_draft);
        assert_eq!(token.as_ptr(), token_allocation);
        assert!(restore_draft.auth_token.is_empty());
        drop(token);

        entity.update(cx, |entity, cx| {
            entity.start_mcp_token_save(
                restore_draft,
                "mcp-test".to_string(),
                std::future::ready(false),
                cx,
            );
            assert!(entity.mcp_save_task.is_some());
        });
        cx.run_until_parked();

        entity.update(cx, |entity, _cx| {
            assert!(entity.mcp_save_task.is_none());
            let intent = entity
                .take_credential_intents()
                .pop_front()
                .expect("MCP failure intent");
            assert!(matches!(
                intent,
                AiCredentialIntent::McpServerSaveFailed { draft }
                    if draft.auth_token.is_empty() && draft.name == "server-test"
            ));
        });
    }

    #[test]
    fn mcp_config_consumes_sensitive_record_allocations() {
        let mut draft = AiMcpServerDraft::default();
        draft.name = "server-test".to_string();
        draft
            .env
            .push(("TOKEN".to_string(), "sensitive-value".to_string()));
        let sensitive_allocation = draft.env[0].1.as_ptr();

        let config = mcp_server_config_from_draft(draft, "mcp-test".to_string());

        let persisted_value = config
            .get("env")
            .and_then(|env| env.get("TOKEN"))
            .and_then(serde_json::Value::as_str)
            .expect("persisted MCP environment value");
        assert_eq!(persisted_value.as_ptr(), sensitive_allocation);
    }

    #[gpui::test]
    fn mcp_runtime_task_is_entity_retained_and_delivers_once(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        entity.update(cx, |entity, cx| {
            assert!(entity.start_mcp_runtime_task(
                "server-test".to_string(),
                std::future::ready(Some("server-test".to_string())),
                cx,
            ));
            assert!(entity.mcp_runtime_tasks.contains_key("server-test"));
            assert!(!entity.start_mcp_runtime_task(
                "server-test".to_string(),
                std::future::ready(None),
                cx,
            ));
        });
        cx.run_until_parked();

        entity.update(cx, |entity, _cx| {
            assert!(!entity.mcp_runtime_tasks.contains_key("server-test"));
            let intent = entity
                .take_credential_intents()
                .pop_front()
                .expect("MCP removal intent");
            assert!(matches!(
                intent,
                AiCredentialIntent::McpServerRemoved { server_id }
                    if server_id == "server-test"
            ));
            assert!(entity.take_credential_intents().is_empty());
        });
    }

    #[gpui::test]
    fn hidden_ai_surfaces_suspend_ui_probes_and_resume_mcp_ticks(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        entity.update(cx, |entity, cx| {
            assert_eq!(
                entity.workspace_visibility(),
                AiWorkspaceVisibility::default()
            );
            assert!(!entity.request_mcp_status_tick(Duration::from_secs(60), cx));
            entity.request_provider_key_statuses(["provider-a".to_string()]);
            assert!(entity.provider_key_status_pending.is_empty());
            assert!(!entity.request_selector_provider_probe(
                oxideterm_ai::AiProviderView {
                    api_protocol: oxideterm_ai::AiApiProtocol::default(),
                    id: "provider-a".to_string(),
                    provider_type: "ollama".to_string(),
                    name: "Provider A".to_string(),
                    base_url: "http://127.0.0.1:11434".to_string(),
                    models: Vec::new(),
                    enabled: true,
                    custom: false,
                },
                "/api/tags",
            ));
            assert!(!entity.request_acp_agent_probe(test_acp_agent("agent-a")));
            assert!(!entity.request_acp_model_discovery(
                "conversation-a".to_string(),
                test_acp_agent("agent-a"),
                std::path::PathBuf::new(),
            ));

            assert!(entity.set_workspace_visibility(AiWorkspaceVisibility {
                model_selector_surface: false,
                settings_surface: true,
            }));
            assert!(entity.request_mcp_status_tick(Duration::from_secs(60), cx));
            assert!(entity.mcp_status_tick_task.is_some());
            assert!(!entity.request_mcp_status_tick(Duration::from_secs(60), cx));

            assert!(entity.set_workspace_visibility(AiWorkspaceVisibility::default()));
            assert!(entity.mcp_status_tick_task.is_none());
            assert!(entity.set_workspace_visibility(AiWorkspaceVisibility {
                model_selector_surface: false,
                settings_surface: true,
            }));
            assert!(entity.request_mcp_status_tick(Duration::from_secs(60), cx));
        });
    }

    #[gpui::test]
    fn invalidating_provider_key_status_clears_cached_and_pending_state(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });

        let worker_tx = entity.update(cx, |entity, _cx| {
            entity.set_provider_key_status("provider-a".to_string(), true);
            entity
                .provider_key_status_pending
                .insert("provider-a".to_string());
            entity.invalidate_provider_key_status("provider-a");
            assert!(!entity.provider_has_key("provider-a"));
            assert!(!entity.provider_key_status_pending.contains("provider-a"));
            entity.provider_key_status_tx.clone()
        });
        worker_tx
            .send(AiProviderKeyStatusDelivery {
                provider_id: "provider-a".to_string(),
                has_key: true,
            })
            .unwrap();

        cx.run_until_parked();

        cx.read(|cx| {
            assert!(!entity.read(cx).provider_has_key("provider-a"));
        });
    }

    #[gpui::test]
    fn direct_selector_status_rejects_stale_probe_completion(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let worker_tx = entity.update(cx, |entity, _cx| {
            entity
                .selector_probe_generations
                .insert("provider-a".to_string(), 7);
            entity.selector_probe_pending = 1;
            entity.set_selector_provider_online("provider-a".to_string(), true);
            entity.selector_probe_tx.clone()
        });
        worker_tx
            .send(AiModelSelectorProbeDelivery {
                provider_id: "provider-a".to_string(),
                generation: 7,
                online: false,
            })
            .unwrap();

        cx.run_until_parked();

        cx.read(|cx| {
            let entity = entity.read(cx);
            assert!(entity.selector_provider_is_online("provider-a"));
            assert_eq!(entity.selector_probe_pending, 0);
        });
    }

    #[gpui::test]
    fn stream_replacement_cancel_and_release_abort_each_owned_task_once(cx: &mut TestAppContext) {
        let runtime = test_worker_runtime();
        let entity = cx.new({
            let runtime = Arc::clone(&runtime);
            move |cx| AiWorkspaceEntity::new(runtime, oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let chat_drop_count = Arc::new(AtomicUsize::new(0));
        let terminal_drop_count = Arc::new(AtomicUsize::new(0));

        let chat_generation = entity.update(cx, |entity, _cx| {
            entity
                .begin_chat_stream("conversation-a".into(), "assistant-a".into())
                .0
        });
        let first_chat_task = spawn_abort_counted_task(&runtime, Arc::clone(&chat_drop_count));
        entity.update(cx, |entity, _cx| {
            entity.set_chat_stream_task(chat_generation, first_chat_task)
        });
        let replacement_chat_task =
            spawn_abort_counted_task(&runtime, Arc::clone(&chat_drop_count));
        entity.update(cx, |entity, _cx| {
            entity.set_chat_stream_task(chat_generation, replacement_chat_task)
        });
        wait_for_lifecycle_count(&chat_drop_count, 1);

        let terminal_generation = entity.update(cx, |entity, _cx| {
            entity.terminal_inline_panel.loading = true;
            entity.terminal_inline_panel.generation =
                entity.terminal_inline_panel.generation.wrapping_add(1);
            entity.terminal_inline_panel.generation
        });
        let first_terminal_task =
            spawn_abort_counted_task(&runtime, Arc::clone(&terminal_drop_count));
        entity.update(cx, |entity, _cx| {
            entity.set_terminal_inline_stream_task(terminal_generation, first_terminal_task)
        });
        let replacement_terminal_task =
            spawn_abort_counted_task(&runtime, Arc::clone(&terminal_drop_count));
        entity.update(cx, |entity, _cx| {
            entity.set_terminal_inline_stream_task(terminal_generation, replacement_terminal_task)
        });
        wait_for_lifecycle_count(&terminal_drop_count, 1);

        entity.update(cx, |entity, _cx| {
            entity.cancel_chat_stream_for("conversation-a");
            entity.close_terminal_inline_panel();
            // Repeated cancellation cannot abort an already-consumed handle.
            entity.cancel_chat_stream_for("conversation-a");
            entity.close_terminal_inline_panel();
        });
        wait_for_lifecycle_count(&chat_drop_count, 2);
        wait_for_lifecycle_count(&terminal_drop_count, 2);

        let chat_generation = entity.update(cx, |entity, _cx| {
            entity
                .begin_chat_stream("conversation-a".into(), "assistant-a".into())
                .0
        });
        let chat_task = spawn_abort_counted_task(&runtime, Arc::clone(&chat_drop_count));
        let terminal_task = spawn_abort_counted_task(&runtime, Arc::clone(&terminal_drop_count));
        entity.update(cx, |entity, _cx| {
            entity.set_chat_stream_task(chat_generation, chat_task);
            entity.terminal_inline_panel.loading = true;
            entity.terminal_inline_panel.generation =
                entity.terminal_inline_panel.generation.wrapping_add(1);
            let terminal_generation = entity.terminal_inline_panel.generation;
            entity.set_terminal_inline_stream_task(terminal_generation, terminal_task);
        });

        drop(entity);
        cx.update(|_cx| {});
        cx.run_until_parked();

        wait_for_lifecycle_count(&chat_drop_count, 3);
        wait_for_lifecycle_count(&terminal_drop_count, 3);
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(chat_drop_count.load(Ordering::Acquire), 3);
        assert_eq!(terminal_drop_count.load(Ordering::Acquire), 3);
    }

    #[gpui::test]
    fn hidden_ai_workspace_keeps_terminal_inline_stream_running(cx: &mut TestAppContext) {
        let runtime = test_worker_runtime();
        let entity = cx.new({
            let runtime = Arc::clone(&runtime);
            move |cx| AiWorkspaceEntity::new(runtime, oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let completed = Arc::new(AtomicUsize::new(0));
        let completed_for_task = Arc::clone(&completed);
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
        let terminal_task = runtime.spawn(async move {
            started_tx
                .send(())
                .expect("visibility test should observe task start");
            if release_rx.await.is_ok() {
                completed_for_task.fetch_add(1, Ordering::AcqRel);
            }
        });
        started_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("terminal inline stream should start");
        entity.update(cx, |entity, _cx| {
            entity.terminal_inline_panel.loading = true;
            entity.terminal_inline_panel.generation =
                entity.terminal_inline_panel.generation.wrapping_add(1);
            let terminal_generation = entity.terminal_inline_panel.generation;
            entity.set_terminal_inline_stream_task(terminal_generation, terminal_task);
            entity.set_workspace_visibility(AiWorkspaceVisibility {
                model_selector_surface: true,
                settings_surface: false,
            });
            entity.set_workspace_visibility(AiWorkspaceVisibility::default());
        });

        release_tx
            .send(())
            .expect("hiding AI surfaces must not abort terminal inline streaming");
        wait_for_lifecycle_count(&completed, 1);
        entity.read_with(cx, |entity, _cx| {
            assert!(entity.terminal_inline_stream_task.is_some());
        });
    }

    #[gpui::test]
    fn inline_stream_invalidates_only_the_panel(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let response_updates = Rc::new(std::cell::Cell::new(0));
        let mount_updates = Rc::new(std::cell::Cell::new(0));
        let (_response, _mount) = cx.update(|cx| {
            let response_updates = response_updates.clone();
            let mount_updates = mount_updates.clone();
            (
                cx.subscribe(
                    &entity,
                    move |_, _: &crate::inline::AiInlineResponseChanged, _| {
                        response_updates.set(response_updates.get() + 1);
                    },
                ),
                cx.subscribe(&entity, move |_, event: &AiWorkspaceEvent, _| {
                    let _ = event;
                    mount_updates.set(mount_updates.get() + 1);
                }),
            )
        });
        let (generation, sender) = entity.update(cx, |ai, _| {
            ai.open_terminal_inline_panel(String::new());
            ai.terminal_inline_panel.loading = true;
            (
                ai.terminal_inline_panel.generation,
                ai.terminal_inline_tx.clone(),
            )
        });
        for chunk in ["echo ", "hello"] {
            sender
                .send(AiTerminalInlineDelivery::Content {
                    generation,
                    chunk: chunk.into(),
                })
                .unwrap();
            cx.run_until_parked();
        }
        assert_eq!(response_updates.get(), 2);
        assert_eq!(mount_updates.get(), 0);
        entity.read_with(cx, |ai, _| {
            assert_eq!(ai.terminal_inline_panel.response, "echo hello")
        });

        sender
            .send(AiTerminalInlineDelivery::Done { generation })
            .unwrap();
        cx.run_until_parked();
        assert_eq!(response_updates.get(), 3);
        assert_eq!(mount_updates.get(), 0);
    }

    #[gpui::test]
    fn chat_stream_generation_task_boundary_and_delivery_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let (generation, worker_tx) = entity.update(cx, |entity, _cx| {
            entity.set_workspace_visibility(AiWorkspaceVisibility {
                model_selector_surface: true,
                settings_surface: false,
            });
            entity.create_chat_conversation("conversation-a".into(), None, 1);
            let mut message = oxideterm_ai::tool_loop::agent_chat_message(oxideterm_ai::AiChatRole::Assistant, String::new());
            message.id = "assistant-a".into();
            message.is_streaming = true;
            entity.add_message("conversation-a", message);
            let stream = entity.begin_chat_stream("conversation-a".into(), "assistant-a".into());
            entity.set_workspace_visibility(AiWorkspaceVisibility::default());
            stream
        });
        use crate::stream::AiChatWindowDelivery;

        let delivery = |event| AiStreamDelivery {
            generation,
            conversation_id: "conversation-a".into(),
            assistant_id: "assistant-a".into(),
            event,
        };
        let content = |text: &str| AiStreamDeliveryEvent::Stream(
            oxideterm_ai::AiStreamEvent::Content(text.to_owned()),
        );
        let (sender, mut response) = tokio::sync::oneshot::channel();
        for event in [
            content("before"),
            AiStreamDeliveryEvent::RuntimeContextRequested { tool_session_id: oxideterm_ai::ToolSessionId::new(), sender },
            content(" after"),
            AiStreamDeliveryEvent::Stream(oxideterm_ai::AiStreamEvent::Done),
            content(" stale"),
        ] {
            worker_tx.send(delivery(event)).unwrap();
        }
        cx.run_until_parked();

        let displayed = |ai: &AiWorkspaceEntity| {
            ai.conversation_state().active_conversation().unwrap()
                .messages.iter().find(|message| message.id == "assistant-a").unwrap()
                .content.clone()
        };
        let (first_ticket, request) = entity.update(cx, |ai, cx| {
            assert_eq!(displayed(ai), "before");
            let request = ai.take_chat_window_request(cx).unwrap();
            assert!(ai.take_chat_window_request(cx).is_none());
            request
        });
        let AiChatWindowDelivery::Host(AiStreamDelivery {
            event: AiStreamDeliveryEvent::RuntimeContextRequested { sender, .. }, ..
        }) = request else {
            panic!("expected the runtime context request before the trailing chunk");
        };
        sender.send(Some("runtime context".into())).unwrap();
        assert_eq!(response.try_recv().unwrap().as_deref(), Some("runtime context"));
        entity.update(cx, |ai, cx| ai.acknowledge_chat_window_request(first_ticket + 1, cx));
        cx.run_until_parked();
        entity.update(cx, |ai, cx| {
            assert_eq!(displayed(ai), "before");
            assert!(ai.take_chat_window_request(cx).is_none());
            ai.acknowledge_chat_window_request(first_ticket, cx);
        });
        cx.run_until_parked();

        entity.update(cx, |ai, cx| {
            assert_eq!(displayed(ai), "before after");
            let (second_ticket, request) = ai.take_chat_window_request(cx).unwrap();
            assert!(matches!(request, AiChatWindowDelivery::Host(AiStreamDelivery {
                event: AiStreamDeliveryEvent::Stream(oxideterm_ai::AiStreamEvent::Done), ..
            })));
            assert!(ai.complete_chat_stream(generation));
            ai.acknowledge_chat_window_request(first_ticket, cx);
            assert!(ai.take_chat_window_request(cx).is_none());
            ai.acknowledge_chat_window_request(second_ticket, cx);
        });
        cx.run_until_parked();
        entity.update(cx, |ai, cx| {
            assert_eq!(displayed(ai), "before after");
            assert!(!ai.is_chat_stream_generation(generation));
            assert!(ai.take_chat_window_request(cx).is_none());
        });
    }

    #[gpui::test]
    fn approval_preview_is_visible_redacted_and_cleared_after_interaction(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        entity.update(cx, |entity, _cx| {
            let (generation, _) =
                entity.begin_chat_stream("conversation-a".into(), "assistant-a".into());
            let arguments = r#"{"command":"pwd\nls -la","apiKey":"private-test-key"}"#;
            let key = (generation, "tool-a".to_string());
            for approved in [true, false] {
                let (sender, mut receiver) = tokio::sync::oneshot::channel();
                entity.register_tool_approval(generation, "tool-a".into(), sender);
                entity.update_tool_approval_preview(
                    generation,
                    "assistant-a",
                    "tool-a",
                    arguments,
                    "pending_user_approval",
                );
                let preview: serde_json::Value =
                    serde_json::from_str(entity.tool_approval_previews[&key].as_str()).unwrap();
                assert_eq!(
                    preview,
                    serde_json::json!({"command": "pwd\nls -la", "apiKey": "[REDACTED]"})
                );
                assert!(
                    entity
                        .chat_ui
                        .tool_call_expansion_state
                        .contains("assistant-a:tool-a")
                );
                let persisted = oxideterm_ai::sanitize_tool_arguments_text_for_persistence(
                    entity.tool_approval_previews[&key].as_str(),
                );
                assert!(!persisted.contains("pwd"));
                assert!(!persisted.contains("private-test-key"));
                entity.resolve_tool_approval(generation, "tool-a", approved);
                assert_eq!(receiver.try_recv(), Ok(approved));
                assert!(!entity.tool_approval_previews.contains_key(&key));
            }
            entity.update_tool_approval_preview(
                generation,
                "assistant-a",
                "tool-a",
                arguments,
                "pending_user_approval",
            );
            entity.update_tool_approval_preview(generation, "assistant-a", "tool-a", "{}", "error");
            assert!(!entity.tool_approval_previews.contains_key(&key));
            entity.update_tool_approval_preview(
                generation,
                "assistant-a",
                "tool-a",
                arguments,
                "pending_user_approval",
            );
            entity.cancel_chat_stream_for("conversation-a");
            assert!(!entity.tool_approval_previews.contains_key(&key));
            entity.update_tool_approval_preview(
                generation,
                "assistant-a",
                "tool-a",
                arguments,
                "pending_user_approval",
            );
            assert!(!entity.tool_approval_previews.contains_key(&key));
        });
    }

    #[gpui::test]
    fn tool_approval_generation_and_lifecycle_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let (generation, _) = entity.update(cx, |entity, _cx| {
            entity.begin_chat_stream("conversation-a".into(), "assistant-a".into())
        });
        let (first_sender, mut first_receiver) = tokio::sync::oneshot::channel();
        let (replacement_sender, mut replacement_receiver) = tokio::sync::oneshot::channel();
        entity.update(cx, |entity, _cx| {
            assert!(entity.register_tool_approval(generation, "tool-a".to_string(), first_sender,));
            assert!(entity.register_tool_approval(
                generation,
                "tool-a".to_string(),
                replacement_sender,
            ));
            assert!(entity.resolve_tool_approval(generation, "tool-a", true));
            assert!(!entity.resolve_tool_approval(generation, "tool-a", false));
        });
        assert_eq!(first_receiver.try_recv(), Ok(false));
        assert_eq!(replacement_receiver.try_recv(), Ok(true));

        let (stale_sender, mut stale_receiver) = tokio::sync::oneshot::channel();
        entity.update(cx, |entity, _cx| {
            entity.cancel_chat_stream_for("conversation-a");
            assert!(!entity.register_tool_approval(
                generation,
                "stale-tool".to_string(),
                stale_sender,
            ));
        });
        assert_eq!(stale_receiver.try_recv(), Ok(false));

        let (release_sender, mut release_receiver) = tokio::sync::oneshot::channel();
        let current_generation = entity.update(cx, |entity, _cx| {
            let (current_generation, _) =
                entity.begin_chat_stream("conversation-a".into(), "assistant-a".into());
            assert!(entity.register_tool_approval(
                current_generation,
                "release-tool".to_string(),
                release_sender,
            ));
            current_generation
        });
        assert!(current_generation > generation);

        drop(entity);
        cx.update(|_cx| {});
        cx.run_until_parked();

        assert_eq!(release_receiver.try_recv(), Ok(false));
    }

    #[gpui::test]
    fn history_description_pages_bound_loaded_rows_and_preserve_scroll_anchor(
        cx: &mut TestAppContext,
    ) {
        use oxideterm_ai::{AiChatMessage, AiChatState, ConversationStore, HistoryMutation};
        let directory = tempfile::tempdir().unwrap();
        let store = ConversationStore::open(directory.path().join("chat_history.v4.redb")).unwrap();
        let mut state = AiChatState::default();
        state.create_conversation("paged".into(), None, 1, None);
        store
            .apply(vec![HistoryMutation::Create {
                conversation: state.conversations.remove(0),
                revision: 1,
            }])
            .unwrap();
        let message = |index: usize| {
            serde_json::from_value::<AiChatMessage>(serde_json::json!({
                "id":format!("message-{index}"), "role":"user", "timestamp_ms":index,
                "content":format!("message {index}\n{}", "body text ".repeat(200)),
            }))
            .unwrap()
        };
        store
            .apply(
                (0..1000)
                    .map(|index| HistoryMutation::PutMessage {
                        conversation_id: "paged".into(),
                        branch_id: "main".into(),
                        message: message(index),
                        revision: index as u64 + 2,
                    })
                    .collect(),
            )
            .unwrap();
        let runtime = test_runtime();
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(runtime.clone(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        entity.update(cx, |ai, cx| {
            ai.initialize_history(directory.path().join("chat_history.redb"), cx)
        });
        let settle = |cx: &mut TestAppContext, first: &str, count: usize| {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                runtime.block_on(tokio::task::yield_now());
                cx.run_until_parked();
                if entity.read_with(cx, |ai, _| {
                    ai.history
                        .pages
                        .get("paged")
                        .is_some_and(|page| !page.loading)
                        && ai
                            .conversation_state
                            .active_conversation()
                            .is_some_and(|conversation| {
                                conversation.messages.len() == count
                                    && conversation
                                        .messages
                                        .first()
                                        .is_some_and(|message| message.id == first)
                            })
                }) {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "requested history page did not arrive"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        };
        settle(cx, "message-950", 50);
        assert_eq!(
            store.cached_bytes(),
            0,
            "description loading must not decode bodies"
        );
        entity.update(cx, |ai, _| {
            let conversation = ai.conversation_state.active_conversation().unwrap();
            assert_eq!(
                conversation
                    .messages
                    .iter()
                    .map(|message| message.id.clone())
                    .collect::<Vec<_>>(),
                (950..1000)
                    .map(|index| format!("message-{index}"))
                    .collect::<Vec<_>>()
            );
            assert!(
                conversation
                    .messages
                    .iter()
                    .all(|message| message.content.chars().count() <= 256)
            );
            ai.sync_chat_message_list(
                "paged",
                &(0..52).collect::<Vec<_>>(),
                ai_chat_virtual_list_spec(),
            );
            ai.chat_ui.message_list_state.scroll_to(gpui::ListOffset {
                item_ix: 1,
                offset_in_item: gpui::px(13.0),
            });
            ai.request_history_page("paged".into(), true);
        });
        settle(cx, "message-900", 100);
        entity.update(cx, |ai, _| {
            ai.sync_chat_message_list(
                "paged",
                &(0..102).collect::<Vec<_>>(),
                ai_chat_virtual_list_spec(),
            );
            ai.restore_history_scroll_anchor("paged");
            let top = ai.chat_ui.message_list_state.logical_scroll_top();
            assert_eq!((top.item_ix, top.offset_in_item), (51, gpui::px(13.0)));
            ai.chat_ui.message_list_state.scroll_to(gpui::ListOffset {
                item_ix: 1,
                offset_in_item: gpui::px(7.0),
            });
            ai.request_history_page("paged".into(), true);
        });
        settle(cx, "message-850", 100);
        entity.update(cx, |ai, _| {
            let page = &ai.history.pages["paged"];
            assert_eq!(page.descriptions.len(), 100);
            assert_eq!(page.after.as_ref().unwrap().before_sequence, 949);
            ai.request_history_page("paged".into(), false);
        });
        settle(cx, "message-900", 100);
        entity.update(cx, |ai, _| {
            assert_eq!(
                ai.conversation_state
                    .active_conversation()
                    .unwrap()
                    .messages
                    .last()
                    .unwrap()
                    .id,
                "message-999"
            );
            assert!(ai.history.pages["paged"].after.is_none());
            ai.request_history_body("paged".into(), "message-999".into(), None, false);
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            runtime.block_on(tokio::task::yield_now());
            cx.run_until_parked();
            if let Some(view) = entity.read_with(cx, |ai, _| {
                ai.history.pages["paged"]
                    .bodies
                    .get("message-999")
                    .and_then(std::sync::Weak::upgrade)
            }) {
                assert_eq!(
                    view.message.content,
                    format!("message 999\n{}", "body text ".repeat(200))
                );
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "visible message body did not arrive"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let cached_main = entity.read_with(cx, |ai, _| {
            ai.history.pages["paged"].bodies["message-999"]
                .upgrade()
                .unwrap()
        });
        let archive_owner = history::HistoryViewOwner::Archive("paged".into(), "archive".into());
        entity.update(cx, |ai, _| {
            let description = ai.history.pages["paged"].descriptions["message-999"].clone();
            ai.history
                .auxiliary_pages
                .entry(archive_owner.clone())
                .or_default()
                .descriptions
                .insert(description.id.clone(), description);
            ai.request_history_owned_body(archive_owner.clone(), "message-999".into(), None, false);
            // A main-list refresh must not invalidate an independent visible archive query.
            ai.request_history_conversation("paged".into());
        });
        settle(cx, "message-950", 50);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            runtime.block_on(tokio::task::yield_now());
            cx.run_until_parked();
            if let Some(view) = entity.read_with(cx, |ai, _| {
                ai.history_view(&archive_owner)
                    .and_then(|page| page.bodies.get("message-999"))
                    .and_then(std::sync::Weak::upgrade)
            }) {
                assert_eq!(
                    view.message.content,
                    format!("message 999\n{}", "body text ".repeat(200))
                );
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "main page refresh stranded the archive body query"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        settle(cx, "message-950", 50);
        entity.read_with(cx, |ai, _| {
            let refreshed = ai.history.pages["paged"].bodies["message-999"]
                .upgrade()
                .unwrap();
            assert!(
                Arc::ptr_eq(&cached_main, &refreshed),
                "unchanged history refresh must retain the decoded row"
            );
        });
        struct HistoryPageRow {
            ai: Entity<AiWorkspaceEntity>,
            state: gpui::ListState,
        }
        impl gpui::Render for HistoryPageRow {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                let ai = self.ai.clone();
                let list = gpui::list(self.state.clone(), move |_, _, cx| {
                    ai.update(cx, |ai, cx| {
                        ai.request_visible_history_page("paged".into(), true, cx)
                    });
                    div().h(gpui::px(20.0)).w_full().into_any_element()
                })
                .size_full();
                div()
                    .relative()
                    .size_full()
                    .child(list)
                    .child(oxideterm_gpui_ui::scroll::Scrollbar::for_list(&self.state))
            }
        }
        let list_state = entity.read_with(cx, |ai, _| ai.chat_ui.message_list_state.clone());
        list_state.reset(1);
        for mode in [gpui::FollowMode::Tail, gpui::FollowMode::Normal] {
            list_state.set_follow_mode(mode);
            let view = cx.new(|_| HistoryPageRow {
                ai: entity.clone(),
                state: list_state.clone(),
            });
            cx.add_empty_window().draw(
                gpui::point(gpui::px(0.0), gpui::px(0.0)),
                gpui::size(gpui::px(200.0), gpui::px(100.0)),
                move |_, _| view.into_any_element(),
            );
            if matches!(mode, gpui::FollowMode::Tail) {
                cx.run_until_parked();
                assert_eq!(
                    entity.read_with(cx, |ai, _| ai
                        .conversation_state
                        .active_conversation()
                        .unwrap()
                        .messages
                        .first()
                        .unwrap()
                        .id
                        .clone()),
                    "message-950"
                );
            } else {
                settle(cx, "message-900", 100);
            }
        }
        assert!(store.cached_bytes() <= oxideterm_ai::HISTORY_CACHE_BYTES);
        let owner = history::HistoryViewOwner::Main("paged".into());
        let held = entity.update(cx, |ai, cx| {
            let mut body = message(999);
            body.content = "visible payload".into();
            let body = Arc::new(oxideterm_ai::live_message_view(&body, "paged", 1, None).unwrap());
            let weak = Arc::downgrade(&body);
            ai.history
                .pages
                .get_mut("paged")
                .unwrap()
                .bodies
                .insert("message-999".into(), weak.clone());
            ai.retain_visible_history_body(owner.clone(), "message-999".into(), cx);
            weak
        });
        cx.run_until_parked();
        assert_eq!(held.upgrade().unwrap().message.content, "visible payload");
        entity.update(cx, |ai, cx| {
            ai.retain_visible_history_body(owner, "message-998".into(), cx);
        });
        cx.run_until_parked();
        assert!(
            held.upgrade().is_none(),
            "scrolling away must release uncached body ownership"
        );
    }

    #[gpui::test]
    fn cursor_answers_require_submission_and_stopping_cancels_only_its_run(
        cx: &mut TestAppContext,
    ) {
        let runtime = test_runtime();
        let entity = cx
            .new(|cx| AiWorkspaceEntity::new(runtime, oxideterm_ai::AiProviderKeyStore::new(), cx));
        let (first_tx, mut first_rx) = tokio::sync::oneshot::channel();
        let (second_tx, mut second_rx) = tokio::sync::oneshot::channel();
        entity.update(cx, |ai, _| {
            ai.create_conversation("first".into(), None, 1, None);
            ai.create_conversation("second".into(), None, 2, None);
            let first = ai.begin_chat_stream("first".into(), "reply-first".into()).0;
            let second = ai.begin_chat_stream("second".into(), "reply-second".into()).0;
            for (generation, conversation, sender) in [(first, "first", first_tx), (second, "second", second_tx)] {
                let request = serde_json::from_value(serde_json::json!({
                    "toolCallId":"same-call", "questions":[
                        {"id":"mode","prompt":"Which mode?","options":[{"id":"a","label":"Agent"},{"id":"p","label":"Plan"}]},
                        {"id":"features","prompt":"Which features?","options":[{"id":"x","label":"One"},{"id":"y","label":"Two"}],"allowMultiple":true}
                    ]
                })).unwrap();
                ai.pending_cursor_requests.insert((generation, "same-call".into()), AiPendingCursorRequest {
                    conversation_id: conversation.into(), assistant_id: format!("reply-{conversation}"),
                    request: oxideterm_ai::CursorRequest::Questions(request), selections: vec![vec![],vec![]], response_tx: Some(sender),
                });
            }
            ai.cancel_chat_stream_for("first");
            assert!(!ai.resolve_cursor_request(first, "same-call", true));
            assert!(!ai.resolve_cursor_request(second, "same-call", true));
            ai.select_cursor_option(second, "same-call", 0, 0);
            ai.select_cursor_option(second, "same-call", 0, 1);
            ai.select_cursor_option(second, "same-call", 1, 0);
            ai.select_cursor_option(second, "same-call", 1, 1);
            assert_eq!(ai.pending_cursor_requests[&(second, "same-call".into())].selections, [vec!["p"],vec!["x","y"]]);
            assert!(matches!(second_rx.try_recv(), Err(tokio::sync::oneshot::error::TryRecvError::Empty)));
            assert!(ai.resolve_cursor_request(second, "same-call", true));
            assert!(!ai.resolve_cursor_request(second, "same-call", true));
        });
        assert_eq!(
            first_rx.try_recv().unwrap().unwrap(),
            serde_json::json!({"outcome":{"outcome":"cancelled"}})
        );
        assert_eq!(
            second_rx.try_recv().unwrap().unwrap(),
            serde_json::json!({"outcome":{"outcome":"answered","answers":[
                {"questionId":"mode","selectedOptionIds":["p"]}, {"questionId":"features","selectedOptionIds":["x","y"]}
            ]}})
        );
    }

    #[gpui::test]
    fn user_question_answers_are_scoped_to_the_live_conversation_run(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let store =
            oxideterm_ai::ConversationStore::open(directory.path().join("history.redb")).unwrap();
        let writer = oxideterm_ai::HistoryWriter::new(store.clone()).unwrap();
        let runtime = test_runtime();
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(runtime.clone(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let (first_tx, mut first_rx) = tokio::sync::oneshot::channel();
        let (second_tx, mut second_rx) = tokio::sync::oneshot::channel();
        entity.update(cx, |ai, _| {
            ai.history.store = Some(store.clone());
            ai.history.writer = Some(writer);
            ai.history.status = HistoryStatus::Ready;
            ai.create_conversation("first".into(), None, 1, None);
            ai.create_conversation("second".into(), None, 2, None);
            let first = ai.begin_chat_stream("first".into(), "reply-first".into()).0;
            let second = ai
                .begin_chat_stream("second".into(), "reply-second".into())
                .0;
            ai.pending_user_questions.insert(
                (first, "question".into()),
                AiPendingUserQuestion {
                    conversation_id: "first".into(),
                    dispatch: None,
                    sender: first_tx,
                },
            );
            ai.pending_user_questions.insert(
                (second, "question".into()),
                AiPendingUserQuestion {
                    conversation_id: "second".into(),
                    dispatch: None,
                    sender: second_tx,
                },
            );
            ai.cancel_chat_stream_for("first");
            assert!(!ai.resolve_user_question(
                first,
                "question",
                zeroize::Zeroizing::new("stale answer".into())
            ));
            assert!(!ai.resolve_user_question(
                second,
                "question",
                zeroize::Zeroizing::new("  ".into())
            ));
            assert!(ai.resolve_user_question(
                second,
                "question",
                zeroize::Zeroizing::new("Staging".into())
            ));
            assert!(!ai.resolve_user_question(
                second,
                "question",
                zeroize::Zeroizing::new("duplicate".into())
            ));
        });
        assert!(matches!(
            first_rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Closed)
        ));
        assert!(
            matches!(
                second_rx.try_recv(),
                Err(tokio::sync::oneshot::error::TryRecvError::Empty)
            ),
            "the answer waits for the UI's commit acknowledgement"
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let answer = loop {
            runtime.block_on(tokio::task::yield_now());
            cx.run_until_parked();
            match second_rx.try_recv() {
                Ok(answer) => break answer,
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "answer must resume after the write commits"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("answer was lost: {error}"),
            }
        };
        assert_eq!(answer.as_str(), "Staging");
        let events = store.events("second", "user_answer", None, 10).unwrap();
        assert_eq!(
            events
                .iter()
                .map(|(_, event)| (event["toolCallId"].as_str(), event["answer"].as_str()))
                .collect::<Vec<_>>(),
            vec![(Some("question"), Some("Staging"))]
        );
        assert!(
            store
                .events("first", "user_answer", None, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[gpui::test]
    fn independent_conversations_route_reused_tool_ids_and_cancel_only_their_owner(
        cx: &mut TestAppContext,
    ) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let (first, second) = entity.update(cx, |ai, _cx| {
            let first = ai.begin_chat_stream("first".into(), "reply-first".into()).0;
            let second = ai
                .begin_chat_stream("second".into(), "reply-second".into())
                .0;
            (first, second)
        });
        let (first_tx, mut first_rx) = tokio::sync::oneshot::channel();
        let (second_tx, mut second_rx) = tokio::sync::oneshot::channel();
        entity.update(cx, |ai, _cx| {
            assert!(ai.register_tool_approval(first, "same-id".into(), first_tx));
            assert!(ai.register_tool_approval(second, "same-id".into(), second_tx));
            assert!(!ai.chat_stream_matches(first, "second", "reply-second"));
            ai.cancel_chat_stream_for("first");
            assert!(ai.chat_stream_matches(second, "second", "reply-second"));
            assert!(!ai.resolve_tool_approval(first, "same-id", true));
            assert!(ai.resolve_tool_approval(second, "same-id", true));
        });
        assert_eq!(first_rx.try_recv().unwrap(), false);
        assert_eq!(second_rx.try_recv().unwrap(), true);
    }

    #[gpui::test]
    fn tool_candidate_selection_is_keyboard_ordered_and_cancelled_with_stream(
        cx: &mut TestAppContext,
    ) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let (generation, _) = entity.update(cx, |entity, _cx| {
            entity
                .conversation_state
                .create_conversation("conversation-a".into(), None, 0, None);
            entity.begin_chat_stream("conversation-a".into(), "assistant-a".into())
        });
        let (sender, mut receiver) = tokio::sync::oneshot::channel();
        entity.update(cx, |entity, _cx| {
            entity.focus_chat_input();
            assert!(entity.register_tool_candidate_selection(
                generation,
                "tool-choice".to_string(),
                3,
                sender,
            ));
            assert!(entity.move_tool_candidate_selection(1));
            assert!(entity.move_tool_candidate_selection(1));
            assert!(entity.move_tool_candidate_selection(1));
            assert_eq!(
                entity
                    .chat_ui()
                    .tool_candidate_selection
                    .as_ref()
                    .map(|selection| selection.selected_index),
                Some(2),
            );
            assert!(!entity.chat_ui().input_focused);
            assert!(entity.resolve_tool_candidate_selection(generation, "tool-choice", Some(2)));
            assert!(entity.chat_ui().input_focused);
        });
        assert_eq!(receiver.try_recv(), Ok(Some(2)));

        let (cancel_sender, mut cancel_receiver) = tokio::sync::oneshot::channel();
        entity.update(cx, |entity, _cx| {
            entity.focus_chat_input();
            assert!(entity.register_tool_candidate_selection(
                generation,
                "tool-cancel".to_string(),
                2,
                cancel_sender,
            ));
            entity.cancel_chat_stream_for("conversation-a");
            assert!(entity.chat_ui().input_focused);
        });
        assert_eq!(cancel_receiver.try_recv(), Ok(None));
    }

    #[gpui::test]
    fn entity_release_stops_all_entity_delivery_waiters(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let (
            model_refresh_wake,
            provider_key_status_wake,
            selector_probe_wake,
            acp_agent_probe_wake,
            acp_model_discovery_wake,
            terminal_inline_wake,
            chat_stream_wake,
            compaction_wake,
        ) = cx.read(|cx| {
            let entity = entity.read(cx);
            (
                entity.model_refresh_tx.wake(),
                entity.provider_key_status_tx.wake(),
                entity.selector_probe_tx.wake(),
                entity.acp_agent_probe_tx.wake(),
                entity.acp_model_discovery_tx.wake(),
                entity.terminal_inline_tx.wake(),
                entity.chat_stream_tx.wake(),
                entity.compaction_tx.wake(),
            )
        });

        drop(entity);
        cx.update(|_cx| {});
        cx.run_until_parked();

        // The workspace runtime owns in-flight HTTP work independently.
        assert!(model_refresh_wake.is_stopped());
        assert!(provider_key_status_wake.is_stopped());
        assert!(selector_probe_wake.is_stopped());
        assert!(acp_agent_probe_wake.is_stopped());
        assert!(acp_model_discovery_wake.is_stopped());
        assert!(terminal_inline_wake.is_stopped());
        assert!(chat_stream_wake.is_stopped());
        assert!(compaction_wake.is_stopped());
    }
}

#[cfg(test)]
mod refresh_failure_message_tests {
    use super::*;

    #[test]
    fn keeps_the_cause_chain_on_one_bounded_line() {
        let error = anyhow::Error::new(std::io::Error::other("certificate verify failed"))
            .context("model refresh request failed");
        let message = model_refresh_failure_message(&error);
        assert!(
            message.contains("model refresh request failed"),
            "{message}"
        );
        assert!(message.contains("certificate verify failed"), "{message}");
        assert!(
            !message.contains('\n') && !message.contains('\r'),
            "{message}"
        );

        let truncated = model_refresh_failure_message(&anyhow::Error::new(std::io::Error::other(
            "x".repeat(500),
        )));
        assert_eq!(truncated.chars().count(), 400);
    }

    #[test]
    fn flattens_newlines_inside_the_cause() {
        let error = anyhow::Error::new(std::io::Error::other("line one\nline two\r\nline three"));
        let message = model_refresh_failure_message(&error);
        assert!(
            !message.contains('\n') && !message.contains('\r'),
            "{message}"
        );
        assert_eq!(message.matches("line").count(), 3, "{message}");
    }
}
