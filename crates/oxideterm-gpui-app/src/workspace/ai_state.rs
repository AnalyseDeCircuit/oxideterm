use super::*;

pub(in crate::workspace) enum AiWorkspaceEvent {
    ModelRefreshDeliveryReady,
    ProviderKeyStatusChanged,
    SelectorProviderStatusChanged,
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

/// Owns AI worker delivery slices as they move out of the workspace root.
pub(in crate::workspace) struct AiWorkspaceEntity {
    task_runtime: Arc<tokio::runtime::Runtime>,
    key_store: oxideterm_ai::AiProviderKeyStore,
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
}

impl AiWorkspaceEntity {
    pub(in crate::workspace) fn new(
        task_runtime: Arc<tokio::runtime::Runtime>,
        key_store: oxideterm_ai::AiProviderKeyStore,
        cx: &mut Context<Self>,
    ) -> Self {
        let (model_refresh_tx, model_refresh_rx) =
            crate::workspace::delivery::ActiveDeliverySender::channel();
        let (provider_key_status_tx, provider_key_status_rx) =
            crate::workspace::delivery::ActiveDeliverySender::channel();
        let (selector_probe_tx, selector_probe_rx) =
            crate::workspace::delivery::ActiveDeliverySender::channel();
        let entity = Self {
            task_runtime,
            key_store,
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
        };
        entity.schedule_model_refresh_delivery(cx);
        entity.schedule_provider_key_status_delivery(cx);
        entity.schedule_selector_probe_delivery(cx);
        entity
    }

    pub(in crate::workspace) fn model_is_refreshing(&self, provider_id: &str) -> bool {
        self.refreshing_models.contains(provider_id)
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

    pub(in crate::workspace) fn invalidate_provider_key_status(&mut self, provider_id: &str) {
        self.provider_key_status.remove(provider_id);
        self.provider_key_status_pending.remove(provider_id);
    }

    pub(in crate::workspace) fn request_provider_key_statuses(
        &mut self,
        provider_ids: impl IntoIterator<Item = String>,
    ) {
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
    ) {
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
}

impl gpui::EventEmitter<AiWorkspaceEvent> for AiWorkspaceEntity {}

/// Owns all AI-related workspace state while preserving the existing feature boundaries.
pub(super) struct AiWorkspaceState {
    pub(super) delivery_wake: crate::workspace::delivery::ActiveDeliveryWake,
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
    pub(super) reasoning_menu_open: bool,
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
    pub(super) compaction_rx: Option<std::sync::mpsc::Receiver<AiCompactionDelivery>>,
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
    pub(super) acp_agent_probe_tx:
        Option<crate::workspace::delivery::ActiveDeliverySender<AcpAgentProbeDelivery>>,
    pub(super) acp_agent_probe_rx: Option<std::sync::mpsc::Receiver<AcpAgentProbeDelivery>>,
}

/// Owns provider/model settings and selector state not yet extracted into the AI Entity.
pub(super) struct AiModelWorkspaceState {
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
    pub(super) selector_status_signature: u64,
    pub(super) acp_model_options:
        HashMap<(String, String), Vec<oxideterm_ai::AcpSessionConfigOption>>,
    pub(super) acp_model_discovery_pending: HashSet<(String, String)>,
    pub(super) acp_model_discovery_tx:
        Option<crate::workspace::delivery::ActiveDeliverySender<AcpModelDiscoveryDelivery>>,
    pub(super) acp_model_discovery_rx: Option<std::sync::mpsc::Receiver<AcpModelDiscoveryDelivery>>,
    pub(super) mcp_add_dialog: Option<AiMcpServerDraft>,
    pub(super) key_store: oxideterm_ai::AiProviderKeyStore,
}

/// Owns lazy RAG storage and knowledge reindex delivery state.
pub(super) struct AiKnowledgeWorkspaceState {
    pub(super) rag_store: LazyAiRagStore,
    pub(super) reindex_cancel: Option<Arc<AtomicBool>>,
    pub(super) reindex_rx: Option<std::sync::mpsc::Receiver<KnowledgeReindexDelivery>>,
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
            delivery_wake: crate::workspace::delivery::ActiveDeliveryWake::default(),
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
            reasoning_menu_open: false,
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
            compaction_rx: None,
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
        }
    }
}

impl AiModelWorkspaceState {
    fn new(key_store: oxideterm_ai::AiProviderKeyStore) -> Self {
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
            selector_status_signature: 0,
            acp_model_options: HashMap::new(),
            acp_model_discovery_pending: HashSet::new(),
            acp_model_discovery_tx: None,
            acp_model_discovery_rx: None,
            mcp_add_dialog: None,
            key_store,
        }
    }
}

