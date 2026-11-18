use gpui::{AnyElement, App, Div, Entity, IntoElement, MouseButton, prelude::*};
use oxideterm_gpui_ui::form_field;
use oxideterm_gpui_ui::select::{
    BrowserFocusOrigin, SelectAnchorId, browser_focus_visible, select_anchor_probe,
    select_trigger_with_focus_visible,
};
use oxideterm_theme::ThemeTokens;

use super::{ConnectionsDispatch, ConnectionsViews, ConnectionsWindowRequest};
use crate::{ConnectionFormState, NewConnectionSelect, entity::ConnectionFlowEntity};

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
    }
}

impl ConnectionsViews {
    pub fn group_select(
        &self,
        tokens: &ThemeTokens,
        i18n: &oxideterm_i18n::I18n,
        label: String,
        group: &str,
        cx: &App,
    ) -> AnyElement {
        group_select(&self.flow, &self.dispatch, tokens, i18n, label, group, cx)
    }

    pub fn labelled_select(
        &self,
        tokens: &ThemeTokens,
        select: NewConnectionSelect,
        label: String,
        value: String,
        placeholder: bool,
        disabled: bool,
        cx: &App,
    ) -> AnyElement {
        labelled_select(
            &self.flow,
            &self.dispatch,
            tokens,
            select,
            label,
            value,
            placeholder,
            disabled,
            cx,
        )
    }

    pub fn select_trigger(
        &self,
        tokens: &ThemeTokens,
        select: NewConnectionSelect,
        value: String,
        placeholder: bool,
        disabled: bool,
        cx: &App,
    ) -> Div {
        select_trigger(
            &self.flow.read(cx).form,
            tokens,
            select,
            value,
            placeholder,
            disabled,
        )
    }

    pub fn track_select_anchor(
        &self,
        select: NewConnectionSelect,
        trigger: impl IntoElement,
        cx: &App,
    ) -> AnyElement {
        track_select_anchor(&self.flow, select, trigger, cx)
    }
}

pub(super) fn group_select(
    flow: &Entity<ConnectionFlowEntity>,
    dispatch: &ConnectionsDispatch,
    tokens: &ThemeTokens,
    i18n: &oxideterm_i18n::I18n,
    label: String,
    group: &str,
    cx: &App,
) -> AnyElement {
    let value = if crate::conversion::connection_group_is_ungrouped(group, i18n) {
        i18n.t("ssh.form.ungrouped")
    } else {
        group.trim().to_owned()
    };
    labelled_select(
        flow,
        dispatch,
        tokens,
        NewConnectionSelect::Group,
        label,
        value,
        false,
        false,
        cx,
    )
}

pub(in crate::views) fn labelled_select(
    flow: &Entity<ConnectionFlowEntity>,
    dispatch: &ConnectionsDispatch,
    tokens: &ThemeTokens,
    select: NewConnectionSelect,
    label: String,
    value: String,
    placeholder: bool,
    disabled: bool,
    cx: &App,
) -> AnyElement {
    let dispatch = dispatch.clone();
    let owner = flow.downgrade();
    let trigger = select_trigger(
        &flow.read(cx).form,
        tokens,
        select,
        value,
        placeholder,
        disabled,
    )
    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
        if !disabled {
            let _ = owner.update(cx, |flow, cx| {
                if let Some(form) = flow.form.form.as_mut() {
                    form.field_focused = false;
                    form.selected_field = None;
                }
                if flow.form.open_select == Some(select) {
                    flow.form.close_select();
                } else {
                    flow.form.open_select = Some(select);
                    flow.form.select_focus_origin = Some(BrowserFocusOrigin::Pointer);
                }
                cx.notify();
            });
            dispatch(ConnectionsWindowRequest::FocusFormSelect, Some(window), cx);
        }
        cx.stop_propagation();
    });
    form_field(
        tokens,
        label,
        track_select_anchor(flow, select, trigger, cx),
    )
}

pub(super) fn select_trigger(
    state: &ConnectionFormState,
    tokens: &ThemeTokens,
    select: NewConnectionSelect,
    value: String,
    placeholder: bool,
    disabled: bool,
) -> Div {
    select_trigger_with_focus_visible(
        tokens,
        value,
        placeholder,
        disabled,
        browser_focus_visible(state.open_select == Some(select), state.select_focus_origin),
    )
}

pub(super) fn track_select_anchor(
    flow_entity: &gpui::Entity<ConnectionFlowEntity>,
    select: NewConnectionSelect,
    trigger: impl IntoElement,
    cx: &App,
) -> AnyElement {
    let flow = flow_entity.read(cx);
    let anchors = flow.select_anchor_store();
    let notify_on_change = flow.form.open_select == Some(select);
    let entity = flow_entity.downgrade();
    select_anchor_probe(select_anchor_id(select), trigger, move |anchor, _, cx| {
        // An open menu needs a repaint after scrolling changes its anchor.
        if anchors.update(anchor) && notify_on_change {
            let _ = entity.update(cx, |_, cx| cx.notify());
        }
    })
    .into_any_element()
}
