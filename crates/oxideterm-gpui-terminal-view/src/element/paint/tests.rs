use super::*;

#[test]
fn sixel_clips_partial_cell_rows_without_stretching() {
    let bounds = Bounds::new(point(px(4.0), px(8.0)), size(px(20.0), px(7.0)));
    assert_eq!(
        sixel_clip_bounds(
            bounds,
            size(2, 2),
            point(0, 0),
            point(0, 1),
            size(px(10.0), px(6.0))
        ),
        Bounds::new(point(px(4.0), px(8.0)), size(px(20.0), px(6.0)))
    );
    let narrow = Bounds::new(point(px(-4.0), px(8.0)), size(px(7.0), px(6.0)));
    assert_eq!(
        sixel_clip_bounds(
            narrow,
            size(2, 1),
            point(1, 0),
            point(0, 0),
            size(px(4.0), px(6.0))
        ),
        Bounds::new(point(px(0.0), px(8.0)), size(px(3.0), px(6.0)))
    );
    assert_eq!(
        sixel_clip_bounds(
            bounds,
            size(2, 2),
            point(0, 1),
            point(0, 0),
            size(px(10.0), px(6.0))
        ),
        Bounds::new(point(px(4.0), px(14.0)), size(px(20.0), px(1.0)))
    );
}

#[test]
fn native_pixels_and_explicit_cell_boxes_have_distinct_sizes() {
    let cells = Bounds::new(point(px(20.0), px(40.0)), size(px(20.0), px(40.0)));
    assert_eq!(
        kitty_image_bounds(cells, size(8, 4), point(2, 4), false, 1.0),
        Bounds::new(point(px(22.0), px(44.0)), size(px(8.0), px(4.0)))
    );
    assert_eq!(
        kitty_image_bounds(cells, size(8, 4), point(2, 4), true, 1.0),
        Bounds::new(point(px(22.0), px(57.5)), size(px(18.0), px(9.0)))
    );
}

#[test]
fn retina_native_images_fit_their_physical_cell_box() {
    let cells = Bounds::new(point(px(10.0), px(20.0)), size(px(20.0), px(20.0)));
    let image = kitty_image_bounds(cells, size(40, 40), point(0, 0), false, 2.0);
    assert_eq!(image.size, size(px(20.0), px(20.0)));
    let sixel = Bounds::new(cells.origin, native_image_size(size(40, 40), 2.0));
    assert_eq!(
        sixel_clip_bounds(
            sixel,
            size(2, 2),
            point(0, 0),
            point(0, 0),
            size(px(10.0), px(10.0))
        ),
        sixel
    );
    let offset = kitty_image_bounds(cells, size(40, 20), point(2, 4), false, 2.0);
    assert_eq!(
        offset,
        Bounds::new(point(px(11.0), px(22.0)), size(px(20.0), px(10.0)))
    );
    assert_eq!(
        kitty_image_bounds(cells, size(40, 20), point(2, 4), true, 2.0),
        Bounds::new(point(px(11.0), px(26.25)), size(px(19.0), px(9.5)))
    );
}
#[test]
fn aligned_run_position_y_keeps_fallback_runs_on_primary_baseline() {
    let primary_ascent = px(14.28);
    let primary_descent = px(4.20);
    let line_height = px(16.63);
    let origin_y = px(20.0);

    let baseline_from = |pos_y: Pixels, ascent: Pixels, descent: Pixels| {
        pos_y + (line_height - ascent - descent) * 0.5 + ascent
    };

    let adjusted_y = aligned_run_position_y(
        origin_y,
        primary_ascent,
        primary_descent,
        px(15.50),
        px(6.10),
    );
    let primary_baseline = baseline_from(origin_y, primary_ascent, primary_descent);
    let adjusted_baseline = baseline_from(adjusted_y, px(15.50), px(6.10));

    assert!((f32::from(adjusted_baseline) - f32::from(primary_baseline)).abs() < 0.001);
}

