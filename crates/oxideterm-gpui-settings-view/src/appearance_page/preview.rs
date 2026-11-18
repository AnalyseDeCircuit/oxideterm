use std::path::PathBuf;

use gpui::{AnyElement, App, Entity, div, prelude::*, px, rgb};
use oxideterm_gpui_background::{BackgroundFit, BackgroundPreferences, BackgroundPreview};
use oxideterm_i18n::I18n;
use oxideterm_settings::{BackgroundScope, PersistedSettings};
use oxideterm_settings_model::{ThemeEditorState, ThemeTarget, custom_theme_display_name};
use oxideterm_theme::{AppUiColors, TerminalTheme, ThemeTokens};

use crate::{
    SettingsSelect, ThemePreviewPage,
    appearance::{settings_appearance_theme_preview, settings_theme_editor_preview},
    application_theme_description,
    entity::SettingsWorkspaceEntity,
    theme_picker::appearance_theme_palette,
};

pub struct AppearancePreview<'a> {
    pub tokens: ThemeTokens,
    pub i18n: &'a I18n,
    pub state: &'a Entity<SettingsWorkspaceEntity>,
    pub open_settings_select: Option<SettingsSelect>,
    pub hovered_theme: Option<&'a str>,
}

impl AppearancePreview<'_> {
    pub fn render(
        &self,
        settings: &PersistedSettings,
        preview: Entity<BackgroundPreview>,
        show_hint: bool,
        cx: &mut App,
    ) -> AnyElement {
        let preview_target = self
            .open_settings_select
            .and_then(SettingsSelect::theme_target);
        let preview_dark = self
            .open_settings_select
            .and_then(SettingsSelect::theme_system_dark)
            .unwrap_or(self.state.read(cx).appearance_edit_dark.unwrap_or(false));
        let preview_theme = preview_target.and(self.hovered_theme);
        let preview_id = |target: ThemeTarget| {
            if preview_target == Some(target) {
                preview_theme.unwrap_or_else(|| target.scheme_id(settings, preview_dark))
            } else {
                target.scheme_id(settings, preview_dark)
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
        let page = self.state.read(cx).theme_preview_page;
        let mut style = settings.terminal.background_for_scheme(Some(preview_dark));
        if let Some(blur) = self.state.read(cx).background_blur_preview(Some(
            self.state.read(cx).appearance_edit_dark.unwrap_or(false),
        )) {
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
                application_theme_description(application_id, preview_tokens.ui, self.i18n),
                self.i18n,
                page,
                has_background.then(|| preview.clone().into_any_element()),
                style.scope,
                {
                    let workspace = self.state.downgrade();
                    move |page, _window, cx| {
                        let _ = workspace.update(cx, |settings, cx| {
                            settings.theme_preview_page = page;
                            cx.notify();
                        });
                    }
                },
            ))
            .when(style.day_cycle && has_background, |view| {
                view.child(self.daylight_controls(preview.clone(), cx))
            })
            .when(show_hint, |view| {
                view.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.appearance.theme_preview_hint")),
                )
            })
            .into_any_element()
    }

    pub fn render_editor(
        &self,
        editor: &ThemeEditorState,
        terminal: TerminalTheme,
        ui: AppUiColors,
        mono_font: gpui::SharedString,
        cx: &mut App,
    ) -> AnyElement {
        let mut tokens = self.tokens;
        tokens.ui = ui;
        tokens.terminal = terminal;
        tokens.refresh_palette_metrics();
        let background = background_preferences_for_style(&editor.background, &tokens);
        let preview = self.state.read(cx).editor_background_preview.clone();
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
                mono_font,
                has_background.then(|| preview.clone().into_any_element()),
                editor.background.scope,
            ))
            .when(editor.background.day_cycle && has_background, |view| {
                view.child(self.daylight_controls(preview, cx))
            })
            .into_any_element()
    }

    fn daylight_controls(&self, preview: Entity<BackgroundPreview>, cx: &App) -> AnyElement {
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
                    crate::background_page::appearance_option_pill(
                        &self.tokens,
                        self.i18n.t(&format!("settings_view.terminal.{key}")),
                        selected == hour,
                        {
                            let settings = self.state.downgrade();
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
}

pub fn background_preferences_for_style(
    style: &oxideterm_settings::BackgroundStyle,
    tokens: &ThemeTokens,
) -> Option<BackgroundPreferences> {
    if !style.enabled {
        return None;
    }
    let effect =
        style.effect.as_ref().map(
            |effect| oxideterm_gpui_background::GeneratedEffectPreferences {
                kind: match effect.kind {
                    oxideterm_settings::GeneratedBackgroundKind::Mineral => {
                        oxideterm_gpui_background::GeneratedEffectKind::Mineral
                    }
                    oxideterm_settings::GeneratedBackgroundKind::Fog => {
                        oxideterm_gpui_background::GeneratedEffectKind::Fog
                    }
                    oxideterm_settings::GeneratedBackgroundKind::Tide => {
                        oxideterm_gpui_background::GeneratedEffectKind::Tide
                    }
                    oxideterm_settings::GeneratedBackgroundKind::TideParticles => {
                        oxideterm_gpui_background::GeneratedEffectKind::TideParticles
                    }
                    oxideterm_settings::GeneratedBackgroundKind::Meteor => {
                        oxideterm_gpui_background::GeneratedEffectKind::Meteor
                    }
                    oxideterm_settings::GeneratedBackgroundKind::Particles => {
                        oxideterm_gpui_background::GeneratedEffectKind::Particles
                    }
                    oxideterm_settings::GeneratedBackgroundKind::Caustics => {
                        oxideterm_gpui_background::GeneratedEffectKind::Caustics
                    }
                },
                strength: effect.strength,
                sheen: effect.sheen,
                max_fps: effect.max_fps,
                colors: effect
                    .colors
                    .unwrap_or([tokens.ui.accent, tokens.terminal.cyan]),
                speed: if tokens.motion.enabled && tokens.motion.spatial_enabled {
                    effect.speed
                } else {
                    0.0
                },
                size: effect.size,
                brightness: effect.brightness,
                roughness: effect.roughness,
                direction: effect.direction,
                particle_count: effect.particle_count,
                preview_viewport: None,
            },
        );
    let path = match (&style.image, effect.is_some()) {
        (Some(path), _) => PathBuf::from(path),
        (None, true) => PathBuf::new(),
        (None, false) => return None,
    };
    Some(BackgroundPreferences {
        scene: oxideterm_gpui_background::ScenePreferences {
            pause_on_input: style.pause_on_input,
            parallax: style.parallax && tokens.motion.enabled && tokens.motion.spatial_enabled,
            camera: style
                .camera
                .filter(|_| tokens.motion.enabled && tokens.motion.spatial_enabled)
                .map(|camera| oxideterm_gpui_background::CameraPreferences {
                    motion: match camera.motion {
                        oxideterm_settings::BackgroundCameraMotion::Pan => {
                            oxideterm_gpui_background::CameraMotion::Pan
                        }
                        oxideterm_settings::BackgroundCameraMotion::ZoomIn => {
                            oxideterm_gpui_background::CameraMotion::ZoomIn
                        }
                        oxideterm_settings::BackgroundCameraMotion::ZoomOut => {
                            oxideterm_gpui_background::CameraMotion::ZoomOut
                        }
                    },
                    amount: camera.amount,
                    speed: camera.speed,
                }),
            day_cycle: style.day_cycle,
            dark: {
                let color = tokens.terminal.background;
                ((color >> 16 & 255) * 299 + (color >> 8 & 255) * 587 + (color & 255) * 114)
                    < 128000
            },
            ..Default::default()
        },
        path,
        effect,
        readability: (style.readability > 0.0).then_some(
            oxideterm_gpui_background::ReadingOverlay {
                color: tokens.terminal.background,
                opacity: style.readability.clamp(0.0, 1.0),
            },
        ),
        alignment: style.alignment.position(),
        opacity: style.opacity.clamp(0.0, 1.0) as f32,
        blur: style.blur.clamp(0, 20) as f32,
        fit: match style.fit {
            oxideterm_settings::BackgroundFit::Cover => BackgroundFit::Cover,
            oxideterm_settings::BackgroundFit::Contain => BackgroundFit::Contain,
            oxideterm_settings::BackgroundFit::Fill => BackgroundFit::Fill,
            oxideterm_settings::BackgroundFit::Tile => BackgroundFit::Tile,
        },
        limits: oxideterm_gpui_background::PlaybackLimits {
            max_width: style.max_width,
            max_height: style.max_height,
            max_fps: style.max_fps,
        },
        on_failure: None,
    })
}
