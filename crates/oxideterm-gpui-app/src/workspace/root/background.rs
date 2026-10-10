use super::super::*;

use oxideterm_gpui_terminal::background_display_target;

struct BundledWorkspaceBackground {
    file_name: &'static str,
    bytes: &'static [u8],
}

// Bundled gallery assets are installed on startup and protected from user deletion.
const BUNDLED_WORKSPACE_BACKGROUNDS: &[BundledWorkspaceBackground] = &[
    BundledWorkspaceBackground {
        file_name: "oxide-ambient-v1.png",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/resources/backgrounds/oxide-ambient-v1.png"
        )),
    },
    BundledWorkspaceBackground {
        file_name: "oxide-nocturne-v1.webp",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/resources/backgrounds/oxide-nocturne-v1.webp"
        )),
    },
    BundledWorkspaceBackground {
        file_name: "oxide-verdant-v1.webp",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/resources/backgrounds/oxide-verdant-v1.webp"
        )),
    },
    BundledWorkspaceBackground {
        file_name: "oxide-dawn-mist-v1.mp4",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/resources/backgrounds/oxide-dawn-mist-v1.mp4"
        )),
    },
    BundledWorkspaceBackground {
        file_name: "oxide-night-flow-v1.mp4",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/resources/backgrounds/oxide-night-flow-v1.mp4"
        )),
    },
];

pub(in crate::workspace) fn ensure_bundled_workspace_backgrounds(
    settings_path: &Path,
) -> Result<()> {
    for background in BUNDLED_WORKSPACE_BACKGROUNDS {
        ensure_bundled_background_image(settings_path, background.file_name, background.bytes)?;
    }
    Ok(())
}

