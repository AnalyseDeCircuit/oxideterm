use gpui::Div;
use oxideterm_gpui_ui::select::{
    BrowserFocusOrigin, SelectAnchorId, browser_focus_visible, select_trigger_with_focus_visible,
};
use oxideterm_theme::ThemeTokens;

mod controls;
pub(super) use controls::{group_select, labelled_select, select_trigger, track_select_anchor};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NewConnectionSelect {
    Totp,
    JumpTotp,
    Group,
    KeyAuthSource,
    ManagedKey,
    StandaloneSftpSecondaryKeyAuthSource,
    StandaloneSftpSecondaryManagedKey,
    JumpSavedConnection,
    TunnelHost,
    JumpKeyAuthSource,
    JumpManagedKey,
    UpstreamProxyPolicy,
    UpstreamProxyProtocol,
    UpstreamProxyAuth,
    StandaloneSftpSecondaryUpstreamProxyPolicy,
    StandaloneSftpSecondaryUpstreamProxyProtocol,
    StandaloneSftpSecondaryUpstreamProxyAuth,
    RemoteDesktopSshGateway,
    LocalShell,
    SerialPort,
    BleAdapter,
    SerialDataBits,
    SerialStopBits,
    SerialParity,
    SerialFlowControl,
    TerminalEncoding,
    TerminalBackspaceSequence,
    TerminalDeleteSequence,
    TelnetInputLineEnding,
    TelnetOutputLineEnding,
    ContainerEngine,
    ContainerPodContainer,
    BleInputLineEnding,
    BleOutputLineEnding,
    BleSendMode,
    BleDisplayMode,
    TerminalSemanticScheme,
    TerminalHighlightRuleSet,
    TerminalSessionLogPolicy,
    TerminalBellPolicy,
    X11ForwardingMode,
    X11UntrustedTimeout,
    PrimaryActionMenu,
}

impl NewConnectionSelect {
    pub fn trigger(
        self,
        tokens: &ThemeTokens,
        open: Option<Self>,
        origin: Option<BrowserFocusOrigin>,
        value: String,
        placeholder: bool,
        disabled: bool,
    ) -> Div {
        select_trigger_with_focus_visible(
            tokens,
            value,
            placeholder,
            disabled,
            browser_focus_visible(open == Some(self), origin),
        )
    }
}

