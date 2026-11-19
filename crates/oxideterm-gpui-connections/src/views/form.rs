use gpui::{
    AnchoredPositionMode, AnyElement, App, ClipboardItem, Context, Corner, Entity, KeyDownEvent,
    MouseButton, MouseMoveEvent, ParentElement, PathPromptOptions, Render, Rgba, ScrollHandle,
    SharedString, Styled, Subscription, Timer, Window, anchored, deferred, div, point, prelude::*,
    px, rgb, rgba,
};
use zeroize::Zeroize;

use crate::{
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
    launch::SshConnectionIntent,
};

use super::{
    ConnectionImeFrame, ConnectionsDispatch, ConnectionsEnvironment, ConnectionsInput,
    ConnectionsViewBindings, ConnectionsWindowRequest, algorithms::SshAlgorithmEditor,
    connection_text_input,
};
use crate::icons::{DISTRO_ICONS, SESSION_ICON_CHOICES, SessionIcon, session_icon_from_id};
use crate::{
    configuration::restore_saved_password_placeholder_if_empty, entity::ConnectionFlowEntity,
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
use oxideterm_gpui_ui::icons::LucideIcon;
use oxideterm_gpui_ui::select::BrowserFocusOrigin;
use oxideterm_gpui_ui::{
    ButtonTone, CheckboxOptions, TextInputView, button,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions,
    },
    checkbox, checkbox_with, form_field,
    modal::{dismissible_dialog_backdrop, popover_backdrop},
    modal_body, modal_container, modal_footer, modal_header, segmented_tab, segmented_tabs,
    select::{
        SelectAnchorId, select_option, select_option_action, select_overlay_popup_with_max_height,
    },
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
mod ble_fields;
mod composer;
mod field_controls;
mod form_modal;
mod ftp;
mod proxy_chain_view;
mod standalone_sftp_modal;
mod tunnel;
pub use tunnel::{TunnelFormEnvironment, TunnelKind};

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
const TAURI_PROXY_CHAIN_MAX_HEIGHT: f32 = 250.0; // Tauri max-h-[250px]
const TAURI_PROXY_CHAIN_SECTION_PADDING: f32 = 16.0; // Tauri p-4
const TAURI_PROXY_CHAIN_HEADER_MARGIN: f32 = 16.0; // Tauri mb-4
const TAURI_PROXY_CHAIN_NODE_SIZE: f32 = 32.0; // Tauri w-8 h-8
const TAURI_PROXY_CHAIN_LINE_WIDTH: f32 = 32.0; // Tauri w-8
const TAURI_PROXY_CHAIN_CONNECTOR_THICKNESS: f32 = 2.0; // Tauri w-0.5 h-0.5
const TAURI_PROXY_CHAIN_CARD_PADDING: f32 = 12.0; // Tauri p-3
const TAURI_SERIAL_GRID_GAP: f32 = 16.0; // Tauri serial grid gap-4
pub const NEW_CONNECTION_TYPE_SIDEBAR_WIDTH: f32 = 160.0;
use super::algorithms::{SSH_ALGORITHM_CATEGORY_COLUMN_WIDTH, SSH_ALGORITHM_DETAIL_COLUMN_WIDTH};
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

use oxideterm_connections::{ConnectionInfo, ManagedSshKeyInfo};
use oxideterm_i18n::I18n;
use oxideterm_ssh::NodeId;
use oxideterm_terminal::ShellInfo;
use oxideterm_theme::ThemeTokens;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Clone)]
pub struct FormPluginType {
    pub id: String,
    pub title: String,
    pub icon: String,
}

pub struct FormTotpCredential {
    pub id: String,
    pub name: String,
}

#[derive(Default)]
pub struct ConnectionFormEnvironment {
    pub connections: Vec<ConnectionInfo>,
    pub groups: Vec<String>,
    pub totp: Vec<FormTotpCredential>,
    pub keys: Vec<ManagedSshKeyInfo>,
    pub node_titles: HashMap<NodeId, String>,
    pub tunnel: Option<TunnelFormEnvironment>,
    pub terminal: oxideterm_settings::TerminalSettings,
    pub ui_font: String,
    pub default_shell_id: Option<String>,
    pub local_shells: Vec<ShellInfo>,
    pub plugin_types: Vec<(String, FormPluginType)>,
    pub installed_plugins: std::collections::HashSet<String>,
    pub mosh_available: bool,
    pub rdp_available: bool,
    pub vnc_available: bool,
    pub panel_glass: bool,
}