#[test]
fn device_aligned_stroke_snaps_to_physical_pixels() {
    for (logical, scale, expected) in [
        (1.0, 1.0, 1.0),
        (1.0, 1.25, 0.8),
        (1.0, 1.5, 2.0 / 1.5),
        (2.0, 1.25, 2.4),
        (1.0, 2.0, 1.0),
    ] {
        let snapped = f32::from(device_aligned_stroke(logical, scale));
        assert!((snapped - expected).abs() < 0.001);
        let device_px = snapped * scale;
        assert!((device_px - device_px.round()).abs() < 0.001);
    }
}

#[test]
fn underline_top_offset_tracks_primary_font_baseline_inside_cell() {
    let mut metrics = TerminalMetrics {
        font: terminal_font_with_family_and_cjk(
            TERMINAL_FONT,
            None,
            TERMINAL_FONT_LIGATURES,
            TERMINAL_FONT_WEIGHT,
        ),
        font_size: px(14.0),
        cell_width: px(8.4),
        letter_spacing: None,
        line_height: px(18.48),
        primary_ascent: px(14.28),
        primary_descent: px(4.20),
    };
    let thickness = px(1.0);
    let expected = gpui::underline_y_offset(
        metrics.line_height,
        metrics.primary_ascent,
        metrics.primary_descent,
    );
    assert_eq!(underline_top_offset(&metrics, thickness), expected);

    // Compact line heights clamp the underline inside the row instead of bleeding into row+1.
    metrics.line_height = px(16.63);
    let max_top = metrics.line_height - thickness;
    assert_eq!(underline_top_offset(&metrics, thickness), max_top);

    metrics.primary_ascent = px(0.0);
    metrics.primary_descent = px(0.0);
    assert_eq!(underline_top_offset(&metrics, thickness), max_top);
}

#[test]
fn command_gutter_geometry_keeps_end_elbow_and_clearance_before_cell_zero() {
    let base_overlay = TerminalCommandMarkOverlay {
        start_row: 1,
        end_row: 4,
        has_top: true,
        has_bottom: true,
        stale: false,
        selected: false,
        hovered: false,
        running: false,
        exit_code: Some(0),
        collapsed: false,
        foldable: true,
        fold_rotation: 0.0,
    };
    let origin = point(px(24.0), px(10.0));
    let expanded = command_gutter_geometry(&base_overlay, origin, 20.0, 9.0, 2.0);
    assert_eq!(
        expanded.head.map(|(kind, _)| kind),
        Some(CommandGutterHead::Square)
    );
    assert_eq!(expanded.fold_rotation, 0.0);
    assert!(expanded.finished);
    assert!(expanded.elbow_right_x > expanded.spine_x);
    assert!(expanded.elbow_right_x <= f32::from(origin.x) - 1.2);
    assert!(expanded.bend_radius >= 1.1);
    assert!(expanded.rail_bottom > expanded.rail_top);

    let collapsed = command_gutter_geometry(
        &TerminalCommandMarkOverlay {
            collapsed: true,
            fold_rotation: 1.0,
            end_row: 2,
            ..base_overlay.clone()
        },
        origin,
        20.0,
        9.0,
        2.0,
    );
    assert_eq!(
        collapsed.head.map(|(kind, _)| kind),
        Some(CommandGutterHead::Square)
    );
    assert_eq!(collapsed.fold_rotation, 1.0);
    // Rotated 45-degree diamond projects sqrt(2) * half vertically, pushing rail_top slightly lower.
    assert!(collapsed.rail_top > expanded.rail_top);
    assert!(collapsed.finished);
    assert!(collapsed.rail_bottom > collapsed.rail_top);

    let running = command_gutter_geometry(
        &TerminalCommandMarkOverlay {
            running: true,
            has_bottom: false,
            foldable: false,
            ..base_overlay
        },
        origin,
        20.0,
        9.0,
        2.0,
    );
    assert_eq!(
        running.head.map(|(kind, _)| kind),
        Some(CommandGutterHead::Dot)
    );
    assert!(!running.finished);
    assert_eq!(running.bend_radius, 0.0);
}
