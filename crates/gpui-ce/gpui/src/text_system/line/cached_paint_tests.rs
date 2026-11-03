use super::tests::{
    assert_underline_primitives_eq, test_underline_handler_at_scales, underline_test_line,
};
use super::*;
use crate::{ContentMask, ShapedRun, TestAppContext, hsla};

fn decoration(len: usize) -> DecorationRun {
    DecorationRun {
        len: len as u32,
        color: hsla(0.6, 0.4, 0.7, 0.8),
        background_color: None,
        underline: Some(UnderlineStyle {
            color: None,
            thickness: px(1.),
            wavy: false,
        }),
        strikethrough: Some(StrikethroughStyle {
            color: None,
            thickness: px(1.),
        }),
    }
}

#[gpui::test]
fn cached_uniform_decorations_preserve_geometry_and_live_styles(cx: &mut TestAppContext) {
    test_underline_handler_at_scales(cx, |window, cx| {
        for zero_advance in [false, true] {
            for leading_empty_run in [false, true] {
                let text = "aébc";
                let mut line =
                    underline_test_line(text, &[decoration(text.len())], zero_advance, window);
                let layout = Arc::get_mut(&mut line.layout).unwrap();
                for glyph in &mut layout.runs[0].glyphs {
                    glyph.position.x += px(3.25);
                }
                if leading_empty_run {
                    let font_id = layout.runs[0].font_id;
                    layout.runs.insert(
                        0,
                        ShapedRun {
                            font_id,
                            glyphs: Vec::new(),
                        },
                    );
                }
                let cache = LinePaintCache::default();
                let mut first_batch = None;
                for variant in 0..3 {
                    let mut style = decoration(text.len());
                    if variant > 0 {
                        style.underline = Some(UnderlineStyle {
                            color: Some(hsla(0.1, 0.9, 0.5, 0.7)),
                            thickness: px(2.),
                            wavy: true,
                        });
                        style.strikethrough = Some(StrikethroughStyle {
                            color: Some(hsla(0.8, 0.6, 0.3, 0.9)),
                            thickness: px(3.),
                        });
                    }
                    if variant == 2 {
                        style.underline = None;
                    }
                    let origin = point(px(20.25), px(12.25));
                    let row = Bounds::new(point(px(0.), px(0.)), size(px(120.), px(80.)));
                    let mask = ContentMask {
                        bounds: Bounds::new(Point::default(), size(px(50.), px(80.))),
                    };
                    window.with_content_mask(Some(mask), |window| {
                        window.with_element_opacity(
                            Some(if variant == 0 { 0.75 } else { 0.25 }),
                            |window| {
                                window.next_frame.scene.clear();
                                window.paint_layer(row, |window| {
                                    line.layout
                                        .paint(
                                            origin,
                                            px(20.),
                                            TextAlign::Left,
                                            None,
                                            std::slice::from_ref(&style),
                                            window,
                                            cx,
                                        )
                                        .unwrap();
                                });
                                let reference = window.next_frame.scene.underlines.clone();
                                window.next_frame.scene.clear();
                                window.paint_layer(row, |window| {
                                    if variant == 0 {
                                        line.layout
                                            .paint_cached(
                                                origin,
                                                px(20.),
                                                std::slice::from_ref(&style),
                                                &cache,
                                                window,
                                                cx,
                                            )
                                            .unwrap();
                                    } else {
                                        line.layout
                                            .paint_cached_in_current_layer(
                                                origin,
                                                px(20.),
                                                std::slice::from_ref(&style),
                                                &cache,
                                                window,
                                                cx,
                                            )
                                            .unwrap();
                                    }
                                });
                                assert_underline_primitives_eq(
                                    &window.next_frame.scene.underlines,
                                    &reference,
                                );
                            },
                        );
                    });
                    let batch = cache
                        .prepared
                        .lock()
                        .as_ref()
                        .expect("uniform decorations use the glyph cache")
                        .glyphs
                        .clone();
                    if let Some(first) = &first_batch {
                        assert!(
                            Arc::ptr_eq(first, &batch),
                            "stroke style and opacity must not evict glyphs"
                        );
                    } else {
                        first_batch = Some(batch);
                    }
                }
                let first_batch = first_batch.unwrap();
                let mut style = decoration(text.len());
                style.color = hsla(0.2, 0.8, 0.4, 1.);
                line.layout
                    .paint_cached(
                        point(px(20.25), px(12.25)),
                        px(20.),
                        &[style.clone()],
                        &cache,
                        window,
                        cx,
                    )
                    .unwrap();
                let recolored = cache.prepared.lock().as_ref().unwrap().glyphs.clone();
                assert!(
                    !Arc::ptr_eq(&first_batch, &recolored),
                    "foreground changes invalidate glyph resources"
                );
                line.layout
                    .paint_cached(
                        point(px(20.25), px(12.25)),
                        px(24.),
                        &[style],
                        &cache,
                        window,
                        cx,
                    )
                    .unwrap();
                assert!(
                    !Arc::ptr_eq(&recolored, &cache.prepared.lock().as_ref().unwrap().glyphs),
                    "line height changes the glyph baseline"
                );
            }
        }
    });
}

#[gpui::test]
fn cached_partial_decorations_preserve_general_painter_boundaries(cx: &mut TestAppContext) {
    test_underline_handler_at_scales(cx, |window, cx| {
        for styles in [
            vec![decoration(1)],
            vec![
                decoration(1),
                DecorationRun {
                    underline: None,
                    ..decoration(3)
                },
            ],
        ] {
            let line = underline_test_line("abcd", &styles, false, window);
            let origin = point(px(12.25), px(10.25));
            window.next_frame.scene.clear();
            line.layout
                .paint(origin, px(20.), TextAlign::Left, None, &styles, window, cx)
                .unwrap();
            let reference = window.next_frame.scene.underlines.clone();
            window.next_frame.scene.clear();
            line.layout
                .paint_cached(
                    origin,
                    px(20.),
                    &styles,
                    &LinePaintCache::default(),
                    window,
                    cx,
                )
                .unwrap();
            assert_underline_primitives_eq(&window.next_frame.scene.underlines, &reference);
        }
    });
}