impl ConnectionFormEnvironment {
    pub fn from_store(
        store: &oxideterm_connections::ConnectionStore,
        settings: &oxideterm_settings::PersistedSettings,
    ) -> Self {
        Self {
            connections: store.connection_infos(),
            groups: store.groups().to_vec(),
            totp: store
                .totp_credentials()
                .iter()
                .map(|entry| FormTotpCredential {
                    id: entry.id.clone(),
                    name: entry.name.clone(),
                })
                .collect(),
            keys: store.managed_ssh_keys(),
            terminal: settings.terminal.clone(),
            ui_font: settings.appearance.ui_font_family.clone(),
            default_shell_id: settings.local_terminal.default_shell_id.clone(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum FormMount {
    Body,
    Overlays,
}

pub(super) struct ConnectionFormView {
    mount: FormMount,
    flow: Entity<ConnectionFlowEntity>,
    _flow_changed: Subscription,
    tokens: ThemeTokens,
    i18n: I18n,
    theme: super::ConnectionsEnvironmentSource,
    environment_source: Rc<dyn Fn(&App) -> Option<ConnectionFormEnvironment>>,
    environment: ConnectionFormEnvironment,
    runtime: tokio::runtime::Handle,
    ime: super::ConnectionImeSource,
    dispatch: ConnectionsDispatch,
    selectable_text: Rc<dyn Fn(&str, (&str, &str), String, u32, &mut App) -> AnyElement>,
    scroll_handles: RefCell<HashMap<String, ScrollHandle>>,
    user_motion: oxideterm_gpui_ui::motion::UserSegmentedControlMotionState,
}

impl ConnectionFormView {
    pub(super) fn new(
        flow: Entity<ConnectionFlowEntity>,
        bindings: &ConnectionsViewBindings,
        theme: &ConnectionsEnvironment,
        runtime: tokio::runtime::Handle,
        mount: FormMount,
        cx: &mut Context<Self>,
    ) -> Self {
        let flow_changed = cx.observe(&flow, |_, _, cx| cx.notify());
        Self {
            mount,
            flow,
            _flow_changed: flow_changed,
            tokens: theme.tokens,
            i18n: theme.i18n.clone(),
            theme: bindings.environment.clone(),
            environment_source: bindings.form_environment.clone(),
            environment: ConnectionFormEnvironment::default(),
            runtime,
            ime: bindings.ime.clone(),
            dispatch: bindings.dispatch.clone(),
            selectable_text: bindings.form_selectable_text.clone(),
            scroll_handles: RefCell::new(HashMap::new()),
            user_motion: Default::default(),
        }
    }

    fn request(
        &self,
        request: ConnectionsWindowRequest,
        window: Option<&mut Window>,
        cx: &mut App,
    ) {
        (self.dispatch)(request, window, cx);
    }

    fn connection_form_state<'a>(&self, cx: &'a App) -> &'a ConnectionFormState {
        &self.flow.read(cx).form
    }

    fn update_connection_form_state<R>(
        &self,
        cx: &mut App,
        update: impl FnOnce(&mut ConnectionFormState) -> R,
    ) -> R {
        self.flow.update(cx, |flow, cx| {
            let result = update(&mut flow.form);
            cx.notify();
            result
        })
    }

    pub(super) fn selectable_text_scroll_handle(&self, key: impl Into<String>) -> ScrollHandle {
        self.scroll_handles
            .borrow_mut()
            .entry(key.into())
            .or_default()
            .clone()
    }

    fn frame(&self, target: ConnectionsInput, cx: &App) -> Option<ConnectionImeFrame> {
        (self.ime)(target, cx)
    }

    fn render_selectable_text_scoped(
        &self,
        scope: &str,
        key: (&str, &str),
        value: String,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        (self.selectable_text)(scope, key, value, color, cx)
    }

    fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
        gpui::svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    fn render_animated_chevron(
        &self,
        id: impl Into<gpui::ElementId>,
        expanded: bool,
        size: f32,
        color: Rgba,
    ) -> AnyElement {
        oxideterm_gpui_ui::motion::animated_chevron(
            &self.tokens,
            id,
            gpui::svg()
                .path(LucideIcon::ChevronRight.path())
                .size(px(size))
                .text_color(color),
            expanded,
        )
    }

    fn render_loading_icon(
        &self,
        id: impl Into<gpui::ElementId>,
        size: f32,
        color: Rgba,
    ) -> AnyElement {
        oxideterm_gpui_ui::motion::animated_spinner(
            &self.tokens,
            id,
            gpui::svg()
                .path(LucideIcon::LoaderCircle.path())
                .size(px(size))
                .text_color(color),
        )
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        listener: impl Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        oxideterm_gpui_ui::button::toolbar_action_button(
            &self.tokens,
            label,
            icon,
            options,
            listener,
        )
    }

    fn workspace_icon_action_button(
        &self,
        icon: LucideIcon,
        size: f32,
        color: Rgba,
        options: IconButtonOptions,
        listener: impl Fn(&mut Self, &gpui::MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        let actionable = !(options.disabled || options.loading);
        oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, size, color),
            options,
        )
        .when(actionable, |button| {
            button.on_mouse_down(MouseButton::Left, cx.listener(listener))
        })
        .when(!actionable, |button| {
            button.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        })
    }

    fn plugin_requirement_message(&self, id: &str, name: &str, _cx: &App) -> String {
        self.i18n
            .t(if self.environment.installed_plugins.contains(id) {
                "plugin.enable_required"
            } else {
                "plugin.install_required"
            })
            .replace("{{name}}", name)
    }

    fn algorithms(&self) -> SshAlgorithmEditor<'_> {
        SshAlgorithmEditor::new(&self.flow, &self.tokens, &self.i18n, &self.dispatch)
    }

