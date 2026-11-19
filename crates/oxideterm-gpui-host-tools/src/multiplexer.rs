//! Provider-neutral multiplexer Host Tool: session list, object tree, verb
//! dispatch, confirms, the input dialog and pane reads. The panel renders the
//! `multiplexer` model only; no provider wire type reaches this module.

use super::*;

mod dialogs;

mod panel;
use chrono::TimeZone;
use gpui::Div;
use oxideterm_connection_monitor::multiplexer::{
    Multiplexer, MuxKind, MuxNode, MuxSession, MuxSnapshot, MuxStatus, MuxVerb, MuxVerbKind,
    node_matches_query, status_label_key,
};
use std::collections::{HashMap, HashSet};
use zeroize::Zeroize;

/// `verb_command` validates its payload grammar, so a fixed placeholder answers
/// whether a row supports a payload verb before the user has typed anything.
const MUX_VERB_PROBE: &str = "mux verb";

/// Every multiplexer row renders its verb buttons from the model's verb table;
/// a row that is itself a detached agent only offers the agent attach.
const MUX_ROW_INDENT: f32 = 16.0;

#[derive(Default)]
pub(crate) struct MuxPanel {
    /// `None` keeps the panel on the tool's own tmux list.
    pub(crate) provider: Option<Multiplexer>,
    pub(crate) host: String,
    pub(crate) snapshot: Option<MuxSnapshot>,
    pub(crate) sessions_refreshed_ms: Option<u64>,
    pub(crate) snapshot_running: Option<MuxSnapshotRequest>,
    pub(crate) snapshot_pending: Option<MuxSnapshotRequest>,
    /// Subtrees belong to one session, so each expanded session is captured
    /// from that session's own socket and cached under its name.
    pub(crate) details: HashMap<String, Vec<MuxNode>>,
    /// Sessions whose subtree capture failed. The model gives a subtree no
    /// status of its own, so the row renders one localized failure line.
    pub(crate) detail_failed: HashSet<String>,
    pub(crate) detail_running: Option<MuxDetailRequest>,
    pub(crate) session: Option<String>,
    pub(crate) expanded: HashSet<(MuxKind, String)>,
    pub(crate) confirm: Option<HostToolConfirmState<MuxActionRequest>>,
    pub(crate) action_running: Option<MuxActionRun>,
    pub(crate) read: Option<MuxReadDialog>,
    /// The Entity owns one live capture per query kind and aborts it when its
    /// request is superseded, when the host or provider changes, or when the
    /// viewer that asked for it closes. Remote command output must not be
    /// fetched for a panel the user already left.
    pub(crate) snapshot_task: Option<tokio::task::JoinHandle<()>>,
    pub(crate) detail_task: Option<tokio::task::JoinHandle<()>>,
    pub(crate) action_task: Option<tokio::task::JoinHandle<()>>,
    pub(crate) read_task: Option<tokio::task::JoinHandle<()>>,
    /// Never reset: a generation that repeats would let a capture that started
    /// before a host change be accepted as one that started after it.
    pub(crate) generation: u64,
}

impl MuxPanel {
    /// Switching provider or connection drops host data but keeps the choice.
    /// The in-flight captures belong to the dropped host, so they are aborted
    /// instead of left to deliver into the new context.
    pub(super) fn clear_host(&mut self) {
        let Self {
            provider,
            generation,
            snapshot_task,
            detail_task,
            action_task,
            read_task,
            ..
        } = self;
        for task in [snapshot_task, detail_task, action_task, read_task] {
            if let Some(task) = task.take() {
                task.abort();
            }
        }
        *self = Self {
            provider: *provider,
            generation: *generation,
            ..Default::default()
        };
    }

    /// Stops the capture behind one slot when its request is superseded. The
    /// delivery of an aborted task never reaches the panel, so the slot it
    /// held is already free for the replacement request.
    fn cancel(task: &mut Option<tokio::task::JoinHandle<()>>) {
        if let Some(task) = task.take() {
            task.abort();
        }
    }

    fn next_generation(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }

    fn clear_session_tree(&mut self) {
        Self::cancel(&mut self.detail_task);
        self.details.clear();
        self.detail_failed.clear();
        self.detail_running = None;
        self.session = None;
        self.expanded.clear();
    }

    fn failed_snapshot() -> MuxSnapshot {
        // Captured remote output never becomes the failure text; the panel owns
        // the localized fallback.
        MuxSnapshot {
            status: MuxStatus::Error(None),
            sessions: Vec::new(),
        }
    }
}

impl HostToolsEntity {
    // ---- snapshot lifecycle ----

