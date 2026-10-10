use super::*;

pub(in crate::workspace) const THEME_EDITOR_MODAL_WIDTH: f32 = 672.0; // Tauri ThemeEditorModal max-w-2xl.
pub(in crate::workspace) const THEME_EDITOR_MODAL_MAX_HEIGHT: f32 = 760.0; // Tauri max-h-[85vh] on the default native window.
pub(in crate::workspace) const THEME_EDITOR_HEADER_PADDING_X: f32 = 16.0; // DialogHeader px-4.
pub(in crate::workspace) const THEME_EDITOR_HEADER_PADDING_Y: f32 = 12.0; // DialogHeader py-3.
pub(in crate::workspace) const THEME_EDITOR_BODY_PADDING_X: f32 = 16.0; // Body px-4.
pub(in crate::workspace) const THEME_EDITOR_BODY_PADDING_Y: f32 = 12.0; // Body py-3.
pub(in crate::workspace) const THEME_EDITOR_BODY_GAP: f32 = 16.0; // Tauri space-y-4.
pub(in crate::workspace) const THEME_EDITOR_INPUT_HEIGHT: f32 = 32.0; // Tauri Input h-8.
pub(in crate::workspace) const THEME_EDITOR_DUPLICATE_WIDTH: f32 = 180.0; // Tauri duplicate select w-[180px].

const BACKGROUND_SCOPE_OPTIONS: [(BackgroundScope, &str); 2] = [
    (
        BackgroundScope::Content,
        "settings_view.terminal.bg_scope_content",
    ),
    (
        BackgroundScope::Window,
        "settings_view.terminal.bg_scope_window",
    ),
];

// Persisted opacity keeps 0.1% steps so the slider's expanded near-opaque
// range is not collapsed back into whole-percent jumps.
const WINDOW_OPACITY_STEPS_PER_UNIT: f64 = 1000.0;

/// Maps window opacity to slider travel in `0.0..=1.0`.
///
/// Compositors blend window alpha linearly, but on a dark theme over bright
/// content the first few percent of transparency are already very visible.
/// Travel is quadratic in transparency so most of the slider covers the
/// near-opaque range.
fn window_opacity_slider_position(opacity: f64) -> f32 {
    let transparency = ((MAX_WINDOW_OPACITY - opacity) / (MAX_WINDOW_OPACITY - MIN_WINDOW_OPACITY))
        .clamp(0.0, 1.0);
    (1.0 - transparency.sqrt()) as f32
}

pub(super) fn window_opacity_from_slider_position(position: f32) -> f64 {
    let travel_from_opaque = 1.0 - f64::from(position.clamp(0.0, 1.0));
    let opacity =
        MAX_WINDOW_OPACITY - (MAX_WINDOW_OPACITY - MIN_WINDOW_OPACITY) * travel_from_opaque.powi(2);
    (opacity * WINDOW_OPACITY_STEPS_PER_UNIT).round() / WINDOW_OPACITY_STEPS_PER_UNIT
}

fn window_opacity_label(opacity: f64) -> String {
    let percent = format!("{:.1}", opacity * SETTINGS_PERCENT_SCALE);
    let percent = percent.strip_suffix(".0").unwrap_or(&percent);
    format!("{percent}%")
}

fn background_scope_index(scope: BackgroundScope) -> usize {
    BACKGROUND_SCOPE_OPTIONS
        .iter()
        .position(|(option, _)| *option == scope)
        .unwrap_or(0)
}

