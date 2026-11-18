use super::{settings_ai_section_heading, textarea};
use crate::{
    SettingsInput,
    components::{SettingsControls, SettingsControlsSource, SettingsWindowRequest},
    entity::SettingsWorkspaceEntity,
};
use gpui::{
    AnyElement, App, Context, Div, Entity, MouseButton, MouseDownEvent, Render, Window, div,
    prelude::*, px, rgb, rgba, svg,
};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions, icon_button, toolbar_button,
    },
    checkbox::checkbox,
    icons::LucideIcon,
};
use oxideterm_settings::PersistedSettings;
use oxideterm_settings_model::{
    AcpAgentPreset, ai_add_acp_agent, ai_add_acp_agent_preset, ai_delete_acp_agent,
    persisted_settings_input_value,
};
use std::{ops::Deref, rc::Rc};

pub type AcpProbeStatus = Rc<dyn Fn(&str, &App) -> bool>;

pub struct AcpSettingsView {
    state: Entity<SettingsWorkspaceEntity>,
    controls: SettingsControlsSource,
    pending: AcpProbeStatus,
    plugin_id: Option<String>,
}

impl AcpSettingsView {
    pub fn new(
        state: Entity<SettingsWorkspaceEntity>,
        controls: SettingsControlsSource,
        pending: AcpProbeStatus,
        plugin_id: Option<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            controls,
            pending,
            plugin_id,
        }
    }

    pub(crate) fn plugin_builder(
        &self,
        plugin_id: String,
    ) -> impl FnOnce(&mut Context<Self>) -> Self + use<> {
        let state = self.state.clone();
        let controls = self.controls.clone();
        let pending = self.pending.clone();
        move |cx| Self::new(state, controls, pending, Some(plugin_id), cx)
    }
}

impl Render for AcpSettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(controls) = (self.controls)(cx) else {
            return div().into_any_element();
        };
        let page = AcpPage {
            controls,
            state: &self.state,
            pending: &self.pending,
        };
        match self.plugin_id.as_deref() {
            Some(plugin_id) => div()
                .children(
                    page.settings
                        .ai
                        .acp_agents
                        .iter()
                        .enumerate()
                        .filter(|(_, agent)| agent.plugin_id.as_deref() == Some(plugin_id))
                        .map(|(index, agent)| page.ai_acp_agent_card(index, agent, cx)),
                )
                .into_any_element(),
            None => page.ai_acp_agents_section(page.controls.settings, cx),
        }
    }
}

struct AcpPage<'a> {
    state: &'a Entity<SettingsWorkspaceEntity>,
    controls: SettingsControls<'a>,
    pending: &'a AcpProbeStatus,
}
impl<'a> Deref for AcpPage<'a> {
    type Target = SettingsControls<'a>;
    fn deref(&self) -> &Self::Target {
        &self.controls
    }
}

const AI_ACP_AGENT_TEXTAREA_MIN_H: f32 = 72.0; // Tauri min-h-[72px] for ACP args/env drafts.
const AI_ACP_AGENT_FIELD_MIN_WIDTH: f32 = 220.0; // Keep ACP form fields readable on narrow settings panes.
const AI_ACP_AGENT_TEXTAREA_FIELD_MIN_WIDTH: f32 = 240.0; // Multiline command fields need more room than compact selects.
const AI_ACP_AGENT_CAPABILITY_MIN_WIDTH: f32 = 150.0; // Capability checkboxes should wrap as chips, not grid columns.
const AI_READONLY_VALUE_WIDTH: f32 = 180.0; // Keep compact status fields aligned with text inputs.

