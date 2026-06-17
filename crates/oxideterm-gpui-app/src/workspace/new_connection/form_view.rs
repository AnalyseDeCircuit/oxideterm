use gpui::{
    AnchoredPositionMode, AnyElement, ClipboardItem, Context, Corner, KeyDownEvent, MouseButton,
    MouseMoveEvent, ParentElement, PathPromptOptions, SharedString, Styled, Window, anchored,
    deferred, div, point, prelude::*, px, rgb, rgba,
};

use super::{
    form_state::{
        NewConnectionField, NewConnectionForm, NewConnectionSelect, NewConnectionSubmitAction,
        NewConnectionTransport, NewConnectionUpstreamProxyAuth, NewConnectionUpstreamProxyPolicy,
        SSH_DEFAULT_PORT_TEXT, SavedConnectionPromptAction, SshAuthTab, TELNET_DEFAULT_PORT_TEXT,
        apply_transport_default_port, backspace_current_connection_field,
        clear_connection_selection, clear_current_connection_field, connection_field_is_selected,
        current_connection_field, insert_text_into_current_connection_field,
        new_connection_form_mode, next_connection_field, next_jump_connection_field,
        select_current_connection_field, text_from_keystroke,
    },
    ssh_flow::SshConnectionIntent,
};
use crate::assets::LucideIcon;
use crate::workspace::SelectableTextScrollExt;
use crate::workspace::WorkspaceApp;
use crate::workspace::{browser_behavior, ime::WorkspaceImeTarget};
use gpui::Div;
use oxideterm_connections::SavedUpstreamProxyProtocol;
use oxideterm_gpui_ui::{
    ButtonTone, TextInputView, button,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions,
    },
    checkbox, form_field,
    modal::{dismissible_dialog_backdrop, popover_backdrop},
    modal_body, modal_container, modal_footer, modal_header,
    radio_group::{radio_group, radio_group_item},
    segmented_tab, segmented_tabs,
    select::{
        SelectAnchorId, select_anchor_probe, select_option, select_option_action,
        select_overlay_popup_with_max_height, select_trigger_with_focus_visible,
    },
    text_input, text_input_anchor_probe,
};

const TAURI_EDIT_MODAL_WIDTH: f32 = 500.0; // Tauri sm:max-w-[500px]
const TAURI_EDIT_COLOR_FALLBACK: u32 = 0x22d3ee;
const TAURI_EDIT_COLOR_FALLBACK_TEXT: &str = "#22d3ee";
const TAURI_PROMPT_ERROR_ALPHA: u32 = 0x1a; // Tailwind red-500/10
const TAURI_PROMPT_ERROR_BORDER_ALPHA: u32 = 0x80; // Tailwind red-500/50
const TAURI_PASSWORD_ICON_BUTTON_SIZE: f32 = 28.0; // Tauri h-7 w-7
const TAURI_PASSWORD_ICON_BUTTON_OFFSET: f32 = 4.0; // Tauri right-1 top-1
const TAURI_PASSWORD_ICON_SIZE: f32 = 16.0; // Tauri h-4 w-4
const TAURI_JUMP_MODAL_WIDTH: f32 = 425.0; // Tauri sm:max-w-[425px]
const TAURI_DRILL_DOWN_MODAL_WIDTH: f32 = 480.0; // Tauri DrillDownDialog sm:max-w-[480px]
const TAURI_PROXY_CHAIN_MAX_HEIGHT: f32 = 250.0; // Tauri max-h-[250px]
const TAURI_PROXY_CHAIN_SECTION_PADDING: f32 = 16.0; // Tauri p-4
const TAURI_PROXY_CHAIN_HEADER_MARGIN: f32 = 16.0; // Tauri mb-4
const TAURI_PROXY_CHAIN_NODE_SIZE: f32 = 32.0; // Tauri w-8 h-8
const TAURI_PROXY_CHAIN_LINE_WIDTH: f32 = 32.0; // Tauri w-8
const TAURI_PROXY_CHAIN_CONNECTOR_THICKNESS: f32 = 2.0; // Tauri w-0.5 h-0.5
const TAURI_PROXY_CHAIN_CARD_PADDING: f32 = 12.0; // Tauri p-3
const TAURI_SERIAL_GRID_GAP: f32 = 16.0; // Tauri serial grid gap-4
const TAURI_SERIAL_PANEL_BG_ALPHA: u32 = 0x66; // Tauri serial bg-theme-bg/40

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConnectionButtonAction {
    Cancel,
    Test,
    Connect,
    Save,
    SaveAndConnect,
}

