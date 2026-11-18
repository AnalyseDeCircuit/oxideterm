use std::{ops::Deref, rc::Rc};

use gpui::{
    AnyElement, App, Div, Entity, MouseButton, MouseDownEvent, Window, div, prelude::*, px, rgb,
};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_button,
    },
    switch::{SwitchOptions, switch_with},
    text_input::TextInputContentAlign,
};
use oxideterm_i18n::I18n;
use oxideterm_settings::{
    SettingsUpstreamProxyAuth, SettingsUpstreamProxyConfig, SettingsUpstreamProxyProtocol,
    UpdateProxyMode,
};

use crate::{
    components::SettingsControls, entity::SettingsWorkspaceEntity, surface::nav::render_icon, *,
};

const SETTINGS_NETWORK_FIELD_WIDTH: f32 = 320.0; // Desktop preference for normal proxy fields.
const SETTINGS_NETWORK_PORT_FIELD_WIDTH: f32 = 140.0; // Ports should stay compact instead of sharing a full row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkProxyAuthMode {
    None,
    Password,
}

pub fn default_settings_upstream_proxy_config() -> SettingsUpstreamProxyConfig {
    SettingsUpstreamProxyConfig {
        protocol: SettingsUpstreamProxyProtocol::Socks5,
        host: "127.0.0.1".to_string(),
        port: 1080,
        auth: SettingsUpstreamProxyAuth::None,
        remote_dns: true,
        no_proxy: String::new(),
    }
}

pub fn network_proxy_auth_label(mode: NetworkProxyAuthMode, i18n: &I18n) -> String {
    match mode {
        NetworkProxyAuthMode::None => i18n.t("settings_view.network.auth_none"),
        NetworkProxyAuthMode::Password => i18n.t("settings_view.network.auth_password"),
    }
}

use crate::select_menu::{network_application_proxy_mode_label, network_proxy_protocol_label};

#[derive(Clone, Copy)]
pub enum ProxyRequest {
    Disclaimer,
    Enabled,
    RemoteDns,
    Test,
    SavePassword,
    RemovePassword,
}

pub struct ProxyPage<'a> {
    pub controls: SettingsControls<'a>,
    pub state: &'a Entity<SettingsWorkspaceEntity>,
    pub dispatch: Rc<dyn Fn(ProxyRequest, &mut App)>,
}

impl<'a> Deref for ProxyPage<'a> {
    type Target = SettingsControls<'a>;

    fn deref(&self) -> &Self::Target {
        &self.controls
    }
}

