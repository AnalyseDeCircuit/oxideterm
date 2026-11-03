#![cfg(target_os = "windows")]

use std::{
    borrow::Cow,
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use gpui::{
    AppContext, AtlasKey, AtlasTile, Bounds, ContentMask, Context, DecorationRun, DevicePixels,
    FontStyle, FontWeight, HeadlessAppContext, HeadlessAtlas, IntoElement, LineLayout,
    LinePaintCache, MonochromeSprite, Pixels, PlatformAtlas, PlatformHeadlessRenderer, Point,
    PolychromeSprite, Render, Scene, Size, StrikethroughStyle, Styled, SubpixelSprite, TextAlign,
    TextRun, Underline, UnderlineStyle, Window, WindowTextSystem, black, canvas, font, hsla, point,
    px, size,
};

#[derive(Default)]
struct CountingAtlas {
    inner: HeadlessAtlas,
    lookups: AtomicUsize,
}

impl PlatformAtlas for CountingAtlas {
    fn get_or_insert_with<'a>(
        &self,
        key: AtlasKey,
        build: &mut dyn FnMut() -> anyhow::Result<Option<(Size<DevicePixels>, Cow<'a, [u8]>)>>,
    ) -> anyhow::Result<Option<AtlasTile>> {
        self.lookups.fetch_add(1, Ordering::Relaxed);
        self.inner.get_or_insert_with(key, build)
    }
    fn remove(&self, key: &AtlasKey) {
        self.inner.remove(key);
    }
    fn contains(&self, key: &AtlasKey) -> bool {
        self.inner.contains(key)
    }
}

#[derive(Default)]
struct PaintedScene {
    monochrome: Vec<MonochromeSprite>,
    subpixel: Vec<SubpixelSprite>,
    polychrome: Vec<PolychromeSprite>,
    underlines: Vec<Underline>,
}

struct SceneRecorder {
    atlas: Arc<CountingAtlas>,
    captured: Rc<RefCell<Option<PaintedScene>>>,
}

impl PlatformHeadlessRenderer for SceneRecorder {
    fn render_scene(&mut self, scene: &Scene, _: Size<DevicePixels>) -> anyhow::Result<()> {
        *self.captured.borrow_mut() = Some(PaintedScene {
            monochrome: scene.monochrome_sprites.clone(),
            subpixel: scene.subpixel_sprites.clone(),
            polychrome: scene.polychrome_sprites.clone(),
            underlines: scene.underlines.clone(),
        });
        Ok(())
    }
    fn render_scene_to_image(
        &mut self,
        scene: &Scene,
        size: Size<DevicePixels>,
    ) -> anyhow::Result<image::RgbaImage> {
        self.render_scene(scene, size)?;
        // Only draw commands are observed; this recorder does not rasterize a screenshot.
        Ok(image::RgbaImage::new(0, 0))
    }
    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.atlas.clone()
    }
}

struct PaintState {
    layout: Arc<LineLayout>,
    decorations: Vec<DecorationRun>,
    cache: LinePaintCache,
    cached: bool,
    opacity: f32,
    origin: Point<Pixels>,
}

struct PaintView(Rc<RefCell<PaintState>>);

impl Render for PaintView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let state = self.0.clone();
        let opacity = state.borrow().opacity;
        canvas(
            |_, _, _| {},
            move |_, _, window, cx| {
                let state = state.borrow();
                let origin = state.origin;
                let mask = ContentMask {
                    bounds: Bounds::new(point(px(15.), px(0.)), size(px(65.), px(80.))),
                };
                window.with_content_mask(Some(mask), |window| {
                    // The outer layer represents the terminal row; decorated runs
                    // must preserve the reference painter's nested ordering.
                    window.paint_layer(
                        Bounds::new(Point::default(), size(px(160.), px(80.))),
                        |window| {
                            if state.cached {
                                state
                                    .layout
                                    .paint_cached(
                                        origin,
                                        px(24.),
                                        &state.decorations,
                                        &state.cache,
                                        window,
                                        cx,
                                    )
                                    .unwrap();
                            } else {
                                state
                                    .layout
                                    .paint(
                                        origin,
                                        px(24.),
                                        TextAlign::Left,
                                        None,
                                        &state.decorations,
                                        window,
                                        cx,
                                    )
                                    .unwrap();
                            }
                        },
                    );
                });
            },
        )
        .size_full()
        .opacity(opacity)
    }
}

