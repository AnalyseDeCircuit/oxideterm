//! Provider-neutral multiplexer Host Tool: session list, object tree, verb
//! dispatch, confirms, the input dialog and pane reads. The panel renders the
//! `multiplexer` model only; no provider wire type reaches this module.

use super::*;
use crate::workspace::selectable_text::SelectableTextRenderState;
use chrono::TimeZone;
use gpui::Div;
use oxideterm_connection_monitor::multiplexer::{
    Multiplexer, MuxKind, MuxNode, MuxSession, MuxSnapshot, MuxStatus, MuxVerb, MuxVerbKind,
    node_matches_query, status_label_key,
};
use oxideterm_gpui_ui::button::ButtonVariant;
use oxideterm_gpui_ui::text_input::{TextInputView, text_input_anchor_probe};
use std::collections::{HashMap, HashSet};
use zeroize::Zeroize;

/// `verb_command` validates its payload grammar, so a fixed placeholder answers
/// whether a row supports a payload verb before the user has typed anything.
const MUX_VERB_PROBE: &str = "mux verb";

/// Every multiplexer row renders its verb buttons from the model's verb table;
/// a row that is itself a detached agent only offers the agent attach.
const MUX_ROW_INDENT: f32 = 16.0;

#[derive(Default)]
pub(super) struct MuxPanel {
    /// `None` keeps the panel on the tool's own tmux list.
    pub(super) provider: Option<Multiplexer>,
    pub(super) host: String,
    pub(super) snapshot: Option<MuxSnapshot>,
    pub(super) sessions_refreshed_ms: Option<u64>,
    pub(super) snapshot_running: Option<MuxSnapshotRequest>,
    pub(super) snapshot_pending: Option<MuxSnapshotRequest>,
    /// Subtrees belong to one session, so each expanded session is captured
    /// from that session's own socket and cached under its name.
    pub(super) details: HashMap<String, Vec<MuxNode>>,
    /// Sessions whose subtree capture failed. The model gives a subtree no
    /// status of its own, so the row renders one localized failure line.
    pub(super) detail_failed: HashSet<String>,
    pub(super) detail_running: Option<MuxDetailRequest>,
    pub(super) session: Option<String>,
    pub(super) expanded: HashSet<(MuxKind, String)>,
    pub(super) confirm: Option<HostToolConfirmState<MuxActionRequest>>,
    pub(super) action_running: Option<MuxActionRun>,
    pub(super) read: Option<MuxReadDialog>,
    /// The Entity owns one live capture per query kind and aborts it when its
    /// request is superseded, when the host or provider changes, or when the
    /// viewer that asked for it closes. Remote command output must not be
    /// fetched for a panel the user already left.
    pub(super) snapshot_task: Option<tokio::task::JoinHandle<()>>,
    pub(super) detail_task: Option<tokio::task::JoinHandle<()>>,
    pub(super) action_task: Option<tokio::task::JoinHandle<()>>,
    pub(super) read_task: Option<tokio::task::JoinHandle<()>>,
    /// Never reset: a generation that repeats would let a capture that started
    /// before a host change be accepted as one that started after it.
    pub(super) generation: u64,
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

    pub(in crate::workspace::connection_monitor) fn mux_confirm_view(
        &self,
    ) -> Option<(MuxActionRequest, oxideterm_gpui_ui::motion::ExitPhase)> {
        self.host_tmux
            .mux
            .confirm
            .as_ref()
            .map(|state| (state.request.clone(), state.presence.phase()))
    }

