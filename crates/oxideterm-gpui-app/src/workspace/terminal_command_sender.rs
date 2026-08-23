// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use gpui::{AppContext, Context, Entity, Task, Timer, WeakEntity};
use oxideterm_gpui_editor::{
    EditorContextMenuLabels, EditorPresentation, EditorSettings, TextEditorView,
};
use oxideterm_gpui_terminal::TerminalPane;
use oxideterm_terminal::{
    TerminalSenderFrame, TerminalSenderInputMode, TerminalSenderPacing, TerminalSenderPlanError,
    build_terminal_sender_plan,
};
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::PaneId;
use zeroize::Zeroizing;

pub(super) const TERMINAL_SENDER_DEFAULT_HEIGHT: f32 = 280.0;
pub(super) const TERMINAL_SENDER_MIN_HEIGHT: f32 = 260.0;
pub(super) const TERMINAL_SENDER_MAX_VIEWPORT_RATIO: f32 = 0.65;
const TERMINAL_SENDER_MIN_TERMINAL_HEIGHT: f32 = 180.0;
const TERMINAL_SENDER_WINDOW_CHROME_RESERVE: f32 = 96.0;
pub(super) const TERMINAL_SENDER_RESIZE_HOTZONE_HEIGHT: f32 = 14.0;
pub(super) const TERMINAL_SENDER_MIN_INTERVAL_MS: u64 = 20;
pub(super) const TERMINAL_SENDER_MAX_INTERVAL_MS: u64 = 60_000;
pub(super) const TERMINAL_SENDER_MAX_REPEAT_COUNT: u32 = 9_999;
const TERMINAL_SENDER_PROGRESS_NOTIFY_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct TerminalCommandSenderId(pub(super) u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TerminalCommandSenderTargetScope {
    Current,
    All,
    Selected,
    Group,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TerminalCommandSenderStatus {
    Idle,
    Running,
    Completed,
    Stopped,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TerminalCommandSenderFailure {
    EmptyInput,
    InvalidHex,
    NoTargets,
    TargetBusy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalCommandSenderOutcome {
    Completed,
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalCommandSenderLayout {
    Hidden,
    Compact,
    Expanded,
}

impl TerminalCommandSenderLayout {
    fn toggled_visibility(self) -> Self {
        if self == Self::Hidden {
            Self::Compact
        } else {
            Self::Hidden
        }
    }

    fn with_expanded(self, expanded: bool) -> Self {
        if expanded {
            Self::Expanded
        } else if self == Self::Hidden {
            Self::Hidden
        } else {
            Self::Compact
        }
    }

    fn editor_presentation(self) -> EditorPresentation {
        if self == Self::Expanded {
            EditorPresentation::Document
        } else {
            EditorPresentation::Inline
        }
    }
}

#[derive(Clone)]
pub(super) struct TerminalCommandSenderTarget {
    pub(super) pane_id: PaneId,
    pub(super) pane: WeakEntity<TerminalPane>,
}

#[derive(Clone, Copy)]
struct TerminalCommandSenderResizeDrag {
    start_cursor_y: gpui::Pixels,
    start_height: f32,
}

#[derive(Clone)]
pub(super) struct TerminalCommandSenderDocumentSnapshot {
    pub(super) id: TerminalCommandSenderId,
    pub(super) editor: Entity<TextEditorView>,
    pub(super) input_mode: TerminalSenderInputMode,
    pub(super) pacing: TerminalSenderPacing,
    pub(super) interval_ms: u64,
    pub(super) repeat_count: u32,
    pub(super) target_scope: TerminalCommandSenderTargetScope,
    pub(super) selected_group_id: Option<uuid::Uuid>,
    pub(super) selected_targets: HashSet<PaneId>,
    pub(super) status: TerminalCommandSenderStatus,
    pub(super) failure: Option<TerminalCommandSenderFailure>,
    pub(super) completed_units: u64,
    pub(super) total_units: u64,
    pub(super) accepted_writes: u64,
    pub(super) skipped_writes: u64,
}

struct TerminalCommandSenderDocument {
    id: TerminalCommandSenderId,
    editor: Entity<TextEditorView>,
    input_mode: TerminalSenderInputMode,
    pacing: TerminalSenderPacing,
    interval_ms: u64,
    repeat_count: u32,
    target_scope: TerminalCommandSenderTargetScope,
    selected_group_id: Option<uuid::Uuid>,
    selected_targets: HashSet<PaneId>,
    status: TerminalCommandSenderStatus,
    failure: Option<TerminalCommandSenderFailure>,
    completed_units: u64,
    total_units: u64,
    accepted_writes: u64,
    skipped_writes: u64,
    generation: u64,
    last_progress_notify: Option<Instant>,
}

impl TerminalCommandSenderDocument {
    fn snapshot(&self) -> TerminalCommandSenderDocumentSnapshot {
        TerminalCommandSenderDocumentSnapshot {
            id: self.id,
            editor: self.editor.clone(),
            input_mode: self.input_mode,
            pacing: self.pacing,
            interval_ms: self.interval_ms,
            repeat_count: self.repeat_count,
            target_scope: self.target_scope,
            selected_group_id: self.selected_group_id,
            selected_targets: self.selected_targets.clone(),
            status: self.status,
            failure: self.failure,
            completed_units: self.completed_units,
            total_units: self.total_units,
            accepted_writes: self.accepted_writes,
            skipped_writes: self.skipped_writes,
        }
    }
}

struct TerminalCommandSenderRunTarget {
    pane_id: PaneId,
    pane: WeakEntity<TerminalPane>,
}

/// Owns sender documents, target snapshots, timers, cancellation, and progress.
pub(super) struct TerminalCommandSenderEntity {
    layout: TerminalCommandSenderLayout,
    panel_height: f32,
    resize_drag: Option<TerminalCommandSenderResizeDrag>,
    documents: Vec<TerminalCommandSenderDocument>,
    active_document_id: TerminalCommandSenderId,
    active_tasks: HashMap<TerminalCommandSenderId, Task<()>>,
    target_owners: HashMap<PaneId, TerminalCommandSenderId>,
    next_document_id: u64,
    compact_editor_placeholder: String,
    expanded_editor_placeholder: String,
    editor_context_menu_labels: EditorContextMenuLabels,
    editor_tokens: ThemeTokens,
}

impl TerminalCommandSenderEntity {
    pub(super) fn new(
        tokens: ThemeTokens,
        compact_editor_placeholder: String,
        expanded_editor_placeholder: String,
        editor_context_menu_labels: EditorContextMenuLabels,
        cx: &mut Context<Self>,
    ) -> Self {
        let first_id = TerminalCommandSenderId(1);
        let first = Self::new_document(
            first_id,
            tokens,
            &compact_editor_placeholder,
            &editor_context_menu_labels,
            TerminalCommandSenderLayout::Compact.editor_presentation(),
            cx,
        );
        Self {
            layout: TerminalCommandSenderLayout::Compact,
            panel_height: TERMINAL_SENDER_DEFAULT_HEIGHT,
            resize_drag: None,
            documents: vec![first],
            active_document_id: first_id,
            active_tasks: HashMap::new(),
            target_owners: HashMap::new(),
            next_document_id: 2,
            compact_editor_placeholder,
            expanded_editor_placeholder,
            editor_context_menu_labels,
            editor_tokens: tokens,
        }
    }

    fn new_document(
        id: TerminalCommandSenderId,
        tokens: ThemeTokens,
        placeholder: &str,
        context_menu_labels: &EditorContextMenuLabels,
        presentation: EditorPresentation,
        cx: &mut Context<Self>,
    ) -> TerminalCommandSenderDocument {
        let placeholder = placeholder.to_string();
        let context_menu_labels = context_menu_labels.clone();
        let editor = cx.new(|cx| {
            let mut editor = TextEditorView::new(String::new(), &tokens, cx);
            let settings = EditorSettings {
                soft_wrap: false,
                indentation_markers: false,
                highlight_special_chars: false,
                placeholder: Some(placeholder),
                ..EditorSettings::default()
            };
            editor.set_settings(settings, cx);
            editor.set_presentation(presentation, cx);
            editor.set_context_menu_labels(context_menu_labels);
            editor
        });
        TerminalCommandSenderDocument {
            id,
            editor,
            input_mode: TerminalSenderInputMode::Text,
            pacing: TerminalSenderPacing::Line,
            interval_ms: 1_000,
            repeat_count: 1,
            target_scope: TerminalCommandSenderTargetScope::Current,
            selected_group_id: None,
            selected_targets: HashSet::new(),
            status: TerminalCommandSenderStatus::Idle,
            failure: None,
            completed_units: 0,
            total_units: 0,
            accepted_writes: 0,
            skipped_writes: 0,
            generation: 0,
            last_progress_notify: None,
        }
    }

    pub(super) fn is_expanded(&self) -> bool {
        self.layout == TerminalCommandSenderLayout::Expanded
    }

    pub(super) fn is_visible(&self) -> bool {
        self.layout != TerminalCommandSenderLayout::Hidden
    }

    pub(super) fn toggle_visible(&mut self, cx: &mut Context<Self>) -> bool {
        self.layout = self.layout.toggled_visibility();
        // Hiding changes presentation only; documents and running jobs stay owned here.
        self.resize_drag = None;
        self.sync_editor_presentation(cx);
        cx.notify();
        self.is_visible()
    }

    pub(super) fn set_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        let next_layout = self.layout.with_expanded(expanded);
        if self.layout == next_layout {
            return;
        }
        // Compact and expanded layouts share the same documents and jobs.
        self.layout = next_layout;
        self.resize_drag = None;
        self.sync_editor_presentation(cx);
        cx.notify();
    }

    pub(super) fn toggle_expanded(&mut self, cx: &mut Context<Self>) {
        self.set_expanded(!self.is_expanded(), cx);
    }

    pub(super) fn document_snapshots(&self) -> Vec<TerminalCommandSenderDocumentSnapshot> {
        self.documents
            .iter()
            .map(TerminalCommandSenderDocument::snapshot)
            .collect()
    }

    pub(super) fn active_document_snapshot(&self) -> Option<TerminalCommandSenderDocumentSnapshot> {
        self.document(self.active_document_id)
            .map(TerminalCommandSenderDocument::snapshot)
    }

    pub(super) fn active_document_id(&self) -> TerminalCommandSenderId {
        self.active_document_id
    }

    pub(super) fn replace_active_text(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) -> TerminalCommandSenderId {
        let (sender_id, editor) = self.editable_active_editor(cx);
        editor.update(cx, |editor, cx| {
            editor.replace_text_external(text, cx);
            editor.move_cursor_to_document_end(cx);
        });
        sender_id
    }

    pub(super) fn append_active_text(
        &mut self,
        suffix: &str,
        separate_with_space: bool,
        cx: &mut Context<Self>,
    ) -> TerminalCommandSenderId {
        let (sender_id, editor) = self.editable_active_editor(cx);
        let mut text = Zeroizing::new(editor.read(cx).buffer().text());
        if separate_with_space
            && text
                .chars()
                .next_back()
                .is_some_and(|last| !last.is_whitespace())
        {
            text.push(' ');
        }
        text.push_str(suffix);
        editor.update(cx, |editor, cx| {
            editor.replace_text_external(std::mem::take(&mut *text), cx);
            editor.move_cursor_to_document_end(cx);
        });
        sender_id
    }

    pub(super) fn running_count(&self) -> usize {
        self.documents
            .iter()
            .filter(|document| document.status == TerminalCommandSenderStatus::Running)
            .count()
    }

    pub(super) fn set_active_document(
        &mut self,
        sender_id: TerminalCommandSenderId,
        cx: &mut Context<Self>,
    ) {
        if self.active_document_id == sender_id || self.document(sender_id).is_none() {
            return;
        }
        self.active_document_id = sender_id;
        cx.notify();
    }

    pub(super) fn add_document(&mut self, cx: &mut Context<Self>) -> TerminalCommandSenderId {
        let sender_id = TerminalCommandSenderId(self.next_document_id);
        self.next_document_id = self.next_document_id.wrapping_add(1).max(1);
        self.documents.push(Self::new_document(
            sender_id,
            self.editor_tokens,
            if self.is_expanded() {
                &self.expanded_editor_placeholder
            } else {
                &self.compact_editor_placeholder
            },
            &self.editor_context_menu_labels,
            self.layout.editor_presentation(),
            cx,
        ));
        self.active_document_id = sender_id;
        cx.notify();
        sender_id
    }

    pub(super) fn remove_document(
        &mut self,
        sender_id: TerminalCommandSenderId,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.documents.len() == 1 {
            return false;
        }
        self.cancel_task(sender_id);
        let Some(index) = self
            .documents
            .iter()
            .position(|document| document.id == sender_id)
        else {
            return false;
        };
        self.documents.remove(index);
        if self.active_document_id == sender_id {
            let next_index = index.min(self.documents.len().saturating_sub(1));
            self.active_document_id = self.documents[next_index].id;
        }
        cx.notify();
        true
    }

    pub(super) fn set_input_mode(
        &mut self,
        sender_id: TerminalCommandSenderId,
        mode: TerminalSenderInputMode,
        cx: &mut Context<Self>,
    ) {
        if self
            .document(sender_id)
            .is_some_and(|document| document.status == TerminalCommandSenderStatus::Running)
        {
            return;
        }
        if let Some(document) = self.document_mut(sender_id)
            && document.input_mode != mode
        {
            document.input_mode = mode;
            document.failure = None;
            cx.notify();
        }
    }

    pub(super) fn set_pacing(
        &mut self,
        sender_id: TerminalCommandSenderId,
        pacing: TerminalSenderPacing,
        cx: &mut Context<Self>,
    ) {
        if self
            .document(sender_id)
            .is_some_and(|document| document.status == TerminalCommandSenderStatus::Running)
        {
            return;
        }
        if let Some(document) = self.document_mut(sender_id)
            && document.pacing != pacing
        {
            document.pacing = pacing;
            document.interval_ms = document.interval_ms.max(match pacing {
                TerminalSenderPacing::Line => 100,
                TerminalSenderPacing::Character => TERMINAL_SENDER_MIN_INTERVAL_MS,
            });
            document.failure = None;
            cx.notify();
        }
    }

    pub(super) fn adjust_interval(
        &mut self,
        sender_id: TerminalCommandSenderId,
        delta_ms: i64,
        cx: &mut Context<Self>,
    ) {
        if self
            .document(sender_id)
            .is_some_and(|document| document.status == TerminalCommandSenderStatus::Running)
        {
            return;
        }
        if let Some(document) = self.document_mut(sender_id) {
            let minimum = match document.pacing {
                TerminalSenderPacing::Line => 100,
                TerminalSenderPacing::Character => TERMINAL_SENDER_MIN_INTERVAL_MS,
            };
            let next = if delta_ms.is_negative() {
                document.interval_ms.saturating_sub(delta_ms.unsigned_abs())
            } else {
                document.interval_ms.saturating_add(delta_ms as u64)
            }
            .clamp(minimum, TERMINAL_SENDER_MAX_INTERVAL_MS);
            if next != document.interval_ms {
                document.interval_ms = next;
                cx.notify();
            }
        }
    }

    pub(super) fn adjust_repeat_count(
        &mut self,
        sender_id: TerminalCommandSenderId,
        delta: i32,
        cx: &mut Context<Self>,
    ) {
        if self
            .document(sender_id)
            .is_some_and(|document| document.status == TerminalCommandSenderStatus::Running)
        {
            return;
        }
        if let Some(document) = self.document_mut(sender_id) {
            let next = if delta.is_negative() {
                document.repeat_count.saturating_sub(delta.unsigned_abs())
            } else {
                document.repeat_count.saturating_add(delta as u32)
            }
            .clamp(1, TERMINAL_SENDER_MAX_REPEAT_COUNT);
            if next != document.repeat_count {
                document.repeat_count = next;
                cx.notify();
            }
        }
    }

    pub(super) fn set_target_scope(
        &mut self,
        sender_id: TerminalCommandSenderId,
        scope: TerminalCommandSenderTargetScope,
        cx: &mut Context<Self>,
    ) {
        if self
            .document(sender_id)
            .is_some_and(|document| document.status == TerminalCommandSenderStatus::Running)
        {
            return;
        }
        if let Some(document) = self.document_mut(sender_id)
            && document.target_scope != scope
        {
            document.target_scope = scope;
            document.failure = None;
            cx.notify();
        }
    }

    pub(super) fn toggle_selected_target(
        &mut self,
        sender_id: TerminalCommandSenderId,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) {
        if self
            .document(sender_id)
            .is_some_and(|document| document.status == TerminalCommandSenderStatus::Running)
        {
            return;
        }
        if let Some(document) = self.document_mut(sender_id) {
            if !document.selected_targets.remove(&pane_id) {
                document.selected_targets.insert(pane_id);
            }
            document.target_scope = TerminalCommandSenderTargetScope::Selected;
            document.failure = None;
            cx.notify();
        }
    }

    pub(super) fn set_target_group(
        &mut self,
        sender_id: TerminalCommandSenderId,
        group_id: uuid::Uuid,
        targets: &[PaneId],
        cx: &mut Context<Self>,
    ) {
        if self
            .document(sender_id)
            .is_some_and(|document| document.status == TerminalCommandSenderStatus::Running)
        {
            return;
        }
        if let Some(document) = self.document_mut(sender_id) {
            document.selected_group_id = Some(group_id);
            document.selected_targets.clear();
            document.selected_targets.extend(targets.iter().copied());
            document.target_scope = TerminalCommandSenderTargetScope::Group;
            document.failure = None;
            cx.notify();
        }
    }

    pub(super) fn retain_live_targets(
        &mut self,
        live_panes: &HashSet<PaneId>,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        for document in &mut self.documents {
            let previous = document.selected_targets.len();
            document
                .selected_targets
                .retain(|pane_id| live_panes.contains(pane_id));
            changed |= document.selected_targets.len() != previous;
        }
        if changed {
            cx.notify();
        }
    }

    pub(super) fn start(
        &mut self,
        sender_id: TerminalCommandSenderId,
        current_pane_id: Option<PaneId>,
        candidates: Vec<TerminalCommandSenderTarget>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(document) = self.document(sender_id) else {
            return false;
        };
        // Starting is not an implicit restart gesture. Refusing a second Start
        // keeps the active task and its target ownership unchanged.
        if document.status == TerminalCommandSenderStatus::Running {
            return false;
        }
        let input = Zeroizing::new(document.editor.read(cx).buffer().text());
        let mode = document.input_mode;
        let pacing = document.pacing;
        let repeat_count = document.repeat_count;
        let interval = Duration::from_millis(document.interval_ms);
        let scope = document.target_scope;
        let selected_targets = document.selected_targets.clone();
        let plan = match build_terminal_sender_plan(input, mode, pacing, repeat_count) {
            Ok(plan) => plan,
            Err(error) => {
                self.set_failure(sender_id, plan_failure(error), cx);
                return false;
            }
        };

        let targets = resolve_run_targets(scope, current_pane_id, &selected_targets, candidates);
        if targets.is_empty() {
            self.set_failure(sender_id, TerminalCommandSenderFailure::NoTargets, cx);
            return false;
        }
        if targets.iter().any(|target| {
            self.target_owners
                .get(&target.pane_id)
                .is_some_and(|owner| *owner != sender_id)
        }) {
            self.set_failure(sender_id, TerminalCommandSenderFailure::TargetBusy, cx);
            return false;
        }

        self.cancel_task(sender_id);
        let generation = {
            let document = self
                .document_mut(sender_id)
                .expect("sender document disappeared before task start");
            document.generation = document.generation.wrapping_add(1);
            document.status = TerminalCommandSenderStatus::Running;
            document.failure = None;
            document.completed_units = 0;
            document.total_units = plan.total_units();
            document.accepted_writes = 0;
            document.skipped_writes = 0;
            document.last_progress_notify = None;
            document.generation
        };
        for target in &targets {
            self.target_owners.insert(target.pane_id, sender_id);
        }
        if let Some(editor) = self
            .document(sender_id)
            .map(|document| document.editor.clone())
        {
            editor.update(cx, |editor, _cx| editor.set_read_only(true));
        }

        let task = cx.spawn(async move |sender, cx| {
            let mut stopped_for_missing_targets = false;
            'repeats: for repeat_index in 0..plan.repeat_count() {
                for (frame_index, frame) in plan.frames().iter().enumerate() {
                    let is_final_unit = repeat_index + 1 == plan.repeat_count()
                        && frame_index + 1 == plan.frames().len();
                    let should_continue = sender
                        .update(cx, |sender, cx| {
                            sender.dispatch_frame(sender_id, generation, frame, &targets, cx)
                        })
                        .unwrap_or(false);
                    if !should_continue {
                        stopped_for_missing_targets = true;
                        break 'repeats;
                    }
                    if !is_final_unit {
                        Timer::after(interval).await;
                    }
                }
            }
            let _ = sender.update(cx, |sender, cx| {
                sender.finish_run(
                    sender_id,
                    generation,
                    if stopped_for_missing_targets {
                        TerminalCommandSenderOutcome::Stopped
                    } else {
                        TerminalCommandSenderOutcome::Completed
                    },
                    cx,
                );
            });
        });
        self.active_tasks.insert(sender_id, task);
        cx.notify();
        true
    }

    pub(super) fn stop(
        &mut self,
        sender_id: TerminalCommandSenderId,
        cx: &mut Context<Self>,
    ) -> bool {
        let running = self
            .document(sender_id)
            .is_some_and(|document| document.status == TerminalCommandSenderStatus::Running);
        if !running {
            return false;
        }
        self.cancel_task(sender_id);
        if let Some(document) = self.document_mut(sender_id) {
            document.generation = document.generation.wrapping_add(1);
            document.status = TerminalCommandSenderStatus::Stopped;
            document.editor.update(cx, |editor, _cx| {
                editor.set_read_only(false);
            });
        }
        cx.notify();
        true
    }

    pub(super) fn stop_all(&mut self, cx: &mut Context<Self>) {
        let running_ids = self
            .documents
            .iter()
            .filter(|document| document.status == TerminalCommandSenderStatus::Running)
            .map(|document| document.id)
            .collect::<Vec<_>>();
        for sender_id in running_ids {
            let _ = self.stop(sender_id, cx);
        }
    }

    fn dispatch_frame(
        &mut self,
        sender_id: TerminalCommandSenderId,
        generation: u64,
        frame: &TerminalSenderFrame,
        targets: &[TerminalCommandSenderRunTarget],
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(document) = self.document(sender_id) else {
            return false;
        };
        if document.generation != generation
            || document.status != TerminalCommandSenderStatus::Running
        {
            return false;
        }

        let mut accepted = 0u64;
        let mut skipped = 0u64;
        let mut live_targets = 0usize;
        for target in targets {
            let result = target.pane.update(cx, |pane, cx| match frame {
                TerminalSenderFrame::TextLine(line) => pane.send_command_sender_line(line, cx),
                TerminalSenderFrame::TextChunk(text) => {
                    pane.send_command_sender_text_chunk(text, cx)
                }
                TerminalSenderFrame::RawBytes(bytes) => {
                    pane.send_command_sender_raw_bytes(bytes, cx)
                }
            });
            match result {
                Ok(true) => {
                    live_targets += 1;
                    accepted = accepted.saturating_add(1);
                }
                Ok(false) => {
                    live_targets += 1;
                    skipped = skipped.saturating_add(1);
                }
                Err(_) => skipped = skipped.saturating_add(1),
            }
        }
        let panel_expanded = self.is_expanded();
        let Some(document) = self.document_mut(sender_id) else {
            return false;
        };
        document.completed_units = document.completed_units.saturating_add(1);
        document.accepted_writes = document.accepted_writes.saturating_add(accepted);
        document.skipped_writes = document.skipped_writes.saturating_add(skipped);
        let now = Instant::now();
        let progress_complete = document.completed_units >= document.total_units;
        let repaint_due = document.last_progress_notify.is_none_or(|last| {
            now.duration_since(last) >= TERMINAL_SENDER_PROGRESS_NOTIFY_INTERVAL
        });
        if panel_expanded && (progress_complete || repaint_due) {
            document.last_progress_notify = Some(now);
            cx.notify();
        }
        // A live pane that rejected the frame is not a successful local write.
        accepted > 0 && live_targets > 0
    }

    fn finish_run(
        &mut self,
        sender_id: TerminalCommandSenderId,
        generation: u64,
        outcome: TerminalCommandSenderOutcome,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.document_mut(sender_id) else {
            return;
        };
        if document.generation != generation
            || document.status != TerminalCommandSenderStatus::Running
        {
            return;
        }
        document.status = match outcome {
            TerminalCommandSenderOutcome::Completed => TerminalCommandSenderStatus::Completed,
            TerminalCommandSenderOutcome::Stopped => TerminalCommandSenderStatus::Stopped,
        };
        document.editor.update(cx, |editor, _cx| {
            editor.set_read_only(false);
        });
        self.release_target_ownership(sender_id);
        cx.notify();
    }

    fn set_failure(
        &mut self,
        sender_id: TerminalCommandSenderId,
        failure: TerminalCommandSenderFailure,
        cx: &mut Context<Self>,
    ) {
        if let Some(document) = self.document_mut(sender_id) {
            document.status = TerminalCommandSenderStatus::Failed;
            document.failure = Some(failure);
            document.completed_units = 0;
            document.total_units = 0;
            document.accepted_writes = 0;
            document.skipped_writes = 0;
            cx.notify();
        }
    }

    fn cancel_task(&mut self, sender_id: TerminalCommandSenderId) {
        // Dropping the Entity-owned GPUI task cancels its pending timer.
        self.active_tasks.remove(&sender_id);
        self.release_target_ownership(sender_id);
    }

    fn release_target_ownership(&mut self, sender_id: TerminalCommandSenderId) {
        self.target_owners.retain(|_, owner| *owner != sender_id);
    }

    fn editable_active_editor(
        &mut self,
        cx: &mut Context<Self>,
    ) -> (TerminalCommandSenderId, Entity<TextEditorView>) {
        let sender_id = if self
            .document(self.active_document_id)
            .is_some_and(|document| document.status == TerminalCommandSenderStatus::Running)
        {
            self.add_document(cx)
        } else {
            self.active_document_id
        };
        let editor = self
            .document(sender_id)
            .expect("active sender document disappeared")
            .editor
            .clone();
        (sender_id, editor)
    }

    fn document(
        &self,
        sender_id: TerminalCommandSenderId,
    ) -> Option<&TerminalCommandSenderDocument> {
        self.documents
            .iter()
            .find(|document| document.id == sender_id)
    }

    fn document_mut(
        &mut self,
        sender_id: TerminalCommandSenderId,
    ) -> Option<&mut TerminalCommandSenderDocument> {
        self.documents
            .iter_mut()
            .find(|document| document.id == sender_id)
    }

    pub(super) fn panel_height_for_viewport(&self, viewport_height: f32) -> f32 {
        adjusted_sender_panel_height(self.panel_height, 0.0, viewport_height)
    }

    pub(super) fn is_resizing(&self) -> bool {
        self.resize_drag.is_some()
    }

    pub(super) fn start_resize(
        &mut self,
        cursor_y: gpui::Pixels,
        viewport_height: f32,
        cx: &mut Context<Self>,
    ) {
        let current_height = self.panel_height_for_viewport(viewport_height);
        self.panel_height = current_height;
        self.resize_drag = Some(TerminalCommandSenderResizeDrag {
            start_cursor_y: cursor_y,
            start_height: current_height,
        });
        cx.notify();
    }

    pub(super) fn update_resize(
        &mut self,
        cursor_y: gpui::Pixels,
        viewport_height: f32,
        dragging: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.resize_drag else {
            return;
        };
        if !dragging {
            self.finish_resize(cx);
            return;
        }
        let next_height = adjusted_sender_panel_height(
            drag.start_height,
            f32::from(cursor_y - drag.start_cursor_y),
            viewport_height,
        );
        if (next_height - self.panel_height).abs() >= f32::EPSILON {
            self.panel_height = next_height;
            cx.notify();
        }
    }

    pub(super) fn finish_resize(&mut self, cx: &mut Context<Self>) {
        if self.resize_drag.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn reset_height(&mut self, viewport_height: f32, cx: &mut Context<Self>) {
        let height =
            adjusted_sender_panel_height(TERMINAL_SENDER_DEFAULT_HEIGHT, 0.0, viewport_height);
        let changed = (height - self.panel_height).abs() >= f32::EPSILON;
        self.panel_height = height;
        self.resize_drag = None;
        if changed {
            cx.notify();
        }
    }

    pub(super) fn sync_editor_appearance(
        &mut self,
        tokens: ThemeTokens,
        font_family: String,
        font_size: f32,
        line_height: f32,
        background_active: bool,
        cx: &mut Context<Self>,
    ) {
        self.editor_tokens = tokens;
        for document in &self.documents {
            document.editor.update(cx, |editor, cx| {
                editor.apply_runtime_settings(
                    &tokens,
                    font_family.clone(),
                    font_size,
                    line_height,
                    false,
                    background_active,
                    cx,
                );
            });
        }
    }

    fn sync_editor_presentation(&mut self, cx: &mut Context<Self>) {
        let placeholder = if self.is_expanded() {
            self.expanded_editor_placeholder.clone()
        } else {
            self.compact_editor_placeholder.clone()
        };
        let presentation = self.layout.editor_presentation();
        for document in &self.documents {
            document.editor.update(cx, |editor, cx| {
                editor.set_presentation(presentation, cx);
                editor.set_placeholder(Some(placeholder.clone()), cx);
            });
        }
    }
}

fn resolve_run_targets(
    scope: TerminalCommandSenderTargetScope,
    current_pane_id: Option<PaneId>,
    selected_targets: &HashSet<PaneId>,
    candidates: Vec<TerminalCommandSenderTarget>,
) -> Vec<TerminalCommandSenderRunTarget> {
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|target| match scope {
            TerminalCommandSenderTargetScope::Current => current_pane_id == Some(target.pane_id),
            TerminalCommandSenderTargetScope::All => true,
            TerminalCommandSenderTargetScope::Selected => {
                selected_targets.contains(&target.pane_id)
            }
            TerminalCommandSenderTargetScope::Group => selected_targets.contains(&target.pane_id),
        })
        .filter(|target| seen.insert(target.pane_id))
        .map(|target| TerminalCommandSenderRunTarget {
            pane_id: target.pane_id,
            pane: target.pane,
        })
        .collect()
}

fn plan_failure(error: TerminalSenderPlanError) -> TerminalCommandSenderFailure {
    match error {
        TerminalSenderPlanError::EmptyInput | TerminalSenderPlanError::EmptyHexInput => {
            TerminalCommandSenderFailure::EmptyInput
        }
        TerminalSenderPlanError::InvalidHexDigit | TerminalSenderPlanError::OddHexDigitCount => {
            TerminalCommandSenderFailure::InvalidHex
        }
        TerminalSenderPlanError::ZeroRepeatCount | TerminalSenderPlanError::UnitCountOverflow => {
            TerminalCommandSenderFailure::EmptyInput
        }
    }
}

fn adjusted_sender_panel_height(start_height: f32, delta_y: f32, viewport_height: f32) -> f32 {
    // Moving the divider upward grows the bottom sender panel.
    let ratio_limit = viewport_height * TERMINAL_SENDER_MAX_VIEWPORT_RATIO;
    let terminal_limit = viewport_height
        - TERMINAL_SENDER_WINDOW_CHROME_RESERVE
        - TERMINAL_SENDER_MIN_TERMINAL_HEIGHT;
    let maximum = ratio_limit
        .min(terminal_limit)
        .max(TERMINAL_SENDER_MIN_HEIGHT);
    (start_height - delta_y).clamp(TERMINAL_SENDER_MIN_HEIGHT, maximum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_errors_map_without_retaining_input_details() {
        assert_eq!(
            plan_failure(TerminalSenderPlanError::InvalidHexDigit),
            TerminalCommandSenderFailure::InvalidHex
        );
        assert_eq!(
            plan_failure(TerminalSenderPlanError::EmptyInput),
            TerminalCommandSenderFailure::EmptyInput
        );
    }
}
