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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum WorkspaceImeTarget {
    CommandPalette,
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

impl WorkspaceImeTarget {
    pub(super) fn anchor_id(self) -> TextInputAnchorId {
        let id = match self {
            Self::CommandPalette => 4,
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

pub(super) fn keystroke_commits_platform_text(keystroke: &Keystroke) -> bool {
    if keystroke.modifiers.platform || keystroke.modifiers.control {
        return false;
    }

    keystroke
        .key_char
        .as_deref()
        .is_some_and(|text| !text.is_empty() && !text.chars().any(char::is_control))
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

        self.search.visible.then_some(WorkspaceImeTarget::Search)
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

    pub(super) fn finish_ime_selection_drag(&mut self) {
        self.ime_drag_selection = None;
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

        let bounds = self.text_input_anchors.get(&target.anchor_id())?.bounds;
        let padding = px(self.tokens.metrics.ui_control_padding_x);
        let left = bounds.left() + padding;
        let right = bounds.right() - padding;
        let width = right - left;
        if width <= px(1.0) || position.x <= left {
            return Some(0);
        }
        if position.x >= right {
            return Some(text_len);
        }

        let relative_x = (position.x - left).clamp(px(0.0), width);
        Some(self.ime_index_for_relative_x(target, &text, relative_x, window))
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
            WorkspaceImeTarget::CommandPalette => Some(self.command_palette.raw_query.clone()),
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
        if keystroke.modifiers.platform || keystroke.modifiers.control {
            return false;
        }
        if !matches!(keystroke.key.as_str(), "backspace" | "delete") {
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
            match keystroke.key.as_str() {
                "backspace" if caret > 0 => previous_utf16_boundary(&text, caret)..caret,
                "delete" if caret < text.encode_utf16().count() => {
                    caret..next_utf16_boundary(&text, caret)
                }
                _ => return false,
            }
        } else {
            return false;
        };
        let caret = range.start;
        self.clear_ime_selection();
        self.replace_ime_target_text(target, Some(range), "", cx);
        self.set_ime_selection_from_anchor(target, caret, caret);
        true
    }

    pub(super) fn copy_active_text_input(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(target) = self.active_ime_target() else {
            return false;
        };
        let Some(text) = self.text_for_ime_target(target) else {
            return false;
        };
        let Some(selection) = self
            .ime_selected_range_for_target(target)
            .filter(|range| range.start < range.end)
        else {
            return true;
        };
        let copied = utf16_slice(&text, selection);
        cx.write_to_clipboard(ClipboardItem::new_string(copied));
        true
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

    fn replace_ime_target_text(
        &mut self,
        target: WorkspaceImeTarget,
        replacement_range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        match target {
            WorkspaceImeTarget::CommandPalette => {
                replace_utf16(&mut self.command_palette.raw_query, replacement_range, text);
                let (mode, _) = parse_command_palette_mode(&self.command_palette.raw_query);
                self.command_palette.mode = mode;
                self.command_palette.selected_index = 0;
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
        WorkspaceImeTarget::Settings(input) => settings_input_accepts_newline(input),
        WorkspaceImeTarget::AiChatInput | WorkspaceImeTarget::AiMessageEdit => true,
        WorkspaceImeTarget::SessionManager(SessionManagerInput::OxideExportDescription) => true,
        _ => false,
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

#[cfg(test)]
mod tests {
    use gpui::{Keystroke, Modifiers};

    use super::keystroke_commits_platform_text;

    fn key(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            key: key.to_string(),
            key_char: key_char.map(str::to_string),
            modifiers,
        }
    }

    #[test]
    fn printable_keystrokes_are_deferred_to_platform_text_input() {
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
}
