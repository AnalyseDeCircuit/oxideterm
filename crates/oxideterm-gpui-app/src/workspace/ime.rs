use std::{cell::RefCell, collections::HashMap, fmt, ops::Range, rc::Rc, time::Instant};

use gpui::{
    App, Bounds, ClipboardItem, Context, Element, ElementId, Entity, FocusHandle, GlobalElementId,
    InputHandler, InspectorElementId, Keystroke, LayoutId, Pixels, Point, SharedString, Style,
    TextRun, Timer, UTF16Selection, Window, font, point, px, rgb,
};
use oxideterm_editor_core::utf16::{
    byte_index_for_utf16, control_k_delete_end, floor_char_boundary, line_end_for_utf16_offset,
    line_range_for_utf16_offset, line_ranges_utf16, line_start_for_utf16_offset,
    next_utf16_boundary, next_word_boundary, previous_utf16_boundary, previous_word_boundary,
    replace_utf16, transpose_text_at_utf16_offset, utf16_offset_for_byte_index,
    utf16_offset_for_char_index, utf16_slice, vertical_line_navigation_destination,
    word_range_for_utf16_offset,
};

use super::WorkspaceApp;
use super::connection_monitor::HostToolsTextInput;
use super::file_manager::FileManagerInput;
use super::forwards::ForwardInput;
use super::graphics::GraphicsInput;
use super::launcher::LauncherInput;
use super::new_connection::{
    CONNECTION_NOTES_LINE_HEIGHT, CONNECTION_NOTES_VERTICAL_PADDING, NewConnectionField,
    refresh_connection_timeout_seconds, refresh_identity_agent_availability,
};
use super::quick_commands::QuickCommandInput;
use super::session_manager::{SessionManagerInput, SessionManagerState};
use super::sftp::SftpInput;
use super::terminal_git::TerminalGitPanelSection;
use oxideterm_gpui_settings_view::SettingsInput;
use oxideterm_gpui_ui::{
    tauri_ui_font_family,
    text_input::{
        TextInputAnchor, TextInputAnchorId, TextInputContentAlign, text_input_secret_mask,
    },
};
use zeroize::{Zeroize, Zeroizing};

const READ_ONLY_TEXT_EM_WIDTH: f32 = 16.0;
const READ_ONLY_TEXT_LINE_HEIGHT_ESTIMATE: f32 = 28.0;
const SECRET_IME_BMP_PROXY: char = '\u{2022}';
const SECRET_IME_ASTRAL_PROXY: char = '\u{1f512}';
const CARET_BLINK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(530);

fn secret_ime_proxy(secret: &str) -> String {
    // Preserve every UTF-16 boundary without copying secret content across the
    // Entity boundary. Editing ranges remain valid for both BMP and astral input.
    secret
        .chars()
        .map(|character| {
            if character.len_utf16() == 1 {
                SECRET_IME_BMP_PROXY
            } else {
                SECRET_IME_ASTRAL_PROXY
            }
        })
        .collect()
}

fn ime_text_snapshot(target: WorkspaceImeTarget, value: &str) -> String {
    if ime_target_is_secret(target) {
        secret_ime_proxy(value)
    } else {
        value.to_owned()
    }
}

fn session_manager_ime_text(
    session_manager: &SessionManagerState,
    input: SessionManagerInput,
) -> Option<String> {
    if session_manager.focused_input() != Some(input) {
        return None;
    }
    let value = session_manager.input_value(input)?;
    Some(ime_text_snapshot(
        WorkspaceImeTarget::SessionManager(input),
        value,
    ))
}

/// Shares layout-only text input anchors without retaining the workspace entity.
#[derive(Clone, Default)]
pub(super) struct TextInputAnchorStore {
    anchors: Rc<RefCell<HashMap<TextInputAnchorId, TextInputAnchor>>>,
}

impl TextInputAnchorStore {
    pub(super) fn get(&self, id: TextInputAnchorId) -> Option<TextInputAnchor> {
        self.anchors.borrow().get(&id).copied()
    }

    pub(super) fn changed(&self, anchor: TextInputAnchor) -> bool {
        self.get(anchor.id) != Some(anchor)
    }

    pub(super) fn update(&self, anchor: TextInputAnchor) {
        if self.changed(anchor) {
            // Anchor probes run during layout, so geometry updates must not
            // trigger a second render loop.
            self.anchors.borrow_mut().insert(anchor.id, anchor);
        }
    }