    /// The tool's entry point for a provider tab: capture the session list for
    /// one connection, coalescing repeat requests behind the in-flight sample.
    pub(super) fn refresh_mux_sessions(
        &mut self,
        host: String,
        feedback: HostSnapshotFeedback,
        cx: &mut Context<Self>,
    ) {
        if self.host_tmux.mux.provider.is_none() {
            return;
        }
        if self.host_tmux.mux.host != host {
            self.host_tmux.mux.clear_host();
            self.host_tmux.mux.host = host.clone();
        }
        let Some(provider) = self.host_tmux.mux.provider else {
            return;
        };
        self.request_mux_snapshot(
            // The generation is stamped when the capture starts; this request
            // only names its target.
            MuxSnapshotRequest {
                provider,
                connection_id: host,
                feedback,
                generation: 0,
            },
            cx,
        );
    }

    fn request_mux_snapshot(&mut self, request: MuxSnapshotRequest, cx: &mut Context<Self>) {
        let state = &mut self.host_tmux.mux;
        // Coalescing is by target, not by the full request: every started
        // capture carries its own generation, so the running slot never equals
        // a request that arrived later.
        if state
            .snapshot_running
            .as_ref()
            .is_some_and(|running| running.same_target(&request))
            || state
                .snapshot_pending
                .as_ref()
                .is_some_and(|pending| pending.same_target(&request))
        {
            return;
        }
        if state.snapshot_running.is_some() {
            // Tool selection, the sidebar becoming visible and the refresh
            // button can all ask for the same sample; keep only the newest.
            if request.feedback.should_toast() {
                cx.emit(HostToolsEvent::ShowNotice(
                    HostToolsNotice::MuxSnapshotAlreadyRunning,
                ));
            }
            state.snapshot_pending = Some(request);
            return;
        }
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            state.snapshot = Some(MuxPanel::failed_snapshot());
            cx.notify();
            return;
        };
        self.start_mux_snapshot(request, runtime, cx);
    }

    fn start_mux_snapshot(
        &mut self,
        request: MuxSnapshotRequest,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        let provider = request.provider;
        let windows = self.mux_windows(&request.connection_id);
        if self.host_tmux.mux.host != request.connection_id {
            // A different connection must not keep showing another's sessions
            // or the subtrees captured for them.
            self.host_tmux.mux.host = request.connection_id.clone();
            self.host_tmux.mux.snapshot = None;
            self.host_tmux.mux.clear_session_tree();
        }
        let request = MuxSnapshotRequest {
            generation: self.host_tmux.mux.next_generation(),
            ..request
        };
        MuxPanel::cancel(&mut self.host_tmux.mux.snapshot_task);
        self.host_tmux.mux.snapshot_running = Some(request.clone());
        let spawned = self.spawn_mux_snapshot_capture(
            provider.list_command(windows),
            request,
            HOST_MUX_SNAPSHOT_TIMEOUT,
            HOST_MUX_SNAPSHOT_MAX_OUTPUT_SIZE,
            runtime,
        );
        match spawned {
            Some(task) => self.host_tmux.mux.snapshot_task = Some(task),
            None => {
                self.host_tmux.mux.snapshot_running = None;
                // The capture never started (no live target); end the loading
                // state so the panel shows a failure the user can retry.
                self.host_tmux.mux.snapshot = Some(MuxPanel::failed_snapshot());
            }
        }
        cx.notify();
    }

    pub(super) fn finish_host_mux_snapshot(
        &mut self,
        delivery: MuxSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_tmux.mux.snapshot_running.as_ref() != Some(&delivery.request) {
            // An answer for a capture the user already replaced must not even
            // leave its remote bytes behind un-wiped.
            discard_mux_capture(delivery.result);
            return;
        }
        let Some(provider) = self.host_tmux.mux.provider else {
            discard_mux_capture(delivery.result);
            return;
        };
        self.host_tmux.mux.snapshot_running = None;
        self.host_tmux.mux.snapshot_task = None;
        let snapshot = match delivery.result {
            Ok(mut output) if !output.truncated => {
                let snapshot =
                    provider.parse_sessions(&output.stdout, &output.stderr, output.exit_code);
                output.stdout.zeroize();
                output.stderr.zeroize();
                snapshot
            }
            // A truncated or failed capture is answered with the localized
            // fallback, never with the remote stream.
            Ok(mut output) => {
                output.stdout.zeroize();
                output.stderr.zeroize();
                MuxPanel::failed_snapshot()
            }
            Err(()) => MuxPanel::failed_snapshot(),
        };
        let expanded = self.host_tmux.mux.session.clone();
        let connection_id = delivery.request.connection_id.clone();
        let keep_expanded = snapshot.status == MuxStatus::Available
            && expanded
                .as_deref()
                .is_some_and(|session| snapshot.sessions.iter().any(|entry| entry.name == session));
        // The session list changed, so every cached subtree is stale. The
        // subtree capture that may still be running read the list this refresh
        // replaces, so it is aborted with its slot: its answer must neither
        // repopulate the cache nor free the slot of the capture that follows.
        self.host_tmux.mux.details.clear();
        self.host_tmux.mux.detail_failed.clear();
        MuxPanel::cancel(&mut self.host_tmux.mux.detail_task);
        self.host_tmux.mux.detail_running = None;
        self.host_tmux.mux.snapshot = Some(snapshot);
        self.host_tmux.mux.sessions_refreshed_ms = Some(mux_now_ms());
        if !keep_expanded {
            self.host_tmux.mux.session = None;
            self.host_tmux.mux.expanded.clear();
        }
        // A request that arrived mid-flight reuses the same bounded channel;
        // dropping it would leave the panel stale after a switch.
        let pending = self.host_tmux.mux.snapshot_pending.take();
        if let Some(pending) = pending
            && let Some(runtime) = self.lifecycle_runtime.clone()
        {
            self.start_mux_snapshot(pending, runtime, cx);
        } else if keep_expanded
            && let Some(session) = expanded
            && let Some(runtime) = self.lifecycle_runtime.clone()
        {
            self.request_mux_detail(connection_id, session, runtime, cx);
        }
        cx.notify();
    }

    // ---- session subtrees ----

    /// Expanding a session loads its tree from that session's own endpoint;
    /// collapsing keeps the cache so re-opening is free.
    fn toggle_mux_session(&mut self, session: &str, cx: &mut Context<Self>) {
        if self.host_tmux.mux.session.as_deref() == Some(session) {
            self.host_tmux.mux.session = None;
            cx.notify();
            return;
        }
        self.host_tmux.mux.session = Some(session.to_string());
        self.host_tmux.mux.expanded.clear();
        cx.notify();
        if self.host_tmux.mux.details.contains_key(session) {
            return;
        }
        let Some(connection_id) =
            (!self.host_tmux.mux.host.is_empty()).then(|| self.host_tmux.mux.host.clone())
        else {
            return;
        };
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            return;
        };
        self.request_mux_detail(connection_id, session.to_string(), runtime, cx);
    }

    fn request_mux_detail(
        &mut self,
        connection_id: String,
        session: String,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        let Some(provider) = self.host_tmux.mux.provider else {
            return;
        };
        let request = MuxDetailRequest {
            provider,
            connection_id,
            session,
            generation: 0,
        };
        if self
            .host_tmux
            .mux
            .detail_running
            .as_ref()
            .is_some_and(|running| running.same_target(&request))
        {
            return;
        }
        let windows = self.mux_windows(&request.connection_id);
        let Some(command) = provider.detail_command(windows, &request.session) else {
            self.host_tmux.mux.detail_failed.insert(request.session);
            cx.notify();
            return;
        };
        let request = MuxDetailRequest {
            generation: self.host_tmux.mux.next_generation(),
            ..request
        };
        self.host_tmux.mux.detail_failed.remove(&request.session);
        MuxPanel::cancel(&mut self.host_tmux.mux.detail_task);
        self.host_tmux.mux.detail_running = Some(request.clone());
        let spawned = self.spawn_mux_detail_capture(
            command,
            request.clone(),
            HOST_MUX_DETAIL_TIMEOUT,
            HOST_MUX_DETAIL_MAX_OUTPUT_SIZE,
            runtime,
        );
        match spawned {
            Some(task) => self.host_tmux.mux.detail_task = Some(task),
            None => {
                self.host_tmux.mux.detail_running = None;
                self.host_tmux.mux.detail_failed.insert(request.session);
            }
        }
        cx.notify();
    }

    pub(super) fn finish_host_mux_detail(
        &mut self,
        delivery: MuxDetailDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_tmux.mux.detail_running.as_ref() != Some(&delivery.request) {
            discard_mux_capture(delivery.result);
            return;
        }
        let session = delivery.request.session;
        self.host_tmux.mux.detail_running = None;
        self.host_tmux.mux.detail_task = None;
        let tree = match delivery.result {
            Ok(mut output) if !output.truncated && output.exit_code == Some(0) => {
                let tree = delivery.request.provider.parse_tree(&output.stdout);
                output.stdout.zeroize();
                output.stderr.zeroize();
                tree
            }
            Ok(mut output) => {
                output.stdout.zeroize();
                output.stderr.zeroize();
                None
            }
            Err(()) => None,
        };
        match tree {
            Some(tree) => {
                self.host_tmux.mux.detail_failed.remove(&session);
                self.host_tmux.mux.details.insert(session, tree);
            }
            None => {
                self.host_tmux.mux.details.remove(&session);
                self.host_tmux.mux.detail_failed.insert(session);
            }
        }
        cx.notify();
    }

    // ---- destructive actions (confirm) ----

    pub(super) fn open_mux_confirm(
        &mut self,
        request: MuxActionRequest,
        cx: &mut Context<Self>,
    ) -> Option<HostToolsNotice> {
        if self.host_tmux.mux.action_running.is_some() {
            return Some(HostToolsNotice::MuxActionAlreadyRunning);
        }
        HostToolConfirmState::open(&mut self.host_tmux.mux.confirm, request);
        cx.notify();
        None
    }

    pub fn mux_confirm_is_open(&self) -> bool {
        self.mux_confirm_view().is_some()
    }

    fn mux_confirm_view(&self) -> Option<(MuxActionRequest, oxideterm_gpui_ui::motion::ExitPhase)> {
        self.host_tmux
            .mux
            .confirm
            .as_ref()
            .map(|state| (state.request.clone(), state.presence.phase()))
    }

    pub(crate) fn dismiss_mux_confirm(&mut self, cx: &mut Context<Self>) {
        if self.host_tmux.mux.confirm.take().is_some() {
            cx.notify();
        }
    }

    pub fn begin_mux_confirm_exit(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        let Some(generation) = self
            .host_tmux
            .mux
            .confirm
            .as_mut()
            .and_then(|state| state.presence.begin_exit())
        else {
            return false;
        };
        if delay.is_zero() {
            self.host_tmux.mux.confirm = None;
            cx.notify();
            return true;
        }
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |entity, cx| {
                if entity
                    .host_tmux
                    .mux
                    .confirm
                    .as_ref()
                    .is_some_and(|state| state.presence.finish_exit(generation))
                {
                    entity.host_tmux.mux.confirm = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
        true
    }

    pub fn confirm_mux_action_from_view(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            self.begin_mux_confirm_exit(delay, cx);
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::MuxConnectionMissing,
            ));
            return;
        };
        let Some(request) = self
            .host_tmux
            .mux
            .confirm
            .as_ref()
            .map(|state| state.request.clone())
        else {
            return;
        };
        if !self.begin_mux_confirm_exit(delay, cx) {
            return;
        }
        for notice in self.run_mux_action(request, runtime, cx) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    /// Builds the verb command for one request and reserves the single
    /// mutating slot. User-authored labels and commands never reach the audit.
    fn run_mux_action(
        &mut self,
        request: MuxActionRequest,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        if self.host_tmux.mux.action_running.is_some() {
            return vec![HostToolsNotice::MuxActionAlreadyRunning];
        }
        let windows = self.mux_windows(&request.connection_id);
        let Some(command) = request.provider.verb_command(
            windows,
            &request.session,
            request.node.as_ref(),
            &request.verb,
        ) else {
            return vec![HostToolsNotice::MuxActionFailed];
        };
        self.start_mux_action(request, command, runtime, cx)
    }

    fn start_mux_action(
        &mut self,
        request: MuxActionRequest,
        command: zeroize::Zeroizing<String>,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        if self.connection_os_type(&request.connection_id).is_none() {
            return vec![HostToolsNotice::MuxConnectionMissing];
        }
        let MuxActionRequest {
            provider,
            connection_id,
            session,
            target_label,
            ..
        } = request;
        // A target label is not an identity: two rows can share one, and a
        // provider switch keeps the label while changing what it names. The
        // slot records the whole request plus a fresh generation, so only this
        // run's completion frees it and an older answer cannot speak for it.
        let run = MuxActionRun {
            provider,
            connection_id,
            session,
            target_label,
            generation: self.host_tmux.mux.next_generation(),
        };
        self.host_tmux.mux.action_running = Some(run.clone());
        let spawned = self.spawn_mux_action(
            provider,
            command,
            run,
            HOST_MUX_ACTION_TIMEOUT,
            HOST_MUX_ACTION_MAX_OUTPUT_SIZE,
            runtime,
        );
        match spawned {
            Some(task) => self.host_tmux.mux.action_task = Some(task),
            None => {
                self.host_tmux.mux.action_running = None;
                return vec![HostToolsNotice::MuxConnectionMissing];
            }
        }
        cx.notify();
        Vec::new()
    }

    pub(super) fn finish_host_mux_action(
        &mut self,
        delivery: MuxActionDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_tmux.mux.action_running.as_ref() != Some(&delivery.request) {
            return;
        }
        self.host_tmux.mux.action_running = None;
        self.host_tmux.mux.action_task = None;
        let MuxActionDelivery { request, result } = delivery;
        cx.emit(HostToolsEvent::ShowNotice(match result {
            Ok(true) => HostToolsNotice::MuxActionSucceeded {
                target_label: request.target_label,
            },
            Ok(false) | Err(()) => HostToolsNotice::MuxActionFailed,
        }));
        // Sample the mutated connection only while this tool is what the user
        // is looking at and still showing that connection; a hidden, switched
        // or moved panel refreshes on entry instead.
        if self.monitoring.tmux_enabled
            && self.visibility.sidebar_is_visible()
            && self.active_tool() == ContextSidebarTool::Tmux
            && self.host_tmux.mux.provider.is_some()
            && self.host_tmux.mux.host == request.connection_id
        {
            self.refresh_mux_sessions(request.connection_id, HostSnapshotFeedback::Silent, cx);
        }
        cx.notify();
    }

    pub fn mux_action_running(&self) -> bool {
        self.host_tmux.mux.action_running.is_some()
    }

    /// Session that toolbar actions apply to: the expanded one, otherwise the
    /// first listed session.
    fn mux_action_session(&self) -> Option<String> {
        self.host_tmux.mux.session.clone().or_else(|| {
            self.host_tmux.mux.snapshot.as_ref().and_then(|snapshot| {
                snapshot
                    .sessions
                    .first()
                    .map(|session| session.name.clone())
            })
        })
    }

    // ---- pane reads ----

    fn request_mux_read(&mut self, session: &str, node: &MuxNode, cx: &mut Context<Self>) {
        if self
            .host_tmux
            .mux
            .read
            .as_ref()
            .is_some_and(|dialog| dialog.loading)
        {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::MuxReadAlreadyRunning,
            ));
            return;
        }
        let Some(provider) = self.host_tmux.mux.provider else {
            return;
        };
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::MuxConnectionMissing,
            ));
            return;
        };
        if self.host_tmux.mux.host.is_empty() {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::MuxConnectionMissing,
            ));
            return;
        }
        let connection_id = self.host_tmux.mux.host.clone();
        let windows = self.mux_windows(&connection_id);
        let Some(command) = provider.read_command(windows, session, node) else {
            cx.emit(HostToolsEvent::ShowNotice(HostToolsNotice::MuxReadFailed));
            return;
        };
        // Closing a read viewer and reopening the same pane requests the same
        // buffer again; only a fresh generation tells the two captures apart.
        let request = MuxReadRequest {
            provider,
            connection_id,
            session: session.to_string(),
            target_id: node.id.clone(),
            target_label: node.label.clone(),
            generation: self.host_tmux.mux.next_generation(),
        };
        // The read viewer owns the screen from here. The panel's search field
        // is the shared tool input, so without this its focus survives the
        // modal and a keystroke the viewer does not consume edits the query.
        if self.ui.input_is_focused(HostToolsTextInput::TmuxSearch) {
            self.ui.clear_input_focus();
        }
        self.host_tmux.mux.read = Some(MuxReadDialog {
            request: request.clone(),
            output: None,
            failed: false,
            loading: true,
        });
        MuxPanel::cancel(&mut self.host_tmux.mux.read_task);
        let spawned = self.spawn_mux_read(
            command,
            request,
            HOST_MUX_READ_TIMEOUT,
            HOST_MUX_READ_MAX_OUTPUT_SIZE,
            runtime,
        );
        match spawned {
            Some(task) => self.host_tmux.mux.read_task = Some(task),
            None => {
                self.host_tmux.mux.read = None;
                cx.emit(HostToolsEvent::ShowNotice(
                    HostToolsNotice::MuxConnectionMissing,
                ));
            }
        }
        cx.notify();
    }

    pub fn dismiss_mux_read_dialog(&mut self, cx: &mut Context<Self>) {
        // The capture belongs to this viewer: closing it stops the fetch
        // instead of letting scrollback arrive for a dialog the user left.
        MuxPanel::cancel(&mut self.host_tmux.mux.read_task);
        if self.host_tmux.mux.read.take().is_some() {
            cx.notify();
        }
    }
    pub(super) fn finish_host_mux_read(
        &mut self,
        delivery: MuxReadDelivery,
        cx: &mut Context<Self>,
    ) {
        let matched = self
            .host_tmux
            .mux
            .read
            .as_ref()
            .is_some_and(|dialog| dialog.request == delivery.request);
        if !matched {
            // Pane scrollback can hold whatever the user typed into that pane.
            discard_mux_capture(delivery.result);
            return;
        }
        let Some(dialog) = self.host_tmux.mux.read.as_mut() else {
            discard_mux_capture(delivery.result);
            return;
        };
        dialog.loading = false;
        self.host_tmux.mux.read_task = None;
        match delivery.result {
            Ok(mut output) if !output.truncated && output.exit_code == Some(0) => {
                output.stderr.zeroize();
                // The last Entity/render owner clears the retained buffer.
                dialog.output = Some(Arc::new(zeroize::Zeroizing::new(std::mem::take(
                    &mut output.stdout,
                ))));
                dialog.failed = false;
            }
            // Failed output is never rendered or copied into an error string.
            Ok(mut output) => {
                output.stdout.zeroize();
                output.stderr.zeroize();
                dialog.output = None;
                dialog.failed = true;
            }
            Err(()) => {
                dialog.output = None;
                dialog.failed = true;
            }
        }
        cx.notify();
    }

    pub fn mux_read_is_open(&self) -> bool {
        self.mux_read_dialog().is_some()
    }

    fn mux_read_dialog(&self) -> Option<MuxReadDialog> {
        self.host_tmux.mux.read.clone()
    }

    pub fn mux_windows(&self, connection_id: &str) -> bool {
        self.connection_os_type(connection_id)
            .is_some_and(|os| os.eq_ignore_ascii_case("windows"))
    }
}

