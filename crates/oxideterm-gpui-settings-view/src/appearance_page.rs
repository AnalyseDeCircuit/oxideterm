use crate::{
    components::{SettingsControls, SettingsWindowDispatch, SettingsWindowRequest},
    surface::nav::render_icon,
    *,
};
use gpui::{
    AnyElement, App, ClickEvent, Div, ImageSource, MouseButton, MouseDownEvent, ObjectFit, Window,
    div, img, prelude::*, px, rgb, rgba,
};
use oxideterm_gpui_platform::vibrancy::{NativeVibrancyMode, VibrancySupport};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_button,
    },
    select::{SelectAnchorId, select_anchor_probe},
    slider::{SliderView, slider},
    switch::switch,
};
use oxideterm_render_policy::EffectiveRenderPolicy;
use oxideterm_settings::{
    AppIconVariant, AppearanceScheme, CornerStyle, MAX_WINDOW_OPACITY, MIN_WINDOW_OPACITY,
    PersistedSettings,
};
use oxideterm_settings_model::{ThemeTarget, custom_theme_display_name, is_custom_theme_id};
use oxideterm_theme::ThemeTokens;
use std::rc::Rc;

mod preview;
pub use preview::{AppearancePreview, background_preferences_for_style};

pub const APPEARANCE_BORDER_RADIUS_MIN: f32 = 0.0;
pub const APPEARANCE_BORDER_RADIUS_MAX: f32 = 16.0;
pub const APPEARANCE_UI_FONT_SIZE_MIN: f32 = 11.0;
pub const APPEARANCE_UI_FONT_SIZE_MAX: f32 = 20.0;
const SETTINGS_PERCENT_SCALE: f64 = 100.0;

#[derive(Clone, Copy)]
pub enum AppearanceAction {
    Mode(Option<AppearanceScheme>),
    Scheme(bool),
    Assign(bool),
    Import(ThemeTarget),
    Edit(ThemeTarget),
    New(ThemeTarget),
    Corner(CornerStyle),
    Icon(AppIconVariant),
    Titlebar(bool),
}

pub struct AppearancePageData {
    pub system_dark: bool,
    pub edit_scheme: Option<bool>,
    pub onboarding: bool,
    pub render_policy: EffectiveRenderPolicy,
    pub vibrancy_mode: NativeVibrancyMode,
    pub vibrancy_support: VibrancySupport,
    pub icon_variants: &'static [AppIconVariant],
    pub icon_size: f32,
    pub icon_source: fn(AppIconVariant) -> ImageSource,
}

impl AppearancePageData {
    pub fn page<'a>(
        self,
        controls: SettingsControls<'a>,
        dispatch: Rc<dyn Fn(AppearanceAction, &mut App)>,
    ) -> AppearancePage<'a> {
        let Self {
            system_dark, edit_scheme, onboarding, render_policy, vibrancy_mode,
            vibrancy_support, icon_variants, icon_size, icon_source,
        } = self;
        AppearancePage {
            controls, system_dark, edit_scheme, onboarding, render_policy, vibrancy_mode,
            vibrancy_support, icon_variants, icon_size, icon_source, dispatch,
        }
    }
}

