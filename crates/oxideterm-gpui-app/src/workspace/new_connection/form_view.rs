use gpui::{
    AnchoredPositionMode, AnyElement, ClipboardItem, Context, Corner, KeyDownEvent, MouseButton,
    MouseMoveEvent, ParentElement, PathPromptOptions, SharedString, Styled, Window, anchored,
    deferred, div, point, prelude::*, px, rgb, rgba,
};

use super::{
    ConnectionFormState,
    form_state::{
        CONNECTION_NOTES_LINE_HEIGHT, CONNECTION_NOTES_MIN_HEIGHT,
        CONNECTION_NOTES_VERTICAL_PADDING, ConnectionRouteTarget, NewConnectionField,
        NewConnectionForm, NewConnectionFormMode, NewConnectionProxyHop, NewConnectionSelect,
        NewConnectionSubmitAction, NewConnectionTransport, NewConnectionUpstreamProxyAuth,
        NewConnectionUpstreamProxyPolicy, RDP_DEFAULT_PORT_TEXT, RemoteDesktopSessionFeature,
        RemoteDesktopVncPreference, SSH_DEFAULT_PORT_TEXT, SavedConnectionPromptAction,
        SshAuthFamily, SshAuthTab, SshKeyAuthSource, TELNET_DEFAULT_PORT_TEXT,
        VNC_DEFAULT_PORT_TEXT, apply_remote_desktop_vnc_preference, apply_transport_default_port,
        apply_transport_default_username, auth_family_from_tab, auth_tab_from_key_source,
        backspace_current_connection_field, clear_connection_selection,
        clear_current_connection_field, connection_field_is_selected,
        connection_icon_field_visible, connection_secret_field_visible, current_connection_field,
        default_auth_tab_for_family, insert_text_into_current_connection_field,
        key_source_from_tab, new_connection_form_mode, next_connection_field,
        next_jump_connection_field, next_standalone_sftp_field, remote_desktop_feature_selected,
        remote_desktop_feature_supported, remote_desktop_vnc_preference_selected,
        select_current_connection_field, text_from_keystroke,
        toggle_connection_secret_field_visibility, toggle_remote_desktop_feature,
    },
    ssh_flow::SshConnectionIntent,
};
use crate::assets::LucideIcon;
use crate::workspace::SelectableTextScrollExt;
use crate::workspace::WorkspaceApp;
use crate::workspace::{
    browser_behavior,
    ime::{WorkspaceImeTarget, keystroke_uses_text_edit_modifier},
    session_icons::{SESSION_ICON_CHOICES, session_icon_from_id},
};
use gpui::Div;
use oxideterm_connections::{
    ConnectionTerminalBackspaceSequence, ConnectionTerminalDeleteSequence,
    ConnectionTerminalEncoding, ConnectionX11ForwardingMode, MoshIpFamily, MoshPredictionMode,
    SavedUpstreamProxyProtocol, StandaloneSftpTransferMode,
};
use oxideterm_gpui_settings_view::{
    terminal_backspace_sequence_label, terminal_delete_sequence_label, terminal_encoding_label,
};
use oxideterm_gpui_ui::{
    ActionChipOptions, ButtonTone, CheckboxOptions, ScrollableElement, StatusPillOptions,
    StatusTone, TextInputView, action_chip, button,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions,
    },
    checkbox, checkbox_with, form_field,
    modal::{dialog_backdrop_color, dismissible_dialog_backdrop, modal_backdrop, popover_backdrop},
    modal_body, modal_container, modal_footer, modal_header, segmented_tab, segmented_tabs,
    select::{
        SelectAnchorId, select_anchor_probe, select_option, select_option_action,
        select_overlay_popup_with_max_height, select_trigger_with_focus_visible,
    },
    status_pill, text_input,
    text_input::{
        text_caret, text_input_value_segments, text_input_value_segments_with_marked_range,
    },
};
use oxideterm_remote_desktop::{
    RemoteDesktopVncCompression, RemoteDesktopVncImageQuality, RemoteDesktopVncSecurityPolicy,
    RemoteDesktopVncSessionMode,
};
use oxideterm_settings_model::{settings_multiline_line_ranges, settings_multiline_line_selection};
// Keep the modal, proxy-chain, and field-control implementations in explicit
// submodules so their dependencies and visibility remain locally auditable.
mod field_controls;
mod form_modal;
mod proxy_chain_view;
mod ssh_algorithm_editor;
mod standalone_sftp_modal;

