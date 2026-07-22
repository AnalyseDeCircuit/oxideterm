use super::*;

/// Owns all AI-related workspace state while preserving the existing feature boundaries.
pub(super) struct AiWorkspaceState {
    pub(super) chat: AiChatWorkspaceState,
    pub(super) runtime: AiRuntimeWorkspaceState,
    pub(super) models: AiModelWorkspaceState,
    pub(super) knowledge: AiKnowledgeWorkspaceState,
}

/// Identifies the AI confirmation whose retained payload may finish exiting.
#[derive(Clone, Copy)]
pub(super) enum AiStandardConfirmKind {
    Safety,
    Summarize,
}

/// Owns AI chat presentation, conversation persistence, streaming, and compaction state.
pub(super) struct AiChatWorkspaceState {
    pub(super) sidebar_resizing: bool,
    pub(super) sidebar_width: f32,
    pub(super) overlay_window_size: Option<(f32, f32)>,
    pub(super) overlay_window_bounds_subscription: Option<Subscription>,
    pub(super) conversation_state: oxideterm_ai::AiChatState,
    pub(super) message_list_state: ListState,
    pub(super) message_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) markdown_cache: RefCell<AiMarkdownDocumentCache>,
    pub(super) context_token_cache: RefCell<AiContextTokenBreakdownCache>,
    pub(super) persistence_store: Option<oxideterm_ai::AiChatPersistenceStore>,
    pub(super) initialized: bool,
    pub(super) initialization_error: Option<AiChatInitializationError>,
    pub(super) inline_panel: AiInlinePanelState,
    pub(super) conversation_list_open: bool,
    pub(super) menu_open: bool,
    pub(super) safety_menu_open: bool,
    pub(super) safety_confirm_open: bool,
    pub(super) safety_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) summarize_confirm_open: bool,
    pub(super) summarize_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) clear_all_confirm_open: bool,
    pub(super) delete_message_confirm: Option<String>,
    pub(super) safety_bypass_conversations: HashSet<String>,
    pub(super) draft: String,
    pub(super) input_focused: bool,
    pub(super) footer_focus: Option<AiChatFooterAction>,
    pub(super) editing_message_id: Option<String>,
    pub(super) editing_message_draft: String,
    pub(super) editing_message_focused: bool,
    pub(super) thinking_expansion_state: HashMap<String, bool>,
    pub(super) tool_call_expansion_state: HashSet<String>,
    pub(super) autocomplete_index: usize,
    pub(super) autocomplete_suppressed: bool,
    pub(super) context_popover_open: bool,
    pub(super) model_switch_warning_percentage: Option<usize>,
    pub(super) context_trim_notice_count: Option<usize>,
    pub(super) context_trim_notice_sequence: u64,
    pub(super) include_context: bool,
    pub(super) include_all_panes: bool,
    pub(super) loading: bool,
    pub(super) stream_generation: u64,
    pub(super) stream_task: Option<tokio::task::JoinHandle<()>>,
    pub(super) stream_rx: Option<std::sync::mpsc::Receiver<AiStreamDelivery>>,
    pub(super) stream_polling: bool,
    pub(super) compaction_rx: Option<std::sync::mpsc::Receiver<AiCompactionDelivery>>,
    pub(super) compaction_polling: bool,
    pub(super) compacting_conversations: HashSet<String>,
    pub(super) compaction_notice: Option<AiCompactionNotice>,
    pub(super) pending_after_compaction: Option<AiPendingChatStream>,
    pub(super) next_sequence: u64,
}

/// Owns AI execution registries, agent integration, tool approvals, and runtime records.
pub(super) struct AiRuntimeWorkspaceState {
    pub(super) epoch: String,
    pub(super) command_record_sequence: u64,
    pub(super) command_records: VecDeque<AiRuntimeCommandRecord>,
    pub(super) tool_execution_records: VecDeque<AiToolExecutionRecord>,
    pub(super) tool_result_facts: VecDeque<AiToolResultFact>,
    pub(super) cli_agent_sessions: HashMap<String, AiCliAgentSession>,
    pub(super) pending_tool_approvals: HashMap<String, tokio::sync::oneshot::Sender<bool>>,
    pub(super) agent_fs: NodeAgentIdeFileSystem,
    pub(super) mcp_registry: oxideterm_ai::McpRegistry,
    pub(super) acp_runtime_registry: oxideterm_ai::AcpRuntimeRegistry,
    pub(super) acp_agent_probe_pending: HashSet<String>,
    pub(super) acp_agent_probe_tx: Option<std::sync::mpsc::Sender<AcpAgentProbeDelivery>>,
    pub(super) acp_agent_probe_rx: Option<std::sync::mpsc::Receiver<AcpAgentProbeDelivery>>,
    pub(super) acp_agent_probe_polling: bool,
}