pub struct AppearancePage<'a> {
    pub controls: SettingsControls<'a>,
    pub system_dark: bool,
    pub edit_scheme: Option<bool>,
    pub onboarding: bool,
    pub render_policy: EffectiveRenderPolicy,
    pub vibrancy_mode: NativeVibrancyMode,
    pub vibrancy_support: VibrancySupport,
    pub icon_variants: &'static [AppIconVariant],
    pub icon_size: f32,
    pub icon_source: fn(AppIconVariant) -> ImageSource,
    pub dispatch: Rc<dyn Fn(AppearanceAction, &mut App)>,
}
impl<'a> std::ops::Deref for AppearancePage<'a> {
    type Target = SettingsControls<'a>;
    fn deref(&self) -> &Self::Target {
        &self.controls
    }
}
impl AppearancePage<'_> {
    pub fn section(
        &self,
        section: oxideterm_settings_model::SettingsSectionId,
        settings: &PersistedSettings,
        preview: Option<AnyElement>,
        cx: &App,
    ) -> AnyElement {
        use oxideterm_settings_model::SettingsSectionId::*;
        match section {
            AppearanceTheme => {
                let mut rows = self.appearance_theme_controls_with_hints(settings, true, cx);
                rows.extend(preview);
                self.appearance_card(self.i18n.t("settings_view.appearance.theme"), None, rows)
            }
            AppearanceLayout => self.appearance_layout_card(settings, cx),
            AppearanceEffects => self.appearance_effects_card(settings, cx),
            AppearanceAppIcon => self.appearance_app_icon_card(settings, cx),
            _ => div().into_any_element(),
        }
    }

    fn press(
        &self,
        action: AppearanceAction,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let dispatch = self.dispatch.clone();
        move |_, _, cx| dispatch(action, cx)
    }
    fn click(
        &self,
        action: AppearanceAction,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + use<> {
        let dispatch = self.dispatch.clone();
        move |_, _, cx| dispatch(action, cx)
    }
    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        toolbar_button(self.tokens, label, icon, options).on_mouse_down(MouseButton::Left, press)
    }
    fn appearance_slider_control(
        &self,
        id: SettingsSlider,
        anchor: SelectAnchorId,
        min: f32,
        max: f32,
        value: f32,
        _: &App,
    ) -> AnyElement {
        render_appearance_slider(
            self.tokens,
            id,
            anchor,
            min,
            max,
            value,
            self.controls.dispatch.clone(),
        )
    }
    pub fn appearance_theme_controls_with_hints(
        &self,
        settings: &PersistedSettings,
        show_hints: bool,
        cx: &App,
    ) -> Vec<AnyElement> {
        use oxideterm_settings::AppearanceScheme;
        let follow = settings.appearance.follow_system_appearance;
        let modes = oxideterm_gpui_ui::tabs::tabs_list(&self.tokens)
            .h_auto()
            .flex_wrap()
            .children(
                [
                    (
                        Some(AppearanceScheme::Light),
                        "settings_view.appearance.mode_light",
                    ),
                    (
                        Some(AppearanceScheme::Dark),
                        "settings_view.appearance.mode_dark",
                    ),
                    (None, "settings_view.appearance.follow_system"),
                ]
                .map(|(scheme, key)| {
                    let selected = match scheme {
                        None => follow,
                        Some(scheme) => {
                            !follow && settings.appearance.manual_scheme == Some(scheme)
                        }
                    };
                    oxideterm_gpui_ui::tabs::tabs_trigger(&self.tokens, self.i18n.t(key), selected)
                        .on_mouse_down(
                            MouseButton::Left,
                            self.press(AppearanceAction::Mode(scheme)),
                        )
                }),
            );
        let mut rows = vec![self.appearance_row(
            "settings_view.appearance.mode",
            if show_hints {
                "settings_view.appearance.mode_hint"
            } else {
                ""
            },
            modes.into_any_element(),
        )];
        if !follow && settings.appearance.manual_scheme.is_none() {
            rows.push(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.appearance.legacy_scheme_hint"))
                    .into_any_element(),
            );
        }
        rows.extend([
            self.appearance_scheme_control("theme-scheme", show_hints, cx),
            self.appearance_theme_target_row(settings, ThemeTarget::Application, show_hints, cx),
            self.appearance_theme_target_row(settings, ThemeTarget::Terminal, show_hints, cx),
        ]);
        rows
    }

    pub fn appearance_scheme_control(
        &self,
        id: &'static str,
        show_hint: bool,
        _cx: &App,
    ) -> AnyElement {
        let selected = self.edit_scheme.unwrap_or(self.system_dark);
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
                    .child(self.i18n.t("settings_view.appearance.edit_scheme")),
            )
            .child(
                oxideterm_gpui_ui::tabs::tabs_list(&self.tokens).children(
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
                        .id((id, u64::from(dark)))
                        .on_click(self.click(AppearanceAction::Scheme(dark)))
                    }),
                ),
            )
            .when(
                !self.settings.appearance.follow_system_appearance
                    && self.settings.appearance.manual_scheme.is_none(),
                |body| {
                    body.child(self.appearance_action_button(
                        "lucide/check.svg",
                        self.i18n.t("settings_view.appearance.use_current_scheme"),
                        self.press(AppearanceAction::Assign(selected)),
                    ))
                },
            )
            .when(show_hint, |body| {
                body.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.appearance.scheme_edit_hint")),
                )
            })
            .into_any_element()
    }

    fn appearance_theme_target_row(
        &self,
        settings: &PersistedSettings,
        target: ThemeTarget,
        show_hint: bool,
        cx: &App,
    ) -> AnyElement {
        let (title_key, hint_key, select) = match target {
            ThemeTarget::Application => (
                "settings_view.appearance.application_theme",
                if show_hint {
                    "settings_view.appearance.application_theme_hint"
                } else {
                    ""
                },
                SettingsSelect::AppearanceTheme,
            ),
            ThemeTarget::Terminal => (
                "settings_view.appearance.terminal_theme",
                if show_hint {
                    "settings_view.appearance.color_theme_hint"
                } else {
                    ""
                },
                SettingsSelect::AppearanceTerminalTheme,
            ),
        };
        let scheme = self.edit_scheme;
        let selected_id = target.scheme_id(settings, scheme.unwrap_or(self.system_dark));
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
                .when(!self.onboarding, |control| {
                    control.child(
                        div()
                            .w_full()
                            .flex()
                            .flex_wrap()
                            .justify_end()
                            .items_center()
                            .gap(px(self.tokens.spacing.two))
                            .child(self.appearance_action_button(
                                "lucide/upload.svg",
                                self.i18n.t("settings_view.appearance.theme_import"),
                                self.press(AppearanceAction::Import(target)),
                            ))
                            .when(is_custom_theme_id(selected_id), |actions| {
                                actions.child(self.appearance_action_button(
                                    "lucide/pencil.svg",
                                    self.i18n.t("settings_view.custom_theme.edit"),
                                    self.press(AppearanceAction::Edit(target)),
                                ))
                            })
                            .child(self.appearance_action_button(
                                "lucide/plus.svg",
                                self.i18n.t("settings_view.custom_theme.create"),
                                self.press(AppearanceAction::New(target)),
                            )),
                    )
                })
                .into_any_element(),
        )
    }

    fn appearance_corner_style_control(
        &self,
        selected: oxideterm_settings::CornerStyle,
        _cx: &App,
    ) -> AnyElement {
        use oxideterm_settings::CornerStyle;
        oxideterm_gpui_ui::tabs::tabs_list(&self.tokens)
            .children(
                [
                    (
                        CornerStyle::Circular,
                        "settings_view.appearance.corner_circular",
                    ),
                    (
                        CornerStyle::Continuous,
                        "settings_view.appearance.corner_continuous",
                    ),
                ]
                .into_iter()
                .enumerate()
                .map(|(index, (style, key))| {
                    oxideterm_gpui_ui::tabs::tabs_trigger(
                        &self.tokens,
                        self.i18n.t(key),
                        selected == style,
                    )
                    .id(("appearance-corner-style", index))
                    .on_click(self.click(AppearanceAction::Corner(style)))
                }),
            )
            .into_any_element()
    }

    pub fn appearance_layout_card(&self, settings: &PersistedSettings, cx: &App) -> AnyElement {
        let rows = vec![
            self.appearance_row(
                "settings_view.appearance.density",
                "settings_view.appearance.density_hint",
                self.appearance_select_control(
                    SettingsSelect::AppearanceDensity,
                    density_label(settings.appearance.ui_density, &self.i18n),
                    self.tokens.metrics.settings_select_width,
                    cx,
                ),
            ),
            self.appearance_row(
                "settings_view.appearance.corner_style",
                "settings_view.appearance.corner_style_hint",
                self.appearance_corner_style_control(settings.appearance.corner_style, cx),
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
        .collect();
        self.appearance_card(
            self.i18n.t("settings_view.appearance.layout"),
            None,
            self.separated_settings_rows(rows),
        )
    }

    pub fn appearance_effects_card(&self, settings: &PersistedSettings, cx: &App) -> AnyElement {
        let mut rows = self.separated_settings_rows(vec![
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
                    self.tokens.metrics.settings_select_width,
                    cx,
                ),
            ),
            self.appearance_row(
                "settings_view.appearance.render_profile",
                "settings_view.appearance.render_profile_hint",
                self.appearance_select_control(
                    SettingsSelect::AppearanceRenderProfile,
                    render_profile_label(settings.appearance.render_profile, &self.i18n),
                    self.tokens.metrics.settings_select_width,
                    cx,
                ),
            ),
            self.appearance_row(
                "settings_view.appearance.frosted_glass",
                "settings_view.appearance.frosted_glass_hint",
                self.appearance_select_control(
                    SettingsSelect::AppearanceFrostedGlass,
                    frosted_glass_label(settings.appearance.frosted_glass, &self.i18n),
                    self.tokens.metrics.settings_select_width,
                    cx,
                ),
            ),
        ]);
        rows.push(self.appearance_vibrancy_status());
        self.appearance_card(self.i18n.t("settings_view.appearance.effects"), None, rows)
    }

    pub fn appearance_app_icon_card(&self, settings: &PersistedSettings, cx: &App) -> AnyElement {
        self.appearance_card_with_icon(
            "lucide/app-window.svg",
            self.i18n.t("settings_view.appearance.app_icon"),
            vec![self.appearance_app_icon_row(settings.appearance.app_icon, cx)],
        )
    }

    pub fn appearance_app_icon_row(&self, selected: AppIconVariant, cx: &App) -> AnyElement {
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

    pub fn appearance_app_icon_picker(&self, selected: AppIconVariant, cx: &App) -> AnyElement {
        let mut picker = div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_start()
            .gap(px(10.0))
            .min_w(px(0.0))
            .flex_wrap();

        for variant in self.icon_variants {
            picker =
                picker.child(self.appearance_app_icon_option(*variant, *variant == selected, cx));
        }

        picker.into_any_element()
    }

    pub fn appearance_app_icon_option(
        &self,
        variant: AppIconVariant,
        selected: bool,
        _cx: &App,
    ) -> AnyElement {
        let border_color = if selected {
            self.tokens.ui.accent
        } else {
            self.tokens.ui.border
        };
        let image = img((self.icon_source)(variant))
            .size(px(self.icon_size))
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
                        .child(render_icon(
                            "lucide/check.svg",
                            11.0,
                            rgb(self.tokens.ui.bg),
                        )),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                self.press(AppearanceAction::Icon(variant)),
            )
            .into_any_element()
    }

    pub fn appearance_vibrancy_status(&self) -> AnyElement {
        let mode = self.vibrancy_mode;
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

    fn appearance_window_titlebar_row(&self, checked: bool, _cx: &App) -> AnyElement {
        self.appearance_row(
            "settings_view.appearance.show_window_titlebar",
            "settings_view.appearance.show_window_titlebar_hint",
            switch(&self.tokens, "window-titlebar", String::new(), checked)
                .on_mouse_down(
                    MouseButton::Left,
                    self.press(AppearanceAction::Titlebar(checked)),
                )
                .into_any_element(),
        )
    }

    pub fn appearance_select_control(
        &self,
        select_id: SettingsSelect,
        value: String,
        width: f32,
        cx: &App,
    ) -> AnyElement {
        div()
            .relative()
            .w(px(width))
            .min_w(px(0.0))
            .child(self.settings_select_control(select_id, value, false, Some(width), cx))
            .into_any_element()
    }

    pub fn appearance_text_input_control(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        width: f32,
        cx: &App,
    ) -> AnyElement {
        self.settings_text_input_control(input, value, placeholder, width, cx)
    }

    pub fn appearance_radius_control(&self, settings: &PersistedSettings, cx: &App) -> AnyElement {
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

    pub fn appearance_card(
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

    pub fn appearance_card_with_icon(
        &self,
        icon: &'static str,
        title: String,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.appearance_card_shell(
            settings_appearance_card_header(
                &self.tokens,
                title,
                Some(render_icon(icon, 16.0, rgb(self.tokens.ui.text))),
                None,
            ),
            rows,
        )
    }

    pub fn appearance_card_shell(&self, header: AnyElement, rows: Vec<AnyElement>) -> AnyElement {
        settings_appearance_card_shell(&self.tokens, self.has_background, header, rows)
    }

    pub fn appearance_action_button(
        &self,
        icon: &'static str,
        label: String,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        // Appearance header actions are Tauri small outline toolbar buttons.
        // Route activation through the workspace Button boundary so browser
        // disabled/loading/focus-visible behavior can stay centralized.
        self.workspace_toolbar_action_button(
            label,
            Some(render_icon(icon, 14.0, rgb(self.tokens.ui.text))),
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

    pub fn appearance_row(
        &self,
        label_key: &str,
        hint_key: &str,
        control: AnyElement,
    ) -> AnyElement {
        settings_appearance_row(&self.tokens, &self.i18n, label_key, hint_key, control)
    }

    pub fn appearance_slider_value_control(
        &self,
        slider: SettingsSlider,
        anchor_id: SelectAnchorId,
        min: f32,
        max: f32,
        value: f32,
        unit: &'static str,
        cx: &App,
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
    pub fn appearance_slider_labeled_control(
        &self,
        slider: SettingsSlider,
        anchor_id: SelectAnchorId,
        min: f32,
        max: f32,
        value: f32,
        label: String,
        cx: &App,
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
}

pub fn render_appearance_slider(
    tokens: &ThemeTokens,
    id: SettingsSlider,
    anchor: SelectAnchorId,
    min: f32,
    max: f32,
    value: f32,
    dispatch: SettingsWindowDispatch,
) -> AnyElement {
    let down = dispatch.clone();
    let up = dispatch.clone();
    let motion = dispatch.clone();
    div()
        .w(px(tokens.metrics.settings_slider_width))
        .child(select_anchor_probe(
            anchor,
            slider(
                tokens,
                SliderView {
                    min,
                    max,
                    value,
                    disabled: false,
                },
            )
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                down(
                    SettingsWindowRequest::BeginSlider(id, f32::from(event.position.x)),
                    window,
                    cx,
                );
                cx.stop_propagation();
            })
            .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                up(SettingsWindowRequest::FinishSlider, window, cx);
                cx.stop_propagation();
            })
            .on_mouse_move(move |event, window, cx| {
                motion(SettingsWindowRequest::MoveSlider(event.clone()), window, cx)
            }),
            move |anchor, window, cx| {
                dispatch(SettingsWindowRequest::SliderAnchor(anchor), window, cx)
            },
        ))
        .into_any_element()
}
// Persisted opacity keeps 0.1% steps so the slider's expanded near-opaque
// range is not collapsed back into whole-percent jumps.
const WINDOW_OPACITY_STEPS_PER_UNIT: f64 = 1000.0;

/// Maps window opacity to slider travel in `0.0..=1.0`.
///
/// Compositors blend window alpha linearly, but on a dark theme over bright
/// content the first few percent of transparency are already very visible.
/// Travel is quadratic in transparency so most of the slider covers the
/// near-opaque range.
pub fn window_opacity_slider_position(opacity: f64) -> f32 {
    let transparency = ((MAX_WINDOW_OPACITY - opacity) / (MAX_WINDOW_OPACITY - MIN_WINDOW_OPACITY))
        .clamp(0.0, 1.0);
    (1.0 - transparency.sqrt()) as f32
}

pub fn window_opacity_from_slider_position(position: f32) -> f64 {
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

#[cfg(test)]
mod theme_preview_tests {
    use gpui::{Context, Render};
    use oxideterm_i18n::I18n;
    use oxideterm_settings::BackgroundScope;

    use super::*;
    use crate::theme_picker::appearance_theme_palette;

    struct Preview {
        page: ThemePreviewPage,
        window_background: bool,
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
                self.window_background.then(|| {
                    div()
                        .absolute()
                        .inset_0()
                        .bg(rgb(0x406080))
                        .into_any_element()
                }),
                BackgroundScope::Window,
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
            window_background: false,
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

    #[gpui::test]
    fn window_background_is_visible_through_every_preview_page(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| Preview {
            page: ThemePreviewPage::Terminal,
            window_background: true,
        });
        cx.simulate_resize(gpui::size(px(800.), px(500.)));
        for (page, selector) in [
            (ThemePreviewPage::Terminal, "theme-preview-terminal-sample"),
            (
                ThemePreviewPage::Connections,
                "theme-preview-connections-sample",
            ),
            (ThemePreviewPage::Sftp, "theme-preview-sftp-sample"),
            (
                ThemePreviewPage::NewConnection,
                "theme-preview-new-connection-sample",
            ),
        ] {
            view.update(cx, |view, cx| {
                view.page = page;
                cx.notify();
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let sample = cx.debug_bounds(selector).unwrap();
            cx.update(|window, _| {
                let point = sample.bottom_right() - gpui::point(px(24.), px(24.));
                let point = point.map(|value| value.scale(window.scale_factor()));
                let covering = window
                    .painted_quads()
                    .into_iter()
                    .filter(|quad| quad.bounds.contains(&point))
                    .filter_map(|quad| quad.background.as_solid())
                    .filter(|color| color.a > 0.)
                    .collect::<Vec<_>>();
                assert_eq!(
                    covering.last(),
                    Some(&gpui::Hsla::from(rgb(0x406080))),
                    "{page:?} must not tint the already composed window background again"
                );
            });
            if matches!(page, ThemePreviewPage::Terminal | ThemePreviewPage::Sftp) {
                for (selector, target) in [
                    ("theme-preview-terminal-tab", ThemePreviewPage::Terminal),
                    ("theme-preview-sftp-tab", ThemePreviewPage::Sftp),
                ] {
                    let bounds = cx.debug_bounds(selector).unwrap();
                    cx.update(|window, _| {
                        let point = (bounds.origin + gpui::point(px(20.), bounds.size.height / 2.))
                            .map(|value| value.scale(window.scale_factor()));
                        let colors = window
                            .painted_quads()
                            .into_iter()
                            .filter(|quad| quad.bounds.contains(&point))
                            .filter_map(|quad| quad.background.as_solid())
                            .filter(|color| color.a > 0.)
                            .collect::<Vec<_>>();
                        let background = gpui::Hsla::from(rgb(0x406080));
                        let background_index = colors
                            .iter()
                            .position(|color| *color == background)
                            .unwrap();
                        let overlays = &colors[background_index + 1..];
                        if page == target {
                            assert_eq!(
                                overlays,
                                &[gpui::Hsla::from(rgba(0x10203066))],
                                "selected tab keeps a translucent highlight"
                            );
                        } else {
                            assert_eq!(
                                overlays,
                                &[],
                                "inactive tab and tab strip must reveal the window background"
                            );
                        }
                    });
                }
            }
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
