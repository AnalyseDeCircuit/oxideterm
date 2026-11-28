// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

mod runtime;
#[cfg(test)]
mod tests;
#[cfg(test)]
use runtime::plan_failure;

use std::collections::{HashMap, HashSet};
use std::time::Duration;
use std::{cell::Cell, rc::Rc};
use web_time::Instant;

use gpui::{AppContext, Context, Entity, Task};
use oxideterm_gpui_editor::{
    EditorContextMenuLabels, EditorPresentation, EditorSettings, TextEditorView,
};
use oxideterm_gpui_ui::text_input::TextInputViewport;
use oxideterm_terminal_encoding::command_sender::{
    TerminalSenderFrame, TerminalSenderInputMode, TerminalSenderPacing, TerminalSenderPlanError,
    build_terminal_sender_plan,
};
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::PaneId;
use zeroize::Zeroizing;

pub const TERMINAL_SENDER_DEFAULT_HEIGHT: f32 = 280.0;
pub const TERMINAL_SENDER_MIN_HEIGHT: f32 = 260.0;
pub const TERMINAL_SENDER_MAX_VIEWPORT_RATIO: f32 = 0.65;
const TERMINAL_SENDER_MIN_TERMINAL_HEIGHT: f32 = 180.0;
pub const TERMINAL_SENDER_RESIZE_HOTZONE_HEIGHT: f32 = 14.0;
pub const TERMINAL_SENDER_MIN_INTERVAL_MS: u64 = 20;
pub const TERMINAL_SENDER_MAX_INTERVAL_MS: u64 = 60_000;
pub const TERMINAL_SENDER_MAX_REPEAT_COUNT: u32 = 9_999;
const TERMINAL_SENDER_PROGRESS_NOTIFY_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerminalCommandSenderId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalCommandSenderTargetScope {
    Current,
    All,
    Selected,
    Group,
}