pub fn select_anchor_id(select_id: NewConnectionSelect) -> SelectAnchorId {
    match select_id {
        NewConnectionSelect::Totp => SelectAnchorId::NewConnectionTotp,
        NewConnectionSelect::JumpTotp => SelectAnchorId::NewConnectionJumpTotp,
        NewConnectionSelect::Group => SelectAnchorId::NewConnectionGroup,
        NewConnectionSelect::TunnelHost => SelectAnchorId::NewConnectionTunnelHost,
        NewConnectionSelect::KeyAuthSource => SelectAnchorId::NewConnectionKeyAuthSource,
        NewConnectionSelect::ManagedKey => SelectAnchorId::NewConnectionManagedKey,
        NewConnectionSelect::StandaloneSftpSecondaryKeyAuthSource => {
            SelectAnchorId::NewConnectionStandaloneSftpSecondaryKeyAuthSource
        }
        NewConnectionSelect::StandaloneSftpSecondaryManagedKey => {
            SelectAnchorId::NewConnectionStandaloneSftpSecondaryManagedKey
        }
        NewConnectionSelect::JumpSavedConnection => {
            SelectAnchorId::NewConnectionJumpSavedConnection
        }
        NewConnectionSelect::RemoteDesktopSshGateway => {
            SelectAnchorId::NewConnectionRemoteDesktopSshGateway
        }
        NewConnectionSelect::JumpKeyAuthSource => SelectAnchorId::NewConnectionJumpKeyAuthSource,
        NewConnectionSelect::JumpManagedKey => SelectAnchorId::NewConnectionJumpManagedKey,
        NewConnectionSelect::UpstreamProxyPolicy => {
            SelectAnchorId::NewConnectionUpstreamProxyPolicy
        }
        NewConnectionSelect::UpstreamProxyProtocol => {
            SelectAnchorId::NewConnectionUpstreamProxyProtocol
        }
        NewConnectionSelect::UpstreamProxyAuth => SelectAnchorId::NewConnectionUpstreamProxyAuth,
        NewConnectionSelect::StandaloneSftpSecondaryUpstreamProxyPolicy => {
            SelectAnchorId::NewConnectionStandaloneSftpSecondaryUpstreamProxyPolicy
        }
        NewConnectionSelect::StandaloneSftpSecondaryUpstreamProxyProtocol => {
            SelectAnchorId::NewConnectionStandaloneSftpSecondaryUpstreamProxyProtocol
        }
        NewConnectionSelect::StandaloneSftpSecondaryUpstreamProxyAuth => {
            SelectAnchorId::NewConnectionStandaloneSftpSecondaryUpstreamProxyAuth
        }
        NewConnectionSelect::LocalShell => SelectAnchorId::NewConnectionLocalShell,
        NewConnectionSelect::SerialPort => SelectAnchorId::NewConnectionSerialPort,
        NewConnectionSelect::ContainerEngine => SelectAnchorId::NewConnectionContainerEngine,
        NewConnectionSelect::ContainerPodContainer => {
            SelectAnchorId::NewConnectionContainerPodContainer
        }
        NewConnectionSelect::BleAdapter => SelectAnchorId::NewConnectionBleAdapter,
        NewConnectionSelect::SerialDataBits => SelectAnchorId::NewConnectionSerialDataBits,
        NewConnectionSelect::SerialStopBits => SelectAnchorId::NewConnectionSerialStopBits,
        NewConnectionSelect::SerialParity => SelectAnchorId::NewConnectionSerialParity,
        NewConnectionSelect::SerialFlowControl => SelectAnchorId::NewConnectionSerialFlowControl,
        NewConnectionSelect::TerminalEncoding => SelectAnchorId::NewConnectionTerminalEncoding,
        NewConnectionSelect::TerminalBackspaceSequence => {
            SelectAnchorId::NewConnectionTerminalBackspaceSequence
        }
        NewConnectionSelect::TerminalDeleteSequence => {
            SelectAnchorId::NewConnectionTerminalDeleteSequence
        }
        NewConnectionSelect::TelnetInputLineEnding => {
            SelectAnchorId::NewConnectionTelnetInputLineEnding
        }
        NewConnectionSelect::TelnetOutputLineEnding => {
            SelectAnchorId::NewConnectionTelnetOutputLineEnding
        }
        NewConnectionSelect::BleInputLineEnding => SelectAnchorId::NewConnectionBleInputLineEnding,
        NewConnectionSelect::BleOutputLineEnding => {
            SelectAnchorId::NewConnectionBleOutputLineEnding
        }
        NewConnectionSelect::BleSendMode => SelectAnchorId::NewConnectionBleSendMode,
        NewConnectionSelect::BleDisplayMode => SelectAnchorId::NewConnectionBleDisplayMode,
        NewConnectionSelect::TerminalSemanticScheme => {
            SelectAnchorId::NewConnectionTerminalSemanticScheme
        }
        NewConnectionSelect::TerminalHighlightRuleSet => {
            SelectAnchorId::NewConnectionTerminalHighlightRuleSet
        }
        NewConnectionSelect::TerminalSessionLogPolicy => {
            SelectAnchorId::NewConnectionTerminalSessionLogPolicy
        }
        NewConnectionSelect::TerminalBellPolicy => SelectAnchorId::NewConnectionTerminalBellPolicy,
        NewConnectionSelect::X11ForwardingMode => SelectAnchorId::NewConnectionX11Mode,
        NewConnectionSelect::X11UntrustedTimeout => SelectAnchorId::NewConnectionX11Timeout,
        NewConnectionSelect::PrimaryActionMenu => SelectAnchorId::NewConnectionPrimaryAction,
    }
}