impl AiKnowledgeWorkspaceState {
    fn new() -> Self {
        Self {
            rag_store: LazyAiRagStore::default(),
            reindex_cancel: None,
            reindex_rx: None,
            window_activation_subscription: None,
        }
    }
}

#[cfg(test)]
mod entity_tests {
    use super::*;
    use gpui::TestAppContext;

    fn test_runtime() -> Arc<tokio::runtime::Runtime> {
        Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("AI entity test runtime"),
        )
    }

    #[gpui::test]
    fn model_refresh_state_and_delivery_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let generation = entity.update(cx, |entity, _cx| {
            let generation = entity
                .begin_model_refresh("provider-a")
                .expect("first refresh starts");
            assert!(entity.begin_model_refresh("provider-a").is_none());
            generation
        });
        let worker_tx = cx.read(|cx| entity.read(cx).model_refresh_tx.clone());
        worker_tx
            .send(AiModelRefreshWorkerDelivery {
                index: 2,
                provider_id: "provider-a".to_string(),
                generation,
                result: Ok(oxideterm_ai::ProviderModelRefresh {
                    models: vec!["model-a".to_string()],
                    context_windows: HashMap::new(),
                }),
            })
            .unwrap();

        cx.run_until_parked();

        cx.read(|cx| {
            let entity = entity.read(cx);
            assert!(!entity.model_is_refreshing("provider-a"));
            assert_eq!(entity.model_refresh_pending, 0);
        });
        let intents = entity.update(cx, |entity, _cx| entity.take_model_refresh_intents());
        assert_eq!(intents.len(), 1);
        assert!(matches!(
            intents.front(),
            Some(AiModelRefreshIntent::Updated {
                index: 2,
                provider_id,
                refresh,
            }) if provider_id == "provider-a" && refresh.models == ["model-a"]
        ));
    }

    #[gpui::test]
    fn provider_key_status_delivery_is_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let worker_tx = entity.update(cx, |entity, _cx| {
            // Seed the pending marker directly so the test never touches the
            // operating system keychain or creates secret material.
            entity
                .provider_key_status_pending
                .insert("provider-a".to_string());
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
            let entity = entity.read(cx);
            assert!(entity.provider_has_key("provider-a"));
            assert!(!entity.provider_key_status_pending.contains("provider-a"));
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
    fn selector_probe_state_and_delivery_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let worker_tx = entity.update(cx, |entity, _cx| {
            entity
                .selector_probe_generations
                .insert("provider-a".to_string(), 7);
            entity.selector_probe_pending = 1;
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
            assert!(!entity.selector_provider_is_online("provider-a"));
            assert_eq!(entity.selector_probe_pending, 0);
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
    fn entity_release_stops_all_entity_delivery_waiters(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let (model_refresh_wake, provider_key_status_wake, selector_probe_wake) = cx.read(|cx| {
            let entity = entity.read(cx);
            (
                entity.model_refresh_tx.wake(),
                entity.provider_key_status_tx.wake(),
                entity.selector_probe_tx.wake(),
            )
        });

        drop(entity);
        cx.update(|_cx| {});
        cx.run_until_parked();

        // The workspace runtime owns in-flight HTTP work independently.
        assert!(model_refresh_wake.is_stopped());
        assert!(provider_key_status_wake.is_stopped());
        assert!(selector_probe_wake.is_stopped());
    }
}