fn mux_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn format_mux_clock(ms: u64) -> String {
    chrono::Local
        .timestamp_millis_opt(ms as i64)
        .single()
        .map(|time| time.format("%H:%M:%S").to_string())
        .unwrap_or_default()
}

/// The provider's level names (`tab`, `pane`) stay untranslated, and so does
/// the choice between one and many: the caller passes the locale's singular
/// and plural forms and the count picks between them, which keeps an English
/// plural marker out of every other language.
fn mux_count_label(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("{count} {one}")
    } else {
        format!("{count} {many}")
    }
}

/// A verb is laid out from its payload-free kind, so the probe payload has to
/// survive the model's grammar check without meaning anything to the provider.
fn mux_probe_verb(kind: MuxVerbKind) -> MuxVerb {
    match kind {
        MuxVerbKind::Focus => MuxVerb::Focus,
        MuxVerbKind::CreateChild => MuxVerb::CreateChild,
        MuxVerbKind::CreateWorkspace => MuxVerb::CreateWorkspace {
            label: MUX_VERB_PROBE.to_string(),
            cwd: None,
        },
        MuxVerbKind::Rename => MuxVerb::Rename {
            label: MUX_VERB_PROBE.to_string(),
        },
        MuxVerbKind::Run => MuxVerb::Run {
            command: zeroize::Zeroizing::new(MUX_VERB_PROBE.to_string()),
        },
        MuxVerbKind::Read => MuxVerb::Read,
        MuxVerbKind::Close => MuxVerb::Close,
        MuxVerbKind::Stop => MuxVerb::Stop,
        MuxVerbKind::Delete => MuxVerb::Delete,
    }
}