    fn render_ssh_algorithms_navigation_row(&self, cx: &App) -> AnyElement {
        self.algorithms().render_ssh_algorithms_navigation_row(cx)
    }
    fn render_ssh_algorithm_category_column(&self, cx: &App) -> AnyElement {
        self.algorithms().render_ssh_algorithm_category_column(cx)
    }
    fn render_ssh_algorithm_detail_column(&self, cx: &App) -> AnyElement {
        self.algorithms().render_ssh_algorithm_detail_column(cx)
    }
}

impl Render for ConnectionFormView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.flow.read(cx).form.form.is_none() {
            return div().into_any_element();
        }
        let Some(theme) = (self.theme)(cx) else {
            return div().into_any_element();
        };
        let Some(environment) = (self.environment_source)(cx) else {
            return div().into_any_element();
        };
        self.tokens = theme.tokens;
        self.i18n = theme.i18n;
        self.environment = environment;
        match self.mount {
            FormMount::Body => div()
                .absolute()
                .inset_0()
                .child(if self.environment.tunnel.is_none() {
                    self.render_new_connection_modal(window, cx)
                } else {
                    self.render_ssh_tunnel_form(window, cx)
                        .unwrap_or_else(|| div().into_any_element())
                })
                .into_any_element(),
            FormMount::Overlays => {
                let jump_open = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .is_some_and(|form| form.jump_server_form.is_some());
                let jump = jump_open.then(|| self.render_add_jump_server_modal(window, cx));
                div()
                    .absolute()
                    .inset_0()
                    .children(jump)
                    .children(self.render_composer_overlays(cx))
                    .children(self.render_new_connection_select_overlay(window, cx))
                    .children(self.algorithms().render_ssh_algorithm_context_menu(cx))
                    .into_any_element()
            }
        }
    }
}
const NEW_CONNECTION_AUTH_SELECTOR_ID: &str = "new-connection-auth-selector";
const EDIT_CONNECTION_AUTH_SELECTOR_ID: &str = "edit-connection-auth-selector";
const PROMPT_CONNECTION_AUTH_SELECTOR_ID: &str = "prompt-connection-auth-selector";
const DRILL_DOWN_AUTH_SELECTOR_ID: &str = "drill-down-auth-selector";
const JUMP_CONNECTION_AUTH_SELECTOR_ID: &str = "jump-connection-auth-selector";

