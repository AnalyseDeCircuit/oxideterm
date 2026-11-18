use std::rc::Rc;

use gpui::{AnyElement, App, Div, Entity, Window, div, prelude::*, px, rgb};
use oxideterm_ai::agent::{
    AgentModel, AgentOptions, DEFAULT_AGENT_CONCURRENCY, MAX_AGENT_CONCURRENCY,
};
use oxideterm_gpui_ui::{
    button::{ButtonOptions, ButtonSize, ButtonVariant, button_with},
    checkbox::checkbox,
    tabs::{segmented_tab, segmented_tabs},
};
use oxideterm_settings::PersistedSettings;
use oxideterm_settings_model::ai_provider_views;

use crate::{
    components::{SettingsControls, SettingsWindowRequest},
    entity::SettingsWorkspaceEntity,
};

pub fn global_agent_options(settings: &PersistedSettings) -> AgentOptions {
    settings
        .ai
        .extra
        .get("subagents")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}

pub fn agent_concurrency(settings: &PersistedSettings) -> usize {
    settings
        .ai
        .extra
        .get("subagentConcurrency")
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(DEFAULT_AGENT_CONCURRENCY)
        .clamp(1, MAX_AGENT_CONCURRENCY)
}

pub fn set_agent_concurrency(settings: &mut PersistedSettings, limit: usize) {
    settings
        .ai
        .extra
        .insert("subagentConcurrency".into(), serde_json::json!(limit));
}

fn edit_options(settings: &mut PersistedSettings, edit: impl FnOnce(&mut AgentOptions)) {
    let mut options = global_agent_options(settings);
    edit(&mut options);
    settings.ai.extra.insert(
        "subagents".into(),
        serde_json::to_value(options).expect("agent options are serializable"),
    );
}

fn control(
    controls: &SettingsControls<'_>,
    id: String,
    label: String,
    body: Div,
    action: impl Fn(&mut Window, &mut App) + 'static,
) -> gpui::Stateful<Div> {
    let action = Rc::new(action);
    let click = action.clone();
    let accent = controls.tokens.ui.accent;
    body.id(gpui::SharedString::from(id))
        .role(gpui::Role::Button)
        .aria_label(label)
        .focusable()
        .tab_stop(true)
        .focus_visible(move |style| style.border_1().border_color(rgb(accent)))
        .on_click(move |_, window, cx| {
            click(window, cx);
            cx.stop_propagation();
        })
        .on_key_down(move |event, window, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                action(window, cx);
                cx.stop_propagation();
            }
        })
}

fn button(
    controls: &SettingsControls<'_>,
    id: String,
    label: String,
    action: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let body = button_with(
        controls.tokens,
        label.clone(),
        ButtonOptions {
            variant: ButtonVariant::Ghost,
            size: ButtonSize::Sm,
            ..Default::default()
        },
    );
    control(controls, id, label, body, action).into_any_element()
}

pub(crate) fn render(
    controls: &SettingsControls<'_>,
    state: &Entity<SettingsWorkspaceEntity>,
    cx: &App,
) -> AnyElement {
    let options = global_agent_options(controls.settings);
    let checked = options.enabled;
    let i18n = controls.i18n;
    let tokens = controls.tokens;
    let toggle = control(
        controls,
        "allow-subagents".into(),
        i18n.t("ai.agents.allow"),
        checkbox(tokens, "allow-subagents", String::new(), checked),
        {
            let dispatch = controls.dispatch.clone();
            move |window, cx| {
                dispatch(
                    SettingsWindowRequest::Edit(Box::new(|settings| {
                        edit_options(settings, |options| options.enabled = !options.enabled);
                    })),
                    window,
                    cx,
                )
            }
        },
    )
    .role(gpui::Role::CheckBox)
    .aria_toggled(if checked {
        gpui::Toggled::True
    } else {
        gpui::Toggled::False
    })
    .into_any_element();
    let picker = button(
        controls,
        "agent-default-model".into(),
        options
            .default_model
            .map(|model| format!("{} / {}", model.provider_id, model.model))
            .unwrap_or_else(|| i18n.t("ai.agents.inherit_model")),
        {
            let state = state.downgrade();
            move |_, cx| {
                let _ = state.update(cx, |state, cx| {
                    state.ai_subagent_picker_open = !state.ai_subagent_picker_open;
                    cx.notify();
                });
            }
        },
    );
    let mut result = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(tokens.metrics.settings_card_gap))
        .when(!controls.settings.ai.tool_use.enabled, |section| {
            section.child(
                div()
                    .text_size(px(tokens.metrics.ui_text_xs))
                    .text_color(rgb(tokens.ui.warning))
                    .child(i18n.t("settings_view.ai.subagents_tools_off")),
            )
        })
        .child(controls.setting_row(
            "ai.agents.allow",
            "settings_view.ai.subagents_scope_hint",
            toggle,
            cx,
        ))
        .child(controls.setting_row("ai.agents.model", "ai.agents.model_hint", picker, cx));
    if state.read(cx).ai_subagent_picker_open {
        let choices = std::iter::once(None).chain(
            ai_provider_views(controls.settings)
                .into_iter()
                .filter(|provider| provider.enabled)
                .flat_map(|provider| {
                    provider.models.into_iter().map(move |model| {
                        Some(AgentModel {
                            provider_id: provider.id.clone(),
                            model,
                        })
                    })
                }),
        );
        result = result.child(
            div()
                .id("agent-model-options")
                .max_h(px(180.0))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(tokens.spacing.one))
                .children(choices.map(|model| {
                    let (id, label) = match &model {
                        Some(model) => (
                            format!("agent-model-{}-{}", model.provider_id, model.model),
                            format!("{} / {}", model.provider_id, model.model),
                        ),
                        None => (
                            "agent-inherit-model".into(),
                            i18n.t("ai.agents.inherit_model"),
                        ),
                    };
                    let state = state.downgrade();
                    let dispatch = controls.dispatch.clone();
                    button(controls, id, label, move |window, cx| {
                        let model = model.clone();
                        dispatch(
                            SettingsWindowRequest::Edit(Box::new(move |settings| {
                                edit_options(settings, |options| options.default_model = model);
                            })),
                            window,
                            cx,
                        );
                        let _ = state.update(cx, |state, cx| {
                            state.ai_subagent_picker_open = false;
                            cx.notify();
                        });
                    })
                })),
        );
    }
    let current = agent_concurrency(controls.settings);
    let mut choices = segmented_tabs(tokens);
    for limit in 1..=MAX_AGENT_CONCURRENCY {
        let dispatch = controls.dispatch.clone();
        choices = choices.child(
            control(
                controls,
                format!("agent-limit-{limit}"),
                limit.to_string(),
                segmented_tab(tokens, limit.to_string(), current == limit),
                move |window, cx| {
                    dispatch(
                        SettingsWindowRequest::SetAiAgentConcurrency(limit),
                        window,
                        cx,
                    )
                },
            )
            .aria_selected(current == limit),
        );
    }
    result = result.child(controls.setting_row(
        "ai.agents.concurrency",
        "settings_view.ai.subagents_global_hint",
        choices.into_any_element(),
        cx,
    ));
    super::available(
        controls.settings_card(
            "settings_view.ai.conversation_agents",
            "settings_view.ai.conversation_agents_hint",
            vec![result.into_any_element()],
        ),
        controls.settings.ai.enabled,
    )
}