use field_controls::{AuthSelectorContext, ConnectionFormSection, serial_port_display_label};

const TAURI_EDIT_MODAL_WIDTH: f32 = 500.0; // Tauri sm:max-w-[500px]
const TAURI_EDIT_COLOR_FALLBACK: u32 = 0x22d3ee;
const TAURI_EDIT_COLOR_FALLBACK_TEXT: &str = "#22d3ee";
const TAURI_PROMPT_FEEDBACK_ALPHA: u32 = 0x1a;
const TAURI_PROMPT_FEEDBACK_BORDER_ALPHA: u32 = 0x80;
const SECRET_VISIBILITY_BUTTON_SIZE: f32 = 28.0;
const SECRET_VISIBILITY_BUTTON_OFFSET: f32 = 4.0;
const SECRET_VISIBILITY_ICON_SIZE: f32 = 16.0;
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
const NEW_CONNECTION_TYPE_SIDEBAR_WIDTH: f32 = 160.0;
const SSH_ALGORITHM_CATEGORY_COLUMN_WIDTH: f32 = 208.0;
const SSH_ALGORITHM_DETAIL_COLUMN_WIDTH: f32 = 420.0;
const NEW_CONNECTION_MODAL_VIEWPORT_MARGIN: f32 = 32.0;
const CONNECTION_TERMINAL_CONTROL_MIN_WIDTH: f32 = 220.0;
const CONNECTION_ICON_COLOR_CONTROL_MIN_WIDTH: f32 = 220.0;

// Persistence enums stay independent from the settings crate while sharing user-facing labels.
fn connection_terminal_encoding_label(encoding: ConnectionTerminalEncoding) -> &'static str {
    match encoding {
        ConnectionTerminalEncoding::Utf8 => "UTF-8",
        ConnectionTerminalEncoding::Gbk => "GBK",
        ConnectionTerminalEncoding::Gb18030 => "GB18030",
        ConnectionTerminalEncoding::Big5 => "Big5",
        ConnectionTerminalEncoding::ShiftJis => "Shift_JIS",
        ConnectionTerminalEncoding::EucJp => "EUC-JP",
        ConnectionTerminalEncoding::EucKr => "EUC-KR",
        ConnectionTerminalEncoding::Windows1252 => "Windows-1252",
    }
}

fn connection_terminal_backspace_sequence_label(
    sequence: ConnectionTerminalBackspaceSequence,
) -> &'static str {
    match sequence {
        ConnectionTerminalBackspaceSequence::Delete => "DEL (0x7F)",
        ConnectionTerminalBackspaceSequence::ControlH => "Ctrl+H (0x08)",
    }
}

