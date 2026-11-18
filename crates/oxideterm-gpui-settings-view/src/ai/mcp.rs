use gpui::{
    AnyElement, App, Context, Div, ListAlignment, ListState, MouseButton, MouseDownEvent, Render,
    Rgba, Window, div, prelude::*, px, rgb, rgba, svg,
};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions, icon_button, toolbar_button,
    },
    icons::LucideIcon,
    motion::animated_spinner,
    virtual_list::{
        TauriVirtualListSpec, VirtualListSignatureCache,
        sync_tauri_variable_list_state_by_signatures, tauri_virtual_list,
    },
};
use oxideterm_i18n::I18n;
use oxideterm_settings::PersistedSettings;
use oxideterm_settings_model::{ai_mcp_configs, ai_mcp_server_signature, ai_mcp_transport_label};
use oxideterm_theme::ThemeTokens;
use std::{cell::RefCell, collections::HashSet, rc::Rc};

use crate::components::{SettingsComponents, SettingsControlsSource};

const AI_MCP_SERVER_LIST_ESTIMATED_HEIGHT: f32 = 156.0;
const AI_MCP_SERVER_LIST_OVERSCAN: usize = 4;
const AI_MCP_PANEL_BORDER_ALPHA: u32 = 0x66; // Tauri border-theme-border/40.
const AI_MCP_PANEL_BG_ALPHA: u32 = 0x4d; // Tauri bg-theme-bg-panel/30.
const AI_MCP_CODE_BG_ALPHA: u32 = 0x99; // Tauri bg-theme-bg-panel/60.
const AI_MCP_TOOL_BORDER_ALPHA: u32 = 0x33; // Tauri border-theme-border/20.
const AI_MCP_CARD_ACTION_H: f32 = 28.0; // Tauri MCP card actions h-7.
const AI_MCP_CARD_ACTION_PX: f32 = 8.0; // Tauri px-2.
const AI_MCP_CARD_ICON_BUTTON: f32 = 28.0; // Tauri h-7 w-7 p-0.
const AI_MCP_ACTION_ICON: f32 = 14.0; // Tauri w-3.5 h-3.5.
const AI_MCP_STATUS_ICON: f32 = 12.0; // Tauri status icons w-3 h-3.
const AI_MCP_VISIBLE_TOOL_CHIPS: usize = 8; // Keep virtualized card height bounded near the list row estimate.

pub enum AiMcpRequest {
    Open,
    FinishDialog { submit: bool },
    RefreshTools(String),
    Remove(String),
    SetConnected { id: String, was_connected: bool },
    Tick,
}

pub type AiMcpDispatcher = Rc<dyn Fn(AiMcpRequest, &mut Window, &mut App)>;

pub struct AiMcpBindings {
    pub controls: SettingsControlsSource,
    pub snapshots: Rc<dyn Fn(&App) -> Vec<oxideterm_ai::McpServerStateSnapshot>>,
    pub request: AiMcpDispatcher,
}

pub struct AiMcpSettingsView {
    bindings: AiMcpBindings,
    tokens: ThemeTokens,
    i18n: I18n,
    list: ListState,
    cache: RefCell<VirtualListSignatureCache>,
}