include!("form_modal.rs");
include!("proxy_chain_view.rs");
include!("field_controls.rs");

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_new_connection_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let saved_connection_form_uses_unloaded_secret =
            self.saved_connection_form_uses_unloaded_secret();
        let Some(form) = self.new_connection_form.as_mut() else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let text_input = text_from_keystroke(&event.keystroke).map(str::to_string);

        if self.open_new_connection_select.is_some()
            && matches!(key, "escape" | "enter" | "tab")
            && !modifiers.platform
        {
            self.close_new_connection_select();
            cx.notify();
            return true;
        }

        if !form.field_focused {
            match key {
                "escape" => {
                    if form.jump_server_form.is_some() {
                        form.jump_server_form = None;
                        cx.notify();
                        return true;
                    }
                    self.close_new_connection_form(window, cx);
                    return true;
                }
                "enter" => {
                    if form.jump_server_form.is_some() {
                        self.add_pending_jump_server(cx);
                        return true;
                    }
                    self.submit_new_connection_form(window, cx);
                    return true;
                }
                "tab" => {
                    form.field_focused = true;
                    self.new_connection_caret_visible = true;
                    cx.notify();
                    return true;
                }
                _ => return true,
            }
        }

        let password_locked = saved_connection_form_uses_unloaded_secret
            && form.focused_field == NewConnectionField::Password
            && !form.password_loaded;
        if password_locked && !matches!(key, "escape" | "enter" | "tab") {
            return true;
        }

        let focused_field_accepts_ime = matches!(
            form.focused_field,
            NewConnectionField::Name
                | NewConnectionField::Host
                | NewConnectionField::Username
                | NewConnectionField::Group
                | NewConnectionField::Color
                | NewConnectionField::TelnetProfileName
                | NewConnectionField::JumpHost
                | NewConnectionField::JumpUsername
                | NewConnectionField::UpstreamProxyHost
                | NewConnectionField::UpstreamProxyNoProxy
                | NewConnectionField::UpstreamProxyUsername
        );

        if modifiers.platform {
            match key {
                "a" => {
                    select_current_connection_field(form);
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
                "c" => {
                    if form.selected_field == Some(form.focused_field) {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            current_connection_field(form).to_string(),
                        ));
                    }
                }
                "x" => {
                    if form.selected_field == Some(form.focused_field) {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            current_connection_field(form).to_string(),
                        ));
                        clear_current_connection_field(form);
                        form.error = None;
                        self.new_connection_caret_visible = true;
                        cx.notify();
                    }
                }
                "v" => {
                    self.paste_into_new_connection_field(cx);
                }
                _ => {}
            }
            return true;
        }

        match key {
            "escape" => {
                if form.jump_server_form.is_some() {
                    form.jump_server_form = None;
                    form.field_focused = false;
                    self.ime_marked_text = None;
                    cx.notify();
                    return true;
                }
                self.close_new_connection_form(window, cx);
                true
            }
            "enter" => {
                if form.jump_server_form.is_some() {
                    self.add_pending_jump_server(cx);
                    return true;
                }
                self.submit_new_connection_form(window, cx);
                true
            }
            "tab" => {
                form.focused_field = if let Some(jump_form) = form.jump_server_form.as_ref() {
                    next_jump_connection_field(
                        form.focused_field,
                        jump_form.auth_tab,
                        !modifiers.shift,
                    )
                } else {
                    next_connection_field(
                        form.focused_field,
                        form.auth_tab,
                        form.transport,
                        form.upstream_proxy_policy,
                        form.upstream_proxy_auth,
                        !modifiers.shift,
                    )
                };
                form.field_focused = true;
                clear_connection_selection(form);
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            "backspace" => {
                let changed = backspace_current_connection_field(form)
                    || form.error.take().is_some()
                    || !self.new_connection_caret_visible;
                if changed {
                    // Empty Backspace in an unchanged field is a browser no-op;
                    // only repaint when text, selection, error, or caret state
                    // actually changes.
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
                true
            }
            "space" => {
                if focused_field_accepts_ime {
                    return true;
                }
                insert_text_into_current_connection_field(form, " ");
                form.error = None;
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            _ => {
                if focused_field_accepts_ime {
                    return true;
                }
                let Some(text) = text_input else {
                    return true;
                };
                insert_text_into_current_connection_field(form, &text);
                form.error = None;
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
        }
    }

    pub(in crate::workspace) fn paste_into_new_connection_field(&mut self, cx: &mut Context<Self>) {
        let saved_connection_form_uses_unloaded_secret =
            self.saved_connection_form_uses_unloaded_secret();
        let Some(form) = self.new_connection_form.as_mut() else {
            return;
        };
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if saved_connection_form_uses_unloaded_secret
            && form.focused_field == NewConnectionField::Password
            && !form.password_loaded
        {
            return;
        }
        let single_line = text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .lines()
            .collect::<Vec<_>>()
            .join(" ");
        insert_text_into_current_connection_field(form, &single_line);
        form.error = None;
        self.new_connection_caret_visible = true;
        cx.notify();
    }
}
