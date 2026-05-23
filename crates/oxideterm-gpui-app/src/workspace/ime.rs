use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, Element, ElementId, Entity, FocusHandle, GlobalElementId,
    InputHandler, InspectorElementId, Keystroke, LayoutId, Pixels, Point, SharedString, Style,
    TextRun, UTF16Selection, Window, font, point, px, rgb,
};

use super::WorkspaceApp;
use super::command_palette::parse_command_palette_mode;
use super::file_manager::FileManagerInput;
use super::forwards::ForwardInput;
use super::graphics::GraphicsInput;
use super::launcher::LauncherInput;
use super::new_connection::NewConnectionField;
use super::quick_commands::QuickCommandInput;
use super::session_manager::SessionManagerInput;
use super::settings::settings_input_accepts_newline;
use super::sftp::SftpInput;
use oxideterm_gpui_settings_view::SettingsInput;
use oxideterm_gpui_ui::{
    tauri_ui_font_family,
    text_input::{TextInputAnchor, TextInputAnchorId},
};

const READ_ONLY_TEXT_EM_WIDTH: f32 = 16.0;
const READ_ONLY_TEXT_LINE_HEIGHT_ESTIMATE: f32 = 28.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum WorkspaceImeTarget {
    ReadOnlyText(u64),
    CommandPalette,
    ShortcutsModalSearch,
    Search,
    TerminalCommandBar,
    TerminalCastSearch,
    QuickCommand(QuickCommandInput),
    Settings(SettingsInput),
    SessionManager(SessionManagerInput),
    Forwards(ForwardInput),
    FileManager(FileManagerInput),
    Launcher(LauncherInput),
    Graphics(GraphicsInput),
    AiModelSelectorSearch,
    AiChatInput,
    AiMessageEdit,
    Sftp(SftpInput),
    NewConnection(NewConnectionField),
    KeyboardInteractive(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceImeSelection {
    target: WorkspaceImeTarget,
    range: Range<usize>,
    reversed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceImeDragSelection {
    target: WorkspaceImeTarget,
    anchor: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PendingPlatformTextCommit {
    target: WorkspaceImeTarget,
    text: String,
    generation: u64,
    consumed: bool,
}

impl WorkspaceImeTarget {
    pub(super) fn anchor_id(self) -> TextInputAnchorId {
        let id = match self {
            Self::ReadOnlyText(id) => id.wrapping_add(50_000),
            Self::CommandPalette => 4,
            Self::ShortcutsModalSearch => 5,
            Self::Search => 1,
            Self::TerminalCommandBar => 2,
            Self::TerminalCastSearch => 3,
            Self::QuickCommand(input) => 500 + input.anchor_key(),
            Self::Settings(input) => 1_000 + input.anchor_key(),
            Self::SessionManager(input) => 1_500 + input.anchor_key(),
            Self::Forwards(input) => 1_700 + input.anchor_key(),
            Self::FileManager(input) => 1_800 + input.anchor_key(),
            Self::Launcher(input) => 1_850 + input.anchor_key(),
            Self::Graphics(input) => 1_875 + input.anchor_key(),
            Self::AiModelSelectorSearch => 1_895,
            Self::AiChatInput => 1_896,
            Self::AiMessageEdit => 1_897,
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
        if self.view.read(cx).active_ime_target().is_some() {
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

#[cfg(test)]
fn keystroke_commits_platform_text(keystroke: &Keystroke) -> bool {
    keystroke_platform_text(keystroke).is_some()
}

pub(super) fn active_ime_should_defer_printable_key(
    active_ime_target: bool,
    keystroke: &Keystroke,
) -> bool {
    // Browser-backed inputs receive printable text through the platform text
    // owner. Page-level key handlers must not append the same character first,
    // otherwise GPUI can commit the same key again through `InputHandler`.
    active_ime_target && keystroke_platform_text(keystroke).is_some()
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

impl InputHandler for WorkspaceInputHandler {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        self.view.update(cx, |view, _cx| {
            let target = view.active_ime_target()?;
            view.text_for_ime_target(target).map(|text| {
                let text_len = text.encode_utf16().count();
                let (range, reversed) =
                    if let Some(selection) = view.ime_selection_for_target(target) {
                        (selection.range, selection.reversed)
                    } else {
                        match target {
                            _ if view.selected_ime_target == Some(target) => (0..text_len, false),
                            WorkspaceImeTarget::NewConnection(field)
                                if view
                                    .new_connection_form
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
        self.view.update(cx, |view, _cx| {
            let text_len = view.active_ime_text()?.encode_utf16().count();
            let marked_len = view
                .ime_marked_text
                .as_deref()
                .map(str::encode_utf16)
                .map(Iterator::count)
                .unwrap_or_default();
            (marked_len > 0).then_some(text_len..text_len + marked_len)
        })
    }

    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        self.view.update(cx, |view, _cx| {
            let text = view.active_ime_text()?;
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
        _range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut App,
    ) {
        let _ = self.view.update(cx, |view, cx| {
            view.ime_marked_text = (!new_text.is_empty()).then(|| new_text.to_string());
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
        _range_utf16: Range<usize>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        self.view.update(cx, |view, _cx| {
            let target = view.active_ime_target()?;
            let bounds = view
                .text_input_anchors
                .get(&target.anchor_id())
                .map(|anchor| anchor.bounds)
                .unwrap_or(self.fallback_bounds);
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
        self.view.update(cx, |view, _cx| {
            let target = view.active_ime_target()?;
            view.ime_index_for_position(target, point, window)
        })
    }

    fn apple_press_and_hold_enabled(&mut self) -> bool {
        false
    }
}

impl WorkspaceApp {
    pub(super) fn defer_active_ime_printable_key(
        &mut self,
        keystroke: &Keystroke,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(target) = self.active_ime_target() else {
            return false;
        };
        let Some(text) = keystroke_platform_text(keystroke) else {
            return false;
        };

        let generation = self.next_platform_text_commit_generation;
        self.next_platform_text_commit_generation =
            self.next_platform_text_commit_generation.wrapping_add(1);
        self.pending_platform_text_commit = Some(PendingPlatformTextCommit {
            target,
            text: text.to_string(),
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
        cx: &mut Context<Self>,
    ) {
        if self.text_input_anchors.get(&anchor.id) != Some(&anchor) {
            let should_notify = self
                .active_ime_target()
                .is_some_and(|target| target.anchor_id() == anchor.id);
            self.text_input_anchors.insert(anchor.id, anchor);
            if should_notify {
                cx.notify();
            }
        }
    }

    pub(super) fn active_ime_target(&self) -> Option<WorkspaceImeTarget> {
        if let Some(challenge) = self.keyboard_interactive_challenge.as_ref() {
            return Some(WorkspaceImeTarget::KeyboardInteractive(
                challenge.focused_prompt,
            ));
        }

        if let Some(form) = self.new_connection_form.as_ref()
            && form.field_focused
            && self.new_connection_field_accepts_ime(form.focused_field)
        {
            return Some(WorkspaceImeTarget::NewConnection(form.focused_field));
        }

        if let Some(input) = self.focused_settings_input {
            return Some(WorkspaceImeTarget::Settings(input));
        }

        if self.command_palette.open {
            return Some(WorkspaceImeTarget::CommandPalette);
        }

        if self.shortcuts_modal.open {
            return Some(WorkspaceImeTarget::ShortcutsModalSearch);
        }

        if self.terminal_quick_commands_open
            && let Some(input) = self.quick_commands.focused_input
        {
            return Some(WorkspaceImeTarget::QuickCommand(input));
        }

        if self.auto_route_modal.open
            && self.session_manager.focused_input == Some(SessionManagerInput::AutoRouteDisplayName)
        {
            return Some(WorkspaceImeTarget::SessionManager(
                SessionManagerInput::AutoRouteDisplayName,
            ));
        }

        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == oxideterm_workspace::TabKind::SessionManager)
            && let Some(input) = self.session_manager.focused_input
        {
            return Some(WorkspaceImeTarget::SessionManager(input));
        }

        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == oxideterm_workspace::TabKind::Forwards)
            && let Some(input) = self.forwarding_view.focused_input
        {
            return Some(WorkspaceImeTarget::Forwards(input));
        }

        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == oxideterm_workspace::TabKind::FileManager)
            && let Some(input) = self.file_manager.focused_input
        {
            return Some(WorkspaceImeTarget::FileManager(input));
        }

        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == oxideterm_workspace::TabKind::Launcher)
            && let Some(input) = self.launcher.focused_input
        {
            return Some(WorkspaceImeTarget::Launcher(input));
        }

        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == oxideterm_workspace::TabKind::Graphics)
            && let Some(input) = self.graphics.focused_input
        {
            return Some(WorkspaceImeTarget::Graphics(input));
        }

        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == oxideterm_workspace::TabKind::Sftp)
            && let Some(input) = self.sftp_view.focused_input
        {
            return Some(WorkspaceImeTarget::Sftp(input));
        }

        if self.ai_sidebar_visible()
            && self.ai_model_selector_open
            && self.ai_model_selector_search_focused
        {
            return Some(WorkspaceImeTarget::AiModelSelectorSearch);
        }

        if self.ai_sidebar_visible() && self.ai_chat_input_focused {
            return Some(WorkspaceImeTarget::AiChatInput);
        }

        if self.ai_sidebar_visible()
            && self.ai_editing_message_id.is_some()
            && self.ai_editing_message_focused
        {
            return Some(WorkspaceImeTarget::AiMessageEdit);
        }

        if self.terminal_command_bar_focused && self.active_tab().is_some_and(is_terminal_tab) {
            return Some(WorkspaceImeTarget::TerminalCommandBar);
        }

        if self
            .terminal_cast_player
            .as_ref()
            .is_some_and(|player| player.search_focused)
        {
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

    pub(super) fn active_ime_target_blinks_caret(&self) -> bool {
        // Browser editable inputs keep their caret blinking regardless of which
        // page owns the field. Drive the shared native blink timer from the IME
        // owner instead of a hand-maintained list of focused booleans, otherwise
        // newly migrated inputs such as the AI sidebar can render a stale
        // invisible caret after text input.
        self.active_ime_target()
            .is_some_and(ime_target_should_blink_caret)
    }

    pub(super) fn marked_text_for_target(&self, target: WorkspaceImeTarget) -> Option<&str> {
        (self.active_ime_target() == Some(target))
            .then_some(self.ime_marked_text.as_deref())
            .flatten()
    }

    pub(super) fn ime_selected_range_for_target(
        &self,
        target: WorkspaceImeTarget,
    ) -> Option<Range<usize>> {
        self.ime_selection_range_for_target(target)
    }

    fn ime_selection_range_for_target(&self, target: WorkspaceImeTarget) -> Option<Range<usize>> {
        self.ime_selection_for_target(target)
            .map(|selection| selection.range)
            .or_else(|| {
                if self.selected_ime_target == Some(target) {
                    self.text_for_ime_target(target)
                        .map(|text| 0..text.encode_utf16().count())
                } else if self.active_ime_target() == Some(target) {
                    self.text_for_ime_target(target).map(|text| {
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

    pub(super) fn clear_ime_selection(&mut self) {
        self.selected_ime_target = None;
        self.selected_ime_range = None;
        self.ime_drag_selection = None;
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
        let Some(index) = self.ime_index_for_position(target, position, window) else {
            self.clear_ime_selection();
            cx.notify();
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
        if event.click_count <= 1 || event.modifiers.shift {
            self.begin_ime_selection(target, event.position, event.modifiers.shift, window, cx);
            return;
        }

        let Some(index) = self.ime_index_for_position(target, event.position, window) else {
            self.clear_ime_selection();
            cx.notify();
            return;
        };
        let Some(text) = self.text_for_ime_target(target) else {
            self.clear_ime_selection();
            cx.notify();
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
        let Some(index) = self.ime_index_for_position(drag.target, position, window) else {
            return;
        };
        self.set_ime_selection_from_anchor(drag.target, drag.anchor, index);
        cx.notify();
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
        let Some(text) = self.text_for_ime_target(drag.target) else {
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
        self.set_ime_selection_from_anchor(drag.target, drag.anchor, index);
        cx.notify();
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

    fn set_ime_selection_from_anchor(
        &mut self,
        target: WorkspaceImeTarget,
        anchor: usize,
        index: usize,
    ) {
        self.selected_ime_target = None;
        if anchor == index {
            self.selected_ime_range = Some(WorkspaceImeSelection {
                target,
                range: index..index,
                reversed: false,
            });
        } else if index < anchor {
            self.selected_ime_range = Some(WorkspaceImeSelection {
                target,
                range: index..anchor,
                reversed: true,
            });
        } else {
            self.selected_ime_range = Some(WorkspaceImeSelection {
                target,
                range: anchor..index,
                reversed: false,
            });
        }
    }

    fn ime_index_for_position(
        &self,
        target: WorkspaceImeTarget,
        position: Point<Pixels>,
        window: &mut Window,
    ) -> Option<usize> {
        let text = self.text_for_ime_target(target)?;
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

        let bounds = self.text_input_anchors.get(&target.anchor_id())?.bounds;
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

        let relative_x = (position.x - left).clamp(px(0.0), width);
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
        let relative_y = (position.y - bounds.top()).max(px(0.0));
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
            _ if ime_target_is_read_only(target) && line_count > 0 => {
                let inferred = f32::from(bounds.size.height) / line_count as f32;
                px(inferred.clamp(16.0, 40.0))
            }
            _ => px(self.tokens.metrics.ui_control_height),
        }
    }

    fn ime_target_horizontal_padding(target: WorkspaceImeTarget, control_padding_x: f32) -> Pixels {
        match target {
            WorkspaceImeTarget::TerminalCommandBar
            | WorkspaceImeTarget::AiChatInput
            | WorkspaceImeTarget::AiMessageEdit
            | WorkspaceImeTarget::ReadOnlyText(_) => {
                // These targets report an anchor around the painted text itself.
                // Applying the shared form-control padding again makes hit testing
                // drift right of the visible caret.
                px(0.0)
            }
            _ => px(control_padding_x),
        }
    }

    fn active_ime_text(&self) -> Option<String> {
        let target = self.active_ime_target()?;
        self.text_for_ime_target(target)
    }

    fn new_connection_field_accepts_ime(&self, field: NewConnectionField) -> bool {
        if field == NewConnectionField::Password
            && self.editing_saved_connection_id.is_some()
            && self.saved_connection_prompt_action.is_none()
            && self
                .new_connection_form
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

        if self.ime_target_is_secret(target) {
            return self.secret_ime_index_for_relative_x(text, relative_x, window);
        }

        let shaped = self.shape_ime_text(text, window);
        let byte_index = shaped.closest_index_for_x(relative_x.clamp(px(0.0), shaped.width));
        utf16_offset_for_byte_index(text, byte_index)
    }

    fn secret_ime_index_for_relative_x(
        &self,
        text: &str,
        relative_x: Pixels,
        window: &mut Window,
    ) -> usize {
        let display = "•".repeat(text.chars().count());
        if display.is_empty() {
            return 0;
        }
        let shaped = self.shape_ime_text(&display, window);
        let display_byte_index =
            shaped.closest_index_for_x(relative_x.clamp(px(0.0), shaped.width));
        let display_byte_index =
            floor_char_boundary(&display, display_byte_index.min(display.len()));
        let display_chars = display[..display_byte_index].chars().count();
        utf16_offset_for_char_index(text, display_chars)
    }

    fn shape_ime_text(&self, text: &str, window: &mut Window) -> gpui::ShapedLine {
        let font = font(tauri_ui_font_family(
            &self.settings_store.settings().appearance.ui_font_family,
        ));
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

    fn ime_target_is_secret(&self, target: WorkspaceImeTarget) -> bool {
        matches!(
            target,
            WorkspaceImeTarget::NewConnection(
                NewConnectionField::Password
                    | NewConnectionField::Passphrase
                    | NewConnectionField::JumpPassword
                    | NewConnectionField::JumpPassphrase
            ) | WorkspaceImeTarget::KeyboardInteractive(_)
        )
    }

    fn text_for_ime_target(&self, target: WorkspaceImeTarget) -> Option<String> {
        match target {
            WorkspaceImeTarget::ReadOnlyText(id) => self
                .selectable_text_values
                .get(&id)
                .cloned()
                .or_else(|| self.selectable_text_group_text(id)),
            WorkspaceImeTarget::CommandPalette => Some(self.command_palette.raw_query.clone()),
            WorkspaceImeTarget::ShortcutsModalSearch => Some(self.shortcuts_modal.query.clone()),
            WorkspaceImeTarget::Search => Some(self.search.query.clone()),
            WorkspaceImeTarget::TerminalCommandBar => self
                .terminal_command_bar_focused
                .then(|| self.terminal_command_bar_draft.clone()),
            WorkspaceImeTarget::TerminalCastSearch => self
                .terminal_cast_player
                .as_ref()
                .filter(|player| player.search_focused)
                .map(|player| player.search_query.clone()),
            WorkspaceImeTarget::QuickCommand(input) => self.quick_command_input_value(input),
            WorkspaceImeTarget::Settings(input) => {
                if self.focused_settings_input == Some(input) {
                    Some(self.settings_input_draft.clone())
                } else {
                    None
                }
            }
            WorkspaceImeTarget::SessionManager(input) => {
                if self.session_manager.focused_input == Some(input) {
                    Some(match input {
                        SessionManagerInput::Search => self.session_manager.search_query.clone(),
                        SessionManagerInput::SavedSearch => {
                            self.session_manager.saved_search_query.clone()
                        }
                        SessionManagerInput::NewGroup => {
                            self.session_manager.new_group_name.clone()
                        }
                        SessionManagerInput::AutoRouteDisplayName => {
                            self.auto_route_modal.display_name.clone()
                        }
                        SessionManagerInput::OxideImportPassword => self
                            .session_manager
                            .oxide_import_dialog
                            .as_ref()
                            .map(|dialog| dialog.password.clone())?,
                        SessionManagerInput::OxideExportPassword => self
                            .session_manager
                            .oxide_export_dialog
                            .as_ref()
                            .map(|dialog| dialog.password.clone())?,
                        SessionManagerInput::OxideExportConfirmPassword => self
                            .session_manager
                            .oxide_export_dialog
                            .as_ref()
                            .map(|dialog| dialog.confirm_password.clone())?,
                        SessionManagerInput::OxideExportDescription => self
                            .session_manager
                            .oxide_export_dialog
                            .as_ref()
                            .map(|dialog| dialog.description.clone())?,
                    })
                } else {
                    None
                }
            }
            WorkspaceImeTarget::Forwards(input) => {
                if self.forwarding_view.focused_input == Some(input) {
                    Some(self.forward_input_value(input).to_string())
                } else {
                    None
                }
            }
            WorkspaceImeTarget::FileManager(input) => {
                if self.file_manager.focused_input == Some(input) {
                    Some(self.file_manager_input_value(input).to_string())
                } else {
                    None
                }
            }
            WorkspaceImeTarget::Launcher(input) => {
                if self.launcher.focused_input == Some(input) {
                    Some(self.launcher_input_value(input).to_string())
                } else {
                    None
                }
            }
            WorkspaceImeTarget::Graphics(input) => {
                if self.graphics.focused_input == Some(input) {
                    Some(self.graphics_input_value(input).to_string())
                } else {
                    None
                }
            }
            WorkspaceImeTarget::AiModelSelectorSearch => self
                .ai_model_selector_search_focused
                .then(|| self.ai_model_selector_search_query.clone()),
            WorkspaceImeTarget::AiChatInput => self
                .ai_chat_input_focused
                .then(|| self.ai_chat_draft.clone()),
            WorkspaceImeTarget::AiMessageEdit => self
                .ai_editing_message_focused
                .then(|| self.ai_editing_message_draft.clone()),
            WorkspaceImeTarget::Sftp(input) => {
                if self.sftp_view.focused_input == Some(input) {
                    Some(self.sftp_input_value(input).to_string())
                } else {
                    None
                }
            }
            WorkspaceImeTarget::NewConnection(field) => {
                let form = self.new_connection_form.as_ref()?;
                new_connection_field_value(form, field).map(str::to_string)
            }
            WorkspaceImeTarget::KeyboardInteractive(index) => self
                .keyboard_interactive_challenge
                .as_ref()
                .and_then(|challenge| challenge.responses.get(index).cloned()),
        }
    }

    fn replace_active_ime_text(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.active_ime_target() else {
            return;
        };
        if platform_text_commit_is_duplicate(&mut self.pending_platform_text_commit, target, text) {
            self.ime_marked_text = None;
            return;
        }
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
        if !keystroke.modifiers.platform {
            return false;
        }
        match keystroke.key.as_str() {
            "a" => self.select_all_active_text_input(cx),
            "c" => self.copy_active_text_input(cx),
            "x" | "v"
                if self
                    .active_ime_target()
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
        let Some(target) = self.active_ime_target() else {
            return false;
        };
        let Some(text) = self.text_for_ime_target(target) else {
            return false;
        };
        let range = if let Some(range) = self
            .ime_selected_range_for_target(target)
            .filter(|range| range.start < range.end)
        {
            range
        } else if let Some(caret) = self.ime_selection_range_for_target(target) {
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
        let Some(target) = self.active_ime_target() else {
            return false;
        };
        if target == WorkspaceImeTarget::TerminalCommandBar
            && self.terminal_command_bar_should_accept_inline_suggestion(keystroke, cx)
        {
            return false;
        }
        if target == WorkspaceImeTarget::TerminalCommandBar
            && matches!(
                keystroke.key.as_str(),
                "up" | "arrowup" | "down" | "arrowdown"
            )
        {
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
            && !self.ai_chat_autocomplete_items().is_empty()
        {
            return false;
        }
        let Some(text) = self.text_for_ime_target(target) else {
            return false;
        };
        let text_len = text.encode_utf16().count();
        let Some(selection) = self.ime_selection_for_navigation(target, text_len) else {
            return false;
        };
        let Some(next) =
            self.text_input_navigation_destination(target, &text, &selection, keystroke)
        else {
            return false;
        };

        if keystroke.modifiers.shift {
            let anchor = selection_anchor(&selection);
            self.set_ime_selection_from_anchor(target, anchor, next);
        } else {
            self.set_ime_selection_from_anchor(target, next, next);
        }
        self.ime_marked_text = None;
        self.ime_drag_selection = None;
        self.new_connection_caret_visible = true;
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
        let Some(target) = self.active_ime_target() else {
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
        let Some(replacement_range) = self.ime_selection_range_for_target(target) else {
            return false;
        };
        let caret = replacement_range.start + 1;
        self.clear_ime_selection();
        self.replace_ime_target_text(target, Some(replacement_range), "\n", cx);
        self.set_ime_selection_from_anchor(target, caret, caret);
        self.ime_marked_text = None;
        self.new_connection_caret_visible = true;
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
        let Some(target) = self.active_ime_target() else {
            return false;
        };
        if ime_target_is_read_only(target) {
            return false;
        }
        let Some(text) = self.text_for_ime_target(target) else {
            return false;
        };
        let Some(selection) = self.ime_selection_range_for_target(target) else {
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
        self.new_connection_caret_visible = true;
        cx.notify();
        true
    }

    pub(super) fn copy_active_text_input(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(target) = self.active_ime_target() else {
            return false;
        };
        let Some(text) = self.text_for_ime_target(target) else {
            return false;
        };
        let selection = self.ime_selected_range_for_target(target);
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
        let Some(target) = self.active_ime_target() else {
            return false;
        };
        let Some(text) = self.text_for_ime_target(target) else {
            return false;
        };
        let Some(range) = self
            .ime_selected_range_for_target(target)
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
        let Some(target) = self.active_ime_target() else {
            return false;
        };
        if ime_target_is_read_only(target) {
            return true;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return true;
        };
        let text = normalize_clipboard_text_for_ime_target(target, &text);
        let replacement_range = self.ime_selection_range_for_target(target);
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
        let Some(target) = self.active_ime_target() else {
            return false;
        };
        if self.text_for_ime_target(target).is_none() {
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
                (self.active_ime_target() == Some(target)).then_some(WorkspaceImeSelection {
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

    fn terminal_command_bar_should_accept_inline_suggestion(
        &self,
        keystroke: &Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        matches!(keystroke.key.as_str(), "right" | "arrowright")
            && !keystroke.modifiers.shift
            && !keystroke.modifiers.platform
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.control
            && self
                .terminal_command_bar_visible_suggestions(cx)
                .iter()
                .any(|candidate| candidate.inline_safe)
    }

    fn replace_ime_target_text(
        &mut self,
        target: WorkspaceImeTarget,
        replacement_range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        match target {
            WorkspaceImeTarget::ReadOnlyText(_) => {}
            WorkspaceImeTarget::CommandPalette => {
                replace_utf16(&mut self.command_palette.raw_query, replacement_range, text);
                let (mode, _) = parse_command_palette_mode(&self.command_palette.raw_query);
                self.command_palette.mode = mode;
                self.command_palette.selected_index = 0;
                self.new_connection_caret_visible = true;
                cx.notify();
            }
            WorkspaceImeTarget::ShortcutsModalSearch => {
                replace_utf16(&mut self.shortcuts_modal.query, replacement_range, text);
                self.shortcuts_modal.scroll_handle = gpui::UniformListScrollHandle::new();
                self.new_connection_caret_visible = true;
                cx.notify();
            }
            WorkspaceImeTarget::Search => {
                replace_utf16(&mut self.search.query, replacement_range, text);
                self.update_search_query(cx);
            }
            WorkspaceImeTarget::TerminalCommandBar => {
                if self.terminal_command_bar_focused {
                    replace_utf16(
                        &mut self.terminal_command_bar_draft,
                        replacement_range,
                        text,
                    );
                    self.terminal_command_suggestions_open = false;
                    self.terminal_command_suggestion_highlighted = None;
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
            }
            WorkspaceImeTarget::TerminalCastSearch => {
                if let Some(player) = self.terminal_cast_player.as_mut()
                    && player.search_focused
                {
                    replace_utf16(&mut player.search_query, replacement_range, text);
                    self.update_terminal_cast_search(cx);
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
            }
            WorkspaceImeTarget::QuickCommand(input) => {
                if self.quick_commands.focused_input == Some(input) {
                    replace_utf16(
                        self.quick_command_input_value_mut(input),
                        replacement_range,
                        text,
                    );
                    if input == QuickCommandInput::Search {
                        // Browser filtering invalidates the active option until
                        // ArrowUp/ArrowDown or hover establishes a fresh row.
                        self.quick_commands.highlighted_command = None;
                    }
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
            }
            WorkspaceImeTarget::Settings(input) => {
                if self.focused_settings_input == Some(input) {
                    replace_utf16(&mut self.settings_input_draft, replacement_range, text);
                    self.apply_settings_input_draft(input, cx);
                }
            }
            WorkspaceImeTarget::SessionManager(input) => {
                if self.session_manager.focused_input == Some(input) {
                    match input {
                        SessionManagerInput::Search => {
                            replace_utf16(
                                &mut self.session_manager.search_query,
                                replacement_range,
                                text,
                            );
                            self.clear_session_selection_for_invisible_rows();
                        }
                        SessionManagerInput::SavedSearch => {
                            replace_utf16(
                                &mut self.session_manager.saved_search_query,
                                replacement_range,
                                text,
                            );
                        }
                        SessionManagerInput::NewGroup => {
                            replace_utf16(
                                &mut self.session_manager.new_group_name,
                                replacement_range,
                                text,
                            );
                        }
                        SessionManagerInput::AutoRouteDisplayName => {
                            replace_utf16(
                                &mut self.auto_route_modal.display_name,
                                replacement_range,
                                text,
                            );
                        }
                        SessionManagerInput::OxideImportPassword => {
                            if let Some(dialog) = self.session_manager.oxide_import_dialog.as_mut()
                            {
                                replace_utf16(&mut dialog.password, replacement_range, text);
                                dialog.error = None;
                            }
                        }
                        SessionManagerInput::OxideExportPassword => {
                            if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut()
                            {
                                replace_utf16(&mut dialog.password, replacement_range, text);
                                dialog.error = None;
                            }
                        }
                        SessionManagerInput::OxideExportConfirmPassword => {
                            if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut()
                            {
                                replace_utf16(
                                    &mut dialog.confirm_password,
                                    replacement_range,
                                    text,
                                );
                                dialog.error = None;
                            }
                        }
                        SessionManagerInput::OxideExportDescription => {
                            if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut()
                            {
                                replace_utf16(&mut dialog.description, replacement_range, text);
                                dialog.error = None;
                            }
                        }
                    }
                    cx.notify();
                }
            }
            WorkspaceImeTarget::Forwards(input) => {
                if self.forwarding_view.focused_input == Some(input) {
                    replace_utf16(self.forward_input_value_mut(input), replacement_range, text);
                    self.forwarding_view.error = None;
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
            }
            WorkspaceImeTarget::FileManager(input) => {
                if self.file_manager.focused_input == Some(input) {
                    replace_utf16(
                        self.file_manager_input_value_mut(input),
                        replacement_range,
                        text,
                    );
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
            }
            WorkspaceImeTarget::Launcher(input) => {
                if self.launcher.focused_input == Some(input) {
                    replace_utf16(
                        self.launcher_input_value_mut(input),
                        replacement_range,
                        text,
                    );
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
            }
            WorkspaceImeTarget::Graphics(input) => {
                if self.graphics.focused_input == Some(input) {
                    replace_utf16(
                        self.graphics_input_value_mut(input),
                        replacement_range,
                        text,
                    );
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
            }
            WorkspaceImeTarget::AiModelSelectorSearch => {
                if self.ai_model_selector_search_focused {
                    replace_utf16(
                        &mut self.ai_model_selector_search_query,
                        replacement_range,
                        text,
                    );
                    // Search changes rebuild the visible model rows; clear the
                    // Radix-style active item so keyboard focus cannot point at
                    // a filtered-out model.
                    self.ai_model_selector_highlighted_model = None;
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
            }
            WorkspaceImeTarget::AiChatInput => {
                if self.ai_chat_input_focused {
                    replace_utf16(&mut self.ai_chat_draft, replacement_range, text);
                    self.ai_chat_autocomplete_suppressed = false;
                    self.ai_chat_autocomplete_index = 0;
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
            }
            WorkspaceImeTarget::AiMessageEdit => {
                if self.ai_editing_message_focused {
                    replace_utf16(&mut self.ai_editing_message_draft, replacement_range, text);
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
            }
            WorkspaceImeTarget::Sftp(input) => {
                if self.sftp_view.focused_input == Some(input) {
                    replace_utf16(self.sftp_input_value_mut(input), replacement_range, text);
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
            }
            WorkspaceImeTarget::NewConnection(field) => {
                if let Some(form) = self.new_connection_form.as_mut() {
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
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
            }
            WorkspaceImeTarget::KeyboardInteractive(index) => {
                if let Some(challenge) = self.keyboard_interactive_challenge.as_mut()
                    && let Some(response) = challenge.responses.get_mut(index)
                {
                    replace_utf16(response, replacement_range, text);
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
            }
        }
    }
}

fn is_terminal_tab(tab: &oxideterm_workspace::Tab) -> bool {
    matches!(
        tab.kind,
        oxideterm_workspace::TabKind::LocalTerminal | oxideterm_workspace::TabKind::SshTerminal
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
        NewConnectionField::CertPath => &form.cert_path,
        NewConnectionField::Passphrase => &form.passphrase,
        NewConnectionField::Group => &form.group,
        NewConnectionField::PostConnectCommand => &form.post_connect_command,
        NewConnectionField::Color => &form.color,
        NewConnectionField::JumpHost => &form.jump_server_form.as_ref()?.host,
        NewConnectionField::JumpPort => &form.jump_server_form.as_ref()?.port,
        NewConnectionField::JumpUsername => &form.jump_server_form.as_ref()?.username,
        NewConnectionField::JumpPassword => &form.jump_server_form.as_ref()?.password,
        NewConnectionField::JumpKeyPath => &form.jump_server_form.as_ref()?.key_path,
        NewConnectionField::JumpCertPath => &form.jump_server_form.as_ref()?.cert_path,
        NewConnectionField::JumpPassphrase => &form.jump_server_form.as_ref()?.passphrase,
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
        NewConnectionField::CertPath => &mut form.cert_path,
        NewConnectionField::Passphrase => &mut form.passphrase,
        NewConnectionField::Group => &mut form.group,
        NewConnectionField::PostConnectCommand => &mut form.post_connect_command,
        NewConnectionField::Color => &mut form.color,
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
    }
}

fn replace_utf16(value: &mut String, range: Option<Range<usize>>, replacement: &str) {
    let range = range.unwrap_or_else(|| {
        let end = value.encode_utf16().count();
        end..end
    });
    let start = byte_index_for_utf16(value, range.start);
    let end = byte_index_for_utf16(value, range.end);
    value.replace_range(start..end, replacement);
}

fn normalize_clipboard_text_for_ime_target(target: WorkspaceImeTarget, text: &str) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    if ime_target_accepts_newline(target) {
        normalized
    } else {
        normalized.lines().collect::<Vec<_>>().join(" ")
    }
}

fn ime_target_accepts_newline(target: WorkspaceImeTarget) -> bool {
    match target {
        WorkspaceImeTarget::ReadOnlyText(_) => true,
        WorkspaceImeTarget::Settings(input) => settings_input_accepts_newline(input),
        WorkspaceImeTarget::AiChatInput | WorkspaceImeTarget::AiMessageEdit => true,
        WorkspaceImeTarget::SessionManager(SessionManagerInput::OxideExportDescription) => true,
        _ => false,
    }
}

fn ime_target_is_read_only(target: WorkspaceImeTarget) -> bool {
    matches!(target, WorkspaceImeTarget::ReadOnlyText(_))
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

fn utf16_slice(value: &str, range: Range<usize>) -> String {
    let start = byte_index_for_utf16(value, range.start);
    let end = byte_index_for_utf16(value, range.end);
    value[start..end].to_string()
}

fn byte_index_for_utf16(value: &str, offset: usize) -> usize {
    let mut utf16_count = 0;
    for (byte_index, ch) in value.char_indices() {
        if utf16_count >= offset {
            return byte_index;
        }
        utf16_count += ch.len_utf16();
    }
    value.len()
}

fn utf16_offset_for_byte_index(value: &str, byte_offset: usize) -> usize {
    let byte_offset = floor_char_boundary(value, byte_offset.min(value.len()));
    value[..byte_offset].encode_utf16().count()
}

fn utf16_offset_for_char_index(value: &str, char_offset: usize) -> usize {
    value.chars().take(char_offset).map(char::len_utf16).sum()
}

fn char_index_for_utf16(value: &str, offset: usize) -> usize {
    let mut utf16_count = 0;
    for (char_index, ch) in value.chars().enumerate() {
        if utf16_count >= offset {
            return char_index;
        }
        utf16_count += ch.len_utf16();
    }
    value.chars().count()
}

fn floor_char_boundary(value: &str, mut byte_offset: usize) -> usize {
    while byte_offset > 0 && !value.is_char_boundary(byte_offset) {
        byte_offset -= 1;
    }
    byte_offset
}

fn previous_utf16_boundary(value: &str, offset: usize) -> usize {
    let mut previous = 0;
    let mut utf16_count = 0;
    for ch in value.chars() {
        if utf16_count >= offset {
            break;
        }
        previous = utf16_count;
        utf16_count += ch.len_utf16();
    }
    previous
}

fn next_utf16_boundary(value: &str, offset: usize) -> usize {
    let mut utf16_count = 0;
    for ch in value.chars() {
        let next = utf16_count + ch.len_utf16();
        if utf16_count >= offset {
            return next;
        }
        utf16_count = next;
    }
    value.encode_utf16().count()
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

fn previous_word_boundary(value: &str, offset: usize) -> usize {
    let current = byte_index_for_utf16(value, offset);
    let prefix = &value[..current];
    let mut saw_word = false;
    for (byte_index, ch) in prefix.char_indices().rev() {
        if ch.is_whitespace() {
            if saw_word {
                return prefix[..byte_index + ch.len_utf8()].encode_utf16().count();
            }
        } else {
            saw_word = true;
        }
    }
    0
}

fn next_word_boundary(value: &str, offset: usize) -> usize {
    let current = byte_index_for_utf16(value, offset);
    let suffix = &value[current..];
    let mut saw_word = false;
    for (relative_byte, ch) in suffix.char_indices() {
        if ch.is_whitespace() {
            if saw_word {
                return value[..current + relative_byte].encode_utf16().count();
            }
        } else {
            saw_word = true;
        }
    }
    value.encode_utf16().count()
}

fn word_range_for_utf16_offset(value: &str, offset: usize) -> Range<usize> {
    let text_len = value.encode_utf16().count();
    if text_len == 0 {
        return 0..0;
    }
    let mut byte_index = byte_index_for_utf16(value, offset.min(text_len));
    if byte_index == value.len() && byte_index > 0 {
        byte_index = previous_char_start(value, byte_index);
    }
    if value[byte_index..]
        .chars()
        .next()
        .is_some_and(char::is_whitespace)
        && byte_index > 0
    {
        let previous = previous_char_start(value, byte_index);
        if !value[previous..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        {
            byte_index = previous;
        }
    }

    let mut start = byte_index;
    while start > 0 {
        let previous = previous_char_start(value, start);
        let Some(ch) = value[previous..].chars().next() else {
            break;
        };
        if ch.is_whitespace() {
            break;
        }
        start = previous;
    }

    let mut end = byte_index;
    while end < value.len() {
        let Some(ch) = value[end..].chars().next() else {
            break;
        };
        if ch.is_whitespace() {
            break;
        }
        end += ch.len_utf8();
    }

    utf16_offset_for_byte_index(value, start)..utf16_offset_for_byte_index(value, end)
}

fn line_range_for_utf16_offset(value: &str, offset: usize) -> Range<usize> {
    let ranges = line_ranges_utf16(value);
    let text_len = value.encode_utf16().count();
    ranges
        .iter()
        .find(|range| offset <= range.end)
        .cloned()
        .unwrap_or(text_len..text_len)
}

fn line_start_for_utf16_offset(value: &str, offset: usize) -> usize {
    line_range_for_utf16_offset(value, offset).start
}

fn line_end_for_utf16_offset(value: &str, offset: usize) -> usize {
    line_range_for_utf16_offset(value, offset).end
}

fn control_k_delete_end(value: &str, offset: usize) -> usize {
    let line_end = line_end_for_utf16_offset(value, offset);
    if line_end > offset {
        return line_end;
    }
    next_utf16_boundary(value, offset)
}

fn transpose_text_at_utf16_offset(value: &str, offset: usize) -> Option<(String, usize)> {
    let mut chars: Vec<char> = value.chars().collect();
    if chars.len() < 2 {
        return None;
    }
    let text_len = value.encode_utf16().count();
    let right = if offset >= text_len {
        chars.len() - 1
    } else {
        char_index_for_utf16(value, offset).min(chars.len() - 1)
    };
    if right == 0 {
        return None;
    }
    let left = right - 1;
    chars.swap(left, right);
    let next_caret = if offset >= text_len {
        text_len
    } else {
        utf16_offset_for_char_index(&chars.iter().collect::<String>(), right + 1)
    };
    Some((chars.into_iter().collect(), next_caret))
}

fn vertical_line_navigation_destination(value: &str, offset: usize, down: bool) -> usize {
    let ranges = line_ranges_utf16(value);
    if ranges.is_empty() {
        return 0;
    }
    let line_index = ranges
        .iter()
        .position(|range| offset <= range.end)
        .unwrap_or_else(|| ranges.len().saturating_sub(1));
    let current = &ranges[line_index];
    let column = offset.saturating_sub(current.start);
    if down {
        let Some(next) = ranges.get(line_index + 1) else {
            return value.encode_utf16().count();
        };
        next.start + column.min(next.end.saturating_sub(next.start))
    } else {
        if line_index == 0 {
            return 0;
        }
        let previous = &ranges[line_index - 1];
        previous.start + column.min(previous.end.saturating_sub(previous.start))
    }
}

fn line_ranges_utf16(value: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut offset = 0;
    for ch in value.chars() {
        if ch == '\n' {
            ranges.push(start..offset);
            offset += ch.len_utf16();
            start = offset;
        } else {
            offset += ch.len_utf16();
        }
    }
    ranges.push(start..offset);
    ranges
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
    if pending.target != target || pending.text != text {
        return false;
    }
    if pending.consumed {
        *pending_commit = None;
        return true;
    }
    pending.consumed = true;
    false
}

fn previous_char_start(value: &str, byte_index: usize) -> usize {
    value[..byte_index]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use gpui::{Keystroke, Modifiers, px};

    use super::{
        CopyShortcutOwner, PendingPlatformTextCommit, WorkspaceApp, WorkspaceImeTarget,
        active_ime_should_defer_printable_key, collapsed_copy_shortcut_is_owned_by_target,
        control_k_delete_end, copy_shortcut_owner_for_target, ime_target_should_blink_caret,
        keystroke_commits_platform_text, line_end_for_utf16_offset, line_range_for_utf16_offset,
        line_start_for_utf16_offset, next_utf16_boundary, next_word_boundary,
        platform_text_commit_is_duplicate, previous_utf16_boundary, previous_word_boundary,
        soft_wrapped_line_ranges_utf16, transpose_text_at_utf16_offset,
        vertical_line_navigation_destination, word_range_for_utf16_offset,
    };

    fn key(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            key: key.to_string(),
            key_char: key_char.map(str::to_string),
            modifiers,
        }
    }

    #[test]
    fn printable_keystrokes_are_platform_text_input() {
        assert!(keystroke_commits_platform_text(&key(
            "a",
            Some("a"),
            Modifiers::default()
        )));
        assert!(keystroke_commits_platform_text(&key(
            "space",
            Some(" "),
            Modifiers::default()
        )));
        assert!(keystroke_commits_platform_text(&key(
            "s",
            Some("ß"),
            Modifiers {
                alt: true,
                ..Modifiers::default()
            }
        )));
    }

    #[test]
    fn shortcuts_and_control_keys_stay_on_manual_key_path() {
        assert!(!keystroke_commits_platform_text(&key(
            "backspace",
            None,
            Modifiers::default()
        )));
        assert!(!keystroke_commits_platform_text(&key(
            "v",
            None,
            Modifiers {
                platform: true,
                ..Modifiers::default()
            }
        )));
        assert!(!keystroke_commits_platform_text(&key(
            "a",
            Some("\u{1}"),
            Modifiers {
                control: true,
                ..Modifiers::default()
            }
        )));
    }

    #[test]
    fn active_ime_defers_printable_keys_to_platform_text_owner() {
        let printable = key("a", Some("a"), Modifiers::default());
        let shortcut = key(
            "a",
            Some("a"),
            Modifiers {
                platform: true,
                ..Modifiers::default()
            },
        );

        assert!(active_ime_should_defer_printable_key(true, &printable));
        assert!(!active_ime_should_defer_printable_key(false, &printable));
        assert!(!active_ime_should_defer_printable_key(true, &shortcut));
    }

    #[test]
    fn editable_ime_targets_drive_the_shared_caret_blink_timer() {
        assert!(ime_target_should_blink_caret(
            WorkspaceImeTarget::AiChatInput
        ));
        assert!(ime_target_should_blink_caret(
            WorkspaceImeTarget::AiModelSelectorSearch
        ));
        assert!(!ime_target_should_blink_caret(
            WorkspaceImeTarget::ReadOnlyText(1)
        ));
    }

    #[test]
    fn self_padded_text_targets_do_not_shift_hit_testing() {
        assert_eq!(
            WorkspaceApp::ime_target_horizontal_padding(
                WorkspaceImeTarget::TerminalCommandBar,
                12.0,
            ),
            px(0.0)
        );
        assert_eq!(
            WorkspaceApp::ime_target_horizontal_padding(WorkspaceImeTarget::AiChatInput, 12.0),
            px(0.0)
        );
        assert_eq!(
            WorkspaceApp::ime_target_horizontal_padding(WorkspaceImeTarget::CommandPalette, 12.0),
            px(12.0)
        );
    }

    #[test]
    fn platform_text_commit_dedupes_only_same_deferred_key() {
        let mut pending = Some(PendingPlatformTextCommit {
            target: WorkspaceImeTarget::CommandPalette,
            text: "a".to_string(),
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
            text: "a".to_string(),
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
            text: "a".to_string(),
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