/// Output that arrived for a capture the panel no longer waits for still holds
/// remote text, which can contain anything the user typed into a pane, so it is
/// wiped here rather than dropped along with the delivery.
fn discard_mux_capture(result: Result<oxideterm_ssh::SshCommandOutput, ()>) {
    if let Ok(mut output) = result {
        output.stdout.zeroize();
        output.stderr.zeroize();
    }
}

pub fn mux_row_title(provider: Multiplexer, session: &str) -> String {
    // The localized label is the fallback for an unnamed launch; the provider
    // name and session carry everything the tab needs to identify itself.
    format!("{} · {}", provider.name(), session)
}

/// Search work for one render pass. The query is normalized once and every
/// loaded subtree is walked once, so the rows below read a recorded answer
/// instead of re-testing the same fields for the same node per decision.
struct MuxSearchPass {
    /// Trimmed and lowercased; an empty query filters nothing.
    query: String,
    /// Nodes whose own values or descendants match. The trees are borrowed
    /// from the model for the whole pass and are not mutated while it renders,
    /// so a node address keeps naming the same row.
    matched: HashSet<*const MuxNode>,
}

impl MuxSearchPass {
    /// Walks the subtrees of the listed sessions only: a tree cached for a
    /// session the provider no longer lists cannot be rendered, and a panel
    /// without a snapshot has nothing to filter.
    pub fn new(
        raw_query: &str,
        snapshot: Option<&MuxSnapshot>,
        details: &HashMap<String, Vec<MuxNode>>,
    ) -> Self {
        let query = mux_normalize_query(raw_query);
        let mut matched = HashSet::new();
        if !query.is_empty()
            && let Some(snapshot) = snapshot
        {
            for session in &snapshot.sessions {
                if let Some(tree) = details.get(&session.name) {
                    mux_record_matches(tree, &query, &mut matched);
                }
            }
        }
        Self { query, matched }
    }

