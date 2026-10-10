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
    let image = if background.fit == BackgroundFit::Tile && background.blur <= 0.01 {
        gpui::img(background.path.clone())
            .with_fallback(|| div().size_full().into_any_element())
            .id(Arc::<std::path::Path>::from(background.path))
    } else if let Some(image) = image {
        let id = ("background-image", image.id.0);
        gpui::img(image).id(id)
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

    #[gpui::test]
    fn every_effect_keeps_the_underlying_image_visible(cx: &mut gpui::TestAppContext) {
        for kind in [
            GeneratedEffectKind::Mineral,
            GeneratedEffectKind::Fog,
            GeneratedEffectKind::Tide,
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
