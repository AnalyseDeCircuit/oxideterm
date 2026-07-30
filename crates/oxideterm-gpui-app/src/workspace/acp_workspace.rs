use std::collections::{HashMap, VecDeque};

use gpui::{Context, EventEmitter};
use oxideterm_gpui_terminal::{SharedTerminalSession, TerminalNoticeVariant};
use oxideterm_sftp::TextDiffLineKind;

use super::{
    WorkspaceApp,
    sidebar::{AiStreamDelivery, AiStreamDeliveryEvent},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum AcpThreadRunState {
    Connecting,
    Ready,
    Running,
    Failed,
    Stopped,
}

#[derive(Clone, Debug)]
pub(in crate::workspace) struct AcpThreadSnapshot {
    pub(in crate::workspace) agent_id: String,
    pub(in crate::workspace) session_id: Option<String>,
    pub(in crate::workspace) title: Option<String>,
    pub(in crate::workspace) current_mode_id: Option<String>,
    pub(in crate::workspace) available_modes: Vec<oxideterm_ai::AcpSessionMode>,
    pub(in crate::workspace) config_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
    pub(in crate::workspace) available_commands: Vec<serde_json::Value>,
    pub(in crate::workspace) plan: Option<serde_json::Value>,
    pub(in crate::workspace) usage: Option<serde_json::Value>,
    pub(in crate::workspace) state: AcpThreadRunState,
    pub(in crate::workspace) last_error: Option<String>,
    pub(in crate::workspace) auth_required: bool,
    session_cwd: std::path::PathBuf,
    host_policy: oxideterm_ai::AcpHostCapabilityPolicy,
    active_turn_id: Option<String>,
}

impl AcpThreadSnapshot {
    fn new(
        agent_id: String,
        session_cwd: std::path::PathBuf,
        host_policy: oxideterm_ai::AcpHostCapabilityPolicy,
    ) -> Self {
        Self {
            agent_id,
            session_id: None,
            title: None,
            current_mode_id: None,
            available_modes: Vec::new(),
            config_options: Vec::new(),
            available_commands: Vec::new(),
            plan: None,
            usage: None,
            state: AcpThreadRunState::Connecting,
            last_error: None,
            auth_required: false,
            session_cwd,
            host_policy,
            active_turn_id: None,
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::workspace) struct AcpTurnRoute {
    pub(in crate::workspace) generation: u64,
    pub(in crate::workspace) conversation_id: String,
    pub(in crate::workspace) assistant_id: String,
}

pub(in crate::workspace) struct AcpThreadStart {
    pub(in crate::workspace) route: AcpTurnRoute,
    pub(in crate::workspace) launch_config: oxideterm_ai::AcpLaunchConfig,
    pub(in crate::workspace) host_policy: oxideterm_ai::AcpHostCapabilityPolicy,
    pub(in crate::workspace) request: oxideterm_ai::AcpManagedPromptRequest,
}

pub(in crate::workspace) struct AcpWorkspaceDelivery {
    pub(in crate::workspace) route: Option<AcpTurnRoute>,
    pub(in crate::workspace) conversation_id: Option<String>,
    pub(in crate::workspace) event: oxideterm_ai::AcpManagedEvent,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::workspace) enum AcpWorkspaceEvent {
    DeliveriesReady,
}

pub(in crate::workspace) struct AcpWorkspaceEntity {
    task_runtime: std::sync::Arc<tokio::runtime::Runtime>,
    manager: oxideterm_ai::AcpConnectionManager,
    threads: HashMap<String, AcpThreadSnapshot>,
    active_connection_ids: HashMap<String, u64>,
    session_threads: HashMap<AcpSessionOwner, String>,
    turn_routes: HashMap<String, AcpTurnRoute>,
    turn_tasks: HashMap<String, tokio::task::JoinHandle<()>>,
    terminals: HashMap<String, AcpVisibleTerminal>,
    file_write_previews: HashMap<String, zeroize::Zeroizing<String>>,
    diagnostics: HashMap<String, VecDeque<String>>,
    auth_methods: HashMap<String, Vec<oxideterm_ai::AcpAuthMethod>>,
    deliveries: VecDeque<AcpWorkspaceDelivery>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct AcpSessionOwner {
    agent_id: String,
    session_id: String,
}

impl AcpSessionOwner {
    fn new(agent_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            session_id: session_id.into(),
        }
    }
}

#[derive(Clone)]
pub(in crate::workspace) struct AcpVisibleTerminal {
    owner: AcpSessionOwner,
    pub(in crate::workspace) session: SharedTerminalSession,
    pub(in crate::workspace) output_byte_limit: Option<usize>,
}

impl EventEmitter<AcpWorkspaceEvent> for AcpWorkspaceEntity {}

impl AcpWorkspaceEntity {
    pub(in crate::workspace) fn new(
        task_runtime: std::sync::Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = oxideterm_ai::AcpConnectionManager::new(env!("CARGO_PKG_VERSION"), event_tx);
        cx.spawn(async move |weak, cx| {
            while let Some(event) = event_rx.recv().await {
                if weak
                    .update(cx, |entity, cx| entity.receive_event(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.on_release(|entity, _cx| {
            // Entity release is the authority boundary for every ACP process
            // and turn owned by this workspace window.
            for (_, task) in entity.turn_tasks.drain() {
                task.abort();
            }
            for (_, terminal) in entity.terminals.drain() {
                terminal.session.lock().shutdown();
            }
            entity.file_write_previews.clear();
            let agent_ids = entity
                .threads
                .values()
                .map(|thread| thread.agent_id.clone())
                .collect::<std::collections::HashSet<_>>();
            for agent_id in agent_ids {
                entity.manager.shutdown_agent(&agent_id);
            }
        })
        .detach();
        Self {
            task_runtime,
            manager,
            threads: HashMap::new(),
            active_connection_ids: HashMap::new(),
            session_threads: HashMap::new(),
            turn_routes: HashMap::new(),
            turn_tasks: HashMap::new(),
            terminals: HashMap::new(),
            file_write_previews: HashMap::new(),
            diagnostics: HashMap::new(),
            auth_methods: HashMap::new(),
            deliveries: VecDeque::new(),
        }
    }

    pub(in crate::workspace) fn start_turn(&mut self, start: AcpThreadStart) -> bool {
        let turn_id = start.request.turn_id.clone();
        if self.turn_tasks.contains_key(&turn_id) {
            return false;
        }
        let conversation_id = start.route.conversation_id.clone();
        let agent_id = start.launch_config.id.clone();
        if self
            .threads
            .get(&conversation_id)
            .is_some_and(|thread| thread.agent_id == agent_id && thread.active_turn_id.is_some())
        {
            // A thread has exactly one protocol turn at a time. The UI guard
            // is not an ownership boundary, so enforce it again in the entity.
            return false;
        }
        let session_cwd = start.request.cwd.clone();
        if self
            .threads
            .get(&conversation_id)
            .is_some_and(|thread| thread.agent_id != agent_id)
            && let Some(previous_thread) = self.threads.remove(&conversation_id)
        {
            // A conversation changing agents closes only its previous session.
            // The old agent process may still own sessions for other chats.
            self.release_thread_resources(&conversation_id, &previous_thread);
            let manager = self.manager.clone();
            let previous_agent_id = previous_thread.agent_id;
            let previous_thread_id = conversation_id.clone();
            self.task_runtime.spawn(async move {
                let _ = manager
                    .close_thread(&previous_agent_id, &previous_thread_id, false)
                    .await;
            });
        }
        let thread = self
            .threads
            .entry(conversation_id.clone())
            .or_insert_with(|| {
                AcpThreadSnapshot::new(
                    agent_id.clone(),
                    session_cwd.clone(),
                    start.host_policy.clone(),
                )
            });
        thread.state = AcpThreadRunState::Connecting;
        thread.last_error = None;
        thread.auth_required = false;
        thread.active_turn_id = Some(turn_id.clone());
        thread.session_cwd = start.request.cwd.clone();
        thread.host_policy = start.host_policy.clone();
        self.turn_routes.insert(turn_id.clone(), start.route);

        let manager = self.manager.clone();
        let task = self.task_runtime.spawn(async move {
            let _ = manager
                .prompt(start.launch_config, start.host_policy, start.request)
                .await;
        });
        self.turn_tasks.insert(turn_id, task);
        true
    }

    pub(in crate::workspace) fn cancel_active_turn(
        &self,
        conversation_id: &str,
    ) -> Result<(), oxideterm_ai::AcpConnectionError> {
        let thread = self
            .threads
            .get(conversation_id)
            .ok_or(oxideterm_ai::AcpConnectionError::Unavailable)?;
        self.manager.cancel(&thread.agent_id, conversation_id)
    }

    pub(in crate::workspace) fn set_config_selection(
        &self,
        conversation_id: &str,
        selection: oxideterm_ai::AcpSessionConfigSelection,
    ) -> bool {
        let Some(thread) = self.threads.get(conversation_id) else {
            return false;
        };
        if thread.session_id.is_none() {
            return false;
        }
        let manager = self.manager.clone();
        let agent_id = thread.agent_id.clone();
        let thread_id = conversation_id.to_string();
        self.task_runtime.spawn(async move {
            let _ = manager
                .set_config_selection(&agent_id, &thread_id, selection)
                .await;
        });
        true
    }

    pub(in crate::workspace) fn set_mode(&self, conversation_id: &str, mode_id: String) -> bool {
        let Some(thread) = self.threads.get(conversation_id) else {
            return false;
        };
        if thread.session_id.is_none()
            || !thread
                .available_modes
                .iter()
                .any(|mode| mode.mode_id == mode_id)
        {
            return false;
        }
        let manager = self.manager.clone();
        let agent_id = thread.agent_id.clone();
        let thread_id = conversation_id.to_string();
        self.task_runtime.spawn(async move {
            let _ = manager.set_mode(&agent_id, &thread_id, mode_id).await;
        });
        true
    }

    pub(in crate::workspace) fn authentication_methods(
        &self,
        conversation_id: &str,
    ) -> Option<Vec<oxideterm_ai::AcpAuthMethod>> {
        let thread = self.threads.get(conversation_id)?;
        thread
            .auth_required
            .then(|| self.auth_methods.get(&thread.agent_id).cloned())
            .flatten()
    }

    pub(in crate::workspace) fn authenticate(
        &self,
        conversation_id: &str,
        method_id: String,
    ) -> bool {
        let Some(thread) = self.threads.get(conversation_id) else {
            return false;
        };
        let manager = self.manager.clone();
        let agent_id = thread.agent_id.clone();
        self.task_runtime.spawn(async move {
            let _ = manager.authenticate(&agent_id, method_id).await;
        });
        true
    }

    pub(in crate::workspace) fn close_thread(
        &mut self,
        conversation_id: &str,
        delete_remote: bool,
    ) -> bool {
        let Some(thread) = self.threads.remove(conversation_id) else {
            return false;
        };
        self.release_thread_resources(conversation_id, &thread);
        let manager = self.manager.clone();
        let agent_id = thread.agent_id;
        let thread_id = conversation_id.to_string();
        self.task_runtime.spawn(async move {
            let _ = manager
                .close_thread(&agent_id, &thread_id, delete_remote)
                .await;
        });
        true
    }

    pub(in crate::workspace) fn close_all_threads(&mut self, delete_remote: bool) {
        let conversation_ids = self.threads.keys().cloned().collect::<Vec<_>>();
        for conversation_id in conversation_ids {
            self.close_thread(&conversation_id, delete_remote);
        }
    }

    fn release_thread_resources(&mut self, conversation_id: &str, thread: &AcpThreadSnapshot) {
        if let Some(turn_id) = thread.active_turn_id.as_deref() {
            let _ = self.manager.cancel(&thread.agent_id, conversation_id);
            if let Some(task) = self.turn_tasks.remove(turn_id) {
                task.abort();
            }
            self.turn_routes.remove(turn_id);
        }
        let Some(session_id) = thread.session_id.as_deref() else {
            return;
        };
        let owner = AcpSessionOwner::new(thread.agent_id.clone(), session_id);
        self.session_threads.remove(&owner);
        let owned_terminal_ids = self
            .terminals
            .iter()
            .filter(|(_, terminal)| terminal.owner == owner)
            .map(|(terminal_id, _)| terminal_id.clone())
            .collect::<Vec<_>>();
        for terminal_id in owned_terminal_ids {
            if let Some(terminal) = self.terminals.remove(&terminal_id) {
                terminal.session.lock().shutdown();
            }
        }
    }

    pub(in crate::workspace) fn route_for_turn(&self, turn_id: &str) -> Option<&AcpTurnRoute> {
        self.turn_routes.get(turn_id)
    }

    pub(in crate::workspace) fn route_for_session(
        &self,
        agent_id: &str,
        session_id: &str,
    ) -> Option<&AcpTurnRoute> {
        let owner = AcpSessionOwner::new(agent_id, session_id);
        let conversation_id = self.session_threads.get(&owner)?;
        let turn_id = self
            .threads
            .get(conversation_id)?
            .active_turn_id
            .as_deref()?;
        self.turn_routes.get(turn_id)
    }

    fn conversation_for_session(&self, agent_id: &str, session_id: &str) -> Option<&str> {
        let owner = AcpSessionOwner::new(agent_id, session_id);
        self.session_threads.get(&owner).map(String::as_str)
    }

    pub(in crate::workspace) fn session_context(
        &self,
        agent_id: &str,
        session_id: &str,
    ) -> Option<(std::path::PathBuf, oxideterm_ai::AcpHostCapabilityPolicy)> {
        let owner = AcpSessionOwner::new(agent_id, session_id);
        let conversation_id = self.session_threads.get(&owner)?;
        let thread = self.threads.get(conversation_id)?;
        Some((thread.session_cwd.clone(), thread.host_policy.clone()))
    }

    pub(in crate::workspace) fn register_terminal(
        &mut self,
        terminal_id: String,
        agent_id: String,
        owner_session_id: String,
        session: SharedTerminalSession,
        output_byte_limit: Option<usize>,
    ) {
        // The ACP entity remains the lifecycle owner even though the terminal
        // is projected into the normal workspace tab registry.
        self.terminals.insert(
            terminal_id,
            AcpVisibleTerminal {
                owner: AcpSessionOwner::new(agent_id, owner_session_id),
                session,
                output_byte_limit,
            },
        );
    }

    pub(in crate::workspace) fn terminal(
        &self,
        terminal_id: &str,
        agent_id: &str,
        owner_session_id: &str,
    ) -> Option<AcpVisibleTerminal> {
        let owner = AcpSessionOwner::new(agent_id, owner_session_id);
        self.terminals
            .get(terminal_id)
            .filter(|terminal| terminal.owner == owner)
            .cloned()
    }

    pub(in crate::workspace) fn release_terminal(
        &mut self,
        terminal_id: &str,
        agent_id: &str,
        owner_session_id: &str,
    ) -> Option<AcpVisibleTerminal> {
        let owner = AcpSessionOwner::new(agent_id, owner_session_id);
        if self
            .terminals
            .get(terminal_id)
            .is_some_and(|terminal| terminal.owner == owner)
        {
            self.terminals.remove(terminal_id)
        } else {
            None
        }
    }

    pub(in crate::workspace) fn register_file_write_preview(
        &mut self,
        tool_call_id: String,
        preview: zeroize::Zeroizing<String>,
    ) {
        // File contents stay in this ephemeral owner and never enter persisted
        // chat tool payloads or diagnostic output.
        self.file_write_previews.insert(tool_call_id, preview);
    }

    pub(in crate::workspace) fn file_write_preview(&self, tool_call_id: &str) -> Option<String> {
        self.file_write_previews
            .get(tool_call_id)
            .map(|preview| preview.as_str().to_string())
    }

    pub(in crate::workspace) fn remove_file_write_preview(&mut self, tool_call_id: &str) {
        self.file_write_previews.remove(tool_call_id);
    }

    pub(in crate::workspace) fn take_deliveries(&mut self) -> VecDeque<AcpWorkspaceDelivery> {
        std::mem::take(&mut self.deliveries)
    }

    fn receive_event(&mut self, event: oxideterm_ai::AcpManagedEvent, cx: &mut Context<Self>) {
        if !self.accepts_event(&event) {
            return;
        }
        match &event {
            oxideterm_ai::AcpManagedEvent::ConnectionState {
                agent_id,
                state,
                message,
                ..
            } => {
                for thread in self
                    .threads
                    .values_mut()
                    .filter(|thread| &thread.agent_id == agent_id)
                {
                    thread.state = match state {
                        oxideterm_ai::AcpConnectionState::Connecting => {
                            AcpThreadRunState::Connecting
                        }
                        oxideterm_ai::AcpConnectionState::Ready => AcpThreadRunState::Ready,
                        oxideterm_ai::AcpConnectionState::Stopped => AcpThreadRunState::Stopped,
                        oxideterm_ai::AcpConnectionState::Failed => AcpThreadRunState::Failed,
                    };
                    thread.last_error = message.clone();
                }
            }
            oxideterm_ai::AcpManagedEvent::Diagnostic {
                agent_id, message, ..
            } => {
                const MAX_DIAGNOSTIC_LINES: usize = 100;

                let diagnostics = self.diagnostics.entry(agent_id.clone()).or_default();
                diagnostics.push_back(message.clone());
                while diagnostics.len() > MAX_DIAGNOSTIC_LINES {
                    diagnostics.pop_front();
                }
            }
            oxideterm_ai::AcpManagedEvent::AuthenticationMethods {
                agent_id, methods, ..
            } => {
                self.auth_methods.insert(agent_id.clone(), methods.clone());
            }
            oxideterm_ai::AcpManagedEvent::AuthenticationFinished {
                agent_id, result, ..
            } => {
                for thread in self
                    .threads
                    .values_mut()
                    .filter(|thread| &thread.agent_id == agent_id)
                {
                    thread.auth_required = result.is_err();
                    match result {
                        Ok(()) => {
                            thread.state = AcpThreadRunState::Ready;
                            thread.last_error = None;
                        }
                        Err(error) => {
                            thread.state = AcpThreadRunState::Failed;
                            thread.last_error = Some(error.to_string());
                        }
                    }
                }
            }
            oxideterm_ai::AcpManagedEvent::SessionReady {
                agent_id,
                thread_id,
                outcome,
                ..
            } => {
                let thread = self
                    .threads
                    .get_mut(thread_id)
                    .expect("current ACP turn must retain its thread owner");
                thread.session_id = Some(outcome.session_id.clone());
                thread.config_options = outcome.session_config_options.clone();
                match outcome.session_modes.as_ref() {
                    Some(modes) => {
                        thread.current_mode_id = Some(modes.current_mode_id.clone());
                        thread.available_modes = modes.available_modes.clone();
                    }
                    None => {
                        thread.current_mode_id = None;
                        thread.available_modes.clear();
                    }
                }
                thread.state = AcpThreadRunState::Running;
                thread.last_error = None;
                thread.auth_required = false;
                self.session_threads.insert(
                    AcpSessionOwner::new(agent_id.clone(), outcome.session_id.clone()),
                    thread_id.clone(),
                );
            }
            oxideterm_ai::AcpManagedEvent::ConfigUpdated {
                thread_id,
                config_options,
                ..
            } => {
                if let Some(thread) = self.threads.get_mut(thread_id) {
                    thread.config_options = config_options.clone();
                    thread.last_error = None;
                }
            }
            oxideterm_ai::AcpManagedEvent::ModeUpdated {
                thread_id, mode_id, ..
            } => {
                if let Some(thread) = self.threads.get_mut(thread_id) {
                    thread.current_mode_id = Some(mode_id.clone());
                    thread.last_error = None;
                }
            }
            oxideterm_ai::AcpManagedEvent::ControlFailed {
                thread_id, error, ..
            } => {
                if let Some(thread) = self.threads.get_mut(thread_id) {
                    thread.last_error = Some(error.to_string());
                }
            }
            oxideterm_ai::AcpManagedEvent::Client {
                agent_id,
                event: oxideterm_ai::AcpClientEvent::SessionUpdate(notification),
                ..
            } => {
                let session_id = notification.session_id.to_string();
                if let Some(update) = oxideterm_ai::acp_session_state_update(notification) {
                    self.apply_session_update(agent_id, &session_id, update);
                }
            }
            oxideterm_ai::AcpManagedEvent::TurnFinished {
                thread_id,
                turn_id,
                result,
                ..
            } => {
                if let Some(task) = self.turn_tasks.remove(turn_id) {
                    drop(task);
                }
                if let Some(thread) = self.threads.get_mut(thread_id) {
                    thread.active_turn_id = None;
                    match result {
                        Ok(outcome) => {
                            thread.session_id = Some(outcome.session_id.clone());
                            thread.config_options = outcome.session_config_options.clone();
                            match outcome.session_modes.as_ref() {
                                Some(modes) => {
                                    thread.current_mode_id = Some(modes.current_mode_id.clone());
                                    thread.available_modes = modes.available_modes.clone();
                                }
                                None => {
                                    thread.current_mode_id = None;
                                    thread.available_modes.clear();
                                }
                            }
                            thread.state = AcpThreadRunState::Ready;
                            thread.last_error = None;
                            thread.auth_required = false;
                        }
                        Err(error) => {
                            thread.state = AcpThreadRunState::Failed;
                            thread.auth_required =
                                matches!(error, oxideterm_ai::AcpConnectionError::AuthRequired);
                            thread.last_error = Some(error.to_string());
                        }
                    }
                }
            }
            oxideterm_ai::AcpManagedEvent::Client { .. } => {}
        }

        let route = match &event {
            oxideterm_ai::AcpManagedEvent::SessionReady { turn_id, .. }
            | oxideterm_ai::AcpManagedEvent::TurnFinished { turn_id, .. } => {
                self.route_for_turn(turn_id).cloned()
            }
            oxideterm_ai::AcpManagedEvent::ConfigUpdated { .. } => None,
            oxideterm_ai::AcpManagedEvent::ModeUpdated { .. } => None,
            oxideterm_ai::AcpManagedEvent::ControlFailed { .. } => None,
            oxideterm_ai::AcpManagedEvent::Client {
                agent_id, event, ..
            } => event
                .session_id()
                .and_then(|session_id| self.route_for_session(agent_id, &session_id).cloned()),
            oxideterm_ai::AcpManagedEvent::ConnectionState { .. } => None,
            oxideterm_ai::AcpManagedEvent::Diagnostic { .. } => None,
            oxideterm_ai::AcpManagedEvent::AuthenticationMethods { .. }
            | oxideterm_ai::AcpManagedEvent::AuthenticationFinished { .. } => None,
        };
        let conversation_id = match &event {
            oxideterm_ai::AcpManagedEvent::SessionReady { thread_id, .. }
            | oxideterm_ai::AcpManagedEvent::TurnFinished { thread_id, .. }
            | oxideterm_ai::AcpManagedEvent::ConfigUpdated { thread_id, .. }
            | oxideterm_ai::AcpManagedEvent::ModeUpdated { thread_id, .. }
            | oxideterm_ai::AcpManagedEvent::ControlFailed { thread_id, .. } => {
                Some(thread_id.clone())
            }
            oxideterm_ai::AcpManagedEvent::Client {
                agent_id, event, ..
            } => event.session_id().and_then(|session_id| {
                self.conversation_for_session(agent_id, &session_id)
                    .map(str::to_string)
            }),
            _ => None,
        };
        self.deliveries.push_back(AcpWorkspaceDelivery {
            route,
            conversation_id,
            event,
        });
        if let oxideterm_ai::AcpManagedEvent::TurnFinished { turn_id, .. } = &self
            .deliveries
            .back()
            .expect("ACP delivery was just queued")
            .event
        {
            self.turn_routes.remove(turn_id);
        }
        cx.emit(AcpWorkspaceEvent::DeliveriesReady);
        cx.notify();
    }

    fn apply_session_update(
        &mut self,
        agent_id: &str,
        session_id: &str,
        update: oxideterm_ai::AcpSessionStateUpdate,
    ) {
        let owner = AcpSessionOwner::new(agent_id, session_id);
        let Some(conversation_id) = self.session_threads.get(&owner).cloned() else {
            return;
        };
        let Some(thread) = self.threads.get_mut(&conversation_id) else {
            return;
        };
        match update {
            oxideterm_ai::AcpSessionStateUpdate::ConfigOptions(config_options) => {
                thread.config_options = config_options;
            }
            oxideterm_ai::AcpSessionStateUpdate::CurrentMode(mode_id) => {
                thread.current_mode_id = Some(mode_id);
            }
            oxideterm_ai::AcpSessionStateUpdate::AvailableCommands(commands) => {
                thread.available_commands = commands;
            }
            oxideterm_ai::AcpSessionStateUpdate::Plan(plan) => {
                thread.plan = Some(plan);
            }
            oxideterm_ai::AcpSessionStateUpdate::SessionInfo { title, .. } => {
                thread.title = title;
            }
            oxideterm_ai::AcpSessionStateUpdate::Usage(usage) => thread.usage = Some(usage),
        }
    }

    fn accepts_event(&mut self, event: &oxideterm_ai::AcpManagedEvent) -> bool {
        match event {
            oxideterm_ai::AcpManagedEvent::ConnectionState {
                agent_id,
                connection_id,
                state: oxideterm_ai::AcpConnectionState::Connecting,
                ..
            } => {
                let active_connection_id = self
                    .active_connection_ids
                    .entry(agent_id.clone())
                    .or_insert(*connection_id);
                if *connection_id < *active_connection_id {
                    false
                } else {
                    *active_connection_id = *connection_id;
                    true
                }
            }
            oxideterm_ai::AcpManagedEvent::ConnectionState {
                agent_id,
                connection_id,
                ..
            }
            | oxideterm_ai::AcpManagedEvent::Diagnostic {
                agent_id,
                connection_id,
                ..
            }
            | oxideterm_ai::AcpManagedEvent::AuthenticationMethods {
                agent_id,
                connection_id,
                ..
            }
            | oxideterm_ai::AcpManagedEvent::AuthenticationFinished {
                agent_id,
                connection_id,
                ..
            }
            | oxideterm_ai::AcpManagedEvent::Client {
                agent_id,
                connection_id,
                ..
            } => self.active_connection_ids.get(agent_id) == Some(connection_id),
            oxideterm_ai::AcpManagedEvent::SessionReady {
                agent_id,
                thread_id,
                turn_id,
                ..
            }
            | oxideterm_ai::AcpManagedEvent::TurnFinished {
                agent_id,
                thread_id,
                turn_id,
                ..
            } => self.threads.get(thread_id).is_some_and(|thread| {
                thread.agent_id == *agent_id
                    && thread.active_turn_id.as_deref() == Some(turn_id.as_str())
            }),
            oxideterm_ai::AcpManagedEvent::ConfigUpdated {
                agent_id,
                connection_id,
                thread_id,
                ..
            }
            | oxideterm_ai::AcpManagedEvent::ModeUpdated {
                agent_id,
                connection_id,
                thread_id,
                ..
            }
            | oxideterm_ai::AcpManagedEvent::ControlFailed {
                agent_id,
                connection_id,
                thread_id,
                ..
            } => {
                self.active_connection_ids.get(agent_id) == Some(connection_id)
                    && self
                        .threads
                        .get(thread_id)
                        .is_some_and(|thread| thread.agent_id == *agent_id)
            }
        }
    }
}

pub(in crate::workspace) fn acp_file_write_preview(
    existing: &str,
    proposed: &str,
) -> zeroize::Zeroizing<String> {
    const MAX_PREVIEW_LINES: usize = 400;
    const MAX_PREVIEW_BYTES: usize = 64 * 1024;

    let mut preview = String::from("--- existing\n+++ proposed\n");
    let lines = oxideterm_sftp::compute_text_diff(existing, proposed);
    let truncated_lines = lines.len() > MAX_PREVIEW_LINES;
    for line in lines.into_iter().take(MAX_PREVIEW_LINES) {
        let prefix = match line.kind {
            TextDiffLineKind::Unchanged => ' ',
            TextDiffLineKind::Added => '+',
            TextDiffLineKind::Removed => '-',
        };
        preview.push(prefix);
        preview.push_str(&line.content);
        preview.push('\n');
        if preview.len() >= MAX_PREVIEW_BYTES {
            break;
        }
    }
    if truncated_lines || preview.len() >= MAX_PREVIEW_BYTES {
        let mut boundary = preview.len().min(MAX_PREVIEW_BYTES);
        while !preview.is_char_boundary(boundary) {
            boundary = boundary.saturating_sub(1);
        }
        preview.truncate(boundary);
        preview.push_str("\n…");
    }
    zeroize::Zeroizing::new(preview)
}

impl WorkspaceApp {
    pub(in crate::workspace) fn forward_acp_workspace_deliveries(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let deliveries = self
            .acp_entity
            .update(cx, |entity, _cx| entity.take_deliveries());
        for delivery in deliveries {
            match &delivery.event {
                oxideterm_ai::AcpManagedEvent::ConfigUpdated {
                    thread_id,
                    config_options,
                    ..
                } => {
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.apply_acp_session_state_update(
                            thread_id,
                            oxideterm_ai::AcpSessionStateUpdate::ConfigOptions(
                                config_options.clone(),
                            ),
                        );
                    });
                    continue;
                }
                oxideterm_ai::AcpManagedEvent::ModeUpdated {
                    thread_id, mode_id, ..
                } => {
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.apply_acp_session_state_update(
                            thread_id,
                            oxideterm_ai::AcpSessionStateUpdate::CurrentMode(mode_id.clone()),
                        );
                    });
                    continue;
                }
                oxideterm_ai::AcpManagedEvent::ControlFailed { .. } => {
                    self.push_ai_settings_toast(
                        self.i18n.t("settings_view.ai.acp_agent_error_unknown"),
                        TerminalNoticeVariant::Error,
                        cx,
                    );
                    continue;
                }
                oxideterm_ai::AcpManagedEvent::Client {
                    event: oxideterm_ai::AcpClientEvent::SessionUpdate(notification),
                    ..
                } => {
                    if let Some(conversation_id) = delivery.conversation_id.as_deref()
                        && let Some(update) = oxideterm_ai::acp_session_state_update(notification)
                    {
                        self.ai_entity.update(cx, |ai, _cx| {
                            ai.apply_acp_session_state_update(conversation_id, update);
                        });
                    }
                }
                _ => {}
            }
            if let (
                Some(route),
                oxideterm_ai::AcpManagedEvent::TurnFinished {
                    agent_id,
                    result: Ok(_),
                    ..
                },
            ) = (delivery.route.as_ref(), &delivery.event)
            {
                self.ai_entity.update(cx, |ai, _cx| {
                    ai.mark_acp_handoff_cursor(
                        &route.conversation_id,
                        agent_id,
                        &route.assistant_id,
                    );
                });
            }
            let Some(route) = delivery.route else {
                continue;
            };
            let event = match delivery.event {
                oxideterm_ai::AcpManagedEvent::SessionReady {
                    agent_id, outcome, ..
                } => AiStreamDeliveryEvent::AcpSessionStarted {
                    session_id: outcome.session_id,
                    session_metadata: outcome.session_metadata,
                    session_config_options: outcome.session_config_options,
                    session_modes: outcome.session_modes,
                    agent_id,
                },
                oxideterm_ai::AcpManagedEvent::Client {
                    agent_id, event, ..
                } => AiStreamDeliveryEvent::AcpClientEvent { agent_id, event },
                oxideterm_ai::AcpManagedEvent::TurnFinished { result, .. } => {
                    AiStreamDeliveryEvent::Stream(if result.is_ok() {
                        oxideterm_ai::AiStreamEvent::Done
                    } else {
                        oxideterm_ai::AiStreamEvent::Error("stream_failed".to_string())
                    })
                }
                oxideterm_ai::AcpManagedEvent::ConnectionState { .. } => continue,
                oxideterm_ai::AcpManagedEvent::Diagnostic { .. } => continue,
                oxideterm_ai::AcpManagedEvent::AuthenticationMethods { .. }
                | oxideterm_ai::AcpManagedEvent::AuthenticationFinished { .. } => continue,
                oxideterm_ai::AcpManagedEvent::ConfigUpdated { .. } => continue,
                oxideterm_ai::AcpManagedEvent::ModeUpdated { .. } => continue,
                oxideterm_ai::AcpManagedEvent::ControlFailed { .. } => continue,
            };
            self.ai_entity.update(cx, |ai, _cx| {
                ai.enqueue_chat_stream_delivery(AiStreamDelivery {
                    generation: route.generation,
                    conversation_id: route.conversation_id,
                    assistant_id: route.assistant_id,
                    event,
                });
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AppContext;

    #[test]
    fn session_owner_keeps_equal_agent_session_ids_isolated() {
        let mut sessions = HashMap::new();
        sessions.insert(AcpSessionOwner::new("agent-a", "session-1"), "chat-a");
        sessions.insert(AcpSessionOwner::new("agent-b", "session-1"), "chat-b");

        assert_eq!(sessions.len(), 2);
        assert_eq!(
            sessions.get(&AcpSessionOwner::new("agent-a", "session-1")),
            Some(&"chat-a")
        );
        assert_eq!(
            sessions.get(&AcpSessionOwner::new("agent-b", "session-1")),
            Some(&"chat-b")
        );
    }

    #[gpui::test]
    fn stale_connection_and_turn_events_cannot_replace_current_state(
        cx: &mut gpui::TestAppContext,
    ) {
        let task_runtime = std::sync::Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("ACP test runtime"),
        );
        let entity = cx.new(|cx| AcpWorkspaceEntity::new(task_runtime, cx));
        entity.update(cx, |entity, _cx| {
            let current_connecting = oxideterm_ai::AcpManagedEvent::ConnectionState {
                agent_id: "agent-current".to_string(),
                connection_id: 2,
                state: oxideterm_ai::AcpConnectionState::Connecting,
                message: None,
            };
            let stale_diagnostic = oxideterm_ai::AcpManagedEvent::Diagnostic {
                agent_id: "agent-current".to_string(),
                connection_id: 1,
                message: "stale".to_string(),
            };
            let current_diagnostic = oxideterm_ai::AcpManagedEvent::Diagnostic {
                agent_id: "agent-current".to_string(),
                connection_id: 2,
                message: "current".to_string(),
            };
            assert!(entity.accepts_event(&current_connecting));
            assert!(!entity.accepts_event(&stale_diagnostic));
            assert!(entity.accepts_event(&current_diagnostic));

            let mut thread = AcpThreadSnapshot::new(
                "agent-current".to_string(),
                std::path::PathBuf::from("."),
                oxideterm_ai::AcpHostCapabilityPolicy::default(),
            );
            thread.active_turn_id = Some("turn-current".to_string());
            entity.threads.insert("chat-1".to_string(), thread);

            let stale = oxideterm_ai::AcpManagedEvent::SessionReady {
                agent_id: "agent-old".to_string(),
                thread_id: "chat-1".to_string(),
                turn_id: "turn-old".to_string(),
                outcome: oxideterm_ai::AcpPromptSessionOutcome {
                    session_id: "session-old".to_string(),
                    session_metadata: None,
                    session_config_options: Vec::new(),
                    session_modes: None,
                },
            };
            let current = oxideterm_ai::AcpManagedEvent::SessionReady {
                agent_id: "agent-current".to_string(),
                thread_id: "chat-1".to_string(),
                turn_id: "turn-current".to_string(),
                outcome: oxideterm_ai::AcpPromptSessionOutcome {
                    session_id: "session-current".to_string(),
                    session_metadata: None,
                    session_config_options: Vec::new(),
                    session_modes: None,
                },
            };

            assert!(!entity.accepts_event(&stale));
            assert!(entity.accepts_event(&current));
        });
    }
}