    /// An empty search hides nothing, so every row renders unfiltered.
    fn is_active(&self) -> bool {
        !self.query.is_empty()
    }

    /// A session is listed when its name matches or when the search reached
    /// anything in its loaded subtree. A session nobody expanded has no
    /// subtree captured, so only its name can answer for it.
    fn matches_session(&self, session: &MuxSession, tree: Option<&Vec<MuxNode>>) -> bool {
        if !self.is_active() {
            return true;
        }
        session.name.to_lowercase().contains(self.query.as_str())
            || tree.is_some_and(|tree| tree.iter().any(|node| self.matches_subtree(node)))
    }

    /// Whether this row or anything below it matches, which is what keeps a
    /// branch listed while its own values say nothing.
    fn matches_subtree(&self, node: &MuxNode) -> bool {
        !self.is_active() || self.matched.contains(&std::ptr::from_ref(node))
    }

    /// Whether the search reached anything inside these children: that is what
    /// reveals a branch the user never expanded. An inactive search reveals
    /// nothing, because there is nothing it had to reveal.
    fn matches_inside(&self, nodes: &[MuxNode]) -> bool {
        self.is_active() && nodes.iter().any(|node| self.matches_subtree(node))
    }
}

/// The model's matchers normalize their own input, so the panel lowers the raw
/// text once per pass instead of once per tested row; a normalized query still
/// costs the matcher one cheap re-normalization per row, which a normalized
/// entry point in the model would remove.
fn mux_normalize_query(query: &str) -> String {
    query.trim().to_lowercase()
}

