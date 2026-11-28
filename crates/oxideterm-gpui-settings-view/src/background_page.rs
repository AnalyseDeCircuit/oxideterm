use gpui::{
    AnyElement, App, Context, Div, Entity, MouseButton, MouseDownEvent, Window, div, prelude::*,
    px, relative, rgb, rgba,
};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_button,
    },
    select::SelectAnchorId,
    switch::switch,
};
use oxideterm_settings::{
    BackgroundAlignment, BackgroundScope, BackgroundStyle, MAX_TERMINAL_BACKGROUND_OPACITY,
    MIN_TERMINAL_BACKGROUND_OPACITY, PersistedSettings,
};
use oxideterm_theme::ThemeTokens;
use std::{
    ops::Deref,
    path::{Path, PathBuf},
    rc::Rc,
};

use crate::{
    appearance::background_tab_icon, appearance_page::AppearancePage,
    entity::SettingsWorkspaceEntity, surface::nav::render_icon, *,
};

const SETTINGS_PERCENT_SCALE: f64 = 100.0;
const APPEARANCE_BACKGROUND_SCOPE_SWITCHER_ID: &str = "appearance-background-scope-switcher";

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

fn background_scope_index(scope: BackgroundScope) -> usize {
    BACKGROUND_SCOPE_OPTIONS
        .iter()
        .position(|(option, _)| *option == scope)
        .unwrap_or(0)
}

#[derive(Clone)]
pub enum BackgroundRequest {
    Marketplace,
    Pick,
    Clear,
    Remove(String),
}

pub enum BackgroundGalleryEdit {
    Pick,
    Clear,
    Remove(String),
}

pub struct BackgroundPage<'a> {
    pub appearance: AppearancePage<'a>,
    pub state: &'a Entity<SettingsWorkspaceEntity>,
    pub preview: Option<AnyElement>,
    pub animated_media: bool,
    pub is_built_in: &'a dyn Fn(&Path) -> bool,
    pub dispatch: Rc<dyn Fn(BackgroundRequest, &mut Window, &mut App)>,
    pub persist: Rc<dyn Fn(Box<dyn FnOnce(&mut BackgroundStyle)>, &mut App)>,
}

impl<'a> Deref for BackgroundPage<'a> {
    type Target = AppearancePage<'a>;

    fn deref(&self) -> &Self::Target {
        &self.appearance
    }
}

