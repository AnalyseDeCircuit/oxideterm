use super::*;

pub(in crate::workspace) enum AiWorkspaceEvent {
    AcpAgentProbeDeliveryReady,
    AcpModelDiscoveryDeliveryReady,
    ChatStreamDeliveryReady,
    CompactionDeliveryReady,
    CompactionStateChanged,
    KnowledgeReindexDeliveryReady,
    ModelRefreshDeliveryReady,
    ProviderKeyStatusChanged,
    SelectorProviderStatusChanged,
    TerminalInlineDeliveryReady,
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
    knowledge_reindex_progress: Option<(usize, usize)>,
    knowledge_reindex_cancel: Option<Arc<AtomicBool>>,
    knowledge_reindex_tx:
        crate::workspace::delivery::ActiveDeliverySender<AiKnowledgeReindexDelivery>,
    knowledge_reindex_rx: std::sync::mpsc::Receiver<AiKnowledgeReindexDelivery>,
    knowledge_reindex_intents: VecDeque<AiKnowledgeReindexIntent>,
    terminal_inline_panel: AiInlinePanelState,
    terminal_inline_tx: crate::workspace::delivery::ActiveDeliverySender<AiTerminalInlineDelivery>,
    terminal_inline_rx: std::sync::mpsc::Receiver<AiTerminalInlineDelivery>,
    chat_stream_generation: u64,
    chat_stream_task: Option<tokio::task::JoinHandle<()>>,
    chat_stream_tx: AiStreamDeliverySender,
    chat_stream_rx: std::sync::mpsc::Receiver<AiStreamDelivery>,
    chat_stream_deliveries: VecDeque<AiStreamDelivery>,
    compaction_tx: AiCompactionDeliverySender,
    compaction_rx: std::sync::mpsc::Receiver<AiCompactionDelivery>,
    compaction_deliveries: VecDeque<AiCompactionDelivery>,
    compacting_conversations: HashSet<String>,
    compaction_notice: Option<AiCompactionNotice>,
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
            acp_agent_probe_pending: HashSet::new(),
            acp_agent_probe_tx,
            acp_agent_probe_rx,
            acp_agent_probe_intents: VecDeque::new(),
            acp_model_options: HashMap::new(),
            acp_model_discovery_pending: HashSet::new(),
            acp_model_discovery_tx,
            acp_model_discovery_rx,
            acp_model_discovery_intents: VecDeque::new(),
            knowledge_reindex_progress: None,
            knowledge_reindex_cancel: None,
            knowledge_reindex_tx,
            knowledge_reindex_rx,
            knowledge_reindex_intents: VecDeque::new(),
            terminal_inline_panel: AiInlinePanelState::default(),
            terminal_inline_tx,
            terminal_inline_rx,
            chat_stream_generation: 0,
            chat_stream_task: None,
            chat_stream_tx,
            chat_stream_rx,
            chat_stream_deliveries: VecDeque::new(),
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

    pub(in crate::workspace) fn acp_agent_probe_is_pending(&self, agent_id: &str) -> bool {
        self.acp_agent_probe_pending.contains(agent_id)
    }