/// Records every node the normalized query reaches, answering the model's field
/// matcher at most once per node: when a descendant matched, the row is visible
/// either way, so its own fields are never lowered twice.
fn mux_record_matches(
    nodes: &[MuxNode],
    query: &str,
    matched: &mut HashSet<*const MuxNode>,
) -> bool {
    let mut visible = false;
    for node in nodes {
        let descendants = mux_record_matches(&node.children, query, matched);
        if descendants || node_matches_query(node, query) {
            matched.insert(std::ptr::from_ref(node));
            visible = true;
        }
    }
    visible
}

#[cfg(test)]
mod mux_search_pass_tests {
    use super::*;

    fn node(kind: MuxKind, id: &str, label: &str, children: Vec<MuxNode>) -> MuxNode {
        MuxNode {
            kind,
            id: id.to_string(),
            label: label.to_string(),
            children,
            focused: false,
            status: None,
            agent: None,
            title: None,
            cwd: None,
            orphan_agent: false,
            restore_failed: false,
            tab_count: 0,
            pane_count: 0,
        }
    }

    /// One workspace whose tab, not the workspace itself, carries the text.
    fn tree(label: &str) -> Vec<MuxNode> {
        vec![node(
            MuxKind::Workspace,
            "wA",
            label,
            vec![node(MuxKind::Tab, "t1", "Helper Window", Vec::new())],
        )]
    }

