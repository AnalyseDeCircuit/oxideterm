pub(crate) mod control;
#[cfg(target_family = "wasm")]
pub use control::{
    render_appearance_slider, window_opacity_from_slider_position, window_opacity_slider_position,
};

use crate::components::SettingsEdit;
use oxideterm_settings::BackgroundStyle;

#[cfg(not(target_family = "wasm"))]
use {
    crate::{
        SettingsSlider,
        appearance_page::{
            APPEARANCE_BORDER_RADIUS_MAX, APPEARANCE_BORDER_RADIUS_MIN,
            APPEARANCE_UI_FONT_SIZE_MAX, APPEARANCE_UI_FONT_SIZE_MIN,
            window_opacity_from_slider_position,
        },
        entity::SettingsWorkspaceEntity,
    },
    gpui::Context,
    oxideterm_gpui_ui::{select::OverlayAnchor, slider::slider_pointer_percent},
    oxideterm_settings::{
        MAX_TERMINAL_BACKGROUND_OPACITY, MIN_TERMINAL_BACKGROUND_OPACITY, PersistedSettings,
    },
    oxideterm_theme::ThemeTokens,
    std::time::Duration,
};

pub enum SliderEdit {
    Settings(SettingsEdit),
    WindowOpacity(f64),
    Background(Box<dyn FnOnce(&mut BackgroundStyle)>),
}

#[cfg(not(target_family = "wasm"))]
impl SettingsWorkspaceEntity {
    pub fn set_slider_drag(&mut self, active: Option<SettingsSlider>) {
        self.slider_drag = active;
    }

    pub fn slider_drag(&self) -> Option<SettingsSlider> {
        self.slider_drag
    }