    pub(in crate::workspace) fn request_acp_agent_probe(
        &mut self,
        agent: oxideterm_settings::AcpAgentConfig,
    ) -> bool {
        if self.acp_agent_probe_pending.contains(&agent.id) {
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
                                let auth_required = !response.auth_methods.is_empty();
                                AiAcpAgentProbeResult {
                                    runtime_state: if auth_required {
                                        oxideterm_settings::AcpAgentRuntimeState::AuthRequired
                                    } else {
                                        oxideterm_settings::AcpAgentRuntimeState::Ready
                                    },
                                    auth_status: if auth_required {
                                        oxideterm_settings::AcpAgentAuthStatus::Required
                                    } else {
                                        oxideterm_settings::AcpAgentAuthStatus::NotRequired
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
        self.task_runtime.spawn(async move {
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
                        config.api_key = api_key;
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
            tokio::spawn(oxideterm_ai::stream_chat_completion(
                config,
                oxideterm_ai::sanitize_api_messages_for_provider(messages),
                stream_tx,
            ));
            while let Some(event) = stream_rx.recv().await {
                match event {
                    oxideterm_ai::AiStreamEvent::Content(chunk) => {
                        let _ =
                            worker_tx.send(AiTerminalInlineDelivery::Content { generation, chunk });
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
                    | oxideterm_ai::AiStreamEvent::ToolCall { .. }
                    | oxideterm_ai::AiStreamEvent::ToolCallComplete { .. } => {}
                }
            }
        });
        true
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
        if let Some(task) = self.chat_stream_task.take() {
            task.abort();
        }
        self.chat_stream_generation = self.chat_stream_generation.saturating_add(1);
        (self.chat_stream_generation, self.chat_stream_tx.clone())
    }

    pub(in crate::workspace) fn set_chat_stream_task(
        &mut self,
        generation: u64,
        task: tokio::task::JoinHandle<()>,
    ) {
        if generation == self.chat_stream_generation {
            self.chat_stream_task = Some(task);
        } else {
            task.abort();
        }
    }

    pub(in crate::workspace) fn cancel_chat_stream(&mut self) -> u64 {
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
        // Invalidate any delivery queued after the terminal event, matching the
        // old one-shot receiver lifetime without keeping a receiver on the root.
        self.chat_stream_task.take();
        self.chat_stream_generation = self.chat_stream_generation.saturating_add(1);
        true
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
                }
                AiTerminalInlineDelivery::Error {
                    generation,
                    message,
                } if generation == panel.generation => {
                    panel.loading = false;
                    panel.error = Some(message);
                    changed = true;
                }
                _ => {}
            }
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

fn ai_acp_probe_error_result(kind: &'static str) -> AiAcpAgentProbeResult {
    // Only stable categories cross the worker boundary; process errors may
    // include args, env values, or local authentication material.
    AiAcpAgentProbeResult {
        runtime_state: oxideterm_settings::AcpAgentRuntimeState::Error,
        auth_status: oxideterm_settings::AcpAgentAuthStatus::Unknown,
        last_error_kind: Some(kind.to_string()),
    }
}

impl gpui::EventEmitter<AiWorkspaceEvent> for AiWorkspaceEntity {}

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
    pub(super) mcp_add_dialog: Option<AiMcpServerDraft>,
    pub(super) key_store: oxideterm_ai::AiProviderKeyStore,
}

/// Owns lazy RAG storage and the workspace-window observation adapter.
pub(super) struct AiKnowledgeWorkspaceState {
    pub(super) rag_store: LazyAiRagStore,
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
            mcp_add_dialog: None,
            key_store,
        }
    }
}

impl AiKnowledgeWorkspaceState {
    fn new() -> Self {
        Self {
            rag_store: LazyAiRagStore::default(),
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
    fn acp_agent_probe_state_and_safe_intent_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let worker_tx = entity.update(cx, |entity, _cx| {
            entity.acp_agent_probe_pending.insert("agent-a".to_string());
            entity.acp_agent_probe_tx.clone()
        });
        worker_tx
            .send(AiAcpAgentProbeDelivery {
                agent_id: "agent-a".to_string(),
                result: AiAcpAgentProbeResult {
                    runtime_state: oxideterm_settings::AcpAgentRuntimeState::AuthRequired,
                    auth_status: oxideterm_settings::AcpAgentAuthStatus::Required,
                    last_error_kind: None,
                },
            })
            .unwrap();

        cx.run_until_parked();

        let intents = entity.update(cx, |entity, _cx| {
            assert!(!entity.acp_agent_probe_is_pending("agent-a"));
            entity.take_acp_agent_probe_intents()
        });
        assert_eq!(intents.len(), 1);
        let intent = intents.front().expect("ACP probe intent");
        assert_eq!(intent.agent_id, "agent-a");
        assert_eq!(
            intent.runtime_state,
            oxideterm_settings::AcpAgentRuntimeState::AuthRequired
        );
        assert_eq!(
            intent.auth_status,
            oxideterm_settings::AcpAgentAuthStatus::Required
        );
        assert!(intent.last_error_kind.is_none());
    }

    #[gpui::test]
    fn acp_model_discovery_delivery_and_options_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let config_options = vec![oxideterm_ai::AcpSessionConfigOption {
            config_id: "model".to_string(),
            name: "Model".to_string(),
            category: Some("model".to_string()),
            current_value_id: "model-a".to_string(),
            choices: vec![oxideterm_ai::AcpSessionConfigChoice {
                value_id: "model-a".to_string(),
                label: "Model A".to_string(),
            }],
        }];
        let worker_tx = entity.update(cx, |entity, _cx| {
            entity
                .acp_model_discovery_pending
                .insert(("conversation-a".to_string(), "agent-a".to_string()));
            entity.acp_model_discovery_tx.clone()
        });
        worker_tx
            .send(AiAcpModelDiscoveryDelivery {
                conversation_id: "conversation-a".to_string(),
                agent_id: "agent-a".to_string(),
                config_options: Some(config_options.clone()),
            })
            .unwrap();

        cx.run_until_parked();

        let intents = entity.update(cx, |entity, _cx| entity.take_acp_model_discovery_intents());
        assert_eq!(intents.len(), 1);
        entity.update(cx, |entity, _cx| {
            entity.apply_acp_model_discovery(
                intents.into_iter().next().expect("discovery intent"),
                true,
            );
            assert_eq!(
                entity.acp_model_options("conversation-a", "agent-a"),
                Some(config_options)
            );
            assert!(!entity.acp_model_discovery_is_pending("conversation-a", "agent-a"));
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
    fn terminal_inline_stream_state_and_delivery_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let worker_tx = entity.update(cx, |entity, _cx| {
            entity.terminal_inline_panel.open = true;
            entity.terminal_inline_panel.loading = true;
            entity.terminal_inline_panel.generation = 7;
            entity.terminal_inline_tx.clone()
        });
        worker_tx
            .send(AiTerminalInlineDelivery::Content {
                generation: 6,
                chunk: "stale".to_string(),
            })
            .unwrap();
        worker_tx
            .send(AiTerminalInlineDelivery::KeyStatus {
                generation: 7,
                has_key: true,
            })
            .unwrap();
        worker_tx
            .send(AiTerminalInlineDelivery::Content {
                generation: 7,
                chunk: "safe output".to_string(),
            })
            .unwrap();
        worker_tx
            .send(AiTerminalInlineDelivery::Done { generation: 7 })
            .unwrap();

        cx.run_until_parked();

        entity.read_with(cx, |entity, _cx| {
            let panel = entity.terminal_inline_panel();
            assert_eq!(panel.response, "safe output");
            assert_eq!(panel.has_api_key, Some(true));
            assert!(!panel.loading);
        });

        entity.update(cx, |entity, _cx| entity.close_terminal_inline_panel());
        worker_tx
            .send(AiTerminalInlineDelivery::Content {
                generation: 7,
                chunk: "late output".to_string(),
            })
            .unwrap();
        cx.run_until_parked();
        entity.read_with(cx, |entity, _cx| {
            let panel = entity.terminal_inline_panel();
            assert!(!panel.open);
            assert_eq!(panel.generation, 8);
            assert_eq!(panel.response, "safe output");
        });
    }

    #[gpui::test]
    fn chat_stream_generation_task_boundary_and_delivery_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let (generation, worker_tx) = entity.update(cx, |entity, _cx| entity.begin_chat_stream());
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
    fn compaction_lifecycle_notice_and_delivery_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|cx| {
            AiWorkspaceEntity::new(test_runtime(), oxideterm_ai::AiProviderKeyStore::new(), cx)
        });
        let worker_tx = entity.update(cx, |entity, cx| {
            assert!(entity.begin_compaction("conversation-a"));
            assert!(!entity.begin_compaction("conversation-a"));
            entity.set_compaction_notice_running("conversation-a", cx);
            entity.compaction_tx.clone()
        });
        worker_tx
            .send(AiCompactionDelivery {
                kind: AiCompactionDeliveryKind::Summary,
                conversation_id: "conversation-a".to_string(),
                base_ids: Vec::new(),
                plan: None,
                summary: "summary".to_string(),
                stream_error: None,
                resume_after: None,
                silent: false,
            })
            .unwrap();

        cx.run_until_parked();

        entity.read_with(cx, |entity, _cx| {
            let notice = entity.compaction_notice().expect("running notice");
            assert_eq!(notice.conversation_id, "conversation-a");
            assert_eq!(notice.phase, AiCompactionNoticePhase::Running);
        });
        let deliveries = entity.update(cx, |entity, _cx| {
            entity.finish_compaction("conversation-a");
            entity.take_compaction_deliveries()
        });
        assert_eq!(deliveries.len(), 1);
        assert!(matches!(
            deliveries.front().map(|delivery| &delivery.kind),
            Some(AiCompactionDeliveryKind::Summary)
        ));
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