    pub(in crate::workspace::connection_monitor) fn dismiss_mux_confirm(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.host_tmux.mux.confirm.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn begin_mux_confirm_exit(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
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

    pub(super) fn confirm_mux_action_from_view(&mut self, delay: Duration, cx: &mut Context<Self>) {
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

    pub(in crate::workspace::connection_monitor) fn mux_action_running(&self) -> bool {
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

    pub(in crate::workspace::connection_monitor) fn dismiss_mux_read_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
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

    fn mux_read_dialog(&self) -> Option<MuxReadDialog> {
        self.host_tmux.mux.read.clone()
    }

    fn mux_windows(&self, connection_id: &str) -> bool {
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

impl HostToolsEntity {
    // ---- panel ----

    /// The provider-tab branch of the multiplexer tool: one header, one search
    /// box, one status row and the session tree for the selected connection.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_mux_panel(
        &self,
        connections: &[MonitorConnectionOption],
        selected_id: &str,
        search_ime: &HostToolsPlainTextImeFrame,
        terminal_available: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &SelectableTextRenderState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let same_host = self.host_tmux.mux.host == selected_id;
        let snapshot = same_host
            .then(|| self.host_tmux.mux.snapshot.clone())
            .flatten();
        let last_refreshed_ms = same_host
            .then_some(self.host_tmux.mux.sessions_refreshed_ms)
            .flatten();
        // One search pass for the whole panel: the query is normalized once and
        // every listed subtree is walked once, so the status-row count and the
        // list below read the same match answers instead of re-testing the same
        // rows for every render decision.
        let search = MuxSearchPass::new(
            &self.ui.host_tmux_search_query,
            snapshot.as_ref(),
            &self.host_tmux.mux.details,
        );
        let visible_sessions: Vec<&MuxSession> = snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .sessions
                    .iter()
                    .filter(|session| {
                        search
                            .matches_session(session, self.host_tmux.mux.details.get(&session.name))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let header = div()
            .flex_none()
            .w_full()
            .min_w_0()
            .px_3()
            .pt_3()
            .pb_2()
            .flex()
            .flex_col()
            .gap_2()
            .border_b_1()
            .border_color(rgba((tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
            .child(self.render_connection_switcher(
                connections,
                selected_id,
                self.host_tmux.mux.snapshot_running.is_some(),
                tokens,
                mono_font_family.clone(),
                selectable_text,
                cx,
            ))
            .child(self.render_mux_tabs(cx, tokens))
            .child(self.render_host_tmux_search(search_ime, tokens, i18n, cx))
            .child(self.render_mux_status_row(
                selected_id,
                visible_sessions.len(),
                last_refreshed_ms,
                terminal_available,
                tokens,
                i18n,
                cx,
            ));
        let body = self.render_mux_body(
            selected_id,
            snapshot.as_ref(),
            &visible_sessions,
            &search,
            terminal_available,
            tokens,
            i18n,
            mono_font_family,
            selectable_text,
            cx,
        );

        div()
            .id("host-mux-panel")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(header)
            .child(body)
            .into_any_element()
    }

    pub(super) fn render_mux_tabs(
        &self,
        cx: &mut Context<Self>,
        tokens: &ThemeTokens,
    ) -> AnyElement {
        let mut tabs = oxideterm_gpui_ui::tabs::tabs_list(tokens);
        for provider in [None, Some(Multiplexer::Zellij), Some(Multiplexer::Herdr)] {
            let label = match provider {
                // The tab bar labels each provider by its display name, which
                // starts with a capital like the sidebar entry; the tool itself
                // keeps the collective name.
                None => "Tmux",
                Some(provider) => provider.name(),
            };
            tabs = tabs.child(
                oxideterm_gpui_ui::tabs::tabs_trigger(
                    tokens,
                    label.into(),
                    self.host_tmux.mux.provider == provider,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |tools, _, _, cx| {
                        if tools.host_tmux.mux.provider != provider {
                            tools.host_tmux.mux.clear_host();
                            tools.host_tmux.mux.provider = provider;
                            // A dialog or viewer opened for the previous
                            // provider describes objects this one does not
                            // have, so both close with the host state.
                            tools.dismiss_mux_input_dialog(cx);
                            tools.dismiss_mux_read_dialog(cx);
                            tools.request_active_tool_snapshot(HostSnapshotFeedback::Silent, cx);
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        tabs.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_mux_status_row(
        &self,
        host: &str,
        visible_sessions: usize,
        last_refreshed_ms: Option<u64>,
        terminal_available: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let left = match last_refreshed_ms {
            Some(ms) => format!(
                "{} {} · {} {}",
                visible_sessions,
                i18n.t("sidebar.multiplexer.sessions_title"),
                i18n.t("sidebar.multiplexer.last_refreshed"),
                format_mux_clock(ms)
            ),
            None => format!(
                "{} {}",
                visible_sessions,
                i18n.t("sidebar.multiplexer.sessions_title")
            ),
        };
        let windows = self.mux_windows(host);
        // A workspace needs a session to live in; the expanded one, otherwise
        // the first listed session.
        let context_session = self.mux_action_session();
        let new_workspace_supported = match (self.mux_provider(), context_session.as_deref()) {
            (Some(provider), Some(session)) => self.mux_verb_supported(
                provider,
                windows,
                session,
                None,
                MuxVerbKind::CreateWorkspace,
            ),
            _ => false,
        };
        let default_attach = self
            .mux_provider()
            .and_then(|provider| provider.attach_command(windows, None));
        let host = host.to_owned();
        let attach_host = host.clone();
        let new_workspace_label = i18n.t("sidebar.multiplexer.new_workspace");
        let open_label = i18n.t("sidebar.multiplexer.open");
        let attach_title = i18n.t("sidebar.multiplexer.attach_title");
        let attach_opened = i18n.t("sidebar.multiplexer.toast.attach_opened");
        let attach_missing = i18n.t("sidebar.multiplexer.toast.connection_missing");
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .text_size(px(11.0))
            .text_color(rgb(theme.text_muted))
            .child(div().flex_none().child(left))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .when(new_workspace_supported, |row| {
                        row.child(host_tools_tooltip_icon_button(
                            tokens,
                            LucideIcon::Plus,
                            13.0,
                            rgb(theme.text),
                            oxideterm_gpui_ui::button::IconButtonOptions {
                                size: 24.0,
                                disabled: self.mux_action_running(),
                                has_background: true,
                                background: Some(rgb(theme.bg_hover)),
                                hover_background: Some(rgb(theme.bg_panel)),
                                idle_opacity: 1.0,
                                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                            },
                            new_workspace_label,
                            "host-mux-new-workspace",
                            true,
                            cx.listener(move |tools, _event, window, cx| {
                                let Some(session) = tools.mux_action_session() else {
                                    cx.emit(HostToolsEvent::ShowNotice(
                                        HostToolsNotice::MuxConnectionMissing,
                                    ));
                                    return;
                                };
                                tools.open_mux_input_dialog(
                                    MuxInputDialogKind::NewWorkspace,
                                    &session,
                                    "",
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ))
                    })
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        LucideIcon::Terminal,
                        13.0,
                        rgb(theme.text),
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 24.0,
                            disabled: default_attach.is_none() || !terminal_available,
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        open_label,
                        "host-mux-open",
                        true,
                        cx.listener(move |tools, _event, window, cx| {
                            let Some(command) = default_attach.clone() else {
                                return;
                            };
                            tools.dispatch_mux_terminal(
                                &attach_host,
                                command,
                                attach_title.clone(),
                                attach_opened.clone(),
                                attach_missing.clone(),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ))
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        LucideIcon::RefreshCw,
                        13.0,
                        rgb(theme.text),
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 24.0,
                            disabled: self.host_tmux.mux.snapshot_running.is_some(),
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        i18n.t("sidebar.multiplexer.actions.refresh"),
                        "host-mux-refresh",
                        true,
                        cx.listener(move |tools, _event, _window, cx| {
                            tools.refresh_mux_sessions(
                                host.clone(),
                                HostSnapshotFeedback::Toast,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_mux_body(
        &self,
        host: &str,
        snapshot: Option<&MuxSnapshot>,
        sessions: &[&MuxSession],
        search: &MuxSearchPass,
        terminal_available: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &SelectableTextRenderState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let center = |icon: LucideIcon, color: u32, text: String, cx: &mut Context<Self>| {
            host_tools_center_state(icon, color, text, selectable_text, cx)
        };
        let Some(snapshot) = snapshot else {
            return center(
                LucideIcon::Bot,
                theme.text_muted,
                i18n.t("sidebar.multiplexer.sampling"),
                cx,
            );
        };
        match &snapshot.status {
            MuxStatus::Unavailable => {
                return center(
                    LucideIcon::Bot,
                    theme.text_muted,
                    i18n.t("sidebar.multiplexer.unavailable"),
                    cx,
                );
            }
            MuxStatus::ServerNotRunning => {
                return center(
                    LucideIcon::Bot,
                    theme.text_muted,
                    i18n.t("sidebar.multiplexer.server_not_running"),
                    cx,
                );
            }
            // The provider's own message is bounded by the model; anything else
            // falls back to the localized failure text.
            MuxStatus::Error(Some(message)) => {
                return center(
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    i18n.t("sidebar.multiplexer.error")
                        .replace("{{error}}", message),
                    cx,
                );
            }
            MuxStatus::Error(None) => {
                return center(
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    i18n.t("sidebar.multiplexer.failed"),
                    cx,
                );
            }
            MuxStatus::Available => {}
        }
        // The panel filtered these sessions through this same pass, so the
        // count in the status row and the list here cannot disagree.
        if sessions.is_empty() {
            // A query that hides every session must not read as "no sessions
            // exist", so the empty state names the search instead.
            let empty_key = if !search.is_active() {
                "sidebar.multiplexer.empty"
            } else {
                "sidebar.multiplexer.search_empty"
            };
            return center(LucideIcon::Bot, theme.text_muted, i18n.t(empty_key), cx);
        }
        let windows = self.mux_windows(host);
        let mut list = div()
            .id("host-mux-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .children(self.render_mux_sessions_header(windows, tokens, i18n, cx));
        for &session in sessions {
            list = list.child(self.render_mux_session_row(
                host,
                session,
                search,
                terminal_available,
                windows,
                tokens,
                i18n,
                mono_font_family.clone(),
                cx,
            ));
        }
        list.into_any_element()
    }

    /// Only a provider with a non-interactive create-session verb gets the
    /// header button; `new_session_command` answers that with a fixed name.
    fn render_mux_sessions_header(
        &self,
        windows: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let provider = self.mux_provider()?;
        if provider
            .new_session_command(windows, MUX_VERB_PROBE)
            .is_none()
        {
            return None;
        }
        let theme = tokens.ui;
        Some(
            div()
                .w_full()
                .px_3()
                .pt_3()
                .pb_1()
                .flex()
                .items_center()
                .justify_end()
                .gap_2()
                // The status row above already counts the sessions, so this
                // row carries only the create affordance.
                .child(host_tools_tooltip_icon_button(
                    tokens,
                    LucideIcon::Plus,
                    11.0,
                    rgb(theme.text),
                    oxideterm_gpui_ui::button::IconButtonOptions {
                        size: 18.0,
                        has_background: true,
                        background: Some(rgb(theme.bg_hover)),
                        hover_background: Some(rgb(theme.bg_panel)),
                        idle_opacity: 1.0,
                        ..oxideterm_gpui_ui::button::IconButtonOptions::compact(18.0)
                    },
                    i18n.t("sidebar.multiplexer.actions.new_session"),
                    "host-mux-new-session",
                    true,
                    cx.listener(move |tools, _event, window, cx| {
                        // A new session defines its own name; no context session.
                        tools.open_mux_input_dialog(
                            MuxInputDialogKind::NewSession,
                            "",
                            "",
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ))
                .into_any_element(),
        )
    }

    fn mux_provider(&self) -> Option<Multiplexer> {
        self.host_tmux.mux.provider
    }

    /// Whether the model can build a command for this verb at all. The panel
    /// lays a button out only when an answer exists, so the built line is
    /// dropped here; the probe payload is a fixed literal that survives the
    /// model's grammar check without meaning anything to the provider.
    fn mux_verb_supported(
        &self,
        provider: Multiplexer,
        windows: bool,
        session: &str,
        node: Option<&MuxNode>,
        kind: MuxVerbKind,
    ) -> bool {
        provider
            .verb_command(windows, session, node, &mux_probe_verb(kind))
            .is_some()
    }

    fn dispatch_mux_terminal(
        &mut self,
        host: &str,
        command: String,
        title: String,
        opened_notice: String,
        missing_notice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if host == LOCAL_HOST_CONNECTION_ID {
            window.dispatch_action(
                Box::new(HostToolsWindowRequest::new(
                    HostToolsWindowIntent::OpenLocalMultiplexer {
                        command,
                        title,
                        opened_notice,
                        missing_notice,
                    },
                )),
                cx,
            );
            // The workspace handler owns both copies: it reports the open once
            // the tab exists, so a failed launch is never covered by success.
            return;
        }
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::OpenExistingNodeTerminal {
                    connection_id: host.to_string(),
                    command,
                    title,
                    opened_notice,
                    missing_notice,
                },
            )),
            cx,
        );
    }
}

fn mux_row_title(provider: Multiplexer, session: &str) -> String {
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
    fn new(
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

    #[test]
    fn name_only_match_reveals_no_children() {
        let (snapshot, details) = captured("helper-session", tree("alpha"));
        let search = MuxSearchPass::new("helper-session", Some(&snapshot), &details);
        let roots = details.get("helper-session").unwrap();
        assert!(search.matches_session(&snapshot.sessions[0], Some(roots)));
        // The expansion under this row has nothing to list, which is the case
        // that renders the localized no-object state instead of a blank area.
        assert!(!search.matches_inside(roots));
        assert!(!search.matches_subtree(&roots[0]));
    }

    #[test]
    fn empty_query_lists_everything_and_reveals_nothing() {
        let (snapshot, details) = captured("sess", tree("alpha"));
        let search = MuxSearchPass::new("   ", Some(&snapshot), &details);
        let roots = details.get("sess").unwrap();
        assert!(!search.is_active());
        assert!(search.matches_session(&snapshot.sessions[0], Some(roots)));
        assert!(search.matches_subtree(&roots[0]));
        assert!(search.matches_subtree(&roots[0].children[0]));
        assert!(!search.matches_inside(roots));
    }

    #[test]
    fn uncaptured_session_is_judged_by_name_alone() {
        let (snapshot, details) = captured("sess", tree("alpha"));
        let search = MuxSearchPass::new("sess", Some(&snapshot), &details);
        assert!(search.matches_session(&snapshot.sessions[0], None));
        let search = MuxSearchPass::new("other", Some(&snapshot), &details);
        assert!(!search.matches_session(&snapshot.sessions[0], None));
    }

    #[test]
    fn row_identity_separates_providers_and_sessions() {
        let pane = node(MuxKind::Pane, "0", "shell", Vec::new());
        let zellij = HostToolsEntity::mux_row_identity(Multiplexer::Zellij, "s", Some(&pane));
        assert_eq!(
            zellij,
            HostToolsEntity::mux_row_identity(Multiplexer::Zellij, "s", Some(&pane))
        );
        assert_ne!(
            zellij,
            HostToolsEntity::mux_row_identity(Multiplexer::Herdr, "s", Some(&pane))
        );
        assert_ne!(
            zellij,
            HostToolsEntity::mux_row_identity(Multiplexer::Zellij, "other", Some(&pane))
        );
        assert_ne!(
            HostToolsEntity::mux_row_identity(Multiplexer::Zellij, "s", None),
            HostToolsEntity::mux_row_identity(Multiplexer::Herdr, "s", None)
        );
    }

    #[test]
    fn count_label_picks_the_locale_forms() {
        assert_eq!(mux_count_label(1, "Tab", "Tabs"), "1 Tab");
        assert_eq!(mux_count_label(2, "Tab", "Tabs"), "2 Tabs");
        // A translated unit never carries an appended English plural marker.
        assert_eq!(mux_count_label(3, "个标签", "个标签"), "3 个标签");
    }
}

impl HostToolsEntity {
    // ---- session rows ----

    /// Element identity for a multiplexer row. The provider and the owning
    /// session have to be part of it: the provider tabs reuse the same session
    /// names and two trees can hand out the same object id, so a shorter key
    /// would carry hover, focus and expansion state onto a row that is not the
    /// same row.
    fn mux_row_identity(provider: Multiplexer, session: &str, node: Option<&MuxNode>) -> String {
        match node {
            Some(node) => format!(
                "{}:{}:{:?}:{}",
                provider.name(),
                session,
                node.kind,
                node.id
            ),
            None => format!("{}:{}", provider.name(), session),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_mux_session_row(
        &self,
        host: &str,
        session: &MuxSession,
        search: &MuxSearchPass,
        terminal_available: bool,
        windows: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let Some(provider) = self.mux_provider() else {
            return div().into_any_element();
        };
        let details = self.host_tmux.mux.details.get(&session.name);
        // A search reveals the branches it matched without touching the
        // click-managed expansion state.
        let descendant_match = details.is_some_and(|tree| search.matches_inside(tree));
        let expanded = self.host_tmux.mux.session.as_deref() == Some(session.name.as_str())
            || descendant_match;
        // An attach starts a stopped session, so the action stays enabled while
        // only the name dims to read as inactive.
        let name_color = if session.running {
            theme.text
        } else {
            theme.text_muted
        };
        let attach_command = provider.attach_command(windows, Some(&session.name));
        let attach_label = i18n.t("sidebar.multiplexer.actions.session_attach");
        let attach_opened = i18n.t("sidebar.multiplexer.toast.attach_opened");
        let attach_missing = i18n.t("sidebar.multiplexer.toast.connection_missing");
        let attach_title = mux_row_title(provider, &session.name);
        let attach_host = host.to_owned();
        let row_host = host.to_owned();
        let session_name = session.name.clone();
        let header = div()
            .id(host_tools_row_id(
                "host-mux-session",
                host,
                Self::mux_row_identity(provider, &session.name, None),
            ))
            .w_full()
            .min_w_0()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(theme.bg_hover)))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .h(px(34.0))
                    .pl(px(10.0))
                    .pr_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div().flex_none().child(
                            svg()
                                .path(if expanded {
                                    LucideIcon::ChevronDown.path()
                                } else {
                                    LucideIcon::ChevronRight.path()
                                })
                                .size(px(11.0))
                                .text_color(rgb(theme.text_muted)),
                        ),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE))
                            .text_color(rgb(name_color))
                            .child(session.name.clone()),
                    )
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        LucideIcon::Cable,
                        12.0,
                        rgb(theme.text),
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 22.0,
                            disabled: attach_command.is_none() || !terminal_available,
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                        },
                        attach_label,
                        "host-mux-session-attach",
                        true,
                        cx.listener(move |tools, _event, window, cx| {
                            let Some(command) = attach_command.clone() else {
                                return;
                            };
                            tools.dispatch_mux_terminal(
                                &attach_host,
                                command,
                                attach_title.clone(),
                                attach_opened.clone(),
                                attach_missing.clone(),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ))
                    .children(self.mux_verb_buttons(
                        &row_host,
                        provider,
                        &session.name,
                        None,
                        session.running,
                        windows,
                        12.0,
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let session_name = session.name.clone();
                    move |tools, _event, _window, cx| {
                        tools.toggle_mux_session(&session_name, cx);
                        cx.stop_propagation();
                    }
                }),
            );

        let mut container = div().w_full().min_w_0().flex().flex_col().child(header);
        if expanded {
            container = container.children(self.render_mux_session_children(
                host,
                provider,
                &session_name,
                search,
                terminal_available,
                windows,
                tokens,
                i18n,
                mono_font_family,
                cx,
            ));
        }
        container.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_mux_session_children(
        &self,
        host: &str,
        provider: Multiplexer,
        session: &str,
        search: &MuxSearchPass,
        terminal_available: bool,
        windows: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let theme = tokens.ui;
        let muted_child = |text: String| {
            div()
                .w_full()
                .min_w_0()
                .pl(px(45.0))
                .pr_3()
                .py(px(6.0))
                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                .text_color(rgb(theme.text_muted))
                .child(text)
                .into_any_element()
        };
        if self.host_tmux.mux.detail_failed.contains(session) {
            return vec![muted_child(i18n.t("sidebar.multiplexer.failed"))];
        }
        let Some(tree) = self.host_tmux.mux.details.get(session) else {
            // Expanding requests the subtree, and a refresh re-requests the
            // session that is still open.
            return vec![muted_child(i18n.t("sidebar.multiplexer.sampling"))];
        };
        let roots: Vec<&MuxNode> = tree
            .iter()
            .filter(|node| search.matches_subtree(node))
            .collect();
        if roots.is_empty() {
            // A query that only reached the session name has nothing to show
            // below it, and a blank area reads as a failed capture, so the
            // expansion says the search found no object here. Without a query
            // the session really is empty and keeps its own empty state.
            return if search.is_active() {
                vec![muted_child(
                    i18n.t("sidebar.multiplexer.search_child_empty"),
                )]
            } else {
                vec![muted_child(i18n.t("sidebar.multiplexer.workspace_empty"))]
            };
        }
        roots
            .into_iter()
            .map(|node| {
                self.render_mux_node(
                    host,
                    provider,
                    session,
                    node,
                    0,
                    search,
                    terminal_available,
                    windows,
                    tokens,
                    i18n,
                    mono_font_family.clone(),
                    cx,
                )
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_mux_node(
        &self,
        host: &str,
        provider: Multiplexer,
        session: &str,
        node: &MuxNode,
        depth: usize,
        search: &MuxSearchPass,
        terminal_available: bool,
        windows: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let key = (node.kind, node.id.clone());
        // A match below this row reveals the branch without a click.
        let descendant_match = search.matches_inside(&node.children);
        let expanded = self.host_tmux.mux.expanded.contains(&key) || descendant_match;
        let show_chevron = !node.children.is_empty();
        // The cwd stays visible as a hint unless it already is the label.
        let cwd_hint = node
            .cwd
            .clone()
            .filter(|cwd| cwd.as_str() != node.label.as_str());
        let agent_attach = node.orphan_agent.then(|| {
            (
                provider.agent_attach_command(windows, session, node),
                i18n.t("sidebar.multiplexer.actions.agent_attach"),
                mux_row_title(provider, session),
                i18n.t("sidebar.multiplexer.toast.attach_opened"),
                i18n.t("sidebar.multiplexer.toast.connection_missing"),
                host.to_owned(),
            )
        });
        let row_host = host.to_owned();
        let row_session = session.to_owned();
        let mono = mono_font_family.clone();
        let mut line = div()
            .w_full()
            .min_w_0()
            .when(node.kind != MuxKind::Pane, |inner| {
                inner.h(px(if node.kind == MuxKind::Workspace {
                    36.0
                } else {
                    32.0
                }))
            })
            .when(node.kind == MuxKind::Pane, |inner| inner.py(px(6.0)))
            .pl(px(10.0 + (depth as f32 + 1.0) * MUX_ROW_INDENT))
            .pr_3()
            .flex()
            .items_center()
            .gap_2()
            .when(show_chevron, |inner| {
                inner.child(
                    div().flex_none().child(
                        svg()
                            .path(if expanded {
                                LucideIcon::ChevronDown.path()
                            } else {
                                LucideIcon::ChevronRight.path()
                            })
                            .size(px(11.0))
                            .text_color(rgb(theme.text_muted)),
                    ),
                )
            })
            .child(WorkspaceApp::render_lucide_icon(
                match node.kind {
                    MuxKind::Workspace => LucideIcon::Folder,
                    MuxKind::Tab => LucideIcon::Monitor,
                    MuxKind::Pane => LucideIcon::Terminal,
                },
                13.0,
                rgb(theme.text_muted),
            ))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_size(px(HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE))
                    .text_color(rgb(theme.text))
                    .when(node.kind == MuxKind::Pane, |label| {
                        label.font_family(mono.clone())
                    })
                    .child(node.label.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(10.0))
                    .text_color(rgb(theme.text_muted))
                    .child(i18n.t(match node.kind {
                        MuxKind::Workspace => "sidebar.multiplexer.row_space",
                        MuxKind::Tab => "sidebar.multiplexer.row_tab",
                        MuxKind::Pane if node.orphan_agent => "sidebar.multiplexer.orphan_agent",
                        MuxKind::Pane => "sidebar.multiplexer.row_pane",
                    })),
            )
            .when(node.tab_count > 0, |inner| {
                inner.child(
                    div()
                        .flex_none()
                        .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                        .text_color(rgb(theme.text_muted))
                        .child(mux_count_label(
                            node.tab_count,
                            &i18n.t("sidebar.multiplexer.tabs_suffix_one"),
                            &i18n.t("sidebar.multiplexer.tabs_suffix"),
                        )),
                )
            })
            .when(node.pane_count > 0, |inner| {
                inner.child(
                    div()
                        .flex_none()
                        .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                        .text_color(rgb(theme.text_muted))
                        .child(mux_count_label(
                            node.pane_count,
                            &i18n.t("sidebar.multiplexer.panes_suffix_one"),
                            &i18n.t("sidebar.multiplexer.panes_suffix"),
                        )),
                )
            })
            .when_some(cwd_hint, |inner, cwd| {
                inner.child(
                    div()
                        .flex_none()
                        .min_w_0()
                        .max_w(px(220.0))
                        .truncate()
                        .text_size(px(10.0))
                        .text_color(rgb(theme.text_muted))
                        .child(cwd),
                )
            })
            .when(node.restore_failed, |inner| {
                inner.child(
                    div()
                        .flex_none()
                        .text_size(px(10.0))
                        .text_color(rgb(MONITOR_RED))
                        .child(i18n.t("sidebar.multiplexer.restore_error")),
                )
            })
            .when_some(
                self.render_mux_status_chip(node.status.as_deref(), i18n),
                |inner, chip| inner.child(chip),
            )
            .when(node.focused, |inner| {
                inner.child(
                    div()
                        .flex_none()
                        .text_size(px(10.0))
                        .text_color(rgb(theme.accent))
                        .child(i18n.t("sidebar.multiplexer.focused")),
                )
            });
        line = match agent_attach {
            // A detached agent has no live pane to act on, so its only verb is
            // pointing a terminal at the agent itself.
            Some((command, label, title, opened, missing, host)) => {
                line.child(host_tools_tooltip_icon_button(
                    tokens,
                    LucideIcon::Cable,
                    12.0,
                    rgb(theme.text),
                    oxideterm_gpui_ui::button::IconButtonOptions {
                        size: 22.0,
                        disabled: command.is_none() || !terminal_available,
                        has_background: true,
                        background: Some(rgb(theme.bg_hover)),
                        hover_background: Some(rgb(theme.bg_panel)),
                        idle_opacity: 1.0,
                        ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                    },
                    label,
                    "host-mux-agent-attach",
                    false,
                    cx.listener(move |tools, _event, window, cx| {
                        let Some(command) = command.clone() else {
                            return;
                        };
                        tools.dispatch_mux_terminal(
                            &host,
                            command,
                            title.clone(),
                            opened.clone(),
                            missing.clone(),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ))
            }
            None => line.children(self.mux_verb_buttons(
                &row_host,
                provider,
                &row_session,
                Some(node),
                true,
                windows,
                if node.kind == MuxKind::Pane {
                    12.0
                } else {
                    13.0
                },
                tokens,
                i18n,
                cx,
            )),
        };

        // The row is the identity, hover and expansion target; the line inside
        // it is the single flex row that carries the label and the verbs, so
        // buttons stay on the same baseline instead of stacking below.
        let row = div()
            .id(host_tools_row_id(
                "host-mux-node",
                host,
                Self::mux_row_identity(provider, session, Some(node)),
            ))
            .w_full()
            .min_w_0()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .when(show_chevron, |outer| {
                outer
                    .cursor_pointer()
                    .hover(|hovered| hovered.bg(rgb(theme.bg_hover)))
            })
            .when(show_chevron, |outer| {
                let key = key.clone();
                outer.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |tools, _event, _window, cx| {
                        // Expansion is click-owned; a search-revealed branch
                        // stays outside this set.
                        if !tools.host_tmux.mux.expanded.remove(&key) {
                            tools.host_tmux.mux.expanded.insert(key.clone());
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }),
                )
            })
            .child(line);

        let mut container = div().w_full().min_w_0().flex().flex_col().child(row);
        if expanded {
            container = container.children(
                node.children
                    .iter()
                    .filter(|child| search.matches_subtree(child))
                    .map(|child| {
                        self.render_mux_node(
                            host,
                            provider,
                            session,
                            child,
                            depth + 1,
                            search,
                            terminal_available,
                            windows,
                            tokens,
                            i18n,
                            mono_font_family.clone(),
                            cx,
                        )
                    }),
            );
        }
        container.into_any_element()
    }

    /// A chip exists only for a reported agent state; "unknown" is the absence
    /// of one and would otherwise repeat on every row.
    fn render_mux_status_chip(&self, status: Option<&str>, i18n: &I18n) -> Option<Div> {
        let status = status?;
        let color = match status {
            "working" => MONITOR_EMERALD,
            "blocked" => MONITOR_RED,
            "idle" | "done" => MONITOR_AMBER,
            _ => return None,
        };
        Some(
            div()
                .flex_none()
                .text_size(px(10.0))
                .text_color(rgb(color))
                .child(i18n.t(status_label_key(status))),
        )
    }
}

impl HostToolsEntity {
    // ---- verbs ----

    /// One button per verb the provider implements for this row, and only when
    /// the model can build a command for it.
    #[allow(clippy::too_many_arguments)]
    fn mux_verb_buttons(
        &self,
        host: &str,
        provider: Multiplexer,
        session: &str,
        node: Option<&MuxNode>,
        session_running: bool,
        windows: bool,
        icon_size: f32,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let theme = tokens.ui;
        provider
            .verbs_for(node.map(|node| node.kind))
            .iter()
            .filter_map(|verb_kind| {
                let kind = *verb_kind;
                if !self.mux_verb_supported(provider, windows, session, node, kind) {
                    return None;
                }
                let disabled = self.mux_action_running()
                    || match kind {
                        // Stop needs a live session, delete needs a stopped one.
                        MuxVerbKind::Stop => !session_running,
                        MuxVerbKind::Delete => session_running,
                        _ => false,
                    };
                let (icon, label_key, danger) = mux_verb_view(kind, node.map(|node| node.kind));
                let node = node.cloned();
                let dialog_kind = mux_dialog_kind(kind, node.as_ref());
                let prefill = mux_verb_prefill(kind, node.as_ref());
                let host = host.to_owned();
                let session = session.to_owned();
                let target_label = node
                    .as_ref()
                    .map(|node| node.label.clone())
                    .unwrap_or_else(|| session.clone());
                Some(host_tools_tooltip_icon_button(
                    tokens,
                    icon,
                    icon_size,
                    rgb(if danger { MONITOR_RED } else { theme.text }),
                    oxideterm_gpui_ui::button::IconButtonOptions {
                        size: 22.0,
                        disabled,
                        has_background: true,
                        background: Some(if danger {
                            rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA)
                        } else {
                            rgb(theme.bg_hover)
                        }),
                        hover_background: Some(if danger {
                            rgba((MONITOR_RED << 8) | 0x30)
                        } else {
                            rgb(theme.bg_panel)
                        }),
                        idle_opacity: 1.0,
                        ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                    },
                    i18n.t(label_key),
                    mux_verb_element_id(kind),
                    true,
                    cx.listener(move |tools, _event, window, cx| {
                        cx.stop_propagation();
                        match kind {
                            MuxVerbKind::Read => {
                                if let Some(node) = node.as_ref() {
                                    tools.request_mux_read(&session, node, cx);
                                }
                            }
                            MuxVerbKind::Rename
                            | MuxVerbKind::Run
                            | MuxVerbKind::CreateWorkspace => {
                                // The payload verbs collect their value first;
                                // a dialog without a target can send nothing.
                                let Some(dialog) = dialog_kind.clone() else {
                                    cx.emit(HostToolsEvent::ShowNotice(
                                        HostToolsNotice::MuxActionFailed,
                                    ));
                                    return;
                                };
                                tools.open_mux_input_dialog(dialog, &session, &prefill, window, cx);
                            }
                            _ => {
                                let request = MuxActionRequest {
                                    provider,
                                    connection_id: host.clone(),
                                    session: session.clone(),
                                    target_label: target_label.clone(),
                                    node: node.clone(),
                                    verb: mux_probe_verb(kind),
                                };
                                if matches!(
                                    kind,
                                    MuxVerbKind::Close | MuxVerbKind::Stop | MuxVerbKind::Delete
                                ) {
                                    if let Some(notice) = tools.open_mux_confirm(request, cx) {
                                        cx.emit(HostToolsEvent::ShowNotice(notice));
                                    } else {
                                        // The root owns only the shared focus
                                        // adapter, not the confirm state, so an
                                        // open confirm leaves keyboard focus on
                                        // the previous Confirm button and Enter
                                        // would run the next destructive verb
                                        // unasked.
                                        window.dispatch_action(
                                            Box::new(HostToolsWindowRequest::new(
                                                HostToolsWindowIntent::PrepareMuxConfirm,
                                            )),
                                            cx,
                                        );
                                    }
                                } else {
                                    tools.dispatch_mux_verb(request, cx);
                                }
                            }
                        }
                    }),
                ))
            })
            .collect()
    }

    fn dispatch_mux_verb(&mut self, request: MuxActionRequest, cx: &mut Context<Self>) {
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::MuxConnectionMissing,
            ));
            return;
        };
        for notice in self.run_mux_action(request, runtime, cx) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    // ---- input dialog ----

    pub(in crate::workspace::connection_monitor) fn open_mux_input_dialog(
        &mut self,
        kind: MuxInputDialogKind,
        session: &str,
        prefill: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(provider) = self.mux_provider() else {
            return;
        };
        self.ui.host_mux_input_dialog = Some(MuxInputDialog {
            kind,
            session: session.to_string(),
            // The dialog belongs to the host it was opened for: a lifecycle
            // fallback can move the panel to another connection while it is
            // open, and submitting then would mutate the wrong machine.
            provider,
            connection_id: self.host_tmux.mux.host.clone(),
            value: zeroize::Zeroizing::new(prefill.to_string()),
            error: None,
        });
        self.ui.focus_input(HostToolsTextInput::MuxDialog);
        // Sidebar clicks do not move keyboard focus; without this handoff the
        // typed run command would keep going to the focused terminal.
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::PrepareMuxInputDialog,
            )),
            cx,
        );
        cx.notify();
    }

    pub(in crate::workspace::connection_monitor) fn dismiss_mux_input_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.ui.host_mux_input_dialog.take().is_some() {
            if self.ui.input_is_focused(HostToolsTextInput::MuxDialog) {
                self.ui.clear_input_focus();
            }
            cx.notify();
        }
    }

    pub(super) fn submit_mux_input_from_view(&mut self, cx: &mut Context<Self>) {
        // A rejected payload must leave the typed text in front of the user, so
        // the dialog is only consumed once the model accepts the command. The
        // reason is answered under the input, not by a toast over the panel.
        let blank = self
            .ui
            .host_mux_input_dialog
            .as_ref()
            .is_none_or(|dialog| dialog.value.trim().is_empty());
        if blank {
            if let Some(dialog) = self.ui.host_mux_input_dialog.as_mut() {
                dialog.error = Some("sidebar.multiplexer.inputs.error_required");
                cx.notify();
            }
            return;
        }
        if self.host_tmux.mux.action_running.is_some() {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::MuxActionAlreadyRunning,
            ));
            cx.notify();
            return;
        }
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::MuxConnectionMissing,
            ));
            return;
        };
        let Some(provider) = self.mux_provider() else {
            return;
        };
        if self.host_tmux.mux.host.is_empty() {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::MuxConnectionMissing,
            ));
            return;
        }
        let connection_id = self.host_tmux.mux.host.clone();
        let Some(mut dialog) = self.ui.host_mux_input_dialog.take() else {
            return;
        };
        if dialog.provider != provider || dialog.connection_id != connection_id {
            // The panel moved to another host or provider while the dialog was
            // open. The typed value describes an object this host does not
            // have, so nothing runs and the dialog goes away with it.
            drop(dialog);
            self.ui.clear_input_focus();
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::MuxConnectionMissing,
            ));
            cx.notify();
            return;
        }
        dialog.error = None;
        self.ui.clear_input_focus();
        let value = dialog.value.trim();
        let windows = self.mux_windows(&connection_id);
        let (verb, target_label, node) = match &dialog.kind {
            MuxInputDialogKind::NewWorkspace => {
                // "label path" packs both fields into one input line.
                let mut parts = value.splitn(2, char::is_whitespace);
                let label = parts.next().unwrap_or_default().to_string();
                let cwd = parts.next().unwrap_or_default().trim().to_string();
                (
                    MuxVerb::CreateWorkspace {
                        label: label.clone(),
                        cwd: (!cwd.is_empty()).then_some(cwd),
                    },
                    label,
                    None,
                )
            }
            MuxInputDialogKind::RenameWorkspace { target }
            | MuxInputDialogKind::RenameTab { target }
            | MuxInputDialogKind::RenamePane { target } => (
                MuxVerb::Rename {
                    label: value.to_owned(),
                },
                value.to_owned(),
                Some(target.clone()),
            ),
            MuxInputDialogKind::RunPane { target } => (
                MuxVerb::Run {
                    command: zeroize::Zeroizing::new(value.to_owned()),
                },
                target.label.clone(),
                Some(target.clone()),
            ),
            // A new session is a terminal command, dispatched by the view.
            MuxInputDialogKind::NewSession => return,
        };
        // The model owns the payload grammar, so it decides whether this value
        // can become a command at all; a refusal reopens the dialog with the
        // text and names the reason where it was typed.
        let Some(command) = provider.verb_command(windows, &dialog.session, node.as_ref(), &verb)
        else {
            dialog.error = Some("sidebar.multiplexer.inputs.error_rejected");
            self.ui.host_mux_input_dialog = Some(dialog);
            cx.notify();
            return;
        };
        for notice in self.start_mux_action(
            MuxActionRequest {
                provider,
                connection_id,
                session: dialog.session,
                target_label,
                node,
                verb,
            },
            command,
            runtime,
            cx,
        ) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
        cx.notify();
    }

    // ---- confirm ----

    pub(in crate::workspace::connection_monitor) fn render_mux_confirm_view(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        focused_action: Option<ConfirmDialogAction>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (request, phase) = self.mux_confirm_view()?;
        let description = i18n
            .t(mux_confirm_description_key(&request))
            .replace("{{name}}", &request.target_label)
            .replace("{{session}}", &request.session)
            .replace(
                "{{target}}",
                request
                    .node
                    .as_ref()
                    .map(|node| node.id.as_str())
                    .unwrap_or_default(),
            );
        let exit_delay = oxideterm_gpui_ui::motion::duration(
            tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        let confirm_delay = exit_delay;
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                tokens,
                "host-mux-confirm-motion",
                phase,
                ConfirmDialogView {
                    variant: ConfirmDialogVariant::Danger,
                    title: div()
                        .child(i18n.t("sidebar.multiplexer.confirm.title"))
                        .into_any_element(),
                    description: Some(div().child(description).into_any_element()),
                    cancel_label: div()
                        .child(i18n.t("sidebar.multiplexer.confirm.cancel"))
                        .into_any_element(),
                    confirm_label: div()
                        .child(i18n.t("sidebar.multiplexer.confirm.run"))
                        .into_any_element(),
                },
                focused_action,
                cx.listener(move |tools, _event, _window, cx| {
                    // The global keyboard-focus marker is transient render
                    // input; the confirmation lifecycle belongs to Host Tools.
                    tools.begin_mux_confirm_exit(exit_delay, cx);
                }),
                cx.listener(move |tools, _event, _window, cx| {
                    tools.confirm_mux_action_from_view(confirm_delay, cx);
                }),
            )
            .into_any_element(),
        )
    }
}

/// The verb's icon and copy, resolved from the tree level the verb runs on.
fn mux_verb_view(
    kind: MuxVerbKind,
    node_kind: Option<MuxKind>,
) -> (LucideIcon, &'static str, bool) {
    use MuxKind::*;
    use MuxVerbKind::*;
    match kind {
        Focus => (LucideIcon::Eye, "sidebar.multiplexer.focus", false),
        CreateChild => (
            LucideIcon::Plus,
            match node_kind {
                // A child of a workspace or of a session is a tab; below a tab
                // or a pane it is a split.
                None | Some(Workspace) => "sidebar.multiplexer.new_tab",
                Some(Tab) | Some(Pane) => "sidebar.multiplexer.split",
            },
            false,
        ),
        CreateWorkspace => (
            LucideIcon::Plus,
            "sidebar.multiplexer.actions.new_workspace",
            false,
        ),
        Rename => (
            LucideIcon::Pencil,
            "sidebar.multiplexer.actions.rename",
            false,
        ),
        Run => (LucideIcon::Play, "sidebar.multiplexer.actions.send", false),
        Read => (
            LucideIcon::FileText,
            "sidebar.multiplexer.actions.read",
            false,
        ),
        Close => (LucideIcon::X, "sidebar.multiplexer.actions.close", true),
        Stop => (
            LucideIcon::X,
            "sidebar.multiplexer.actions.session_stop",
            true,
        ),
        Delete => (
            LucideIcon::Trash2,
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

fn mux_input_title_key(kind: &MuxInputDialogKind) -> &'static str {
    match kind {
        MuxInputDialogKind::NewSession => "sidebar.multiplexer.inputs.new_session",
        MuxInputDialogKind::NewWorkspace => "sidebar.multiplexer.inputs.new_workspace",
        MuxInputDialogKind::RenameWorkspace { .. } => "sidebar.multiplexer.inputs.rename_workspace",
        MuxInputDialogKind::RenameTab { .. } => "sidebar.multiplexer.inputs.rename_tab",
        MuxInputDialogKind::RenamePane { .. } => "sidebar.multiplexer.inputs.rename_pane",
        MuxInputDialogKind::RunPane { .. } => "sidebar.multiplexer.inputs.run_pane",
    }
}

fn mux_input_placeholder_key(kind: &MuxInputDialogKind) -> &'static str {
    match kind {
        MuxInputDialogKind::NewSession => "sidebar.multiplexer.inputs.new_session_hint",
        MuxInputDialogKind::NewWorkspace => "sidebar.multiplexer.inputs.new_workspace_hint",
        MuxInputDialogKind::RenameWorkspace { .. }
        | MuxInputDialogKind::RenameTab { .. }
        | MuxInputDialogKind::RenamePane { .. } => "sidebar.multiplexer.inputs.rename_hint",
        MuxInputDialogKind::RunPane { .. } => "sidebar.multiplexer.inputs.run_pane_hint",
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_host_mux_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_tools.read(cx).mux_confirm_view().is_none() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.begin_host_mux_confirm_exit(cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_host_mux_action(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(in crate::workspace) fn handle_host_mux_input_dialog_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_tools.read(cx).ui.host_mux_input_dialog.is_none() {
            return false;
        }
        if event.keystroke.modifiers.platform {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_mux_input_dialog(cx);
                });
                self.ime_marked_text = None;
                self.clear_ime_selection();
                cx.notify();
                true
            }
            "enter" => {
                self.submit_host_mux_input_dialog(window, cx);
                true
            }
            _ => false,
        }
    }

    pub(in crate::workspace) fn handle_host_mux_read_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_tools.read(cx).mux_read_dialog().is_none() {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" {
            self.host_tools.update(cx, |host_tools, cx| {
                host_tools.dismiss_mux_read_dialog(cx);
            });
            // The viewer has no input of its own, so an unfinished composition
            // would otherwise commit into whatever field held focus before it.
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
        }
        true
    }

    pub(super) fn confirm_host_mux_action(&mut self, cx: &mut Context<Self>) {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.confirm_mux_action_from_view(delay, cx);
        });
    }

    /// Keeps the request mounted until the current exit generation completes.
    fn begin_host_mux_confirm_exit(&mut self, cx: &mut Context<Self>) -> bool {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.begin_mux_confirm_exit(delay, cx)
        })
    }

    pub(super) fn submit_host_mux_input_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A new session is created-or-attached by typing the provider command
        // into a terminal, so this one kind is dispatched here where the window
        // and the localized notices live; every other kind is a captured action.
        let new_session = self
            .host_tools
            .read(cx)
            .ui
            .host_mux_input_dialog
            .as_ref()
            .filter(|dialog| matches!(dialog.kind, MuxInputDialogKind::NewSession))
            .map(|dialog| dialog.value.trim().to_string());
        if let Some(name) = new_session {
            if name.is_empty() {
                return;
            }
            let read = self.host_tools.read(cx);
            let host = read.host_tmux.mux.host.clone();
            let Some(provider) = read.host_tmux.mux.provider else {
                return;
            };
            let windows = read.mux_windows(&host);
            let Some(command) = provider.new_session_command(windows, &name) else {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_mux_input_dialog(cx);
                    cx.emit(HostToolsEvent::ShowNotice(HostToolsNotice::MuxActionFailed));
                });
                return;
            };
            let title = mux_row_title(provider, &name);
            let opened = self.i18n.t("sidebar.multiplexer.toast.attach_opened");
            let missing = self.i18n.t("sidebar.multiplexer.toast.connection_missing");
            self.host_tools.update(cx, |host_tools, cx| {
                host_tools.dismiss_mux_input_dialog(cx);
                host_tools
                    .dispatch_mux_terminal(&host, command, title, opened, missing, window, cx);
            });
        } else {
            self.host_tools.update(cx, |host_tools, cx| {
                host_tools.submit_mux_input_from_view(cx);
            });
        }
        self.ime_marked_text = None;
        self.clear_ime_selection();
    }

    pub(in crate::workspace) fn render_host_mux_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let focused_action = self.standard_confirm_focus();
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_mux_confirm_view(&tokens, i18n, focused_action, cx)
        })
    }

    pub(in crate::workspace) fn render_host_mux_input_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let target = WorkspaceImeTarget::HostMuxDialog;
        let (kind, submit_disabled, error, input_control) = {
            let host_tools = self.host_tools.read(cx);
            let ui = &host_tools.ui;
            let dialog = ui.host_mux_input_dialog.as_ref()?;
            let input_control = self
                .workspace_text_input(
                    target,
                    TextInputView {
                        value: dialog.value.as_str(),
                        placeholder: self.i18n.t(mux_input_placeholder_key(&dialog.kind)),
                        focused: ui.input_is_focused(HostToolsTextInput::MuxDialog),
                        caret_visible: self.input_caret.visible(),
                        secret: false,
                        selected_all: false,
                        selected_range: self.ime_selected_range_for_target(target, cx),
                        marked_text: self.marked_text_for_target(target, cx),
                    },
                )
                .h(px(34.0))
                .cursor(CursorStyle::IBeam);
            (
                dialog.kind.clone(),
                dialog.value.trim().is_empty() || host_tools.mux_action_running(),
                dialog.error,
                input_control,
            )
        };
        let workspace = cx.entity();
        let input_control = text_input_anchor_probe(
            target.anchor_id(),
            input_control
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.host_tools.update(cx, |host_tools, _cx| {
                            host_tools.ui.focus_input(HostToolsTextInput::MuxDialog);
                        });
                        this.ime_marked_text = None;
                        this.show_active_input_caret(cx);
                        window.focus(&this.focus_handle, cx);
                        this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                })),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element();
        let cancel_action = self.workspace_confirm_footer_action_button(
            self.i18n.t("sidebar.multiplexer.confirm.cancel"),
            ButtonVariant::Secondary,
            ConfirmDialogAction::Cancel,
            false,
            None,
            |this, _event, _window, cx| {
                this.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_mux_input_dialog(cx);
                });
                this.ime_marked_text = None;
                this.clear_ime_selection();
                cx.notify();
            },
            cx,
        );
        let submit_label = match kind {
            MuxInputDialogKind::RunPane { .. } => self.i18n.t("sidebar.multiplexer.actions.send"),
            _ => self.i18n.t("sidebar.multiplexer.confirm.run"),
        };
        let submit_action = self.workspace_confirm_footer_action_button(
            submit_label,
            ButtonVariant::Default,
            ConfirmDialogAction::Confirm,
            submit_disabled,
            None,
            |this, _event, window, cx| {
                this.submit_host_mux_input_dialog(window, cx);
            },
            cx,
        );
        let theme = self.tokens.ui;
        let dialog = div()
            .w(px(560.0))
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t(mux_input_title_key(&kind))),
                    ),
            )
            .child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(input_control)
                    // The refusal is answered where the value was typed, so
                    // the reason and the text stay together.
                    .when_some(error, |body, key| {
                        body.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(MONITOR_RED))
                                .child(self.i18n.t(key)),
                        )
                    }),
            )
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(cancel_action)
                    .child(submit_action),
            );

        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.dismiss_mux_input_dialog(cx);
                        });
                        this.ime_marked_text = None;
                        this.clear_ime_selection();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(dialog))
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_host_mux_read_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.host_tools.read(cx).mux_read_dialog()?;
        let theme = self.tokens.ui;
        let content = if dialog.loading {
            div()
                .p_4()
                .text_color(rgb(theme.text_muted))
                .child(self.i18n.t("sidebar.multiplexer.read.loading"))
                .into_any_element()
        } else if dialog.failed {
            // A failed capture is answered with the localized fallback only.
            div()
                .p_4()
                .text_color(rgb(MONITOR_RED))
                .child(self.i18n.t("sidebar.multiplexer.toast.read_failed"))
                .into_any_element()
        } else {
            let output = dialog.output.as_deref();
            let mut lines = div()
                .p_3()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .font_family(settings_mono_font_family(self.settings_store.settings()))
                .text_size(px(11.0))
                .text_color(rgb(theme.text));
            let mut rendered = 0;
            if let Some(output) = output {
                for (index, line) in output.lines().enumerate() {
                    let display_line = if line.is_empty() {
                        " ".to_string()
                    } else {
                        line.chars().take(512).collect::<String>()
                    };
                    lines = lines.child(
                        div()
                            .id(("host-mux-read-line", index))
                            .flex_none()
                            .whitespace_nowrap()
                            .child(display_line),
                    );
                    rendered += 1;
                }
            }
            if rendered == 0 {
                lines = lines.child(div().child(self.i18n.t("sidebar.multiplexer.read.empty")));
            }
            lines.into_any_element()
        };

        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.dismiss_mux_read_dialog(cx);
                        });
                        cx.stop_propagation();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                    oxideterm_gpui_ui::modal::dialog_content(&self.tokens)
                        .w(px(760.0))
                        .max_h(px(520.0))
                        .child(
                            div()
                                .flex_none()
                                .px_4()
                                .py_3()
                                .border_b_1()
                                .border_color(rgb(theme.border))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(14.0))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(theme.text))
                                                .child(
                                                    self.i18n
                                                        .t("sidebar.multiplexer.read.title")
                                                        .replace(
                                                            "{{pane}}",
                                                            &dialog.request.target_label,
                                                        ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(11.0))
                                                .text_color(rgb(theme.text_muted))
                                                .child(dialog.request.target_id.clone()),
                                        ),
                                )
                                .child(host_tools_tooltip_icon_button(
                                    &self.tokens,
                                    LucideIcon::X,
                                    14.0,
                                    rgb(theme.text_muted),
                                    oxideterm_gpui_ui::button::IconButtonOptions {
                                        size: 24.0,
                                        has_background: true,
                                        background: Some(rgb(theme.bg_hover)),
                                        hover_background: Some(rgb(theme.bg_panel)),
                                        idle_opacity: 1.0,
                                        ..oxideterm_gpui_ui::button::IconButtonOptions::compact(
                                            24.0,
                                        )
                                    },
                                    self.i18n.t("sidebar.multiplexer.read.close"),
                                    "host-mux-read-close",
                                    true,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.host_tools.update(cx, |host_tools, cx| {
                                            host_tools.dismiss_mux_read_dialog(cx);
                                        });
                                        cx.stop_propagation();
                                    }),
                                )),
                        )
                        .child(
                            div()
                                .id("host-mux-read-scroll")
                                .min_h(px(120.0))
                                .max_h(px(436.0))
                                .overflow_y_scroll()
                                .overflow_x_scroll()
                                .child(content),
                        ),
                ))
                .into_any_element(),
        )
    }
}