impl ProxyPage<'_> {
    fn press(
        &self,
        request: ProxyRequest,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let dispatch = self.dispatch.clone();
        move |_, _, cx| {
            dispatch(request, cx);
            cx.stop_propagation();
        }
    }

    fn secret_field(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        cx: &App,
    ) -> AnyElement {
        (self.field)(
            input,
            &value,
            placeholder,
            None,
            TextInputContentAlign::Start,
            true,
            cx,
        )
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let enabled = !(options.button.disabled || options.loading);
        toolbar_button(self.tokens, label, icon, options).on_mouse_down(
            MouseButton::Left,
            move |event, window, cx| {
                if enabled {
                    press(event, window, cx);
                } else {
                    cx.stop_propagation();
                }
            },
        )
    }
    pub fn shared_proxy(
        &self,
        proxy: Option<&SettingsUpstreamProxyConfig>,
        cx: &App,
    ) -> AnyElement {
        let Some(proxy) = proxy else {
            let disclaimer_accepted = self.settings.network.upstream_proxy_disclaimer_accepted;
            let empty_state = div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(self.tokens.metrics.settings_card_gap))
                .child(self.network_checkbox_row(
                    "settings_view.network.disclaimer",
                    "settings_view.network.disclaimer_hint",
                    disclaimer_accepted,
                    true,
                    ProxyRequest::Disclaimer,
                    cx,
                ))
                .child(
                    div()
                        .flex_none()
                        .child(self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.network.add_shared_proxy"),
                            None,
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Default,
                                    size: ButtonSize::Default,
                                    radius: ButtonRadius::Md,
                                    disabled: !disclaimer_accepted,
                                },
                                ..ToolbarButtonOptions::default()
                            },
                            self.press(ProxyRequest::Enabled),
                        )),
                )
                .into_any_element();
            return self.settings_card(
                "settings_view.network.shared_proxy",
                "settings_view.network.shared_proxy_empty_hint",
                vec![empty_state],
            );
        };

        let content =
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(self.tokens.metrics.settings_card_gap))
                .child(
                    div()
                        .w_full()
                        .min_w(px(0.0))
                        .flex()
                        .flex_wrap()
                        .items_start()
                        .gap(px(16.0))
                        .child(self.network_responsive_field(
                            SETTINGS_NETWORK_FIELD_WIDTH,
                            self.network_select_field(
                                "settings_view.network.protocol",
                                "settings_view.network.protocol_hint",
                                SettingsSelect::NetworkProxyProtocol,
                                network_proxy_protocol_label(proxy.protocol, &self.i18n),
                                true,
                                cx,
                            ),
                        ))
                        .child(self.network_compact_field(
                            SETTINGS_NETWORK_PORT_FIELD_WIDTH,
                            self.network_input_field(
                                "settings_view.network.port",
                                "settings_view.network.port_hint",
                                SettingsInput::NetworkProxyPort,
                                proxy.port.to_string(),
                                "1080".to_string(),
                                true,
                                cx,
                            ),
                        )),
                )
                .child(self.network_full_width_input(
                    "settings_view.network.host",
                    "settings_view.network.host_hint",
                    SettingsInput::NetworkProxyHost,
                    proxy.host.clone(),
                    "127.0.0.1".to_string(),
                    true,
                    cx,
                ))
                .child(self.network_full_width_input(
                    "settings_view.network.no_proxy",
                    "settings_view.network.no_proxy_hint",
                    SettingsInput::NetworkProxyNoProxy,
                    proxy.no_proxy.clone(),
                    "localhost,127.0.0.1,*.internal".to_string(),
                    true,
                    cx,
                ))
                .child(self.network_checkbox_row(
                    "settings_view.network.remote_dns",
                    "settings_view.network.remote_dns_hint",
                    proxy.remote_dns,
                    true,
                    ProxyRequest::RemoteDns,
                    cx,
                ))
                .child(self.card_separator())
                .child(self.settings_network_auth_content(Some(proxy), cx))
                .child(self.card_separator())
                .child(self.settings_network_test_content(true, cx))
                .child(self.card_separator())
                .child(div().w_full().flex().justify_end().child(
                    self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.network.remove_shared_proxy"),
                        Some(render_icon(
                            "lucide/trash-2.svg",
                            16.0,
                            rgb(self.tokens.ui.error),
                        )),
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Destructive,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        self.press(ProxyRequest::Enabled),
                    ),
                ))
                .into_any_element();

        self.settings_card(
            "settings_view.network.shared_proxy",
            "settings_view.network.shared_proxy_hint",
            vec![content],
        )
    }

    fn settings_network_auth_content(
        &self,
        proxy: Option<&SettingsUpstreamProxyConfig>,
        cx: &App,
    ) -> AnyElement {
        let auth_mode = proxy
            .map(|proxy| match &proxy.auth {
                SettingsUpstreamProxyAuth::None => NetworkProxyAuthMode::None,
                SettingsUpstreamProxyAuth::Password { .. } => NetworkProxyAuthMode::Password,
            })
            .unwrap_or(NetworkProxyAuthMode::None);
        let auth_username = proxy
            .and_then(|proxy| match &proxy.auth {
                SettingsUpstreamProxyAuth::Password { username, .. } => Some(username.clone()),
                SettingsUpstreamProxyAuth::None => None,
            })
            .unwrap_or_default();
        let auth_has_saved_password = proxy.is_some_and(|proxy| match &proxy.auth {
            SettingsUpstreamProxyAuth::Password { keychain_id, .. } => keychain_id.is_some(),
            SettingsUpstreamProxyAuth::None => false,
        });

        let mut section = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_row_gap))
            .opacity(if proxy.is_some() { 1.0 } else { 0.4 })
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .gap(px(16.0))
                    .child(self.network_responsive_field(
                        SETTINGS_NETWORK_FIELD_WIDTH,
                        self.network_select_field(
                            "settings_view.network.auth",
                            "settings_view.network.auth_hint",
                            SettingsSelect::NetworkProxyAuth,
                            network_proxy_auth_label(auth_mode, &self.i18n),
                            proxy.is_some(),
                            cx,
                        ),
                    )),
            );

        if auth_mode == NetworkProxyAuthMode::Password {
            section = section
                .child(self.network_full_width_input(
                    "settings_view.network.username",
                    "settings_view.network.username_hint",
                    SettingsInput::NetworkProxyUsername,
                    auth_username,
                    String::new(),
                    proxy.is_some(),
                    cx,
                ))
                .child(self.network_password_field(auth_has_saved_password, proxy.is_some(), cx));
        }

        section.into_any_element()
    }

    fn settings_network_test_content(&self, proxy_enabled: bool, cx: &App) -> AnyElement {
        let snapshot = self.state.read(cx).network_proxy_test_snapshot();
        let host_value = snapshot.test_host;
        let port_value = snapshot.test_port;
        let test_disabled = !proxy_enabled
            || snapshot.test_pending
            || host_value.trim().is_empty()
            || port_value.trim().parse::<u16>().is_err();
        let status = snapshot.test_result.map(|result| match result {
            Ok(elapsed) => self
                .i18n
                .t("settings_view.network.test_success")
                .replace("{{elapsed}}", &elapsed.to_string()),
            Err(error) => self
                .i18n
                .t("settings_view.network.test_error")
                .replace("{{error}}", &error),
        });

        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .opacity(if proxy_enabled { 1.0 } else { 0.4 })
            .child(self.network_subsection_heading(
                "settings_view.network.test_title",
                "settings_view.network.test_hint",
            ))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(16.0))
                    .child(self.network_responsive_field(
                        SETTINGS_NETWORK_FIELD_WIDTH,
                        self.network_input_field(
                            "settings_view.network.test_host",
                            "settings_view.network.test_host_hint",
                            SettingsInput::NetworkProxyTestHost,
                            host_value,
                            "server.example.com".to_string(),
                            proxy_enabled,
                            cx,
                        ),
                    ))
                    .child(self.network_compact_field(
                        SETTINGS_NETWORK_PORT_FIELD_WIDTH,
                        self.network_input_field(
                            "settings_view.network.test_port",
                            "settings_view.network.test_port_hint",
                            SettingsInput::NetworkProxyTestPort,
                            port_value,
                            "22".to_string(),
                            proxy_enabled,
                            cx,
                        ),
                    )),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_none()
                            .child(self.workspace_toolbar_action_button(
                                if snapshot.test_pending {
                                    self.i18n.t("settings_view.network.testing")
                                } else {
                                    self.i18n.t("settings_view.network.test_button")
                                },
                                None,
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Default,
                                        radius: ButtonRadius::Md,
                                        disabled: test_disabled,
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                self.press(ProxyRequest::Test),
                            )),
                    )
                    .when_some(status, |row, status| {
                        row.child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(status),
                        )
                    }),
            )
            .into_any_element()
    }

    pub fn routing(&self, cx: &App) -> AnyElement {
        // Routing policy is distinct from the reusable proxy definition above.
        // SSH remains connection-owned while app and updater routes are global.
        let settings = self.settings;
        let application_mode = settings.network.application_proxy_mode;
        let update_proxy = &settings.general.update_proxy;
        let mut content = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .child(self.network_static_route_row(
                "settings_view.network.ssh_route",
                "settings_view.network.ssh_route_hint",
                "settings_view.network.per_connection",
            ))
            .child(self.card_separator())
            .child(self.network_route_select_row(
                "settings_view.network.application_route",
                "settings_view.network.application_route_hint",
                SettingsSelect::NetworkApplicationProxyMode,
                network_application_proxy_mode_label(application_mode, &self.i18n),
                cx,
            ))
            .child(self.card_separator())
            .child(self.network_route_select_row(
                "settings_view.network.update_route",
                "settings_view.network.update_route_hint",
                SettingsSelect::UpdateProxyMode,
                update_proxy_mode_label(update_proxy.mode, &self.i18n),
                cx,
            ));

        if update_proxy.mode == UpdateProxyMode::Custom {
            content = content.child(self.settings_network_custom_update_proxy(cx));
        }

        content = content.child(
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("settings_view.network.legal_hint")),
        );

        self.settings_card(
            "settings_view.network.routing",
            "settings_view.network.routing_hint",
            vec![content.into_any_element()],
        )
    }

    fn settings_network_custom_update_proxy(&self, cx: &App) -> AnyElement {
        let proxy = &self.settings.general.update_proxy;
        div()
            .w_full()
            .min_w(px(0.0))
            .pt(px(self.tokens.metrics.settings_row_gap))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_row_gap))
            .child(self.network_subsection_heading(
                "settings_view.network.custom_update_proxy",
                "settings_view.network.custom_update_proxy_hint",
            ))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .gap(px(16.0))
                    .child(self.network_responsive_field(
                        SETTINGS_NETWORK_FIELD_WIDTH,
                        self.network_select_field(
                            "settings_view.network.update_protocol",
                            "settings_view.network.update_protocol_hint",
                            SettingsSelect::UpdateProxyProtocol,
                            update_proxy_protocol_label(proxy.protocol, &self.i18n),
                            true,
                            cx,
                        ),
                    ))
                    .child(self.network_compact_field(
                        SETTINGS_NETWORK_PORT_FIELD_WIDTH,
                        self.network_input_field(
                            "settings_view.network.update_port",
                            "settings_view.network.update_port_hint",
                            SettingsInput::UpdateProxyPort,
                            proxy.port.to_string(),
                            "7890".to_string(),
                            true,
                            cx,
                        ),
                    )),
            )
            .child(self.network_full_width_input(
                "settings_view.network.update_host",
                "settings_view.network.update_host_hint",
                SettingsInput::UpdateProxyHost,
                proxy.host.clone(),
                "127.0.0.1".to_string(),
                true,
                cx,
            ))
            .child(self.network_full_width_input(
                "settings_view.network.update_no_proxy",
                "settings_view.network.update_no_proxy_hint",
                SettingsInput::UpdateProxyNoProxy,
                proxy.no_proxy.clone(),
                "localhost,127.0.0.1".to_string(),
                true,
                cx,
            ))
            .into_any_element()
    }

    fn network_subsection_heading(&self, label_key: &str, hint_key: &str) -> AnyElement {
        self.card_subsection_header(
            self.i18n.t(label_key),
            (!hint_key.is_empty()).then(|| self.i18n.t(hint_key)),
        )
    }

    fn network_static_route_row(
        &self,
        label_key: &str,
        hint_key: &str,
        value_key: &str,
    ) -> AnyElement {
        settings_appearance_row(
            &self.tokens,
            &self.i18n,
            label_key,
            hint_key,
            div()
                .max_w_full()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t(value_key))
                .into_any_element(),
        )
    }

    fn network_route_select_row(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        value: String,
        cx: &App,
    ) -> AnyElement {
        self.select_setting_row(
            label_key,
            hint_key,
            select_id,
            value,
            self.tokens.metrics.settings_select_width,
            cx,
        )
    }

    fn network_responsive_field(&self, preferred_width: f32, field: AnyElement) -> AnyElement {
        // Field slots grow with the card and wrap once their preferred width
        // no longer fits, avoiding a fixed-width cluster on wide panes.
        div()
            .max_w_full()
            .min_w(px(0.0))
            .flex_1()
            .flex_basis(px(preferred_width))
            .child(field)
            .into_any_element()
    }

    fn network_compact_field(&self, preferred_width: f32, field: AnyElement) -> AnyElement {
        // Compact numeric fields keep their natural width while larger peers
        // consume the remaining row; max-width still permits narrow panes.
        div()
            .w(px(preferred_width))
            .max_w_full()
            .min_w(px(0.0))
            .flex_initial()
            .child(field)
            .into_any_element()
    }

    fn network_checkbox_row(
        &self,
        label_key: &'static str,
        hint_key: &'static str,
        checked: bool,
        enabled: bool,
        on_toggle: ProxyRequest,
        cx: &App,
    ) -> AnyElement {
        let mut control = switch_with(
            &self.tokens,
            label_key,
            String::new(),
            checked,
            SwitchOptions::default().disabled(!enabled),
        );
        if enabled {
            control = control.on_mouse_down(MouseButton::Left, self.press(on_toggle));
        }
        self.setting_row(label_key, hint_key, control.into_any_element(), cx)
    }

    fn network_select_field(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        value: String,
        enabled: bool,
        cx: &App,
    ) -> AnyElement {
        div()
            .w_full()
            .max_w_full()
            .min_w(px(0.0))
            .grid()
            .gap(px(8.0))
            .child(self.network_field_label(label_key, hint_key))
            .child(self.settings_select_control(select_id, value, !enabled, None, cx))
            .into_any_element()
    }

    fn network_input_field(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: String,
        placeholder: String,
        enabled: bool,
        cx: &App,
    ) -> AnyElement {
        div()
            .w_full()
            .max_w_full()
            .min_w(px(0.0))
            .grid()
            .gap(px(8.0))
            .child(self.network_field_label(label_key, hint_key))
            .child(
                self.settings_text_input_control_fill(input, value, placeholder, cx)
                    .into_any_element(),
            )
            .when(!enabled, |field| field.opacity(0.5))
            .into_any_element()
    }

    fn network_full_width_input(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: String,
        placeholder: String,
        enabled: bool,
        cx: &App,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .grid()
            .gap(px(8.0))
            .child(self.network_field_label(label_key, hint_key))
            .child(self.settings_text_input_control_fill(input, value, placeholder, cx))
            .when(!enabled, |field| field.opacity(0.5))
            .into_any_element()
    }

    fn network_password_field(
        &self,
        has_saved_password: bool,
        enabled: bool,
        cx: &App,
    ) -> AnyElement {
        let password_input = SettingsInput::NetworkProxyPassword;
        let snapshot = self.state.read(cx).network_proxy_password_snapshot();
        let save_disabled = !snapshot.password_present || !enabled;
        let remove_disabled = !has_saved_password && !snapshot.password_present;
        let mut row = div()
            .w_full()
            .min_w(px(0.0))
            .grid()
            .gap(px(8.0))
            .child(self.network_field_label(
                "settings_view.network.password",
                if has_saved_password {
                    "settings_view.network.password_saved_hint"
                } else {
                    "settings_view.network.password_hint"
                },
            ))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex_basis(px(SETTINGS_NETWORK_FIELD_WIDTH))
                            .child(self.secret_field(
                                password_input,
                                String::new(),
                                if has_saved_password {
                                    self.i18n
                                        .t("settings_view.network.password_saved_placeholder")
                                } else {
                                    String::new()
                                },
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_none()
                            .child(self.workspace_toolbar_action_button(
                                self.i18n.t("settings_view.network.save_password"),
                                Some(render_icon(
                                    "lucide/key-round.svg",
                                    16.0,
                                    rgb(if save_disabled {
                                        self.tokens.ui.text_muted
                                    } else {
                                        self.tokens.ui.bg
                                    }),
                                )),
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Default,
                                        radius: ButtonRadius::Md,
                                        disabled: save_disabled,
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                self.press(ProxyRequest::SavePassword),
                            )),
                    )
                    .child(
                        div()
                            .flex_none()
                            .child(self.workspace_toolbar_action_button(
                                self.i18n.t("settings_view.network.remove_password"),
                                Some(render_icon(
                                    "lucide/trash-2.svg",
                                    16.0,
                                    rgb(self.tokens.ui.text),
                                )),
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Ghost,
                                        size: ButtonSize::Default,
                                        radius: ButtonRadius::Md,
                                        disabled: remove_disabled,
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                self.press(ProxyRequest::RemovePassword),
                            )),
                    ),
            )
            .when(!enabled, |field| field.opacity(0.5));

        if let Some(status) = snapshot.password_status {
            row = row.child(
                div()
                    .min_w(px(0.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(status),
            );
        }

        row.into_any_element()
    }

    fn network_field_label(&self, label_key: &str, hint_key: &str) -> AnyElement {
        div()
            .min_w(px(0.0))
            .grid()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(hint_key)),
            )
            .into_any_element()
    }
}
