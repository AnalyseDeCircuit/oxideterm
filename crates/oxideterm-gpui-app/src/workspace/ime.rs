use std::ops::Range;

use gpui::{
    App, Bounds, Context, Element, ElementId, Entity, FocusHandle, GlobalElementId, InputHandler,
    InspectorElementId, Keystroke, LayoutId, Pixels, Style, UTF16Selection, Window, point, px,
};

use super::WorkspaceApp;
use super::forwards::ForwardInput;
use super::new_connection::NewConnectionField;
use super::session_manager::SessionManagerInput;
use super::sftp::SftpInput;
use oxideterm_gpui_settings_view::SettingsInput;
use oxideterm_gpui_ui::text_input::{TextInputAnchor, TextInputAnchorId};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum WorkspaceImeTarget {
    Search,
    TerminalCommandBar,
    Settings(SettingsInput),
    SessionManager(SessionManagerInput),
    Forwards(ForwardInput),
    Sftp(SftpInput),
    NewConnection(NewConnectionField),
    KeyboardInteractive(usize),
}

impl WorkspaceImeTarget {
    pub(super) fn anchor_id(self) -> TextInputAnchorId {
        let id = match self {
            Self::Search => 1,
            Self::TerminalCommandBar => 2,
            Self::Settings(input) => 1_000 + input.anchor_key(),
            Self::SessionManager(input) => 1_500 + input.anchor_key(),
            Self::Forwards(input) => 1_700 + input.anchor_key(),
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
                let range = match target {
                    WorkspaceImeTarget::NewConnection(field)
                        if view
                            .new_connection_form
                            .as_ref()
                            .is_some_and(|form| form.selected_field == Some(field)) =>
                    {
                        0..text_len
                    }
                    _ => text_len..text_len,
                };
                UTF16Selection {
                    range,
                    reversed: false,
                }
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
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<usize> {
        None
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
            && connection_field_accepts_ime(form.focused_field)
        {
            return Some(WorkspaceImeTarget::NewConnection(form.focused_field));
        }

        if let Some(input) = self.focused_settings_input {
            return Some(WorkspaceImeTarget::Settings(input));
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
            .is_some_and(|tab| tab.kind == oxideterm_workspace::TabKind::Sftp)
            && let Some(input) = self.sftp_view.focused_input
        {
            return Some(WorkspaceImeTarget::Sftp(input));
        }

        if self.terminal_command_bar_focused && self.active_tab().is_some_and(is_terminal_tab) {
            return Some(WorkspaceImeTarget::TerminalCommandBar);
        }

        self.search.visible.then_some(WorkspaceImeTarget::Search)
    }

    pub(super) fn marked_text_for_target(&self, target: WorkspaceImeTarget) -> Option<&str> {
        (self.active_ime_target() == Some(target))
            .then_some(self.ime_marked_text.as_deref())
            .flatten()
    }

    fn active_ime_text(&self) -> Option<String> {
        let target = self.active_ime_target()?;
        self.text_for_ime_target(target)
    }

    fn text_for_ime_target(&self, target: WorkspaceImeTarget) -> Option<String> {
        match target {
            WorkspaceImeTarget::Search => Some(self.search.query.clone()),
            WorkspaceImeTarget::TerminalCommandBar => self
                .terminal_command_bar_focused
                .then(|| self.terminal_command_bar_draft.clone()),
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
            WorkspaceImeTarget::Sftp(input) => {
                if self.sftp_view.focused_input == Some(input) {
                    Some(self.sftp_input_value(input).to_string())
                } else {
                    None
                }
            }
            WorkspaceImeTarget::NewConnection(field) => {
                let form = self.new_connection_form.as_ref()?;
                Some(match field {
                    NewConnectionField::Name => form.name.clone(),
                    NewConnectionField::Host => form.host.clone(),
                    NewConnectionField::Username => form.username.clone(),
                    NewConnectionField::Group => form.group.clone(),
                    NewConnectionField::Color => form.color.clone(),
                    NewConnectionField::JumpHost => form
                        .jump_server_form
                        .as_ref()
                        .map(|jump_form| jump_form.host.clone())?,
                    NewConnectionField::JumpUsername => form
                        .jump_server_form
                        .as_ref()
                        .map(|jump_form| jump_form.username.clone())?,
                    NewConnectionField::Port
                    | NewConnectionField::Password
                    | NewConnectionField::KeyPath
                    | NewConnectionField::CertPath
                    | NewConnectionField::Passphrase
                    | NewConnectionField::JumpPort
                    | NewConnectionField::JumpPassword
                    | NewConnectionField::JumpKeyPath
                    | NewConnectionField::JumpCertPath
                    | NewConnectionField::JumpPassphrase => return None,
                })
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
        self.ime_marked_text = None;
        self.replace_ime_target_text(target, replacement_range, text, cx);
    }

    fn replace_ime_target_text(
        &mut self,
        target: WorkspaceImeTarget,
        replacement_range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        match target {
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

fn connection_field_accepts_ime(field: NewConnectionField) -> bool {
    matches!(
        field,
        NewConnectionField::Name
            | NewConnectionField::Host
            | NewConnectionField::Username
            | NewConnectionField::Group
            | NewConnectionField::Color
            | NewConnectionField::JumpHost
            | NewConnectionField::JumpUsername
    )
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