impl AiMcpSettingsView {
    pub fn new(bindings: AiMcpBindings) -> Self {
        Self {
            bindings,
            tokens: oxideterm_theme::default_tokens(),
            i18n: I18n::default(),
            list: ListState::new(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(AI_MCP_SERVER_LIST_ESTIMATED_HEIGHT),
                    AI_MCP_SERVER_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            cache: Default::default(),
        }
    }

    fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    fn render_loading_icon(
        &self,
        id: impl Into<gpui::ElementId>,
        size: f32,
        color: Rgba,
    ) -> AnyElement {
        animated_spinner(
            &self.tokens,
            id,
            svg()
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
        action: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let enabled = !(options.button.disabled || options.loading);
        toolbar_button(&self.tokens, label, icon, options).on_mouse_down(
            MouseButton::Left,
            move |event, window, cx| {
                if enabled {
                    action(event, window, cx);
                } else {
                    cx.stop_propagation();
                }
            },
        )
    }

    fn workspace_icon_action_button(
        &self,
        icon: LucideIcon,
        size: f32,
        color: Rgba,
        options: IconButtonOptions,
        action: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &Context<Self>,
    ) -> Div {
        let enabled = !(options.disabled || options.loading);
        icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, size, color),
            options,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event, window, cx| {
                if enabled {
                    action(this, event, window, cx);
                } else {
                    cx.stop_propagation();
                }
            }),
        )
    }

    fn render_selectable_display_text(
        &self,
        scope: &str,
        key: &str,
        value: String,
        color: u32,
        cx: &App,
    ) -> AnyElement {
        match (self.bindings.controls)(cx) {
            Some(controls) => (controls.label)(scope, key, value, color, cx),
            None => div().child(value).into_any_element(),
        }
    }
    fn ai_mcp_servers_section(
        &self,
        configs: Vec<oxideterm_ai::McpServerConfig>,
        header: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let snapshots = (self.bindings.snapshots)(cx);
        let configured_server_ids: HashSet<_> =
            configs.iter().map(|config| config.id.as_str()).collect();
        // Only live configured MCP rows should drive the retry/status ticker.
        // Stale registry snapshots can otherwise keep the AI settings page
        // repainting even when the MCP section is visually empty.
        if snapshots.iter().any(|snapshot| {
            configured_server_ids.contains(snapshot.config.id.as_str())
                && (snapshot.status == "connecting"
                    || (snapshot.status == "error" && snapshot.config.retry_on_disconnect))
        }) {
            (self.bindings.request)(AiMcpRequest::Tick, window, cx);
        }

        let list = if configs.is_empty() {
            div().flex().flex_col().gap(px(12.0)).child(
                div()
                    .border_1()
                    .border_color(rgba(
                        (self.tokens.ui.border << 8) | AI_MCP_PANEL_BORDER_ALPHA,
                    ))
                    .rounded(px(self.tokens.radii.lg))
                    .py(px(32.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.mcp.no_servers")),
            )
        } else {
            self.sync_ai_mcp_server_list_state(&configs, &snapshots, cx);
            let state = self.list.clone();
            let spec = self.ai_mcp_server_list_spec();
            let workspace = cx.entity();
            div()
                .h(px(
                    configs.len() as f32 * AI_MCP_SERVER_LIST_ESTIMATED_HEIGHT
                ))
                .child(tauri_virtual_list(
                    state,
                    spec,
                    move |index, _window, cx| {
                        let Some(config) = configs.get(index) else {
                            return div().into_any_element();
                        };
                        workspace.update(cx, |this, cx| {
                            let snapshot = snapshots
                                .iter()
                                .find(|snapshot| snapshot.config.id == config.id);
                            div()
                                .pb(px(12.0))
                                .child(this.ai_mcp_server_card(config, snapshot, cx))
                                .into_any_element()
                        })
                    },
                ))
        };

        super::section(&self.tokens, header, [list.into_any_element()])
    }

    fn header(&self, settings: &PersistedSettings, cx: &Context<Self>) -> AnyElement {
        SettingsComponents {
            tokens: &self.tokens,
            i18n: &self.i18n,
            settings,
            has_background: false,
        }
        .settings_card_header_text(
            self.i18n.t("settings_view.mcp.title"),
            Some(self.i18n.t("settings_view.mcp.description")),
            None,
            Some(
                self.workspace_toolbar_action_button(
                    self.i18n.t("settings_view.mcp.add_server"),
                    Some(Self::render_lucide_icon(
                        LucideIcon::Plus,
                        14.0,
                        rgb(self.tokens.ui.bg),
                    )),
                    ToolbarButtonOptions {
                        button: ButtonOptions {
                            variant: ButtonVariant::Default,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: false,
                        },
                        icon_gap: Some(6.0),
                        ..ToolbarButtonOptions::default()
                    },
                    cx.listener(|this, _event, window, cx| {
                        (this.bindings.request)(AiMcpRequest::Open, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
            ),
        )
    }

    fn sync_ai_mcp_server_list_state(
        &self,
        configs: &[oxideterm_ai::McpServerConfig],
        snapshots: &[oxideterm_ai::McpServerStateSnapshot],
        _cx: &App,
    ) {
        let signatures = configs
            .iter()
            .map(|config| {
                ai_mcp_server_signature(
                    config,
                    snapshots
                        .iter()
                        .find(|snapshot| snapshot.config.id == config.id),
                )
            })
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.list,
            &mut self.cache.borrow_mut(),
            "ai-mcp-servers",
            &signatures,
            self.ai_mcp_server_list_spec(),
        );
    }

    fn ai_mcp_server_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(AI_MCP_SERVER_LIST_ESTIMATED_HEIGHT),
            AI_MCP_SERVER_LIST_OVERSCAN,
        )
    }

    fn ai_mcp_server_card(
        &self,
        config: &oxideterm_ai::McpServerConfig,
        snapshot: Option<&oxideterm_ai::McpServerStateSnapshot>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let status = snapshot
            .map(|snapshot| snapshot.status)
            .unwrap_or("disconnected");
        let tools = snapshot
            .map(|snapshot| snapshot.tools.as_slice())
            .unwrap_or_default();
        let endpoint = snapshot
            .and_then(|snapshot| snapshot.endpoint_url.as_deref())
            .or(config.url.as_deref())
            .unwrap_or_default()
            .to_string();
        let command = if config.transport == oxideterm_ai::McpTransport::Stdio {
            Some(
                std::iter::once(config.command.clone().unwrap_or_default())
                    .chain(config.args.clone())
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        } else {
            None
        };
        let toggle_id = config.id.clone();
        let remove_id = config.id.clone();
        let refresh_id = config.id.clone();

        let mut card = div()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba(
                (self.tokens.ui.border << 8) | AI_MCP_PANEL_BORDER_ALPHA,
            ))
            .bg(rgba((self.tokens.ui.bg_panel << 8) | AI_MCP_PANEL_BG_ALPHA))
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(config.name.clone()),
                            )
                            .child(self.ai_mcp_status_badge(status))
                            .child(self.ai_mcp_transport_badge(config.transport))
                            .when(
                                snapshot.is_some_and(|snapshot| {
                                    snapshot.resolved_transport.as_deref() == Some("legacy-sse")
                                        && config.transport != oxideterm_ai::McpTransport::LegacySse
                                }),
                                |row| {
                                    row.child(
                                        div()
                                            .px(px(6.0))
                                            .py(px(2.0))
                                            .rounded(px(self.tokens.radii.sm))
                                            .bg(rgba((self.tokens.ui.warning << 8) | 0x1a))
                                            .text_size(px(self.tokens.metrics.ui_text_2xs))
                                            .text_color(rgb(self.tokens.ui.warning))
                                            .child(
                                                self.i18n
                                                    .t("settings_view.mcp.fallback_legacy_sse"),
                                            ),
                                    )
                                },
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .when(status == "connected", |row| {
                                row.child(self.ai_mcp_card_icon_button(
                                    LucideIcon::RefreshCw,
                                    rgb(self.tokens.ui.text_muted),
                                    false,
                                    move |this, _event, window, cx| {
                                        (this.bindings.request)(
                                            AiMcpRequest::RefreshTools(refresh_id.clone()),
                                            window,
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    },
                                    cx,
                                ))
                            })
                            .child(self.ai_mcp_toggle_button(status, toggle_id, cx))
                            .child(self.ai_mcp_card_icon_button(
                                LucideIcon::Trash2,
                                rgb(self.tokens.ui.error),
                                false,
                                move |this, _event, window, cx| {
                                    (this.bindings.request)(
                                        AiMcpRequest::Remove(remove_id.clone()),
                                        window,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                },
                                cx,
                            )),
                    ),
            );

        if let Some(line) = command.filter(|line| !line.is_empty()) {
            card = card.child(self.ai_mcp_code_line(line, cx));
        } else if !endpoint.is_empty() {
            card = card.child(self.ai_mcp_code_line(endpoint, cx));
        }
        // The status badge reports failure without displaying a server-supplied error body.
        if !tools.is_empty() {
            let mut chips = div().flex().flex_wrap().gap(px(4.0));
            for tool in tools.iter().take(AI_MCP_VISIBLE_TOOL_CHIPS) {
                chips = chips.child(
                    div()
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(self.tokens.radii.sm))
                        .bg(rgba((self.tokens.ui.bg_panel << 8) | AI_MCP_CODE_BG_ALPHA))
                        .text_size(px(self.tokens.metrics.ui_text_2xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(tool.name.clone()),
                );
            }
            let remaining_tools = tools.len().saturating_sub(AI_MCP_VISIBLE_TOOL_CHIPS);
            if remaining_tools > 0 {
                chips = chips.child(
                    div()
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(self.tokens.radii.sm))
                        .bg(rgba((self.tokens.ui.bg_panel << 8) | AI_MCP_CODE_BG_ALPHA))
                        .text_size(px(self.tokens.metrics.ui_text_2xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(format!("+{remaining_tools}")),
                );
            }
            card = card.child(
                div()
                    .mt(px(4.0))
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(rgba(
                        (self.tokens.ui.border << 8) | AI_MCP_TOOL_BORDER_ALPHA,
                    ))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(self.tokens.metrics.ui_text_2xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Wrench,
                                12.0,
                                rgb(self.tokens.ui.text_muted),
                            ))
                            .child(
                                self.i18n
                                    .t("settings_view.mcp.tools_count")
                                    .replace("{{count}}", &tools.len().to_string()),
                            ),
                    )
                    .child(chips),
            );
        }
        card.into_any_element()
    }

    fn ai_mcp_card_icon_button(
        &self,
        icon: LucideIcon,
        icon_color: Rgba,
        disabled: bool,
        on_click: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_icon_action_button(
            icon,
            AI_MCP_ACTION_ICON,
            icon_color,
            IconButtonOptions {
                disabled,
                hover_background: Some(rgba((self.tokens.ui.bg_hover << 8) | 0x80)),
                // MCP cards map Tauri disabled icon actions (`opacity-50`);
                // the workspace wrapper now owns the disabled action guard.
                disabled_opacity: 0.5,
                ..IconButtonOptions::opaque_toolbar(AI_MCP_CARD_ICON_BUTTON, ButtonRadius::Md)
            },
            on_click,
            cx,
        )
        .into_any_element()
    }

    fn ai_mcp_toggle_button(
        &self,
        status: &str,
        server_id: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connected = status == "connected";
        let connecting = status == "connecting";
        let label = if connected {
            self.i18n.t("settings_view.mcp.disconnect")
        } else if connecting {
            self.i18n.t("settings_view.mcp.connecting")
        } else {
            self.i18n.t("settings_view.mcp.connect")
        };
        let icon = if connected {
            LucideIcon::StopCircle
        } else if connecting {
            LucideIcon::LoaderCircle
        } else {
            LucideIcon::Radio
        };
        let mut options = ToolbarButtonOptions::compact_text(
            ButtonVariant::Ghost,
            ButtonRadius::Md,
            AI_MCP_CARD_ACTION_H,
            AI_MCP_CARD_ACTION_PX,
            self.tokens.metrics.ui_text_xs,
        );
        options.button.disabled = connecting;
        options.icon_gap = Some(4.0);
        options.text_color = Some(rgb(self.tokens.ui.text));
        options.hover_background = Some(rgba((self.tokens.ui.bg_hover << 8) | 0x80));
        // Tauri MCP connect/disconnect is a compact shadcn-style card action.
        // Keep loading/disabled behavior in the shared button primitive so the
        // connecting state cannot still submit.
        options.loading = connecting;
        let icon = if connecting {
            self.render_loading_icon(
                (
                    gpui::SharedString::from(format!("mcp-connect-spinner-{server_id}")),
                    0usize,
                ),
                AI_MCP_ACTION_ICON,
                rgb(self.tokens.ui.text),
            )
        } else {
            Self::render_lucide_icon(icon, AI_MCP_ACTION_ICON, rgb(self.tokens.ui.text))
        };
        self.workspace_toolbar_action_button(
            label,
            Some(icon),
            options,
            cx.listener(move |this, _event, window, cx| {
                (this.bindings.request)(
                    AiMcpRequest::SetConnected {
                        id: server_id.clone(),
                        was_connected: connected,
                    },
                    window,
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn ai_mcp_status_badge(&self, status: &str) -> AnyElement {
        let (label_key, color) = match status {
            "connected" => ("settings_view.mcp.status_connected", self.tokens.ui.success),
            "connecting" => (
                "settings_view.mcp.status_connecting",
                self.tokens.ui.warning,
            ),
            "error" => ("settings_view.mcp.status_error", self.tokens.ui.error),
            _ => (
                "settings_view.mcp.status_disconnected",
                self.tokens.ui.text_muted,
            ),
        };
        div()
            .px(px(8.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((color << 8) | 0x33))
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_2xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(color))
            .when(status == "connecting", |badge| {
                badge.child(self.render_loading_icon(
                    "mcp-status-connecting",
                    AI_MCP_STATUS_ICON,
                    rgb(color),
                ))
            })
            .when(status == "connected", |badge| {
                badge.child(Self::render_lucide_icon(
                    LucideIcon::Check,
                    AI_MCP_STATUS_ICON,
                    rgb(color),
                ))
            })
            .child(self.i18n.t(label_key))
            .into_any_element()
    }

    fn ai_mcp_transport_badge(&self, transport: oxideterm_ai::McpTransport) -> AnyElement {
        div()
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgb(self.tokens.ui.bg_panel))
            .text_size(px(self.tokens.metrics.ui_text_2xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(ai_mcp_transport_label(transport).to_uppercase())
            .into_any_element()
    }

    fn ai_mcp_code_line(&self, value: String, cx: &mut Context<Self>) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                div()
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgba((self.tokens.ui.bg_panel << 8) | AI_MCP_CODE_BG_ALPHA))
                    .child(self.render_selectable_display_text(
                        "ai-mcp-code-line",
                        &value,
                        value.clone(),
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            )
            .into_any_element()
    }
}
impl Render for AiMcpSettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(controls) = (self.bindings.controls)(cx) else {
            return div().into_any_element();
        };
        self.tokens = *controls.tokens;
        self.i18n = controls.i18n.clone();
        let enabled = controls.settings.ai.enabled;
        let configs = ai_mcp_configs(controls.settings);
        let header = self.header(controls.settings, cx);
        super::available(
            self.ai_mcp_servers_section(configs, header, window, cx),
            enabled,
        )
    }
}