pub(in crate::workspace) fn is_bundled_workspace_background(
    settings_path: &Path,
    image_path: &Path,
) -> bool {
    let background_directory = background_images_directory(settings_path);
    BUNDLED_WORKSPACE_BACKGROUNDS
        .iter()
        .any(|background| image_path == background_directory.join(background.file_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BackgroundTestView {
        background: TerminalBackgroundPreferences,
        image: Arc<RenderImage>,
    }

    impl Render for BackgroundTestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            workspace_background_image_layer(self.background.clone(), Some(self.image.clone()))
        }
    }

    #[gpui::test]
    fn workspace_background_animation_paints_multiple_frames(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        assert!(cx.update(|window, _| window.is_window_active()));
        let image = Arc::new(RenderImage::new(vec![
            image::Frame::new(image::RgbaImage::from_pixel(
                1,
                1,
                image::Rgba([0, 0, 255, 255]),
            )),
            image::Frame::new(image::RgbaImage::from_pixel(
                1,
                1,
                image::Rgba([255, 0, 0, 255]),
            )),
        ]));
        let background = TerminalBackgroundPreferences {
            path: "animation.webp".into(),
            opacity: 1.0,
            blur: 0.0,
            fit: TerminalBackgroundFit::Cover,
            alignment: (0.5, 0.5),
            effect: None,
            readability: None,
            limits: Default::default(),
            on_failure: None,
            scene: Default::default(),
        };
        let view = cx.update(|_, cx| {
            cx.new(|_| BackgroundTestView {
                background,
                image: image.clone(),
            })
        });
        for expected_all_frames in [false, true] {
            cx.draw(
                gpui::point(px(0.0), px(0.0)),
                gpui::size(px(64.0), px(64.0)),
                |_, _| view.clone().into_any_element(),
            );
            assert_eq!(
                cx.update(|window, _| window.has_image_atlas_entry(&image)),
                expected_all_frames
            );
        }
    }

    #[test]
    fn recognizes_every_bundled_background_as_protected() {
        let settings_path = Path::new("/profile/settings.json");
        let background_directory = background_images_directory(settings_path);

        for background in BUNDLED_WORKSPACE_BACKGROUNDS {
            assert!(is_bundled_workspace_background(
                settings_path,
                &background_directory.join(background.file_name),
            ));
        }
        assert!(!is_bundled_workspace_background(
            settings_path,
            &background_directory.join("user-background.webp"),
        ));
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn sync_system_appearance(
        &mut self,
        appearance: gpui::WindowAppearance,
        cx: &mut Context<Self>,
    ) {
        let dark = matches!(
            appearance,
            gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark
        );
        if self.system_dark == dark {
            return;
        }
        let previous_theme = oxideterm_settings_model::ThemeTarget::Application
            .resolved_id(self.settings_store.settings(), self.system_dark)
            .to_string();
        self.system_dark = dark;
        let settings = self.settings_store.settings();
        self.tokens = tokens_from_settings(settings, dark);
        self.active_background = settings.resolved_background(dark);
        // Appearance events update visual state without saving settings or resetting PTYs.
        let panes: Vec<_> = self
            .tab_host
            .read(cx)
            .panes()
            .iter()
            .map(|(id, pane)| (*id, pane.clone()))
            .collect();
        for (id, pane) in panes {
            let preferences = self.terminal_preferences_for_pane(id, cx);
            pane.update(cx, |pane, cx| {
                pane.set_appearance(
                    preferences.theme,
                    preferences.background,
                    preferences.transparent_background,
                    cx,
                )
            });
        }
        self.sync_terminal_command_sender_appearance(cx);
        self.apply_ide_runtime_settings_to_surfaces(cx);
        let theme = oxideterm_settings_model::ThemeTarget::Application
            .resolved_id(self.settings_store.settings(), dark)
            .to_string();
        if theme != previous_theme {
            self.emit_native_plugin_event_to_subscribers(plugin_host::NATIVE_PLUGIN_APP_THEME_CHANGED_EVENT,
                serde_json::json!({ "theme": crate::workspace::plugin_lifecycle::native_plugin_theme_snapshot(&theme) }), cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn render_workspace_window_background(
        &mut self,
        window_background: &Entity<window_shell::WorkspaceWindowBackgroundEntity>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let background = self.window_background_preferences()?;
        Some(self.render_workspace_background_layer(window_background, background, window, cx))
    }

    pub(in crate::workspace) fn wrap_content_background(
        &mut self,
        window_background: &Entity<window_shell::WorkspaceWindowBackgroundEntity>,
        content: AnyElement,
        background_key: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(background_key) = background_key else {
            return content;
        };
        if matches!(background_key, "terminal" | "local_terminal") {
            return content;
        }
        let Some(background) = self.terminal_background_preferences(background_key) else {
            return content;
        };
        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .child(self.render_workspace_background_layer(
                window_background,
                background,
                window,
                cx,
            ))
            .child(div().relative().size_full().child(content))
            .into_any_element()
    }

    fn render_workspace_background_layer(
        &mut self,
        window_background: &Entity<window_shell::WorkspaceWindowBackgroundEntity>,
        mut background: TerminalBackgroundPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let byte_limit = self.render_policy.image_cache_bytes;
        background.on_failure = Some(self.background_failure_handler(cx));
        window_background.update(cx, |window_background, cx| {
            window_background.cache.set_byte_limit(byte_limit);
            window_background.render_layer(background, window, cx)
        })
    }
}

pub(in crate::workspace) fn background_preferences_for_style(
    style: &oxideterm_settings::BackgroundStyle,
    tokens: &ThemeTokens,
) -> Option<TerminalBackgroundPreferences> {
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
    Some(TerminalBackgroundPreferences {
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
        fit: terminal_background_fit(style.fit),
        limits: oxideterm_gpui_background::PlaybackLimits {
            max_width: style.max_width,
            max_height: style.max_height,
            max_fps: style.max_fps,
        },
        on_failure: None,
    })
}

impl window_shell::WorkspaceWindowBackgroundEntity {
    fn render_layer(
        &mut self,
        background: TerminalBackgroundPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let display = background_display_target(window.bounds().size, window.scale_factor());
        let image = if oxideterm_gpui_background::is_streaming_source(&background.path) {
            None
        } else {
            self.cache.render_background_image(&background, display)
        };
        self.drop_retired_images(Some(window), cx);
        if self.cache.has_pending() {
            self.schedule_decode_completion(cx);
        }
        oxideterm_gpui_background::background_layer(background, image, window, cx)
    }

    fn schedule_decode_completion(&mut self, cx: &mut Context<Self>) {
        if self.decode_completion_task.is_some() {
            return;
        }
        // Each shell owns its cache completion task, so releasing one native
        // window cannot keep repaint work alive through the shared session.
        self.decode_completion_task = Some(cx.spawn(async move |window_background, cx| {
            Timer::after(Duration::from_millis(16)).await;
            let _ = window_background.update(cx, |window_background, cx| {
                window_background.decode_completion_task = None;
                if window_background.cache.drain_completed() {
                    window_background.drop_retired_images(None, cx);
                    cx.notify();
                }
                if window_background.cache.has_pending() {
                    window_background.schedule_decode_completion(cx);
                }
            });
        }));
    }

    fn drop_retired_images(&mut self, mut window: Option<&mut Window>, cx: &mut Context<Self>) {
        for image in self.cache.take_retired_images() {
            // RenderImage entries painted by gpui::img also stay in the atlas
            // until the app explicitly drops their image id.
            if let Some(window) = window.as_mut() {
                cx.drop_image(image, Some(*window));
            } else {
                cx.drop_image(image, None);
            }
        }
    }
}

#[cfg(test)]
use oxideterm_gpui_background::background_image_layer as workspace_background_image_layer;

pub(in crate::workspace) fn default_connections_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("connections.json")
}

pub(in crate::workspace) fn default_saved_forwards_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("forwards.json")
}

pub(in crate::workspace) fn default_session_tree_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("session_tree.json")
}

pub(in crate::workspace) fn default_ai_conversations_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("chat_history.redb")
}
