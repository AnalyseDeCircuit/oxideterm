use super::*;
use crate::workspace::root::init::ai_chat_initialization_error;
use gpui::Task;
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_settings_model::{
    ai_mcp_auth_mode_value, ai_mcp_draft_valid, ai_mcp_draft_valid_for_names,
    ai_mcp_transport_value,
};

pub(in crate::workspace) mod knowledge;

pub(in crate::workspace) enum AiChatInitializationOutcome {
    AlreadyInitialized,
    Loaded,
    Failed,
}

pub(in crate::workspace) enum AiWorkspaceEvent {
    AcpAgentProbeDeliveryReady,
    AcpModelDiscoveryDeliveryReady,
    ChatStreamDeliveryReady,
    CompactionDeliveryReady,
    CompactionStateChanged,
    CredentialOperationReady,
    KnowledgePageChanged,
    KnowledgeReindexDeliveryReady,
    McpRuntimeChanged,
    ModelRefreshDeliveryReady,
    ProviderKeyStatusChanged,
    SelectorProviderStatusChanged,
    SettingsConfirmChanged,
    TerminalInlineDeliveryReady,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum AiChatPopover {
    ConversationList,
    Menu,
    Reasoning,
    Safety,
    Context,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum AiChatConfirmKind {
    ClearAll,
    DeleteMessage { message_id: Arc<str> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct AiChatConfirmSnapshot {
    pub(in crate::workspace) kind: AiChatConfirmKind,
    pub(in crate::workspace) phase: oxideterm_gpui_ui::motion::ExitPhase,
    pub(in crate::workspace) focused_action: Option<ConfirmDialogAction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum AiChatConfirmEffect {
    ClearAll,
    DeleteMessage { message_id: Arc<str> },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum AiChatConfirmKeyAction {
    Cancel,
    Confirm,
    Handled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum AiChatConfirmOwnerKind {
    ClearAll,
    DeleteMessage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct AiChatConfirmOwnerSnapshot {
    pub(in crate::workspace) kind: AiChatConfirmOwnerKind,
    pub(in crate::workspace) phase: oxideterm_gpui_ui::motion::ExitPhase,
}

/// Describes which workspace-owned AI surfaces can currently consume UI probes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::workspace) struct AiWorkspaceVisibility {
    pub(in crate::workspace) model_selector_surface: bool,
    pub(in crate::workspace) settings_surface: bool,
}

impl AiWorkspaceVisibility {
    fn provider_status_visible(self) -> bool {
        self.model_selector_surface || self.settings_surface
    }
}

pub(in crate::workspace) struct AiAcpAgentProbeIntent {
    pub(in crate::workspace) agent_id: String,
    pub(in crate::workspace) runtime_state: oxideterm_settings::AcpAgentRuntimeState,
    pub(in crate::workspace) auth_status: oxideterm_settings::AcpAgentAuthStatus,
    pub(in crate::workspace) last_error_kind: Option<String>,
}

pub(in crate::workspace) struct AiAcpModelDiscoveryIntent {
    pub(in crate::workspace) conversation_id: String,
    agent_id: String,
    config_options: Option<Vec<oxideterm_ai::AcpSessionConfigOption>>,
}

pub(in crate::workspace) enum AiKnowledgeReindexIntent {
    Finished { failed: bool },
}

pub(in crate::workspace) enum AiModelRefreshIntent {
    Updated {
        index: usize,
        provider_id: String,
        refresh: oxideterm_ai::ProviderModelRefresh,
    },
    MissingApiKey {
        provider_id: String,
    },
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum AiCredentialFailure {
    SaveProviderKey,
    RemoveProviderKey,
    SaveMcpToken,
}

pub(in crate::workspace) enum AiCredentialIntent {
    ProviderKeyStored { index: usize, provider_id: String },
    ProviderKeyRemoved,
    McpServerReady { config: serde_json::Value },
    McpServerRemoved { server_id: String },
    Failed(AiCredentialFailure),
}

enum AiSettingsConfirm {
    Enable,
    RemoveProviderKey {
        index: usize,
        provider_id: String,
    },
    RemoveProvider {
        provider_id: String,
        provider_name: String,
    },
}

pub(in crate::workspace) enum AiSettingsConfirmIntent {
    Enable,
    RemoveProviderKey { index: usize, provider_id: String },
    RemoveProvider { provider_id: String },
}

#[derive(Clone, Copy)]
enum AiProviderKeyOperation {
    Store { index: usize },
    Remove,
}

enum AiModelRefreshFailure {
    MissingApiKey,
    Failed,
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
    agent_id: String,
    result: AiAcpAgentProbeResult,
}

struct AiAcpAgentProbeResult {
    runtime_state: oxideterm_settings::AcpAgentRuntimeState,
    auth_status: oxideterm_settings::AcpAgentAuthStatus,
    last_error_kind: Option<String>,
}

struct AiAcpModelDiscoveryDelivery {
    conversation_id: String,
    agent_id: String,
    config_options: Option<Vec<oxideterm_ai::AcpSessionConfigOption>>,
}

enum AiKnowledgeReindexDelivery {
    Progress { current: usize, total: usize },
    Finished { failed: bool },
}

enum AiTerminalInlineDelivery {
    KeyStatus { generation: u64, has_key: bool },
    Content { generation: u64, chunk: String },
    Done { generation: u64 },
    Error { generation: u64, message: String },
}

const AI_TERMINAL_INLINE_DELIVERY_BUDGET: crate::workspace::delivery::DeliveryBudget =
    crate::workspace::delivery::DeliveryBudget::new(128, Duration::from_millis(4));
const AI_CHAT_STREAM_DELIVERY_BUDGET: crate::workspace::delivery::DeliveryBudget =
    crate::workspace::delivery::DeliveryBudget::new(256, Duration::from_millis(4));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum AiSettingsViewSection {
    ProviderSettings,
    ToolUse,
    ContextWindows,
}

/// Owns AI-settings-only presentation state and its state transitions.
struct AiSettingsViewState {
    new_provider_type: String,
    provider_settings_expanded: bool,
    tool_use_expanded: bool,
    context_windows_expanded: bool,
    expanded_providers: std::collections::BTreeMap<String, bool>,
    expanded_provider_models: std::collections::BTreeSet<String>,
    expanded_context_providers: std::collections::BTreeSet<String>,
}

impl Default for AiSettingsViewState {
    fn default() -> Self {
        // The provider catalog defines the fallback used by provider creation.
        let new_provider_type = oxideterm_ai::AI_PROVIDER_TEMPLATES[0]
            .provider_type
            .to_owned();
        Self {
            new_provider_type,
            provider_settings_expanded: true,
            tool_use_expanded: true,
            context_windows_expanded: true,
            expanded_providers: std::collections::BTreeMap::new(),
            expanded_provider_models: std::collections::BTreeSet::new(),
            expanded_context_providers: std::collections::BTreeSet::new(),
        }
    }
}

/// Owns AI worker delivery slices as they move out of the workspace root.
pub(in crate::workspace) struct AiWorkspaceEntity {
    task_runtime: Arc<tokio::runtime::Runtime>,
    key_store: oxideterm_ai::AiProviderKeyStore,
    model_ui: AiModelWorkspaceState,
    chat_ui: AiChatWorkspaceState,
    knowledge_window_activation_subscription: Option<Subscription>,
    visibility: AiWorkspaceVisibility,
    settings_view: AiSettingsViewState,
    settings_secret_drafts: HashMap<SettingsInput, zeroize::Zeroizing<String>>,
    focused_settings_input: Option<SettingsInput>,
    provider_key_operation_tasks: HashMap<String, Task<()>>,
    pending_provider_key_removals: HashSet<String>,
    credential_intents: VecDeque<AiCredentialIntent>,
    mcp_add_dialog: Option<AiMcpServerDraft>,
    mcp_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    mcp_dialog_exit_task: Option<Task<()>>,
    mcp_save_task: Option<Task<()>>,
    mcp_runtime_tasks: HashMap<String, Task<()>>,
    mcp_status_tick_task: Option<Task<()>>,
    settings_confirm: Option<AiSettingsConfirm>,
    settings_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    settings_confirm_exit_task: Option<Task<()>>,
    settings_confirm_intents: VecDeque<AiSettingsConfirmIntent>,
    chat_confirm: Option<AiChatConfirmKind>,
    chat_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    chat_confirm_focused_action: Option<ConfirmDialogAction>,
    chat_confirm_exit_task: Option<Task<()>>,
    model_refresh_generations: HashMap<String, u64>,
    refreshing_models: HashSet<String>,
    model_refresh_tx:
        crate::workspace::delivery::ActiveDeliverySender<AiModelRefreshWorkerDelivery>,
    model_refresh_rx: std::sync::mpsc::Receiver<AiModelRefreshWorkerDelivery>,
    model_refresh_pending: usize,
    next_model_refresh_generation: u64,
    model_refresh_intents: VecDeque<AiModelRefreshIntent>,
    provider_key_status: HashMap<String, bool>,
    provider_key_status_pending: HashSet<String>,
    provider_key_status_tx:
        crate::workspace::delivery::ActiveDeliverySender<AiProviderKeyStatusDelivery>,
    provider_key_status_rx: std::sync::mpsc::Receiver<AiProviderKeyStatusDelivery>,
    selector_provider_online: HashMap<String, bool>,
    selector_probe_generations: HashMap<String, u64>,
    selector_probe_tx:
        crate::workspace::delivery::ActiveDeliverySender<AiModelSelectorProbeDelivery>,
    selector_probe_rx: std::sync::mpsc::Receiver<AiModelSelectorProbeDelivery>,
    selector_probe_pending: usize,
    next_selector_probe_generation: u64,
    acp_agent_probe_pending: HashSet<String>,
    acp_agent_probe_tx: crate::workspace::delivery::ActiveDeliverySender<AiAcpAgentProbeDelivery>,
    acp_agent_probe_rx: std::sync::mpsc::Receiver<AiAcpAgentProbeDelivery>,
    acp_agent_probe_intents: VecDeque<AiAcpAgentProbeIntent>,
    acp_model_options: HashMap<(String, String), Vec<oxideterm_ai::AcpSessionConfigOption>>,
    acp_model_discovery_pending: HashSet<(String, String)>,
    acp_model_discovery_tx:
        crate::workspace::delivery::ActiveDeliverySender<AiAcpModelDiscoveryDelivery>,
    acp_model_discovery_rx: std::sync::mpsc::Receiver<AiAcpModelDiscoveryDelivery>,
    acp_model_discovery_intents: VecDeque<AiAcpModelDiscoveryIntent>,
    rag_store: LazyAiRagStore,
    knowledge_page: knowledge::KnowledgePageState,
    knowledge_import_task: Option<Task<()>>,
    knowledge_embedding_task: Option<Task<()>>,
    knowledge_reindex_progress: Option<(usize, usize)>,
    knowledge_reindex_cancel: Option<Arc<AtomicBool>>,
    knowledge_reindex_tx:
        crate::workspace::delivery::ActiveDeliverySender<AiKnowledgeReindexDelivery>,
    knowledge_reindex_rx: std::sync::mpsc::Receiver<AiKnowledgeReindexDelivery>,
    knowledge_reindex_intents: VecDeque<AiKnowledgeReindexIntent>,
    terminal_inline_panel: AiInlinePanelState,
    terminal_inline_tx: crate::workspace::delivery::ActiveDeliverySender<AiTerminalInlineDelivery>,
    terminal_inline_rx: std::sync::mpsc::Receiver<AiTerminalInlineDelivery>,
    terminal_inline_stream_task: Option<tokio::task::JoinHandle<()>>,
    chat_stream_generation: u64,
    chat_stream_task: Option<tokio::task::JoinHandle<()>>,
    chat_stream_tx: AiStreamDeliverySender,
    chat_stream_rx: std::sync::mpsc::Receiver<AiStreamDelivery>,
    chat_stream_deliveries: VecDeque<AiStreamDelivery>,
    conversation_state: oxideterm_ai::AiChatState,
    persistence_store: Option<oxideterm_ai::AiChatPersistenceStore>,
    chat_initialized: bool,
    chat_initialization_error: Option<AiChatInitializationError>,
    safety_modes_by_conversation: HashMap<String, oxideterm_gpui_ui::ai::AiSafetyMode>,
    chat_loading: bool,
    next_chat_sequence: u64,
    pending_tool_approvals: HashMap<String, tokio::sync::oneshot::Sender<bool>>,
    pending_acp_permission_choices: HashMap<String, tokio::sync::oneshot::Sender<Option<String>>>,
    pending_tool_candidate_selections: HashMap<String, tokio::sync::oneshot::Sender<Option<usize>>>,
    // Runtime evidence transitions are implemented beside stream application,
    // but these collections remain physically owned by this Entity.
    pub(in crate::workspace) tool_execution_records: VecDeque<AiToolExecutionRecord>,
    pub(in crate::workspace) tool_result_facts: VecDeque<AiToolResultFact>,
    agent_fs: NodeAgentIdeFileSystem,
    mcp_registry: oxideterm_ai::McpRegistry,
    compaction_tx: AiCompactionDeliverySender,
    compaction_rx: std::sync::mpsc::Receiver<AiCompactionDelivery>,
    compaction_deliveries: VecDeque<AiCompactionDelivery>,
    compacting_conversations: HashSet<String>,
    compaction_notice: Option<AiCompactionNotice>,
}

impl AiWorkspaceEntity {
    /// Returns the protected provider key store without copying key material.
    pub(in crate::workspace) fn key_store(&self) -> &oxideterm_ai::AiProviderKeyStore {
        &self.key_store
    }

    /// Returns provider and model presentation state from its sole owner.
    pub(in crate::workspace) fn model_ui(&self) -> &AiModelWorkspaceState {
        &self.model_ui
    }

    /// Returns chat presentation state from its sole owner.
    pub(in crate::workspace) fn chat_ui(&self) -> &AiChatWorkspaceState {
        &self.chat_ui
    }

    pub(in crate::workspace) fn sidebar_keyboard_target_focused(&self) -> bool {
        // Keep every editable AI surface in the same root key-routing contract.
        self.chat_ui.input_focused
            || self.chat_ui.footer_focus.is_some()
            || (self.chat_ui.renaming_conversation_id.is_some()
                && self.chat_ui.renaming_conversation_focused)
            || (self.chat_ui.editing_message_id.is_some() && self.chat_ui.editing_message_focused)
            || self.model_ui.selector_search_focused
    }

    pub(in crate::workspace) fn configure_chat_surface(
        &mut self,
        sidebar_width: f32,
        overlay_window_size: Option<(f32, f32)>,
    ) {
        self.chat_ui.sidebar_width = sidebar_width;
        self.chat_ui.overlay_window_size = overlay_window_size;
    }

    pub(in crate::workspace) fn set_chat_draft(&mut self, draft: String) {
        self.chat_ui.draft = draft;
    }

    pub(in crate::workspace) fn focus_chat_input(&mut self) {
        self.chat_ui.input_focused = true;
        self.chat_ui.footer_focus = None;
    }

    pub(in crate::workspace) fn blur_chat_input(&mut self, suppress_autocomplete: bool) {
        self.chat_ui.input_focused = false;
        self.chat_ui.footer_focus = None;
        if suppress_autocomplete {
            self.chat_ui.autocomplete_suppressed = true;
        }
    }

    pub(in crate::workspace) fn set_chat_footer_focus(
        &mut self,
        focus: Option<AiChatFooterAction>,
    ) {
        self.chat_ui.input_focused = focus.is_none();
        self.chat_ui.footer_focus = focus;
    }

    pub(in crate::workspace) fn clear_chat_footer_focus(&mut self) {
        self.chat_ui.footer_focus = None;
    }

    pub(in crate::workspace) fn set_chat_popover_open(
        &mut self,
        popover: AiChatPopover,
        open: bool,
    ) {
        match popover {
            AiChatPopover::ConversationList => self.chat_ui.conversation_list_open = open,
            AiChatPopover::Menu => self.chat_ui.menu_open = open,
            AiChatPopover::Reasoning => self.chat_ui.reasoning_menu_open = open,
            AiChatPopover::Safety => self.chat_ui.safety_menu_open = open,
            AiChatPopover::Context => self.chat_ui.context_popover_open = open,
        }
    }

    pub(in crate::workspace) fn close_chat_popovers(&mut self) {
        self.chat_ui.conversation_list_open = false;
        self.chat_ui.menu_open = false;
        self.chat_ui.reasoning_menu_open = false;
        self.chat_ui.safety_menu_open = false;
        self.chat_ui.context_popover_open = false;
        self.clear_conversation_rename();
    }

    pub(in crate::workspace) fn toggle_chat_context(&mut self) {
        self.chat_ui.include_context = !self.chat_ui.include_context;
        if !self.chat_ui.include_context {
            self.chat_ui.include_all_panes = false;
        }
    }

    pub(in crate::workspace) fn toggle_chat_all_panes(&mut self) {
        self.chat_ui.include_all_panes = !self.chat_ui.include_all_panes;
    }

    pub(in crate::workspace) fn replace_chat_input(
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

    pub(in crate::workspace) fn replace_message_edit(
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

    pub(in crate::workspace) fn replace_conversation_rename(
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

    pub(in crate::workspace) fn apply_chat_autocomplete(
        &mut self,
        candidate: &oxideterm_ai::AiAutocompleteCandidate,
    ) {
        self.chat_ui.draft = oxideterm_ai::apply_ai_autocomplete_candidate(
            &self.chat_ui.draft,
            self.chat_ui.draft.len(),
            candidate,
        );
        self.chat_ui.autocomplete_index = 0;
        self.chat_ui.autocomplete_suppressed = true;
    }

    pub(in crate::workspace) fn move_chat_autocomplete(&mut self, delta: isize, item_count: usize) {
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

    pub(in crate::workspace) fn suppress_chat_autocomplete(&mut self) {
        self.chat_ui.autocomplete_suppressed = true;
    }

    pub(in crate::workspace) fn pop_chat_draft(&mut self) -> bool {
        let changed = self.chat_ui.draft.pop().is_some()
            || self.chat_ui.autocomplete_suppressed
            || self.chat_ui.autocomplete_index != 0;
        self.chat_ui.autocomplete_suppressed = false;
        self.chat_ui.autocomplete_index = 0;
        changed
    }

    pub(in crate::workspace) fn push_chat_draft_newline(&mut self) {
        self.chat_ui.draft.push('\n');
    }

    pub(in crate::workspace) fn pop_conversation_rename(&mut self) -> bool {
        self.chat_ui.renaming_conversation_draft.pop().is_some()
    }

    pub(in crate::workspace) fn focus_conversation_rename(&mut self) {
        self.chat_ui.renaming_conversation_focused = true;
        self.chat_ui.input_focused = false;
        self.chat_ui.footer_focus = None;
        self.chat_ui.editing_message_focused = false;
    }

    pub(in crate::workspace) fn begin_conversation_rename(
        &mut self,
        conversation_id: String,
        title: String,
    ) {
        self.chat_ui.renaming_conversation_id = Some(conversation_id);
        self.chat_ui.renaming_conversation_draft = title;
        self.focus_conversation_rename();
    }

    pub(in crate::workspace) fn clear_conversation_rename(&mut self) {
        self.chat_ui.renaming_conversation_id = None;
        self.chat_ui.renaming_conversation_draft.clear();
        self.chat_ui.renaming_conversation_focused = false;
    }

    pub(in crate::workspace) fn pop_message_edit(&mut self) -> bool {
        self.chat_ui.editing_message_draft.pop().is_some()
    }

    pub(in crate::workspace) fn push_message_edit_newline(&mut self) {
        self.chat_ui.editing_message_draft.push('\n');
    }

    pub(in crate::workspace) fn blur_message_edit(&mut self) {
        self.chat_ui.editing_message_focused = false;
    }

    pub(in crate::workspace) fn focus_message_edit(&mut self) {
        self.chat_ui.editing_message_focused = true;
        self.chat_ui.input_focused = false;
    }

    pub(in crate::workspace) fn begin_message_edit(&mut self, message_id: String, content: String) {
        self.chat_ui.editing_message_id = Some(message_id);
        self.chat_ui.editing_message_draft = content;
        self.focus_message_edit();
    }

    pub(in crate::workspace) fn clear_message_edit(&mut self) {
        self.chat_ui.editing_message_id = None;
        self.chat_ui.editing_message_draft.clear();
        self.chat_ui.editing_message_focused = false;
    }

    pub(in crate::workspace) fn reset_chat_for_conversation_selection(&mut self) {
        self.chat_ui.conversation_list_open = false;
        self.chat_ui.menu_open = false;
        self.chat_ui.safety_menu_open = false;
        self.clear_conversation_rename();
        self.clear_message_edit();
        self.clear_chat_expansions();
        self.blur_chat_input(false);
    }

    pub(in crate::workspace) fn reset_chat_after_conversation_delete(
        &mut self,
        has_conversations: bool,
    ) {
        self.clear_chat_expansions();
        self.clear_conversation_rename();
        self.chat_ui.conversation_list_open = has_conversations;
        self.chat_ui.menu_open = false;
    }

    pub(in crate::workspace) fn reset_chat_for_new_conversation(&mut self) {
        self.chat_ui.conversation_list_open = false;
        self.chat_ui.menu_open = false;
        self.clear_conversation_rename();
        self.chat_ui.draft.clear();
        self.chat_ui.input_focused = false;
        self.chat_ui.autocomplete_index = 0;
        self.chat_ui.autocomplete_suppressed = false;
    }

    pub(in crate::workspace) fn reset_chat_message_list(&mut self) {
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

    pub(in crate::workspace) fn sync_chat_message_list(
        &mut self,
        conversation_id: &str,
        signatures: &[u64],
        spec: TauriVirtualListSpec,
    ) {
        let chat = &mut self.chat_ui;
        let mut cache = chat.message_list_cache.borrow_mut();
        let list_was_reset = sync_tauri_virtual_list_state_by_signatures(
            &mut chat.message_list_state,
            &mut cache,
            conversation_id,
            signatures,
            ListAlignment::Top,
            spec,
        );
        if list_was_reset {
            chat.message_list_state.set_follow_mode(FollowMode::Tail);
        }
    }

    pub(in crate::workspace) fn clear_chat_expansions(&mut self) {
        self.chat_ui.thinking_expansion_state.clear();
        self.chat_ui.tool_call_expansion_state.clear();
    }

    pub(in crate::workspace) fn remove_thinking_expansion(&mut self, message_id: &str) {
        self.chat_ui.thinking_expansion_state.remove(message_id);
    }

    pub(in crate::workspace) fn clear_chat_input_after_submit(&mut self) {
        self.chat_ui.draft.clear();
        self.chat_ui.autocomplete_index = 0;
        self.chat_ui.autocomplete_suppressed = false;
        self.chat_ui.include_context = false;
        self.chat_ui.include_all_panes = false;
    }

    pub(in crate::workspace) fn open_standard_chat_confirm(&mut self, kind: AiStandardConfirmKind) {
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

    pub(in crate::workspace) fn begin_standard_chat_confirm_exit(
        &mut self,
        kind: AiStandardConfirmKind,
    ) -> Option<u64> {
        match kind {
            AiStandardConfirmKind::Safety => self.chat_ui.safety_confirm_presence.begin_exit(),
            AiStandardConfirmKind::Summarize => {
                self.chat_ui.summarize_confirm_presence.begin_exit()
            }
        }
    }

    pub(in crate::workspace) fn finish_standard_chat_confirm_exit(
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

    pub(in crate::workspace) fn set_model_switch_warning(&mut self, percentage: Option<usize>) {
        self.chat_ui.model_switch_warning_percentage = percentage;
    }

    pub(in crate::workspace) fn set_prepared_prompt_usage(&mut self, usage: AiPreparedPromptUsage) {
        self.chat_ui.prepared_prompt_usage = Some(usage);
    }

    pub(in crate::workspace) fn show_context_trim_notice(&mut self, count: usize) -> u64 {
        self.chat_ui.context_trim_notice_count = Some(count);
        self.chat_ui.context_trim_notice_sequence =
            self.chat_ui.context_trim_notice_sequence.saturating_add(1);
        self.chat_ui.context_trim_notice_sequence
    }

    pub(in crate::workspace) fn clear_context_trim_notice(&mut self, sequence: u64) -> bool {
        if self.chat_ui.context_trim_notice_sequence != sequence {
            return false;
        }
        self.chat_ui.context_trim_notice_count = None;
        true
    }

    pub(in crate::workspace) fn set_chat_sidebar_width(&mut self, width: f32) {
        self.chat_ui.sidebar_width = width;
    }

    pub(in crate::workspace) fn set_chat_sidebar_resizing(&mut self, resizing: bool) {
        self.chat_ui.sidebar_resizing = resizing;
    }

    pub(in crate::workspace) fn finish_chat_sidebar_resize(&mut self) -> f32 {
        self.chat_ui.sidebar_resizing = false;
        self.chat_ui.sidebar_width
    }

    pub(in crate::workspace) fn replace_overlay_window_size(
        &mut self,
        size: (f32, f32),
    ) -> Option<(f32, f32)> {
        self.chat_ui.overlay_window_size.replace(size)
    }

    /// Retains workspace-window observers for exactly the AI Entity lifetime.
    pub(in crate::workspace) fn retain_window_observers(
        &mut self,
        overlay_bounds: Subscription,
        knowledge_activation: Subscription,
    ) {
        self.chat_ui.overlay_window_bounds_subscription = Some(overlay_bounds);
        self.knowledge_window_activation_subscription = Some(knowledge_activation);
    }

    pub(in crate::workspace) fn toggle_thinking_expansion(
        &mut self,
        key: String,
        default_expanded: bool,
    ) {
        let current = self
            .chat_ui
            .thinking_expansion_state
            .get(&key)
            .copied()
            .unwrap_or(default_expanded);
        self.chat_ui.thinking_expansion_state.insert(key, !current);
    }

    pub(in crate::workspace) fn toggle_tool_call_expansion(&mut self, key: String) {
        if !self.chat_ui.tool_call_expansion_state.remove(&key) {
            self.chat_ui.tool_call_expansion_state.insert(key);
        }
    }

    pub(in crate::workspace) fn model_selector_is_open(&self, scope: AiModelSelectorScope) -> bool {
        self.model_ui.selector_open && self.model_ui.selector_scope == Some(scope)
    }

    pub(in crate::workspace) fn model_selector_open(&self) -> bool {
        self.model_ui.selector_open
    }

    pub(in crate::workspace) fn model_selector_scope(&self) -> Option<AiModelSelectorScope> {
        self.model_ui.selector_scope
    }

    pub(in crate::workspace) fn model_selector_focus_origin(
        &self,
    ) -> Option<browser_behavior::BrowserFocusOrigin> {
        self.model_ui.selector_focus_origin
    }

    pub(in crate::workspace) fn set_model_selector_focus_origin(
        &mut self,
        origin: Option<browser_behavior::BrowserFocusOrigin>,
    ) {
        self.model_ui.selector_focus_origin = origin;
    }

    pub(in crate::workspace) fn model_selector_search_focused(&self) -> bool {
        self.model_ui.selector_search_focused
    }

    pub(in crate::workspace) fn set_model_selector_search_focused(&mut self, focused: bool) {
        self.model_ui.selector_search_focused = focused;
    }

    pub(in crate::workspace) fn model_selector_search_query(&self) -> &str {
        &self.model_ui.selector_search_query
    }

    pub(in crate::workspace) fn replace_model_selector_search(
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

    pub(in crate::workspace) fn pop_model_selector_search(&mut self) -> bool {
        let changed = self.model_ui.selector_search_query.pop().is_some()
            || self.model_ui.selector_highlighted_model.take().is_some();
        changed
    }

    pub(in crate::workspace) fn clear_model_selector_search(&mut self) -> bool {
        let changed = !self.model_ui.selector_search_query.is_empty()
            || self.model_ui.selector_highlighted_model.is_some();
        self.model_ui.selector_search_query.clear();
        self.model_ui.selector_highlighted_model = None;
        changed
    }

    pub(in crate::workspace) fn set_model_selector_open(
        &mut self,
        scope: AiModelSelectorScope,
        open: bool,
    ) {
        self.model_ui.selector_open = open;
        self.model_ui.selector_scope = open.then_some(scope);
        self.model_ui.selector_search_focused = open;
        self.model_ui.selector_highlighted_model = None;
    }

    pub(in crate::workspace) fn close_model_selector(&mut self) {
        self.model_ui.selector_open = false;
        self.model_ui.selector_scope = None;
        self.model_ui.selector_focus_origin = None;
        self.model_ui.selector_search_focused = false;
        self.model_ui.selector_search_query.clear();
        self.model_ui.selector_highlighted_model = None;
    }

    pub(in crate::workspace) fn model_selector_provider_expanded(&self, provider_id: &str) -> bool {
        self.model_ui
            .selector_expanded_providers
            .contains(provider_id)
    }

    pub(in crate::workspace) fn expand_model_selector_provider(&mut self, provider_id: String) {
        self.model_ui
            .selector_expanded_providers
            .insert(provider_id);
    }

    pub(in crate::workspace) fn toggle_model_selector_provider(
        &mut self,
        provider_id: String,
    ) -> bool {
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

    pub(in crate::workspace) fn model_selector_highlight(&self) -> Option<&(String, String)> {
        self.model_ui.selector_highlighted_model.as_ref()
    }

    pub(in crate::workspace) fn set_model_selector_highlight(
        &mut self,
        highlighted: Option<(String, String)>,
    ) -> bool {
        if self.model_ui.selector_highlighted_model == highlighted {
            return false;
        }
        self.model_ui.selector_highlighted_model = highlighted;
        true
    }

    pub(in crate::workspace) fn move_model_selector_highlight(
        &mut self,
        rows: &[(String, String)],
        delta: isize,
    ) {
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

    pub(in crate::workspace) fn set_model_selector_highlight_edge(
        &mut self,
        rows: &[(String, String)],
        last: bool,
    ) {
        self.model_ui.selector_highlighted_model = if last {
            rows.last().cloned()
        } else {
            rows.first().cloned()
        };
    }

    pub(in crate::workspace) fn model_selector_status_signature_matches(
        &self,
        signature: u64,
    ) -> bool {
        self.model_ui.selector_status_signature == Some(signature)
    }

    pub(in crate::workspace) fn set_model_selector_status_signature(
        &mut self,
        signature: Option<u64>,
    ) {
        self.model_ui.selector_status_signature = signature;
    }

    #[cfg(test)]
    pub(in crate::workspace) fn new(
        task_runtime: Arc<tokio::runtime::Runtime>,
        key_store: oxideterm_ai::AiProviderKeyStore,
        cx: &mut Context<Self>,
    ) -> Self {
        // Entity unit tests do not connect nodes, but still exercise the real
        // disabled filesystem and registry ownership paths.
        let agent_fs = NodeAgentIdeFileSystem::new(
            NodeRouter::new(oxideterm_ssh::SshConnectionRegistry::default()),
            oxideterm_ide_fs::NodeAgentMode::Disabled,
        );
        Self::new_with_agent_fs(task_runtime, key_store, agent_fs, cx)
    }

    pub(in crate::workspace) fn new_with_agent_fs(
        task_runtime: Arc<tokio::runtime::Runtime>,
        key_store: oxideterm_ai::AiProviderKeyStore,
        agent_fs: NodeAgentIdeFileSystem,
        cx: &mut Context<Self>,
    ) -> Self {
        let (model_refresh_tx, model_refresh_rx) =
            crate::workspace::delivery::ActiveDeliverySender::channel();
        let (provider_key_status_tx, provider_key_status_rx) =
            crate::workspace::delivery::ActiveDeliverySender::channel();
        let (selector_probe_tx, selector_probe_rx) =
            crate::workspace::delivery::ActiveDeliverySender::channel();
        let (acp_agent_probe_tx, acp_agent_probe_rx) =
            crate::workspace::delivery::ActiveDeliverySender::channel();
        let (acp_model_discovery_tx, acp_model_discovery_rx) =
            crate::workspace::delivery::ActiveDeliverySender::channel();
        let (knowledge_reindex_tx, knowledge_reindex_rx) =
            crate::workspace::delivery::ActiveDeliverySender::channel();
        let (terminal_inline_tx, terminal_inline_rx) =
            crate::workspace::delivery::ActiveDeliverySender::channel();
        let (chat_stream_tx, chat_stream_rx) =
            crate::workspace::delivery::ActiveDeliverySender::channel();
        let (compaction_tx, compaction_rx) =
            crate::workspace::delivery::ActiveDeliverySender::channel();
        let mcp_registry = oxideterm_ai::McpRegistry::new(key_store.clone());
        let entity = Self {
            task_runtime,
            model_ui: AiModelWorkspaceState::new(),
            chat_ui: AiChatWorkspaceState::new(360.0, None),
            knowledge_window_activation_subscription: None,
            key_store,
            visibility: AiWorkspaceVisibility::default(),
            settings_view: AiSettingsViewState::default(),
            settings_secret_drafts: HashMap::new(),
            focused_settings_input: None,
            provider_key_operation_tasks: HashMap::new(),
            pending_provider_key_removals: HashSet::new(),
            credential_intents: VecDeque::new(),
            mcp_add_dialog: None,
            mcp_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            mcp_dialog_exit_task: None,
            mcp_save_task: None,
            mcp_runtime_tasks: HashMap::new(),
            mcp_status_tick_task: None,
            settings_confirm: None,
            settings_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            settings_confirm_exit_task: None,
            settings_confirm_intents: VecDeque::new(),
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
            acp_agent_probe_tx,
            acp_agent_probe_rx,
            acp_agent_probe_intents: VecDeque::new(),
            acp_model_options: HashMap::new(),
            acp_model_discovery_pending: HashSet::new(),
            acp_model_discovery_tx,
            acp_model_discovery_rx,
            acp_model_discovery_intents: VecDeque::new(),
            rag_store: LazyAiRagStore::default(),
            knowledge_page: knowledge::KnowledgePageState::default(),
            knowledge_import_task: None,
            knowledge_embedding_task: None,
            knowledge_reindex_progress: None,
            knowledge_reindex_cancel: None,
            knowledge_reindex_tx,
            knowledge_reindex_rx,
            knowledge_reindex_intents: VecDeque::new(),
            terminal_inline_panel: AiInlinePanelState::default(),
            terminal_inline_tx,
            terminal_inline_rx,
            terminal_inline_stream_task: None,
            chat_stream_generation: 0,
            chat_stream_task: None,
            chat_stream_tx,
            chat_stream_rx,
            chat_stream_deliveries: VecDeque::new(),
            conversation_state: oxideterm_ai::AiChatState::default(),
            persistence_store: None,
            chat_initialized: false,
            chat_initialization_error: None,
            safety_modes_by_conversation: HashMap::new(),
            chat_loading: false,
            next_chat_sequence: 0,
            pending_tool_approvals: HashMap::new(),
            pending_acp_permission_choices: HashMap::new(),
            pending_tool_candidate_selections: HashMap::new(),
            tool_execution_records: VecDeque::new(),
            tool_result_facts: VecDeque::new(),
            agent_fs,
            mcp_registry,
            compaction_tx,
            compaction_rx,
            compaction_deliveries: VecDeque::new(),
            compacting_conversations: HashSet::new(),
            compaction_notice: None,
        };
        entity.schedule_model_refresh_delivery(cx);
        entity.schedule_provider_key_status_delivery(cx);
        entity.schedule_selector_probe_delivery(cx);
        entity.schedule_acp_agent_probe_delivery(cx);
        entity.schedule_acp_model_discovery_delivery(cx);
        entity.schedule_knowledge_reindex_delivery(cx);
        entity.schedule_terminal_inline_delivery(cx);
        entity.schedule_chat_stream_delivery(cx);
        entity.schedule_compaction_delivery(cx);
        entity
    }

    pub(in crate::workspace) fn set_workspace_visibility(
        &mut self,
        visibility: AiWorkspaceVisibility,
    ) -> bool {
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

    pub(in crate::workspace) fn model_is_refreshing(&self, provider_id: &str) -> bool {
        self.refreshing_models.contains(provider_id)
    }

    pub(in crate::workspace) fn settings_new_provider_type(&self) -> &str {
        &self.settings_view.new_provider_type
    }

    pub(in crate::workspace) fn select_settings_provider_type(
        &mut self,
        provider_type: &str,
        cx: &mut Context<Self>,
    ) {
        if self.settings_view.new_provider_type == provider_type {
            return;
        }
        self.settings_view.new_provider_type.clear();
        self.settings_view.new_provider_type.push_str(provider_type);
        cx.notify();
    }

    pub(in crate::workspace) fn settings_section_expanded(
        &self,
        section: AiSettingsViewSection,
    ) -> bool {
        match section {
            AiSettingsViewSection::ProviderSettings => {
                self.settings_view.provider_settings_expanded
            }
            AiSettingsViewSection::ToolUse => self.settings_view.tool_use_expanded,
            AiSettingsViewSection::ContextWindows => self.settings_view.context_windows_expanded,
        }
    }

    pub(in crate::workspace) fn toggle_settings_section(
        &mut self,
        section: AiSettingsViewSection,
        cx: &mut Context<Self>,
    ) {
        match section {
            AiSettingsViewSection::ProviderSettings => {
                self.settings_view.provider_settings_expanded =
                    !self.settings_view.provider_settings_expanded;
            }
            AiSettingsViewSection::ToolUse => {
                self.settings_view.tool_use_expanded = !self.settings_view.tool_use_expanded;
            }
            AiSettingsViewSection::ContextWindows => {
                self.settings_view.context_windows_expanded =
                    !self.settings_view.context_windows_expanded;
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn settings_provider_expanded(
        &self,
        provider_id: &str,
        default_expanded: bool,
    ) -> bool {
        self.settings_view
            .expanded_providers
            .get(provider_id)
            .copied()
            .unwrap_or(default_expanded)
    }

    pub(in crate::workspace) fn toggle_settings_provider_expanded(
        &mut self,
        provider_id: &str,
        default_expanded: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(expanded) = self.settings_view.expanded_providers.get_mut(provider_id) {
            *expanded = !*expanded;
        } else {
            self.settings_view
                .expanded_providers
                .insert(provider_id.to_owned(), !default_expanded);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn settings_provider_models_expanded(
        &self,
        provider_id: &str,
    ) -> bool {
        self.settings_view
            .expanded_provider_models
            .contains(provider_id)
    }

    pub(in crate::workspace) fn toggle_settings_provider_models(
        &mut self,
        provider_id: &str,
        cx: &mut Context<Self>,
    ) {
        if !self
            .settings_view
            .expanded_provider_models
            .remove(provider_id)
        {
            self.settings_view
                .expanded_provider_models
                .insert(provider_id.to_owned());
        }
        cx.notify();
    }

    pub(in crate::workspace) fn settings_context_provider_expanded(
        &self,
        provider_id: &str,
    ) -> bool {
        self.settings_view
            .expanded_context_providers
            .contains(provider_id)
    }

    pub(in crate::workspace) fn toggle_settings_context_provider(
        &mut self,
        provider_id: &str,
        cx: &mut Context<Self>,
    ) {
        if !self
            .settings_view
            .expanded_context_providers
            .remove(provider_id)
        {
            self.settings_view
                .expanded_context_providers
                .insert(provider_id.to_owned());
        }
        cx.notify();
    }

    pub(in crate::workspace) fn remove_settings_provider_view_state(
        &mut self,
        provider_id: &str,
        cx: &mut Context<Self>,
    ) {
        let provider_changed = self
            .settings_view
            .expanded_providers
            .remove(provider_id)
            .is_some();
        let models_changed = self
            .settings_view
            .expanded_provider_models
            .remove(provider_id);
        let context_changed = self
            .settings_view
            .expanded_context_providers
            .remove(provider_id);
        if provider_changed || models_changed || context_changed {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn hash_settings_provider_layout(&self, hasher: &mut impl Hasher) {
        self.settings_view.provider_settings_expanded.hash(hasher);
        for (provider_id, expanded) in &self.settings_view.expanded_providers {
            provider_id.hash(hasher);
            expanded.hash(hasher);
        }
        for provider_id in &self.settings_view.expanded_provider_models {
            provider_id.hash(hasher);
        }
    }

    pub(in crate::workspace) fn hash_settings_context_layout(&self, hasher: &mut impl Hasher) {
        self.settings_view.context_windows_expanded.hash(hasher);
        for provider_id in &self.settings_view.expanded_context_providers {
            provider_id.hash(hasher);
        }
    }

    pub(in crate::workspace) fn owns_settings_input(input: SettingsInput) -> bool {
        matches!(
            input,
            SettingsInput::AiProviderApiKey(_)
                | SettingsInput::KnowledgeCollectionName
                | SettingsInput::KnowledgeDocumentTitle
        ) || input.is_ai_mcp()
    }

    pub(in crate::workspace) fn focused_settings_input(&self) -> Option<SettingsInput> {
        self.focused_settings_input
    }

    pub(in crate::workspace) fn settings_input_value(&self, input: SettingsInput) -> Option<&str> {
        if let Some(draft) = self.settings_secret_drafts.get(&input) {
            return Some(draft.as_str());
        }
        match input {
            SettingsInput::KnowledgeCollectionName => {
                return Some(&self.knowledge_page.new_collection_name);
            }
            SettingsInput::KnowledgeDocumentTitle => {
                return Some(&self.knowledge_page.new_document_title);
            }
            _ => {}
        }
        let draft = self.mcp_add_dialog.as_ref()?;
        match input {
            SettingsInput::AiMcpName => Some(&draft.name),
            SettingsInput::AiMcpCommand => Some(&draft.command),
            SettingsInput::AiMcpArgs => Some(&draft.args),
            SettingsInput::AiMcpUrl => Some(&draft.url),
            SettingsInput::AiMcpAuthHeaderName => Some(&draft.auth_header_name),
            SettingsInput::AiMcpAuthToken => Some(&draft.auth_token),
            SettingsInput::AiMcpEnvKey(index) => draft.env.get(index).map(|(key, _)| key.as_str()),
            SettingsInput::AiMcpEnvValue(index) => {
                draft.env.get(index).map(|(_, value)| value.as_str())
            }
            SettingsInput::AiMcpHeaderKey(index) => {
                draft.headers.get(index).map(|(key, _)| key.as_str())
            }
            SettingsInput::AiMcpHeaderValue(index) => {
                draft.headers.get(index).map(|(_, value)| value.as_str())
            }
            _ => None,
        }
    }

    pub(in crate::workspace) fn focus_settings_input(
        &mut self,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) -> bool {
        if !Self::owns_settings_input(input) {
            return false;
        }
        if input.is_ai_mcp() && self.mcp_add_dialog.is_none() {
            return false;
        }
        if self.focused_settings_input == Some(input) {
            return true;
        }

        self.clear_focused_settings_input();
        if matches!(input, SettingsInput::AiProviderApiKey(_)) {
            self.settings_secret_drafts
                .insert(input, zeroize::Zeroizing::new(String::new()));
        }
        self.focused_settings_input = Some(input);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn blur_settings_input(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.clear_focused_settings_input();
        if changed {
            cx.notify();
        }
        changed
    }

    pub(in crate::workspace) fn replace_settings_input(
        &mut self,
        input: SettingsInput,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.focused_settings_input != Some(input) {
            return false;
        }
        let value = if let Some(draft) = self.settings_secret_drafts.get_mut(&input) {
            &mut **draft
        } else if let Some(value) = self.knowledge_page.input_value_mut(input) {
            value
        } else {
            let Some(draft) = self.mcp_add_dialog.as_mut() else {
                return false;
            };
            let Some(value) = mcp_draft_input_value_mut(draft, input) else {
                return false;
            };
            value
        };
        replace_utf16(value, replacement_range, text);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn pop_settings_input(
        &mut self,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.focused_settings_input != Some(input) {
            return false;
        }
        let changed = if let Some(draft) = self.settings_secret_drafts.get_mut(&input) {
            draft.pop().is_some()
        } else if let Some(value) = self.knowledge_page.input_value_mut(input) {
            value.pop().is_some()
        } else {
            self.mcp_add_dialog
                .as_mut()
                .and_then(|draft| mcp_draft_input_value_mut(draft, input))
                .is_some_and(|value| value.pop().is_some())
        };
        if changed {
            cx.notify();
        }
        true
    }

    pub(in crate::workspace) fn take_provider_key_secret(
        &mut self,
        input: SettingsInput,
    ) -> Option<zeroize::Zeroizing<String>> {
        if !matches!(input, SettingsInput::AiProviderApiKey(_)) {
            return None;
        }
        self.take_settings_secret(input)
    }

    fn take_settings_secret(&mut self, input: SettingsInput) -> Option<zeroize::Zeroizing<String>> {
        if self.focused_settings_input != Some(input) {
            return None;
        }
        self.focused_settings_input = None;
        let secret = self.settings_secret_drafts.remove(&input)?;
        (!secret.trim().is_empty()).then_some(secret)
    }

    fn clear_focused_settings_input(&mut self) -> bool {
        let Some(input) = self.focused_settings_input.take() else {
            return false;
        };
        // Provider and ACP drafts have no second owner; MCP values remain in
        // the zeroizing dialog draft when their input merely loses focus.
        self.settings_secret_drafts.remove(&input);
        true
    }

    pub(in crate::workspace) fn conversation_state(&self) -> &oxideterm_ai::AiChatState {
        &self.conversation_state
    }

    pub(in crate::workspace) fn rag_store(&self) -> Arc<oxideterm_ai::RagStore> {
        self.rag_store.get()
    }

    pub(in crate::workspace) fn conversation_state_mut(
        &mut self,
    ) -> &mut oxideterm_ai::AiChatState {
        // Callers receive unrestricted mutable conversation access, so any
        // cached render signature may become stale after this boundary.
        self.chat_ui
            .message_signature_cache
            .borrow_mut()
            .invalidate_all();
        &mut self.conversation_state
    }

    pub(in crate::workspace) fn update_chat_message(
        &mut self,
        conversation_id: &str,
        message_id: &str,
        update: impl FnOnce(&mut oxideterm_ai::AiChatMessage),
    ) {
        // Streaming changes one assistant message at a time. Invalidate only
        // that row so long histories are not rehashed for every token.
        self.chat_ui
            .message_signature_cache
            .borrow_mut()
            .invalidate_message(message_id);
        self.conversation_state
            .update_message(conversation_id, message_id, update);
    }

    pub(in crate::workspace) fn chat_is_loading(&self) -> bool {
        self.chat_loading
    }

    pub(in crate::workspace) fn set_chat_loading(&mut self, loading: bool) {
        self.chat_loading = loading;
    }

    pub(in crate::workspace) fn chat_initialization_error(
        &self,
    ) -> Option<&AiChatInitializationError> {
        self.chat_initialization_error.as_ref()
    }

    pub(in crate::workspace) fn active_conversation_safety_mode(
        &self,
    ) -> oxideterm_gpui_ui::ai::AiSafetyMode {
        self.conversation_state
            .active_conversation_id
            .as_ref()
            .and_then(|id| self.safety_modes_by_conversation.get(id))
            .copied()
            .unwrap_or(oxideterm_gpui_ui::ai::AiSafetyMode::Default)
    }

    pub(in crate::workspace) fn ensure_chat_initialized(
        &mut self,
        path: PathBuf,
    ) -> AiChatInitializationOutcome {
        if self.chat_initialized {
            return AiChatInitializationOutcome::AlreadyInitialized;
        }
        self.chat_initialized = true;
        self.load_chat_store(path)
    }

    pub(in crate::workspace) fn retry_chat_initialization(
        &mut self,
        path: PathBuf,
    ) -> AiChatInitializationOutcome {
        self.chat_initialized = true;
        self.load_chat_store(path)
    }

    fn load_chat_store(&mut self, path: PathBuf) -> AiChatInitializationOutcome {
        self.chat_ui
            .message_signature_cache
            .borrow_mut()
            .invalidate_all();
        match oxideterm_ai::AiChatPersistenceStore::load(path) {
            Ok((store, state)) => {
                self.persistence_store = Some(store);
                self.conversation_state = state;
                self.chat_initialization_error = None;
                AiChatInitializationOutcome::Loaded
            }
            Err(error) => {
                // Database errors can contain local paths or serialized data;
                // retain only the stable presentation category.
                self.conversation_state = oxideterm_ai::AiChatState::default();
                self.persistence_store = None;
                self.chat_initialization_error = Some(ai_chat_initialization_error(&error));
                AiChatInitializationOutcome::Failed
            }
        }
    }

    pub(in crate::workspace) fn select_conversation(&mut self, id: String) {
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
        if let Some(previous_index) = previous_index {
            let previous = &mut self.conversation_state.conversations[previous_index];
            previous.messages.clear();
            previous.messages_loaded = false;
        }
        if let Some(conversation) = self
            .conversation_state
            .conversations
            .iter()
            .find(|conversation| conversation.id == id)
            && !conversation.messages_loaded
            && let Some(store) = self.persistence_store.as_ref()
            && let Ok(Some(loaded)) = store.load_conversation(&id)
            && let Some(slot) = self
                .conversation_state
                .conversations
                .iter_mut()
                .find(|conversation| conversation.id == id)
        {
            *slot = loaded;
        }
        self.conversation_state.set_active_conversation(id);
    }

    pub(in crate::workspace) fn delete_conversation(&mut self, id: &str) -> bool {
        self.conversation_state.delete_conversation(id);
        self.safety_modes_by_conversation.remove(id);
        if self.chat_ui.renaming_conversation_id.as_deref() == Some(id) {
            self.clear_conversation_rename();
        }
        !self.conversation_state.conversations.is_empty()
    }

    pub(in crate::workspace) fn rename_conversation(
        &mut self,
        id: &str,
        title: String,
        now_ms: i64,
    ) {
        self.conversation_state
            .rename_conversation(id, title, now_ms);
        self.clear_conversation_rename();
    }

    pub(in crate::workspace) fn clear_conversations(&mut self) {
        self.conversation_state.clear_conversations();
        self.safety_modes_by_conversation.clear();
        self.clear_conversation_rename();
    }

    pub(in crate::workspace) fn set_active_conversation_safety_mode(
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

    pub(in crate::workspace) fn persist_chat_state(&self) {
        let Some(store) = self.persistence_store.as_ref().cloned() else {
            return;
        };
        // The blocking persistence task needs an owned point-in-time projection.
        // Keep this as the only full conversation-state clone at the boundary.
        let state = self.conversation_state.clone();
        let projection_updated_at =
            oxideterm_ai::AiChatPersistenceStore::next_projection_persist_at();
        self.task_runtime.spawn_blocking(move || {
            if store
                .save_state_with_projection_updated_at(state, projection_updated_at)
                .is_err()
            {
                // Persistence errors may include local paths or serialized data.
                eprintln!("[AiChatStore] Failed to persist conversation");
            }
        });
    }

    pub(in crate::workspace) fn persist_transcript_entries(
        &self,
        conversation_id: String,
        entries: Vec<oxideterm_ai::PersistedTranscriptEntry>,
    ) {
        if entries.is_empty() {
            return;
        }
        let Some(store) = self.persistence_store.as_ref().cloned() else {
            return;
        };
        // The store is a shared handle; only the owned entries cross the worker boundary.
        self.task_runtime.spawn_blocking(move || {
            if store
                .append_transcript_entries(&conversation_id, entries)
                .is_err()
            {
                eprintln!("[AiChatStore] Failed to persist transcript entries");
            }
        });
    }

    pub(in crate::workspace) fn persist_diagnostic_events(
        &self,
        conversation_id: String,
        events: Vec<oxideterm_ai::PersistedDiagnosticEvent>,
    ) {
        if events.is_empty() {
            return;
        }
        let Some(store) = self.persistence_store.as_ref().cloned() else {
            return;
        };
        // The store is a shared handle; only the owned events cross the worker boundary.
        self.task_runtime.spawn_blocking(move || {
            if store
                .append_diagnostic_events(&conversation_id, events)
                .is_err()
            {
                eprintln!("[AiChatStore] Failed to persist diagnostic events");
            }
        });
    }

    pub(in crate::workspace) fn cancel_chat_conversation_state(
        &mut self,
    ) -> (
        Option<String>,
        Vec<oxideterm_ai::stream_state::AiStoppedAssistantTurn>,
    ) {
        self.chat_loading = false;
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
        (conversation_id, stopped_turns)
    }

    pub(in crate::workspace) fn next_chat_id(&mut self, now_ms: i64) -> String {
        self.next_chat_sequence = self.next_chat_sequence.saturating_add(1);
        format!("chat-{now_ms}-{}", self.next_chat_sequence)
    }

    pub(in crate::workspace) fn request_model_refresh(
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
                        Ok(Err(_)) | Err(_) => {
                            let _ = worker_tx.send(AiModelRefreshWorkerDelivery {
                                index,
                                provider_id,
                                generation,
                                result: Err(AiModelRefreshFailure::Failed),
                            });
                            return;
                        }
                    }
                }
            };
            let result = oxideterm_ai::fetch_provider_models(provider, api_key)
                .await
                .map_err(|_| AiModelRefreshFailure::Failed);
            let _ = worker_tx.send(AiModelRefreshWorkerDelivery {
                index,
                provider_id,
                generation,
                result,
            });
        });
        true
    }

    pub(in crate::workspace) fn take_model_refresh_intents(
        &mut self,
    ) -> VecDeque<AiModelRefreshIntent> {
        std::mem::take(&mut self.model_refresh_intents)
    }

    pub(in crate::workspace) fn provider_has_key(&self, provider_id: &str) -> bool {
        self.provider_key_status
            .get(provider_id)
            .copied()
            .unwrap_or(false)
    }

    pub(in crate::workspace) fn set_provider_key_status(
        &mut self,
        provider_id: String,
        has_key: bool,
    ) {
        self.provider_key_status_pending.remove(&provider_id);
        self.provider_key_status.insert(provider_id, has_key);
    }

    pub(in crate::workspace) fn store_provider_key(
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

    pub(in crate::workspace) fn remove_provider_key(
        &mut self,
        provider_id: String,
        cx: &mut Context<Self>,
    ) -> bool {
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

    pub(in crate::workspace) fn take_credential_intents(&mut self) -> VecDeque<AiCredentialIntent> {
        std::mem::take(&mut self.credential_intents)
    }

    pub(in crate::workspace) fn settings_confirm_is_open(&self) -> bool {
        self.settings_confirm.is_some()
    }

    pub(in crate::workspace) fn settings_confirm_phase(
        &self,
    ) -> oxideterm_gpui_ui::motion::ExitPhase {
        self.settings_confirm_presence.phase()
    }

    pub(in crate::workspace) fn settings_confirm_is_enable(&self) -> bool {
        matches!(self.settings_confirm, Some(AiSettingsConfirm::Enable))
    }

    pub(in crate::workspace) fn settings_confirm_is_provider_key_remove(&self) -> bool {
        matches!(
            self.settings_confirm,
            Some(AiSettingsConfirm::RemoveProviderKey { .. })
        )
    }

    pub(in crate::workspace) fn settings_confirm_provider_name(&self) -> Option<&str> {
        match self.settings_confirm.as_ref() {
            Some(AiSettingsConfirm::RemoveProvider { provider_name, .. }) => Some(provider_name),
            _ => None,
        }
    }

    pub(in crate::workspace) fn settings_confirm_is_visible(&self) -> bool {
        self.settings_confirm_presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible
    }

    pub(in crate::workspace) fn open_ai_enable_confirm(&mut self, cx: &mut Context<Self>) {
        self.open_settings_confirm(AiSettingsConfirm::Enable, cx);
    }

    pub(in crate::workspace) fn open_provider_key_remove_confirm(
        &mut self,
        index: usize,
        provider_id: String,
        cx: &mut Context<Self>,
    ) {
        self.open_settings_confirm(
            AiSettingsConfirm::RemoveProviderKey { index, provider_id },
            cx,
        );
    }

    pub(in crate::workspace) fn open_provider_remove_confirm(
        &mut self,
        provider_id: String,
        provider_name: String,
        cx: &mut Context<Self>,
    ) {
        self.open_settings_confirm(
            AiSettingsConfirm::RemoveProvider {
                provider_id,
                provider_name,
            },
            cx,
        );
    }

    fn open_settings_confirm(&mut self, confirm: AiSettingsConfirm, cx: &mut Context<Self>) {
        self.settings_confirm_exit_task = None;
        self.settings_confirm_presence.reopen();
        self.settings_confirm = Some(confirm);
        cx.emit(AiWorkspaceEvent::SettingsConfirmChanged);
        cx.notify();
    }

    pub(in crate::workspace) fn begin_settings_confirm_exit(
        &mut self,
        confirmed: bool,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.settings_confirm.is_none() {
            return false;
        }
        let Some(generation) = self.settings_confirm_presence.begin_exit() else {
            return false;
        };
        if delay.is_zero() {
            self.finish_settings_confirm_exit(generation, confirmed, cx);
            return true;
        }

        // The AI owner retains modal completion across settings visibility
        // changes so a confirmed user action cannot be stranded in the root.
        self.settings_confirm_exit_task = Some(cx.spawn(async move |entity, cx| {
            Timer::after(delay).await;
            let _ = entity.update(cx, |entity, cx| {
                entity.settings_confirm_exit_task = None;
                entity.finish_settings_confirm_exit(generation, confirmed, cx);
            });
        }));
        cx.emit(AiWorkspaceEvent::SettingsConfirmChanged);
        cx.notify();
        true
    }

    fn finish_settings_confirm_exit(
        &mut self,
        generation: u64,
        confirmed: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.settings_confirm_presence.finish_exit(generation) {
            return;
        }
        self.settings_confirm_presence.reopen();
        let confirm = self.settings_confirm.take();
        if confirmed {
            let intent = match confirm {
                Some(AiSettingsConfirm::Enable) => Some(AiSettingsConfirmIntent::Enable),
                Some(AiSettingsConfirm::RemoveProviderKey { index, provider_id }) => {
                    Some(AiSettingsConfirmIntent::RemoveProviderKey { index, provider_id })
                }
                Some(AiSettingsConfirm::RemoveProvider { provider_id, .. }) => {
                    Some(AiSettingsConfirmIntent::RemoveProvider { provider_id })
                }
                None => None,
            };
            if let Some(intent) = intent {
                self.settings_confirm_intents.push_back(intent);
            }
        }
        cx.emit(AiWorkspaceEvent::SettingsConfirmChanged);
        cx.notify();
    }

    pub(in crate::workspace) fn take_settings_confirm_intents(
        &mut self,
    ) -> VecDeque<AiSettingsConfirmIntent> {
        std::mem::take(&mut self.settings_confirm_intents)
    }

    pub(in crate::workspace) fn open_chat_confirm(
        &mut self,
        confirm: AiChatConfirmKind,
        cx: &mut Context<Self>,
    ) {
        // Reopen is a new generation and dropping the retained task prevents a
        // stale exit from clearing the replacement payload.
        self.chat_confirm_exit_task = None;
        self.chat_confirm = Some(confirm);
        self.chat_confirm_presence.reopen();
        self.chat_confirm_focused_action = None;
        cx.notify();
    }

    pub(in crate::workspace) fn chat_confirm_snapshot(&self) -> Option<AiChatConfirmSnapshot> {
        self.chat_confirm
            .as_ref()
            .cloned()
            .map(|kind| AiChatConfirmSnapshot {
                kind,
                phase: self.chat_confirm_presence.phase(),
                focused_action: self.chat_confirm_focused_action,
            })
    }

    pub(in crate::workspace) fn chat_confirm_owner_snapshot(
        &self,
    ) -> Option<AiChatConfirmOwnerSnapshot> {
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

    pub(in crate::workspace) fn handle_chat_confirm_key(
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
        match browser_behavior::modal_footer_key_action(
            key,
            shift,
            &ACTIONS,
            self.chat_confirm_focused_action,
            ConfirmDialogAction::Cancel,
        ) {
            Some(browser_behavior::ModalFooterKeyAction::Cancel) => {
                self.chat_confirm_focused_action = None;
                Some(AiChatConfirmKeyAction::Cancel)
            }
            Some(browser_behavior::ModalFooterKeyAction::Focus(action)) => {
                self.chat_confirm_focused_action = Some(action);
                cx.notify();
                Some(AiChatConfirmKeyAction::Handled)
            }
            Some(browser_behavior::ModalFooterKeyAction::Activate(action)) => {
                self.chat_confirm_focused_action = None;
                Some(match action {
                    ConfirmDialogAction::Cancel => AiChatConfirmKeyAction::Cancel,
                    ConfirmDialogAction::Confirm => AiChatConfirmKeyAction::Confirm,
                })
            }
            None => None,
        }
    }

    pub(in crate::workspace) fn begin_chat_confirm_exit(
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

    pub(in crate::workspace) fn mcp_dialog_is_open(&self) -> bool {
        self.mcp_add_dialog.is_some()
    }

    pub(in crate::workspace) fn mcp_draft_is_valid(&self, settings: &PersistedSettings) -> bool {
        self.mcp_add_dialog
            .as_ref()
            .is_some_and(|draft| ai_mcp_draft_valid(draft, settings))
    }

    pub(in crate::workspace) fn mcp_transport(&self) -> Option<oxideterm_ai::McpTransport> {
        self.mcp_add_dialog.as_ref().map(|draft| draft.transport)
    }

    pub(in crate::workspace) fn mcp_auth_mode(&self) -> Option<oxideterm_ai::McpAuthHeaderMode> {
        self.mcp_add_dialog
            .as_ref()
            .map(|draft| draft.auth_header_mode)
    }

    pub(in crate::workspace) fn mcp_auth_token_visible(&self) -> bool {
        self.mcp_add_dialog
            .as_ref()
            .is_some_and(|draft| draft.show_auth_token)
    }

    pub(in crate::workspace) fn mcp_retry_enabled(&self) -> bool {
        self.mcp_add_dialog
            .as_ref()
            .is_some_and(|draft| draft.retry_on_disconnect)
    }

    pub(in crate::workspace) fn mcp_dialog_presence(
        &self,
    ) -> oxideterm_gpui_ui::motion::ExitPresence {
        self.mcp_dialog_presence
    }

    pub(in crate::workspace) fn open_mcp_add_dialog(&mut self, cx: &mut Context<Self>) -> bool {
        if self.mcp_save_task.is_some() {
            return false;
        }
        self.mcp_dialog_exit_task = None;
        self.clear_focused_settings_input();
        self.mcp_dialog_presence.reopen();
        self.mcp_add_dialog = Some(AiMcpServerDraft::default());
        cx.notify();
        true
    }

    pub(in crate::workspace) fn set_mcp_transport(
        &mut self,
        transport: oxideterm_ai::McpTransport,
        cx: &mut Context<Self>,
    ) {
        if let Some(draft) = self.mcp_add_dialog.as_mut() {
            draft.transport = transport;
            cx.notify();
        }
    }

    pub(in crate::workspace) fn set_mcp_auth_mode(
        &mut self,
        mode: oxideterm_ai::McpAuthHeaderMode,
        cx: &mut Context<Self>,
    ) {
        if let Some(draft) = self.mcp_add_dialog.as_mut() {
            draft.auth_header_mode = mode;
            cx.notify();
        }
    }

    pub(in crate::workspace) fn toggle_mcp_auth_token_visibility(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if let Some(draft) = self.mcp_add_dialog.as_mut() {
            draft.show_auth_token = !draft.show_auth_token;
            cx.notify();
        }
    }

    pub(in crate::workspace) fn toggle_mcp_retry(&mut self, cx: &mut Context<Self>) {
        if let Some(draft) = self.mcp_add_dialog.as_mut() {
            draft.retry_on_disconnect = !draft.retry_on_disconnect;
            cx.notify();
        }
    }

    pub(in crate::workspace) fn mcp_record_len(&self, env: bool) -> usize {
        self.mcp_add_dialog
            .as_ref()
            .map(|draft| {
                if env {
                    draft.env.len()
                } else {
                    draft.headers.len()
                }
            })
            .unwrap_or(0)
    }

    pub(in crate::workspace) fn add_mcp_record_entry(&mut self, env: bool, cx: &mut Context<Self>) {
        let Some(draft) = self.mcp_add_dialog.as_mut() else {
            return;
        };
        if env {
            draft
                .env
                .push((format!("KEY_{}", draft.env.len() + 1), String::new()));
        } else {
            draft
                .headers
                .push((format!("HEADER_{}", draft.headers.len() + 1), String::new()));
        }
        cx.notify();
    }

    pub(in crate::workspace) fn remove_mcp_record_entry(
        &mut self,
        env: bool,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = self.mcp_add_dialog.as_mut() else {
            return;
        };
        let removed = if env {
            (index < draft.env.len()).then(|| draft.env.remove(index))
        } else {
            (index < draft.headers.len()).then(|| draft.headers.remove(index))
        };
        if removed.is_some() {
            let focus_shifted = self
                .focused_settings_input
                .is_some_and(|input| match input {
                    SettingsInput::AiMcpEnvKey(input_index)
                    | SettingsInput::AiMcpEnvValue(input_index) => env && input_index >= index,
                    SettingsInput::AiMcpHeaderKey(input_index)
                    | SettingsInput::AiMcpHeaderValue(input_index) => !env && input_index >= index,
                    _ => false,
                });
            if focus_shifted {
                self.focused_settings_input = None;
            }
            cx.notify();
        }
    }

    pub(in crate::workspace) fn begin_mcp_dialog_exit(
        &mut self,
        submit: bool,
        delay: Duration,
        configured_names: HashSet<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.mcp_add_dialog.is_none() {
            return false;
        }
        self.clear_focused_settings_input();
        let Some(generation) = self.mcp_dialog_presence.begin_exit() else {
            return false;
        };
        if delay.is_zero() {
            self.finish_mcp_dialog_exit(generation, submit, configured_names, cx);
            return true;
        }

        // The Entity retains exit completion so a settings visibility change
        // cannot strand a secret-bearing dialog in its transitional phase.
        self.mcp_dialog_exit_task = Some(cx.spawn(async move |entity, cx| {
            Timer::after(delay).await;
            let _ = entity.update(cx, |entity, cx| {
                entity.mcp_dialog_exit_task = None;
                entity.finish_mcp_dialog_exit(generation, submit, configured_names, cx);
            });
        }));
        cx.notify();
        true
    }

    fn finish_mcp_dialog_exit(
        &mut self,
        generation: u64,
        submit: bool,
        configured_names: HashSet<String>,
        cx: &mut Context<Self>,
    ) {
        if !self.mcp_dialog_presence.finish_exit(generation) {
            return;
        }
        self.mcp_dialog_presence.reopen();
        if !submit {
            // Dropping the draft zeroizes tokens, args, env, and headers.
            self.mcp_add_dialog = None;
            cx.notify();
            return;
        }
        self.start_mcp_server_add(configured_names, cx);
    }

    fn start_mcp_server_add(&mut self, configured_names: HashSet<String>, cx: &mut Context<Self>) {
        let Some(mut draft) = self.mcp_add_dialog.take() else {
            return;
        };
        if !ai_mcp_draft_valid_for_names(&draft, &configured_names) {
            self.mcp_add_dialog = Some(draft);
            cx.notify();
            return;
        }

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
                    entity.mcp_add_dialog = Some(pending_draft);
                    entity.mcp_dialog_presence.reopen();
                    entity
                        .credential_intents
                        .push_back(AiCredentialIntent::Failed(
                            AiCredentialFailure::SaveMcpToken,
                        ));
                }
                cx.emit(AiWorkspaceEvent::CredentialOperationReady);
                cx.notify();
            });
        }));
    }

    pub(in crate::workspace) fn request_mcp_status_tick(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
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

    pub(in crate::workspace) fn refresh_mcp_tools(
        &mut self,
        server_id: String,
        cx: &mut Context<Self>,
    ) -> bool {
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

    pub(in crate::workspace) fn set_mcp_server_connected(
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

    pub(in crate::workspace) fn remove_mcp_server(
        &mut self,
        server_id: String,
        cx: &mut Context<Self>,
    ) -> bool {
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

    pub(in crate::workspace) fn invalidate_provider_key_status(&mut self, provider_id: &str) {
        self.provider_key_status.remove(provider_id);
        self.provider_key_status_pending.remove(provider_id);
    }

    pub(in crate::workspace) fn request_provider_key_statuses(
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

    pub(in crate::workspace) fn selector_provider_is_online(&self, provider_id: &str) -> bool {
        self.selector_provider_online
            .get(provider_id)
            .copied()
            .unwrap_or(true)
    }

    pub(in crate::workspace) fn set_selector_provider_online(
        &mut self,
        provider_id: String,
        online: bool,
    ) {
        // Direct state transitions supersede any older network probe result.
        self.selector_probe_generations.remove(&provider_id);
        self.selector_provider_online.insert(provider_id, online);
    }

    pub(in crate::workspace) fn invalidate_selector_provider_status(&mut self, provider_id: &str) {
        self.selector_probe_generations.remove(provider_id);
        self.selector_provider_online.remove(provider_id);
    }

    pub(in crate::workspace) fn request_selector_provider_probe(
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
        self.task_runtime.spawn(async move {
            let online =
                oxideterm_ai::check_model_selector_provider_online(&provider.base_url, endpoint)
                    .await;
            let _ = worker_tx.send(AiModelSelectorProbeDelivery {
                provider_id,
                generation,
                online,
            });
        });
        true
    }

    pub(in crate::workspace) fn acp_agent_probe_is_pending(&self, agent_id: &str) -> bool {
        self.acp_agent_probe_pending.contains(agent_id)
    }

    pub(in crate::workspace) fn request_acp_agent_probe(
        &mut self,
        agent: oxideterm_settings::AcpAgentConfig,
    ) -> bool {
        if !self.visibility.settings_surface || self.acp_agent_probe_pending.contains(&agent.id) {
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
        self.task_runtime.spawn(async move {
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
            let _ = worker_tx.send(AiAcpAgentProbeDelivery { agent_id, result });
        });
        true
    }

    pub(in crate::workspace) fn take_acp_agent_probe_intents(
        &mut self,
    ) -> VecDeque<AiAcpAgentProbeIntent> {
        std::mem::take(&mut self.acp_agent_probe_intents)
    }

    pub(in crate::workspace) fn acp_model_options(
        &self,
        conversation_id: &str,
        agent_id: &str,
    ) -> Option<Vec<oxideterm_ai::AcpSessionConfigOption>> {
        self.acp_model_options
            .get(&(conversation_id.to_string(), agent_id.to_string()))
            .cloned()
    }

    pub(in crate::workspace) fn acp_model_discovery_is_pending(
        &self,
        conversation_id: &str,
        agent_id: &str,
    ) -> bool {
        self.acp_model_discovery_pending
            .contains(&(conversation_id.to_string(), agent_id.to_string()))
    }

    pub(in crate::workspace) fn request_acp_model_discovery(
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
        self.task_runtime.spawn(async move {
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
                conversation_id,
                agent_id,
                config_options,
            });
        });
        true
    }

    pub(in crate::workspace) fn take_acp_model_discovery_intents(
        &mut self,
    ) -> VecDeque<AiAcpModelDiscoveryIntent> {
        std::mem::take(&mut self.acp_model_discovery_intents)
    }

    pub(in crate::workspace) fn apply_acp_model_discovery(
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

    pub(in crate::workspace) fn knowledge_reindex_progress(&self) -> Option<(usize, usize)> {
        self.knowledge_reindex_progress
    }

    pub(in crate::workspace) fn request_knowledge_reindex(
        &mut self,
        store: Arc<oxideterm_ai::RagStore>,
        collection_id: String,
    ) -> bool {
        if self.knowledge_reindex_progress.is_some() {
            return false;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let worker_tx = self.knowledge_reindex_tx.clone();
        self.knowledge_reindex_progress = Some((0, 0));
        self.knowledge_reindex_cancel = Some(cancel);
        // Reindexing is blocking storage work, so keep it off the async runtime workers.
        self.task_runtime.spawn_blocking(move || {
            let mut last_emitted = 0usize;
            let mut on_progress = |current: usize, total: usize| {
                if current == total || current.saturating_sub(last_emitted) >= 10 {
                    let _ = worker_tx.send(AiKnowledgeReindexDelivery::Progress { current, total });
                    last_emitted = current;
                }
            };
            let failed = oxideterm_ai::rag_reindex_collection_with_progress(
                &store,
                &collection_id,
                Some(worker_cancel.as_ref()),
                Some(&mut on_progress),
            )
            .is_err();
            // Storage errors may contain paths or indexed content, so only the
            // stable failure bit crosses back to the GPUI entity.
            let _ = worker_tx.send(AiKnowledgeReindexDelivery::Finished { failed });
        });
        true
    }

    pub(in crate::workspace) fn cancel_knowledge_reindex(&self) -> bool {
        let Some(cancel) = self.knowledge_reindex_cancel.as_ref() else {
            return false;
        };
        cancel.store(true, Ordering::Relaxed);
        true
    }

    pub(in crate::workspace) fn take_knowledge_reindex_intents(
        &mut self,
    ) -> VecDeque<AiKnowledgeReindexIntent> {
        std::mem::take(&mut self.knowledge_reindex_intents)
    }

    pub(in crate::workspace) fn terminal_inline_panel(&self) -> &AiInlinePanelState {
        &self.terminal_inline_panel
    }

    pub(in crate::workspace) fn terminal_inline_panel_mut(&mut self) -> &mut AiInlinePanelState {
        &mut self.terminal_inline_panel
    }

    pub(in crate::workspace) fn open_terminal_inline_panel(&mut self, selection_context: String) {
        self.abort_terminal_inline_stream();
        let panel = &mut self.terminal_inline_panel;
        panel.open = true;
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

    pub(in crate::workspace) fn close_terminal_inline_panel(&mut self) {
        self.abort_terminal_inline_stream();
        let panel = &mut self.terminal_inline_panel;
        panel.open = false;
        panel.prompt_focused = false;
        panel.loading = false;
        panel.error = None;
        panel.generation = panel.generation.wrapping_add(1);
    }

    pub(in crate::workspace) fn terminal_inline_request_context(&self) -> Option<(String, String)> {
        let panel = &self.terminal_inline_panel;
        if panel.loading || panel.prompt.trim().is_empty() {
            return None;
        }
        Some((
            oxideterm_ai::sanitize_for_ai(&panel.prompt),
            panel.selection_context.clone(),
        ))
    }

    pub(in crate::workspace) fn request_terminal_inline(
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

    pub(in crate::workspace) fn refresh_terminal_inline_key_status(
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

    pub(in crate::workspace) fn chat_stream_generation(&self) -> u64 {
        self.chat_stream_generation
    }

    pub(in crate::workspace) fn is_chat_stream_generation(&self, generation: u64) -> bool {
        self.chat_stream_generation == generation
    }

    pub(in crate::workspace) fn begin_chat_stream(&mut self) -> (u64, AiStreamDeliverySender) {
        self.reject_all_tool_interactions();
        if let Some(task) = self.chat_stream_task.take() {
            task.abort();
        }
        self.chat_stream_generation = self.chat_stream_generation.saturating_add(1);
        (self.chat_stream_generation, self.chat_stream_tx.clone())
    }

    pub(in crate::workspace) fn enqueue_chat_stream_delivery(
        &self,
        delivery: AiStreamDelivery,
    ) -> bool {
        // External runtime entities use the same bounded, generation-guarded
        // delivery path without placing their ownership inside this entity.
        self.chat_stream_tx.send(delivery).is_ok()
    }

    pub(in crate::workspace) fn set_chat_stream_task(
        &mut self,
        generation: u64,
        task: tokio::task::JoinHandle<()>,
    ) {
        if generation == self.chat_stream_generation {
            if let Some(replaced_task) = self.chat_stream_task.replace(task) {
                replaced_task.abort();
            }
        } else {
            task.abort();
        }
    }

    pub(in crate::workspace) fn cancel_chat_stream(&mut self) -> u64 {
        self.reject_all_tool_interactions();
        if let Some(task) = self.chat_stream_task.take() {
            task.abort();
        }
        self.chat_stream_generation = self.chat_stream_generation.saturating_add(1);
        self.chat_stream_generation
    }

    pub(in crate::workspace) fn complete_chat_stream(&mut self, generation: u64) -> bool {
        if generation != self.chat_stream_generation {
            return false;
        }
        self.reject_all_tool_interactions();
        // Invalidate any delivery queued after the terminal event, matching the
        // old one-shot receiver lifetime without keeping a receiver on the root.
        if let Some(task) = self.chat_stream_task.take() {
            task.abort();
        }
        self.chat_stream_generation = self.chat_stream_generation.saturating_add(1);
        true
    }

    pub(in crate::workspace) fn register_tool_approval(
        &mut self,
        generation: u64,
        tool_call_id: String,
        sender: tokio::sync::oneshot::Sender<bool>,
    ) -> bool {
        if generation != self.chat_stream_generation {
            let _ = sender.send(false);
            return false;
        }
        if let Some(stale_sender) = self.pending_tool_approvals.insert(tool_call_id, sender) {
            // A repeated protocol id supersedes the older waiter without
            // allowing two workers to consume one user decision.
            let _ = stale_sender.send(false);
        }
        true
    }

    pub(in crate::workspace) fn resolve_tool_approval(
        &mut self,
        tool_call_id: &str,
        approved: bool,
    ) -> bool {
        let Some(sender) = self.pending_tool_approvals.remove(tool_call_id) else {
            return false;
        };
        let _ = sender.send(approved);
        true
    }

    pub(in crate::workspace) fn register_acp_permission_choice(
        &mut self,
        generation: u64,
        tool_call_id: String,
        sender: tokio::sync::oneshot::Sender<Option<String>>,
    ) -> bool {
        if generation != self.chat_stream_generation {
            let _ = sender.send(None);
            return false;
        }
        if let Some(stale_sender) = self
            .pending_acp_permission_choices
            .insert(tool_call_id, sender)
        {
            let _ = stale_sender.send(None);
        }
        true
    }

    pub(in crate::workspace) fn resolve_acp_permission_choice(
        &mut self,
        tool_call_id: &str,
        option_id: Option<String>,
    ) -> bool {
        let Some(sender) = self.pending_acp_permission_choices.remove(tool_call_id) else {
            return false;
        };
        let _ = sender.send(option_id);
        true
    }

    fn reject_all_tool_approvals(&mut self) {
        for (_, sender) in self.pending_tool_approvals.drain() {
            let _ = sender.send(false);
        }
        for (_, sender) in self.pending_acp_permission_choices.drain() {
            let _ = sender.send(None);
        }
    }

    pub(in crate::workspace) fn register_tool_candidate_selection(
        &mut self,
        generation: u64,
        tool_call_id: String,
        candidate_count: usize,
        sender: tokio::sync::oneshot::Sender<Option<usize>>,
    ) -> bool {
        if generation != self.chat_stream_generation || candidate_count == 0 {
            let _ = sender.send(None);
            return false;
        }
        if let Some(stale_sender) = self
            .pending_tool_candidate_selections
            .insert(tool_call_id.clone(), sender)
        {
            // Repeated protocol IDs cancel the older selector before replacing it.
            let _ = stale_sender.send(None);
        }
        let input_was_focused = self.chat_ui.input_focused;
        let footer_focus = self.chat_ui.footer_focus;
        self.chat_ui.tool_candidate_selection = Some(AiToolCandidateSelectionState {
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

    pub(in crate::workspace) fn move_tool_candidate_selection(&mut self, delta: isize) -> bool {
        let Some(selection) = self.chat_ui.tool_candidate_selection.as_mut() else {
            return false;
        };
        selection.selected_index = selection
            .selected_index
            .saturating_add_signed(delta)
            .min(selection.candidate_count.saturating_sub(1));
        true
    }

    pub(in crate::workspace) fn resolve_tool_candidate_selection(
        &mut self,
        tool_call_id: &str,
        selected_index: Option<usize>,
    ) -> bool {
        let Some(sender) = self.pending_tool_candidate_selections.remove(tool_call_id) else {
            return false;
        };
        let selected_index = selected_index.filter(|index| {
            self.chat_ui
                .tool_candidate_selection
                .as_ref()
                .is_some_and(|selection| {
                    selection.tool_call_id == tool_call_id && *index < selection.candidate_count
                })
        });
        if let Some(selection) = self.chat_ui.tool_candidate_selection.take() {
            // Candidate selection temporarily owns keyboard routing. Restore the
            // exact composer focus state instead of always focusing the input.
            self.chat_ui.input_focused = selection.input_was_focused;
            self.chat_ui.footer_focus = selection.footer_focus;
        }
        let _ = sender.send(selected_index);
        true
    }

    fn reject_all_tool_candidate_selections(&mut self) {
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

    pub(in crate::workspace) fn agent_fs(&self) -> &NodeAgentIdeFileSystem {
        &self.agent_fs
    }

    pub(in crate::workspace) fn set_agent_fs_mode(
        &mut self,
        mode: oxideterm_ide_fs::NodeAgentMode,
    ) {
        self.agent_fs.set_mode(mode);
    }

    pub(in crate::workspace) fn mcp_registry(&self) -> &oxideterm_ai::McpRegistry {
        &self.mcp_registry
    }

    pub(in crate::workspace) fn take_chat_stream_deliveries(
        &mut self,
    ) -> VecDeque<AiStreamDelivery> {
        std::mem::take(&mut self.chat_stream_deliveries)
    }

    pub(in crate::workspace) fn compaction_sender(&self) -> AiCompactionDeliverySender {
        self.compaction_tx.clone()
    }

    pub(in crate::workspace) fn take_compaction_deliveries(
        &mut self,
    ) -> VecDeque<AiCompactionDelivery> {
        std::mem::take(&mut self.compaction_deliveries)
    }

    pub(in crate::workspace) fn begin_compaction(&mut self, conversation_id: &str) -> bool {
        self.compacting_conversations
            .insert(conversation_id.to_string())
    }

    pub(in crate::workspace) fn finish_compaction(&mut self, conversation_id: &str) {
        self.compacting_conversations.remove(conversation_id);
    }

    pub(in crate::workspace) fn compaction_notice(&self) -> Option<&AiCompactionNotice> {
        self.compaction_notice.as_ref()
    }

    pub(in crate::workspace) fn set_compaction_notice_running(
        &mut self,
        conversation_id: &str,
        cx: &mut Context<Self>,
    ) {
        self.compaction_notice = Some(AiCompactionNotice {
            conversation_id: conversation_id.to_string(),
            phase: AiCompactionNoticePhase::Running,
            compacted_count: None,
            timestamp_ms: ai_now_ms(),
        });
        cx.emit(AiWorkspaceEvent::CompactionStateChanged);
        cx.notify();
    }

    pub(in crate::workspace) fn set_compaction_notice_done(
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

    pub(in crate::workspace) fn clear_compaction_notice_for(
        &mut self,
        conversation_id: &str,
        cx: &mut Context<Self>,
    ) {
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
        let drain = crate::workspace::delivery::drain_channel(
            &self.model_refresh_rx,
            crate::workspace::delivery::USER_ACTION_DELIVERY_BUDGET,
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
                Err(AiModelRefreshFailure::Failed) => AiModelRefreshIntent::Failed,
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
        let drain = crate::workspace::delivery::drain_channel(
            &self.provider_key_status_rx,
            crate::workspace::delivery::USER_ACTION_DELIVERY_BUDGET,
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
        let drain = crate::workspace::delivery::drain_channel(
            &self.selector_probe_rx,
            crate::workspace::delivery::USER_ACTION_DELIVERY_BUDGET,
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
        let drain = crate::workspace::delivery::drain_channel(
            &self.acp_agent_probe_rx,
            crate::workspace::delivery::USER_ACTION_DELIVERY_BUDGET,
        );
        for delivery in drain.items {
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
        let drain = crate::workspace::delivery::drain_channel(
            &self.acp_model_discovery_rx,
            crate::workspace::delivery::USER_ACTION_DELIVERY_BUDGET,
        );
        for delivery in drain.items {
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

    fn schedule_knowledge_reindex_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.knowledge_reindex_tx.wake();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // Releasing workspace AI state stops only its UI waiter; the
            // blocking storage task retains its own cancellation lifetime.
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
                        .update(cx, |entity, cx| entity.drain_knowledge_reindex_results(cx))
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

    fn drain_knowledge_reindex_results(&mut self, cx: &mut Context<Self>) -> bool {
        let drain = crate::workspace::delivery::drain_channel(
            &self.knowledge_reindex_rx,
            crate::workspace::delivery::LIFECYCLE_DELIVERY_BUDGET,
        );
        let mut changed = false;
        for delivery in drain.items {
            match delivery {
                AiKnowledgeReindexDelivery::Progress { current, total } => {
                    if self.knowledge_reindex_progress.is_some() {
                        self.knowledge_reindex_progress = Some((current, total));
                        changed = true;
                    }
                }
                AiKnowledgeReindexDelivery::Finished { failed } => {
                    if self.knowledge_reindex_progress.take().is_some() {
                        self.knowledge_reindex_cancel = None;
                        self.knowledge_reindex_intents
                            .push_back(AiKnowledgeReindexIntent::Finished { failed });
                        changed = true;
                    }
                }
            }
        }
        if changed {
            cx.emit(AiWorkspaceEvent::KnowledgeReindexDeliveryReady);
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
        let drain = crate::workspace::delivery::drain_channel(
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
            cx.emit(AiWorkspaceEvent::TerminalInlineDeliveryReady);
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
        let drain = crate::workspace::delivery::drain_channel(
            &self.chat_stream_rx,
            AI_CHAT_STREAM_DELIVERY_BUDGET,
        );
        if !drain.items.is_empty() {
            self.chat_stream_deliveries.extend(drain.items);
            cx.emit(AiWorkspaceEvent::ChatStreamDeliveryReady);
            cx.notify();
        }
        drain.outcome.backlog_remaining
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
        let drain = crate::workspace::delivery::drain_channel(
            &self.compaction_rx,
            crate::workspace::delivery::USER_ACTION_DELIVERY_BUDGET,
        );
        if !drain.items.is_empty() {
            self.compaction_deliveries.extend(drain.items);
            cx.emit(AiWorkspaceEvent::CompactionDeliveryReady);
            cx.notify();
        }
        drain.outcome.backlog_remaining
    }
}

fn mcp_draft_input_value_mut(
    draft: &mut AiMcpServerDraft,
    input: SettingsInput,
) -> Option<&mut String> {
    match input {
        SettingsInput::AiMcpName => Some(&mut draft.name),
        SettingsInput::AiMcpCommand => Some(&mut draft.command),
        SettingsInput::AiMcpArgs => Some(&mut draft.args),
        SettingsInput::AiMcpUrl => Some(&mut draft.url),
        SettingsInput::AiMcpAuthHeaderName => Some(&mut draft.auth_header_name),
        SettingsInput::AiMcpAuthToken => Some(&mut draft.auth_token),
        SettingsInput::AiMcpEnvKey(index) => draft.env.get_mut(index).map(|(key, _)| key),
        SettingsInput::AiMcpEnvValue(index) => draft.env.get_mut(index).map(|(_, value)| value),
        SettingsInput::AiMcpHeaderKey(index) => draft.headers.get_mut(index).map(|(key, _)| key),
        SettingsInput::AiMcpHeaderValue(index) => {
            draft.headers.get_mut(index).map(|(_, value)| value)
        }
        _ => None,
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
        if let Some(task) = self.chat_stream_task.take() {
            task.abort();
        }
        self.abort_terminal_inline_stream();
        // Runtime-backed reindex work is not a GPUI task, so entity release
        // must explicitly stop it as well as dropping the retained UI tasks.
        if let Some(cancel) = self.knowledge_reindex_cancel.take() {
            cancel.store(true, Ordering::Release);
        }
    }
}

impl gpui::EventEmitter<AiWorkspaceEvent> for AiWorkspaceEntity {}

/// Identifies the AI confirmation whose retained payload may finish exiting.
#[derive(Clone, Copy)]
pub(super) enum AiStandardConfirmKind {
    Safety,
    Summarize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AiToolCandidateSelectionState {
    pub(super) tool_call_id: String,
    pub(super) selected_index: usize,
    pub(super) candidate_count: usize,
    input_was_focused: bool,
    footer_focus: Option<AiChatFooterAction>,
}

/// Owns AI chat presentation, conversation persistence, streaming, and compaction state.
pub(super) struct AiChatWorkspaceState {
    pub(super) sidebar_resizing: bool,
    pub(super) sidebar_width: f32,
    pub(super) overlay_window_size: Option<(f32, f32)>,
    pub(super) overlay_window_bounds_subscription: Option<Subscription>,
    pub(super) message_list_state: ListState,
    pub(super) message_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) message_signature_cache: RefCell<AiChatMessageSignatureCache>,
    pub(super) markdown_cache: RefCell<AiMarkdownDocumentCache>,
    pub(super) context_token_cache: RefCell<AiContextTokenBreakdownCache>,
    pub(super) prepared_prompt_usage: Option<AiPreparedPromptUsage>,
    pub(super) conversation_list_open: bool,
    pub(super) menu_open: bool,
    pub(super) reasoning_menu_open: bool,
    pub(super) safety_menu_open: bool,
    pub(super) safety_confirm_open: bool,
    pub(super) safety_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) summarize_confirm_open: bool,
    pub(super) summarize_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) draft: String,
    pub(super) input_focused: bool,
    pub(super) footer_focus: Option<AiChatFooterAction>,
    pub(super) renaming_conversation_id: Option<String>,
    pub(super) renaming_conversation_draft: String,
    pub(super) renaming_conversation_focused: bool,
    pub(super) editing_message_id: Option<String>,
    pub(super) editing_message_draft: String,
    pub(super) editing_message_focused: bool,
    pub(super) thinking_expansion_state: HashMap<String, bool>,
    pub(super) tool_call_expansion_state: HashSet<String>,
    pub(super) tool_candidate_selection: Option<AiToolCandidateSelectionState>,
    pub(super) autocomplete_index: usize,
    pub(super) autocomplete_suppressed: bool,
    pub(super) context_popover_open: bool,
    pub(super) model_switch_warning_percentage: Option<usize>,
    pub(super) context_trim_notice_count: Option<usize>,
    pub(super) context_trim_notice_sequence: u64,
    pub(super) include_context: bool,
    pub(super) include_all_panes: bool,
}

/// Owns provider and model presentation state inside the AI Entity.
pub(in crate::workspace) struct AiModelWorkspaceState {
    pub(super) context_model_list_states: RefCell<HashMap<String, ListState>>,
    pub(super) context_model_list_caches: RefCell<HashMap<String, VirtualListSignatureCache>>,
    pub(super) provider_model_chip_list_states: RefCell<HashMap<String, ListState>>,
    pub(super) provider_model_chip_list_caches: RefCell<HashMap<String, VirtualListSignatureCache>>,
    pub(super) provider_card_list_state: ListState,
    pub(super) provider_card_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) mcp_server_list_state: ListState,
    pub(super) mcp_server_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) selector_open: bool,
    pub(super) selector_scope: Option<AiModelSelectorScope>,
    pub(super) selector_focus_origin: Option<browser_behavior::BrowserFocusOrigin>,
    pub(super) selector_search_focused: bool,
    pub(super) selector_search_query: String,
    pub(super) selector_expanded_providers: HashSet<String>,
    pub(super) selector_highlighted_model: Option<(String, String)>,
    pub(super) selector_status_signature: Option<u64>,
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
            markdown_cache: RefCell::new(AiMarkdownDocumentCache::default()),
            context_token_cache: RefCell::new(AiContextTokenBreakdownCache::default()),
            prepared_prompt_usage: None,
            conversation_list_open: false,
            menu_open: false,
            reasoning_menu_open: false,
            safety_menu_open: false,
            safety_confirm_open: false,
            safety_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            summarize_confirm_open: false,
            summarize_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            draft: String::new(),
            input_focused: false,
            footer_focus: None,
            renaming_conversation_id: None,
            renaming_conversation_draft: String::new(),
            renaming_conversation_focused: false,
            editing_message_id: None,
            editing_message_draft: String::new(),
            editing_message_focused: false,
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
            context_model_list_states: RefCell::new(HashMap::new()),
            context_model_list_caches: RefCell::new(HashMap::new()),
            provider_model_chip_list_states: RefCell::new(HashMap::new()),
            provider_model_chip_list_caches: RefCell::new(HashMap::new()),
            provider_card_list_state: ListState::new(
                AI_PROVIDER_CARD_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(AI_PROVIDER_CARD_LIST_ESTIMATED_HEIGHT),
                    AI_PROVIDER_CARD_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            provider_card_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            mcp_server_list_state: ListState::new(
                AI_MCP_SERVER_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(AI_MCP_SERVER_LIST_ESTIMATED_HEIGHT),
                    AI_MCP_SERVER_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            mcp_server_list_cache: RefCell::new(VirtualListSignatureCache::default()),
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
    use std::sync::atomic::AtomicUsize;

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

    fn test_acp_agent(agent_id: &str) -> oxideterm_settings::AcpAgentConfig {
        oxideterm_settings::AcpAgentConfig {
            id: agent_id.to_string(),
            display_name: "Test Agent".to_string(),
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
    fn chat_confirmation_reopen_cancels_stale_exit(cx: &mut TestAppContext) {
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
    }

    #[gpui::test]
    fn chat_confirmation_keys_publish_each_effect_at_most_once(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
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
    fn entity_release_cancels_retained_chat_confirmation_exit(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let (release_sender, release_receiver) = tokio::sync::oneshot::channel();
        entity.update(cx, |entity, cx| {
            // The AI owner retains its modal exit task so Entity release
            // cancels a pending animation completion.
            entity.chat_confirm_exit_task = Some(cx.spawn(async move |_, _| {
                let _ = release_receiver.await;
            }));
        });
        cx.run_until_parked();

        drop(entity);
        cx.update(|_| {});
        cx.run_until_parked();

        assert!(release_sender.send(()).is_err());
    }

    #[gpui::test]
    fn provider_secret_draft_and_operation_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let input = SettingsInput::AiProviderApiKey(3);
        let draft_allocation = entity.update(cx, |entity, cx| {
            assert!(entity.focus_settings_input(input, cx));
            assert!(entity.replace_settings_input(input, None, "test-secret", cx));
            entity
                .settings_input_value(input)
                .expect("provider draft")
                .as_ptr()
        });
        let secret = entity
            .update(cx, |entity, _cx| entity.take_provider_key_secret(input))
            .expect("moved provider secret");
        assert_eq!(secret.as_ptr(), draft_allocation);
        cx.read(|cx| {
            let entity = entity.read(cx);
            assert_eq!(entity.focused_settings_input(), None);
            assert!(entity.settings_input_value(input).is_none());
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
    }

    #[gpui::test]
    fn provider_operation_failure_exposes_only_typed_category(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
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
    }

    #[gpui::test]
    fn provider_removal_is_queued_behind_an_inflight_store(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
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
    fn mcp_dialog_inputs_exit_and_submission_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        entity.update(cx, |entity, cx| {
            assert!(entity.open_mcp_add_dialog(cx));
            assert!(entity.focus_settings_input(SettingsInput::AiMcpName, cx));
            assert!(entity.replace_settings_input(
                SettingsInput::AiMcpName,
                None,
                "server-test",
                cx,
            ));
            assert!(entity.begin_mcp_dialog_exit(
                false,
                Duration::from_millis(10),
                HashSet::new(),
                cx,
            ));
            assert!(entity.mcp_dialog_exit_task.is_some());

            // Reopening invalidates the retained exit task and zeroizes the
            // superseded draft before installing a fresh owner.
            assert!(entity.open_mcp_add_dialog(cx));
            assert!(entity.mcp_dialog_exit_task.is_none());
            assert_eq!(
                entity.settings_input_value(SettingsInput::AiMcpName),
                Some("")
            );
            assert!(entity.focus_settings_input(SettingsInput::AiMcpName, cx));
            assert!(entity.replace_settings_input(
                SettingsInput::AiMcpName,
                None,
                "server-ready",
                cx,
            ));
            assert!(entity.begin_mcp_dialog_exit(true, Duration::ZERO, HashSet::new(), cx,));
            assert!(entity.mcp_add_dialog.is_none());
            let intent = entity
                .take_credential_intents()
                .pop_front()
                .expect("MCP config completion intent");
            assert!(matches!(
                intent,
                AiCredentialIntent::McpServerReady { config }
                    if config.get("name").and_then(serde_json::Value::as_str)
                        == Some("server-ready")
            ));
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
            assert!(
                entity
                    .mcp_add_dialog
                    .as_ref()
                    .is_some_and(|draft| draft.auth_token.is_empty())
            );
            let intent = entity
                .take_credential_intents()
                .pop_front()
                .expect("MCP failure intent");
            assert!(matches!(
                intent,
                AiCredentialIntent::Failed(AiCredentialFailure::SaveMcpToken)
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
    fn ai_settings_confirm_payload_exit_and_intent_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        entity.update(cx, |entity, cx| {
            entity.open_provider_remove_confirm(
                "provider-test".to_string(),
                "Provider Test".to_string(),
                cx,
            );
            assert_eq!(
                entity.settings_confirm_provider_name(),
                Some("Provider Test")
            );
            assert!(entity.begin_settings_confirm_exit(true, Duration::from_millis(10), cx,));
            assert!(entity.settings_confirm_exit_task.is_some());

            // A newer confirmation cancels the retained exit generation.
            entity.open_ai_enable_confirm(cx);
            assert!(entity.settings_confirm_exit_task.is_none());
            assert!(entity.settings_confirm_is_enable());
            assert!(entity.begin_settings_confirm_exit(true, Duration::ZERO, cx));
            assert!(!entity.settings_confirm_is_open());
            let intent = entity
                .take_settings_confirm_intents()
                .pop_front()
                .expect("AI settings confirmation intent");
            assert!(matches!(intent, AiSettingsConfirmIntent::Enable));

            entity.open_provider_key_remove_confirm(4, "provider-key".to_string(), cx);
            assert!(entity.begin_settings_confirm_exit(false, Duration::ZERO, cx));
            assert!(entity.take_settings_confirm_intents().is_empty());
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
    fn knowledge_reindex_progress_cancel_and_completion_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_tx = entity.update(cx, |entity, _cx| {
            entity.knowledge_reindex_progress = Some((0, 0));
            entity.knowledge_reindex_cancel = Some(cancel.clone());
            entity.knowledge_reindex_tx.clone()
        });
        assert!(entity.read_with(cx, |entity, _cx| entity.cancel_knowledge_reindex()));
        assert!(cancel.load(Ordering::Relaxed));

        worker_tx
            .send(AiKnowledgeReindexDelivery::Progress {
                current: 10,
                total: 25,
            })
            .unwrap();
        cx.run_until_parked();
        assert_eq!(
            entity.read_with(cx, |entity, _cx| entity.knowledge_reindex_progress()),
            Some((10, 25))
        );

        worker_tx
            .send(AiKnowledgeReindexDelivery::Finished { failed: true })
            .unwrap();
        cx.run_until_parked();
        let intents = entity.update(cx, |entity, _cx| entity.take_knowledge_reindex_intents());
        assert!(matches!(
            intents.front(),
            Some(AiKnowledgeReindexIntent::Finished { failed: true })
        ));
        assert_eq!(
            entity.read_with(cx, |entity, _cx| entity.knowledge_reindex_progress()),
            None
        );
    }

    #[gpui::test]
    fn hidden_knowledge_page_still_receives_import_completion(cx: &mut TestAppContext) {
        let data_dir = std::env::temp_dir().join(format!(
            "oxideterm-knowledge-entity-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&data_dir).expect("create Knowledge test directory");
        let document_path = data_dir.join("guide.md");
        std::fs::write(&document_path, "# Guide\nEntity-owned import")
            .expect("write Knowledge test document");
        let entity = cx.new(|cx| {
            let mut entity =
                AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx);
            entity.rag_store = LazyAiRagStore::new(data_dir.clone());
            entity
        });
        let store = entity.read_with(cx, |entity, _cx| entity.rag_store());
        let collection = oxideterm_ai::rag_create_collection(
            &store,
            oxideterm_ai::RagCreateCollectionRequest {
                name: "Hidden page".to_string(),
                scope: oxideterm_ai::RagDocScopeRequest::Global,
            },
        )
        .expect("create Knowledge test collection");

        entity.update(cx, |entity, cx| {
            entity.set_workspace_visibility(AiWorkspaceVisibility {
                model_selector_surface: false,
                settings_surface: true,
            });
            assert!(entity.start_knowledge_import(
                std::future::ready(Some(vec![document_path])),
                collection.id.clone(),
                "safe import failure".to_string(),
                cx,
            ));
            assert!(entity.knowledge_import_task.is_some());
            entity.set_workspace_visibility(AiWorkspaceVisibility::default());
        });
        // No settings page or workspace callback is present while the task
        // finishes; the entity must still apply the completion reliably.
        cx.run_until_parked();

        entity.read_with(cx, |entity, _cx| {
            assert!(entity.knowledge_import_task.is_none());
            assert_eq!(entity.knowledge_import_progress(), None);
            assert_eq!(entity.knowledge_error(), None);
        });
        let documents = oxideterm_ai::rag_list_documents(&store, &collection.id, None, Some(10))
            .expect("list imported Knowledge documents");
        assert_eq!(documents.documents.len(), 1);
        drop(entity);
        drop(store);
        std::fs::remove_dir_all(&data_dir).expect("remove Knowledge test directory");
    }

    #[gpui::test]
    fn entity_release_cancels_knowledge_tasks_and_reindex(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let cancel = Arc::new(AtomicBool::new(false));
        let (paths_sender, paths_receiver) = tokio::sync::oneshot::channel();
        entity.update(cx, |entity, cx| {
            entity.knowledge_reindex_cancel = Some(cancel.clone());
            assert!(entity.start_knowledge_import(
                async move { paths_receiver.await.ok().flatten() },
                "collection".to_string(),
                "safe import failure".to_string(),
                cx,
            ));
        });

        drop(entity);
        cx.update(|_cx| {});
        cx.run_until_parked();

        assert!(cancel.load(Ordering::Acquire));
        assert!(
            paths_sender.send(Some(Vec::new())).is_err(),
            "dropping the entity must cancel its retained import future"
        );
    }

    #[gpui::test]
    fn stream_task_replacement_and_cancel_abort_each_task_exactly_once(cx: &mut TestAppContext) {
        let runtime = test_worker_runtime();
        let entity = cx.new({
            let runtime = Arc::clone(&runtime);
            move |cx| AiWorkspaceEntity::new(runtime, oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let chat_drop_count = Arc::new(AtomicUsize::new(0));
        let terminal_drop_count = Arc::new(AtomicUsize::new(0));

        let chat_generation = entity.update(cx, |entity, _cx| entity.begin_chat_stream().0);
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
            entity.cancel_chat_stream();
            entity.close_terminal_inline_panel();
            // Repeated cancellation cannot abort an already-consumed handle.
            entity.cancel_chat_stream();
            entity.close_terminal_inline_panel();
        });
        wait_for_lifecycle_count(&chat_drop_count, 2);
        wait_for_lifecycle_count(&terminal_drop_count, 2);
    }

    #[gpui::test]
    fn entity_release_aborts_chat_and_terminal_stream_tasks_exactly_once(cx: &mut TestAppContext) {
        let runtime = test_worker_runtime();
        let entity = cx.new({
            let runtime = Arc::clone(&runtime);
            move |cx| AiWorkspaceEntity::new(runtime, oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let chat_drop_count = Arc::new(AtomicUsize::new(0));
        let terminal_drop_count = Arc::new(AtomicUsize::new(0));
        let chat_generation = entity.update(cx, |entity, _cx| entity.begin_chat_stream().0);
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

        wait_for_lifecycle_count(&chat_drop_count, 1);
        wait_for_lifecycle_count(&terminal_drop_count, 1);
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(chat_drop_count.load(Ordering::Acquire), 1);
        assert_eq!(terminal_drop_count.load(Ordering::Acquire), 1);
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
    fn chat_stream_generation_task_boundary_and_delivery_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let (generation, worker_tx) = entity.update(cx, |entity, _cx| {
            entity.set_workspace_visibility(AiWorkspaceVisibility {
                model_selector_surface: true,
                settings_surface: false,
            });
            let stream = entity.begin_chat_stream();
            entity.set_workspace_visibility(AiWorkspaceVisibility::default());
            stream
        });
        worker_tx
            .send(AiStreamDelivery {
                generation,
                conversation_id: "conversation-a".to_string(),
                assistant_id: "assistant-a".to_string(),
                event: AiStreamDeliveryEvent::Stream(oxideterm_ai::AiStreamEvent::Content(
                    "chunk".to_string(),
                )),
            })
            .unwrap();

        cx.run_until_parked();

        let deliveries = entity.update(cx, |entity, _cx| entity.take_chat_stream_deliveries());
        assert_eq!(deliveries.len(), 1);
        assert!(matches!(
            &deliveries.front().expect("chat delivery").event,
            AiStreamDeliveryEvent::Stream(oxideterm_ai::AiStreamEvent::Content(chunk))
                if chunk == "chunk"
        ));
        entity.update(cx, |entity, _cx| {
            assert!(entity.complete_chat_stream(generation));
            assert!(!entity.is_chat_stream_generation(generation));
            assert!(!entity.complete_chat_stream(generation));
        });
    }

    #[gpui::test]
    fn tool_approval_generation_and_lifecycle_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let (generation, _) = entity.update(cx, |entity, _cx| entity.begin_chat_stream());
        let (first_sender, mut first_receiver) = tokio::sync::oneshot::channel();
        let (replacement_sender, mut replacement_receiver) = tokio::sync::oneshot::channel();
        entity.update(cx, |entity, _cx| {
            assert!(entity.register_tool_approval(generation, "tool-a".to_string(), first_sender,));
            assert!(entity.register_tool_approval(
                generation,
                "tool-a".to_string(),
                replacement_sender,
            ));
            assert!(entity.resolve_tool_approval("tool-a", true));
            assert!(!entity.resolve_tool_approval("tool-a", false));
        });
        assert_eq!(first_receiver.try_recv(), Ok(false));
        assert_eq!(replacement_receiver.try_recv(), Ok(true));

        let (stale_sender, mut stale_receiver) = tokio::sync::oneshot::channel();
        entity.update(cx, |entity, _cx| {
            entity.cancel_chat_stream();
            assert!(!entity.register_tool_approval(
                generation,
                "stale-tool".to_string(),
                stale_sender,
            ));
        });
        assert_eq!(stale_receiver.try_recv(), Ok(false));

        let (release_sender, mut release_receiver) = tokio::sync::oneshot::channel();
        let current_generation = entity.update(cx, |entity, _cx| {
            let (current_generation, _) = entity.begin_chat_stream();
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
    fn tool_candidate_selection_is_keyboard_ordered_and_cancelled_with_stream(
        cx: &mut TestAppContext,
    ) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let (generation, _) = entity.update(cx, |entity, _cx| entity.begin_chat_stream());
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
            assert!(entity.resolve_tool_candidate_selection("tool-choice", Some(2)));
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
            entity.cancel_chat_stream();
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
            knowledge_reindex_wake,
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
                entity.knowledge_reindex_tx.wake(),
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
        assert!(knowledge_reindex_wake.is_stopped());
        assert!(terminal_inline_wake.is_stopped());
        assert!(chat_stream_wake.is_stopped());
        assert!(compaction_wake.is_stopped());
    }
}