/// Owns provider/model settings, selectors, key status, and model refresh state.
pub(super) struct AiModelWorkspaceState {
    pub(super) context_model_list_states: RefCell<HashMap<String, ListState>>,
    pub(super) context_model_list_caches: RefCell<HashMap<String, VirtualListSignatureCache>>,
    pub(super) reasoning_model_list_states: RefCell<HashMap<String, ListState>>,
    pub(super) reasoning_model_list_caches: RefCell<HashMap<String, VirtualListSignatureCache>>,
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
    pub(super) selector_provider_online: HashMap<String, bool>,
    pub(super) selector_probe_generations: HashMap<String, u64>,
    pub(super) selector_status_signature: u64,
    pub(super) acp_model_options:
        HashMap<(String, String), Vec<oxideterm_ai::AcpSessionConfigOption>>,
    pub(super) acp_model_discovery_pending: HashSet<(String, String)>,
    pub(super) acp_model_discovery_tx: Option<std::sync::mpsc::Sender<AcpModelDiscoveryDelivery>>,
    pub(super) acp_model_discovery_rx: Option<std::sync::mpsc::Receiver<AcpModelDiscoveryDelivery>>,
    pub(super) acp_model_discovery_polling: bool,
    pub(super) mcp_add_dialog: Option<AiMcpServerDraft>,
    pub(super) key_store: oxideterm_ai::AiProviderKeyStore,
    pub(super) provider_key_status: HashMap<String, bool>,
    pub(super) provider_key_status_pending: HashSet<String>,
    pub(super) provider_key_status_tx: Option<std::sync::mpsc::Sender<AiProviderKeyStatusDelivery>>,
    pub(super) provider_key_status_rx:
        Option<std::sync::mpsc::Receiver<AiProviderKeyStatusDelivery>>,
    pub(super) provider_key_status_polling: bool,
    pub(super) refresh_generations: HashMap<String, u64>,
    pub(super) refreshing: HashSet<String>,
    pub(super) refresh_tx: Option<std::sync::mpsc::Sender<AiModelRefreshDelivery>>,
    pub(super) refresh_rx: Option<std::sync::mpsc::Receiver<AiModelRefreshDelivery>>,
    pub(super) refresh_polling: bool,
    pub(super) refresh_pending: usize,
    pub(super) next_refresh_generation: u64,
    pub(super) next_selector_probe_generation: u64,
    pub(super) selector_probe_rx: Option<std::sync::mpsc::Receiver<AiModelSelectorProbeDelivery>>,
    pub(super) selector_probe_tx: Option<std::sync::mpsc::Sender<AiModelSelectorProbeDelivery>>,
    pub(super) selector_probe_polling: bool,
    pub(super) selector_probe_pending: usize,
}

/// Owns lazy RAG storage and knowledge reindex delivery state.
pub(super) struct AiKnowledgeWorkspaceState {
    pub(super) rag_store: LazyAiRagStore,
    pub(super) reindex_cancel: Option<Arc<AtomicBool>>,
    pub(super) reindex_rx: Option<std::sync::mpsc::Receiver<KnowledgeReindexDelivery>>,
    pub(super) reindex_polling: bool,
    pub(super) window_activation_subscription: Option<Subscription>,
}

impl AiWorkspaceState {
    pub(super) fn new(
        agent_fs: NodeAgentIdeFileSystem,
        sidebar_width: f32,
        overlay_window_size: Option<(f32, f32)>,
    ) -> Self {
        // The model state and MCP registry share the same zeroizing key-store cache;
        // no raw provider key is copied into workspace fields during extraction.
        let key_store = oxideterm_ai::AiProviderKeyStore::new();
        let mcp_registry = oxideterm_ai::McpRegistry::new(key_store.clone());

        Self {
            chat: AiChatWorkspaceState::new(sidebar_width, overlay_window_size),
            runtime: AiRuntimeWorkspaceState::new(agent_fs, mcp_registry),
            models: AiModelWorkspaceState::new(key_store),
            knowledge: AiKnowledgeWorkspaceState::new(),
        }
    }
}