impl ConnectionFormView {
    fn close_new_connection_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.request(ConnectionsWindowRequest::CloseForm, Some(window), cx);
    }

    fn show_active_input_caret(&self, cx: &mut Context<Self>) {
        self.request(ConnectionsWindowRequest::ShowCaret, None, cx);
    }

    fn submit_new_connection_form_with_action(
        &self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request(
            ConnectionsWindowRequest::SubmitForm(action),
            Some(window),
            cx,
        );
    }

    fn start_new_connection_flow(
        &self,
        intent: SshConnectionIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request(ConnectionsWindowRequest::TestForm(intent), Some(window), cx);
    }

    fn ensure_kerberos_credentials_availability(&self, cx: &mut Context<Self>) {
        let runtime = self.runtime.clone();
        self.flow.update(cx, |flow, cx| {
            flow.ensure_kerberos_credentials_availability(runtime, cx)
        });
    }

    fn queue_workspace_tooltip(
        &self,
        id: &str,
        label: String,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.request(
            ConnectionsWindowRequest::QueueTooltip {
                id: id.to_owned(),
                label,
                x,
                y,
            },
            None,
            cx,
        );
    }

    fn clear_workspace_tooltip(&self, id: &str, cx: &mut Context<Self>) {
        self.request(
            ConnectionsWindowRequest::ClearTooltip(id.to_owned()),
            None,
            cx,
        );
    }

    fn settings_multiline_input_viewport(
        &self,
        target: ConnectionsInput,
        content: AnyElement,
        cx: &App,
    ) -> AnyElement {
        let Some(frame) = self.frame(target, cx) else {
            return content;
        };
        let scroll =
            self.selectable_text_scroll_handle(format!("multiline-input-{}", frame.anchor_id.0));
        oxideterm_gpui_settings_view::components::multiline_input_viewport(
            frame.anchor_id,
            content,
            false,
            &scroll,
        )
    }
}
impl ConnectionFormView {
    pub(super) fn begin_user_segmented_control_transition_from(
        &mut self,
        control_id: &'static str,
        previous_index: usize,
        target_index: usize,
        cx: &mut Context<Self>,
    ) {
        self.begin_user_segmented_control_transition_with_history(
            control_id,
            Some(previous_index),
            target_index,
            None,
            cx,
        );
    }

    fn begin_user_segmented_control_transition_with_history(
        &mut self,
        control_id: &'static str,
        previous_index: Option<usize>,
        target_index: usize,
        vertical_offset_y: Option<f32>,
        cx: &mut Context<Self>,
    ) {
        let Some(motion) = oxideterm_gpui_ui::segmented_control_motion(&self.tokens) else {
            self.user_motion.clear(control_id);
            return;
        };
        let generation = if previous_index.is_some() {
            self.user_motion.begin_with_previous_index(
                control_id,
                previous_index,
                target_index,
                vertical_offset_y,
            )
        } else {
            self.user_motion
                .begin_with_vertical_offset(control_id, target_index, vertical_offset_y)
        };
        // User intent outlives a virtual-list row only for the real transition,
        // then expires so remounts and programmatic navigation render settled.
        cx.spawn(async move |weak, cx| {
            Timer::after(motion.duration).await;
            let _ = weak.update(cx, |this, cx| {
                if this.user_motion.finish(control_id, generation) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn segmented_control_user_transition(
        &self,
        control_id: &'static str,
        active_index: usize,
    ) -> Option<(u64, Option<f32>)> {
        self.user_motion.transition_for(control_id, active_index)
    }

    pub(super) fn segmented_control_user_previous_index(
        &self,
        control_id: &'static str,
        active_index: usize,
    ) -> Option<usize> {
        self.user_motion
            .previous_index_for(control_id, active_index)
    }
}

mod keys;
pub use keys::FormKeyAction;