impl BackgroundPage<'_> {
    fn request(
        &self,
        request: BackgroundRequest,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let dispatch = self.dispatch.clone();
        move |_, window, cx| {
            dispatch(request.clone(), window, cx);
            cx.stop_propagation();
        }
    }

    fn edit<F: Fn(&mut BackgroundStyle) + 'static>(
        &self,
        edit: F,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<F> {
        let state = self.state.downgrade();
        let persist = self.persist.clone();
        let edit = Rc::new(edit);
        move |_, _, cx| {
            let Some(state) = state.upgrade() else { return };
            if state.read(cx).theme_editor().is_some() {
                state.update(cx, |state, cx| {
                    state.edit_theme_editor_background(|style| edit(style), cx)
                });
            } else {
                let edit = edit.clone();
                persist(Box::new(move |style| edit(style)), cx);
            }
            cx.stop_propagation();
        }
    }
    pub fn render(mut self, settings: &PersistedSettings, cx: &App) -> AnyElement {
        let terminal = &settings.terminal;
        let has_background =
            terminal.background_image.is_some() || terminal.background_effect.is_some();
        let animated_media = self.animated_media;
        let mut rows = Vec::new();
        if let Some(preview) = self.preview.take() {
            rows.push(self.appearance_scheme_control("background-scheme", true, cx));
            rows.push(preview);
        }
        if has_background {
            let enabled = terminal.background_enabled;
            rows.push(
                self.appearance_row(
                    "settings_view.terminal.bg_enabled",
                    "settings_view.terminal.bg_enabled_hint",
                    switch(&self.tokens, "background-enabled", String::new(), enabled)
                        .on_mouse_down(
                            MouseButton::Left,
                            self.edit(move |style| style.enabled = !enabled),
                        )
                        .into_any_element(),
                ),
            );
        }
        rows.push(self.appearance_background_image_slot(settings, cx));
        if terminal.background_image.is_some() {
            let blur = self
                .state
                .read(cx)
                .background_blur_preview(self.appearance.edit_scheme)
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
        rows.push(self.card_separator());
        use oxideterm_settings::GeneratedBackgroundKind;
        const BACKGROUND_EFFECT_OPTIONS: [(Option<GeneratedBackgroundKind>, &str); 8] = [
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
                Some(GeneratedBackgroundKind::TideParticles),
                "settings_view.terminal.bg_source_tide_particles",
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
        ];
        let active_effect_kind = terminal
            .background_effect
            .as_ref()
            .map(|effect| effect.kind);
        let active_effect_key = BACKGROUND_EFFECT_OPTIONS
            .iter()
            .find(|(kind, _)| *kind == active_effect_kind)
            .map(|(_, key)| *key)
            .unwrap_or("settings_view.terminal.bg_effect_none");
        let effects_expanded = self
            .state
            .read(cx)
            .background_effects_expanded
            .unwrap_or(terminal.background_effect.is_some());
        rows.push(self.appearance_collapsible_subsection_header(
            "appearance-bg-effects-toggle",
            "settings_view.terminal.bg_effects",
            Some(self.i18n.t(active_effect_key)),
            effects_expanded,
            {
                let state = self.state.downgrade();
                move |_, _, cx| {
                    let _ = state.update(cx, |entity, cx| {
                        let current = entity
                            .background_effects_expanded
                            .unwrap_or(active_effect_kind.is_some());
                        entity.background_effects_expanded = Some(!current);
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
            },
        ));
        if effects_expanded {
            rows.push(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(self.tokens.spacing.two))
                    .children(BACKGROUND_EFFECT_OPTIONS.map(|(kind, key)| {
                        appearance_option_pill(
                            self.tokens,
                            self.i18n.t(key),
                            kind == active_effect_kind,
                            self.edit(move |style| {
                                if let Some(kind) = kind {
                                    style.effect.get_or_insert_with(Default::default).kind = kind;
                                } else {
                                    style.effect = None;
                                }
                            }),
                        )
                    }))
                    .into_any_element(),
            );
            if let Some(effect) = &terminal.background_effect {
                rows.extend(self.appearance_background_effect_rows(effect, cx));
            }
        }
        if has_background {
            rows.push(self.card_separator());
            rows.extend(self.appearance_background_interaction_rows(settings, animated_media, cx));
            rows.push(self.card_separator());
            let surface_opacity = terminal
                .background_surface_opacity
                .unwrap_or(self.tokens.metrics.sidebar_vibrancy_alpha);
            rows.push(self.appearance_row(
                "settings_view.terminal.bg_surface_opacity",
                "settings_view.terminal.bg_surface_opacity_hint",
                self.appearance_slider_value_control(
                    SettingsSlider::BackgroundSurfaceOpacity,
                    SelectAnchorId::SettingsBackgroundSurfaceOpacity,
                    0.0,
                    100.0,
                    (surface_opacity * 100.0).round(),
                    "%",
                    cx,
                ),
            ));
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
            "lucide/image.svg",
            self.i18n.t("settings_view.terminal.bg_title"),
            rows,
        )
    }

    fn appearance_background_effect_rows(
        &self,
        effect: &oxideterm_settings::GeneratedBackgroundSettings,
        cx: &App,
    ) -> Vec<AnyElement> {
        use oxideterm_settings::GeneratedBackgroundKind;
        let (size_key, brightness_key, hint_key) = match effect.kind {
            GeneratedBackgroundKind::Mineral => {
                ("bg_mineral_scale", "bg_mineral_sheen", "bg_mineral_hint")
            }
            GeneratedBackgroundKind::Fog => ("bg_fog_size", "bg_fog_concentration", "bg_fog_hint"),
            GeneratedBackgroundKind::Tide => ("bg_tide_width", "bg_fog_brightness", "bg_tide_hint"),
            GeneratedBackgroundKind::TideParticles => (
                "bg_tide_particles_size",
                "bg_fog_brightness",
                "bg_tide_particles_hint",
            ),
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
            GeneratedBackgroundKind::Tide
            | GeneratedBackgroundKind::TideParticles
            | GeneratedBackgroundKind::Meteor => Some((
                SettingsSlider::BackgroundEffectDirection,
                "bg_tide_direction",
                0.0,
                360.0,
                effect.direction,
                "°",
            )),
            GeneratedBackgroundKind::Fog
            | GeneratedBackgroundKind::Caustics
            | GeneratedBackgroundKind::Particles => None,
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
        if effect.kind != GeneratedBackgroundKind::Mineral {
            rows.push(self.appearance_row(
                "settings_view.terminal.bg_field_density",
                "settings_view.terminal.bg_field_density_hint",
                self.appearance_slider_value_control(
                    SettingsSlider::BackgroundParticleCount,
                    SelectAnchorId::SettingsBackgroundParticleCount,
                    oxideterm_gpui_background::MIN_EFFECT_DENSITY as f32,
                    oxideterm_gpui_background::MAX_EFFECT_DENSITY as f32,
                    effect.particle_count as f32,
                    "",
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

    fn appearance_background_scope_control(
        &self,
        selected: BackgroundScope,
        cx: &App,
    ) -> AnyElement {
        let active_index = background_scope_index(selected);
        let control_id = APPEARANCE_BACKGROUND_SCOPE_SWITCHER_ID;
        let motion = &self.state.read(cx).background_scope_motion;
        let previous_index = motion
            .previous_index_for(control_id, active_index)
            .unwrap_or(active_index);
        let transitioning = motion.is_active_for(control_id, active_index);
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
        let indicator = oxideterm_gpui_ui::segmented_control::selection_indicator(
            &self.tokens,
            control_id,
            indicator,
            previous_left,
            active_left,
            transitioning,
        );
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
                    .on_mouse_down(MouseButton::Left, {
                        let edit = self.edit(move |style| style.scope = scope);
                        let state = self.state.downgrade();
                        let tokens = *self.tokens;
                        move |event, window, cx| {
                            if target_index != active_index {
                                edit(event, window, cx);
                                let _ = state.update(cx, |state, cx| {
                                    state.animate_background_scope(
                                        &tokens,
                                        active_index,
                                        target_index,
                                        cx,
                                    );
                                });
                            }
                            cx.stop_propagation();
                        }
                    }),
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

    fn appearance_background_interaction_rows(
        &self,
        settings: &PersistedSettings,
        animated: bool,
        cx: &App,
    ) -> Vec<AnyElement> {
        let terminal = &settings.terminal;
        let has_active_interaction = terminal.background_parallax
            || terminal.background_camera.is_some()
            || terminal.background_day_cycle;
        let expanded = self
            .state
            .read(cx)
            .background_interactions_expanded
            .unwrap_or(has_active_interaction);
        let mut rows = vec![self.appearance_collapsible_subsection_header(
            "appearance-bg-interactions-toggle",
            "settings_view.terminal.bg_interactions",
            None,
            expanded,
            {
                let state = self.state.downgrade();
                move |_, _, cx| {
                    let _ = state.update(cx, |entity, cx| {
                        let current = entity
                            .background_interactions_expanded
                            .unwrap_or(has_active_interaction);
                        entity.background_interactions_expanded = Some(!current);
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
            },
        )];
        if !expanded {
            return rows;
        }
        for (index, key, enabled, visible) in [
            (
                1,
                "bg_parallax",
                terminal.background_parallax,
                (terminal.background_image.is_some() && !animated)
                    || terminal.background_effect.as_ref().is_some_and(|effect| {
                        matches!(
                            effect.kind,
                            oxideterm_settings::GeneratedBackgroundKind::Particles
                                | oxideterm_settings::GeneratedBackgroundKind::TideParticles
                        )
                    }),
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
                    switch(&self.tokens, key, String::new(), enabled)
                        .on_mouse_down(
                            MouseButton::Left,
                            self.edit(move |style| match index {
                                0 => style.pause_on_input = !enabled,
                                1 => style.parallax = !enabled,
                                3 => style.camera = (!enabled).then(Default::default),
                                _ => style.day_cycle = !enabled,
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
                                    appearance_option_pill(
                                        self.tokens,
                                        self.i18n.t(&format!("settings_view.terminal.{key}")),
                                        camera.motion == motion,
                                        self.edit(move |style| {
                                            if let Some(camera) = &mut style.camera {
                                                camera.motion = motion;
                                            }
                                        }),
                                    )
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

    fn appearance_background_alignment_control(
        &self,
        selected: BackgroundAlignment,
        _cx: &App,
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
                        appearance_option_pill(
                            self.tokens,
                            self.i18n.t(&format!("settings_view.terminal.{key}")),
                            selected == alignment,
                            self.edit(move |style| style.alignment = alignment),
                        )
                    },
                ))
                .into_any_element(),
        )
    }
    fn appearance_background_image_slot(
        &self,
        settings: &PersistedSettings,
        cx: &App,
    ) -> AnyElement {
        let background_images = self.state.read(cx).background_images_snapshot();
        let editor_open = self.state.read(cx).theme_editor().is_some();
        let has_removable_gallery_images = !editor_open
            && background_images
                .iter()
                .any(|image_path| !(self.is_built_in)(Path::new(image_path)));
        let actions = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .when(!editor_open, |actions| {
                actions
                    .child(self.appearance_action_button(
                        "lucide/download.svg",
                        self.i18n.t("settings_view.terminal.bg_get_more"),
                        self.request(BackgroundRequest::Marketplace),
                    ))
                    .child(self.appearance_action_button(
                        "lucide/plus.svg",
                        self.i18n.t("settings_view.terminal.bg_add"),
                        self.request(BackgroundRequest::Pick),
                    ))
            })
            .when(has_removable_gallery_images, |actions| {
                actions.child(
                    settings_background_clear_all_button(
                        &self.tokens,
                        self.i18n.t("settings_view.terminal.bg_clear_all"),
                        render_icon("lucide/trash-2.svg", 14.0, rgb(self.tokens.ui.error)),
                    )
                    .on_mouse_down(MouseButton::Left, self.request(BackgroundRequest::Clear)),
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

    fn background_image_slot_content(&self, settings: &PersistedSettings, cx: &App) -> AnyElement {
        let background_images = self.state.read(cx).background_images_snapshot();
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

    fn background_thumbnail(&self, image_path: &str, active: bool, cx: &App) -> AnyElement {
        let image_path = image_path.to_string();
        let remove_path = image_path.clone();
        let is_built_in = self.state.read(cx).theme_editor().is_some()
            || (self.is_built_in)(Path::new(&image_path));
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
            move || render_icon("lucide/image.svg", 20.0, rgb(fallback_icon_color)),
        );
        thumbnail
            .when(!is_built_in, |thumbnail| {
                thumbnail.child(
                    settings_background_thumbnail_remove_button(
                        &self.tokens,
                        render_icon("lucide/x.svg", 12.0, rgb(self.tokens.ui.text)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        self.request(BackgroundRequest::Remove(remove_path)),
                    ),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                self.edit(move |style| style.image = Some(image_path.clone())),
            )
            .into_any_element()
    }

    fn appearance_background_tabs(&self, settings: &PersistedSettings, _cx: &App) -> AnyElement {
        let mut pills = Vec::new();
        for (key, label_key, icon) in background_tab_options() {
            let enabled = settings
                .terminal
                .background_enabled_tabs
                .iter()
                .any(|tab| tab == key);
            let key = (*key).to_string();
            pills.push(
                self.background_tab_pill(label_key, background_tab_icon(*icon), enabled)
                    .on_mouse_down(
                        MouseButton::Left,
                        self.edit(move |style| {
                            if let Some(index) =
                                style.enabled_tabs.iter().position(|tab| tab == &key)
                            {
                                style.enabled_tabs.remove(index);
                            } else {
                                style.enabled_tabs.push(key.clone());
                            }
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

    fn background_tab_pill(&self, label_key: &str, icon: &'static str, enabled: bool) -> Div {
        let color = if enabled {
            self.tokens.ui.accent
        } else {
            self.tokens.ui.text_muted
        };
        settings_background_tab_pill(
            &self.tokens,
            self.i18n.t(label_key),
            render_icon(
                icon,
                self.tokens.metrics.settings_background_tab_icon_size,
                rgb(color),
            ),
            enabled,
        )
    }

    fn appearance_collapsible_subsection_header(
        &self,
        id: &'static str,
        title_key: &str,
        summary_badge: Option<String>,
        expanded: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        div()
            .id(id)
            .w_full()
            .min_w_0()
            .px(px(4.0))
            .py(px(4.0))
            .rounded(px(self.tokens.radii.sm))
            .cursor_pointer()
            .hover(|style| style.bg(rgba((self.tokens.ui.bg_hover << 8) | 0x66)))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .on_mouse_down(MouseButton::Left, listener)
            .child(self.card_subsection_header(self.i18n.t(title_key), None))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when_some(summary_badge, |right, badge| {
                        right.child(
                            div()
                                .px(px(8.0))
                                .py(px(2.0))
                                .rounded(px(self.tokens.radii.sm))
                                .bg(rgb(self.tokens.ui.bg_sunken))
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(badge),
                        )
                    })
                    .child(render_icon(
                        if expanded {
                            "lucide/chevron-down.svg"
                        } else {
                            "lucide/chevron-right.svg"
                        },
                        14.0,
                        rgb(self.tokens.ui.text_muted),
                    )),
            )
            .into_any_element()
    }
}

pub fn appearance_option_pill(
    tokens: &ThemeTokens,
    label: String,
    selected: bool,
    listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
    toolbar_button(
        tokens,
        label,
        None,
        ToolbarButtonOptions {
            button: ButtonOptions {
                variant: if selected {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Outline
                },
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled: false,
            },
            height: Some(tokens.metrics.settings_appearance_action_height),
            padding_x: Some(10.0),
            font_size: Some(tokens.metrics.ui_text_xs),
            border: Some(rgb(if selected {
                tokens.ui.accent
            } else {
                tokens.ui.border
            })),
            text_color: Some(rgb(if selected {
                tokens.ui.accent
            } else {
                tokens.ui.text
            })),
            ..ToolbarButtonOptions::default()
        },
    )
    .on_mouse_down(MouseButton::Left, listener)
}

impl SettingsWorkspaceEntity {
    fn animate_background_scope(
        &mut self,
        tokens: &ThemeTokens,
        previous: usize,
        target: usize,
        cx: &mut Context<Self>,
    ) {
        let id = APPEARANCE_BACKGROUND_SCOPE_SWITCHER_ID;
        let Some(motion) = oxideterm_gpui_ui::segmented_control_motion(tokens) else {
            self.background_scope_motion.clear(id);
            self.background_scope_task.take();
            cx.notify();
            return;
        };
        let generation = self.background_scope_motion.begin_with_previous_index(
            id,
            Some(previous),
            target,
            None,
        );
        self.background_scope_task = Some(cx.spawn(async move |settings, cx| {
            cx.background_executor().timer(motion.duration).await;
            let _ = settings.update(cx, |settings, cx| {
                if settings.background_scope_motion.finish(id, generation) {
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }
}