pub(super) fn appearance_theme_palette(settings: &PersistedSettings, id: &str) -> TerminalTheme {
    let mut palette = theme_by_id(id).terminal;
    let Some(colors) = settings
        .custom_themes
        .get(id)
        .and_then(|theme| theme.get("terminalColors"))
    else {
        return palette;
    };
    // Preview only needs RGB values. The full runtime parser allocates static
    // selection-color strings, which must not be repeated on hover repaint.
    for (key, target) in [
        ("background", &mut palette.background),
        ("foreground", &mut palette.foreground),
        ("cursor", &mut palette.cursor),
        ("red", &mut palette.red),
        ("green", &mut palette.green),
        ("yellow", &mut palette.yellow),
        ("blue", &mut palette.blue),
        ("magenta", &mut palette.magenta),
        ("cyan", &mut palette.cyan),
    ] {
        if let Some(color) = colors
            .get(key)
            .and_then(serde_json::Value::as_str)
            .and_then(parse_color_hex)
        {
            *target = color;
        }
    }
    palette
}

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_appearance_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        match section_index {
            0 => self.appearance_theme_card(settings, cx),
            1 => self.appearance_layout_card(settings, cx),
            2 => self.appearance_effects_card(settings, cx),
            3 => self.appearance_background_card(&self.background_settings_for_controls(cx), cx),
            4 => self.appearance_app_icon_card(settings, cx),
            _ => div().into_any_element(),
        }
    }

    pub(in crate::workspace) fn appearance_theme_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut rows = self.appearance_theme_controls(settings, cx);
        rows.push(self.appearance_theme_preview(settings, cx));
        self.appearance_card(self.i18n.t("settings_view.appearance.theme"), None, rows)
    }

    pub(in crate::workspace) fn appearance_theme_controls(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let follow = settings.appearance.follow_system_appearance;
        let mut rows = vec![
            div()
                .w_full()
                .flex()
                .flex_col()
                .child(
                    self.appearance_row(
                        "settings_view.appearance.follow_system",
                        "settings_view.appearance.follow_system_hint",
                        checkbox(&self.tokens, String::new(), follow)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    this.close_settings_select();
                                    this.blur_text_inputs(cx);
                                    this.finish_settings_slider_drag(cx);
                                    this.settings_workspace.update(cx, |settings, cx| {
                                        settings.finish_background_blur_drag(cx)
                                    });
                                    this.appearance_edit_dark =
                                        (!follow).then_some(this.system_dark);
                                    this.edit_settings(
                                        |settings| {
                                            if !follow {
                                                let style = settings.terminal.background_style();
                                                settings
                                                    .terminal
                                                    .system_backgrounds
                                                    .light
                                                    .get_or_insert_with(|| style.clone());
                                                settings
                                                    .terminal
                                                    .system_backgrounds
                                                    .dark
                                                    .get_or_insert(style);
                                            }
                                            settings.appearance.follow_system_appearance = !follow;
                                        },
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            )
                            .into_any_element(),
                    ),
                )
                .child(oxideterm_gpui_ui::motion::auto_height(
                    &self.tokens,
                    "theme-scheme-reveal",
                    follow.then(|| {
                        div()
                            .pt(px(self.tokens.spacing.three))
                            .child(self.appearance_scheme_control("theme-scheme", cx))
                            .into_any_element()
                    }),
                ))
                .into_any_element(),
        ];
        rows.extend([
            self.appearance_theme_target_row(settings, ThemeTarget::Application, cx),
            self.appearance_theme_target_row(settings, ThemeTarget::Terminal, cx),
        ]);
        rows
    }

    fn appearance_scheme_control(&self, id: &'static str, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.appearance_edit_scheme().unwrap_or(self.system_dark);
        let system_key = if self.system_dark {
            "settings_view.appearance.system_dark"
        } else {
            "settings_view.appearance.system_light"
        };
        div()
            .id(id)
            .w_full()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        self.i18n
                            .t("settings_view.appearance.system_current_scheme")
                            .replace("{{scheme}}", &self.i18n.t(system_key)),
                    ),
            )
            .child(
                div().flex().gap(px(self.tokens.spacing.two)).children(
                    [
                        (false, "settings_view.appearance.system_light"),
                        (true, "settings_view.appearance.system_dark"),
                    ]
                    .map(|(dark, key)| {
                        oxideterm_gpui_ui::tabs::tabs_trigger(
                            &self.tokens,
                            self.i18n.t(key),
                            selected == dark,
                        )
                        .id(("appearance-scheme", u64::from(dark)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_settings_select();
                            this.blur_text_inputs(cx);
                            this.finish_settings_slider_drag(cx);
                            this.appearance_edit_dark = Some(dark);
                            cx.notify();
                            cx.stop_propagation();
                        }))
                    }),
                ),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.appearance.scheme_edit_hint")),
            )
            .into_any_element()
    }

    fn appearance_theme_target_row(
        &self,
        settings: &PersistedSettings,
        target: ThemeTarget,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (title_key, hint_key, select) = match target {
            ThemeTarget::Application => (
                "settings_view.appearance.application_theme",
                "settings_view.appearance.application_theme_hint",
                SettingsSelect::AppearanceTheme,
            ),
            ThemeTarget::Terminal => (
                "settings_view.appearance.terminal_theme",
                "settings_view.appearance.color_theme_hint",
                SettingsSelect::AppearanceTerminalTheme,
            ),
        };
        let scheme = self.appearance_edit_scheme();
        let selected_id = target.resolved_id(settings, scheme.unwrap_or(self.system_dark));
        let select = match (target, scheme) {
            (ThemeTarget::Application, Some(false)) => SettingsSelect::AppearanceThemeLight,
            (ThemeTarget::Application, Some(true)) => SettingsSelect::AppearanceThemeDark,
            (ThemeTarget::Terminal, Some(false)) => SettingsSelect::AppearanceTerminalThemeLight,
            (ThemeTarget::Terminal, Some(true)) => SettingsSelect::AppearanceTerminalThemeDark,
            (_, None) => select,
        };
        self.appearance_row(
            title_key,
            hint_key,
            div()
                .w(px(self.tokens.metrics.settings_select_width))
                .flex_none()
                .flex()
                .flex_col()
                .items_end()
                .gap(px(self.tokens.spacing.two))
                .child(self.appearance_select_control(
                    select,
                    custom_theme_display_name(settings, selected_id),
                    self.tokens.metrics.settings_select_width,
                    cx,
                ))
                .when(!self.onboarding.open, |control| {
                    control.child(
                        div()
                            .w_full()
                            .flex()
                            .flex_wrap()
                            .justify_end()
                            .items_center()
                            .gap(px(self.tokens.spacing.two))
                            .child(self.appearance_action_button(
                                LucideIcon::Upload,
                                self.i18n.t("settings_view.appearance.theme_import"),
                                cx.listener(move |this, _, _, cx| {
                                    this.import_theme_from_file(target, cx);
                                    cx.stop_propagation();
                                }),
                            ))
                            .when(is_custom_theme_id(selected_id), |actions| {
                                actions.child(self.appearance_action_button(
                                    LucideIcon::Pencil,
                                    self.i18n.t("settings_view.custom_theme.edit"),
                                    cx.listener(move |this, _, _, cx| {
                                        let dark = this
                                            .appearance_edit_scheme()
                                            .unwrap_or(this.system_dark);
                                        let id = target
                                            .resolved_id(this.settings_store.settings(), dark)
                                            .to_string();
                                        this.open_theme_editor(target, Some(id), cx);
                                        cx.stop_propagation();
                                    }),
                                ))
                            })
                            .child(self.appearance_action_button(
                                LucideIcon::Plus,
                                self.i18n.t("settings_view.custom_theme.create"),
                                cx.listener(move |this, _, _, cx| {
                                    this.open_theme_editor(target, None, cx);
                                    cx.stop_propagation();
                                }),
                            )),
                    )
                })
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn appearance_layout_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_card(
            self.i18n.t("settings_view.appearance.layout"),
            None,
            vec![
                self.appearance_row(
                    "settings_view.appearance.density",
                    "settings_view.appearance.density_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceDensity,
                        density_label(settings.appearance.ui_density, &self.i18n),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.appearance.border_radius",
                    "settings_view.appearance.border_radius_hint",
                    self.appearance_radius_control(settings, cx),
                ),
                self.appearance_row(
                    "settings_view.appearance.ui_font",
                    "settings_view.appearance.ui_font_hint",
                    self.appearance_text_input_control(
                        SettingsInput::AppearanceUiFont,
                        settings.appearance.ui_font_family.clone(),
                        self.i18n.t("settings_view.appearance.ui_font_placeholder"),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.appearance.ui_font_size",
                    "settings_view.appearance.ui_font_size_hint",
                    self.appearance_slider_value_control(
                        SettingsSlider::AppearanceUiFontSize,
                        SelectAnchorId::SettingsAppearanceUiFontSizeSlider,
                        APPEARANCE_UI_FONT_SIZE_MIN,
                        APPEARANCE_UI_FONT_SIZE_MAX,
                        settings.appearance.ui_font_size as f32,
                        "px",
                        cx,
                    ),
                ),
            ]
            .into_iter()
            .chain(cfg!(target_os = "linux").then(|| {
                self.appearance_window_titlebar_row(settings.appearance.show_window_titlebar, cx)
            }))
            .collect(),
        )
    }

    pub(in crate::workspace) fn appearance_effects_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_card(
            self.i18n.t("settings_view.appearance.effects"),
            None,
            vec![
                self.appearance_row(
                    "settings_view.appearance.window_opacity",
                    "settings_view.appearance.window_opacity_hint",
                    self.appearance_slider_labeled_control(
                        SettingsSlider::AppearanceWindowOpacity,
                        SelectAnchorId::SettingsAppearanceWindowOpacitySlider,
                        0.0,
                        1.0,
                        window_opacity_slider_position(settings.appearance.window_opacity),
                        window_opacity_label(settings.appearance.window_opacity),
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.appearance.animation",
                    "settings_view.appearance.animation_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceAnimation,
                        animation_label(settings.appearance.animation_speed, &self.i18n),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.appearance.render_profile",
                    "settings_view.appearance.render_profile_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceRenderProfile,
                        render_profile_label(settings.appearance.render_profile, &self.i18n),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.appearance.frosted_glass",
                    "settings_view.appearance.frosted_glass_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceFrostedGlass,
                        frosted_glass_label(settings.appearance.frosted_glass, &self.i18n),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
            ]
            .into_iter()
            .chain(std::iter::once(self.appearance_vibrancy_status(settings)))
            .collect(),
        )
    }

    pub(in crate::workspace) fn appearance_app_icon_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_card_with_icon(
            LucideIcon::AppWindow,
            self.i18n.t("settings_view.appearance.app_icon"),
            vec![self.appearance_app_icon_row(settings.appearance.app_icon, cx)],
        )
    }

    pub(in crate::workspace) fn appearance_app_icon_row(
        &self,
        selected: AppIconVariant,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // The icon picker can wrap into multiple rows. Keep its label above the
        // grid so narrow settings panes do not squeeze localized copy vertically.
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.appearance.app_icon_variant")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(
                                self.i18n
                                    .t("settings_view.appearance.app_icon_variant_hint"),
                            ),
                    ),
            )
            .child(self.appearance_app_icon_picker(selected, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn appearance_app_icon_picker(
        &self,
        selected: AppIconVariant,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut picker = div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_start()
            .gap(px(10.0))
            .min_w(px(0.0))
            .flex_wrap();

        for variant in crate::app_icon::APP_ICON_VARIANTS {
            picker =
                picker.child(self.appearance_app_icon_option(*variant, *variant == selected, cx));
        }

        picker.into_any_element()
    }

    pub(in crate::workspace) fn appearance_app_icon_option(
        &self,
        variant: AppIconVariant,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let icon_path = crate::app_icon::app_icon_variant_resource_path(variant);
        let border_color = if selected {
            self.tokens.ui.accent
        } else {
            self.tokens.ui.border
        };
        let image = img(icon_path)
            .size(px(42.0))
            .object_fit(ObjectFit::Contain)
            .rounded(px(self.tokens.radii.md));

        div()
            .relative()
            .size(px(58.0))
            .flex_none()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(border_color))
            .bg(rgb(self.tokens.ui.bg_sunken))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|button| button.bg(rgb(self.tokens.ui.bg_hover)))
            .child(image)
            .when(selected, |button| {
                button.child(
                    div()
                        .absolute()
                        .right(px(4.0))
                        .bottom(px(4.0))
                        .size(px(16.0))
                        .rounded_full()
                        .bg(rgb(self.tokens.ui.accent))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Self::render_lucide_icon(
                            LucideIcon::Check,
                            11.0,
                            rgb(self.tokens.ui.bg),
                        )),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(|settings| settings.appearance.app_icon = variant, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn appearance_background_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let terminal = &settings.terminal;
        let editor_open = self.settings_workspace.read(cx).theme_editor().is_some();
        let has_background =
            terminal.background_image.is_some() || terminal.background_effect.is_some();
        let animated_media = self.settings_workspace.update(cx, |entity, cx| {
            entity.background_media_is_animated(terminal.background_image.as_deref(), cx)
        });
        let mut rows = Vec::new();
        if !editor_open {
            if settings.appearance.follow_system_appearance {
                rows.push(self.appearance_scheme_control("background-scheme", cx));
            }
            let preview = self.settings_workspace.read(cx).background_preview.clone();
            rows.push(self.appearance_preview(self.settings_store.settings(), preview, cx));
        }
        if has_background {
            let enabled = terminal.background_enabled;
            rows.push(
                self.appearance_row(
                    "settings_view.terminal.bg_enabled",
                    "settings_view.terminal.bg_enabled_hint",
                    checkbox(&self.tokens, String::new(), enabled)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.edit_background_style(|style| style.enabled = !enabled, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .into_any_element(),
                ),
            );
        }
        rows.push(self.appearance_background_image_slot(settings, cx));
        if terminal.background_image.is_some() {
            let blur = self
                .settings_workspace
                .read(cx)
                .background_blur_preview(self.appearance_edit_scheme())
                .unwrap_or(terminal.background_blur);
            rows.extend([
                self.appearance_row(
                    "settings_view.terminal.bg_opacity",
                    "settings_view.terminal.bg_opacity_hint",
                    self.appearance_slider_value_control(
                        SettingsSlider::AppearanceBackgroundOpacity,
                        SelectAnchorId::SettingsAppearanceBackgroundOpacitySlider,
                        (MIN_TERMINAL_BACKGROUND_OPACITY * SETTINGS_PERCENT_SCALE) as f32,
                        (MAX_TERMINAL_BACKGROUND_OPACITY * SETTINGS_PERCENT_SCALE) as f32,
                        (terminal.background_opacity * SETTINGS_PERCENT_SCALE).round() as f32,
                        "%",
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.terminal.bg_blur",
                    "settings_view.terminal.bg_blur_hint",
                    self.appearance_slider_value_control(
                        SettingsSlider::AppearanceBackgroundBlur,
                        SelectAnchorId::SettingsAppearanceBackgroundBlurSlider,
                        0.0,
                        20.0,
                        blur as f32,
                        "px",
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.terminal.bg_fit",
                    "settings_view.terminal.bg_fit_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceBackgroundFit,
                        background_fit_label(terminal.background_fit, &self.i18n),
                        self.tokens.metrics.settings_appearance_fit_select_width,
                        cx,
                    ),
                ),
                self.appearance_background_alignment_control(terminal.background_alignment, cx),
            ]);
            for (input, key, hint, value) in [
                (
                    SettingsInput::BackgroundMaxWidth,
                    "bg_max_width",
                    "bg_dimensions_hint",
                    terminal.background_max_width,
                ),
                (
                    SettingsInput::BackgroundMaxHeight,
                    "bg_max_height",
                    "bg_dimensions_hint",
                    terminal.background_max_height,
                ),
                (
                    SettingsInput::BackgroundMaxFps,
                    "bg_max_fps",
                    "bg_media_fps_hint",
                    terminal.background_max_fps,
                ),
            ]
            .into_iter()
            .filter(|(input, _, _, _)| animated_media || *input != SettingsInput::BackgroundMaxFps)
            {
                rows.push(self.appearance_row(
                    &format!("settings_view.terminal.{key}"),
                    &format!("settings_view.terminal.{hint}"),
                    self.appearance_text_input_control(
                        input,
                        value.map(|value| value.to_string()).unwrap_or_default(),
                        self.i18n.t("settings_view.terminal.bg_auto"),
                        112.0,
                        cx,
                    ),
                ));
            }
        }
        rows.push(separator(&self.tokens, SeparatorOrientation::Horizontal).into_any_element());
        rows.push(settings_appearance_card_title(
            &self.tokens,
            self.i18n.t("settings_view.terminal.bg_effects"),
            None,
        ));
        use oxideterm_settings::GeneratedBackgroundKind;
        rows.push(
            div()
                .flex()
                .flex_wrap()
                .gap(px(self.tokens.spacing.two))
                .children(
                    [
                        (None, "settings_view.terminal.bg_effect_none"),
                        (
                            Some(GeneratedBackgroundKind::Mineral),
                            "settings_view.terminal.bg_source_mineral",
                        ),
                        (
                            Some(GeneratedBackgroundKind::Fog),
                            "settings_view.terminal.bg_source_fog",
                        ),
                        (
                            Some(GeneratedBackgroundKind::Tide),
                            "settings_view.terminal.bg_source_tide",
                        ),
                        (
                            Some(GeneratedBackgroundKind::Meteor),
                            "settings_view.terminal.bg_source_meteor",
                        ),
                        (
                            Some(GeneratedBackgroundKind::Particles),
                            "settings_view.terminal.bg_source_particles",
                        ),
                        (
                            Some(GeneratedBackgroundKind::Caustics),
                            "settings_view.terminal.bg_source_caustics",
                        ),
                    ]
                    .map(|(kind, key)| {
                        self.appearance_action_button(
                            LucideIcon::Sparkles,
                            self.i18n.t(key),
                            cx.listener(move |this, _, _, cx| {
                                this.edit_background_style(
                                    |style| {
                                        if let Some(kind) = kind {
                                            style
                                                .effect
                                                .get_or_insert_with(Default::default)
                                                .kind = kind;
                                        } else {
                                            style.effect = None;
                                        }
                                    },
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        )
                        .when(
                            kind == terminal
                                .background_effect
                                .as_ref()
                                .map(|effect| effect.kind),
                            |button| button.border_color(rgb(self.tokens.ui.accent)),
                        )
                    }),
                )
                .into_any_element(),
        );
        if let Some(effect) = &terminal.background_effect {
            rows.extend(self.appearance_background_effect_rows(effect, cx));
        }
        if has_background {
            rows.push(separator(&self.tokens, SeparatorOrientation::Horizontal).into_any_element());
            rows.extend(self.appearance_background_interaction_rows(settings, animated_media, cx));
            rows.push(separator(&self.tokens, SeparatorOrientation::Horizontal).into_any_element());
            rows.push(self.appearance_row(
                "settings_view.terminal.bg_readability",
                "settings_view.terminal.bg_readability_hint",
                self.appearance_slider_value_control(
                    SettingsSlider::BackgroundReadability,
                    SelectAnchorId::SettingsBackgroundReadability,
                    0.0,
                    100.0,
                    terminal.background_readability * 100.0,
                    "%",
                    cx,
                ),
            ));
            rows.push(self.appearance_row(
                "settings_view.terminal.bg_scope",
                "settings_view.terminal.bg_scope_hint",
                self.appearance_background_scope_control(terminal.background_scope, cx),
            ));
            if terminal.background_scope == BackgroundScope::Content {
                rows.push(self.appearance_background_tabs(settings, cx));
            }
        }
        self.appearance_card_with_icon(
            LucideIcon::Image,
            self.i18n.t("settings_view.terminal.bg_title"),
            rows,
        )
    }

    fn appearance_background_effect_rows(
        &self,
        effect: &oxideterm_settings::GeneratedBackgroundSettings,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        use oxideterm_settings::GeneratedBackgroundKind;
        let (size_key, brightness_key, hint_key) = match effect.kind {
            GeneratedBackgroundKind::Mineral => {
                ("bg_mineral_scale", "bg_mineral_sheen", "bg_mineral_hint")
            }
            GeneratedBackgroundKind::Fog => ("bg_fog_size", "bg_fog_concentration", "bg_fog_hint"),
            GeneratedBackgroundKind::Tide => ("bg_tide_width", "bg_fog_brightness", "bg_tide_hint"),
            GeneratedBackgroundKind::Meteor => {
                ("bg_meteor_length", "bg_fog_brightness", "bg_meteor_hint")
            }
            GeneratedBackgroundKind::Particles => (
                "bg_particles_size",
                "bg_fog_brightness",
                "bg_particles_hint",
            ),
            GeneratedBackgroundKind::Caustics => {
                ("bg_caustics_scale", "bg_fog_brightness", "bg_caustics_hint")
            }
        };
        let mut rows = vec![self.appearance_row(
            "settings_view.terminal.bg_effect_strength",
            "settings_view.terminal.bg_effect_strength_hint",
            self.appearance_slider_value_control(
                SettingsSlider::BackgroundEffectStrength,
                SelectAnchorId::SettingsBackgroundEffectStrength,
                0.0,
                100.0,
                effect.strength * 100.0,
                "%",
                cx,
            ),
        )];
        let brightness_slider = if effect.kind == GeneratedBackgroundKind::Mineral {
            SettingsSlider::BackgroundEffectSheen
        } else {
            SettingsSlider::BackgroundEffectBrightness
        };
        for (slider, key, min, max, value) in [
            (
                SettingsSlider::BackgroundEffectSize,
                size_key,
                30.0,
                200.0,
                effect.size * 100.0,
            ),
            (
                brightness_slider,
                brightness_key,
                0.0,
                100.0,
                if effect.kind == GeneratedBackgroundKind::Mineral {
                    effect.sheen
                } else {
                    effect.brightness
                } * 100.0,
            ),
            (
                SettingsSlider::BackgroundEffectSpeed,
                "bg_fog_speed",
                0.0,
                300.0,
                effect.speed * 100.0,
            ),
        ]
        .into_iter()
        .filter(|(slider, _, _, _, _)| {
            effect.has_motion() || *slider != SettingsSlider::BackgroundEffectSpeed
        }) {
            rows.push(self.appearance_row(
                &format!("settings_view.terminal.{key}"),
                &format!("settings_view.terminal.{hint_key}"),
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
        let detail = match effect.kind {
            GeneratedBackgroundKind::Mineral => Some((
                SettingsSlider::BackgroundEffectRoughness,
                "bg_mineral_roughness",
                0.0,
                100.0,
                effect.roughness * 100.0,
                "%",
            )),
            GeneratedBackgroundKind::Tide | GeneratedBackgroundKind::Meteor => Some((
                SettingsSlider::BackgroundEffectDirection,
                "bg_tide_direction",
                0.0,
                360.0,
                effect.direction,
                "°",
            )),
            GeneratedBackgroundKind::Particles => Some((
                SettingsSlider::BackgroundParticleCount,
                "bg_particles_count",
                4.0,
                24.0,
                effect.particle_count as f32,
                "",
            )),
            GeneratedBackgroundKind::Fog | GeneratedBackgroundKind::Caustics => None,
        };
        if let Some((slider, key, min, max, value, unit)) = detail {
            rows.push(self.appearance_row(
                &format!("settings_view.terminal.{key}"),
                &format!("settings_view.terminal.{hint_key}"),
                self.appearance_slider_value_control(
                    slider,
                    settings_slider_anchor_id(slider),
                    min,
                    max,
                    value,
                    unit,
                    cx,
                ),
            ));
        }
        for index in 0..2 {
            rows.push(
                self.appearance_row(
                    &format!("settings_view.terminal.bg_fog_color_{}", index + 1),
                    "settings_view.terminal.bg_fog_colors_hint",
                    self.appearance_text_input_control(
                        SettingsInput::BackgroundEffectColor(index),
                        effect
                            .colors
                            .map(|colors| format!("#{:06x}", colors[index]))
                            .unwrap_or_default(),
                        self.i18n.t("settings_view.terminal.bg_auto"),
                        112.0,
                        cx,
                    ),
                ),
            );
        }
        if effect.has_motion() {
            rows.push(
                self.appearance_row(
                    "settings_view.terminal.bg_effect_max_fps",
                    "settings_view.terminal.bg_effect_fps_hint",
                    self.appearance_text_input_control(
                        SettingsInput::BackgroundEffectMaxFps,
                        effect
                            .max_fps
                            .map(|value| value.to_string())
                            .unwrap_or_default(),
                        self.i18n.t("settings_view.terminal.bg_auto"),
                        112.0,
                        cx,
                    ),
                ),
            );
        }
        rows
    }

    pub(in crate::workspace) fn appearance_background_scope_control(
        &self,
        selected: BackgroundScope,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_index = background_scope_index(selected);
        let control_id = selection_motion::APPEARANCE_BACKGROUND_SCOPE_SWITCHER_ID;
        let previous_index = self
            .segmented_control_user_previous_index(control_id, active_index)
            .unwrap_or(active_index);
        let transition_generation = self
            .segmented_control_user_transition(control_id, active_index)
            .map(|(generation, _)| generation);
        let item_width = 1.0 / BACKGROUND_SCOPE_OPTIONS.len() as f32;
        let active_left = active_index as f32 * item_width;
        let previous_left = previous_index as f32 * item_width;
        let indicator = div()
            .absolute()
            .top_0()
            .bottom_0()
            .w(relative(item_width))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((self.tokens.ui.accent << 8) | 0x24));
        let indicator = match (
            transition_generation,
            oxideterm_gpui_ui::segmented_control_motion(&self.tokens),
        ) {
            (Some(generation), Some(motion)) if motion.spatial => indicator
                .with_animation(
                    (
                        gpui::ElementId::from(control_id),
                        format!("selection-{generation}"),
                    ),
                    Animation::new(motion.duration)
                        .with_easing(oxideterm_gpui_ui::motion::ease_in_out_cubic),
                    move |indicator, progress| {
                        indicator.left(relative(oxideterm_gpui_ui::motion::lerp(
                            previous_left,
                            active_left,
                            progress,
                        )))
                    },
                )
                .into_any_element(),
            (Some(generation), Some(motion)) => indicator
                .left(relative(active_left))
                .with_animation(
                    (
                        gpui::ElementId::from(control_id),
                        format!("selection-{generation}"),
                    ),
                    Animation::new(motion.duration),
                    |indicator, progress| indicator.opacity(progress),
                )
                .into_any_element(),
            _ => indicator.left(relative(active_left)).into_any_element(),
        };
        let mut options = div()
            .relative()
            .grid()
            .grid_cols(BACKGROUND_SCOPE_OPTIONS.len() as u16)
            .child(indicator);
        for (target_index, (scope, label_key)) in
            BACKGROUND_SCOPE_OPTIONS.iter().copied().enumerate()
        {
            let active = scope == selected;
            options = options.child(
                div()
                    .relative()
                    .h(px(28.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.sm))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(if active {
                        self.tokens.ui.accent
                    } else {
                        self.tokens.ui.text_muted
                    }))
                    .cursor_pointer()
                    .hover({
                        let hover = self.tokens.ui.bg_hover;
                        move |segment| segment.bg(rgb(hover))
                    })
                    .child(self.i18n.t(label_key))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if target_index != active_index {
                                this.edit_background_style(|style| style.scope = scope, cx);
                                this.begin_user_segmented_control_transition_from(
                                    control_id,
                                    active_index,
                                    target_index,
                                    cx,
                                );
                            }
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        // The grid keeps both localized labels at equal intrinsic widths while
        // the existing shell remains unchanged around the moving selection.
        div()
            .flex()
            .items_center()
            .p(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_sunken))
            .child(options)
            .into_any_element()
    }

    pub(in crate::workspace) fn appearance_card(
        &self,
        title: String,
        actions: Option<AnyElement>,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.appearance_card_shell(
            settings_appearance_card_header(&self.tokens, title, None, actions),
            rows,
        )
    }

    pub(in crate::workspace) fn appearance_card_with_icon(
        &self,
        icon: LucideIcon,
        title: String,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.appearance_card_shell(
            settings_appearance_card_header(
                &self.tokens,
                title,
                Some(Self::render_lucide_icon(
                    icon,
                    16.0,
                    rgb(self.tokens.ui.text),
                )),
                None,
            ),
            rows,
        )
    }

    pub(in crate::workspace) fn appearance_card_shell(
        &self,
        header: AnyElement,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        settings_appearance_card_shell(
            &self.tokens,
            self.settings_background_active(),
            header,
            rows,
        )
    }

    pub(in crate::workspace) fn appearance_action_button(
        &self,
        icon: LucideIcon,
        label: String,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        // Appearance header actions are Tauri small outline toolbar buttons.
        // Route activation through the workspace Button boundary so browser
        // disabled/loading/focus-visible behavior can stay centralized.
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(
                icon,
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
                height: Some(self.tokens.metrics.settings_appearance_action_height),
                padding_x: Some(10.0),
                font_size: Some(self.tokens.metrics.ui_text_xs),
                background: Some(rgba(0x00000000)),
                border: Some(rgb(self.tokens.ui.border)),
                text_color: Some(rgb(self.tokens.ui.text)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
    }

    pub(in crate::workspace) fn appearance_row(
        &self,
        label_key: &str,
        hint_key: &str,
        control: AnyElement,
    ) -> AnyElement {
        settings_appearance_row(&self.tokens, &self.i18n, label_key, hint_key, control)
    }

    pub(in crate::workspace) fn appearance_vibrancy_status(
        &self,
        settings: &PersistedSettings,
    ) -> AnyElement {
        let mode = effective_vibrancy_mode(settings, &self.render_policy);
        let mode_label = frosted_glass_label(frosted_glass_mode_from_native(mode), &self.i18n);
        let effective_text = format!(
            "{} {}",
            self.i18n
                .t("settings_view.appearance.frosted_glass_effective_mode"),
            mode_label
        );
        let (status_key, status_color) = if !self.render_policy.allow_vibrancy {
            (
                "settings_view.appearance.frosted_glass_status_profile_disabled",
                self.tokens.ui.warning,
            )
        } else if mode == NativeVibrancyMode::Off {
            (
                "settings_view.appearance.frosted_glass_status_off",
                self.tokens.ui.text_muted,
            )
        } else {
            match self.vibrancy_support {
                VibrancySupport::Supported => (
                    "settings_view.appearance.frosted_glass_status_active",
                    self.tokens.ui.success,
                ),
                VibrancySupport::Fallback { .. } => (
                    "settings_view.appearance.frosted_glass_status_fallback",
                    self.tokens.ui.warning,
                ),
                VibrancySupport::Unsupported { .. } => (
                    "settings_view.appearance.frosted_glass_status_unsupported",
                    self.tokens.ui.error,
                ),
            }
        };
        let blur_key = if self.render_policy.allow_background_blur {
            "settings_view.appearance.frosted_glass_dialog_blur_enabled"
        } else {
            "settings_view.appearance.frosted_glass_dialog_blur_disabled"
        };
        let status_row = |color: u32, label: String| {
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_row()
                .items_start()
                .gap(px(8.0))
                .child(
                    div()
                        .mt(px(5.0))
                        .size(px(6.0))
                        .flex_none()
                        .rounded_full()
                        .bg(rgb(color)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .line_height(px(18.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(label),
                )
                .into_any_element()
        };

        // This status block explains the two independent glass layers. They
        // intentionally follow different render-policy gates so low-power mode
        // can keep cheap system material while disabling expensive realtime blur.
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_sunken))
            .px(px(12.0))
            .py(px(10.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(status_row(self.tokens.ui.accent, effective_text))
            .child(status_row(status_color, self.i18n.t(status_key)))
            .child(status_row(self.tokens.ui.text_muted, self.i18n.t(blur_key)))
            .into_any_element()
    }

    fn appearance_window_titlebar_row(&self, checked: bool, cx: &mut Context<Self>) -> AnyElement {
        self.appearance_row(
            "settings_view.appearance.show_window_titlebar",
            "settings_view.appearance.show_window_titlebar_hint",
            checkbox(&self.tokens, String::new(), checked)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.edit_settings(
                            |settings| settings.appearance.show_window_titlebar = !checked,
                            cx,
                        );
                        // Detached windows read the same workspace settings but
                        // need an explicit repaint after their chrome changes.
                        cx.refresh_windows();
                    }),
                )
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn appearance_select_control(
        &self,
        select_id: SettingsSelect,
        value: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .relative()
            .w(px(width))
            .min_w(px(0.0))
            .child(self.settings_select_control(select_id, value, false, Some(width), cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn appearance_text_input_control(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.as_str()
        } else {
            value.as_str()
        };
        let target = WorkspaceImeTarget::Settings(input);
        self.text_input_with_workspace_ime(
            target,
            text_input(
                &self.tokens,
                TextInputView {
                    value: display_value,
                    placeholder,
                    focused,
                    caret_visible: self.input_caret.visible(),
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
            )
            .w(px(width)),
            move |this, cx| {
                let current = this.current_settings_input_value(input, cx);
                this.focus_settings_input(input, current, cx);
            },
            cx,
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn appearance_radius_control(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_appearance_radius_control(
            &self.tokens,
            settings.appearance.border_radius,
            self.appearance_slider_control(
                SettingsSlider::AppearanceBorderRadius,
                SelectAnchorId::SettingsAppearanceBorderRadiusSlider,
                APPEARANCE_BORDER_RADIUS_MIN,
                APPEARANCE_BORDER_RADIUS_MAX,
                settings.appearance.border_radius as f32,
                cx,
            ),
        )
    }

    pub(in crate::workspace) fn appearance_slider_value_control(
        &self,
        slider: SettingsSlider,
        anchor_id: SelectAnchorId,
        min: f32,
        max: f32,
        value: f32,
        unit: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_slider_labeled_control(
            slider,
            anchor_id,
            min,
            max,
            value,
            format!("{}{}", value.round() as i64, unit),
            cx,
        )
    }

    /// Slider whose track position and displayed value use different scales.
    pub(in crate::workspace) fn appearance_slider_labeled_control(
        &self,
        slider: SettingsSlider,
        anchor_id: SelectAnchorId,
        min: f32,
        max: f32,
        value: f32,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .child(self.appearance_slider_control(slider, anchor_id, min, max, value, cx))
            .child(
                div()
                    .w(px(48.0))
                    .text_align(gpui::TextAlign::Right)
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(label),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn appearance_slider_control(
        &self,
        slider_id: SettingsSlider,
        anchor_id: SelectAnchorId,
        min: f32,
        max: f32,
        value: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let workspace = cx.entity();
        div()
            .w(px(self.tokens.metrics.settings_slider_width))
            .child(select_anchor_probe(
                anchor_id,
                slider(
                    &self.tokens,
                    SliderView {
                        min,
                        max,
                        value,
                        disabled: false,
                    },
                )
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                        this.close_settings_select();
                        this.focused_settings_input = None;
                        this.settings_slider_drag = Some(slider_id);
                        this.apply_settings_slider_from_position(
                            slider_id,
                            f32::from(event.position.x),
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                        this.finish_settings_slider_drag(cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(cx.listener(
                    |this, event: &MouseMoveEvent, _window, cx| {
                        this.update_settings_slider_drag(event, cx);
                    },
                )),
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn appearance_theme_preview(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let preview = self
            .settings_workspace
            .read(cx)
            .theme_background_preview
            .clone();
        self.appearance_preview(settings, preview, cx)
    }

    fn appearance_preview(
        &self,
        settings: &PersistedSettings,
        preview: Entity<oxideterm_gpui_background::BackgroundPreview>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let preview_target = self
            .open_settings_select
            .and_then(SettingsSelect::theme_target);
        let preview_dark = self
            .open_settings_select
            .and_then(SettingsSelect::theme_system_dark)
            .unwrap_or(self.appearance_edit_scheme().unwrap_or(self.system_dark));
        let preview_id = |target: ThemeTarget| {
            if preview_target == Some(target) {
                self.settings_theme_preview
                    .as_deref()
                    .unwrap_or(target.resolved_id(settings, preview_dark))
            } else {
                target.resolved_id(settings, preview_dark)
            }
        };
        let application_id = preview_id(ThemeTarget::Application);
        let terminal_id = preview_id(ThemeTarget::Terminal);
        // Resolve both halves locally so hovering never applies draft colors to the workspace.
        let mut preview_tokens = ThemeTokens {
            ui: oxideterm_settings_model::theme_ui_colors(settings, application_id),
            terminal: appearance_theme_palette(settings, terminal_id),
            ..self.tokens
        };
        preview_tokens.refresh_palette_metrics();
        let page = self.settings_workspace.read(cx).theme_preview_page;
        let mut style = settings.resolved_background(preview_dark);
        if let Some(blur) = self
            .settings_workspace
            .read(cx)
            .background_blur_preview(self.appearance_edit_scheme())
        {
            style.blur = blur;
        }
        let page_key = match page {
            ThemePreviewPage::Terminal => "terminal",
            ThemePreviewPage::Sftp => "sftp",
            _ => "session_manager",
        };
        let background = if style.scope == BackgroundScope::Window
            || style.enabled_tabs.iter().any(|tab| tab == page_key)
        {
            background_preferences_for_style(&style, &preview_tokens)
        } else {
            None
        };
        let has_background = background.is_some();
        preview.update(cx, |preview, cx| preview.set_preferences(background, cx));
        div()
            .id(("appearance-preview", preview.entity_id()))
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(settings_appearance_theme_preview(
                &preview_tokens,
                settings,
                custom_theme_display_name(settings, application_id),
                custom_theme_display_name(settings, terminal_id),
                oxideterm_gpui_settings_view::application_theme_description(
                    application_id,
                    preview_tokens.ui,
                    &self.i18n,
                ),
                &self.i18n,
                page,
                has_background.then(|| preview.clone().into_any_element()),
                style.scope,
                {
                    let workspace = self.settings_workspace.downgrade();
                    move |page, _window, cx| {
                        let _ = workspace.update(cx, |settings, cx| {
                            settings.theme_preview_page = page;
                            cx.notify();
                        });
                    }
                },
            ))
            .when(style.day_cycle && has_background, |view| {
                view.child(self.appearance_daylight_preview_control(preview.clone(), cx))
            })
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.appearance.theme_preview_hint")),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_theme_editor_modal(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (editor, editor_phase) = {
            let settings_workspace = self.settings_workspace.read(cx);
            (
                settings_workspace.theme_editor_snapshot()?,
                settings_workspace.theme_editor_phase(),
            )
        };
        let terminal = editor_terminal_theme(&editor.terminal_colors);
        let ui = editor_ui_colors(&editor.ui_colors);
        let title_key = if editor.edit_theme_id.is_some() {
            "settings_view.custom_theme.edit_title"
        } else {
            "settings_view.custom_theme.create_title"
        };
        let save_disabled = editor.name.trim().is_empty();
        let form_visible = editor_phase == oxideterm_gpui_ui::motion::ExitPhase::Visible;

        let dialog = div()
            .w(px(THEME_EDITOR_MODAL_WIDTH))
            .max_h(px(THEME_EDITOR_MODAL_MAX_HEIGHT))
            .rounded(px(self.tokens.radii.md))
            .overflow_hidden()
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            // Header/footer/body each paint to the rounded shell edge. Keeping
            // the shell itself background-free avoids a second corner color
            // when GPUI clips overflow with a rectangular mask.
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex_none()
                    .px(px(THEME_EDITOR_HEADER_PADDING_X))
                    .py(px(THEME_EDITOR_HEADER_PADDING_Y))
                    .border_b_1()
                    .border_color(rgb(self.tokens.ui.border))
                    // Tauri's DialogContent clips this painted header through
                    // the modal radius; mirror that edge ownership in GPUI.
                    .rounded_t(px(rounded_shell_child_radius(self.tokens.radii.md)))
                    .bg(rgb(self.tokens.ui.bg_panel))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_base))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text_heading))
                            .child(self.i18n.t(title_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.custom_theme.description")),
                    ),
            )
            .child(
                div()
                    .id("theme-editor-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .selectable_overflow_y_scrollbar(
                        &self.selectable_text_scroll_handle("theme-editor-scroll"),
                    )
                    .px(px(THEME_EDITOR_BODY_PADDING_X))
                    .py(px(THEME_EDITOR_BODY_PADDING_Y))
                    .bg(rgb(self.tokens.ui.bg_elevated))
                    .flex()
                    .flex_col()
                    .gap(px(THEME_EDITOR_BODY_GAP))
                    .child(self.theme_editor_name_duplicate_row(&editor, cx))
                    .child(self.theme_editor_preview(&editor, terminal, ui, cx))
                    .child(self.theme_editor_section_tabs(&editor, cx))
                    .child(if editor.active_section == ThemeEditorSection::Background {
                        self.appearance_background_card(
                            &self.background_settings_for_controls(cx),
                            cx,
                        )
                    } else {
                        self.theme_editor_color_grid(&editor, cx)
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(THEME_EDITOR_HEADER_PADDING_X))
                    .py(px(THEME_EDITOR_HEADER_PADDING_Y))
                    .border_t_1()
                    .border_color(rgb(self.tokens.ui.border))
                    // The footer background sits on the shell edge, so it must
                    // own the bottom corners instead of relying on rectangular clipping.
                    .rounded_b(px(rounded_shell_child_radius(self.tokens.radii.md)))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .bg(rgb(self.tokens.ui.bg_panel))
                    .child(if editor.edit_theme_id.is_some() {
                        self.theme_editor_footer_button(
                            LucideIcon::Trash2,
                            self.i18n.t("settings_view.custom_theme.delete"),
                            self.tokens.ui.error,
                            cx.listener(|this, _event, _window, cx| {
                                this.delete_theme_editor_theme(cx);
                                cx.stop_propagation();
                            }),
                        )
                        .into_any_element()
                    } else {
                        div().into_any_element()
                    })
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                // ThemeEditorModal uses normal shadcn Button
                                // footer actions in Tauri; route clicks through
                                // the shared workspace guard rather than a raw
                                // primitive-level mouse handler.
                                self.workspace_toolbar_action_button(
                                    self.i18n.t("settings_view.custom_theme.cancel"),
                                    None,
                                    ToolbarButtonOptions {
                                        button: ButtonOptions {
                                            variant: ButtonVariant::Outline,
                                            size: ButtonSize::Sm,
                                            radius: ButtonRadius::Md,
                                            disabled: false,
                                        },
                                        ..ToolbarButtonOptions::default()
                                    },
                                    cx.listener(|this, _event, _window, cx| {
                                        this.close_theme_editor(cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                            )
                            .child(self.workspace_toolbar_action_button(
                                self.i18n.t("settings_view.custom_theme.save"),
                                Some(Self::render_lucide_icon(
                                    LucideIcon::Save,
                                    12.0,
                                    rgb(self.tokens.ui.accent_text),
                                )),
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: save_disabled,
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(|this, _event, _window, cx| {
                                    this.save_theme_editor(cx);
                                    cx.stop_propagation();
                                }),
                            )),
                    ),
            );

        Some(
            dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        // Tauri ThemeEditorModal passes Dialog onOpenChange
                        // directly through, so overlay close cancels editing.
                        this.close_theme_editor(cx);
                        cx.stop_propagation();
                    }),
                )
                .child(oxideterm_gpui_ui::motion::form_transition(
                    &self.tokens,
                    "theme-editor-form-enter",
                    dialog,
                    form_visible,
                ))
                .when(!form_visible, |backdrop| {
                    backdrop.child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .occlude()
                            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                                cx.stop_propagation();
                            })
                            .on_scroll_wheel(|_event, _window, cx| cx.stop_propagation()),
                    )
                })
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn theme_editor_name_duplicate_row(
        &self,
        editor: &ThemeEditorState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_theme_editor_name_duplicate_row(
            self.theme_editor_label("settings_view.custom_theme.name"),
            self.theme_editor_text_input(
                SettingsInput::CustomThemeName,
                &editor.name,
                self.i18n.t("settings_view.custom_theme.name_placeholder"),
                ThemeEditorTextInputKind::Form,
                cx,
            ),
            editor
                .edit_theme_id
                .is_none()
                .then(|| self.theme_editor_duplicate_row(editor, cx)),
        )
    }

    pub(in crate::workspace) fn theme_editor_duplicate_row(
        &self,
        editor: &ThemeEditorState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let value = if editor.duplicate_theme_touched {
            theme_display_name(&editor.duplicate_theme)
        } else {
            self.i18n.t("settings_view.custom_theme.select_base")
        };
        settings_theme_editor_duplicate_row(
            self.theme_editor_label("settings_view.custom_theme.duplicate_from"),
            self.theme_editor_duplicate_select(value, cx),
        )
    }
    pub(in crate::workspace) fn theme_editor_duplicate_select(
        &self,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let select_id = SettingsSelect::CustomThemeDuplicate;
        self.settings_select_control_with_trigger_style(
            select_id,
            value,
            false,
            Some(THEME_EDITOR_DUPLICATE_WIDTH),
            |trigger| trigger.h(px(THEME_EDITOR_INPUT_HEIGHT)),
            cx,
        )
    }

    pub(in crate::workspace) fn theme_editor_preview(
        &self,
        editor: &ThemeEditorState,
        terminal: TerminalTheme,
        ui: AppUiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut tokens = self.tokens;
        tokens.ui = ui;
        tokens.terminal = terminal;
        tokens.refresh_palette_metrics();
        let background = background_preferences_for_style(&editor.background, &tokens);
        let preview = self
            .settings_workspace
            .read(cx)
            .editor_background_preview
            .clone();
        let has_background = background.is_some();
        preview.update(cx, |preview, cx| preview.set_preferences(background, cx));
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(settings_theme_editor_preview(
                &tokens,
                &editor.name,
                terminal,
                ui,
                settings_mono_font_family(self.settings_store.settings()),
                has_background.then(|| preview.clone().into_any_element()),
                editor.background.scope,
            ))
            .when(editor.background.day_cycle && has_background, |view| {
                view.child(self.appearance_daylight_preview_control(preview, cx))
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn theme_editor_section_tabs(
        &self,
        editor: &ThemeEditorState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .border_b_1()
            .border_color(rgb(self.tokens.ui.border))
            .child(self.theme_editor_section_tab(
                ThemeEditorSection::Terminal,
                "settings_view.custom_theme.terminal_colors",
                editor.active_section,
                cx,
            ))
            .child(self.theme_editor_section_tab(
                ThemeEditorSection::Background,
                "settings_view.terminal.bg_title",
                editor.active_section,
                cx,
            ))
            .child(self.theme_editor_section_tab(
                ThemeEditorSection::Ui,
                "settings_view.custom_theme.ui_colors",
                editor.active_section,
                cx,
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn theme_editor_section_tab(
        &self,
        section: ThemeEditorSection,
        label_key: &str,
        active_section: ThemeEditorSection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = section == active_section;
        div()
            .px(px(12.0))
            .py(px(6.0))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(if active {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text_muted
            }))
            .bg(rgba(0x00000000))
            .cursor_pointer()
            .hover(|tab| tab.text_color(rgb(self.tokens.ui.text)))
            .child(div().child(self.i18n.t(label_key)))
            .child(div().h(px(2.0)).w_full().bg(if active {
                rgb(self.tokens.ui.accent)
            } else {
                rgba(0x00000000)
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.blur_text_inputs(cx);
                    this.finish_settings_slider_drag(cx);
                    this.settings_workspace.update(cx, |settings, cx| {
                        settings.select_theme_editor_section(section, cx);
                    });
                    this.close_settings_select();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn theme_editor_color_grid(
        &self,
        editor: &ThemeEditorState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if editor.active_section == ThemeEditorSection::Ui {
            return self.theme_editor_ui_color_sections(editor, cx);
        }

        let (fields, colors, section) = match editor.active_section {
            ThemeEditorSection::Terminal => (
                TERMINAL_THEME_COLOR_FIELDS,
                editor.terminal_colors.as_slice(),
                ThemeEditorSection::Terminal,
            ),
            ThemeEditorSection::Ui | ThemeEditorSection::Background => {
                unreachable!("non-terminal sections render separately")
            }
        };
        self.theme_editor_color_grid_for_fields(fields, colors, section, cx)
    }

    pub(in crate::workspace) fn theme_editor_ui_color_sections(
        &self,
        editor: &ThemeEditorState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = editor.ui_colors.as_slice();

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.custom_theme.ui_colors_hint")),
                    )
                    .child(
                        // Mirrors Tauri ThemeEditorModal's outline Button with
                        // Copy icon, with activation guarded by the shared
                        // workspace Button wrapper.
                        self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.custom_theme.auto_derive"),
                            Some(Self::render_lucide_icon(
                                LucideIcon::Copy,
                                12.0,
                                rgb(self.tokens.ui.text),
                            )),
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Outline,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: false,
                                },
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(|this, _event, _window, cx| {
                                this.settings_workspace.update(cx, |settings, cx| {
                                    settings.derive_theme_editor_ui_colors(cx);
                                });
                                cx.stop_propagation();
                            }),
                        ),
                    ),
            )
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_background",
                &[0, 1, 2, 3, 4, 5, 6, 7],
                colors,
                cx,
            ))
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_text",
                &[8, 9, 10],
                colors,
                cx,
            ))
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_border",
                &[12, 13, 14],
                colors,
                cx,
            ))
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_accent",
                &[15, 16, 17, 18],
                colors,
                cx,
            ))
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_semantic",
                &[19, 20, 21, 22],
                colors,
                cx,
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn theme_editor_ui_section(
        &self,
        title_key: &str,
        indexes: &[usize],
        colors: &[String],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut cells = Vec::new();
        for &index in indexes {
            let Some(field) = UI_THEME_COLOR_FIELDS.get(index) else {
                continue;
            };
            let color = colors
                .get(index)
                .cloned()
                .unwrap_or_else(|| "#000000".to_string());
            cells.push(self.theme_editor_color_cell(
                field,
                color,
                SettingsInput::CustomThemeUiColor(index),
                cx,
            ));
        }

        settings_theme_editor_color_section(&self.tokens, self.i18n.t(title_key), cells)
    }

    pub(in crate::workspace) fn theme_editor_color_grid_for_fields(
        &self,
        fields: &[ThemeColorField],
        colors: &[String],
        section: ThemeEditorSection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut cells = Vec::new();
        for (index, field) in fields.iter().enumerate() {
            let color = colors
                .get(index)
                .cloned()
                .unwrap_or_else(|| "#000000".to_string());
            let input = match section {
                ThemeEditorSection::Terminal => SettingsInput::CustomThemeTerminalColor(index),
                ThemeEditorSection::Ui => SettingsInput::CustomThemeUiColor(index),
                ThemeEditorSection::Background => unreachable!("background has no palette fields"),
            };
            cells.push(self.theme_editor_color_cell(field, color, input, cx));
        }
        settings_theme_editor_color_grid(cells)
    }

    pub(in crate::workspace) fn theme_editor_color_cell(
        &self,
        field: &ThemeColorField,
        color: String,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let parsed = parse_color_hex(&color).unwrap_or(0);
        let focused = self
            .settings_workspace
            .read(cx)
            .settings_entity_focused_input()
            == Some(input);
        let label = self.i18n.t(&format!(
            "settings_view.custom_theme.colors.{}",
            field.label_key
        ));
        let swatch = settings_theme_editor_color_swatch(&self.tokens, parsed)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.focus_settings_input(input, String::new(), cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element();
        let value_control = if focused {
            self.theme_editor_text_input(
                input,
                &color,
                "#RRGGBB".to_string(),
                ThemeEditorTextInputKind::InlineColor,
                cx,
            )
        } else {
            settings_theme_editor_color_value(
                &self.tokens,
                color,
                settings_mono_font_family(self.settings_store.settings()),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.focus_settings_input(input, String::new(), cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
        };
        settings_theme_editor_color_cell(&self.tokens, label, swatch, value_control)
    }

    pub(in crate::workspace) fn theme_editor_label(&self, key: &str) -> AnyElement {
        settings_theme_editor_label(&self.tokens, self.i18n.t(key))
    }

    pub(in crate::workspace) fn theme_editor_text_input(
        &self,
        input: SettingsInput,
        value: &str,
        placeholder: String,
        kind: ThemeEditorTextInputKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings_workspace = self.settings_workspace.read(cx);
        let focused = settings_workspace.settings_entity_focused_input() == Some(input);
        let display_value = settings_workspace
            .settings_entity_input_value(input)
            .unwrap_or(value);
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let control = settings_theme_editor_text_input(
            &self.tokens,
            TextInputView {
                value: display_value,
                placeholder,
                focused,
                caret_visible: self.input_caret.visible(),
                secret: false,
                selected_all: false,
                selected_range: self.ime_selected_range_for_target(target, cx),
                marked_text: self.marked_text_for_target(target, cx),
            },
            kind,
            settings_mono_font_family(self.settings_store.settings()),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                this.focus_settings_input(input, String::new(), cx);
                this.ime_marked_text = None;
                window.focus(&this.focus_handle, cx);
                this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                cx.stop_propagation();
            }),
        )
        .on_mouse_down_out(cx.listener(move |this, _event, _window, cx| {
            // Settings inputs are manually focused rather than native controls.
            // Release this editor when the next pointer press lands elsewhere.
            let is_focused = this
                .settings_workspace
                .read(cx)
                .settings_entity_focused_input()
                == Some(input);
            if is_focused {
                this.blur_text_inputs(cx);
            }
        }))
        .on_mouse_move(
            cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                this.update_ime_selection_drag_from_mouse_move(event, window, cx);
            }),
        );
        text_input_anchor_probe(target.anchor_id(), control, move |anchor, _window, cx| {
            let _ = workspace.update(cx, |this, cx| {
                this.update_text_input_anchor(anchor, cx);
            });
        })
        .into_any_element()
    }

    pub(in crate::workspace) fn theme_editor_footer_button(
        &self,
        icon: LucideIcon,
        label: String,
        color: u32,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        // Theme editor delete uses a color-tinted outline button in Tauri.
        // Keep only the tint local; activation still goes through the shared
        // Button guard used by the rest of the modal footer.
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(icon, 12.0, rgb(color))),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                icon_gap: Some(4.0),
                border: Some(rgba((color << 8) | 0x4d)),
                text_color: Some(rgb(color)),
                hover_background: Some(rgba((color << 8) | 0x1a)),
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
    }

    pub(in crate::workspace) fn open_theme_editor(
        &mut self,
        target: ThemeTarget,
        edit_theme_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let settings = self.settings_store.settings();
        let scheme = self.appearance_edit_scheme();
        let mut initial = settings.clone();
        target.apply(
            &mut initial,
            target
                .resolved_id(settings, scheme.unwrap_or(self.system_dark))
                .to_string(),
        );
        let mut editor = theme_editor_from_settings(
            &initial,
            target,
            edit_theme_id.clone(),
            self.i18n.t("settings_view.custom_theme.new_theme_name"),
        );
        editor.system_dark = scheme;
        editor.background = edit_theme_id
            .as_deref()
            .and_then(|id| settings.custom_themes.get(id))
            .and_then(|theme| theme.get("background"))
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_else(|| settings.terminal.background_for_scheme(scheme));
        self.settings_workspace.update(cx, |settings, cx| {
            settings.open_theme_editor(editor, cx);
        });
        self.close_settings_select();
        self.focused_settings_input = None;
    }

    pub(in crate::workspace) fn close_theme_editor(&mut self, cx: &mut Context<Self>) {
        self.close_settings_select();
        self.focused_settings_input = None;
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.settings_workspace.update(cx, |settings, cx| {
            settings.cancel_theme_editor(delay, cx);
        });
    }

    pub(in crate::workspace) fn save_theme_editor(&mut self, cx: &mut Context<Self>) {
        self.close_settings_select();
        self.focused_settings_input = None;
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.settings_workspace.update(cx, |settings, cx| {
            settings.save_theme_editor(delay, cx);
        });
    }

    pub(in crate::workspace) fn delete_theme_editor_theme(&mut self, cx: &mut Context<Self>) {
        self.close_settings_select();
        self.focused_settings_input = None;
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.settings_workspace.update(cx, |settings, cx| {
            settings.delete_theme_editor(delay, cx);
        });
    }

    pub(in crate::workspace) fn import_theme_from_file(
        &mut self,
        target: ThemeTarget,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.appearance.theme_import"),
            )),
        });
        let selection = async move {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return None;
            };
            paths.into_iter().next()
        };
        let runtime = self.forwarding_runtime.handle().clone();
        let system_dark = self.appearance_edit_scheme();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_theme_import(target, system_dark, selection, runtime, cx);
        });
    }

    pub(in crate::workspace) fn send_settings_notice(
        &self,
        title: String,
        variant: TerminalNoticeVariant,
        cx: &App,
    ) {
        self.push_workspace_notice(
            TerminalNotice {
                title,
                description: None,
                status_text: None,
                progress: None,
                variant,
            },
            cx,
        );
    }

    pub(in crate::workspace) fn appearance_background_image_slot(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let background_images = self
            .settings_workspace
            .read(cx)
            .background_images_snapshot();
        let editor_open = self.settings_workspace.read(cx).theme_editor().is_some();
        let has_removable_gallery_images = !editor_open
            && background_images.iter().any(|image_path| {
                !is_bundled_workspace_background(self.settings_store.path(), Path::new(image_path))
            });
        let actions = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .when(!editor_open, |actions| {
                actions.child(self.appearance_action_button(
                    LucideIcon::Plus,
                    self.i18n.t("settings_view.terminal.bg_add"),
                    cx.listener(|this, _event, _window, cx| {
                        this.pick_background_image(cx);
                        cx.stop_propagation();
                    }),
                ))
            })
            .when(has_removable_gallery_images, |actions| {
                actions.child(
                    settings_background_clear_all_button(
                        &self.tokens,
                        self.i18n.t("settings_view.terminal.bg_clear_all"),
                        Self::render_lucide_icon(
                            LucideIcon::Trash2,
                            14.0,
                            rgb(self.tokens.ui.error),
                        ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.clear_background_image_gallery(cx);
                            cx.stop_propagation();
                        }),
                    ),
                )
            })
            .into_any_element();
        settings_background_gallery(
            &self.tokens,
            self.i18n.t("settings_view.terminal.bg_gallery"),
            actions,
            self.background_image_slot_content(settings, cx),
        )
    }

    pub(in crate::workspace) fn background_image_slot_content(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let background_images = self
            .settings_workspace
            .read(cx)
            .background_images_snapshot();
        if background_images.is_empty() {
            return settings_background_empty_hint(
                &self.tokens,
                self.i18n.t("settings_view.terminal.bg_hint"),
            );
        }

        let current = settings.terminal.background_image.as_deref();
        let thumbnails = background_images
            .iter()
            .map(|image_path| {
                self.background_thumbnail(image_path, current == Some(image_path.as_str()), cx)
            })
            .collect();
        settings_background_thumbnails_layout(thumbnails)
    }

    pub(in crate::workspace) fn pick_background_image(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.terminal.bg_select_title"),
            )),
        });
        let settings_path = self.settings_store.path().to_path_buf();
        let control_settings = self.background_settings_for_controls(cx);
        let system_dark = self.appearance_edit_scheme();
        let current_path = control_settings
            .terminal
            .background_image
            .as_ref()
            .map(PathBuf::from);
        let runtime = self.forwarding_runtime.handle().clone();
        let selection = async move {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return None;
            };
            Some(paths)
        };
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_background_image_import(
                selection,
                settings_path,
                current_path,
                system_dark,
                runtime,
                cx,
            );
        });
    }

    pub(in crate::workspace) fn background_thumbnail(
        &self,
        image_path: &str,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let image_path = image_path.to_string();
        let remove_path = image_path.clone();
        let is_built_in = self.settings_workspace.read(cx).theme_editor().is_some()
            || is_bundled_workspace_background(self.settings_store.path(), Path::new(&image_path));
        let fallback_icon_color = self.tokens.ui.text_muted;
        let thumbnail = settings_background_thumbnail_frame(
            &self.tokens,
            &image_path,
            oxideterm_gpui_background::poster_source(PathBuf::from(&image_path)),
            active,
            self.i18n.t("settings_view.terminal.bg_active"),
            self.i18n.t(&format!(
                "settings_view.terminal.bg_format_{}",
                match Path::new(&image_path)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "jpg" | "jpeg" => "jpeg",
                    "png" => "png",
                    "webp" => "webp",
                    "gif" => "gif",
                    "bmp" => "bmp",
                    "mp4" => "mp4",
                    "m4v" => "m4v",
                    _ => "unknown",
                }
            )),
            move || {
                WorkspaceApp::render_lucide_icon(LucideIcon::Image, 20.0, rgb(fallback_icon_color))
            },
        );
        thumbnail
            .when(!is_built_in, |thumbnail| {
                thumbnail.child(
                    settings_background_thumbnail_remove_button(
                        &self.tokens,
                        Self::render_lucide_icon(LucideIcon::X, 12.0, rgb(self.tokens.ui.text)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.remove_background_image_from_gallery(remove_path.clone(), cx);
                            cx.stop_propagation();
                        }),
                    ),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    let selected_path = image_path.clone();
                    this.edit_background_style(
                        move |style| {
                            style.image = Some(selected_path);
                        },
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn remove_background_image_from_gallery(
        &mut self,
        image_path: String,
        cx: &mut Context<Self>,
    ) {
        let settings_path = self.settings_store.path().to_path_buf();
        let runtime = self.forwarding_runtime.handle().clone();
        let current_path = self
            .background_settings_for_controls(cx)
            .terminal
            .background_image;
        let system_dark = self.appearance_edit_scheme();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.remove_background_image(
                settings_path,
                image_path,
                current_path,
                system_dark,
                runtime,
                cx,
            );
        });
    }

    pub(in crate::workspace) fn clear_background_image_gallery(&mut self, cx: &mut Context<Self>) {
        let settings_path = self.settings_store.path().to_path_buf();
        let runtime = self.forwarding_runtime.handle().clone();
        let current_path = self
            .background_settings_for_controls(cx)
            .terminal
            .background_image;
        let system_dark = self.appearance_edit_scheme();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.clear_background_image_gallery(
                settings_path,
                current_path,
                system_dark,
                runtime,
                cx,
            );
        });
    }

    pub(in crate::workspace) fn appearance_background_tabs(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut pills = Vec::new();
        for (key, label_key, icon) in background_tab_options() {
            let enabled = settings
                .terminal
                .background_enabled_tabs
                .iter()
                .any(|tab| tab == key);
            let key = (*key).to_string();
            pills.push(
                self.background_tab_pill(
                    &key,
                    label_key,
                    settings_background_tab_lucide(*icon),
                    enabled,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.toggle_background_tab(&key, cx);
                    }),
                )
                .into_any_element(),
            );
        }

        settings_background_tabs_section(
            &self.tokens,
            self.i18n.t("settings_view.terminal.bg_tabs"),
            self.i18n.t("settings_view.terminal.bg_tabs_hint"),
            pills,
        )
    }

    pub(in crate::workspace) fn background_tab_pill(
        &self,
        _key: &str,
        label_key: &str,
        icon: LucideIcon,
        enabled: bool,
    ) -> Div {
        let color = if enabled {
            self.tokens.ui.accent
        } else {
            self.tokens.ui.text_muted
        };
        settings_background_tab_pill(
            &self.tokens,
            self.i18n.t(label_key),
            Self::render_lucide_icon(
                icon,
                self.tokens.metrics.settings_background_tab_icon_size,
                rgb(color),
            ),
            enabled,
        )
    }

    pub(in crate::workspace) fn toggle_background_tab(
        &mut self,
        key: &str,
        cx: &mut Context<Self>,
    ) {
        self.edit_background_style(
            |style| {
                if let Some(index) = style.enabled_tabs.iter().position(|tab| tab == key) {
                    style.enabled_tabs.remove(index);
                } else {
                    style.enabled_tabs.push(key.to_string());
                }
            },
            cx,
        );
    }
}

