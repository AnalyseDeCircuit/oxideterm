use super::*;
use oxideterm_settings::{BackgroundAlignment, BackgroundStyle};

impl WorkspaceApp {
    pub(super) fn appearance_background_interaction_rows(
        &self,
        settings: &PersistedSettings,
        animated: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let terminal = &settings.terminal;
        let mut rows = vec![settings_appearance_card_title(
            &self.tokens,
            self.i18n.t("settings_view.terminal.bg_interactions"),
            None,
        )];
        for (index, key, enabled, visible) in [
            (
                1,
                "bg_parallax",
                terminal.background_parallax,
                terminal.background_image.is_some() && !animated,
            ),
            (
                3,
                "bg_camera",
                terminal.background_camera.is_some(),
                terminal.background_image.is_some() && !animated,
            ),
            (
                0,
                "bg_pause_input",
                terminal.background_pause_on_input,
                animated
                    || terminal.background_parallax
                    || terminal.background_camera.is_some()
                    || terminal
                        .background_effect
                        .as_ref()
                        .is_some_and(|effect| effect.has_motion()),
            ),
            (2, "bg_day_cycle", terminal.background_day_cycle, true),
        ] {
            if !visible {
                continue;
            }
            rows.push(
                self.appearance_row(
                    &format!("settings_view.terminal.{key}"),
                    &format!("settings_view.terminal.{key}_hint"),
                    checkbox(&self.tokens, String::new(), enabled)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.edit_background_style(
                                    |style| match index {
                                        0 => style.pause_on_input = !enabled,
                                        1 => style.parallax = !enabled,
                                        3 => style.camera = (!enabled).then(Default::default),
                                        _ => style.day_cycle = !enabled,
                                    },
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        )
                        .into_any_element(),
                ),
            );
            if index == 3
                && let Some(camera) = terminal.background_camera
            {
                use oxideterm_settings::BackgroundCameraMotion;
                rows.push(
                    self.appearance_row(
                        "settings_view.terminal.bg_camera_motion",
                        "settings_view.terminal.bg_camera_motion_hint",
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(self.tokens.spacing.two))
                            .children(
                                [
                                    (BackgroundCameraMotion::Pan, "bg_camera_pan"),
                                    (BackgroundCameraMotion::ZoomIn, "bg_camera_zoom_in"),
                                    (BackgroundCameraMotion::ZoomOut, "bg_camera_zoom_out"),
                                ]
                                .into_iter()
                                .map(|(motion, key)| {
                                    self.appearance_action_button(
                                        LucideIcon::Image,
                                        self.i18n.t(&format!("settings_view.terminal.{key}")),
                                        cx.listener(move |this, _, _, cx| {
                                            this.edit_background_style(
                                                |style| {
                                                    if let Some(camera) = &mut style.camera {
                                                        camera.motion = motion;
                                                    }
                                                },
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }),
                                    )
                                    .when(camera.motion == motion, |button| {
                                        button.border_color(rgb(self.tokens.ui.accent))
                                    })
                                }),
                            )
                            .into_any_element(),
                    ),
                );
                for (slider, key, min, max, value) in [
                    (
                        SettingsSlider::BackgroundCameraAmount,
                        "bg_camera_amount",
                        0.0,
                        100.0,
                        camera.amount * 100.0,
                    ),
                    (
                        SettingsSlider::BackgroundCameraSpeed,
                        "bg_camera_speed",
                        10.0,
                        200.0,
                        camera.speed * 100.0,
                    ),
                ] {
                    rows.push(self.appearance_row(
                        &format!("settings_view.terminal.{key}"),
                        "settings_view.terminal.bg_camera_hint",
                        self.appearance_slider_value_control(
                            slider,
                            settings_slider_anchor_id(slider),
                            min,
                            max,
                            value,
                            "%",
                            cx,
                        ),
                    ));
                }
            }
        }
        rows
    }

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
                    self.appearance_action_button(
                        LucideIcon::Image,
                        self.i18n.t(&format!("settings_view.terminal.{key}")),
                        cx.listener(move |_, _, _, cx| {
                            preview.update(cx, |preview, cx| preview.set_preview_hour(hour, cx));
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    )
                    .when(selected == hour, |button| {
                        button.border_color(rgb(self.tokens.ui.accent))
                    })
                }),
            )
            .into_any_element()
    }

    pub(super) fn appearance_edit_scheme(&self) -> Option<bool> {
        self.settings_store
            .settings()
            .appearance
            .follow_system_appearance
            .then_some(self.appearance_edit_dark.unwrap_or(self.system_dark))
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
                settings.resolved_background(self.appearance_edit_dark.unwrap_or(self.system_dark))
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
            let dark = self.appearance_edit_scheme();
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

    pub(super) fn appearance_background_alignment_control(
        &self,
        selected: BackgroundAlignment,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let labels = [
            "bg_align_top_left",
            "bg_align_top",
            "bg_align_top_right",
            "bg_align_left",
            "bg_align_center",
            "bg_align_right",
            "bg_align_bottom_left",
            "bg_align_bottom",
            "bg_align_bottom_right",
        ];
        self.appearance_row(
            "settings_view.terminal.bg_alignment",
            "settings_view.terminal.bg_alignment_hint",
            div()
                .grid()
                .grid_cols(3)
                .gap(px(self.tokens.spacing.one))
                .children(BackgroundAlignment::ALL.into_iter().zip(labels).map(
                    |(alignment, key)| {
                        self.appearance_action_button(
                            LucideIcon::Image,
                            self.i18n.t(&format!("settings_view.terminal.{key}")),
                            cx.listener(move |this, _, _, cx| {
                                this.edit_background_style(|style| style.alignment = alignment, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .when(selected == alignment, |button| {
                            button.border_color(rgb(self.tokens.ui.accent))
                        })
                    },
                ))
                .into_any_element(),
        )
    }
}