fn connection_terminal_delete_sequence_label(
    sequence: ConnectionTerminalDeleteSequence,
) -> &'static str {
    match sequence {
        ConnectionTerminalDeleteSequence::Csi3Tilde => "CSI 3~",
        ConnectionTerminalDeleteSequence::Delete => "DEL (0x7F)",
        ConnectionTerminalDeleteSequence::ControlH => "Ctrl+H (0x08)",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConnectionButtonAction {
    Cancel,
    Test,
    Connect,
    Save,
    SaveAndConnect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConnectionFormKeyResult {
    NotHandled,
    Handled,
    CloseForm,
    CloseJumpForm,
    Submit,
    AddJumpServer,
    Paste,
}

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_new_connection_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let saved_connection_form_uses_unloaded_secret =
            self.saved_connection_form_uses_unloaded_secret(cx);
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let text_input = text_from_keystroke(&event.keystroke).map(str::to_string);

        if self.connection_form_state(cx).open_select.is_some()
            && matches!(key, "escape" | "enter" | "tab")
            && !modifiers.platform
        {
            self.close_new_connection_select(cx);
            cx.notify();
            return true;
        }

        let caret_was_visible = self.input_caret.visible();
        let uses_text_edit_modifier = keystroke_uses_text_edit_modifier(&event.keystroke);
        let (result, show_caret) = self.connection_flow.update(cx, |connection_flow, cx| {
            let Some(form) = connection_flow.form.form.as_mut() else {
                return (ConnectionFormKeyResult::NotHandled, false);
            };
            if !form.field_focused {
                return match key {
                    "escape" if form.jump_server_form.is_some() => {
                        (ConnectionFormKeyResult::CloseJumpForm, false)
                    }
                    "escape" => (ConnectionFormKeyResult::CloseForm, false),
                    "enter" if form.jump_server_form.is_some() => {
                        (ConnectionFormKeyResult::AddJumpServer, false)
                    }
                    "enter" => (ConnectionFormKeyResult::Submit, false),
                    "tab" => {
                        form.field_focused = true;
                        cx.notify();
                        (ConnectionFormKeyResult::Handled, true)
                    }
                    _ => (ConnectionFormKeyResult::Handled, false),
                };
            }

            let password_uses_saved_value = saved_connection_form_uses_unloaded_secret
                && form.focused_field == NewConnectionField::Password
                && !form.password_loaded;
            if password_uses_saved_value {
                if uses_text_edit_modifier && key == "v" {
                    return (ConnectionFormKeyResult::Paste, false);
                }
                if text_input.is_some() || key == "space" {
                    // The protected value stays in the keychain; only new input becomes UI-owned.
                    form.password_loaded = true;
                } else if !matches!(key, "escape" | "enter" | "tab") {
                    return (ConnectionFormKeyResult::Handled, false);
                }
            }

            let focused_field_accepts_ime = matches!(
                form.focused_field,
                NewConnectionField::Name
                    | NewConnectionField::Host
                    | NewConnectionField::Username
                    | NewConnectionField::Group
                    | NewConnectionField::Notes
                    | NewConnectionField::InitialRemotePath
                    | NewConnectionField::StandaloneSftpSecondaryHost
                    | NewConnectionField::StandaloneSftpSecondaryUsername
                    | NewConnectionField::StandaloneSftpSecondaryInitialRemotePath
                    | NewConnectionField::StandaloneSftpSecondaryIdentityAgent
                    | NewConnectionField::ProxyCommand
                    | NewConnectionField::Color
                    | NewConnectionField::IdentityAgent
                    | NewConnectionField::TelnetProfileName
                    | NewConnectionField::MoshServerExecutable
                    | NewConnectionField::MoshUdpHost
                    | NewConnectionField::MoshLocale
                    | NewConnectionField::JumpHost
                    | NewConnectionField::JumpUsername
                    | NewConnectionField::JumpIdentityAgent
                    | NewConnectionField::UpstreamProxyHost
                    | NewConnectionField::UpstreamProxyNoProxy
                    | NewConnectionField::UpstreamProxyUsername
            );

            if uses_text_edit_modifier {
                let mut show_caret = false;
                match key {
                    "a" => {
                        select_current_connection_field(form);
                        show_caret = true;
                    }
                    "c" if form.selected_field == Some(form.focused_field) => {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            current_connection_field(form).to_string(),
                        ));
                    }
                    "x" if form.selected_field == Some(form.focused_field) => {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            current_connection_field(form).to_string(),
                        ));
                        clear_current_connection_field(form);
                        restore_saved_password_placeholder_if_empty(
                            form,
                            saved_connection_form_uses_unloaded_secret,
                        );
                        form.error = None;
                        show_caret = true;
                    }
                    "v" => return (ConnectionFormKeyResult::Paste, false),
                    _ => {}
                }
                if show_caret {
                    cx.notify();
                }
                return (ConnectionFormKeyResult::Handled, show_caret);
            }

            match key {
                "escape" if form.jump_server_form.is_some() => {
                    (ConnectionFormKeyResult::CloseJumpForm, false)
                }
                "escape" => (ConnectionFormKeyResult::CloseForm, false),
                "enter" if form.jump_server_form.is_some() => {
                    (ConnectionFormKeyResult::AddJumpServer, false)
                }
                "enter" => (ConnectionFormKeyResult::Submit, false),
                "tab" => {
                    form.focused_field = if let Some(jump_form) = form.jump_server_form.as_ref() {
                        next_jump_connection_field(
                            form.focused_field,
                            jump_form.auth_tab,
                            jump_form.gssapi_enabled,
                            !modifiers.shift,
                        )
                    } else if form.transport == NewConnectionTransport::StandaloneSftp {
                        next_standalone_sftp_field(form, !modifiers.shift)
                    } else {
                        next_connection_field(
                            form.focused_field,
                            form.auth_tab,
                            form.gssapi_enabled,
                            form.transport,
                            form.upstream_proxy_policy,
                            form.upstream_proxy_auth,
                            !modifiers.shift,
                        )
                    };
                    form.field_focused = true;
                    clear_connection_selection(form);
                    cx.notify();
                    (ConnectionFormKeyResult::Handled, true)
                }
                "backspace" => {
                    let field_changed = backspace_current_connection_field(form);
                    restore_saved_password_placeholder_if_empty(
                        form,
                        saved_connection_form_uses_unloaded_secret,
                    );
                    let changed =
                        field_changed || form.error.take().is_some() || !caret_was_visible;
                    if changed {
                        cx.notify();
                    }
                    (ConnectionFormKeyResult::Handled, changed)
                }
                "space" if !focused_field_accepts_ime => {
                    insert_text_into_current_connection_field(form, " ");
                    form.error = None;
                    cx.notify();
                    (ConnectionFormKeyResult::Handled, true)
                }
                _ if focused_field_accepts_ime => (ConnectionFormKeyResult::Handled, false),
                _ => {
                    if let Some(text) = text_input.as_deref() {
                        insert_text_into_current_connection_field(form, text);
                        form.error = None;
                        cx.notify();
                        (ConnectionFormKeyResult::Handled, true)
                    } else {
                        (ConnectionFormKeyResult::Handled, false)
                    }
                }
            }
        });

        if show_caret {
            self.show_active_input_caret(cx);
            cx.notify();
        }
        match result {
            ConnectionFormKeyResult::NotHandled => false,
            ConnectionFormKeyResult::Handled => true,
            ConnectionFormKeyResult::CloseForm => {
                self.close_new_connection_form(window, cx);
                true
            }
            ConnectionFormKeyResult::CloseJumpForm => {
                self.begin_jump_server_form_exit(cx);
                true
            }
            ConnectionFormKeyResult::Submit => {
                self.submit_new_connection_form(window, cx);
                true
            }
            ConnectionFormKeyResult::AddJumpServer => {
                self.add_pending_jump_server(cx);
                true
            }
            ConnectionFormKeyResult::Paste => {
                self.paste_into_new_connection_field(cx);
                true
            }
        }
    }

    pub(in crate::workspace) fn paste_into_new_connection_field(&mut self, cx: &mut Context<Self>) {
        let saved_connection_form_uses_unloaded_secret =
            self.saved_connection_form_uses_unloaded_secret(cx);
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let pasted = self.update_connection_form_state(cx, |state| {
            let Some(form) = state.form.as_mut() else {
                return false;
            };
            if form.focused_field == NewConnectionField::Notes {
                insert_text_into_current_connection_field(form, &normalized);
            } else {
                // All other connection form controls remain single-line inputs.
                let single_line = normalized.lines().collect::<Vec<_>>().join(" ");
                if saved_connection_form_uses_unloaded_secret
                    && form.focused_field == NewConnectionField::Password
                    && !form.password_loaded
                {
                    if single_line.is_empty() {
                        return false;
                    }
                    // A pasted replacement is owned by the form; the saved value is never loaded.
                    form.password_loaded = true;
                }
                insert_text_into_current_connection_field(form, &single_line);
            }
            form.error = None;
            true
        });
        if pasted {
            self.show_active_input_caret(cx);
            cx.notify();
        }
    }
}

fn restore_saved_password_placeholder_if_empty(
    form: &mut NewConnectionForm,
    editing_saved_connection: bool,
) {
    if editing_saved_connection
        && form.focused_field == NewConnectionField::Password
        && form.saved_password_keychain_id.is_some()
        && form.password.is_empty()
    {
        form.password_loaded = false;
        form.password_visible = false;
    }
}