#[test]
#[ignore = "requires Windows DirectWrite and installed fonts; run explicitly on the host"]
fn native_cached_decorations_match_reference_glyphs() {
    let native = gpui_platform::current_platform(false);
    let atlas = Arc::new(CountingAtlas::default());
    let captured = Rc::new(RefCell::new(None));
    let mut cx = HeadlessAppContext::with_platform(native.text_system(), Arc::new(()), {
        let atlas = atlas.clone();
        let captured = captured.clone();
        move || {
            Ok(Some(Box::new(SceneRecorder {
                atlas: atlas.clone(),
                captured: captured.clone(),
            })))
        }
    });
    let initial =
        WindowTextSystem::new(cx.text_system().clone()).layout_line("", px(16.), &[], None);
    let state = Rc::new(RefCell::new(PaintState {
        layout: initial,
        decorations: vec![DecorationRun {
            len: 0,
            color: black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        cache: LinePaintCache::default(),
        cached: false,
        opacity: 1.,
        origin: point(px(12.25), px(12.25)),
    }));
    let window = cx
        .open_window(size(px(160.), px(80.)), {
            let state = state.clone();
            move |_, cx| cx.new(|_| PaintView(state))
        })
        .unwrap();
    for text in ["cache e\u{301} 中文", "A😀B"] {
        for italic in [false, true] {
            let mut text_font = font("Consolas");
            if italic {
                text_font.style = FontStyle::Italic;
                text_font.weight = FontWeight::BOLD;
            }
            let color = hsla(0.6, 0.4, 0.7, 0.8);
            let layout = cx
                .update_window(window.into(), |_, window, _| {
                    window.text_system().layout_line(
                        text,
                        px(16.),
                        &[TextRun {
                            len: text.len(),
                            font: text_font,
                            color,
                            ..Default::default()
                        }],
                        None,
                    )
                })
                .unwrap();
            for shifted_glyph in [false, true] {
                let mut layout = LineLayout {
                    font_size: layout.font_size,
                    width: layout.width,
                    ascent: layout.ascent,
                    descent: layout.descent,
                    runs: layout.runs.clone(),
                    len: layout.len,
                };
                if shifted_glyph {
                    // Non-baseline glyphs must retain exact device-phase invalidation.
                    layout.runs[0].glyphs[0].position.y += px(0.375);
                }
                let shared_baseline = layout
                    .runs
                    .iter()
                    .flat_map(|run| &run.glyphs)
                    .all(|glyph| glyph.position.y == px(0.));
                state.borrow_mut().layout = Arc::new(layout);
                for segmented in [false, true] {
                    state.borrow_mut().cache = LinePaintCache::default();
                    for scale in [1., 1.25, 1.5, 2., 3.] {
                        for (offset_index, offset) in
                            [0., 0.125, 0.5, 0.875, 1.25, -1.5].into_iter().enumerate()
                        {
                            state.borrow_mut().origin.y = px(12.25 + offset);
                            for (variant, opacity) in [0.25, 0.75].into_iter().enumerate() {
                                {
                                    let mut state = state.borrow_mut();
                                    state.opacity = opacity;
                                    let decoration = DecorationRun {
                                        len: text.len() as u32,
                                        color,
                                        background_color: None,
                                        underline: Some(UnderlineStyle {
                                            color: (variant == 1)
                                                .then_some(hsla(0.1, 0.7, 0.5, 0.9)),
                                            thickness: px(1. + variant as f32),
                                            wavy: variant == 1,
                                        }),
                                        strikethrough: Some(StrikethroughStyle {
                                            color: None,
                                            thickness: px(1.),
                                        }),
                                    };
                                    state.decorations = if segmented {
                                        vec![
                                            DecorationRun {
                                                len: 1,
                                                color: hsla(2.25, 0.4, 0.7, 0.8),
                                                underline: None,
                                                strikethrough: None,
                                                ..decoration.clone()
                                            },
                                            DecorationRun {
                                                len: text.len() as u32 - 1,
                                                color: hsla(-0.25, 0.4, 0.7, 0.8),
                                                underline: None,
                                                strikethrough: None,
                                                ..decoration
                                            },
                                        ]
                                    } else {
                                        vec![decoration]
                                    };
                                }
                                let mut frames = Vec::new();
                                for cached in [false, true] {
                                    state.borrow_mut().cached = cached;
                                    let before = atlas.lookups.load(Ordering::Relaxed);
                                    cx.update_window(window.into(), |_, window, cx| {
                                        window.set_scale_factor(scale);
                                        window.refresh();
                                        window.draw(cx).clear(cx);
                                    })
                                    .unwrap();
                                    cx.capture_screenshot(window.into()).unwrap();
                                    if cached
                                        && (variant == 1 || (offset_index > 0 && shared_baseline))
                                    {
                                        assert_eq!(
                                            atlas.lookups.load(Ordering::Relaxed),
                                            before,
                                            "opacity, stroke changes and shared-baseline scrolling must reuse native glyph tiles"
                                        );
                                    }
                                    frames.push(
                                        captured.borrow_mut().take().expect("rendered scene"),
                                    );
                                }
                                let [reference, actual] = frames.as_slice() else {
                                    unreachable!()
                                };
                                assert!(
                                    !reference.monochrome.is_empty()
                                        || !reference.subpixel.is_empty(),
                                    "reference must contain actual native glyphs"
                                );
                                macro_rules! compare {
                        ($field:ident; $($member:ident),+) => {
                            assert_eq!(actual.$field.iter().map(|s| ($(s.$member,)+)).collect::<Vec<_>>(), reference.$field.iter().map(|s| ($(s.$member,)+)).collect::<Vec<_>>(), "{}: {text}, italic={italic}, scale={scale}, segmented={segmented}", stringify!($field));
                        };
                    }
                                compare!(monochrome; order, pad, bounds, content_mask, color, tile, transformation);
                                compare!(subpixel; order, pad, bounds, content_mask, color, tile, transformation);
                                compare!(polychrome; order, pad, bounds, content_mask, opacity, grayscale, corner_radii, tile);
                                compare!(underlines; order, pad, bounds, content_mask, color, thickness, wavy);
                            }
                        }
                    }
                }
            }
        }
    }
}