impl AiChatWorkspaceState {
    fn new(sidebar_width: f32, overlay_window_size: Option<(f32, f32)>) -> Self {
        Self {
            sidebar_resizing: false,
            sidebar_width,
            overlay_window_size,
            overlay_window_bounds_subscription: None,
            conversation_state: oxideterm_ai::AiChatState::default(),
            message_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                ai_chat_virtual_list_spec(),
            ),
            message_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            markdown_cache: RefCell::new(AiMarkdownDocumentCache::default()),
            context_token_cache: RefCell::new(AiContextTokenBreakdownCache::default()),
            persistence_store: None,
            initialized: false,
            initialization_error: None,
            inline_panel: AiInlinePanelState::default(),
            conversation_list_open: false,
            menu_open: false,
            safety_menu_open: false,
            safety_confirm_open: false,
            safety_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            summarize_confirm_open: false,
            summarize_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            clear_all_confirm_open: false,
            delete_message_confirm: None,
            safety_bypass_conversations: HashSet::new(),
            draft: String::new(),
            input_focused: false,
            footer_focus: None,
            editing_message_id: None,
            editing_message_draft: String::new(),
            editing_message_focused: false,
            thinking_expansion_state: HashMap::new(),
            tool_call_expansion_state: HashSet::new(),
            autocomplete_index: 0,
            autocomplete_suppressed: false,
            context_popover_open: false,
            model_switch_warning_percentage: None,
            context_trim_notice_count: None,
            context_trim_notice_sequence: 0,
            include_context: false,
            include_all_panes: false,
            loading: false,
            stream_generation: 0,
            stream_task: None,
            stream_rx: None,
            stream_polling: false,
            compaction_rx: None,
            compaction_polling: false,
            compacting_conversations: HashSet::new(),
            compaction_notice: None,
            pending_after_compaction: None,
            next_sequence: 0,
        }
    }
}

impl AiRuntimeWorkspaceState {
    fn new(agent_fs: NodeAgentIdeFileSystem, mcp_registry: oxideterm_ai::McpRegistry) -> Self {
        Self {
            epoch: uuid::Uuid::new_v4().to_string(),
            command_record_sequence: 0,
            command_records: VecDeque::new(),
            tool_execution_records: VecDeque::new(),
            tool_result_facts: VecDeque::new(),
            cli_agent_sessions: HashMap::new(),
            pending_tool_approvals: HashMap::new(),
            agent_fs,
            mcp_registry,
            acp_runtime_registry: oxideterm_ai::AcpRuntimeRegistry::default(),
            acp_agent_probe_pending: HashSet::new(),
            acp_agent_probe_tx: None,
            acp_agent_probe_rx: None,
            acp_agent_probe_polling: false,
        }
    }
}

impl AiModelWorkspaceState {
    fn new(key_store: oxideterm_ai::AiProviderKeyStore) -> Self {
        Self {
            context_model_list_states: RefCell::new(HashMap::new()),
            context_model_list_caches: RefCell::new(HashMap::new()),
            reasoning_model_list_states: RefCell::new(HashMap::new()),
            reasoning_model_list_caches: RefCell::new(HashMap::new()),
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
            selector_provider_online: HashMap::new(),
            selector_probe_generations: HashMap::new(),
            selector_status_signature: 0,
            acp_model_options: HashMap::new(),
            acp_model_discovery_pending: HashSet::new(),
            acp_model_discovery_tx: None,
            acp_model_discovery_rx: None,
            acp_model_discovery_polling: false,
            mcp_add_dialog: None,
            key_store,
            provider_key_status: HashMap::new(),
            provider_key_status_pending: HashSet::new(),
            provider_key_status_tx: None,
            provider_key_status_rx: None,
            provider_key_status_polling: false,
            refresh_generations: HashMap::new(),
            refreshing: HashSet::new(),
            refresh_tx: None,
            refresh_rx: None,
            refresh_polling: false,
            refresh_pending: 0,
            next_refresh_generation: 0,
            next_selector_probe_generation: 0,
            selector_probe_rx: None,
            selector_probe_tx: None,
            selector_probe_polling: false,
            selector_probe_pending: 0,
        }
    }
}

impl AiKnowledgeWorkspaceState {
    fn new() -> Self {
        Self {
            rag_store: LazyAiRagStore::default(),
            reindex_cancel: None,
            reindex_rx: None,
            reindex_polling: false,
            window_activation_subscription: None,
        }
    }
}