    fn captured(session: &str, tree: Vec<MuxNode>) -> (MuxSnapshot, HashMap<String, Vec<MuxNode>>) {
        let snapshot = MuxSnapshot {
            status: MuxStatus::Available,
            sessions: vec![MuxSession {
                name: session.to_string(),
                running: true,
            }],
        };
        let mut details = HashMap::new();
        details.insert(session.to_string(), tree);
        (snapshot, details)
    }

    #[test]
    fn deep_match_answers_reveal_and_filter_from_one_walk() {
        let (snapshot, details) = captured("sess", tree("alpha"));
        let search = MuxSearchPass::new("  HELPER ", Some(&snapshot), &details);
        let roots = details.get("sess").unwrap();
        assert!(search.is_active());
        // The session is listed, the branch stays visible and the matched row
        // itself is visible: all three read the same recorded answer.
        assert!(search.matches_session(&snapshot.sessions[0], Some(roots)));
        assert!(search.matches_inside(roots));
        assert!(search.matches_subtree(&roots[0]));
        assert!(search.matches_subtree(&roots[0].children[0]));
    }
}

/// The verb's icon and copy, resolved from the tree level the verb runs on.
fn mux_verb_view(
    kind: MuxVerbKind,
    node_kind: Option<MuxKind>,
) -> (&'static str, &'static str, bool) {
    use MuxKind::*;
    use MuxVerbKind::*;
    match kind {
        Focus => ("lucide/eye.svg", "sidebar.multiplexer.focus", false),
        CreateChild => (
            "lucide/plus.svg",
            match node_kind {
                // A child of a workspace or of a session is a tab; below a tab
                // or a pane it is a split.
                None | Some(Workspace) => "sidebar.multiplexer.new_tab",
                Some(Tab) | Some(Pane) => "sidebar.multiplexer.split",
            },
            false,
        ),
        CreateWorkspace => (
            "lucide/plus.svg",
            "sidebar.multiplexer.actions.new_workspace",
            false,
        ),
        Rename => (
            "lucide/pencil.svg",
            "sidebar.multiplexer.actions.rename",
            false,
        ),
        Run => ("lucide/play.svg", "sidebar.multiplexer.actions.send", false),
        Read => (
            "lucide/file-text.svg",
            "sidebar.multiplexer.actions.read",
            false,
        ),
        Close => ("lucide/x.svg", "sidebar.multiplexer.actions.close", true),
        Stop => (
            "lucide/x.svg",
            "sidebar.multiplexer.actions.session_stop",
            true,
        ),
        Delete => (
            "lucide/trash-2.svg",
            "sidebar.multiplexer.actions.session_delete",
            false,
        ),
    }
}

