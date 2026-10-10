use crate::{BackgroundFit, BackgroundPreferences};
use gpui::{
    AnyElement, Bounds, DevicePixels, ObjectFit, Pixels, RenderImage, Size, StyledImage, div,
    prelude::*,
};
use std::sync::Arc;

pub fn background_object_fit(fit: BackgroundFit) -> ObjectFit {
    match fit {
        BackgroundFit::Cover => ObjectFit::Cover,
        BackgroundFit::Contain => ObjectFit::Contain,
        BackgroundFit::Fill => ObjectFit::Fill,
        BackgroundFit::Tile => ObjectFit::None,
    }
}

pub fn background_image_layer(
    background: BackgroundPreferences,
    image: Option<Arc<RenderImage>>,
) -> AnyElement {
    let image = if let Some(image) = image {
        let id = ("background-image", image.id.0);
        gpui::img(image).id(id)
    } else if background.fit == BackgroundFit::Tile && background.blur <= 0.01 {
        gpui::img(background.path.clone())
            .with_fallback(|| div().size_full().into_any_element())
            .id(Arc::<std::path::Path>::from(background.path))
    } else {
        return div().absolute().inset_0().into_any_element();
    };
    div()
        .absolute()
        .inset_0()
        .overflow_hidden()
        .child(
            image
                .size_full()
                .object_fit(background_object_fit(background.fit))
                .object_position(background.alignment)
                .opacity(background.opacity.clamp(0.0, 1.0)),
        )
        .into_any_element()
}