impl AcpPage<'_> {
    fn press<F: Fn() -> SettingsWindowRequest + 'static>(
        &self,
        build: F,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<F> {
        let dispatch = self.controls.dispatch.clone();
        move |_, window, cx| {
            dispatch(build(), window, cx);
            cx.stop_propagation();
        }
    }

    fn edit<F: Fn(&mut PersistedSettings) + Clone + 'static>(
        &self,
        f: F,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<F> {
        self.press(move || SettingsWindowRequest::Edit(Box::new(f.clone())))
    }

    fn render_lucide_icon(icon: LucideIcon, size: f32, color: gpui::Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        action: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let enabled = !(options.button.disabled || options.loading);
        toolbar_button(self.tokens, label, icon, options).on_mouse_down(
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

    fn ai_icon_button(
        &self,
        icon: LucideIcon,
        disabled: bool,
        action: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        icon_button(
            self.tokens,
            Self::render_lucide_icon(icon, 15.0, rgb(self.tokens.ui.error)),
            IconButtonOptions {
                disabled,
                hover_background: Some(rgba((self.tokens.ui.bg_hover << 8) | 0x80)),
                ..IconButtonOptions::opaque_toolbar(30.0, ButtonRadius::Md)
            },
        )
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            if !disabled {
                action(event, window, cx);
            } else {
                cx.stop_propagation();
            }
        })
        .into_any_element()
    }

    fn ai_i18n_error(&self, key: &str, error: &str) -> String {
        self.i18n.t(key).replace("{{error}}", error)
    }

    fn i18n_count(&self, key: &str, count: usize) -> String {
        self.i18n.t(key).replace("{{count}}", &count.to_string())
    }

    fn current_settings_input_value(&self, input: SettingsInput, _: &App) -> String {
        persisted_settings_input_value(self.settings, input).unwrap_or_default()
    }
    fn ai_readonly_value(&self, label: String) -> AnyElement {
        div()
            .w(px(AI_READONLY_VALUE_WIDTH))
            .max_w_full()
            .min_w(px(0.0))
            .h(px(32.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
            .bg(rgba((self.tokens.ui.bg << 8) | 0x66))
            .px(px(10.0))
            .flex()
            .items_center()
            .text_size(px(12.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(div().min_w(px(0.0)).truncate().child(label))
            .into_any_element()
    }

    fn ai_acp_agents_section(&self, settings: &PersistedSettings, cx: &App) -> AnyElement {
        let agent_count = settings
            .ai
            .acp_agents
            .iter()
            .filter(|agent| agent.plugin_id.is_none())
            .count();
        let mut section = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(settings_ai_section_heading(
                        self.tokens,
                        self.i18n.t("ai.model_selector.custom_acp_agents"),
                        self.i18n_count("settings_view.ai.acp_agents_summary", agent_count),
                    ))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .flex_wrap()
                            .justify_end()
                            .gap(px(8.0))
                            .child(self.workspace_toolbar_action_button(
                                self.i18n.t(if self.state.read(cx).ai_acp_expanded {
                                    "plugin.hide_details"
                                } else {
                                    "plugin.show_details"
                                }),
                                Some(Self::render_lucide_icon(
                                    if self.state.read(cx).ai_acp_expanded {
                                        LucideIcon::ChevronDown
                                    } else {
                                        LucideIcon::ChevronRight
                                    },
                                    14.0,
                                    rgb(self.tokens.ui.text_muted),
                                )),
                                ToolbarButtonOptions::default(),
                                {
                                    let state = self.state.downgrade();
                                    move |_, _, cx| {
                                        let _ = state.update(cx, |state, cx| {
                                            state.ai_acp_expanded = !state.ai_acp_expanded;
                                            cx.notify();
                                        });
                                        cx.stop_propagation();
                                    }
                                },
                            ))
                            .child(self.ai_acp_agent_add_button(
                                self.i18n.t("settings_view.ai.acp_agent_add"),
                                None,
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .whitespace_normal()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.ai.acp_agents_custom_hint")),
            );

        if !self.state.read(cx).ai_acp_expanded {
            return section.into_any_element();
        }
        if agent_count == 0 {
            return section
                .child(
                    div()
                        .py(px(12.0))
                        .text_size(px(12.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.ai.acp_agents_empty")),
                )
                .into_any_element();
        }

        for (index, agent) in settings.ai.acp_agents.iter().enumerate() {
            if agent.plugin_id.is_none() {
                section = section.child(self.ai_acp_agent_card(index, agent, cx));
            }
        }
        section.into_any_element()
    }

    fn ai_acp_agent_card(
        &self,
        index: usize,
        agent: &oxideterm_settings::AcpAgentConfig,
        cx: &App,
    ) -> AnyElement {
        let testing = (self.pending)(&agent.id, cx);
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x99))
            .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
            .p(px(14.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(self.ai_acp_agent_enabled_toggle(index, agent.enabled))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_end()
                            .gap(px(8.0))
                            .when_some(agent.status.last_error_kind.as_ref(), |row, error| {
                                let error_label = self.i18n.t(acp_agent_error_kind_key(error));
                                row.child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_2xs))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(self.ai_i18n_error(
                                            "settings_view.ai.acp_agent_last_error",
                                            &error_label,
                                        )),
                                )
                            })
                            .child(self.ai_acp_agent_status_badge(agent))
                            .child(self.ai_acp_agent_test_button(agent, testing))
                            .when(agent.plugin_id.is_none(), |actions| {
                                actions.child(self.ai_icon_button(
                                    LucideIcon::Trash2,
                                    testing,
                                    self.edit(move |settings| ai_delete_acp_agent(settings, index)),
                                ))
                            }),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(10.0))
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_FIELD_MIN_WIDTH,
                        self.ai_labeled_text_input(
                            "settings_view.ai.acp_agent_name",
                            SettingsInput::AiAcpAgentDisplayName(index),
                            self.i18n.t("settings_view.ai.acp_agent_new_name"),
                            cx,
                        ),
                    ))
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_FIELD_MIN_WIDTH,
                        if let Some(plugin_id) = agent.plugin_id.as_ref() {
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(6.0))
                                .child(self.i18n.t("settings_view.ai.acp_agent_plugin"))
                                .child(self.ai_readonly_value(plugin_id.clone()))
                                .into_any_element()
                        } else {
                            self.ai_labeled_text_input(
                                "settings_view.ai.acp_agent_command",
                                SettingsInput::AiAcpAgentCommand(index),
                                self.i18n
                                    .t("settings_view.ai.acp_agent_command_placeholder"),
                                cx,
                            )
                        },
                    ))
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_FIELD_MIN_WIDTH,
                        self.ai_labeled_text_input(
                            "settings_view.ai.acp_agent_cwd",
                            SettingsInput::AiAcpAgentCwd(index),
                            self.i18n.t("settings_view.ai.acp_agent_cwd_placeholder"),
                            cx,
                        ),
                    ))
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_FIELD_MIN_WIDTH,
                        self.ai_readonly_value(
                            self.i18n.t(acp_agent_auth_status_key(&agent.auth.status)),
                        ),
                    )),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(10.0))
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_TEXTAREA_FIELD_MIN_WIDTH,
                        textarea(
                            &self.controls,
                            SettingsInput::AiAcpAgentArgs(index),
                            self.i18n.t("settings_view.ai.acp_agent_args"),
                            self.i18n.t("settings_view.ai.acp_agent_args_placeholder"),
                            self.i18n.t("settings_view.ai.acp_agent_args_placeholder"),
                            self.current_settings_input_value(
                                SettingsInput::AiAcpAgentArgs(index),
                                cx,
                            ),
                            AI_ACP_AGENT_TEXTAREA_MIN_H,
                            cx,
                        ),
                    ))
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_TEXTAREA_FIELD_MIN_WIDTH,
                        textarea(
                            &self.controls,
                            SettingsInput::AiAcpAgentEnv(index),
                            self.i18n.t("settings_view.ai.acp_agent_env"),
                            self.i18n.t("settings_view.ai.acp_agent_env_placeholder"),
                            self.i18n.t("settings_view.ai.acp_agent_env_placeholder"),
                            self.current_settings_input_value(
                                SettingsInput::AiAcpAgentEnv(index),
                                cx,
                            ),
                            AI_ACP_AGENT_TEXTAREA_MIN_H,
                            cx,
                        ),
                    )),
            )
            .child(self.ai_acp_agent_capabilities(index, agent))
            .into_any_element()
    }

    fn ai_responsive_field(&self, min_width: f32, control: AnyElement) -> AnyElement {
        // Flex-basis keeps desktop columns stable, while min-width zero lets the
        // field shrink only after it has wrapped onto its own line.
        div()
            .min_w(px(0.0))
            .max_w_full()
            .flex_1()
            .flex_basis(px(min_width))
            .child(control)
            .into_any_element()
    }

    fn ai_acp_agent_add_button(
        &self,
        label: String,
        preset: Option<AcpAgentPreset>,
        _cx: &App,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(
                LucideIcon::Plus,
                14.0,
                rgb(self.tokens.ui.text_muted),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                icon_gap: Some(6.0),
                ..ToolbarButtonOptions::default()
            },
            {
                let dispatch = self.controls.dispatch.clone();
                let state = self.state.downgrade();
                move |_, window, cx| {
                    dispatch(
                        SettingsWindowRequest::Edit(Box::new(move |settings| match preset {
                            Some(preset) => ai_add_acp_agent_preset(settings, preset),
                            None => ai_add_acp_agent(settings),
                        })),
                        window,
                        cx,
                    );
                    let _ = state.update(cx, |state, cx| {
                        state.ai_acp_expanded = true;
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
            },
        )
        .into_any_element()
    }

    fn ai_labeled_text_input(
        &self,
        label_key: &str,
        input: SettingsInput,
        placeholder: String,
        cx: &App,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.settings_text_input_control(
                input,
                self.current_settings_input_value(input, cx),
                placeholder,
                240.0,
                cx,
            ))
            .into_any_element()
    }

    fn ai_acp_agent_enabled_toggle(&self, index: usize, enabled: bool) -> AnyElement {
        checkbox(
            self.tokens,
            format!("acp-enabled-{}", self.settings.ai.acp_agents[index].id),
            if enabled {
                self.i18n.t("settings_view.ai.acp_agent_enabled")
            } else {
                self.i18n.t("settings_view.ai.acp_agent_disabled")
            },
            enabled,
        )
        .on_mouse_down(
            MouseButton::Left,
            self.edit(move |settings| {
                if let Some(agent) = settings.ai.acp_agents.get_mut(index) {
                    agent.enabled = !enabled;
                }
            }),
        )
        .into_any_element()
    }

    fn ai_acp_agent_status_badge(&self, agent: &oxideterm_settings::AcpAgentConfig) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((self.tokens.ui.bg << 8) | 0x80))
            .px(px(8.0))
            .py(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_2xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                self.i18n
                    .t(acp_agent_runtime_status_key(&agent.status.state)),
            )
            .into_any_element()
    }

    fn ai_acp_agent_test_button(
        &self,
        agent: &oxideterm_settings::AcpAgentConfig,
        testing: bool,
    ) -> AnyElement {
        // Capture only the stable id. Cloning the whole config here would keep
        // token-bearing args and env alive for the lifetime of the render tree.
        let agent_id = agent.id.clone();
        let mut options = ToolbarButtonOptions::compact_text(
            ButtonVariant::Outline,
            ButtonRadius::Md,
            28.0,
            8.0,
            12.0,
        );
        options.button.disabled = testing;
        options.icon_gap = Some(6.0);
        options.loading = testing;

        self.workspace_toolbar_action_button(
            if testing {
                self.i18n.t("settings_view.ai.acp_agent_testing")
            } else {
                self.i18n.t("settings_view.ai.acp_agent_test")
            },
            Some(Self::render_lucide_icon(
                LucideIcon::RefreshCw,
                12.0,
                rgb(self.tokens.ui.text_muted),
            )),
            options,
            self.press(move || SettingsWindowRequest::ProbeAcpAgent(agent_id.clone())),
        )
        .into_any_element()
    }

    fn ai_acp_agent_capabilities(
        &self,
        index: usize,
        agent: &oxideterm_settings::AcpAgentConfig,
    ) -> AnyElement {
        // Capabilities are part of the agent form, not another independent card.
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(self.card_separator())
            .child(
                div()
                    .pt(px(4.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.ai.acp_agent_capabilities")),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_CAPABILITY_MIN_WIDTH,
                        self.ai_acp_agent_capability_toggle(
                            index,
                            "settings_view.ai.acp_agent_capability_read",
                            agent.capability_policy.fs_read_text_file,
                            |policy| policy.fs_read_text_file = !policy.fs_read_text_file,
                        ),
                    ))
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_CAPABILITY_MIN_WIDTH,
                        self.ai_acp_agent_capability_toggle(
                            index,
                            "settings_view.ai.acp_agent_capability_write",
                            agent.capability_policy.fs_write_text_file,
                            |policy| policy.fs_write_text_file = !policy.fs_write_text_file,
                        ),
                    ))
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_CAPABILITY_MIN_WIDTH,
                        self.ai_acp_agent_capability_toggle(
                            index,
                            "settings_view.ai.acp_agent_capability_terminal",
                            agent.capability_policy.terminal,
                            |policy| policy.terminal = !policy.terminal,
                        ),
                    )),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_2xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.ai.acp_agent_capabilities_hint")),
            )
            .into_any_element()
    }

    fn ai_acp_agent_capability_toggle(
        &self,
        index: usize,
        label_key: &str,
        checked: bool,
        toggle: fn(&mut oxideterm_settings::AcpAgentCapabilityPolicy),
    ) -> AnyElement {
        let label = self.i18n.t(label_key);
        checkbox(
            self.tokens,
            format!("acp-{}-{label_key}", self.settings.ai.acp_agents[index].id),
            label,
            checked,
        )
        .on_mouse_down(
            MouseButton::Left,
            self.edit(move |settings| {
                if let Some(agent) = settings.ai.acp_agents.get_mut(index) {
                    toggle(&mut agent.capability_policy);
                }
            }),
        )
        .into_any_element()
    }
}

