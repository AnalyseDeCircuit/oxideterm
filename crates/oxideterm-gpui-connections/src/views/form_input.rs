use super::{ConnectionsEnvironment, TunnelKind};
use crate::{NewConnectionField, form_state::NewConnectionSubmitAction};
use gpui::{
    AnyElement, App, CursorStyle, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, Window,
    prelude::*,
};
use oxideterm_gpui_ui::text_input::{
    TextInputAnchor, TextInputAnchorId, TextInputView, TextInputViewport, text_input_anchor_probe,
};
use oxideterm_theme::ThemeTokens;
use std::{ops::Range, rc::Rc};
use zeroize::Zeroizing;
#[derive(Clone, Copy)]
pub enum ConnectionsInput {
    Form(NewConnectionField),
    KeyboardInteractive(usize),
}

pub(super) enum FormAction {
    FocusForm,
    FocusFormSelect,
    FocusInput(ConnectionsInput, MouseDownEvent),
    DragInputSelection(MouseMoveEvent),
    ClearComposition,
    ShowCaret,
    CloseForm,
    SubmitForm(NewConnectionSubmitAction),
    TestForm {
        standalone_sftp: bool,
    },
    OpenTunnel,
    LeaveTunnel,
    CloseTunnel,
    SaveTunnel {
        start: bool,
    },
    SetTunnelKind(TunnelKind),
    ToggleTunnelStartup,
    SelectTunnelHost(String),
    OpenPluginForm {
        plugin: String,
        type_id: String,
    },
    OpenMoshPlugin,
    OpenRemoteDesktopPlugin(oxideterm_remote_desktop::RemoteDesktopProtocol),
    #[cfg(not(target_family = "wasm"))]
    ConnectNextHop {
        parent: crate::form_model::NodeId,
        connection_id: String,
    },
    RevealSavedPassword,
    #[cfg(target_family = "wasm")]
    RequestBleScan,
    QueueTooltip {
        id: String,
        label: String,
        x: f32,
        y: f32,
    },
    ClearTooltip(String),
}

pub struct ConnectionImeFrame {
    pub anchor_id: TextInputAnchorId,
    pub viewport: TextInputViewport,
    pub caret_visible: bool,
    pub selected_range: Option<Range<usize>>,
    pub active_offset: Option<usize>,
    // The platform composition may contain a pending secret character.
    pub marked_text: Option<Zeroizing<String>>,
    pub marked_range: Option<Range<usize>>,
    pub notes_with_composition: Option<String>,
    pub update_anchor: Rc<dyn Fn(TextInputAnchor)>,
}

pub(super) type FormDispatch = Rc<dyn Fn(FormAction, Option<&mut Window>, &mut App)>;
pub type ConnectionsEnvironmentSource = Rc<dyn Fn(&App) -> Option<ConnectionsEnvironment>>;
pub type ConnectionImeSource = Rc<dyn Fn(ConnectionsInput, &App) -> Option<ConnectionImeFrame>>;

fn is_mono_connection_field(field: crate::form_state::NewConnectionField) -> bool {
    use crate::form_state::NewConnectionField as Field;
    matches!(
        field,
        Field::LocalCwd
            | Field::Host
            | Field::Port
            | Field::Username
            | Field::KeyPath
            | Field::CertPath
            | Field::GssapiServerIdentity
            | Field::IdentityAgent
            | Field::InitialRemotePath
            | Field::ConnectTimeoutSeconds
            | Field::KeepAliveIntervalSeconds
            | Field::KeepAliveCountMax
            | Field::StandaloneSftpSecondaryHost
            | Field::StandaloneSftpSecondaryPort
            | Field::StandaloneSftpSecondaryUsername
            | Field::StandaloneSftpSecondaryKeyPath
            | Field::StandaloneSftpSecondaryCertPath
            | Field::StandaloneSftpSecondaryGssapiServerIdentity
            | Field::StandaloneSftpSecondaryIdentityAgent
            | Field::StandaloneSftpSecondaryInitialRemotePath
            | Field::StandaloneSftpSecondaryConnectTimeoutSeconds
            | Field::StandaloneSftpSecondaryProxyCommand
            | Field::StandaloneSftpSecondaryUpstreamProxyHost
            | Field::StandaloneSftpSecondaryUpstreamProxyPort
            | Field::StandaloneSftpSecondaryUpstreamProxyNoProxy
            | Field::StandaloneSftpSecondaryUpstreamProxyUsername
            | Field::PostConnectCommand
            | Field::PreConnectCommand
            | Field::ProxyCommand
            | Field::JumpHost
            | Field::JumpPort
            | Field::JumpUsername
            | Field::JumpKeyPath
            | Field::JumpCertPath
            | Field::JumpGssapiServerIdentity
            | Field::JumpIdentityAgent
            | Field::UpstreamProxyHost
            | Field::UpstreamProxyPort
            | Field::UpstreamProxyNoProxy
            | Field::UpstreamProxyUsername
            | Field::SerialPortPath
            | Field::SerialBaudRate
            | Field::BleMtu
            | Field::MoshServerExecutable
            | Field::MoshUdpHost
            | Field::MoshUdpPort
            | Field::MoshLocale
            | Field::TunnelTargetHost
            | Field::TunnelTargetPort
    )
}

pub(super) fn form_text_input(
    tokens: &ThemeTokens,
    dispatch: &FormDispatch,
    target: ConnectionsInput,
    frame: ConnectionImeFrame,
    view: TextInputView<'_>,
) -> AnyElement {
    let is_mono =
        matches!(target, ConnectionsInput::Form(field) if is_mono_connection_field(field));
    let mut input = crate::input::paint_input(
        tokens,
        TextInputView {
            caret_visible: frame.caret_visible,
            selected_range: frame.selected_range,
            marked_text: frame.marked_text.as_ref().map(|text| text.as_str()),
            ..view
        },
        &frame.viewport,
        frame.active_offset,
    );
    if is_mono {
        input = input.font_family(gpui::SharedString::from(
            tokens.metrics.markdown_code_font_family,
        ));
    }
    bind_connection_input(
        input,
        target,
        frame.anchor_id,
        frame.update_anchor,
        dispatch,
    )
}

pub(super) fn bind_connection_input(
    input: gpui::Div,
    target: ConnectionsInput,
    anchor_id: TextInputAnchorId,
    update_anchor: Rc<dyn Fn(TextInputAnchor)>,
    dispatch: &FormDispatch,
) -> AnyElement {
    let focus = dispatch.clone();
    let drag = dispatch.clone();
    let id = match target {
        ConnectionsInput::Form(field) => ("connection-field", field as usize),
        ConnectionsInput::KeyboardInteractive(index) => ("kbi-prompt", index),
    };
    let input = match target {
        ConnectionsInput::Form(_) => input.cursor(CursorStyle::IBeam),
        ConnectionsInput::KeyboardInteractive(_) => input,
    };
    text_input_anchor_probe(
        anchor_id,
        input
            .id(id)
            .on_mouse_down(
                MouseButton::Left,
                move |event: &MouseDownEvent, window, cx| {
                    focus(
                        FormAction::FocusInput(target, event.clone()),
                        Some(window),
                        cx,
                    );
                    cx.stop_propagation();
                },
            )
            .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                drag(
                    FormAction::DragInputSelection(event.clone()),
                    Some(window),
                    cx,
                );
            }),
        move |anchor, _, _| update_anchor(anchor),
    )
    .into_any_element()
}