impl TerminalCommandSenderTargetScope {
    pub fn includes(
        self,
        pane: PaneId,
        current: Option<PaneId>,
        selected: &HashSet<PaneId>,
    ) -> bool {
        match self {
            Self::Current => current == Some(pane),
            Self::All => true,
            Self::Selected | Self::Group => selected.contains(&pane),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalCommandSenderStatus {
    Idle,
    Running,
    Completed,
    Stopped,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalCommandSenderFailure {
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
    fn panel_height(self, preferred_height: f32, available_height: f32) -> f32 {
        match self {
            Self::Hidden => 0.0,
            // The compact input is fixed chrome. Reserving terminal space here
            // would make it compete with the quick-command dock's own reserve.
            Self::Compact => {
                oxideterm_gpui_ui::WORKSPACE_STATUS_BAR_HEIGHT.min(available_height.max(0.0))
            }
            Self::Expanded => adjusted_sender_panel_height(preferred_height, 0.0, available_height),
        }
    }

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
        let _ = self;
        EditorPresentation::Inline
    }
}

#[derive(Clone)]
pub struct TerminalCommandSenderTarget {
    pub pane_id: PaneId,
    pub write: Rc<dyn Fn(&TerminalSenderFrame, &mut gpui::App) -> Option<bool>>,
}

#[derive(Clone, Copy)]
struct TerminalCommandSenderResizeDrag {
    start_cursor_y: gpui::Pixels,
    start_height: f32,
}

#[derive(Clone)]
pub struct TerminalCommandSenderDocumentSnapshot {
    pub id: TerminalCommandSenderId,
    pub editor: Entity<TextEditorView>,
    pub input_mode: TerminalSenderInputMode,
    pub pacing: TerminalSenderPacing,
    pub interval_ms: u64,
    pub repeat_count: u32,
    pub target_scope: TerminalCommandSenderTargetScope,
    pub selected_group_id: Option<uuid::Uuid>,
    pub selected_targets: HashSet<PaneId>,
    pub status: TerminalCommandSenderStatus,
    pub failure: Option<TerminalCommandSenderFailure>,
    pub completed_units: u64,
    pub total_units: u64,
    pub accepted_writes: u64,
    pub skipped_writes: u64,
}

struct TerminalCommandSenderDocument {
    id: TerminalCommandSenderId,
    editor: Entity<TextEditorView>,
    compact_draft: Zeroizing<String>,
    compact_viewport: TextInputViewport,
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CompactSuggestionMode {
    Automatic,
    History,
    Dismissed,
}

/// Owns sender documents, target snapshots, timers, cancellation, and progress.
pub struct TerminalCommandSenderEntity {
    layout: TerminalCommandSenderLayout,
    compact_focused: bool,
    compact_suggestion_mode: CompactSuggestionMode,
    compact_suggestion_highlighted: Option<usize>,
    panel_height: f32,
    pub available_height: Rc<Cell<f32>>,
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
    pub fn new(
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
            compact_focused: false,
            compact_suggestion_mode: CompactSuggestionMode::Automatic,
            compact_suggestion_highlighted: None,
            panel_height: TERMINAL_SENDER_DEFAULT_HEIGHT,
            available_height: Rc::new(Cell::new(0.0)),
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
                highlight_current_line: false,
                placeholder: Some(placeholder),
                ..EditorSettings::default()
            };
            editor.set_settings(settings, cx);
            editor.set_border_visible(false);
            editor.set_presentation(presentation, cx);
            editor.set_context_menu_labels(context_menu_labels);
            editor
        });
        TerminalCommandSenderDocument {
            id,
            editor,
            compact_draft: Zeroizing::new(String::new()),
            compact_viewport: TextInputViewport::default(),
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

    pub fn is_expanded(&self) -> bool {
        self.layout == TerminalCommandSenderLayout::Expanded
    }

    pub fn is_visible(&self) -> bool {
        self.layout != TerminalCommandSenderLayout::Hidden
    }

    pub fn toggle_visible(&mut self, cx: &mut Context<Self>) -> bool {
        let next_layout = self.layout.toggled_visibility();
        self.sync_text_for_layout_change(next_layout, cx);
        self.layout = next_layout;
        self.compact_focused = false;
        self.dismiss_compact_suggestions();
        // Hiding changes presentation only; documents and running jobs stay owned here.
        self.resize_drag = None;
        self.sync_editor_presentation(cx);
        cx.notify();
        self.is_visible()
    }

    pub fn set_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        let next_layout = self.layout.with_expanded(expanded);
        if self.layout == next_layout {
            return;
        }
        self.sync_text_for_layout_change(next_layout, cx);
        self.layout = next_layout;
        self.compact_focused = false;
        self.dismiss_compact_suggestions();
        self.resize_drag = None;
        self.sync_editor_presentation(cx);
        cx.notify();
    }

    pub fn toggle_expanded(&mut self, cx: &mut Context<Self>) {
        self.set_expanded(!self.is_expanded(), cx);
    }

    pub fn document_snapshots(&self) -> Vec<TerminalCommandSenderDocumentSnapshot> {
        self.documents
            .iter()
            .map(TerminalCommandSenderDocument::snapshot)
            .collect()
    }

    pub fn active_document_snapshot(&self) -> Option<TerminalCommandSenderDocumentSnapshot> {
        self.document(self.active_document_id)
            .map(TerminalCommandSenderDocument::snapshot)
    }

    pub fn active_document_id(&self) -> TerminalCommandSenderId {
        self.active_document_id
    }

    pub fn compact_focused(&self) -> bool {
        self.compact_focused
    }

    pub fn set_compact_focused(&mut self, focused: bool, cx: &mut Context<Self>) {
        let focused = focused && self.layout == TerminalCommandSenderLayout::Compact;
        if self.compact_focused != focused {
            self.compact_focused = focused;
            if !focused {
                self.dismiss_compact_suggestions();
            }
            cx.notify();
        }
    }

    pub fn compact_suggestion_mode(&self) -> CompactSuggestionMode {
        self.compact_suggestion_mode
    }

    pub fn compact_suggestion_highlighted(&self) -> Option<usize> {
        self.compact_suggestion_highlighted
    }

    pub fn move_compact_suggestion_selection(
        &mut self,
        suggestions_len: usize,
        move_down: bool,
        mode: CompactSuggestionMode,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        if suggestions_len == 0 {
            if self.dismiss_compact_suggestions() {
                cx.notify();
            }
            return None;
        }
        if self.compact_suggestion_mode != mode {
            self.compact_suggestion_highlighted = None;
        }
        let last = suggestions_len.saturating_sub(1);
        let next = if move_down {
            self.compact_suggestion_highlighted
                .map(|index| index.saturating_add(1).min(last))
                .unwrap_or(0)
        } else {
            self.compact_suggestion_highlighted
                .map(|index| index.saturating_sub(1))
                .unwrap_or(last)
        };
        self.compact_suggestion_mode = mode;
        self.compact_suggestion_highlighted = Some(next);
        cx.notify();
        Some(next)
    }

    pub fn dismiss_compact_suggestions(&mut self) -> bool {
        let had_highlight = self.compact_suggestion_highlighted.take().is_some();
        let changed =
            self.compact_suggestion_mode != CompactSuggestionMode::Automatic || had_highlight;
        self.compact_suggestion_mode = CompactSuggestionMode::Automatic;
        changed
    }

    pub fn hide_compact_suggestions(&mut self, cx: &mut Context<Self>) {
        self.compact_suggestion_mode = CompactSuggestionMode::Dismissed;
        self.compact_suggestion_highlighted = None;
        cx.notify();
    }

    pub fn show_compact_history(&mut self, cx: &mut Context<Self>) {
        self.compact_suggestion_mode = CompactSuggestionMode::History;
        self.compact_suggestion_highlighted = None;
        cx.notify();
    }

    pub fn active_compact_draft(&self) -> Option<&str> {
        self.document(self.active_document_id)
            .map(|document| document.compact_draft.as_str())
    }

    pub fn active_compact_viewport(&self) -> Option<TextInputViewport> {
        self.document(self.active_document_id)
            .map(|document| document.compact_viewport.clone())
    }

    pub fn compact_placeholder(&self) -> &str {
        &self.compact_editor_placeholder
    }

    pub fn replace_active_compact_text(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) -> TerminalCommandSenderId {
        let sender_id = self.editable_active_document_id(cx);
        if let Some(document) = self.document_mut(sender_id) {
            document.compact_draft = Zeroizing::new(text);
            document.failure = None;
        }
        self.dismiss_compact_suggestions();
        cx.notify();
        sender_id
    }

    pub fn clear_compact_text(
        &mut self,
        sender_id: TerminalCommandSenderId,
        cx: &mut Context<Self>,
    ) {
        if let Some(document) = self.document_mut(sender_id) {
            document.compact_draft = Zeroizing::new(String::new());
            document.compact_viewport = TextInputViewport::default();
            self.dismiss_compact_suggestions();
            cx.notify();
        }
    }

    pub fn replace_active_text(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) -> TerminalCommandSenderId {
        if !self.is_expanded() {
            if contains_line_break(&text) {
                // Single-line compact input cannot display or edit multiline commands;
                // promote to the multiline sender editor automatically.
                self.set_expanded(true, cx);
            } else {
                return self.replace_active_compact_text(text, cx);
            }
        }
        let (sender_id, editor) = self.editable_active_editor(cx);
        editor.update(cx, |editor, cx| {
            editor.replace_text_external(text, cx);
            editor.move_cursor_to_document_end(cx);
        });
        sender_id
    }

    pub fn append_active_text(
        &mut self,
        suffix: &str,
        separate_with_space: bool,
        cx: &mut Context<Self>,
    ) -> TerminalCommandSenderId {
        if !self.is_expanded() {
            if contains_line_break(suffix) {
                self.set_expanded(true, cx);
            } else {
                let sender_id = self.editable_active_document_id(cx);
                if let Some(document) = self.document_mut(sender_id) {
                    if separate_with_space
                        && document
                            .compact_draft
                            .chars()
                            .next_back()
                            .is_some_and(|last| !last.is_whitespace())
                    {
                        document.compact_draft.push(' ');
                    }
                    document.compact_draft.push_str(suffix);
                    document.failure = None;
                    self.dismiss_compact_suggestions();
                    cx.notify();
                }
                return sender_id;
            }
        }
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

    pub fn running_count(&self) -> usize {
        self.documents
            .iter()
            .filter(|document| document.status == TerminalCommandSenderStatus::Running)
            .count()
    }

    pub fn set_active_document(
        &mut self,
        sender_id: TerminalCommandSenderId,
        cx: &mut Context<Self>,
    ) {
        if self.active_document_id == sender_id || self.document(sender_id).is_none() {
            return;
        }
        self.active_document_id = sender_id;
        self.dismiss_compact_suggestions();
        cx.notify();
    }

    pub fn add_document(&mut self, cx: &mut Context<Self>) -> TerminalCommandSenderId {
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

    pub fn remove_document(
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

    pub fn set_input_mode(
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
            self.dismiss_compact_suggestions();
            cx.notify();
        }
    }

    pub fn set_pacing(
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

    pub fn adjust_interval(
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

    pub fn adjust_repeat_count(
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

    pub fn set_target_scope(
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
            self.dismiss_compact_suggestions();
            cx.notify();
        }
    }

    pub fn toggle_selected_target(
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

    pub fn set_target_group(
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

    pub fn retain_live_targets(&mut self, live_panes: &HashSet<PaneId>, cx: &mut Context<Self>) {
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

    fn editable_active_editor(
        &mut self,
        cx: &mut Context<Self>,
    ) -> (TerminalCommandSenderId, Entity<TextEditorView>) {
        let sender_id = self.editable_active_document_id(cx);
        let editor = self
            .document(sender_id)
            .expect("active sender document disappeared")
            .editor
            .clone();
        (sender_id, editor)
    }

    fn editable_active_document_id(&mut self, cx: &mut Context<Self>) -> TerminalCommandSenderId {
        if self
            .document(self.active_document_id)
            .is_some_and(|document| document.status == TerminalCommandSenderStatus::Running)
        {
            self.add_document(cx)
        } else {
            self.active_document_id
        }
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

    pub fn panel_height(&self) -> f32 {
        self.layout
            .panel_height(self.panel_height, self.available_height.get())
    }

    pub fn is_resizing(&self) -> bool {
        self.resize_drag.is_some()
    }

    pub fn start_resize(&mut self, cursor_y: gpui::Pixels, cx: &mut Context<Self>) {
        let current_height = self.panel_height();
        self.panel_height = current_height;
        self.resize_drag = Some(TerminalCommandSenderResizeDrag {
            start_cursor_y: cursor_y,
            start_height: current_height,
        });
        cx.notify();
    }

    pub fn update_resize(
        &mut self,
        cursor_y: gpui::Pixels,
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
            self.available_height.get(),
        );
        if (next_height - self.panel_height).abs() >= f32::EPSILON {
            self.panel_height = next_height;
            cx.notify();
        }
    }

    pub fn finish_resize(&mut self, cx: &mut Context<Self>) {
        if self.resize_drag.take().is_some() {
            cx.notify();
        }
    }

    pub fn reset_height(&mut self, cx: &mut Context<Self>) {
        let height = adjusted_sender_panel_height(
            TERMINAL_SENDER_DEFAULT_HEIGHT,
            0.0,
            self.available_height.get(),
        );
        let changed = (height - self.panel_height).abs() >= f32::EPSILON;
        self.panel_height = height;
        self.resize_drag = None;
        if changed {
            cx.notify();
        }
    }

    pub fn sync_editor_appearance(
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

    fn sync_text_for_layout_change(
        &mut self,
        next_layout: TerminalCommandSenderLayout,
        cx: &mut Context<Self>,
    ) {
        // The rendered surface is the sole live editor; layout boundaries transfer its draft.
        if self.layout == TerminalCommandSenderLayout::Expanded
            && next_layout != TerminalCommandSenderLayout::Expanded
        {
            for document in &mut self.documents {
                document.compact_draft = Zeroizing::new(document.editor.read(cx).buffer().text());
                document.compact_viewport = TextInputViewport::default();
            }
        } else if self.layout != TerminalCommandSenderLayout::Expanded
            && next_layout == TerminalCommandSenderLayout::Expanded
        {
            for document in &self.documents {
                let text = document.compact_draft.to_string();
                document.editor.update(cx, |editor, cx| {
                    editor.replace_text_external(text, cx);
                    editor.move_cursor_to_document_end(cx);
                });
            }
        }
    }
}

fn contains_line_break(text: &str) -> bool {
    text.contains('\n') || text.contains('\r')
}

fn adjusted_sender_panel_height(start_height: f32, delta_y: f32, available_height: f32) -> f32 {
    // Moving the divider upward grows the bottom sender panel.
    let ratio_limit = available_height * TERMINAL_SENDER_MAX_VIEWPORT_RATIO;
    let terminal_limit = (available_height - TERMINAL_SENDER_MIN_TERMINAL_HEIGHT).max(0.0);
    let maximum = ratio_limit.min(terminal_limit).max(0.0);
    // The preferred minimum must yield to the terminal when the surface is small.
    (start_height - delta_y).clamp(TERMINAL_SENDER_MIN_HEIGHT.min(maximum), maximum)
}