fn acp_agent_auth_status_key(status: &oxideterm_settings::AcpAgentAuthStatus) -> &'static str {
    match status {
        oxideterm_settings::AcpAgentAuthStatus::Unknown => {
            "settings_view.ai.acp_agent_auth_unknown"
        }
        oxideterm_settings::AcpAgentAuthStatus::NotRequired => {
            "settings_view.ai.acp_agent_auth_not_required"
        }
        oxideterm_settings::AcpAgentAuthStatus::Required => {
            "settings_view.ai.acp_agent_auth_required"
        }
        oxideterm_settings::AcpAgentAuthStatus::Authenticated => {
            "settings_view.ai.acp_agent_auth_authenticated"
        }
        oxideterm_settings::AcpAgentAuthStatus::Expired => {
            "settings_view.ai.acp_agent_auth_expired"
        }
    }
}

fn acp_agent_runtime_status_key(status: &oxideterm_settings::AcpAgentRuntimeState) -> &'static str {
    match status {
        oxideterm_settings::AcpAgentRuntimeState::Unknown => {
            "settings_view.ai.acp_agent_status_unknown"
        }
        oxideterm_settings::AcpAgentRuntimeState::Ready => {
            "settings_view.ai.acp_agent_status_ready"
        }
        oxideterm_settings::AcpAgentRuntimeState::AuthRequired => {
            "settings_view.ai.acp_agent_status_auth_required"
        }
        oxideterm_settings::AcpAgentRuntimeState::Error => {
            "settings_view.ai.acp_agent_status_error"
        }
    }
}

fn acp_agent_error_kind_key(kind: &str) -> &'static str {
    match kind {
        "command_not_found" => "settings_view.ai.acp_agent_error_command_not_found",
        "config" => "settings_view.ai.acp_agent_error_config",
        "initialize" => "settings_view.ai.acp_agent_error_initialize",
        _ => "settings_view.ai.acp_agent_error_unknown",
    }
}