pub(crate) fn aligned_background_bounds(
    bounds: Bounds<Pixels>,
    image_size: Size<DevicePixels>,
    fit: BackgroundFit,
    alignment: (f32, f32),
) -> Bounds<Pixels> {
    background_object_fit(fit).get_aligned_bounds(bounds, image_size, alignment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GeneratedEffectKind, GeneratedEffectPreferences, ReadingOverlay};
    use gpui::{Context, Render, Window, px};

    struct ComposedBackground {
        background: BackgroundPreferences,
        image: Arc<RenderImage>,
    }

    impl Render for ComposedBackground {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div().relative().size_full().child(crate::background_layer(
                self.background.clone(),
                Some(self.image.clone()),
                window,
                cx,
            ))
        }
    }

    fn input_background(kind: GeneratedEffectKind) -> ComposedBackground {
        ComposedBackground {
            background: BackgroundPreferences {
                path: Default::default(),
                opacity: 1.0,
                blur: 0.0,
                fit: BackgroundFit::Cover,
                alignment: (0.5, 0.5),
                effect: Some(GeneratedEffectPreferences {
                    kind,
                    strength: 1.0,
                    sheen: 0.5,
                    max_fps: None,
                    colors: [0x8090aa, 0x90aa80],
                    speed: 1.0,
                    size: 1.0,
                    brightness: 1.0,
                    roughness: 0.5,
                    direction: 25.0,
                    particle_count: 12,
                    preview_viewport: None,
                }),
                readability: None,
                limits: Default::default(),
                on_failure: None,
                scene: crate::ScenePreferences {
                    pause_on_input: true,
                    ..Default::default()
                },
            },
            image: Arc::new(RenderImage::new(vec![image::Frame::new(
                image::RgbaImage::new(1, 1),
            )])),
        }
    }

    #[gpui::test]
    fn typing_pause_restarts_effect_motion_with_acceleration(cx: &mut gpui::TestAppContext) {
        use std::time::Duration;
        let (_, cx) = cx.add_window_view(|_, _| input_background(GeneratedEffectKind::Mineral));
        cx.simulate_resize(gpui::size(px(640.0), px(360.0)));
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        let draw_phase = |window: &mut Window, cx: &mut gpui::App| {
            window.draw(cx).clear(cx);
            window
                .painted_quads()
                .iter()
                .find_map(|quad| match quad.background.kind() {
                    gpui::BackgroundKind::ProceduralNoise { phase, .. } => Some(phase),
                    _ => None,
                })
                .expect("the live effect must be painted")
        };
        cx.update(draw_phase);
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
        cx.update(|window, cx| crate::note_input_activity(window.window_handle().window_id(), cx));
        cx.run_until_parked();
        let paused = cx.update(draw_phase);
        cx.executor().advance_clock(Duration::from_millis(800));
        cx.run_until_parked();
        assert_eq!(cx.update(draw_phase), paused);
        // A second keystroke extends the pause; no terminal contents are involved.
        cx.update(|window, cx| crate::note_input_activity(window.window_handle().window_id(), cx));
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        assert_eq!(cx.update(draw_phase), paused);
        for (advance_ms, elapsed_motion) in [(200, 0.0375), (200, 0.2), (100, 0.3)] {
            cx.executor()
                .advance_clock(Duration::from_millis(advance_ms));
            cx.run_until_parked();
            let actual = cx.update(draw_phase);
            assert!(
                (actual - paused - elapsed_motion * 0.07).abs() < 0.00001,
                "painted phase must follow the eased clock: {actual}, paused {paused}, motion {elapsed_motion}"
            );
        }
    }

    #[gpui::test]
    fn particles_have_pointer_depth_without_a_still_image_and_respect_input_pause(
        cx: &mut gpui::TestAppContext,
    ) {
        use std::time::Duration;
        let (view, cx) = cx.add_window_view(|_, _| {
            let mut view = input_background(GeneratedEffectKind::Particles);
            view.background.scene.parallax = true;
            // Isolate pointer motion from ambient drift when comparing painted locations.
            view.background.effect.as_mut().unwrap().speed = 0.0;
            view
        });
        cx.simulate_resize(gpui::size(px(640.0), px(360.0)));
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        let centers = |window: &mut Window, cx: &mut gpui::App| {
            window.draw(cx).clear(cx);
            window
                .painted_quads()
                .iter()
                .map(|quad| {
                    quad.bounds
                        .center()
                        .map(|value| px(value.0 / window.scale_factor()))
                })
                .collect::<Vec<_>>()
        };
        let before = cx.update(centers);
        cx.simulate_mouse_move(gpui::point(px(620.0), px(180.0)), None, Default::default());
        for _ in 0..8 {
            cx.executor().advance_clock(Duration::from_millis(16));
            cx.run_until_parked();
            cx.update(centers);
        }
        let shifted = cx.update(centers);
        assert_eq!(before.len(), shifted.len());
        let travel: Vec<_> = before
            .iter()
            .zip(&shifted)
            .map(|(a, b)| (b.x - a.x).as_f32())
            .collect();
        assert!(travel.iter().all(|shift| *shift >= 0.0));
        assert!(
            travel.iter().any(|shift| *shift > 3.0),
            "midground must react to the pointer"
        );
        assert!(
            travel.iter().any(|shift| *shift > 0.0 && *shift < 3.0),
            "far particles must have smaller parallax"
        );
        cx.update(|window, cx| crate::note_input_activity(window.window_handle().window_id(), cx));
        cx.run_until_parked();
        let paused = cx.update(centers);
        cx.simulate_mouse_move(gpui::point(px(20.0), px(180.0)), None, Default::default());
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
        assert_eq!(
            cx.update(centers),
            paused,
            "typing must freeze parallax too"
        );
        view.update(cx, |view, cx| {
            view.background.scene.parallax = false;
            cx.notify();
        });
        assert_eq!(
            cx.update(centers),
            before,
            "disabling parallax restores unshifted positions"
        );
    }

    #[gpui::test]
    fn every_effect_keeps_the_underlying_image_visible(cx: &mut gpui::TestAppContext) {
        for kind in [
            GeneratedEffectKind::Mineral,
            GeneratedEffectKind::Fog,
            GeneratedEffectKind::Tide,
            GeneratedEffectKind::TideParticles,
            GeneratedEffectKind::Meteor,
            GeneratedEffectKind::Particles,
            GeneratedEffectKind::Caustics,
        ] {
            let image = Arc::new(RenderImage::new(vec![image::Frame::new(
                image::RgbaImage::from_pixel(8, 8, image::Rgba([30, 60, 90, 255])),
            )]));
            let (view, window_cx) = cx.add_window_view(|_, _| ComposedBackground {
                background: BackgroundPreferences {
                    path: "background.png".into(),
                    opacity: 0.3,
                    blur: 0.0,
                    fit: BackgroundFit::Cover,
                    alignment: (0.5, 0.5),
                    effect: Some(GeneratedEffectPreferences {
                        kind,
                        strength: 0.4,
                        sheen: 0.5,
                        max_fps: None,
                        colors: [0x8090aa, 0x90aa80],
                        speed: 0.0,
                        size: 1.0,
                        brightness: if kind == GeneratedEffectKind::Mineral {
                            0.0
                        } else {
                            1.0
                        },
                        roughness: 0.5,
                        direction: 90.0,
                        particle_count: 12,
                        preview_viewport: None,
                    }),
                    readability: Some(ReadingOverlay {
                        color: 0x102030,
                        opacity: 0.2,
                    }),
                    limits: Default::default(),
                    scene: crate::ScenePreferences {
                        pause_on_input: true,
                        parallax: true,
                        day_cycle: true,
                        preview_hour: Some(6.0),
                        ..Default::default()
                    },
                    on_failure: None,
                },
                image: image.clone(),
            });
            window_cx.simulate_resize(gpui::size(px(320.0), px(180.0)));
            window_cx.update(|window, cx| window.draw(cx).clear(cx));
            assert!(
                window_cx.update(|window, _| window.has_image_atlas_entry(&image)),
                "{kind:?} must paint the selected image as well as its effect"
            );
            window_cx.update(|window, _| {
                let quads = window.painted_quads();
                assert!(
                    quads.iter().any(|quad| {
                        quad.background.as_solid() == Some(gpui::rgba(0xf3be820b).into())
                    }),
                    "the morning tint must be composed with the selected image"
                );
                let mask = quads
                    .last()
                    .expect("readability mask is drawn above the effect");
                assert_eq!(
                    mask.background.as_solid(),
                    Some(gpui::rgba(0x10203033).into())
                );
                if kind == GeneratedEffectKind::Mineral {
                    let noise = quads
                        .iter()
                        .find_map(|quad| match quad.background.kind() {
                            gpui::BackgroundKind::ProceduralNoise { colors, sheen, .. } => {
                                Some((colors, sheen))
                            }
                            _ => None,
                        })
                        .expect("texture overlay is drawn independently");
                    assert!(
                        (noise.0[0].a - 0.64).abs() < 0.001,
                        "media overlays use enhanced strength independently of the image's 0.3 opacity"
                    );
                    assert_eq!(noise.1, 0.5, "opacity must preserve the highlight strength");
                }
            });
            drop(view);
        }
    }
}
