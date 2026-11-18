use super::*;

use gpui::{Div, Stateful};
use oxideterm_ai::{AI_PROVIDER_TEMPLATES, McpAuthHeaderMode, McpTransport};
use oxideterm_settings::{IdeAgentMode, PluginMarketplaceSort};
use oxideterm_settings_model::ai_provider_views;

pub struct ExternalSelectInputs<'a> {
    pub provider_type: &'a str,
    pub mcp_transport: Option<McpTransport>,
    pub mcp_auth: Option<McpAuthHeaderMode>,
}

#[derive(Clone, Copy)]
pub enum ExternalSelect {
    MarketplaceSort(PluginMarketplaceSort),
    ProviderType(&'static str),
    McpTransport(McpTransport),
    McpAuth(McpAuthHeaderMode),
}

pub fn plugin_marketplace_sort_label(sort: PluginMarketplaceSort) -> &'static str {
    match sort {
        PluginMarketplaceSort::Name => "plugin.marketplace_sort_name",
        PluginMarketplaceSort::RecentUpdates => "plugin.marketplace_sort_updated",
        PluginMarketplaceSort::NewestListings => "plugin.marketplace_sort_newest",
    }
}

impl SettingsSelectMenu<'_> {
    pub(super) fn external_popup(
        &self,
        select: SettingsSelect,
        width: f32,
    ) -> Option<Stateful<Div>> {
        let settings = self.settings;
        match select {
            SettingsSelect::PluginMarketplaceSort => Some(
                self.external_options(
                    select_overlay_popup(self.tokens, width),
                    [
                        PluginMarketplaceSort::Name,
                        PluginMarketplaceSort::RecentUpdates,
                        PluginMarketplaceSort::NewestListings,
                    ]
                    .into_iter()
                    .map(|sort| {
                        (
                            self.i18n.t(plugin_marketplace_sort_label(sort)),
                            sort == settings.plugin_marketplace_sort,
                            ExternalSelect::MarketplaceSort(sort),
                        )
                    }),
                ),
            ),
            SettingsSelect::IdeAgentMode => {
                let mut popup = select_overlay_popup(self.tokens, width);
                for mode in [
                    IdeAgentMode::Ask,
                    IdeAgentMode::Enabled,
                    IdeAgentMode::Disabled,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            self.tokens,
                            ide_agent_label(mode, self.i18n),
                            mode == settings.ide.agent_mode,
                        ),
                        false,
                        false,
                        self.action(move |actions, _, window, cx| {
                            actions.send(SelectMenuRequest::Close, window, cx);
                            actions.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.ide.agent_mode = mode
                                })),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            SettingsSelect::AiProviderTemplate => Some(self.external_options(
                select_overlay_popup(self.tokens, width.max(224.0)),
                AI_PROVIDER_TEMPLATES.iter().map(|template| {
                    (
                        self.i18n.t(template.label_key),
                        template.provider_type == self.external.provider_type,
                        ExternalSelect::ProviderType(template.provider_type),
                    )
                }),
            )),
            SettingsSelect::AiWebSearchProvider => {
                let current = settings.ai.web_search.provider_id.as_deref();
                let mut popup = select_panel_overlay_popup_with_max_height(
                    self.tokens,
                    width.max(224.0),
                    320.0,
                );
                let providers =
                    std::iter::once((None, self.i18n.t("settings_view.ai.web_provider_none")))
                        .chain(
                            ai_provider_views(settings)
                                .into_iter()
                                .filter(oxideterm_ai::web::official_deepseek_provider)
                                .map(|provider| (Some(provider.id), provider.name)),
                        );
                for (provider, label) in providers {
                    popup = popup.child(select_option_action(
                        select_option(self.tokens, label, current == provider.as_deref()),
                        false,
                        false,
                        self.action(move |actions, _, window, cx| {
                            actions.send(SelectMenuRequest::Close, window, cx);
                            let provider = provider.clone();
                            actions.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.ai.web_search.provider_id = provider
                                })),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            SettingsSelect::AiMcpTransport => Some(
                self.external_options(
                    select_overlay_popup(self.tokens, width.max(220.0)),
                    [
                        (McpTransport::Stdio, "stdio"),
                        (
                            McpTransport::StreamableHttp,
                            "Streamable HTTP (auto fallback)",
                        ),
                        (McpTransport::LegacySse, "Legacy SSE"),
                    ]
                    .into_iter()
                    .map(|(transport, label)| {
                        (
                            label.to_owned(),
                            transport == self.external.mcp_transport.unwrap_or(McpTransport::Stdio),
                            ExternalSelect::McpTransport(transport),
                        )
                    }),
                ),
            ),
            SettingsSelect::AiMcpAuthMode => Some(
                self.external_options(
                    select_overlay_popup(self.tokens, width.max(220.0)),
                    [
                        (
                            McpAuthHeaderMode::Bearer,
                            "settings_view.mcp.auth_header_mode_bearer",
                        ),
                        (
                            McpAuthHeaderMode::Raw,
                            "settings_view.mcp.auth_header_mode_raw",
                        ),
                        (
                            McpAuthHeaderMode::None,
                            "settings_view.mcp.auth_header_mode_none",
                        ),
                    ]
                    .into_iter()
                    .map(|(mode, key)| {
                        (
                            self.i18n.t(key),
                            mode == self.external.mcp_auth.unwrap_or(McpAuthHeaderMode::Bearer),
                            ExternalSelect::McpAuth(mode),
                        )
                    }),
                ),
            ),
            _ => None,
        }
    }

    fn external_options(
        &self,
        mut popup: Stateful<Div>,
        options: impl IntoIterator<Item = (String, bool, ExternalSelect)>,
    ) -> Stateful<Div> {
        for (label, selected, request) in options {
            popup = popup.child(select_option_action(
                select_option(self.tokens, label, selected),
                false,
                false,
                self.action(move |actions, _, window, cx| {
                    actions.send(SelectMenuRequest::Close, window, cx);
                    actions.send(SelectMenuRequest::External(request), window, cx);
                    cx.stop_propagation();
                }),
            ));
        }
        popup
    }
}