/// Tooltips key the native tooltip id, so one static id per verb is enough
/// inside a row that already owns its element id.
fn mux_verb_element_id(kind: MuxVerbKind) -> &'static str {
    match kind {
        MuxVerbKind::Focus => "host-mux-verb-focus",
        MuxVerbKind::CreateChild => "host-mux-verb-create-child",
        MuxVerbKind::CreateWorkspace => "host-mux-verb-create-workspace",
        MuxVerbKind::Rename => "host-mux-verb-rename",
        MuxVerbKind::Run => "host-mux-verb-run",
        MuxVerbKind::Read => "host-mux-verb-read",
        MuxVerbKind::Close => "host-mux-verb-close",
        MuxVerbKind::Stop => "host-mux-verb-stop",
        MuxVerbKind::Delete => "host-mux-verb-delete",
    }
}

/// The dialog kind a payload verb opens, carrying the row it renames, runs in
/// or splits. A session-level workspace needs no target.
fn mux_dialog_kind(kind: MuxVerbKind, node: Option<&MuxNode>) -> Option<MuxInputDialogKind> {
    match (kind, node) {
        (MuxVerbKind::CreateWorkspace, None) => Some(MuxInputDialogKind::NewWorkspace),
        (MuxVerbKind::Rename, Some(target)) => match target.kind {
            MuxKind::Workspace => Some(MuxInputDialogKind::RenameWorkspace {
                target: target.clone(),
            }),
            MuxKind::Tab => Some(MuxInputDialogKind::RenameTab {
                target: target.clone(),
            }),
            MuxKind::Pane => Some(MuxInputDialogKind::RenamePane {
                target: target.clone(),
            }),
        },
        (MuxVerbKind::Run, Some(target)) => Some(MuxInputDialogKind::RunPane {
            target: target.clone(),
        }),
        _ => None,
    }
}

/// Rename starts from the name the user set, never from text a shell owns.
fn mux_verb_prefill(kind: MuxVerbKind, node: Option<&MuxNode>) -> String {
    match kind {
        MuxVerbKind::Rename => node.map(mux_explicit_label).unwrap_or_default(),
        _ => String::new(),
    }
}

fn mux_explicit_label(node: &MuxNode) -> String {
    // The model resolves a row label from the user's name first, then the OSC
    // title, the agent label, the cwd and the id. A label that matches one of
    // those derived sources is not a name, so prefilling it would rename the
    // object to whatever the shell last printed.
    let derived = [
        node.title.as_deref(),
        node.agent.as_ref().and_then(|agent| agent.label.as_deref()),
        node.cwd.as_deref(),
        Some(node.id.as_str()),
    ];
    if derived
        .into_iter()
        .flatten()
        .any(|value| value == node.label)
    {
        String::new()
    } else {
        node.label.clone()
    }
}

fn mux_confirm_description_key(request: &MuxActionRequest) -> &'static str {
    match (&request.verb, request.node.as_ref().map(|node| node.kind)) {
        (MuxVerb::Close, Some(MuxKind::Workspace)) => "sidebar.multiplexer.confirm.close_workspace",
        (MuxVerb::Close, Some(MuxKind::Tab)) => "sidebar.multiplexer.confirm.close_tab",
        (MuxVerb::Close, _) => "sidebar.multiplexer.confirm.close_pane",
        (MuxVerb::Stop, _) => "sidebar.multiplexer.confirm.stop_session",
        (MuxVerb::Delete, _) => "sidebar.multiplexer.confirm.delete_session",
        _ => "sidebar.multiplexer.confirm.title",
    }
}

pub fn mux_input_title_key(kind: &MuxInputDialogKind) -> &'static str {
    match kind {
        MuxInputDialogKind::NewSession => "sidebar.multiplexer.inputs.new_session",
        MuxInputDialogKind::NewWorkspace => "sidebar.multiplexer.inputs.new_workspace",
        MuxInputDialogKind::RenameWorkspace { .. } => "sidebar.multiplexer.inputs.rename_workspace",
        MuxInputDialogKind::RenameTab { .. } => "sidebar.multiplexer.inputs.rename_tab",
        MuxInputDialogKind::RenamePane { .. } => "sidebar.multiplexer.inputs.rename_pane",
        MuxInputDialogKind::RunPane { .. } => "sidebar.multiplexer.inputs.run_pane",
    }
}

pub fn mux_input_placeholder_key(kind: &MuxInputDialogKind) -> &'static str {
    match kind {
        MuxInputDialogKind::NewSession => "sidebar.multiplexer.inputs.new_session_hint",
        MuxInputDialogKind::NewWorkspace => "sidebar.multiplexer.inputs.new_workspace_hint",
        MuxInputDialogKind::RenameWorkspace { .. }
        | MuxInputDialogKind::RenameTab { .. }
        | MuxInputDialogKind::RenamePane { .. } => "sidebar.multiplexer.inputs.rename_hint",
        MuxInputDialogKind::RunPane { .. } => "sidebar.multiplexer.inputs.run_pane_hint",
    }
}