    pub fn finish_slider_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(active) = self.slider_drag.take() else {
            return false;
        };
        if active == SettingsSlider::AppearanceBackgroundBlur {
            self.finish_background_blur_drag(cx);
        }
        true
    }

    pub fn update_slider(
        &mut self,
        slider: SettingsSlider,
        x: f32,
        anchor: OverlayAnchor,
        settings: &PersistedSettings,
        tokens: &ThemeTokens,
        cx: &mut Context<Self>,
    ) -> Option<SliderEdit> {
        use SettingsSlider::*;
        const BACKGROUND_BLUR_COMMIT_DELAY: Duration = Duration::from_millis(150);
        const SETTINGS_PERCENT_SCALE: f64 = 100.0;
        let width = f32::from(anchor.bounds.size.width).max(1.0);
        let position = slider_pointer_percent(
            x - f32::from(anchor.bounds.left()),
            width,
            tokens.metrics.ui_slider_thumb_size,
        );
        let scale = |min: f32, max: f32| (min + position * (max - min)).round();
        let dark = Some(self.appearance_edit_dark.unwrap_or(false));
        match slider {
            TerminalFontSize => {
                let value = scale(8.0, 32.0) as i64;
                (settings.terminal.font_size != value).then(|| {
                    SliderEdit::Settings(Box::new(move |settings| {
                        settings.terminal.font_size = value
                    }))
                })
            }
            AppearanceUiFontSize => {
                let value = scale(APPEARANCE_UI_FONT_SIZE_MIN, APPEARANCE_UI_FONT_SIZE_MAX) as i64;
                (settings.appearance.ui_font_size != value).then(|| {
                    SliderEdit::Settings(Box::new(move |settings| {
                        settings.appearance.ui_font_size = value
                    }))
                })
            }
            AppearanceBorderRadius | OnboardingBorderRadius | VersionMigrationBorderRadius => {
                let value =
                    scale(APPEARANCE_BORDER_RADIUS_MIN, APPEARANCE_BORDER_RADIUS_MAX) as i64;
                (settings.appearance.border_radius != value).then(|| {
                    SliderEdit::Settings(Box::new(move |settings| {
                        settings.appearance.border_radius = value
                    }))
                })
            }
            AppearanceWindowOpacity => {
                let value = window_opacity_from_slider_position(position);
                (settings.appearance.window_opacity != value)
                    .then_some(SliderEdit::WindowOpacity(value))
            }
            AppearanceBackgroundOpacity => {
                let min = (MIN_TERMINAL_BACKGROUND_OPACITY * SETTINGS_PERCENT_SCALE) as f32;
                let max = (MAX_TERMINAL_BACKGROUND_OPACITY * SETTINGS_PERCENT_SCALE) as f32;
                let value = f64::from(scale(min, max)) / SETTINGS_PERCENT_SCALE;
                let current = self
                    .theme_editor()
                    .map(|editor| editor.background.opacity)
                    .unwrap_or_else(|| settings.terminal.background_for_scheme(dark).opacity);
                (current != value)
                    .then(|| SliderEdit::Background(Box::new(move |style| style.opacity = value)))
            }
            AppearanceBackgroundBlur => {
                let value = scale(0.0, 20.0) as i64;
                if self.theme_editor().is_some() {
                    return Some(SliderEdit::Background(Box::new(move |style| {
                        style.blur = value
                    })));
                }
                let previous = settings.terminal.background_for_scheme(dark).blur;
                self.update_background_blur_preview(
                    previous,
                    value,
                    dark,
                    BACKGROUND_BLUR_COMMIT_DELAY,
                    cx,
                );
                None
            }
            BackgroundSurfaceOpacity => {
                let value = scale(0.0, 100.0) / 100.0;
                Some(SliderEdit::Background(Box::new(move |style| {
                    style.surface_opacity = Some(value)
                })))
            }
            BackgroundReadability => {
                let value = scale(0.0, 100.0) / 100.0;
                Some(SliderEdit::Background(Box::new(move |style| {
                    style.readability = value
                })))
            }
            BackgroundCameraAmount | BackgroundCameraSpeed => {
                let speed = slider == BackgroundCameraSpeed;
                let value = if speed {
                    scale(10.0, 200.0)
                } else {
                    scale(0.0, 100.0)
                } / 100.0;
                Some(SliderEdit::Background(Box::new(move |style| {
                    if let Some(camera) = &mut style.camera {
                        if speed {
                            camera.speed = value;
                        } else {
                            camera.amount = value;
                        }
                    }
                })))
            }
            BackgroundParticleCount
            | BackgroundEffectStrength
            | BackgroundEffectSheen
            | BackgroundEffectSpeed
            | BackgroundEffectSize
            | BackgroundEffectBrightness
            | BackgroundEffectRoughness
            | BackgroundEffectDirection => {
                let (min, max) = match slider {
                    BackgroundEffectSpeed => (0.0, 300.0),
                    BackgroundEffectSize => (30.0, 200.0),
                    BackgroundEffectDirection => (0.0, 360.0),
                    BackgroundParticleCount => (
                        oxideterm_gpui_background::MIN_EFFECT_DENSITY as f32,
                        oxideterm_gpui_background::MAX_EFFECT_DENSITY as f32,
                    ),
                    _ => (0.0, 100.0),
                };
                let value = scale(min, max);
                let fraction = value / 100.0;
                Some(SliderEdit::Background(Box::new(move |style| {
                    let effect = style.effect.get_or_insert_with(Default::default);
                    match slider {
                        BackgroundEffectStrength => effect.strength = fraction,
                        BackgroundParticleCount => effect.particle_count = value as u32,
                        BackgroundEffectSheen => effect.sheen = fraction,
                        BackgroundEffectSpeed => effect.speed = fraction,
                        BackgroundEffectSize => effect.size = fraction,
                        BackgroundEffectRoughness => effect.roughness = fraction,
                        BackgroundEffectDirection => effect.direction = value,
                        _ => effect.brightness = fraction,
                    }
                })))
            }
        }
    }
}
