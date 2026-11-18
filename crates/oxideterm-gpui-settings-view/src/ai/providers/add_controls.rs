use super::*;

impl AiProviderSettingsView {
    pub(super) fn ai_provider_add_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = ai_provider_template_by_type(self.provider_type());
        let provider_template_select = (self.bindings.controls)(cx)
            .map(|controls| {
                controls.settings_select_control(
                    SettingsSelect::AiProviderTemplate,
                    self.i18n.t(selected.label_key),
                    false,
                    Some(AI_PROVIDER_SELECT_W),
                    cx,
                )
            })
            .unwrap_or_else(|| div().into_any_element());

        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .items_end()
            .gap(px(12.0))
            .child(
                div()
                    .grid()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.ai.provider_template")),
                    )
                    .child(provider_template_select),
            )
            .child(
                // Use the workspace action wrapper with LucideIcon::Plus so
                // provider creation matches the other settings toolbar buttons.
                self.workspace_toolbar_action_button(
                    self.i18n.t("settings_view.ai.add_provider"),
                    Some(Self::render_lucide_icon(
                        LucideIcon::Plus,
                        14.0,
                        rgb(self.tokens.ui.text),
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
                    cx.listener(|this, _event, _window, cx| {
                        this.add_ai_provider_from_selected_template(cx);
                    }),
                ),
            )
            .into_any_element()
    }
}
