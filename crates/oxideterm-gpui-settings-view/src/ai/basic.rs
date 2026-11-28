use crate::{
    SettingsSelect,
    components::{SettingsControls, SettingsWindowRequest},
    options::set_ai_enabled,
};
use gpui::{AnyElement, App, MouseButton, div, prelude::*, px, rgb};
use oxideterm_gpui_ui::switch::switch;
use oxideterm_settings_model::{SettingsSectionId, ai_provider_views};

const AI_PROVIDER_SELECT_W: f32 = 224.0;

fn web_search(controls: &SettingsControls<'_>, cx: &App) -> AnyElement {
    let settings = controls.settings;
    let selected = ai_provider_views(settings).into_iter().find(|provider| {
        Some(provider.id.as_str()) == settings.ai.web_search.provider_id.as_deref()
            && oxideterm_ai::web::official_deepseek_provider(provider)
    });
    let label = selected
        .map(|provider| provider.name)
        .unwrap_or_else(|| controls.i18n.t("settings_view.ai.web_provider_none"));
    controls.settings_card(
        "settings_view.ai.web_title",
        "",
        vec![
            controls.bool_row(
                "settings_view.ai.web_enabled",
                "settings_view.ai.web_enabled_hint",
                settings.ai.web_search.enabled,
                |settings, enabled| settings.ai.web_search.enabled = enabled,
                cx,
            ),
            controls.setting_row(
                "settings_view.ai.web_provider",
                "settings_view.ai.web_provider_hint",
                controls.settings_select_control(
                    SettingsSelect::AiWebSearchProvider,
                    label,
                    false,
                    Some(AI_PROVIDER_SELECT_W),
                    cx,
                ),
                cx,
            ),
            div()
                .text_size(px(controls.tokens.metrics.ui_text_xs))
                .text_color(rgb(controls.tokens.ui.text_muted))
                .child(controls.i18n.t("settings_view.ai.web_billing_hint"))
                .into_any_element(),
        ],
    )
}

pub(crate) fn basic_section(
    controls: &SettingsControls<'_>,
    section: SettingsSectionId,
    session_only: bool,
    cx: &App,
) -> AnyElement {
    let enabled = controls.settings.ai.enabled;
    match section {
        SettingsSectionId::AiGeneralSettings => {
            let confirmed = controls.settings.ai.enabled_confirmed;
            let dispatch = controls.dispatch.clone();
            controls.settings_card(
                "settings_view.ai.general",
                "",
                vec![
                    controls.setting_row(
                        "settings_view.ai.enable",
                        "settings_view.ai.enable_hint",
                        switch(controls.tokens, "ai-enabled", String::new(), enabled)
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                let request = if !enabled && !confirmed {
                                    SettingsWindowRequest::ConfirmAiEnable
                                } else {
                                    SettingsWindowRequest::Edit(Box::new(move |settings| {
                                        set_ai_enabled(settings, !enabled)
                                    }))
                                };
                                dispatch(request, window, cx);
                            })
                            .into_any_element(),
                        cx,
                    ),
                ],
            )
        }
        SettingsSectionId::AiGeneralPrivacy => controls.settings_card(
            "settings_view.ai.privacy_notice",
            "",
            vec![
                div()
                    .text_size(px(controls.tokens.metrics.ui_text_xs))
                    .text_color(rgb(controls.tokens.ui.text_muted))
                    .line_height(px(18.0))
                    .child(controls.i18n.t(if session_only { "settings_view.ai.browser_privacy" } else { "settings_view.ai.privacy_text" }))
                    .into_any_element(),
            ],
        ),
        SettingsSectionId::AiToolsWebSearch => super::available(web_search(controls, cx), enabled),
        _ => unreachable!("not an AI general or Web section"),
    }
}
