use super::*;

use gpui::{ImageSource, ObjectFit, img};
use oxideterm_gpui_platform::vibrancy::{NativeVibrancyMode, VibrancySupport};

use oxideterm_render_policy::EffectiveRenderPolicy;

#[path = "preview.rs"]
mod preview;
pub use preview::{AppearancePreview, background_preferences_for_style};
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
            system_dark,
            edit_scheme,
            onboarding,
            render_policy,
            vibrancy_mode,
            vibrancy_support,
            icon_variants,
            icon_size,
            icon_source,
        } = self;
        AppearancePage {
            controls,
            system_dark,
            edit_scheme,
            onboarding,
            render_policy,
            vibrancy_mode,
            vibrancy_support,
            icon_variants,
            icon_size,
            icon_source,
            dispatch,
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

    fn press(
        &self,
        action: AppearanceAction,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        self.core().press(action)
    }
    pub fn appearance_theme_controls_with_hints(
        &self,
        settings: &PersistedSettings,
        show_hints: bool,
        cx: &App,
    ) -> Vec<AnyElement> {
        self.core()
            .appearance_theme_controls_with_hints(settings, show_hints, cx)
    }
    pub fn appearance_scheme_control(
        &self,
        id: &'static str,
        show_hint: bool,
        _cx: &App,
    ) -> AnyElement {
        self.core().appearance_scheme_control(id, show_hint, _cx)
    }
    pub fn appearance_layout_card(&self, settings: &PersistedSettings, cx: &App) -> AnyElement {
        self.core().appearance_layout_card(settings, cx)
    }
    pub fn appearance_select_control(
        &self,
        select_id: SettingsSelect,
        value: String,
        width: f32,
        cx: &App,
    ) -> AnyElement {
        self.core()
            .appearance_select_control(select_id, value, width, cx)
    }
    pub fn appearance_text_input_control(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        width: f32,
        cx: &App,
    ) -> AnyElement {
        self.core()
            .appearance_text_input_control(input, value, placeholder, width, cx)
    }
    pub fn appearance_radius_control(&self, settings: &PersistedSettings, cx: &App) -> AnyElement {
        self.core().appearance_radius_control(settings, cx)
    }
    pub fn appearance_card(
        &self,
        title: String,
        actions: Option<AnyElement>,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.core().appearance_card(title, actions, rows)
    }
    pub fn appearance_card_with_icon(
        &self,
        icon: &'static str,
        title: String,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.core().appearance_card_with_icon(icon, title, rows)
    }
    pub fn appearance_card_shell(&self, header: AnyElement, rows: Vec<AnyElement>) -> AnyElement {
        self.core().appearance_card_shell(header, rows)
    }
    pub fn appearance_action_button(
        &self,
        icon: &'static str,
        label: String,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        self.core().appearance_action_button(icon, label, listener)
    }
    pub fn appearance_row(
        &self,
        label_key: &str,
        hint_key: &str,
        control: AnyElement,
    ) -> AnyElement {
        self.core().appearance_row(label_key, hint_key, control)
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
        self.core()
            .appearance_slider_value_control(slider, anchor_id, min, max, value, unit, cx)
    }
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
        self.core()
            .appearance_slider_labeled_control(slider, anchor_id, min, max, value, label, cx)
    }
    fn core(&self) -> CoreAppearancePage<'_> {
        CoreAppearancePage {
            controls: self.controls.clone(),
            system_dark: self.system_dark,
            edit_scheme: self.edit_scheme,
            onboarding: self.onboarding,
            desktop: true,
            dispatch: self.dispatch.clone(),
        }
    }
}
const SETTINGS_PERCENT_SCALE: f64 = 100.0;
fn window_opacity_label(opacity: f64) -> String {
    let percent = format!("{:.1}", opacity * SETTINGS_PERCENT_SCALE);
    let percent = percent.strip_suffix(".0").unwrap_or(&percent);
    format!("{percent}%")
}