    fn bounds(&self, id: TextInputAnchorId) -> Option<Bounds<Pixels>> {
        self.get(id).map(|anchor| anchor.bounds)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum WorkspaceImeTarget {
    ReadOnlyText(u64),
    CommandPalette,
    ShortcutsModalSearch,
    Search,
    TerminalCwdSearch,
    TerminalGitBranchSearch,
    TerminalGitCommitMessage,
    TerminalProjectSearch,
    TerminalBroadcastGroupName,
    TerminalCastSearch,
    HostProcessSearch,
    HostProcessRenice,
    HostDockerSearch,
    HostServiceSearch,
    HostLogSearch,
    HostTmuxSearch,
    HostTmuxDialogInput,
    HostPortSearch,
    HostScheduleSearch,
    HostFilesystemSearch,
    HostPackageSearch,
    QuickCommand(QuickCommandInput),
    Settings(SettingsInput),
    SessionManager(SessionManagerInput),
    Forwards(ForwardInput),
    FileManager(FileManagerInput),
    Launcher(LauncherInput),
    Graphics(GraphicsInput),
    TabRename,
    AiModelSelectorSearch,
    AiInlinePrompt,
    AiChatInput,
    AiConversationRename,
    AiMessageEdit,
    PluginControl { key: u64, secret: bool },
    Sftp(SftpInput),
    NewConnection(NewConnectionField),
    KeyboardInteractive(usize),
}

/// Read-only caret projection shared with render code without exposing timer writes.
#[derive(Clone, Default)]
pub(super) struct WorkspaceCaretVisibility {
    visible: Rc<std::cell::Cell<bool>>,
}

impl WorkspaceCaretVisibility {
    pub(super) fn visible(&self) -> bool {
        self.visible.get()
    }

    fn set_visible(&self, visible: bool) -> bool {
        if self.visible.get() == visible {
            return false;
        }
        self.visible.set(visible);
        true
    }
}

#[derive(Clone, Copy)]
struct CaretBlinkPause {
    target: WorkspaceImeTarget,
    until: Instant,
}

struct WorkspaceCaretState {
    active_target: Option<WorkspaceImeTarget>,
    visibility: WorkspaceCaretVisibility,
    pause: Option<CaretBlinkPause>,
}

impl WorkspaceCaretState {
    fn new(visibility: WorkspaceCaretVisibility) -> Self {
        visibility.visible.set(true);
        Self {
            active_target: None,
            visibility,
            pause: None,
        }
    }

    fn sync_active_target(&mut self, target: Option<WorkspaceImeTarget>) -> bool {
        if self.active_target == target {
            return false;
        }
        self.active_target = target;
        self.pause = self
            .pause
            .filter(|pause| Some(pause.target) == self.active_target);
        self.visibility.set_visible(true);
        true
    }

    fn show_caret(&mut self) {
        self.visibility.set_visible(true);
    }

    fn pause_settings_caret(&mut self, until: Instant) -> bool {
        let Some(target @ WorkspaceImeTarget::Settings(_)) = self.active_target else {
            return false;
        };
        self.pause = Some(CaretBlinkPause { target, until });
        self.visibility.set_visible(true);
        true
    }

    fn next_tick_delay(&self, now: Instant) -> Option<std::time::Duration> {
        let target = self
            .active_target
            .filter(|target| ime_target_should_blink_caret(*target))?;
        if let Some(pause) = self
            .pause
            .filter(|pause| pause.target == target && now < pause.until)
        {
            return Some(pause.until.saturating_duration_since(now));
        }
        Some(CARET_BLINK_INTERVAL)
    }

    fn advance_tick(&mut self, now: Instant) -> bool {
        let Some(target) = self
            .active_target
            .filter(|target| ime_target_should_blink_caret(*target))
        else {
            return false;
        };
        if self
            .pause
            .is_some_and(|pause| pause.target == target && now < pause.until)
        {
            return false;
        }
        self.pause = None;
        self.visibility.set_visible(!self.visibility.visible())
    }
}

/// Owns the window-scoped caret phase and its only long-lived blink task.
pub(super) struct WorkspaceInputEntity {
    caret: WorkspaceCaretState,
    blink_generation: u64,
    blink_task: Option<gpui::Task<()>>,
}

impl WorkspaceInputEntity {
    pub(super) fn new(visibility: WorkspaceCaretVisibility) -> Self {
        Self {
            caret: WorkspaceCaretState::new(visibility),
            blink_generation: 0,
            blink_task: None,
        }
    }

    pub(super) fn sync_active_target(
        &mut self,
        target: Option<WorkspaceImeTarget>,
        cx: &mut Context<Self>,
    ) {
        if self.caret.sync_active_target(target) {
            self.restart_blink_task(cx);
        }
    }

    pub(super) fn show_caret(&mut self, cx: &mut Context<Self>) {
        self.caret.show_caret();
        self.restart_blink_task(cx);
    }

    pub(super) fn pause_settings_caret_until(&mut self, until: Instant, cx: &mut Context<Self>) {
        if self.caret.pause_settings_caret(until) {
            self.restart_blink_task(cx);
        }
    }

    fn restart_blink_task(&mut self, cx: &mut Context<Self>) {
        self.blink_generation = self.blink_generation.wrapping_add(1);
        self.blink_task = None;
        if self.caret.next_tick_delay(Instant::now()).is_none() {
            return;
        }
        let generation = self.blink_generation;
        self.blink_task = Some(cx.spawn(async move |input, cx| {
            loop {
                let delay = input
                    .update(cx, |input, _cx| {
                        (input.blink_generation == generation)
                            .then(|| input.caret.next_tick_delay(Instant::now()))
                            .flatten()
                    })
                    .ok()
                    .flatten();
                let Some(delay) = delay else {
                    break;
                };
                Timer::after(delay).await;
                let should_continue = input
                    .update(cx, |input, cx| {
                        if input.blink_generation != generation {
                            return false;
                        }
                        if input.caret.advance_tick(Instant::now()) {
                            cx.notify();
                        }
                        input.caret.next_tick_delay(Instant::now()).is_some()
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        }));
    }
}

/// Captures non-secret Host Tools IME presentation state for one render frame.
#[derive(Clone)]
pub(super) struct HostToolsPlainTextImeFrame {
    input: HostToolsTextInput,
    target: WorkspaceImeTarget,
    caret_visible: bool,
    selected_range: Option<Range<usize>>,
    marked_text: Option<String>,
    anchor_store: TextInputAnchorStore,
}

impl HostToolsPlainTextImeFrame {
    fn new(
        input: HostToolsTextInput,
        caret_visible: bool,
        selected_range: Option<Range<usize>>,
        marked_text: Option<String>,
        anchor_store: TextInputAnchorStore,
    ) -> Option<Self> {
        // Tmux dialog commands may contain secrets and must stay on their
        // zeroizing workspace-owned input path.
        let target = workspace_ime_target_for_plain_host_tools_input(input)?;
        Some(Self {
            input,
            target,
            caret_visible,
            selected_range,
            marked_text,
            anchor_store,
        })
    }

    pub(super) fn input(&self) -> HostToolsTextInput {
        self.input
    }

    pub(super) fn anchor_id(&self) -> TextInputAnchorId {
        self.target.anchor_id()
    }

    pub(super) fn caret_visible(&self) -> bool {
        self.caret_visible
    }

    pub(super) fn selected_range(&self) -> Option<Range<usize>> {
        self.selected_range.clone()
    }

    pub(super) fn marked_text(&self) -> Option<&str> {
        self.marked_text.as_deref()
    }

    pub(super) fn update_anchor(&self, anchor: TextInputAnchor) {
        self.anchor_store.update(anchor);
    }
}

pub(super) fn workspace_ime_target_for_plain_host_tools_input(
    input: HostToolsTextInput,
) -> Option<WorkspaceImeTarget> {
    match input {
        HostToolsTextInput::ProcessSearch => Some(WorkspaceImeTarget::HostProcessSearch),
        HostToolsTextInput::ProcessRenice => Some(WorkspaceImeTarget::HostProcessRenice),
        HostToolsTextInput::DockerSearch => Some(WorkspaceImeTarget::HostDockerSearch),
        HostToolsTextInput::ServiceSearch => Some(WorkspaceImeTarget::HostServiceSearch),
        HostToolsTextInput::LogSearch => Some(WorkspaceImeTarget::HostLogSearch),
        HostToolsTextInput::TmuxSearch => Some(WorkspaceImeTarget::HostTmuxSearch),
        HostToolsTextInput::TmuxDialog => None,
        HostToolsTextInput::PortSearch => Some(WorkspaceImeTarget::HostPortSearch),
        HostToolsTextInput::ScheduleSearch => Some(WorkspaceImeTarget::HostScheduleSearch),
        HostToolsTextInput::FilesystemSearch => Some(WorkspaceImeTarget::HostFilesystemSearch),
        HostToolsTextInput::PackageSearch => Some(WorkspaceImeTarget::HostPackageSearch),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceImeSelection {
    target: WorkspaceImeTarget,
    range: Range<usize>,
    reversed: bool,
}

impl WorkspaceImeSelection {
    /// Returns the moving edge that a text viewport must keep visible.
    fn active_offset(&self) -> usize {
        if self.reversed {
            self.range.start
        } else {
            self.range.end
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceImeDragSelection {
    target: WorkspaceImeTarget,
    anchor: usize,
}

#[derive(Eq, PartialEq)]
pub(super) struct PendingPlatformTextCommit {
    target: WorkspaceImeTarget,
    text: Zeroizing<String>,
    generation: u64,
    consumed: bool,
}

impl fmt::Debug for PendingPlatformTextCommit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingPlatformTextCommit")
            .field("target", &self.target)
            .field("text", &"<redacted>")
            .field("generation", &self.generation)
            .field("consumed", &self.consumed)
            .finish()
    }
}

#[derive(Eq, PartialEq)]
pub(super) struct WorkspaceImeMarkedText {
    // IME marked text is rendered in a virtual text buffer but commits into the
    // original value range that was selected when composition started.
    target: WorkspaceImeTarget,
    replacement_range: Range<usize>,
    text: Zeroizing<String>,
}

impl fmt::Debug for WorkspaceImeMarkedText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorkspaceImeMarkedText")
            .field("target", &self.target)
            .field("replacement_range", &self.replacement_range)
            .field("text", &"<redacted>")
            .finish()
    }
}

impl WorkspaceImeMarkedText {
    fn virtual_range(&self) -> Range<usize> {
        let marked_len = self.text.encode_utf16().count();
        self.replacement_range.start..self.replacement_range.start + marked_len
    }

    fn replace(&mut self, replacement_range: Range<usize>, text: &str) {
        // Reuse the composition allocation only after clearing its previous
        // contents, so repeated IME updates do not leave stale secret bytes.
        self.text.zeroize();
        self.text.push_str(text);
        self.replacement_range = replacement_range;
    }
}

impl WorkspaceImeTarget {
    pub(super) fn anchor_id(self) -> TextInputAnchorId {
        let id = match self {
            Self::ReadOnlyText(id) => id.wrapping_add(50_000),
            Self::CommandPalette => 4,
            Self::ShortcutsModalSearch => 5,
            Self::Search => 1,
            Self::TerminalCwdSearch => 18,
            Self::TerminalGitBranchSearch => 17,
            Self::TerminalGitCommitMessage => 20,
            Self::TerminalProjectSearch => 19,
            Self::TerminalBroadcastGroupName => 21,
            Self::TerminalCastSearch => 3,
            Self::HostProcessSearch => 6,
            Self::HostProcessRenice => 7,
            Self::HostDockerSearch => 8,
            Self::HostServiceSearch => 9,
            Self::HostLogSearch => 10,
            Self::HostTmuxSearch => 11,
            Self::HostTmuxDialogInput => 12,
            Self::HostPortSearch => 13,
            Self::HostScheduleSearch => 14,
            Self::HostFilesystemSearch => 15,
            Self::HostPackageSearch => 16,
            Self::QuickCommand(input) => 500 + input.anchor_key(),
            Self::Settings(input) => 1_000 + input.anchor_key(),
            Self::SessionManager(input) => 1_500 + input.anchor_key(),
            Self::Forwards(input) => 1_700 + input.anchor_key(),
            Self::FileManager(input) => 1_800 + input.anchor_key(),
            Self::Launcher(input) => 1_850 + input.anchor_key(),
            Self::Graphics(input) => 1_875 + input.anchor_key(),
            Self::TabRename => 1_890,
            Self::AiModelSelectorSearch => 1_895,
            Self::AiInlinePrompt => 1_896,
            Self::AiChatInput => 1_897,
            Self::AiMessageEdit => 1_898,
            Self::AiConversationRename => 1_899,
            Self::PluginControl { key, .. } => key.wrapping_add(10_000),
            Self::Sftp(input) => 1_900 + input.anchor_key(),
            Self::NewConnection(field) => 2_000 + field as u64,
            Self::KeyboardInteractive(index) => 3_000 + index as u64,
        };
        TextInputAnchorId(id)
    }
}

pub(super) struct WorkspaceImeElement {
    view: Entity<WorkspaceApp>,
    focus_handle: FocusHandle,
}

impl WorkspaceImeElement {
    pub(super) fn new(view: Entity<WorkspaceApp>, focus_handle: FocusHandle) -> Self {
        Self { view, focus_handle }
    }
}

impl gpui::IntoElement for WorkspaceImeElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for WorkspaceImeElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = px(0.0).into();
        style.size.height = px(0.0).into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.view.read(cx).active_ime_target(cx).is_some() {
            window.handle_input(
                &self.focus_handle,
                WorkspaceInputHandler {
                    view: self.view.clone(),
                    fallback_bounds: bounds,
                },
                cx,
            );
        }
    }
}

pub(super) struct WorkspaceInputHandler {
    view: Entity<WorkspaceApp>,
    fallback_bounds: Bounds<Pixels>,
}

pub(super) fn active_ime_should_defer_input_key(
    active_ime_target: bool,
    ime_composing: bool,
    keystroke: &Keystroke,
) -> bool {
    // Browser-backed inputs receive printable text through the platform text
    // owner. Page-level key handlers must not append the same character first,
    // otherwise GPUI can commit the same key again through `InputHandler`.
    active_ime_target
        && (keystroke_platform_text(keystroke).is_some()
            || (ime_composing && ime_composition_control_key(keystroke)))
}

fn keystroke_platform_text(keystroke: &Keystroke) -> Option<&str> {
    if keystroke.modifiers.platform || keystroke.modifiers.control {
        return None;
    }

    keystroke
        .key_char
        .as_deref()
        .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
}

fn ime_composition_control_key(keystroke: &Keystroke) -> bool {
    !keystroke.modifiers.platform
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && matches!(keystroke.key.as_str(), "enter" | "space" | " ")
}

pub(super) fn keystroke_uses_text_edit_modifier(keystroke: &Keystroke) -> bool {
    if cfg!(target_os = "macos") {
        // On macOS, Ctrl+letter is a distinct text-editing chord and must not
        // be treated as Command+letter.
        keystroke.modifiers.platform
    } else {
        // Windows and Linux users expect Ctrl+A/C/X/V for ordinary text fields.
        keystroke.modifiers.platform || keystroke.modifiers.control
    }
}

impl InputHandler for WorkspaceInputHandler {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        self.view.update(cx, |view, cx| {
            let target = view.active_ime_target(cx)?;
            view.text_for_ime_target(target, cx).map(|text| {
                let text_len = text.encode_utf16().count();
                let (range, reversed) =
                    if let Some(selection) = view.ime_selection_for_target(target) {
                        (selection.range, selection.reversed)
                    } else {
                        match target {
                            _ if view.selected_ime_target == Some(target) => (0..text_len, false),
                            WorkspaceImeTarget::NewConnection(field)
                                if view
                                    .connection_form_state(cx)
                                    .form
                                    .as_ref()
                                    .is_some_and(|form| form.selected_field == Some(field)) =>
                            {
                                (0..text_len, false)
                            }
                            _ => (text_len..text_len, false),
                        }
                    };
                UTF16Selection { range, reversed }
            })
        })
    }

    fn marked_text_range(&mut self, _window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.view.update(cx, |view, cx| {
            let target = view.active_ime_target(cx)?;
            let marked = view.marked_text_state_for_target(target, cx)?;
            (!marked.text.is_empty()).then(|| marked.virtual_range())
        })
    }

    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        self.view.update(cx, |view, cx| {
            let text = view.active_ime_text_with_marked_text(cx)?;
            let end = text.encode_utf16().count();
            let clamped = range_utf16.start.min(end)..range_utf16.end.min(end);
            *adjusted_range = Some(clamped.clone());
            Some(utf16_slice(&text, clamped))
        })
    }

    fn replace_text_in_range(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut App,
    ) {
        let _ = self.view.update(cx, |view, cx| {
            view.replace_active_ime_text(replacement_range, text, cx);
        });
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut App,
    ) {
        let _ = self.view.update(cx, |view, cx| {
            let Some(target) = view.active_ime_target(cx) else {
                return;
            };
            if new_text.is_empty() {
                if view.ime_marked_text.take().is_some() {
                    cx.notify();
                }
                return;
            }
            let replacement_range =
                view.marked_text_replacement_range_for_platform_range(target, range_utf16, cx);
            if let Some(marked) = view
                .ime_marked_text
                .as_mut()
                .filter(|marked| marked.target == target)
            {
                marked.replace(replacement_range.clone(), new_text);
            } else {
                view.ime_marked_text = Some(WorkspaceImeMarkedText {
                    target,
                    replacement_range: replacement_range.clone(),
                    // Composition may contain a password or token. The marked
                    // owner zeroizes it on replacement, cancellation, or release.
                    text: Zeroizing::new(new_text.to_string()),
                });
            }
            view.set_ime_selection_from_anchor(
                target,
                replacement_range.start,
                replacement_range.end,
            );
            cx.notify();
        });
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut App) {
        let _ = self.view.update(cx, |view, cx| {
            if view.ime_marked_text.take().is_some() {
                cx.notify();
            }
        });
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        self.view.update(cx, |view, cx| {
            let target = view.active_ime_target(cx)?;
            let bounds = view
                .text_input_anchors
                .bounds(target.anchor_id())
                .unwrap_or(self.fallback_bounds);
            if let WorkspaceImeTarget::QuickCommand(input) = target {
                let visible_text = view.ime_text_with_marked_text_for_target(target, cx)?;
                let byte_index = byte_index_for_utf16(&visible_text, range_utf16.end);
                let viewport = view.terminal.read(cx).quick_commands.input_viewport(input);
                if let Some(position) = viewport.position_for_byte_index(byte_index) {
                    // Keep the platform candidate window aligned with the scrolled caret.
                    return Some(Bounds {
                        origin: point(position.x, bounds.bottom()),
                        size: gpui::size(
                            px(view.tokens.metrics.form_caret_width),
                            bounds.size.height,
                        ),
                    });
                }
            }
            Some(Bounds {
                origin: bounds.origin + point(px(0.0), bounds.size.height),
                size: bounds.size,
            })
        })
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<usize> {
        self.view.update(cx, |view, cx| {
            let target = view.active_ime_target(cx)?;
            view.ime_index_for_position(target, point, window, cx)
        })
    }

    fn apple_press_and_hold_enabled(&mut self) -> bool {
        false
    }
}

impl WorkspaceApp {
    pub(super) fn defer_active_ime_key(
        &mut self,
        keystroke: &Keystroke,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(target) = self.active_ime_target(cx) else {
            return false;
        };
        if self.marked_text_state_for_target(target, cx).is_some()
            && ime_composition_control_key(keystroke)
        {
            return true;
        }
        let Some(text) = keystroke_platform_text(keystroke) else {
            return false;
        };

        let generation = self.next_platform_text_commit_generation;
        self.next_platform_text_commit_generation =
            self.next_platform_text_commit_generation.wrapping_add(1);
        self.pending_platform_text_commit = Some(PendingPlatformTextCommit {
            target,
            // The duplicate marker survives one event turn and therefore owns
            // a zeroizing copy regardless of the active input classification.
            text: Zeroizing::new(text.to_string()),
            generation,
            consumed: false,
        });

        // GPUI/macOS can deliver the same printable key through both keydown and
        // InputHandler in one event turn. Keep the marker scoped to this turn so
        // repeated literal input such as "aa" still inserts both characters.
        cx.defer_in(window, move |this, _window, _cx| {
            if this
                .pending_platform_text_commit
                .as_ref()
                .is_some_and(|pending| pending.generation == generation)
            {
                this.pending_platform_text_commit = None;
            }
        });

        true
    }

    pub(super) fn update_text_input_anchor(
        &mut self,
        anchor: TextInputAnchor,
        _cx: &mut Context<Self>,
    ) {
        self.text_input_anchors.update(anchor);
    }

    pub(super) fn host_tools_plain_text_ime_frame(
        &self,
        input: HostToolsTextInput,
        cx: &App,
    ) -> Option<HostToolsPlainTextImeFrame> {
        let target = workspace_ime_target_for_plain_host_tools_input(input)?;
        HostToolsPlainTextImeFrame::new(
            input,
            self.input_caret.visible(),
            self.ime_selected_range_for_target(target, cx),
            self.marked_text_for_target(target, cx).map(str::to_owned),
            self.text_input_anchors.clone(),
        )
    }

    pub(super) fn show_active_input_caret(&mut self, cx: &mut Context<Self>) {
        self.workspace_input
            .update(cx, |input, cx| input.show_caret(cx));
    }

    pub(super) fn active_ime_target(&self, cx: &App) -> Option<WorkspaceImeTarget> {
        if self.app_lock.locked {
            return Some(WorkspaceImeTarget::Settings(
                SettingsInput::AppLockCurrentPassword,
            ));
        }
        if self.app_lock.dialog.is_some()
            && let Some(input) = self.focused_settings_input
            && matches!(
                input,
                SettingsInput::AppLockCurrentPassword
                    | SettingsInput::AppLockNewPassword
                    | SettingsInput::AppLockConfirmPassword
            )
        {
            return Some(WorkspaceImeTarget::Settings(input));
        }
        if self.tab_rename_dialog.is_some() {
            // The blocking rename dialog owns text input ahead of background surfaces.
            return Some(WorkspaceImeTarget::TabRename);
        }
        if let Some(focused_prompt) = self
            .connection_flow
            .read(cx)
            .focused_keyboard_interactive_prompt()
        {
            return Some(WorkspaceImeTarget::KeyboardInteractive(focused_prompt));
        }

        if let Some(form) = self.connection_form_state(cx).form.as_ref()
            && form.field_focused
            && self.new_connection_field_accepts_ime(form.focused_field, cx)
        {
            return Some(WorkspaceImeTarget::NewConnection(form.focused_field));
        }

        if let Some(input) = self.focused_oxide_dialog_input(cx) {
            // Oxide import/export dialogs are workspace-level overlays, so
            // their focused field takes priority over the underlying surface.
            return Some(WorkspaceImeTarget::SessionManager(input));
        }

        let settings_tab_visible = self
            .active_tab(cx)
            .is_some_and(|tab| tab.kind == oxideterm_workspace::TabKind::Settings);
        if settings_tab_visible {
            if let Some(input) = self
                .settings_workspace
                .read(cx)
                .settings_entity_focused_input()
            {
                return Some(WorkspaceImeTarget::Settings(input));
            }

            if let Some(input) = self.ai_entity.read(cx).focused_settings_input() {
                return Some(WorkspaceImeTarget::Settings(input));
            }
        }

        let legacy_settings_input_visible = settings_tab_visible
            || self
                .active_tab(cx)
                .is_some_and(|tab| tab.kind == oxideterm_workspace::TabKind::CloudSync);
        if legacy_settings_input_visible && let Some(input) = self.focused_settings_input {
            return Some(WorkspaceImeTarget::Settings(input));
        }

        if self.command_palette.read(cx).is_open() {
            return Some(WorkspaceImeTarget::CommandPalette);
        }

        if self.shortcuts_modal.open {
            return Some(WorkspaceImeTarget::ShortcutsModalSearch);
        }

        if self.host_tools_visibility(cx).main_window_is_visible()
            && let Some(input) = self.host_tools.read(cx).ui.focused_input
        {
            return Some(match input {
                HostToolsTextInput::ProcessSearch => WorkspaceImeTarget::HostProcessSearch,
                HostToolsTextInput::ProcessRenice => WorkspaceImeTarget::HostProcessRenice,
                HostToolsTextInput::DockerSearch => WorkspaceImeTarget::HostDockerSearch,
                HostToolsTextInput::ServiceSearch => WorkspaceImeTarget::HostServiceSearch,
                HostToolsTextInput::LogSearch => WorkspaceImeTarget::HostLogSearch,
                HostToolsTextInput::TmuxSearch => WorkspaceImeTarget::HostTmuxSearch,
                HostToolsTextInput::TmuxDialog => WorkspaceImeTarget::HostTmuxDialogInput,
                HostToolsTextInput::PortSearch => WorkspaceImeTarget::HostPortSearch,
                HostToolsTextInput::ScheduleSearch => WorkspaceImeTarget::HostScheduleSearch,
                HostToolsTextInput::FilesystemSearch => WorkspaceImeTarget::HostFilesystemSearch,
                HostToolsTextInput::PackageSearch => WorkspaceImeTarget::HostPackageSearch,
            });
        }

        let terminal_tab_visible = self.active_tab(cx).is_some_and(is_terminal_tab);
        if terminal_tab_visible {
            if self.terminal.read(cx).broadcast_group_editor().is_some() {
                return Some(WorkspaceImeTarget::TerminalBroadcastGroupName);
            }
            let quick_command_input = {
                let quick_commands = &self.terminal.read(cx).quick_commands;
                quick_commands
                    .is_open()
                    .then(|| quick_commands.focused_input())
                    .flatten()
            };
            if let Some(input) = quick_command_input {
                return Some(WorkspaceImeTarget::QuickCommand(input));
            }

            if self.terminal.read(cx).cwd_picker_open() {
                return Some(WorkspaceImeTarget::TerminalCwdSearch);
            }

            if self.terminal.read(cx).git_panel_open() {
                return match self.terminal.read(cx).git_panel_active_section() {
                    TerminalGitPanelSection::Branches => {
                        Some(WorkspaceImeTarget::TerminalGitBranchSearch)
                    }
                    TerminalGitPanelSection::Changes => {
                        Some(WorkspaceImeTarget::TerminalGitCommitMessage)
                    }
                    _ => None,
                };
            }

            if self.terminal.read(cx).project_panel_open() {
                return Some(WorkspaceImeTarget::TerminalProjectSearch);
            }
        }

        if let Some(input) = self.active_session_manager_input(cx) {
            return Some(WorkspaceImeTarget::SessionManager(input));
        }

        if self
            .active_tab(cx)
            .is_some_and(|tab| tab.kind == oxideterm_workspace::TabKind::Forwards)
            && let Some(input) = self.forwarding.read(cx).view().focused_input
        {
            return Some(WorkspaceImeTarget::Forwards(input));
        }

        if self
            .active_tab(cx)
            .is_some_and(|tab| tab.kind == oxideterm_workspace::TabKind::FileManager)
            && let Some(input) = self.file_manager.read(cx).focused_input()
        {
            return Some(WorkspaceImeTarget::FileManager(input));
        }

        if self
            .active_tab(cx)
            .is_some_and(|tab| tab.kind == oxideterm_workspace::TabKind::Launcher)
            && let Some(input) = self.launcher.read(cx).focused_input()
        {
            return Some(WorkspaceImeTarget::Launcher(input));
        }

        if self
            .active_tab(cx)
            .is_some_and(|tab| tab.kind == oxideterm_workspace::TabKind::Graphics)
            && let Some(input) = self.graphics.read(cx).focused_input()
        {
            return Some(WorkspaceImeTarget::Graphics(input));
        }

        if self.visible_sftp_remote_id(cx).is_some()
            && let Some(input) = self.sftp_view.read(cx).focused_input()
        {
            // The input owner may be a full SFTP tab or the embedded terminal
            // sidebar; visibility, not the active tab kind, defines ownership.
            return Some(WorkspaceImeTarget::Sftp(input));
        }

        let terminal_inline_panel = self.ai_entity.read(cx).terminal_inline_panel();
        if (self.ai_sidebar_visible() || terminal_inline_panel.open)
            && self.ai_entity.read(cx).model_selector_open()
            && self.ai_entity.read(cx).model_selector_search_focused()
        {
            return Some(WorkspaceImeTarget::AiModelSelectorSearch);
        }

        if terminal_inline_panel.open && terminal_inline_panel.prompt_focused {
            return Some(WorkspaceImeTarget::AiInlinePrompt);
        }

        if self.ai_sidebar_visible()
            && self
                .ai_entity
                .read(cx)
                .chat_ui()
                .renaming_conversation_id
                .is_some()
            && self
                .ai_entity
                .read(cx)
                .chat_ui()
                .renaming_conversation_focused
        {
            return Some(WorkspaceImeTarget::AiConversationRename);
        }

        if self.ai_sidebar_visible() && self.ai_entity.read(cx).chat_ui().input_focused {
            return Some(WorkspaceImeTarget::AiChatInput);
        }

        if self.ai_sidebar_visible()
            && self
                .ai_entity
                .read(cx)
                .chat_ui()
                .editing_message_id
                .is_some()
            && self.ai_entity.read(cx).chat_ui().editing_message_focused
        {
            return Some(WorkspaceImeTarget::AiMessageEdit);
        }

        if let Some(key) = self.plugin_ui_state(cx).focused_input
            && self.native_plugin_ui_control_is_visible(key, cx)
        {
            let secret = self
                .plugin_ui_state(cx)
                .context(key)
                .is_some_and(|context| context.control_kind == "password");
            return Some(WorkspaceImeTarget::PluginControl { key, secret });
        }

        if terminal_tab_visible && self.terminal.read(cx).cast_search_focused() {
            return Some(WorkspaceImeTarget::TerminalCastSearch);
        }

        if let Some(selection) = self.selected_ime_range.as_ref()
            && matches!(selection.target, WorkspaceImeTarget::ReadOnlyText(_))
        {
            return Some(selection.target);
        }

        if let Some(target @ WorkspaceImeTarget::ReadOnlyText(_)) = self.selected_ime_target {
            return Some(target);
        }

        self.search.visible.then_some(WorkspaceImeTarget::Search)
    }

    pub(super) fn marked_text_for_target(
        &self,
        target: WorkspaceImeTarget,
        cx: &App,
    ) -> Option<&str> {
        self.marked_text_state_for_target(target, cx)
            .map(|marked| marked.text.as_str())
    }

    fn marked_text_state_for_target(
        &self,
        target: WorkspaceImeTarget,
        cx: &App,
    ) -> Option<&WorkspaceImeMarkedText> {
        self.ime_marked_text.as_ref().filter(|marked| {
            marked.target == target
                && self.active_ime_target(cx) == Some(target)
                && !marked.text.is_empty()
        })
    }

    pub(super) fn ime_selected_range_for_target(
        &self,
        target: WorkspaceImeTarget,
        cx: &App,
    ) -> Option<Range<usize>> {
        self.ime_selection_range_for_target(target, cx)
    }

    pub(in crate::workspace) fn ime_active_offset_for_target(
        &self,
        target: WorkspaceImeTarget,
        cx: &App,
    ) -> Option<usize> {
        self.ime_selection_for_target(target)
            .map(|selection| selection.active_offset())
            .or_else(|| {
                self.ime_selection_range_for_target(target, cx)
                    .map(|range| range.end)
            })
    }

    pub(super) fn ime_selection_range_for_target(
        &self,
        target: WorkspaceImeTarget,
        cx: &App,
    ) -> Option<Range<usize>> {
        self.ime_selection_for_target(target)
            .map(|selection| selection.range)
            .or_else(|| {
                if self.selected_ime_target == Some(target) {
                    self.text_for_ime_target(target, cx)
                        .map(|text| 0..text.encode_utf16().count())
                } else if self.active_ime_target(cx) == Some(target) {
                    self.text_for_ime_target(target, cx).map(|text| {
                        let end = text.encode_utf16().count();
                        end..end
                    })
                } else {
                    None
                }
            })
    }

    fn ime_selection_for_target(
        &self,
        target: WorkspaceImeTarget,
    ) -> Option<WorkspaceImeSelection> {
        self.selected_ime_range
            .as_ref()
            .filter(|selection| selection.target == target)
            .cloned()
    }

    pub(super) fn clear_ime_selection(&mut self) -> bool {
        let changed = self.selected_ime_target.is_some()
            || self.selected_ime_range.is_some()
            || self.ime_drag_selection.is_some();
        self.selected_ime_target = None;
        self.selected_ime_range = None;
        self.ime_drag_selection = None;
        changed
    }

    pub(super) fn clear_read_only_ime_selection(&mut self, cx: &mut Context<Self>) {
        let has_read_only_selection = self
            .selected_ime_range
            .as_ref()
            .is_some_and(|selection| ime_target_is_read_only(selection.target))
            || self
                .selected_ime_target
                .is_some_and(ime_target_is_read_only)
            || self
                .ime_drag_selection
                .is_some_and(|drag| ime_target_is_read_only(drag.target));
        if has_read_only_selection {
            self.clear_ime_selection();
            cx.notify();
        }
    }

    pub(super) fn read_only_selection_drag_active(&self) -> bool {
        self.ime_drag_selection
            .is_some_and(|drag| ime_target_is_read_only(drag.target))
    }

    pub(super) fn begin_ime_selection(
        &mut self,
        target: WorkspaceImeTarget,
        position: Point<Pixels>,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.ime_index_for_position(target, position, window, cx) else {
            if self.clear_ime_selection() {
                cx.notify();
            }
            return;
        };

        let anchor = if extend {
            self.selected_ime_range
                .as_ref()
                .filter(|selection| selection.target == target)
                .map(|selection| {
                    if selection.reversed {
                        selection.range.end
                    } else {
                        selection.range.start
                    }
                })
                .unwrap_or(index)
        } else {
            index
        };
        self.ime_drag_selection = Some(WorkspaceImeDragSelection { target, anchor });
        self.set_ime_selection_from_anchor(target, anchor, index);
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn begin_ime_selection_from_mouse_down(
        &mut self,
        target: WorkspaceImeTarget,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // This helper owns the repaint notification for all mouse-down
        // selection paths, so callers must not issue a second cx.notify().
        if event.click_count <= 1 || event.modifiers.shift {
            self.begin_ime_selection(target, event.position, event.modifiers.shift, window, cx);
            return;
        }

        let Some(index) = self.ime_index_for_position(target, event.position, window, cx) else {
            if self.clear_ime_selection() {
                cx.notify();
            }
            return;
        };
        let Some(text) = self.text_for_ime_target(target, cx) else {
            if self.clear_ime_selection() {
                cx.notify();
            }
            return;
        };
        let text_len = text.encode_utf16().count();
        let range = if event.click_count >= 3 {
            if ime_target_accepts_newline(target) {
                line_range_for_utf16_offset(&text, index)
            } else {
                0..text_len
            }
        } else {
            word_range_for_utf16_offset(&text, index)
        };
        self.selected_ime_target = None;
        self.selected_ime_range = Some(WorkspaceImeSelection {
            target,
            range,
            reversed: false,
        });
        self.ime_drag_selection = None;
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn update_ime_selection_drag(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.ime_drag_selection else {
            return;
        };
        let Some(index) = self.ime_index_for_position(drag.target, position, window, cx) else {
            return;
        };
        if self.set_ime_selection_from_anchor(drag.target, drag.anchor, index) {
            cx.notify();
        }
    }

    pub(super) fn update_read_only_selection_drag_at_position(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.ime_drag_selection else {
            return;
        };
        let WorkspaceImeTarget::ReadOnlyText(id) = drag.target else {
            return;
        };
        let Some(text) = self.text_for_ime_target(drag.target, cx) else {
            return;
        };
        let text_len = text.encode_utf16().count();
        let index = if let Some(layout) = self.selectable_text_layouts.get(&id) {
            let byte_index = match layout.index_for_position(position) {
                Ok(index) | Err(index) => index.min(text.len()),
            };
            utf16_offset_for_byte_index(&text, byte_index)
        } else {
            self.selectable_text_group_index_for_position(id, position)
                .unwrap_or(text_len)
                .min(text_len)
        };
        if self.set_ime_selection_from_anchor(drag.target, drag.anchor, index) {
            cx.notify();
        }
    }

    pub(super) fn update_ime_selection_drag_from_mouse_move(
        &mut self,
        event: &gpui::MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging() || self.ime_drag_selection.is_none() {
            return;
        }
        self.update_ime_selection_drag(event.position, window, cx);
        cx.stop_propagation();
    }

    pub(super) fn finish_ime_selection_drag(&mut self, cx: &mut Context<Self>) {
        let drag = self.ime_drag_selection.take();
        if let Some(drag) = drag
            && ime_target_is_read_only(drag.target)
            && self.selected_ime_range.as_ref().is_some_and(|selection| {
                selection.target == drag.target && selection.range.start == selection.range.end
            })
        {
            // Browser text clicks do not leave a page-level caret. Native read-only
            // selection begins on mouse-down, so clear collapsed ranges on mouse-up
            // to keep Cmd-C falling through to terminal/app copy just like Tauri.
            self.selected_ime_range = None;
            self.selected_ime_target = None;
            cx.notify();
        }
    }

    pub(super) fn set_ime_selection_from_anchor(
        &mut self,
        target: WorkspaceImeTarget,
        anchor: usize,
        index: usize,
    ) -> bool {
        let next = selection_from_anchor(target, anchor, index);
        let changed =
            self.selected_ime_target.is_some() || self.selected_ime_range.as_ref() != Some(&next);
        // MouseMove selection events can fire many times within the same text
        // index. Return whether the browser-visible selection range actually
        // changed so drag paths do not repaint on no-op movement.
        self.selected_ime_target = None;
        self.selected_ime_range = Some(next);
        changed
    }

    fn ime_index_for_position(
        &self,
        target: WorkspaceImeTarget,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &App,
    ) -> Option<usize> {
        let text = self.text_for_ime_target(target, cx)?;
        let text_len = text.encode_utf16().count();
        if text_len == 0 {
            return Some(0);
        }

        if let WorkspaceImeTarget::ReadOnlyText(id) = target
            && let Some(layout) = self.selectable_text_layouts.get(&id)
        {
            let byte_index = match layout.index_for_position(position) {
                Ok(index) | Err(index) => index.min(text.len()),
            };
            return Some(utf16_offset_for_byte_index(&text, byte_index));
        }

        if let WorkspaceImeTarget::ReadOnlyText(id) = target
            && let Some(index) = self.selectable_text_group_index_for_position(id, position)
        {
            return Some(index.min(text_len));
        }

        if let WorkspaceImeTarget::QuickCommand(input) = target {
            let viewport = self.terminal.read(cx).quick_commands.input_viewport(input);
            if let Some(byte_index) = viewport.byte_index_for_position(position) {
                let visible_text = self
                    .ime_text_with_marked_text_for_target(target, cx)
                    .unwrap_or_else(|| text.clone());
                return Some(utf16_offset_for_byte_index(
                    &visible_text,
                    byte_index.min(visible_text.len()),
                ));
            }
        }

        let bounds = self.text_input_anchors.bounds(target.anchor_id())?;
        let padding =
            Self::ime_target_horizontal_padding(target, self.tokens.metrics.ui_control_padding_x);
        let left = bounds.left() + padding;
        let right = bounds.right() - padding;
        let width = right - left;
        if width <= px(1.0) || position.x <= left {
            if ime_target_accepts_newline(target) {
                return Some(self.multiline_ime_index_for_position(
                    target,
                    &text,
                    bounds,
                    position,
                    px(0.0),
                    window,
                ));
            }
            return Some(0);
        }
        if position.x >= right {
            if ime_target_accepts_newline(target) {
                return Some(self.multiline_ime_index_for_position(
                    target, &text, bounds, position, width, window,
                ));
            }
            return Some(text_len);
        }

        let relative_x = if Self::ime_target_content_align(target) == TextInputContentAlign::Center
        {
            let text_width = self.shape_ime_text(target, &text, window).width;
            Self::ime_target_relative_x_for_hit_test(target, position.x, left, width, text_width)
        } else {
            position.x - left
        }
        .clamp(px(0.0), width);
        if ime_target_accepts_newline(target) {
            return Some(self.multiline_ime_index_for_position(
                target, &text, bounds, position, relative_x, window,
            ));
        }
        Some(self.ime_index_for_relative_x(target, &text, relative_x, window))
    }

    fn multiline_ime_index_for_position(
        &self,
        target: WorkspaceImeTarget,
        text: &str,
        bounds: Bounds<Pixels>,
        position: Point<Pixels>,
        relative_x: Pixels,
        window: &mut Window,
    ) -> usize {
        let lines = if ime_target_is_read_only(target) {
            soft_wrapped_line_ranges_utf16(
                text,
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
            )
        } else {
            line_ranges_utf16(text)
        };
        if lines.is_empty() {
            return 0;
        }
        let line_height = self.ime_target_line_height(target, bounds, lines.len());
        let relative_y =
            (position.y - bounds.top() - Self::ime_target_vertical_padding(target)).max(px(0.0));
        let line_index =
            ((relative_y / line_height).floor() as usize).min(lines.len().saturating_sub(1));
        let line_range = lines[line_index].clone();
        let line_text = utf16_slice(text, line_range.clone());
        line_range.start + self.ime_index_for_relative_x(target, &line_text, relative_x, window)
    }

    fn ime_target_line_height(
        &self,
        target: WorkspaceImeTarget,
        bounds: Bounds<Pixels>,
        line_count: usize,
    ) -> Pixels {
        match target {
            WorkspaceImeTarget::AiChatInput | WorkspaceImeTarget::AiMessageEdit => px(20.0),
            WorkspaceImeTarget::Settings(input) if input.accepts_newline() => {
                // Tauri textareas hit-test by their visual line box. Settings
                // multiline fields are hand-rendered in GPUI, so keep the IME
                // y-to-line mapping tied to the shared textarea renderer.
                px(input.textarea_line_height())
            }
            WorkspaceImeTarget::NewConnection(NewConnectionField::Notes) => {
                px(CONNECTION_NOTES_LINE_HEIGHT)
            }
            _ if ime_target_is_read_only(target) && line_count > 0 => {
                let inferred = f32::from(bounds.size.height) / line_count as f32;
                px(inferred.clamp(16.0, 40.0))
            }
            _ => px(self.tokens.metrics.ui_control_height),
        }
    }

    fn ime_target_horizontal_padding(target: WorkspaceImeTarget, control_padding_x: f32) -> Pixels {
        match target {
            WorkspaceImeTarget::AiChatInput
            | WorkspaceImeTarget::AiConversationRename
            | WorkspaceImeTarget::AiMessageEdit
            | WorkspaceImeTarget::Sftp(_)
            | WorkspaceImeTarget::ReadOnlyText(_) => {
                // These targets report an anchor around the painted text itself.
                // Applying the shared form-control padding again makes hit testing
                // drift right of the visible caret.
                px(0.0)
            }
            _ => px(control_padding_x),
        }
    }

    fn ime_target_vertical_padding(target: WorkspaceImeTarget) -> Pixels {
        match target {
            WorkspaceImeTarget::Settings(input) if input.accepts_newline() => {
                // Settings textareas render their own `py-2` equivalent. Browser
                // hit-testing starts from the content box, so subtract that top
                // inset before mapping y to a UTF-16 line.
                px(8.0)
            }
            WorkspaceImeTarget::NewConnection(NewConnectionField::Notes) => {
                px(CONNECTION_NOTES_VERTICAL_PADDING)
            }
            _ => px(0.0),
        }
    }

    fn ime_target_content_align(target: WorkspaceImeTarget) -> TextInputContentAlign {
        match target {
            WorkspaceImeTarget::Settings(
                SettingsInput::TerminalFontSize
                | SettingsInput::TerminalLineHeight
                | SettingsInput::IdeFontSize
                | SettingsInput::IdeLineHeight,
            ) => TextInputContentAlign::Center,
            _ => TextInputContentAlign::Start,
        }
    }

    fn ime_target_relative_x_for_hit_test(
        target: WorkspaceImeTarget,
        position_x: Pixels,
        content_left: Pixels,
        content_width: Pixels,
        text_width: Pixels,
    ) -> Pixels {
        if Self::ime_target_content_align(target) != TextInputContentAlign::Center {
            return position_x - content_left;
        }
        // Browser centered inputs hit-test against the painted text box, not
        // the left edge of the padded control. Mirror that geometry so caret
        // placement follows the visible value.
        let centered_text_left = content_left + (content_width - text_width).max(px(0.0)) * 0.5;
        position_x - centered_text_left
    }

    fn active_ime_text_with_marked_text(&self, cx: &App) -> Option<String> {
        let target = self.active_ime_target(cx)?;
        self.ime_text_with_marked_text_for_target(target, cx)
    }

    /// Builds the virtual text buffer seen by the platform while an IME
    /// composition temporarily replaces the target's selected range.
    pub(super) fn ime_text_with_marked_text_for_target(
        &self,
        target: WorkspaceImeTarget,
        cx: &App,
    ) -> Option<String> {
        let mut text = self.text_for_ime_target(target, cx)?;
        if let Some(marked) = self.marked_text_state_for_target(target, cx) {
            let marked_projection = ime_text_snapshot(target, &marked.text);
            replace_utf16(
                &mut text,
                Some(marked.replacement_range.clone()),
                &marked_projection,
            );
        }
        Some(text)
    }

    /// Returns the composition range inside the virtual text buffer.
    pub(super) fn ime_marked_virtual_range_for_target(
        &self,
        target: WorkspaceImeTarget,
        cx: &App,
    ) -> Option<Range<usize>> {
        self.marked_text_state_for_target(target, cx)
            .map(WorkspaceImeMarkedText::virtual_range)
    }

    fn marked_text_replacement_range_for_platform_range(
        &self,
        target: WorkspaceImeTarget,
        platform_range: Option<Range<usize>>,
        cx: &App,
    ) -> Range<usize> {
        let fallback = || {
            self.ime_selection_range_for_target(target, cx)
                .or_else(|| {
                    self.text_for_ime_target(target, cx).map(|text| {
                        let end = text.encode_utf16().count();
                        end..end
                    })
                })
                .unwrap_or(0..0)
        };
        let Some(platform_range) = platform_range else {
            return fallback();
        };
        if let Some(marked) = self.marked_text_state_for_target(target, cx)
            && platform_range == marked.virtual_range()
        {
            // Platform IME callbacks may address the marked substring in the
            // virtual composed text. Map that back to the original value range.
            return marked.replacement_range.clone();
        }
        platform_range
    }

    fn new_connection_field_accepts_ime(&self, field: NewConnectionField, cx: &App) -> bool {
        if field == NewConnectionField::Password
            && self.saved_connection_form_uses_unloaded_secret(cx)
            && self
                .connection_form_state(cx)
                .form
                .as_ref()
                .is_some_and(|form| !form.password_loaded)
        {
            return false;
        }
        true
    }

    fn ime_index_for_relative_x(
        &self,
        target: WorkspaceImeTarget,
        text: &str,
        relative_x: Pixels,
        window: &mut Window,
    ) -> usize {
        let text_len = text.encode_utf16().count();
        if text_len == 0 {
            return 0;
        }

        if ime_target_is_secret(target) {
            return self.secret_ime_index_for_relative_x(target, text, relative_x, window);
        }

        let shaped = self.shape_ime_text(target, text, window);
        let byte_index = shaped.closest_index_for_x(relative_x.clamp(px(0.0), shaped.width));
        utf16_offset_for_byte_index(text, byte_index)
    }

    fn secret_ime_index_for_relative_x(
        &self,
        target: WorkspaceImeTarget,
        text: &str,
        relative_x: Pixels,
        window: &mut Window,
    ) -> usize {
        let display = text_input_secret_mask(text);
        if display.is_empty() {
            return 0;
        }
        let shaped = self.shape_ime_text(target, &display, window);
        let display_byte_index =
            shaped.closest_index_for_x(relative_x.clamp(px(0.0), shaped.width));
        let display_byte_index =
            floor_char_boundary(&display, display_byte_index.min(display.len()));
        let display_chars = display[..display_byte_index].chars().count();
        utf16_offset_for_char_index(text, display_chars)
    }

    fn shape_ime_text(
        &self,
        target: WorkspaceImeTarget,
        text: &str,
        window: &mut Window,
    ) -> gpui::ShapedLine {
        let font = font(self.ime_target_font_family(target));
        let shared = SharedString::from(text.to_string());
        let run = TextRun {
            len: shared.len(),
            font,
            color: rgb(self.tokens.ui.text).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window
            .text_system()
            .shape_line(shared, px(self.tokens.metrics.ui_text_sm), &[run], None)
    }

    fn ime_target_font_family(&self, target: WorkspaceImeTarget) -> SharedString {
        match target {
            WorkspaceImeTarget::Settings(
                SettingsInput::TerminalCommandBarFocusHandoff
                | SettingsInput::TerminalCommandSpecsJson
                | SettingsInput::AiMcpArgs
                | SettingsInput::ManagedKeyPastePrivateKey,
            ) => {
                // These controls are painted with the terminal/settings mono
                // family. Hit-testing with the UI font shifts caret placement
                // across long JSON and command lines.
                super::settings_mono_font_family(self.settings_store.settings())
            }
            WorkspaceImeTarget::QuickCommand(_) => {
                super::settings_mono_font_family(self.settings_store.settings())
            }
            _ => tauri_ui_font_family(&self.settings_store.settings().appearance.ui_font_family),
        }
    }

    fn text_for_ime_target(&self, target: WorkspaceImeTarget, cx: &App) -> Option<String> {
        match target {
            WorkspaceImeTarget::ReadOnlyText(id) => self
                .selectable_text_values
                .get(&id)
                .cloned()
                .or_else(|| self.selectable_text_group_text(id)),
            WorkspaceImeTarget::CommandPalette => {
                Some(self.command_palette.read(cx).query().to_string())
            }
            WorkspaceImeTarget::ShortcutsModalSearch => Some(self.shortcuts_modal.query.clone()),
            WorkspaceImeTarget::Search => Some(self.search.query.clone()),
            WorkspaceImeTarget::TerminalCwdSearch => {
                let terminal = self.terminal.read(cx);
                terminal
                    .cwd_picker_open()
                    .then(|| terminal.cwd_query().to_string())
            }
            WorkspaceImeTarget::TerminalGitBranchSearch => {
                let terminal = self.terminal.read(cx);
                terminal
                    .git_panel_open()
                    .then(|| terminal.git_panel_query().to_string())
            }
            WorkspaceImeTarget::TerminalGitCommitMessage => {
                let terminal = self.terminal.read(cx);
                terminal
                    .git_panel_open()
                    .then(|| terminal.git_commit_message().to_string())
            }
            WorkspaceImeTarget::TerminalProjectSearch => {
                let terminal = self.terminal.read(cx);
                terminal
                    .project_panel_open()
                    .then(|| terminal.project_query().to_string())
            }
            WorkspaceImeTarget::TerminalBroadcastGroupName => self
                .terminal
                .read(cx)
                .broadcast_group_editor()
                .map(|(_, value)| value.to_string()),
            WorkspaceImeTarget::TerminalCastSearch => self
                .terminal
                .read(cx)
                .cast_search_query()
                .map(str::to_string),
            WorkspaceImeTarget::HostProcessSearch => self
                .host_tools
                .read(cx)
                .ui
                .input_value(HostToolsTextInput::ProcessSearch)
                .map(str::to_string),
            WorkspaceImeTarget::HostProcessRenice => self
                .host_tools
                .read(cx)
                .ui
                .input_value(HostToolsTextInput::ProcessRenice)
                .map(str::to_string),
            WorkspaceImeTarget::HostDockerSearch => self
                .host_tools
                .read(cx)
                .ui
                .input_value(HostToolsTextInput::DockerSearch)
                .map(str::to_string),
            WorkspaceImeTarget::HostServiceSearch => self
                .host_tools
                .read(cx)
                .ui
                .input_value(HostToolsTextInput::ServiceSearch)
                .map(str::to_string),
            WorkspaceImeTarget::HostLogSearch => self
                .host_tools
                .read(cx)
                .ui
                .input_value(HostToolsTextInput::LogSearch)
                .map(str::to_string),
            WorkspaceImeTarget::HostTmuxSearch => self
                .host_tools
                .read(cx)
                .ui
                .input_value(HostToolsTextInput::TmuxSearch)
                .map(str::to_string),
            WorkspaceImeTarget::HostTmuxDialogInput => self
                .host_tools
                .read(cx)
                .ui
                .input_value(HostToolsTextInput::TmuxDialog)
                .map(|value| ime_text_snapshot(target, value)),
            WorkspaceImeTarget::HostPortSearch => self
                .host_tools
                .read(cx)
                .ui
                .input_value(HostToolsTextInput::PortSearch)
                .map(str::to_string),
            WorkspaceImeTarget::HostScheduleSearch => self
                .host_tools
                .read(cx)
                .ui
                .input_value(HostToolsTextInput::ScheduleSearch)
                .map(str::to_string),
            WorkspaceImeTarget::HostFilesystemSearch => self
                .host_tools
                .read(cx)
                .ui
                .input_value(HostToolsTextInput::FilesystemSearch)
                .map(str::to_string),
            WorkspaceImeTarget::HostPackageSearch => self
                .host_tools
                .read(cx)
                .ui
                .input_value(HostToolsTextInput::PackageSearch)
                .map(str::to_string),
            WorkspaceImeTarget::QuickCommand(input) => self.quick_command_input_value(input, cx),
            WorkspaceImeTarget::Settings(input) => {
                if self
                    .settings_workspace
                    .read(cx)
                    .settings_entity_focused_input()
                    == Some(input)
                {
                    self.settings_workspace
                        .read(cx)
                        .settings_entity_input_value(input)
                        .map(|value| ime_text_snapshot(target, value))
                } else if self.ai_entity.read(cx).focused_settings_input() == Some(input) {
                    // Platform IME receives only a length-preserving projection
                    // for secrets; the Entity remains the sole plaintext owner.
                    self.ai_entity
                        .read(cx)
                        .settings_input_value(input)
                        .map(|value| ime_text_snapshot(target, value))
                } else if self.focused_settings_input == Some(input) {
                    Some(ime_text_snapshot(target, &self.settings_input_draft))
                } else {
                    None
                }
            }
            WorkspaceImeTarget::SessionManager(input) => {
                session_manager_ime_text(self.session_manager.read(cx), input)
            }
            WorkspaceImeTarget::Forwards(input) => {
                if self.forwarding.read(cx).view().focused_input == Some(input) {
                    Some(self.forward_input_value(input, cx).to_string())
                } else {
                    None
                }
            }
            WorkspaceImeTarget::FileManager(input) => {
                let file_manager = self.file_manager.read(cx);
                if file_manager.focused_input() == Some(input) {
                    Some(file_manager.input_value(input).to_string())
                } else {
                    None
                }
            }
            WorkspaceImeTarget::Launcher(input) => {
                let launcher = self.launcher.read(cx);
                if launcher.focused_input() == Some(input) {
                    Some(launcher.input_value(input).to_string())
                } else {
                    None
                }
            }
            WorkspaceImeTarget::Graphics(input) => {
                let graphics = self.graphics.read(cx);
                if graphics.focused_input() == Some(input) {
                    Some(graphics.input_value(input).to_string())
                } else {
                    None
                }
            }
            WorkspaceImeTarget::TabRename => self
                .tab_rename_dialog
                .as_ref()
                .map(|dialog| dialog.draft.clone()),
            WorkspaceImeTarget::AiModelSelectorSearch => self
                .ai_entity
                .read(cx)
                .model_selector_search_focused()
                .then(|| {
                    self.ai_entity
                        .read(cx)
                        .model_selector_search_query()
                        .to_owned()
                }),
            WorkspaceImeTarget::AiInlinePrompt => {
                let panel = self.ai_entity.read(cx).terminal_inline_panel();
                panel.prompt_focused.then(|| panel.prompt.clone())
            }
            WorkspaceImeTarget::AiChatInput => self
                .ai_entity
                .read(cx)
                .chat_ui()
                .input_focused
                .then(|| self.ai_entity.read(cx).chat_ui().draft.clone()),
            WorkspaceImeTarget::AiConversationRename => self
                .ai_entity
                .read(cx)
                .chat_ui()
                .renaming_conversation_focused
                .then(|| {
                    self.ai_entity
                        .read(cx)
                        .chat_ui()
                        .renaming_conversation_draft
                        .clone()
                }),
            WorkspaceImeTarget::AiMessageEdit => self
                .ai_entity
                .read(cx)
                .chat_ui()
                .editing_message_focused
                .then(|| {
                    self.ai_entity
                        .read(cx)
                        .chat_ui()
                        .editing_message_draft
                        .clone()
                }),
            WorkspaceImeTarget::PluginControl { key, .. } => self
                .native_plugin_ui_control_is_visible(key, cx)
                .then(|| {
                    self.plugin_ui_state(cx)
                        .text(key)
                        .map(|value| ime_text_snapshot(target, value))
                })
                .flatten(),
            WorkspaceImeTarget::Sftp(input) => {
                if self.sftp_view.read(cx).focused_input() == Some(input) {
                    Some(self.sftp_view.read(cx).input_value(input).to_string())
                } else {
                    None
                }
            }
            WorkspaceImeTarget::NewConnection(field) => {
                let form = self.connection_form_state(cx).form.as_ref()?;
                new_connection_field_value(form, field)
                    .map(|value| ime_text_snapshot(target, value))
            }
            WorkspaceImeTarget::KeyboardInteractive(index) => self
                .connection_flow
                .read(cx)
                .keyboard_interactive_response(index)
                .map(|value| ime_text_snapshot(target, value)),
        }
    }

    fn replace_active_ime_text(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.active_ime_target(cx) else {
            return;
        };
        if platform_text_commit_is_duplicate(&mut self.pending_platform_text_commit, target, text) {
            self.ime_marked_text = None;
            return;
        }
        if text.is_empty()
            && replacement_range.as_ref().is_none_or(|range| {
                self.marked_text_state_for_target(target, cx)
                    .is_some_and(|marked| *range == marked.virtual_range())
            })
        {
            self.ime_marked_text = None;
            return;
        }
        let replacement_range = effective_platform_text_replacement_range(
            replacement_range,
            || self.ime_selection_range_for_target(target, cx),
            self.marked_text_state_for_target(target, cx),
        );
        let caret = replacement_range
            .as_ref()
            .map(|range| range.start + text.encode_utf16().count());
        self.ime_marked_text = None;
        self.replace_ime_target_text(target, replacement_range, text, cx);
        if let Some(caret) = caret {
            self.set_ime_selection_from_anchor(target, caret, caret);
        } else {
            self.clear_ime_selection();
        }
    }

    pub(super) fn handle_active_text_input_edit_shortcut(
        &mut self,
        keystroke: &Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        if !keystroke_uses_text_edit_modifier(keystroke) {
            return false;
        }
        match keystroke.key.as_str() {
            "a" => self.select_all_active_text_input(cx),
            "c" => self.copy_active_text_input(cx),
            "x" | "v"
                if self
                    .active_ime_target(cx)
                    .is_some_and(ime_target_is_read_only) =>
            {
                true
            }
            "x" => self.cut_active_text_input(cx),
            "v" => self.paste_active_text_input(cx),
            _ => false,
        }
    }

    pub(super) fn handle_active_text_input_delete_selection(
        &mut self,
        keystroke: &Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        if !matches!(
            keystroke.key.as_str(),
            "backspace" | "delete" | "h" | "d" | "k" | "u"
        ) {
            return false;
        }
        let Some(target) = self.active_ime_target(cx) else {
            return false;
        };
        let Some(text) = self.text_for_ime_target(target, cx) else {
            return false;
        };
        let range = if let Some(range) = self
            .ime_selected_range_for_target(target, cx)
            .filter(|range| range.start < range.end)
        {
            range
        } else if let Some(caret) = self.ime_selection_range_for_target(target, cx) {
            let caret = caret.start.min(text.encode_utf16().count());
            let Some(range) =
                self.text_input_delete_range_for_caret(target, &text, caret, keystroke)
            else {
                return false;
            };
            range
        } else {
            return false;
        };
        if range.start == range.end {
            // Browser inputs still consume boundary Backspace/Delete, but they do
            // not repaint because neither text nor selection changes.
            return true;
        }
        let caret = range.start;
        self.clear_ime_selection();
        self.replace_ime_target_text(target, Some(range), "", cx);
        self.set_ime_selection_from_anchor(target, caret, caret);
        true
    }

    pub(super) fn handle_active_text_input_navigation(
        &mut self,
        keystroke: &Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(target) = self.active_ime_target(cx) else {
            return false;
        };
        let path_completion_visible = match target {
            WorkspaceImeTarget::FileManager(FileManagerInput::Path) => {
                self.file_manager.read(cx).path_completion.is_visible()
            }
            WorkspaceImeTarget::Sftp(SftpInput::LocalPath) => {
                self.sftp_view.read(cx).local_path_completion.is_visible()
            }
            WorkspaceImeTarget::Sftp(SftpInput::RemotePath) => {
                self.sftp_view.read(cx).remote_path_completion.is_visible()
            }
            _ => false,
        };
        if path_completion_owns_vertical_navigation(
            target,
            keystroke.key.as_str(),
            path_completion_visible,
            keystroke.modifiers.platform || keystroke.modifiers.control || keystroke.modifiers.alt,
        ) {
            // The owning surface accepts the key after the shared text-input handler declines it.
            return false;
        }
        if target == WorkspaceImeTarget::CommandPalette
            && matches!(
                keystroke.key.as_str(),
                "home" | "end" | "up" | "arrowup" | "down" | "arrowdown" | "pageup" | "pagedown"
            )
        {
            return false;
        }
        if target == WorkspaceImeTarget::AiChatInput
            && !keystroke.modifiers.shift
            && !keystroke.modifiers.platform
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.control
            && matches!(
                keystroke.key.as_str(),
                "up" | "arrowup" | "down" | "arrowdown"
            )
            && !self.ai_chat_autocomplete_items(cx).is_empty()
        {
            return false;
        }
        let Some(text) = self.text_for_ime_target(target, cx) else {
            return false;
        };
        let text_len = text.encode_utf16().count();
        let Some(selection) = self.ime_selection_for_navigation(target, text_len, cx) else {
            return false;
        };
        let Some(next) =
            self.text_input_navigation_destination(target, &text, &selection, keystroke)
        else {
            return false;
        };

        let (anchor, index) = if keystroke.modifiers.shift {
            (selection_anchor(&selection), next)
        } else {
            (next, next)
        };
        let desired_selection = selection_from_anchor(target, anchor, index);
        if desired_selection == selection
            && self.selected_ime_target.is_none()
            && self.marked_text_state_for_target(target, cx).is_none()
            && self.ime_drag_selection.is_none()
        {
            // Boundary navigation is a consumed browser input event, but an
            // unchanged caret/selection must not repaint the whole workspace.
            return true;
        }
        self.set_ime_selection_from_anchor(target, anchor, index);
        self.ime_marked_text = None;
        self.ime_drag_selection = None;
        self.show_active_input_caret(cx);
        cx.notify();
        true
    }

    pub(super) fn handle_active_text_input_newline(
        &mut self,
        keystroke: &Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        if keystroke.key.as_str() != "enter"
            || keystroke.modifiers.platform
            || keystroke.modifiers.alt
            || keystroke.modifiers.control
        {
            return false;
        }
        let Some(target) = self.active_ime_target(cx) else {
            return false;
        };
        if ime_target_is_read_only(target) {
            return false;
        }
        if !ime_target_accepts_newline(target) {
            return false;
        }
        if matches!(
            target,
            WorkspaceImeTarget::AiChatInput | WorkspaceImeTarget::AiMessageEdit
        ) && !keystroke.modifiers.shift
        {
            return false;
        }
        let Some(replacement_range) = self.ime_selection_range_for_target(target, cx) else {
            return false;
        };
        let caret = replacement_range.start + 1;
        self.clear_ime_selection();
        self.replace_ime_target_text(target, Some(replacement_range), "\n", cx);
        self.set_ime_selection_from_anchor(target, caret, caret);
        self.ime_marked_text = None;
        self.show_active_input_caret(cx);
        cx.notify();
        true
    }

    pub(super) fn handle_active_text_input_transpose(
        &mut self,
        keystroke: &Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        if keystroke.key.as_str() != "t"
            || !keystroke.modifiers.control
            || keystroke.modifiers.platform
            || keystroke.modifiers.alt
        {
            return false;
        }
        let Some(target) = self.active_ime_target(cx) else {
            return false;
        };
        if ime_target_is_read_only(target) {
            return false;
        }
        if ime_target_is_secret(target) {
            // Secret IME text is a geometry-only mask. Transposing that mask
            // would overwrite the real owner with proxy characters.
            return true;
        }
        let Some(text) = self.text_for_ime_target(target, cx) else {
            return false;
        };
        let Some(selection) = self.ime_selection_range_for_target(target, cx) else {
            return false;
        };
        if selection.start < selection.end {
            return true;
        }
        let Some((next_text, next_caret)) = transpose_text_at_utf16_offset(&text, selection.start)
        else {
            return true;
        };
        self.clear_ime_selection();
        let text_len = text.encode_utf16().count();
        self.replace_ime_target_text(target, Some(0..text_len), &next_text, cx);
        self.set_ime_selection_from_anchor(target, next_caret, next_caret);
        self.ime_marked_text = None;
        self.show_active_input_caret(cx);
        cx.notify();
        true
    }

    pub(super) fn copy_active_text_input(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(target) = self.active_ime_target(cx) else {
            return false;
        };
        let Some(text) = self.text_for_ime_target(target, cx) else {
            return false;
        };
        let selection = self.ime_selected_range_for_target(target, cx);
        match copy_shortcut_owner_for_target(target, selection.as_ref()) {
            CopyShortcutOwner::SelectedRange(range) => {
                cx.write_to_clipboard(ClipboardItem::new_string(utf16_slice(&text, range)));
                true
            }
            CopyShortcutOwner::FocusedEditableInput => true,
            CopyShortcutOwner::NextOwner => false,
        }
    }

    pub(super) fn cut_active_text_input(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(target) = self.active_ime_target(cx) else {
            return false;
        };
        let Some(text) = self.text_for_ime_target(target, cx) else {
            return false;
        };
        let Some(range) = self
            .ime_selected_range_for_target(target, cx)
            .filter(|range| range.start < range.end)
        else {
            return true;
        };
        let caret = range.start;
        cx.write_to_clipboard(ClipboardItem::new_string(utf16_slice(&text, range.clone())));
        self.clear_ime_selection();
        self.replace_ime_target_text(target, Some(range), "", cx);
        self.set_ime_selection_from_anchor(target, caret, caret);
        true
    }

    pub(super) fn paste_active_text_input(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(target) = self.active_ime_target(cx) else {
            return false;
        };
        if ime_target_is_read_only(target) {
            return true;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return true;
        };
        // Clipboard reads can contain private keys or passphrases. Own both the
        // platform result and normalized copy with zeroizing drop semantics.
        let clipboard_text = Zeroizing::new(text);
        let text = normalize_clipboard_text_for_ime_target(target, &clipboard_text);
        let replacement_range = self.ime_selection_range_for_target(target, cx);
        let caret = replacement_range
            .as_ref()
            .map(|range| range.start + text.encode_utf16().count());
        self.clear_ime_selection();
        self.replace_ime_target_text(target, replacement_range, &text, cx);
        if let Some(caret) = caret {
            self.set_ime_selection_from_anchor(target, caret, caret);
        }
        true
    }

    pub(super) fn select_all_active_text_input(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(target) = self.active_ime_target(cx) else {
            return false;
        };
        if self.text_for_ime_target(target, cx).is_none() {
            return false;
        }
        self.selected_ime_target = Some(target);
        self.selected_ime_range = None;
        self.ime_drag_selection = None;
        self.ime_marked_text = None;
        cx.notify();
        true
    }

    fn ime_selection_for_navigation(
        &self,
        target: WorkspaceImeTarget,
        text_len: usize,
        cx: &App,
    ) -> Option<WorkspaceImeSelection> {
        self.ime_selection_for_target(target)
            .or_else(|| {
                (self.selected_ime_target == Some(target)).then_some(WorkspaceImeSelection {
                    target,
                    range: 0..text_len,
                    reversed: false,
                })
            })
            .or_else(|| {
                (self.active_ime_target(cx) == Some(target)).then_some(WorkspaceImeSelection {
                    target,
                    range: text_len..text_len,
                    reversed: false,
                })
            })
    }

    fn text_input_navigation_destination(
        &self,
        target: WorkspaceImeTarget,
        text: &str,
        selection: &WorkspaceImeSelection,
        keystroke: &Keystroke,
    ) -> Option<usize> {
        let text_len = text.encode_utf16().count();
        let key = keystroke.key.as_str();
        let focus = selection_focus(selection);
        let has_selection = selection.range.start < selection.range.end;
        let is_multiline = ime_target_accepts_newline(target);
        let destination = match key {
            "a" if keystroke.modifiers.control => {
                if is_multiline {
                    line_start_for_utf16_offset(text, focus)
                } else {
                    0
                }
            }
            "e" if keystroke.modifiers.control => {
                if is_multiline {
                    line_end_for_utf16_offset(text, focus)
                } else {
                    text_len
                }
            }
            "b" if keystroke.modifiers.control => previous_utf16_boundary(text, focus),
            "f" if keystroke.modifiers.control => next_utf16_boundary(text, focus),
            "p" if keystroke.modifiers.control && is_multiline => {
                vertical_line_navigation_destination(text, focus, false)
            }
            "n" if keystroke.modifiers.control && is_multiline => {
                vertical_line_navigation_destination(text, focus, true)
            }
            "left" | "arrowleft" if keystroke.modifiers.platform && is_multiline => {
                line_start_for_utf16_offset(text, focus)
            }
            "right" | "arrowright" if keystroke.modifiers.platform && is_multiline => {
                line_end_for_utf16_offset(text, focus)
            }
            "left" | "arrowleft" if keystroke.modifiers.platform => 0,
            "right" | "arrowright" if keystroke.modifiers.platform => text_len,
            "left" | "arrowleft" if keystroke.modifiers.alt || keystroke.modifiers.control => {
                previous_word_boundary(text, focus)
            }
            "right" | "arrowright" if keystroke.modifiers.alt || keystroke.modifiers.control => {
                next_word_boundary(text, focus)
            }
            "left" | "arrowleft" if !keystroke.modifiers.shift && has_selection => {
                selection.range.start
            }
            "right" | "arrowright" if !keystroke.modifiers.shift && has_selection => {
                selection.range.end
            }
            "left" | "arrowleft" => previous_utf16_boundary(text, focus),
            "right" | "arrowright" => next_utf16_boundary(text, focus),
            "up" | "arrowup" if keystroke.modifiers.platform => 0,
            "down" | "arrowdown" if keystroke.modifiers.platform => text_len,
            "pageup" => 0,
            "pagedown" => text_len,
            "up" | "arrowup" if is_multiline => {
                vertical_line_navigation_destination(text, focus, false)
            }
            "down" | "arrowdown" if is_multiline => {
                vertical_line_navigation_destination(text, focus, true)
            }
            "up" | "arrowup" => 0,
            "down" | "arrowdown" => text_len,
            "home" if is_multiline => line_start_for_utf16_offset(text, focus),
            "end" if is_multiline => line_end_for_utf16_offset(text, focus),
            "home" => 0,
            "end" => text_len,
            _ => return None,
        };
        Some(destination.min(text_len))
    }

    fn text_input_delete_range_for_caret(
        &self,
        target: WorkspaceImeTarget,
        text: &str,
        caret: usize,
        keystroke: &Keystroke,
    ) -> Option<Range<usize>> {
        let text_len = text.encode_utf16().count();
        let is_multiline = ime_target_accepts_newline(target);
        match keystroke.key.as_str() {
            "backspace" if keystroke.modifiers.platform && is_multiline => {
                let line_start = line_start_for_utf16_offset(text, caret);
                Some(line_start..caret)
            }
            "delete" if keystroke.modifiers.platform && is_multiline => {
                let line_end = line_end_for_utf16_offset(text, caret);
                Some(caret..line_end)
            }
            "backspace" if keystroke.modifiers.platform && caret > 0 => Some(0..caret),
            "delete" if keystroke.modifiers.platform && caret < text_len => Some(caret..text_len),
            "h" if keystroke.modifiers.control && caret > 0 => {
                Some(previous_utf16_boundary(text, caret)..caret)
            }
            "d" if keystroke.modifiers.control && caret < text_len => {
                Some(caret..next_utf16_boundary(text, caret))
            }
            "k" if keystroke.modifiers.control && caret < text_len => {
                Some(caret..control_k_delete_end(text, caret))
            }
            "u" if keystroke.modifiers.control => {
                Some(line_start_for_utf16_offset(text, caret)..caret)
            }
            "backspace"
                if (keystroke.modifiers.alt || keystroke.modifiers.control) && caret > 0 =>
            {
                Some(previous_word_boundary(text, caret)..caret)
            }
            "delete"
                if (keystroke.modifiers.alt || keystroke.modifiers.control) && caret < text_len =>
            {
                Some(caret..next_word_boundary(text, caret))
            }
            "backspace"
                if !keystroke.modifiers.platform && !keystroke.modifiers.control && caret > 0 =>
            {
                Some(previous_utf16_boundary(text, caret)..caret)
            }
            "delete"
                if !keystroke.modifiers.platform
                    && !keystroke.modifiers.control
                    && caret < text_len =>
            {
                Some(caret..next_utf16_boundary(text, caret))
            }
            "backspace" | "delete" => Some(caret..caret),
            "h" | "d" | "k" | "u" if keystroke.modifiers.control => Some(caret..caret),
            _ => None,
        }
    }

    fn replace_host_tools_text_input(
        &mut self,
        input: HostToolsTextInput,
        replacement_range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        let changed = self.host_tools.update(cx, |host_tools, _cx| {
            host_tools.replace_text_input(input, replacement_range, text)
        });
        if changed {
            self.show_active_input_caret(cx);
            cx.notify();
        }
    }

    pub(super) fn replace_ime_target_text(
        &mut self,
        target: WorkspaceImeTarget,
        replacement_range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        match target {
            WorkspaceImeTarget::ReadOnlyText(_) => {}
            WorkspaceImeTarget::CommandPalette => {
                self.command_palette.update(cx, |palette, cx| {
                    palette.replace_query_utf16(replacement_range, text, cx);
                });
                self.show_active_input_caret(cx);
                cx.notify();
            }
            WorkspaceImeTarget::ShortcutsModalSearch => {
                replace_utf16(&mut self.shortcuts_modal.query, replacement_range, text);
                self.shortcuts_modal.scroll_handle = gpui::UniformListScrollHandle::new();
                self.show_active_input_caret(cx);
                cx.notify();
            }
            WorkspaceImeTarget::Search => {
                replace_utf16(&mut self.search.query, replacement_range, text);
                self.update_search_query(cx);
            }
            WorkspaceImeTarget::TerminalCwdSearch => {
                if self.terminal.update(cx, |terminal, _cx| {
                    terminal.replace_cwd_query(replacement_range, text)
                }) {
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::TerminalGitBranchSearch => {
                if self.terminal.update(cx, |terminal, _cx| {
                    terminal.replace_git_panel_query(replacement_range, text)
                }) {
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::TerminalGitCommitMessage => {
                if self.terminal.update(cx, |terminal, _cx| {
                    terminal.replace_git_commit_message(replacement_range, text)
                }) {
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::TerminalProjectSearch => {
                if self.terminal.read(cx).project_panel_open()
                    && let Some(key) = self.active_terminal_project_key(cx)
                    && self.terminal.update(cx, |terminal, _cx| {
                        terminal.replace_project_query(&key, replacement_range, text)
                    })
                {
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::TerminalBroadcastGroupName => {
                if self.terminal.update(cx, |terminal, _cx| {
                    terminal.replace_broadcast_group_editor_text(replacement_range, text)
                }) {
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::TerminalCastSearch => {
                if self.terminal.update(cx, |terminal, cx| {
                    terminal.replace_cast_search(replacement_range, text, cx)
                }) {
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::HostProcessSearch => {
                self.replace_host_tools_text_input(
                    HostToolsTextInput::ProcessSearch,
                    replacement_range,
                    text,
                    cx,
                );
            }
            WorkspaceImeTarget::HostProcessRenice => {
                self.replace_host_tools_text_input(
                    HostToolsTextInput::ProcessRenice,
                    replacement_range,
                    text,
                    cx,
                );
            }
            WorkspaceImeTarget::HostDockerSearch => {
                self.replace_host_tools_text_input(
                    HostToolsTextInput::DockerSearch,
                    replacement_range,
                    text,
                    cx,
                );
            }
            WorkspaceImeTarget::HostServiceSearch => {
                self.replace_host_tools_text_input(
                    HostToolsTextInput::ServiceSearch,
                    replacement_range,
                    text,
                    cx,
                );
            }
            WorkspaceImeTarget::HostLogSearch => {
                self.replace_host_tools_text_input(
                    HostToolsTextInput::LogSearch,
                    replacement_range,
                    text,
                    cx,
                );
            }
            WorkspaceImeTarget::HostTmuxSearch => {
                self.replace_host_tools_text_input(
                    HostToolsTextInput::TmuxSearch,
                    replacement_range,
                    text,
                    cx,
                );
            }
            WorkspaceImeTarget::HostTmuxDialogInput => {
                self.replace_host_tools_text_input(
                    HostToolsTextInput::TmuxDialog,
                    replacement_range,
                    text,
                    cx,
                );
            }
            WorkspaceImeTarget::HostPortSearch => {
                self.replace_host_tools_text_input(
                    HostToolsTextInput::PortSearch,
                    replacement_range,
                    text,
                    cx,
                );
            }
            WorkspaceImeTarget::HostScheduleSearch => {
                self.replace_host_tools_text_input(
                    HostToolsTextInput::ScheduleSearch,
                    replacement_range,
                    text,
                    cx,
                );
            }
            WorkspaceImeTarget::HostFilesystemSearch => {
                self.replace_host_tools_text_input(
                    HostToolsTextInput::FilesystemSearch,
                    replacement_range,
                    text,
                    cx,
                );
            }
            WorkspaceImeTarget::HostPackageSearch => {
                self.replace_host_tools_text_input(
                    HostToolsTextInput::PackageSearch,
                    replacement_range,
                    text,
                    cx,
                );
            }
            WorkspaceImeTarget::QuickCommand(input) => {
                if self.terminal.update(cx, |terminal, _cx| {
                    terminal
                        .quick_commands
                        .replace_input(input, replacement_range, text)
                }) {
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::Settings(input) => {
                let entity_input_focused = self
                    .settings_workspace
                    .read(cx)
                    .settings_entity_focused_input()
                    == Some(input);
                if entity_input_focused {
                    self.settings_workspace.update(cx, |settings, cx| {
                        settings.replace_settings_entity_input(input, replacement_range, text, cx);
                    });
                } else if self.ai_entity.read(cx).focused_settings_input() == Some(input) {
                    self.ai_entity.update(cx, |ai, cx| {
                        ai.replace_settings_input(input, replacement_range, text, cx);
                    });
                } else if self.focused_settings_input == Some(input) {
                    replace_utf16(&mut self.settings_input_draft, replacement_range, text);
                    self.apply_settings_input_draft(input, cx);
                }
            }
            WorkspaceImeTarget::SessionManager(input) => {
                let search_changed = self.session_manager.update(cx, |session_manager, cx| {
                    if session_manager.focused_input() != Some(input) {
                        return false;
                    }
                    // The Entity owns secret buffers and applies the platform
                    // replacement without copying their contents to WorkspaceApp.
                    if !session_manager.replace_input(input, replacement_range, text, cx) {
                        return false;
                    }
                    input == SessionManagerInput::Search
                });
                if search_changed {
                    self.clear_session_selection_for_invisible_rows(cx);
                }
            }
            WorkspaceImeTarget::Forwards(input) => {
                if self.forwarding.read(cx).view().focused_input == Some(input) {
                    self.forwarding.update(cx, |forwarding, _cx| {
                        forwarding.replace_input_text(input, replacement_range, text);
                    });
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::FileManager(input) => {
                if self.file_manager.update(cx, |file_manager, cx| {
                    file_manager.replace_input(input, replacement_range, text, cx)
                }) {
                    if input == FileManagerInput::Path {
                        self.refresh_file_manager_path_completion(cx);
                    }
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::Launcher(input) => {
                if self.launcher.update(cx, |launcher, cx| {
                    launcher.replace_input(input, replacement_range, text, cx)
                }) {
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::Graphics(input) => {
                if self.graphics.update(cx, |graphics, cx| {
                    graphics.replace_input(input, replacement_range, text, cx)
                }) {
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::TabRename => {
                if let Some(dialog) = self.tab_rename_dialog.as_mut() {
                    replace_utf16(&mut dialog.draft, replacement_range, text);
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::AiModelSelectorSearch => {
                if self.ai_entity.read(cx).model_selector_search_focused() {
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.replace_model_selector_search(replacement_range, text);
                    });
                    // Search changes rebuild the visible model rows; clear the
                    // Radix-style active item so keyboard focus cannot point at
                    // a filtered-out model.
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::AiInlinePrompt => {
                let changed = self.ai_entity.update(cx, |ai, _cx| {
                    let panel = ai.terminal_inline_panel_mut();
                    if !panel.prompt_focused {
                        return false;
                    }
                    replace_utf16(&mut panel.prompt, replacement_range, text);
                    panel.error = None;
                    true
                });
                if changed {
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::AiChatInput => {
                let changed = self
                    .ai_entity
                    .update(cx, |ai, _cx| ai.replace_chat_input(replacement_range, text));
                if changed {
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::AiConversationRename => {
                let changed = self.ai_entity.update(cx, |ai, _cx| {
                    ai.replace_conversation_rename(replacement_range, text)
                });
                if changed {
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::AiMessageEdit => {
                let changed = self.ai_entity.update(cx, |ai, _cx| {
                    ai.replace_message_edit(replacement_range, text)
                });
                if changed {
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::PluginControl { key, .. } => {
                if self.plugin_ui_state(cx).focused_input == Some(key)
                    && self.native_plugin_ui_control_is_visible(key, cx)
                {
                    self.update_plugin_ui_state(cx, |ui| {
                        if let Some(value) = ui.text_mut(key) {
                            replace_utf16(value, replacement_range, text);
                        }
                    });
                    self.show_active_input_caret(cx);
                    self.dispatch_native_plugin_ui_input_event(key, cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::Sftp(input) => {
                if self.sftp_view.read(cx).focused_input() == Some(input) {
                    self.sftp_view.update(cx, |sftp, _cx| {
                        replace_utf16(sftp.input_value_mut(input), replacement_range, text);
                    });
                    if matches!(input, SftpInput::LocalPath | SftpInput::RemotePath) {
                        self.refresh_sftp_path_completion(input, cx);
                    }
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::NewConnection(field) => {
                let changed = self.update_connection_form_state(cx, |state| {
                    let Some(form) = state.form.as_mut() else {
                        return false;
                    };
                    if form.selected_field == Some(field) && replacement_range.is_none() {
                        *connection_field_value_mut(form, field) = String::new();
                    }
                    replace_utf16(
                        connection_field_value_mut(form, field),
                        replacement_range,
                        text,
                    );
                    form.selected_field = None;
                    form.error = None;
                    refresh_connection_timeout_seconds(form, field);
                    if field == NewConnectionField::IdentityAgent {
                        refresh_identity_agent_availability(form);
                    }
                    true
                });
                if changed {
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
            WorkspaceImeTarget::KeyboardInteractive(index) => {
                let replaced = self.connection_flow.update(cx, |connection_flow, cx| {
                    connection_flow.replace_keyboard_interactive_response(
                        index,
                        replacement_range,
                        text,
                        cx,
                    )
                });
                if replaced {
                    self.show_active_input_caret(cx);
                    cx.notify();
                }
            }
        }
    }
}

fn is_terminal_tab(tab: &oxideterm_workspace::Tab) -> bool {
    matches!(
        tab.kind,
        oxideterm_workspace::TabKind::LocalTerminal
            | oxideterm_workspace::TabKind::SshTerminal
            | oxideterm_workspace::TabKind::MoshTerminal
    )
}

fn new_connection_field_value(
    form: &super::new_connection::NewConnectionForm,
    field: NewConnectionField,
) -> Option<&str> {
    Some(match field {
        NewConnectionField::Name => &form.name,
        NewConnectionField::Host => &form.host,
        NewConnectionField::Port => &form.port,
        NewConnectionField::Username => &form.username,
        NewConnectionField::Password => &form.password,
        NewConnectionField::KeyPath => &form.key_path,
        NewConnectionField::ManagedKeyId => &form.managed_key_id,
        NewConnectionField::CertPath => &form.cert_path,
        NewConnectionField::Passphrase => &form.passphrase,
        NewConnectionField::GssapiServerIdentity => &form.gssapi_server_identity,
        NewConnectionField::IdentityAgent => &form.identity_agent,
        NewConnectionField::Group => &form.group,
        NewConnectionField::Notes => &form.notes,
        NewConnectionField::PostConnectCommand => &form.post_connect_command,
        NewConnectionField::ProxyCommand => &form.proxy_command,
        NewConnectionField::UpstreamProxyHost => &form.upstream_proxy_host,
        NewConnectionField::UpstreamProxyPort => &form.upstream_proxy_port,
        NewConnectionField::UpstreamProxyNoProxy => &form.upstream_proxy_no_proxy,
        NewConnectionField::UpstreamProxyUsername => &form.upstream_proxy_username,
        NewConnectionField::UpstreamProxyPassword => &form.upstream_proxy_password,
        NewConnectionField::Color => &form.color,
        NewConnectionField::IconBackgroundColor => &form.icon_background_color,
        NewConnectionField::SerialPortPath => &form.serial_port_path,
        NewConnectionField::SerialBaudRate => &form.serial_baud_rate,
        NewConnectionField::SerialProfileName => &form.serial_profile_name,
        NewConnectionField::TelnetProfileName => &form.telnet_profile_name,
        NewConnectionField::MoshServerExecutable => &form.mosh_server_executable,
        NewConnectionField::MoshUdpHost => &form.mosh_udp_host,
        NewConnectionField::MoshUdpPort => &form.mosh_udp_port,
        NewConnectionField::MoshLocale => &form.mosh_locale,
        NewConnectionField::InitialRemotePath => &form.sftp_initial_remote_path,
        NewConnectionField::ConnectTimeoutSeconds => &form.connect_timeout_seconds_text,
        NewConnectionField::StandaloneSftpSecondaryHost => &form.standalone_sftp_secondary.host,
        NewConnectionField::StandaloneSftpSecondaryPort => &form.standalone_sftp_secondary.port,
        NewConnectionField::StandaloneSftpSecondaryUsername => {
            &form.standalone_sftp_secondary.username
        }
        NewConnectionField::StandaloneSftpSecondaryPassword => {
            &form.standalone_sftp_secondary.password
        }
        NewConnectionField::StandaloneSftpSecondaryKeyPath => {
            &form.standalone_sftp_secondary.key_path
        }
        NewConnectionField::StandaloneSftpSecondaryManagedKeyId => {
            &form.standalone_sftp_secondary.managed_key_id
        }
        NewConnectionField::StandaloneSftpSecondaryCertPath => {
            &form.standalone_sftp_secondary.cert_path
        }
        NewConnectionField::StandaloneSftpSecondaryPassphrase => {
            &form.standalone_sftp_secondary.passphrase
        }
        NewConnectionField::StandaloneSftpSecondaryGssapiServerIdentity => {
            &form.standalone_sftp_secondary.gssapi_server_identity
        }
        NewConnectionField::StandaloneSftpSecondaryIdentityAgent => {
            &form.standalone_sftp_secondary.identity_agent
        }
        NewConnectionField::StandaloneSftpSecondaryInitialRemotePath => {
            &form.standalone_sftp_secondary.initial_remote_path
        }
        NewConnectionField::StandaloneSftpSecondaryConnectTimeoutSeconds => {
            &form.standalone_sftp_secondary.connect_timeout_seconds_text
        }
        NewConnectionField::StandaloneSftpSecondaryProxyCommand => {
            &form.standalone_sftp_secondary.proxy_command
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyHost => {
            &form.standalone_sftp_secondary.upstream_proxy_host
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyPort => {
            &form.standalone_sftp_secondary.upstream_proxy_port
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyNoProxy => {
            &form.standalone_sftp_secondary.upstream_proxy_no_proxy
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyUsername => {
            &form.standalone_sftp_secondary.upstream_proxy_username
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyPassword => {
            &form.standalone_sftp_secondary.upstream_proxy_password
        }
        NewConnectionField::JumpHost => &form.jump_server_form.as_ref()?.host,
        NewConnectionField::JumpPort => &form.jump_server_form.as_ref()?.port,
        NewConnectionField::JumpUsername => &form.jump_server_form.as_ref()?.username,
        NewConnectionField::JumpPassword => &form.jump_server_form.as_ref()?.password,
        NewConnectionField::JumpKeyPath => &form.jump_server_form.as_ref()?.key_path,
        NewConnectionField::JumpManagedKeyId => &form.jump_server_form.as_ref()?.managed_key_id,
        NewConnectionField::JumpCertPath => &form.jump_server_form.as_ref()?.cert_path,
        NewConnectionField::JumpPassphrase => &form.jump_server_form.as_ref()?.passphrase,
        NewConnectionField::JumpGssapiServerIdentity => {
            &form.jump_server_form.as_ref()?.gssapi_server_identity
        }
        NewConnectionField::JumpIdentityAgent => &form.jump_server_form.as_ref()?.identity_agent,
    })
}

fn connection_field_value_mut(
    form: &mut super::new_connection::NewConnectionForm,
    field: NewConnectionField,
) -> &mut String {
    match field {
        NewConnectionField::Name => &mut form.name,
        NewConnectionField::Host => &mut form.host,
        NewConnectionField::Port => &mut form.port,
        NewConnectionField::Username => &mut form.username,
        NewConnectionField::Password => &mut form.password,
        NewConnectionField::KeyPath => &mut form.key_path,
        NewConnectionField::ManagedKeyId => &mut form.managed_key_id,
        NewConnectionField::CertPath => &mut form.cert_path,
        NewConnectionField::Passphrase => &mut form.passphrase,
        NewConnectionField::GssapiServerIdentity => &mut form.gssapi_server_identity,
        NewConnectionField::IdentityAgent => &mut form.identity_agent,
        NewConnectionField::Group => &mut form.group,
        NewConnectionField::Notes => &mut form.notes,
        NewConnectionField::PostConnectCommand => &mut form.post_connect_command,
        NewConnectionField::ProxyCommand => &mut form.proxy_command,
        NewConnectionField::UpstreamProxyHost => &mut form.upstream_proxy_host,
        NewConnectionField::UpstreamProxyPort => &mut form.upstream_proxy_port,
        NewConnectionField::UpstreamProxyNoProxy => &mut form.upstream_proxy_no_proxy,
        NewConnectionField::UpstreamProxyUsername => &mut form.upstream_proxy_username,
        NewConnectionField::UpstreamProxyPassword => &mut form.upstream_proxy_password,
        NewConnectionField::Color => &mut form.color,
        NewConnectionField::IconBackgroundColor => &mut form.icon_background_color,
        NewConnectionField::SerialPortPath => &mut form.serial_port_path,
        NewConnectionField::SerialBaudRate => &mut form.serial_baud_rate,
        NewConnectionField::SerialProfileName => &mut form.serial_profile_name,
        NewConnectionField::TelnetProfileName => &mut form.telnet_profile_name,
        NewConnectionField::MoshServerExecutable => &mut form.mosh_server_executable,
        NewConnectionField::MoshUdpHost => &mut form.mosh_udp_host,
        NewConnectionField::MoshUdpPort => &mut form.mosh_udp_port,
        NewConnectionField::MoshLocale => &mut form.mosh_locale,
        NewConnectionField::InitialRemotePath => &mut form.sftp_initial_remote_path,
        NewConnectionField::ConnectTimeoutSeconds => &mut form.connect_timeout_seconds_text,
        NewConnectionField::StandaloneSftpSecondaryHost => &mut form.standalone_sftp_secondary.host,
        NewConnectionField::StandaloneSftpSecondaryPort => &mut form.standalone_sftp_secondary.port,
        NewConnectionField::StandaloneSftpSecondaryUsername => {
            &mut form.standalone_sftp_secondary.username
        }
        NewConnectionField::StandaloneSftpSecondaryPassword => {
            &mut form.standalone_sftp_secondary.password
        }
        NewConnectionField::StandaloneSftpSecondaryKeyPath => {
            &mut form.standalone_sftp_secondary.key_path
        }
        NewConnectionField::StandaloneSftpSecondaryManagedKeyId => {
            &mut form.standalone_sftp_secondary.managed_key_id
        }
        NewConnectionField::StandaloneSftpSecondaryCertPath => {
            &mut form.standalone_sftp_secondary.cert_path
        }
        NewConnectionField::StandaloneSftpSecondaryPassphrase => {
            &mut form.standalone_sftp_secondary.passphrase
        }
        NewConnectionField::StandaloneSftpSecondaryGssapiServerIdentity => {
            &mut form.standalone_sftp_secondary.gssapi_server_identity
        }
        NewConnectionField::StandaloneSftpSecondaryIdentityAgent => {
            &mut form.standalone_sftp_secondary.identity_agent
        }
        NewConnectionField::StandaloneSftpSecondaryInitialRemotePath => {
            &mut form.standalone_sftp_secondary.initial_remote_path
        }
        NewConnectionField::StandaloneSftpSecondaryConnectTimeoutSeconds => {
            &mut form.standalone_sftp_secondary.connect_timeout_seconds_text
        }
        NewConnectionField::StandaloneSftpSecondaryProxyCommand => {
            &mut form.standalone_sftp_secondary.proxy_command
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyHost => {
            &mut form.standalone_sftp_secondary.upstream_proxy_host
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyPort => {
            &mut form.standalone_sftp_secondary.upstream_proxy_port
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyNoProxy => {
            &mut form.standalone_sftp_secondary.upstream_proxy_no_proxy
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyUsername => {
            &mut form.standalone_sftp_secondary.upstream_proxy_username
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyPassword => {
            &mut form.standalone_sftp_secondary.upstream_proxy_password
        }
        NewConnectionField::JumpHost => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump host field without jump form")
                .host
        }
        NewConnectionField::JumpPort => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump port field without jump form")
                .port
        }
        NewConnectionField::JumpUsername => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump username field without jump form")
                .username
        }
        NewConnectionField::JumpPassword => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump password field without jump form")
                .password
        }
        NewConnectionField::JumpKeyPath => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump key path field without jump form")
                .key_path
        }
        NewConnectionField::JumpManagedKeyId => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump managed key field without jump form")
                .managed_key_id
        }
        NewConnectionField::JumpCertPath => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump cert path field without jump form")
                .cert_path
        }
        NewConnectionField::JumpPassphrase => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump passphrase field without jump form")
                .passphrase
        }
        NewConnectionField::JumpGssapiServerIdentity => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump Kerberos server field without jump form")
                .gssapi_server_identity
        }
        NewConnectionField::JumpIdentityAgent => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump identity agent field without jump form")
                .identity_agent
        }
    }
}

fn normalize_clipboard_text_for_ime_target(
    target: WorkspaceImeTarget,
    text: &str,
) -> Zeroizing<String> {
    // Normalize in one zeroizing allocation so CRLF conversion never creates
    // an unprotected intermediate copy of secret clipboard contents.
    let mut normalized = Zeroizing::new(String::with_capacity(text.len()));
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\r' {
            if characters.peek() == Some(&'\n') {
                characters.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(character);
        }
    }
    if ime_target_accepts_newline(target) {
        normalized
    } else {
        Zeroizing::new(normalized.lines().collect::<Vec<_>>().join(" "))
    }
}

fn ime_target_accepts_newline(target: WorkspaceImeTarget) -> bool {
    match target {
        WorkspaceImeTarget::ReadOnlyText(_) => true,
        WorkspaceImeTarget::Settings(input) => input.accepts_newline(),
        WorkspaceImeTarget::AiChatInput | WorkspaceImeTarget::AiMessageEdit => true,
        WorkspaceImeTarget::NewConnection(NewConnectionField::Notes) => true,
        WorkspaceImeTarget::SessionManager(SessionManagerInput::OxideExportDescription) => true,
        _ => false,
    }
}

fn ime_target_is_read_only(target: WorkspaceImeTarget) -> bool {
    matches!(target, WorkspaceImeTarget::ReadOnlyText(_))
}

fn ime_target_is_secret(target: WorkspaceImeTarget) -> bool {
    matches!(
        target,
        WorkspaceImeTarget::NewConnection(
            NewConnectionField::Password
                | NewConnectionField::Passphrase
                | NewConnectionField::UpstreamProxyPassword
                | NewConnectionField::JumpPassword
                | NewConnectionField::JumpPassphrase
        ) | WorkspaceImeTarget::KeyboardInteractive(_)
            | WorkspaceImeTarget::HostTmuxDialogInput
    ) || matches!(target, WorkspaceImeTarget::Settings(input) if input.is_secret())
        || matches!(target, WorkspaceImeTarget::SessionManager(input) if input.is_secret())
        || matches!(
            target,
            WorkspaceImeTarget::PluginControl { secret: true, .. }
        )
}

fn ime_target_should_blink_caret(target: WorkspaceImeTarget) -> bool {
    !ime_target_is_read_only(target)
}

fn collapsed_copy_shortcut_is_owned_by_target(target: WorkspaceImeTarget) -> bool {
    !ime_target_is_read_only(target)
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CopyShortcutOwner {
    SelectedRange(Range<usize>),
    FocusedEditableInput,
    NextOwner,
}

fn copy_shortcut_owner_for_target(
    target: WorkspaceImeTarget,
    selection: Option<&Range<usize>>,
) -> CopyShortcutOwner {
    if let Some(range) = selection.filter(|range| range.start < range.end) {
        return CopyShortcutOwner::SelectedRange(range.clone());
    }
    if collapsed_copy_shortcut_is_owned_by_target(target) {
        // Browser inputs own Cmd-C even with a collapsed caret. Read-only page
        // selections do not, so terminal selection/app copy can run next.
        CopyShortcutOwner::FocusedEditableInput
    } else {
        CopyShortcutOwner::NextOwner
    }
}

// Mirrors the browser selection shape used by Shift+navigation and mouse drag.
fn selection_from_anchor(
    target: WorkspaceImeTarget,
    anchor: usize,
    index: usize,
) -> WorkspaceImeSelection {
    if anchor == index {
        WorkspaceImeSelection {
            target,
            range: index..index,
            reversed: false,
        }
    } else if index < anchor {
        WorkspaceImeSelection {
            target,
            range: index..anchor,
            reversed: true,
        }
    } else {
        WorkspaceImeSelection {
            target,
            range: anchor..index,
            reversed: false,
        }
    }
}

fn selection_focus(selection: &WorkspaceImeSelection) -> usize {
    if selection.reversed {
        selection.range.start
    } else {
        selection.range.end
    }
}

fn selection_anchor(selection: &WorkspaceImeSelection) -> usize {
    if selection.reversed {
        selection.range.end
    } else {
        selection.range.start
    }
}

fn soft_wrapped_line_ranges_utf16(
    value: &str,
    max_width_px: f32,
    bounds_height_px: f32,
) -> Vec<Range<usize>> {
    let hard_ranges = line_ranges_utf16(value);
    if value.is_empty() || max_width_px <= 1.0 {
        return hard_ranges;
    }

    let target_lines = (bounds_height_px / READ_ONLY_TEXT_LINE_HEIGHT_ESTIMATE)
        .round()
        .max(hard_ranges.len() as f32) as usize;
    let mut scale = 1.0;
    for _ in 0..8 {
        let lines = soft_wrapped_line_ranges_with_scale(value, max_width_px, scale);
        if lines.len() == target_lines || target_lines <= hard_ranges.len() {
            return lines;
        }
        if lines.len() < target_lines {
            scale *= 1.12;
        } else {
            scale *= 0.92;
        }
    }
    soft_wrapped_line_ranges_with_scale(value, max_width_px, scale)
}

fn soft_wrapped_line_ranges_with_scale(
    value: &str,
    max_width_px: f32,
    scale: f32,
) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut line_start = 0usize;
    let mut line_width = 0.0f32;
    let mut offset = 0usize;
    let mut last_break: Option<(usize, f32)> = None;

    for ch in value.chars() {
        let char_len = ch.len_utf16();
        if ch == '\n' {
            lines.push(line_start..offset);
            offset += char_len;
            line_start = offset;
            line_width = 0.0;
            last_break = None;
            continue;
        }

        let char_width = estimated_read_only_char_width(ch) * scale;
        if line_width + char_width > max_width_px && offset > line_start {
            if let Some((break_offset, break_width)) = last_break.take()
                && break_offset > line_start
            {
                lines.push(line_start..break_offset);
                line_start = break_offset;
                line_width = (line_width - break_width).max(0.0);
            } else {
                lines.push(line_start..offset);
                line_start = offset;
                line_width = 0.0;
            }
        }

        line_width += char_width;
        offset += char_len;
        if ch.is_whitespace() || matches!(ch, '-' | '/' | '\\' | ',' | '.' | ';' | ':') {
            last_break = Some((offset, line_width));
        }
    }

    lines.push(line_start..offset);
    lines
}

fn estimated_read_only_char_width(ch: char) -> f32 {
    if ch == '\t' {
        READ_ONLY_TEXT_EM_WIDTH * 1.8
    } else if ch.is_whitespace() {
        READ_ONLY_TEXT_EM_WIDTH * 0.35
    } else if ch.is_ascii() {
        READ_ONLY_TEXT_EM_WIDTH * 0.58
    } else if ch.len_utf16() > 1 {
        READ_ONLY_TEXT_EM_WIDTH * 1.1
    } else {
        READ_ONLY_TEXT_EM_WIDTH
    }
}

fn platform_text_commit_is_duplicate(
    pending_commit: &mut Option<PendingPlatformTextCommit>,
    target: WorkspaceImeTarget,
    text: &str,
) -> bool {
    let Some(pending) = pending_commit.as_mut() else {
        return false;
    };
    if pending.target != target || pending.text.as_str() != text {
        return false;
    }
    if pending.consumed {
        *pending_commit = None;
        return true;
    }
    pending.consumed = true;
    false
}

fn effective_platform_text_replacement_range(
    platform_range: Option<Range<usize>>,
    current_selection: impl FnOnce() -> Option<Range<usize>>,
    marked_text: Option<&WorkspaceImeMarkedText>,
) -> Option<Range<usize>> {
    if let Some(platform_range) = platform_range {
        if let Some(marked_text) = marked_text
            && platform_range == marked_text.virtual_range()
        {
            return Some(marked_text.replacement_range.clone());
        }
        return Some(platform_range);
    }
    // GPUI may deliver plain printable text without an explicit replacement
    // range. Browser inputs still insert at the live caret, so fall back to the
    // selection state maintained from mouse clicks and keyboard navigation.
    current_selection()
}

fn path_completion_owns_vertical_navigation(
    target: WorkspaceImeTarget,
    key: &str,
    completion_visible: bool,
    has_command_modifier: bool,
) -> bool {
    completion_visible
        && !has_command_modifier
        && matches!(key, "up" | "arrowup" | "down" | "arrowdown")
        && matches!(
            target,
            WorkspaceImeTarget::FileManager(FileManagerInput::Path)
                | WorkspaceImeTarget::Sftp(SftpInput::LocalPath)
                | WorkspaceImeTarget::Sftp(SftpInput::RemotePath)
        )
}

#[cfg(test)]
mod tests {
    use gpui::{Keystroke, Modifiers};
    use zeroize::{Zeroize, Zeroizing};

    use super::{
        CopyShortcutOwner, FileManagerInput, HostToolsPlainTextImeFrame, HostToolsTextInput,
        NewConnectionField, PendingPlatformTextCommit, SettingsInput, SftpInput,
        TextInputAnchorStore, WorkspaceCaretState, WorkspaceCaretVisibility,
        WorkspaceImeMarkedText, WorkspaceImeTarget, active_ime_should_defer_input_key,
        collapsed_copy_shortcut_is_owned_by_target, control_k_delete_end,
        copy_shortcut_owner_for_target, effective_platform_text_replacement_range,
        ime_target_is_secret, ime_text_snapshot, keystroke_platform_text,
        keystroke_uses_text_edit_modifier, line_end_for_utf16_offset, line_range_for_utf16_offset,
        line_start_for_utf16_offset, next_utf16_boundary, next_word_boundary,
        normalize_clipboard_text_for_ime_target, path_completion_owns_vertical_navigation,
        platform_text_commit_is_duplicate, previous_utf16_boundary, previous_word_boundary,
        secret_ime_proxy, soft_wrapped_line_ranges_utf16, transpose_text_at_utf16_offset,
        utf16_offset_for_char_index, vertical_line_navigation_destination,
        word_range_for_utf16_offset, workspace_ime_target_for_plain_host_tools_input,
    };

    fn key(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            key: key.to_string(),
            key_char: key_char.map(str::to_string),
            modifiers,
        }
    }

    #[test]
    fn caret_state_pauses_settings_blink_until_scroll_deadline() {
        let now = std::time::Instant::now();
        let visibility = WorkspaceCaretVisibility::default();
        let mut caret = WorkspaceCaretState::new(visibility.clone());
        let target = WorkspaceImeTarget::Settings(SettingsInput::KeybindingSearch);
        assert!(caret.sync_active_target(Some(target)));
        assert!(caret.pause_settings_caret(now + std::time::Duration::from_millis(700)));
        assert_eq!(
            caret.next_tick_delay(now),
            Some(std::time::Duration::from_millis(700))
        );
        assert!(!caret.advance_tick(now + std::time::Duration::from_millis(699)));
        assert!(visibility.visible());
        assert!(caret.advance_tick(now + std::time::Duration::from_millis(700)));
        assert!(!visibility.visible());
    }

    #[test]
    fn caret_state_resets_phase_and_pause_when_visible_owner_changes() {
        let now = std::time::Instant::now();
        let visibility = WorkspaceCaretVisibility::default();
        let mut caret = WorkspaceCaretState::new(visibility.clone());
        let settings_target = WorkspaceImeTarget::Settings(SettingsInput::KeybindingSearch);
        caret.sync_active_target(Some(settings_target));
        caret.pause_settings_caret(now + std::time::Duration::from_secs(1));
        caret.advance_tick(now + std::time::Duration::from_secs(1));
        assert!(!visibility.visible());

        assert!(caret.sync_active_target(Some(WorkspaceImeTarget::Search)));
        assert!(visibility.visible());
        assert_eq!(
            caret.next_tick_delay(now),
            Some(super::CARET_BLINK_INTERVAL)
        );
        assert!(caret.sync_active_target(Some(WorkspaceImeTarget::ReadOnlyText(7))));
        assert_eq!(caret.next_tick_delay(now), None);
    }

    #[test]
    fn platform_text_input_accepts_printable_text_and_rejects_manual_keys() {
        assert!(keystroke_platform_text(&key("a", Some("a"), Modifiers::default())).is_some());
        assert!(keystroke_platform_text(&key("space", Some(" "), Modifiers::default())).is_some());
        assert!(
            keystroke_platform_text(&key(
                "s",
                Some("ß"),
                Modifiers {
                    alt: true,
                    ..Modifiers::default()
                }
            ))
            .is_some()
        );
        assert!(keystroke_platform_text(&key("backspace", None, Modifiers::default())).is_none());
        assert!(
            keystroke_platform_text(&key(
                "v",
                None,
                Modifiers {
                    platform: true,
                    ..Modifiers::default()
                }
            ))
            .is_none()
        );
        assert!(
            keystroke_platform_text(&key(
                "a",
                Some("\u{1}"),
                Modifiers {
                    control: true,
                    ..Modifiers::default()
                }
            ))
            .is_none()
        );
    }

    #[test]
    fn platform_edit_shortcut_uses_expected_modifier_for_target_os() {
        let platform_v = key(
            "v",
            None,
            Modifiers {
                platform: true,
                ..Modifiers::default()
            },
        );
        let control_v = key(
            "v",
            None,
            Modifiers {
                control: true,
                ..Modifiers::default()
            },
        );

        assert!(keystroke_uses_text_edit_modifier(&platform_v));
        if cfg!(target_os = "macos") {
            assert!(!keystroke_uses_text_edit_modifier(&control_v));
        } else {
            assert!(keystroke_uses_text_edit_modifier(&control_v));
        }
    }

    #[test]
    fn active_ime_defers_only_platform_owned_text_and_composition_keys() {
        let printable = key("a", Some("a"), Modifiers::default());
        let shortcut = key(
            "a",
            Some("a"),
            Modifiers {
                platform: true,
                ..Modifiers::default()
            },
        );

        assert!(active_ime_should_defer_input_key(true, false, &printable));
        assert!(!active_ime_should_defer_input_key(false, false, &printable));
        assert!(!active_ime_should_defer_input_key(true, false, &shortcut));
        let space = key("space", None, Modifiers::default());
        let enter = key("enter", None, Modifiers::default());
        let modified_space = key(
            "space",
            None,
            Modifiers {
                control: true,
                ..Modifiers::default()
            },
        );

        assert!(active_ime_should_defer_input_key(true, true, &space));
        assert!(active_ime_should_defer_input_key(true, true, &enter));
        assert!(!active_ime_should_defer_input_key(true, false, &enter));
        assert!(!active_ime_should_defer_input_key(
            true,
            true,
            &modified_space
        ));
    }

    #[test]
    fn plain_host_tools_ime_frame_rejects_secret_tmux_dialog_input() {
        assert_eq!(
            workspace_ime_target_for_plain_host_tools_input(HostToolsTextInput::ProcessRenice),
            Some(WorkspaceImeTarget::HostProcessRenice)
        );
        assert_eq!(
            workspace_ime_target_for_plain_host_tools_input(HostToolsTextInput::TmuxDialog),
            None
        );
        assert!(
            HostToolsPlainTextImeFrame::new(
                HostToolsTextInput::TmuxDialog,
                true,
                None,
                None,
                TextInputAnchorStore::default(),
            )
            .is_none()
        );
    }

    #[test]
    fn platform_text_commit_dedupes_only_same_deferred_key() {
        let mut pending = Some(PendingPlatformTextCommit {
            target: WorkspaceImeTarget::CommandPalette,
            text: Zeroizing::new("a".to_string()),
            generation: 7,
            consumed: false,
        });

        assert!(!platform_text_commit_is_duplicate(
            &mut pending,
            WorkspaceImeTarget::CommandPalette,
            "a",
        ));
        assert!(platform_text_commit_is_duplicate(
            &mut pending,
            WorkspaceImeTarget::CommandPalette,
            "a",
        ));
        assert_eq!(pending, None);

        let mut next_key = Some(PendingPlatformTextCommit {
            target: WorkspaceImeTarget::CommandPalette,
            text: Zeroizing::new("a".to_string()),
            generation: 8,
            consumed: false,
        });
        assert!(!platform_text_commit_is_duplicate(
            &mut next_key,
            WorkspaceImeTarget::CommandPalette,
            "a",
        ));
    }

    #[test]
    fn platform_text_commit_does_not_dedupe_other_targets_or_text() {
        let mut pending = Some(PendingPlatformTextCommit {
            target: WorkspaceImeTarget::CommandPalette,
            text: Zeroizing::new("a".to_string()),
            generation: 1,
            consumed: true,
        });

        assert!(!platform_text_commit_is_duplicate(
            &mut pending,
            WorkspaceImeTarget::ShortcutsModalSearch,
            "a",
        ));
        assert!(!platform_text_commit_is_duplicate(
            &mut pending,
            WorkspaceImeTarget::CommandPalette,
            "b",
        ));
        assert!(pending.is_some());
    }

    #[test]
    fn platform_text_commit_resolves_explicit_current_and_marked_ranges() {
        let cases = [
            (None, Some(2..2), Some(2..2)),
            (None, Some(1..4), Some(1..4)),
            (Some(5..6), Some(1..4), Some(5..6)),
        ];
        for (platform_range, current_range, expected) in cases {
            assert_eq!(
                effective_platform_text_replacement_range(
                    platform_range,
                    || current_range.clone(),
                    None,
                ),
                expected
            );
        }

        let marked = WorkspaceImeMarkedText {
            target: WorkspaceImeTarget::CommandPalette,
            replacement_range: 2..2,
            text: Zeroizing::new("拼".to_string()),
        };
        let virtual_range = marked.virtual_range();

        let range = effective_platform_text_replacement_range(
            Some(virtual_range),
            || Some(9..9),
            Some(&marked),
        );

        assert_eq!(range, Some(2..2));
    }

    #[test]
    fn read_only_soft_wrap_ranges_follow_visual_line_count() {
        let text = "你好！我是 OxideSens，你的终端助手。我可以帮助你处理终端命令、SSH 连接、文件操作、脚本调试等等。";
        let ranges = soft_wrapped_line_ranges_utf16(text, 260.0, 112.0);
        assert!(ranges.len() >= 3, "{ranges:?}");
        assert_eq!(ranges.first().map(|range| range.start), Some(0));
        assert_eq!(
            ranges.last().map(|range| range.end),
            Some(text.encode_utf16().count())
        );
        for pair in ranges.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
    }

    #[test]
    fn utf16_navigation_keeps_emoji_boundaries() {
        let value = "a😄b";
        assert_eq!(next_utf16_boundary(value, 0), 1);
        assert_eq!(next_utf16_boundary(value, 1), 3);
        assert_eq!(previous_utf16_boundary(value, 3), 1);
        assert_eq!(previous_utf16_boundary(value, 4), 3);
    }

    #[test]
    fn secret_ime_proxy_redacts_content_and_preserves_utf16_boundaries() {
        let secret = "a密😄b";
        let proxy = secret_ime_proxy(secret);

        assert!(!proxy.contains('a'));
        assert!(!proxy.contains('密'));
        assert!(!proxy.contains('😄'));
        assert_eq!(proxy.encode_utf16().count(), secret.encode_utf16().count());
        for character_index in 0..=secret.chars().count() {
            assert_eq!(
                utf16_offset_for_char_index(&proxy, character_index),
                utf16_offset_for_char_index(secret, character_index)
            );
        }
    }

    #[test]
    fn classified_secret_ime_targets_project_only_masked_utf16_geometry() {
        let secret = "token密😄";
        let targets = [
            WorkspaceImeTarget::Settings(SettingsInput::AiProviderApiKey(0)),
            WorkspaceImeTarget::NewConnection(NewConnectionField::Password),
            WorkspaceImeTarget::NewConnection(NewConnectionField::UpstreamProxyPassword),
            WorkspaceImeTarget::KeyboardInteractive(0),
            WorkspaceImeTarget::HostTmuxDialogInput,
            WorkspaceImeTarget::PluginControl {
                key: 42,
                secret: true,
            },
        ];

        for target in targets {
            assert!(ime_target_is_secret(target));
            let projection = ime_text_snapshot(target, secret);
            assert!(!projection.contains(secret));
            assert_eq!(
                projection.encode_utf16().count(),
                secret.encode_utf16().count()
            );
        }
        assert_eq!(
            ime_text_snapshot(WorkspaceImeTarget::Search, secret),
            secret
        );
    }

    #[test]
    fn managed_private_key_clipboard_normalization_preserves_pem_lines() {
        let normalized = normalize_clipboard_text_for_ime_target(
            WorkspaceImeTarget::Settings(SettingsInput::ManagedKeyPastePrivateKey),
            "-----BEGIN TEST KEY-----\r\nfake-material\r-----END TEST KEY-----",
        );

        assert_eq!(
            normalized.as_str(),
            "-----BEGIN TEST KEY-----\nfake-material\n-----END TEST KEY-----"
        );
    }

    #[test]
    fn single_line_secret_clipboard_normalization_flattens_line_breaks() {
        let normalized = normalize_clipboard_text_for_ime_target(
            WorkspaceImeTarget::Settings(SettingsInput::ManagedKeyPastePassphrase),
            "fake\r\npassphrase",
        );

        assert_eq!(normalized.as_str(), "fake passphrase");
    }

    #[test]
    fn platform_commit_and_marked_text_debug_are_redacted() {
        let secret = "debug-secret";
        let mut pending = PendingPlatformTextCommit {
            target: WorkspaceImeTarget::Settings(SettingsInput::AiProviderApiKey(0)),
            text: Zeroizing::new(secret.to_string()),
            generation: 9,
            consumed: false,
        };
        let marked = WorkspaceImeMarkedText {
            target: pending.target,
            replacement_range: 0..0,
            text: Zeroizing::new(secret.to_string()),
        };

        assert!(!format!("{pending:?}").contains(secret));
        assert!(!format!("{marked:?}").contains(secret));
        assert!(format!("{pending:?}").contains("<redacted>"));
        assert!(format!("{marked:?}").contains("<redacted>"));

        pending.text.zeroize();
        assert!(pending.text.is_empty());
    }

    #[test]
    fn marked_text_replacement_and_release_clear_owned_secret() {
        let mut marked = WorkspaceImeMarkedText {
            target: WorkspaceImeTarget::KeyboardInteractive(0),
            replacement_range: 0..0,
            text: Zeroizing::new("old-composition-secret".to_string()),
        };
        let allocation = marked.text.as_ptr();

        marked.replace(2..4, "新值");

        assert_eq!(marked.replacement_range, 2..4);
        assert_eq!(marked.text.as_str(), "新值");
        assert_eq!(marked.text.as_ptr(), allocation);

        marked.text.zeroize();
        assert!(marked.text.is_empty());
    }

    #[test]
    fn word_navigation_matches_browser_style_runs() {
        let value = "alpha beta  gamma";
        assert_eq!(previous_word_boundary(value, 12), 6);
        assert_eq!(
            previous_word_boundary(value, value.encode_utf16().count()),
            12
        );
        assert_eq!(next_word_boundary(value, 0), 5);
        assert_eq!(next_word_boundary(value, 6), 10);
    }

    #[test]
    fn double_click_word_range_handles_edges() {
        assert_eq!(word_range_for_utf16_offset("root", 1), 0..4);
        assert_eq!(word_range_for_utf16_offset("alpha beta", 7), 6..10);
        assert_eq!(word_range_for_utf16_offset("alpha beta", 5), 0..5);
    }

    #[test]
    fn multiline_arrow_navigation_preserves_column() {
        let value = "abc\nde\nfghi";
        assert_eq!(vertical_line_navigation_destination(value, 2, true), 6);
        assert_eq!(vertical_line_navigation_destination(value, 6, true), 9);
        assert_eq!(vertical_line_navigation_destination(value, 9, false), 6);
    }

    #[test]
    fn visible_path_completion_owns_unmodified_vertical_navigation() {
        for target in [
            WorkspaceImeTarget::FileManager(FileManagerInput::Path),
            WorkspaceImeTarget::Sftp(SftpInput::LocalPath),
            WorkspaceImeTarget::Sftp(SftpInput::RemotePath),
        ] {
            assert!(path_completion_owns_vertical_navigation(
                target, "arrowup", true, false,
            ));
            assert!(path_completion_owns_vertical_navigation(
                target, "down", true, false,
            ));
            assert!(!path_completion_owns_vertical_navigation(
                target, "arrowup", false, false,
            ));
            assert!(!path_completion_owns_vertical_navigation(
                target, "arrowup", true, true,
            ));
        }
        assert!(!path_completion_owns_vertical_navigation(
            WorkspaceImeTarget::Search,
            "arrowdown",
            true,
            false,
        ));
        assert!(!path_completion_owns_vertical_navigation(
            WorkspaceImeTarget::Sftp(SftpInput::RemotePath),
            "left",
            true,
            false,
        ));
    }

    #[test]
    fn multiline_line_ranges_match_textarea_navigation() {
        let value = "one\ntwo\nthree";
        assert_eq!(line_range_for_utf16_offset(value, 1), 0..3);
        assert_eq!(line_range_for_utf16_offset(value, 5), 4..7);
        assert_eq!(line_start_for_utf16_offset(value, 10), 8);
        assert_eq!(line_end_for_utf16_offset(value, 10), 13);
    }

    #[test]
    fn control_k_matches_textarea_line_delete() {
        let value = "one\ntwo\nthree";
        assert_eq!(control_k_delete_end(value, 5), 7);
        assert_eq!(control_k_delete_end(value, 7), 8);
    }

    #[test]
    fn control_t_transposes_utf16_characters() {
        assert_eq!(
            transpose_text_at_utf16_offset("abcd", 2),
            Some(("acbd".to_string(), 3))
        );
        assert_eq!(
            transpose_text_at_utf16_offset("a😄b", 3),
            Some(("ab😄".to_string(), 4))
        );
        assert_eq!(
            transpose_text_at_utf16_offset("abcd", 4),
            Some(("abdc".to_string(), 4))
        );
    }

    #[test]
    fn collapsed_read_only_copy_falls_through_to_next_owner() {
        assert!(!collapsed_copy_shortcut_is_owned_by_target(
            WorkspaceImeTarget::ReadOnlyText(42)
        ));
        assert!(collapsed_copy_shortcut_is_owned_by_target(
            WorkspaceImeTarget::Search
        ));
    }

    #[test]
    fn copy_shortcut_owner_prioritizes_selection_then_focused_input_then_terminal() {
        assert_eq!(
            copy_shortcut_owner_for_target(WorkspaceImeTarget::ReadOnlyText(1), Some(&(2..5))),
            CopyShortcutOwner::SelectedRange(2..5)
        );
        assert_eq!(
            copy_shortcut_owner_for_target(WorkspaceImeTarget::Search, Some(&(3..3))),
            CopyShortcutOwner::FocusedEditableInput
        );
        assert_eq!(
            copy_shortcut_owner_for_target(WorkspaceImeTarget::ReadOnlyText(1), Some(&(4..4))),
            CopyShortcutOwner::NextOwner
        );
    }
}
