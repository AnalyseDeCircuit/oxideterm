use super::*;
use crate::proxy_page::{NetworkProxyAuthMode, network_proxy_auth_label};
use oxideterm_settings::{
    SettingsUpstreamProxyAuth, UpdateChannel, UpdateProxyMode, UpdateProxyProtocol,
};

impl<S: CoreSelectOwner> CoreSelectMenu<'_, S> {
    pub(super) fn network_or_update_menu(
        &self,
        tab: SettingsTab,
        select: SettingsSelect,
        width: f32,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        let settings = self.settings;
        match (tab, select) {
            (SettingsTab::Network, SettingsSelect::NetworkProxyAuth) => {
                let mut popup = select_overlay_popup(self.tokens, width);
                let current = settings
                    .network
                    .upstream_proxy
                    .as_ref()
                    .map(|proxy| match proxy.auth {
                        SettingsUpstreamProxyAuth::None => NetworkProxyAuthMode::None,
                        SettingsUpstreamProxyAuth::Password { .. } => {
                            NetworkProxyAuthMode::Password
                        }
                    })
                    .unwrap_or(NetworkProxyAuthMode::None);
                for mode in [NetworkProxyAuthMode::None, NetworkProxyAuthMode::Password] {
                    popup = popup.child(select_option_action(
                        select_option(
                            self.tokens,
                            network_proxy_auth_label(mode, self.i18n),
                            mode == current,
                        ),
                        false,
                        false,
                        self.action(move |this, _, window, cx| {
                            this.send(SelectMenuRequest::Close, window, cx);
                            this.send(SelectMenuRequest::ProxyAuth(mode), window, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Help, SettingsSelect::UpdateChannel) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for channel in [UpdateChannel::Stable, UpdateChannel::Beta] {
                    let label = update_channel_label(channel, &self.i18n);
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            label,
                            channel == settings.general.update_channel,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.general.update_channel = channel
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Network, SettingsSelect::UpdateProxyMode) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = settings.general.update_proxy.mode;
                for mode in [
                    UpdateProxyMode::Direct,
                    UpdateProxyMode::Application,
                    UpdateProxyMode::System,
                    UpdateProxyMode::Custom,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            update_proxy_mode_label(mode, &self.i18n),
                            mode == current,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.general.update_proxy.mode = mode
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Network, SettingsSelect::UpdateProxyProtocol) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = settings.general.update_proxy.protocol;
                for protocol in [
                    UpdateProxyProtocol::Http,
                    UpdateProxyProtocol::Https,
                    UpdateProxyProtocol::Socks5,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            update_proxy_protocol_label(protocol, &self.i18n),
                            protocol == current,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.general.update_proxy.protocol = protocol
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Network, SettingsSelect::NetworkApplicationProxyMode) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = settings.network.application_proxy_mode;
                for mode in [
                    SettingsApplicationProxyMode::System,
                    SettingsApplicationProxyMode::Direct,
                    SettingsApplicationProxyMode::Shared,
                ] {
                    // A missing shared profile is shown but cannot become an
                    // active route, keeping the runtime policy fail-closed.
                    let shared_proxy_missing = mode == SettingsApplicationProxyMode::Shared
                        && settings.network.upstream_proxy.is_none();
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            network_application_proxy_mode_label(mode, &self.i18n),
                            mode == current,
                        ),
                        shared_proxy_missing,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            if shared_proxy_missing {
                                return;
                            }
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.network.application_proxy_mode = mode
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Network, SettingsSelect::NetworkProxyProtocol) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = settings
                    .network
                    .upstream_proxy
                    .as_ref()
                    .map(|proxy| proxy.protocol)
                    .unwrap_or(SettingsUpstreamProxyProtocol::Socks5);
                for protocol in [
                    SettingsUpstreamProxyProtocol::Socks5,
                    SettingsUpstreamProxyProtocol::HttpConnect,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            network_proxy_protocol_label(protocol, &self.i18n),
                            protocol == current,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    if let Some(proxy) = settings.network.upstream_proxy.as_mut() {
                                        proxy.protocol = protocol;
                                    }
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            _ => None,
        }
    }
}
