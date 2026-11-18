use super::*;
use oxideterm_settings::BackgroundStyle;

impl WorkspaceApp {
    pub(super) fn appearance_daylight_preview_control(
        &self,
        preview: Entity<oxideterm_gpui_background::BackgroundPreview>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = preview.read(cx).preview_hour();
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.terminal.bg_time_preview")),
            )
            .children(
                [
                    (None, "bg_auto"),
                    (Some(6.0), "bg_morning"),
                    (Some(12.0), "bg_afternoon"),
                    (Some(18.0), "bg_evening"),
                    (Some(0.0), "bg_night"),
                ]
                .into_iter()
                .map(|(hour, key)| {
                    let preview = preview.clone();
                    oxideterm_gpui_settings_view::background_page::appearance_option_pill(
                        &self.tokens,
                        self.i18n.t(&format!("settings_view.terminal.{key}")),
                        selected == hour,
                        {
                            let settings = self.settings_workspace.downgrade();
                            move |_, _, cx| {
                                preview
                                    .update(cx, |preview, cx| preview.set_preview_hour(hour, cx));
                                let _ = settings.update(cx, |_, cx| cx.notify());
                                cx.stop_propagation();
                            }
                        },
                    )
                }),
            )
            .into_any_element()
    }

    pub(super) fn appearance_edit_scheme(&self, cx: &App) -> Option<bool> {
        Some(
            self.settings_workspace
                .read(cx)
                .appearance_edit_dark
                .unwrap_or(false),
        )
    }
    pub(in crate::workspace) fn background_settings_for_controls(
        &self,
        cx: &App,
    ) -> PersistedSettings {
        let mut settings = self.settings_store.settings().clone();
        let style = self
            .settings_workspace
            .read(cx)
            .theme_editor()
            .map(|editor| editor.background.clone())
            .unwrap_or_else(|| {
                settings
                    .terminal
                    .background_for_scheme(self.appearance_edit_scheme(cx))
            });
        settings.terminal.apply_background_style(style);
        settings
    }

    pub(in crate::workspace) fn edit_background_style(
        &mut self,
        edit: impl FnOnce(&mut BackgroundStyle),
        cx: &mut Context<Self>,
    ) {
        if self.settings_workspace.read(cx).theme_editor().is_some() {
            self.settings_workspace.update(cx, |settings, cx| {
                settings.edit_theme_editor_background(edit, cx);
            });
        } else {
            let dark = self.appearance_edit_scheme(cx);
            self.edit_settings(
                |settings| {
                    let mut style = settings.terminal.background_for_scheme(dark);
                    edit(&mut style);
                    settings.terminal.set_background_for_scheme(dark, style);
                },
                cx,
            );
        }
    }
}