#[cfg(test)]
mod theme_preview_tests {
    use super::*;

    struct Preview {
        page: ThemePreviewPage,
    }

    impl Render for Preview {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let mut tokens = oxideterm_theme::default_tokens();
            tokens.ui.bg = 0x102030;
            tokens.terminal.background = 0x203010;
            tokens.radii.md = 12.0;
            let view = cx.entity().downgrade();
            settings_appearance_theme_preview(
                &tokens,
                &PersistedSettings::default(),
                "Application".into(),
                "Terminal".into(),
                String::new(),
                &I18n::new(oxideterm_i18n::Locale::En),
                self.page,
                None,
                BackgroundScope::Content,
                move |page, _, cx| {
                    view.update(cx, |view, cx| {
                        view.page = page;
                        cx.notify();
                    })
                    .unwrap();
                },
            )
        }
    }

    #[gpui::test]
    fn theme_preview_navigation_switches_samples_and_preserves_rounded_edges(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|_, _| Preview {
            page: ThemePreviewPage::Terminal,
        });
        cx.simulate_resize(gpui::size(px(800.0), px(500.0)));
        for (control, page, content, background) in [
            (
                "theme-preview-connections",
                ThemePreviewPage::Connections,
                "theme-preview-connections-sample",
                0x102030,
            ),
            (
                "theme-preview-new-connection",
                ThemePreviewPage::NewConnection,
                "theme-preview-new-connection-sample",
                0x102030,
            ),
            (
                "theme-preview-sftp",
                ThemePreviewPage::Sftp,
                "theme-preview-sftp-sample",
                0x102030,
            ),
            (
                "theme-preview-terminal-tab",
                ThemePreviewPage::Terminal,
                "theme-preview-terminal-sample",
                0x203010,
            ),
            (
                "theme-preview-sftp-tab",
                ThemePreviewPage::Sftp,
                "theme-preview-sftp-sample",
                0x102030,
            ),
            (
                "theme-preview-terminal",
                ThemePreviewPage::Terminal,
                "theme-preview-terminal-sample",
                0x203010,
            ),
        ] {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let bounds = cx.debug_bounds(control).unwrap();
            cx.simulate_click(bounds.center(), gpui::Modifiers::default());
            assert_eq!(view.read_with(cx, |view, _| view.page), page, "{control}");
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert!(cx.debug_bounds(content).is_some(), "{content}");
            cx.update(|window, _| {
                let quads = window.painted_quads();
                let color: gpui::Hsla = rgb(background).into();
                let sample = quads
                    .iter()
                    .filter(|quad| quad.background.as_solid() == Some(color))
                    .max_by(|a, b| {
                        a.bounds
                            .size
                            .height
                            .partial_cmp(&b.bounds.size.height)
                            .unwrap()
                    })
                    .unwrap();
                assert!(sample.corner_radii.bottom_right.0 > 0.0, "{content}");
                assert!(
                    quads
                        .iter()
                        .all(|quad| quad.border_widths.bottom.0 < 2.0 * window.scale_factor()),
                    "preview tabs must use the shared filled selection style"
                );
            });
        }
    }

    #[test]
    fn custom_theme_preview_reads_all_displayed_colors() {
        let mut settings = PersistedSettings::default();
        settings.terminal.theme = "tokyo-night".into();
        settings.custom_themes.insert(
            "custom:preview".into(),
            serde_json::json!({
                "terminalColors": {
                    "background": "#102030", "foreground": "#e0d0c0", "cursor": "#abcdef",
                    "red": "#aa0000", "green": "#00bb00", "yellow": "#cccc00",
                    "blue": "#0000dd", "magenta": "#ee00ee", "cyan": "#00ffff"
                }
            }),
        );
        let palette = appearance_theme_palette(&settings, "custom:preview");
        assert_eq!(
            [
                palette.background,
                palette.foreground,
                palette.cursor,
                palette.red,
                palette.green,
                palette.yellow,
                palette.blue,
                palette.magenta,
                palette.cyan
            ],
            [
                0x102030, 0xe0d0c0, 0xabcdef, 0xaa0000, 0x00bb00, 0xcccc00, 0x0000dd, 0xee00ee,
                0x00ffff
            ],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_opacity_slider_spends_most_travel_near_opaque() {
        // Linear travel would put 98% at 0.96 and 87.5% at 0.75; the curve
        // must leave the top fifth of the track for the 100%..98% range.
        for (position, opacity, label) in [
            (1.0, 1.0, "100%"),
            (0.9, 0.995, "99.5%"),
            (0.8, 0.98, "98%"),
            (0.5, 0.875, "87.5%"),
            (0.0, 0.5, "50%"),
        ] {
            assert_eq!(
                window_opacity_from_slider_position(position),
                opacity,
                "position={position}"
            );
            assert!(
                (window_opacity_slider_position(opacity) - position).abs() < 1e-6,
                "opacity={opacity}"
            );
            assert_eq!(window_opacity_label(opacity), label);
        }
    }
}
