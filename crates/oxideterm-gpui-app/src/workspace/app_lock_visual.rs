// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::f32::consts::{PI, TAU};

use gpui::{
    AnyElement, Bounds, Corners, Hsla, IntoElement, PathBuilder, Pixels, Point, Styled, canvas,
    fill, point, px, size,
};
use oxideterm_gpui_ui::motion::{ease_in_out_cubic, ease_out_cubic, lerp};
use oxideterm_theme::AppUiColors;

pub(super) const APP_LOCK_ENTER_BASE_MS: u64 = 1040;
pub(super) const APP_LOCK_EXIT_BASE_MS: u64 = 600;
pub(super) const APP_LOCK_RITUAL_SIZE: f32 = 164.0;
pub(super) const APP_LOCK_CHAMBER_SIZE_WIDE: f32 = 132.0;
pub(super) const APP_LOCK_CHAMBER_SIZE_NARROW: f32 = 112.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PadlockTone {
    Open,
    Locked,
    Unlocked,
    Error,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PadlockVisual {
    /// 0.0 = shackle seated and locked, 1.0 = shackle lifted open.
    pub(super) shackle_lift: f32,
    /// Subtle vertical recoil in logical pixels when the shackle snaps shut.
    pub(super) body_dy: f32,
    /// 0.0 = key seated inside the cylinder, 1.0 = key pulled clear to the right.
    pub(super) key_pull: f32,
    /// Key rotation in radians around the keyhole center.
    pub(super) key_turn: f32,
    /// Key visibility (0.0..=1.0).
    pub(super) key_opacity: f32,
    /// Expanding lock/unlock confirmation ring progress (0.0 = hidden, 0.0..1.0 = expanding).
    pub(super) ring_progress: f32,
    /// Overall mechanism scale factor.
    pub(super) scale: f32,
    pub(super) tone: PadlockTone,
}

impl PadlockVisual {
    pub(super) fn settled_locked() -> Self {
        Self {
            shackle_lift: 0.0,
            body_dy: 0.0,
            key_pull: 1.0,
            key_turn: 0.0,
            key_opacity: 0.0,
            ring_progress: 0.0,
            scale: 1.0,
            tone: PadlockTone::Locked,
        }
    }

    pub(super) fn settled_error() -> Self {
        Self {
            tone: PadlockTone::Error,
            ..Self::settled_locked()
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AppLockEnterFrame {
    pub(super) ritual_visible: bool,
    pub(super) ritual_opacity: f32,
    pub(super) ritual_visual: PadlockVisual,
    /// 0.0 at stage center -> 1.0 settled inside the console's lock chamber.
    pub(super) handoff_progress: f32,
    pub(super) screen_opacity: f32,
    pub(super) screen_offset_y: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AppLockExitFrame {
    pub(super) padlock: PadlockVisual,
    pub(super) curtain_opacity: f32,
    pub(super) curtain_offset_y: f32,
}

fn segment_progress(t: f32, start: f32, end: f32) -> f32 {
    if end <= start {
        return if t >= end { 1.0 } else { 0.0 };
    }
    ((t - start) / (end - start)).clamp(0.0, 1.0)
}

fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

/// Computes the four-beat lock ritual with a single-padlock seamless handoff:
/// 1) Padlock pops in open at stage center with key seated
/// 2) Key rotates and pulls smoothly out to the right
/// 3) Shackle snaps shut with recoil and dual confirmation rings
/// 4) The same padlock glides smoothly into the console's lock chamber as the card reveals
pub(super) fn app_lock_enter_frame(t: f32, target_scale: f32) -> AppLockEnterFrame {
    let t = t.clamp(0.0, 1.0);

    let pop_t = segment_progress(t, 0.0, 0.17);
    let turn_t = segment_progress(t, 0.15, 0.29);
    let pull_t = segment_progress(t, 0.26, 0.49);
    let snap_t = segment_progress(t, 0.49, 0.66);
    let ring_t = segment_progress(t, 0.58, 0.86);
    let handoff_t = segment_progress(t, 0.68, 1.0);

    let pop_scale = lerp(0.76, 1.0, ease_out_back(pop_t));
    let handoff = ease_in_out_cubic(handoff_t);
    let glide_scale = lerp(1.0, target_scale, handoff);

    // Brief downward recoil as the shackle seats into the lock housing.
    let recoil_t = segment_progress(t, 0.58, 0.72);
    let body_dy = if recoil_t > 0.0 && recoil_t < 1.0 {
        (recoil_t * PI).sin() * 2.8
    } else {
        0.0
    };

    let key_turn = -0.46 * ease_out_cubic(turn_t);
    let key_pull = ease_in_out_cubic(pull_t);
    let key_opacity = ease_out_cubic(pop_t) * (1.0 - segment_progress(t, 0.37, 0.49));
    let shackle_lift = 1.0 - ease_in_out_cubic(snap_t);

    let ritual_opacity = ease_out_cubic(pop_t);
    let screen_opacity = ease_out_cubic(segment_progress(t, 0.70, 0.96));
    let screen_offset_y = lerp(10.0, 0.0, ease_out_cubic(handoff_t));

    let tone = if snap_t >= 0.72 {
        PadlockTone::Locked
    } else {
        PadlockTone::Open
    };

    AppLockEnterFrame {
        ritual_visible: t < 1.0 && ritual_opacity > 0.002,
        ritual_opacity,
        ritual_visual: PadlockVisual {
            shackle_lift,
            body_dy,
            key_pull,
            key_turn,
            key_opacity,
            ring_progress: ring_t,
            scale: pop_scale * glide_scale,
            tone,
        },
        handoff_progress: handoff,
        screen_opacity,
        screen_offset_y,
    }
}

/// Computes the two-beat unlock sequence:
/// 1) Padlock shackle springs open and turns success-green with a confirmation ring
/// 2) Lock screen dissolves upward to reveal the live workspace
pub(super) fn app_lock_exit_frame(t: f32, spatial: bool) -> AppLockExitFrame {
    let t = t.clamp(0.0, 1.0);
    if !spatial {
        return AppLockExitFrame {
            padlock: PadlockVisual {
                shackle_lift: 1.0,
                tone: PadlockTone::Unlocked,
                ..PadlockVisual::settled_locked()
            },
            curtain_opacity: 1.0 - ease_out_cubic(t),
            curtain_offset_y: 0.0,
        };
    }

    let open_t = segment_progress(t, 0.0, 0.44);
    let ring_t = segment_progress(t, 0.05, 0.56);
    let dissolve_t = segment_progress(t, 0.42, 1.0);

    let shackle_lift = ease_out_back(open_t).clamp(0.0, 1.10);
    let body_dy = if open_t > 0.0 && open_t < 0.7 {
        -((open_t / 0.7) * PI).sin() * 1.8
    } else {
        0.0
    };

    AppLockExitFrame {
        padlock: PadlockVisual {
            shackle_lift,
            body_dy,
            key_pull: 1.0,
            key_turn: 0.0,
            key_opacity: 0.0,
            ring_progress: ring_t,
            scale: lerp(1.0, 1.035, ease_out_cubic(open_t)),
            tone: PadlockTone::Unlocked,
        },
        curtain_opacity: 1.0 - ease_in_out_cubic(dissolve_t),
        curtain_offset_y: lerp(0.0, -14.0, ease_in_out_cubic(dissolve_t)),
    }
}

fn hsla_with_alpha(hex: u32, alpha: f32) -> Hsla {
    let mut color: Hsla = gpui::rgb(hex).into();
    color.a = alpha.clamp(0.0, 1.0);
    color
}

fn circle_path(center: Point<Pixels>, radius: f32, stroke_width: f32) -> Option<gpui::Path<Pixels>> {
    if radius <= 0.5 {
        return None;
    }
    let r = point(px(radius), px(radius));
    let top = point(center.x, center.y - px(radius));
    let bottom = point(center.x, center.y + px(radius));
    let mut builder = PathBuilder::stroke(px(stroke_width));
    builder.move_to(top);
    builder.arc_to(r, px(0.0), false, true, bottom);
    builder.arc_to(r, px(0.0), false, true, top);
    builder.build().ok()
}

fn filled_circle_path(center: Point<Pixels>, radius: f32) -> Option<gpui::Path<Pixels>> {
    if radius <= 0.5 {
        return None;
    }
    let r = point(px(radius), px(radius));
    let top = point(center.x, center.y - px(radius));
    let bottom = point(center.x, center.y + px(radius));
    let mut builder = PathBuilder::fill();
    builder.move_to(top);
    builder.arc_to(r, px(0.0), false, true, bottom);
    builder.arc_to(r, px(0.0), false, true, top);
    builder.close();
    builder.build().ok()
}

fn rotate_offset(dx: f32, dy: f32, angle: f32) -> (f32, f32) {
    let (sin_a, cos_a) = angle.sin_cos();
    (dx * cos_a - dy * sin_a, dx * sin_a + dy * cos_a)
}

/// Renders the precision hardware padlock, vault dial, and animated key inside a GPUI canvas.
pub(super) fn render_padlock_mechanism(
    theme: AppUiColors,
    size_px: f32,
    visual: PadlockVisual,
) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let side = f32::from(bounds.size.width).min(f32::from(bounds.size.height));
            if side <= 4.0 {
                return;
            }
            let cx = f32::from(bounds.origin.x) + f32::from(bounds.size.width) * 0.5;
            let cy = f32::from(bounds.origin.y) + f32::from(bounds.size.height) * 0.5;
            let unit = (side / 124.0) * visual.scale.max(0.1);
            let light = theme.is_light();

            let tone_hex = match visual.tone {
                PadlockTone::Open => theme.text_secondary,
                PadlockTone::Locked => theme.accent,
                PadlockTone::Unlocked => theme.success,
                PadlockTone::Error => theme.error,
            };

            // Soft multi-step radial glow inside the chamber (avoids hard disc edges).
            let chamber_center = point(px(cx), px(cy));
            let base_glow = if light { 0.022 } else { 0.026 };
            for i in 0..6 {
                let frac = i as f32 / 6.0;
                let r = lerp(52.0, 24.0, frac) * unit;
                let a = base_glow * (0.45 + 0.55 * frac);
                if let Some(glow) = filled_circle_path(chamber_center, r) {
                    window.paint_path(glow, hsla_with_alpha(tone_hex, a));
                }
            }

            // Precision 24-tick vault dial ring around the padlock.
            let dial_r_outer = 54.0 * unit;
            let mut minor_ticks = PathBuilder::stroke(px(1.0));
            let mut major_ticks = PathBuilder::stroke(px(1.25));
            for i in 0..24 {
                let angle = (i as f32 / 24.0) * TAU - PI * 0.5;
                let (sin_a, cos_a) = angle.sin_cos();
                let is_cardinal = i % 6 == 0;
                let inner_r = if is_cardinal {
                    dial_r_outer - 5.2 * unit
                } else {
                    dial_r_outer - 2.6 * unit
                };
                let p0 = point(px(cx + cos_a * inner_r), px(cy + sin_a * inner_r));
                let p1 = point(
                    px(cx + cos_a * dial_r_outer),
                    px(cy + sin_a * dial_r_outer),
                );
                if is_cardinal {
                    major_ticks.move_to(p0);
                    major_ticks.line_to(p1);
                } else {
                    minor_ticks.move_to(p0);
                    minor_ticks.line_to(p1);
                }
            }
            if let Ok(path) = minor_ticks.build() {
                window.paint_path(
                    path,
                    hsla_with_alpha(theme.border, if light { 0.52 } else { 0.36 }),
                );
            }
            if let Ok(path) = major_ticks.build() {
                window.paint_path(
                    path,
                    hsla_with_alpha(tone_hex, if light { 0.48 } else { 0.42 }),
                );
            }

            // Crisp inner bezel ring framing the mechanism.
            if let Some(inner_ring) = circle_path(chamber_center, 44.0 * unit, 1.0) {
                window.paint_path(
                    inner_ring,
                    hsla_with_alpha(tone_hex, if light { 0.24 } else { 0.26 }),
                );
            }

            // Dual expanding confirmation rings on lock latch or unlock release.
            if visual.ring_progress > 0.001 && visual.ring_progress < 0.999 {
                let p = ease_out_cubic(visual.ring_progress);
                let pulse_r = lerp(34.0 * unit, 60.0 * unit, p);
                let pulse_alpha = (1.0 - p) * 0.58;
                if let Some(pulse_ring) = circle_path(chamber_center, pulse_r, 1.75) {
                    window.paint_path(pulse_ring, hsla_with_alpha(tone_hex, pulse_alpha));
                }
                let p2 = ease_out_cubic(segment_progress(visual.ring_progress, 0.14, 1.0));
                if p2 > 0.001 && p2 < 0.999 {
                    let echo_r = lerp(38.0 * unit, 57.0 * unit, p2);
                    let echo_alpha = (1.0 - p2) * 0.28;
                    if let Some(echo_ring) = circle_path(chamber_center, echo_r, 1.0) {
                        window.paint_path(echo_ring, hsla_with_alpha(tone_hex, echo_alpha));
                    }
                }
            }

            // Lock body geometry.
            let body_w = 48.0 * unit;
            let body_h = 37.5 * unit;
            let body_x = cx - body_w * 0.5;
            let body_y = cy - 3.5 * unit + visual.body_dy * unit;
            let body_radius = 9.5 * unit;

            // Shackle geometry: left leg stays anchored in its housing socket while the right leg lifts clear.
            let shackle_r = 13.0 * unit;
            let shackle_stroke = (4.8 * unit).max(2.2);
            let lift_px = visual.shackle_lift * 10.5 * unit;
            let arch_center_y = body_y - 9.5 * unit - lift_px;
            let left_x = cx - shackle_r;
            let right_x = cx + shackle_r;
            let left_bottom_y = body_y + 4.5 * unit;
            let right_bottom_y = body_y + 4.5 * unit - lift_px * 1.18;

            let build_shackle = |stroke_w: f32, dx: f32, dy: f32| {
                let mut builder = PathBuilder::stroke(px(stroke_w));
                builder.move_to(point(px(left_x + dx), px(left_bottom_y + dy)));
                builder.line_to(point(px(left_x + dx), px(arch_center_y + dy)));
                builder.arc_to(
                    point(px(shackle_r), px(shackle_r)),
                    px(0.0),
                    false,
                    true,
                    point(px(right_x + dx), px(arch_center_y + dy)),
                );
                builder.line_to(point(px(right_x + dx), px(right_bottom_y + dy)));
                builder.build().ok()
            };

            // Layered metallic shackle: soft shadow -> main brushed bar -> subtle top-left specular sheen.
            if let Some(shadow_path) = build_shackle(shackle_stroke + 1.6, 0.0, 1.2 * unit) {
                window.paint_path(
                    shadow_path,
                    hsla_with_alpha(0x000000, if light { 0.14 } else { 0.36 }),
                );
            }
            if let Some(main_path) = build_shackle(shackle_stroke, 0.0, 0.0) {
                let shackle_hex = match visual.tone {
                    PadlockTone::Unlocked => theme.success,
                    PadlockTone::Error => theme.error,
                    _ => theme.text_heading,
                };
                window.paint_path(main_path, hsla_with_alpha(shackle_hex, 0.94));
            }
            if let Some(specular_path) =
                build_shackle((1.35 * unit).max(1.0), -0.6 * unit, -0.7 * unit)
            {
                window.paint_path(
                    specular_path,
                    hsla_with_alpha(0xffffff, if light { 0.45 } else { 0.30 }),
                );
            }

            // Soft drop shadow beneath the lock body.
            let shadow_bounds = Bounds::new(
                point(px(body_x - 1.0 * unit), px(body_y + 3.5 * unit)),
                size(px(body_w + 2.0 * unit), px(body_h)),
            );
            window.paint_quad(
                fill(
                    shadow_bounds,
                    hsla_with_alpha(0x000000, if light { 0.12 } else { 0.34 }),
                )
                .corner_radii(Corners::all(px(body_radius + 1.0 * unit))),
            );

            // Machined outer chamfer border.
            let border_pad = 1.3 * unit;
            let outer_body_bounds = Bounds::new(
                point(px(body_x - border_pad), px(body_y - border_pad)),
                size(px(body_w + border_pad * 2.0), px(body_h + border_pad * 2.0)),
            );
            window.paint_quad(
                fill(
                    outer_body_bounds,
                    hsla_with_alpha(
                        match visual.tone {
                            PadlockTone::Locked | PadlockTone::Unlocked | PadlockTone::Error => {
                                tone_hex
                            }
                            PadlockTone::Open => theme.border,
                        },
                        if visual.tone == PadlockTone::Open {
                            0.68
                        } else {
                            0.58
                        },
                    ),
                )
                .corner_radii(Corners::all(px(body_radius + border_pad * 0.5))),
            );

            // Main lock body plate.
            let body_bounds = Bounds::new(
                point(px(body_x), px(body_y)),
                size(px(body_w), px(body_h)),
            );
            let body_fill = if light {
                theme.bg_panel
            } else {
                theme.bg_elevated
            };
            window.paint_quad(
                fill(body_bounds, gpui::rgb(body_fill)).corner_radii(Corners::all(px(body_radius))),
            );

            // Subtle upper-half brushed metal sheen + crisp top specular hairline.
            let sheen_bounds = Bounds::new(
                point(px(body_x + 1.0 * unit), px(body_y + 1.0 * unit)),
                size(px(body_w - 2.0 * unit), px(body_h * 0.44)),
            );
            window.paint_quad(
                fill(
                    sheen_bounds,
                    hsla_with_alpha(0xffffff, if light { 0.28 } else { 0.045 }),
                )
                .corner_radii(Corners {
                    top_left: px(body_radius - 1.0 * unit),
                    top_right: px(body_radius - 1.0 * unit),
                    bottom_right: px(2.0 * unit),
                    bottom_left: px(2.0 * unit),
                }),
            );
            let highlight_bounds = Bounds::new(
                point(px(body_x + 8.0 * unit), px(body_y + 1.0 * unit)),
                size(px(body_w - 16.0 * unit), px(1.0)),
            );
            window.paint_quad(fill(
                highlight_bounds,
                hsla_with_alpha(0xffffff, if light { 0.55 } else { 0.16 }),
            ));

            // Machined shackle collar recesses on the top edge of the body.
            let collar_w = 7.2 * unit;
            let collar_h = 2.0 * unit;
            for sx in [left_x, right_x] {
                let collar = Bounds::new(
                    point(px(sx - collar_w * 0.5), px(body_y - 0.5 * unit)),
                    size(px(collar_w), px(collar_h)),
                );
                window.paint_quad(
                    fill(
                        collar,
                        hsla_with_alpha(theme.bg, if light { 0.35 } else { 0.55 }),
                    )
                    .corner_radii(Corners::all(px(1.0 * unit))),
                );
            }

            // Recessed circular escutcheon bezel around the keyhole.
            let keyhole_cx = cx;
            let keyhole_cy = body_y + body_h * 0.42;
            let escutcheon_center = point(px(keyhole_cx), px(keyhole_cy + 1.5 * unit));
            if let Some(recess) = filled_circle_path(escutcheon_center, 10.5 * unit) {
                window.paint_path(
                    recess,
                    hsla_with_alpha(theme.bg, if light { 0.45 } else { 0.48 }),
                );
            }
            if let Some(bezel) = circle_path(escutcheon_center, 10.5 * unit, 1.0) {
                window.paint_path(
                    bezel,
                    hsla_with_alpha(tone_hex, if light { 0.32 } else { 0.34 }),
                );
            }

            // Keyhole cylinder (circle + tapered vertical ward slot).
            if let Some(cylinder) =
                filled_circle_path(point(px(keyhole_cx), px(keyhole_cy - 0.5 * unit)), 4.0 * unit)
            {
                window.paint_path(cylinder, hsla_with_alpha(tone_hex, 0.96));
            }
            let slot_w = 2.9 * unit;
            let slot_h = 7.4 * unit;
            let slot_bounds = Bounds::new(
                point(px(keyhole_cx - slot_w * 0.5), px(keyhole_cy + 1.2 * unit)),
                size(px(slot_w), px(slot_h)),
            );
            window.paint_quad(
                fill(slot_bounds, hsla_with_alpha(tone_hex, 0.96))
                    .corner_radii(Corners::all(px(slot_w * 0.45))),
            );

            // Subtle status indicator pill along the lower housing rim.
            if visual.tone != PadlockTone::Open {
                let led_w = 14.0 * unit;
                let led_h = 2.0 * unit;
                let led_bounds = Bounds::new(
                    point(px(cx - led_w * 0.5), px(body_y + body_h - 4.8 * unit)),
                    size(px(led_w), px(led_h)),
                );
                window.paint_quad(
                    fill(led_bounds, hsla_with_alpha(tone_hex, 0.72))
                        .corner_radii(Corners::all(px(led_h * 0.5))),
                );
            }

            // Sculpted animated key: seated in the cylinder, turns and slides smoothly right.
            if visual.key_opacity > 0.005 {
                let pull_dx = visual.key_pull * 48.0 * unit;
                let pivot_x = keyhole_cx + pull_dx;
                let pivot_y = keyhole_cy - 0.5 * unit;
                let key_color = hsla_with_alpha(theme.accent, visual.key_opacity);
                let key_highlight = hsla_with_alpha(0xffffff, visual.key_opacity * 0.35);
                let key_stroke = (3.0 * unit).max(1.6);

                let to_pt = |dx: f32, dy: f32| {
                    let (rx, ry) = rotate_offset(dx * unit, dy * unit, visual.key_turn);
                    point(px(pivot_x + rx), px(pivot_y + ry))
                };

                // Shaft + collar shoulder + dual bitting teeth.
                let mut shaft = PathBuilder::stroke(px(key_stroke));
                shaft.move_to(to_pt(2.5, 0.0));
                shaft.line_to(to_pt(26.5, 0.0));
                // Collar shoulder before the bow.
                shaft.move_to(to_pt(23.0, -3.2));
                shaft.line_to(to_pt(23.0, 3.2));
                // Dual bitting teeth.
                shaft.move_to(to_pt(8.5, 0.0));
                shaft.line_to(to_pt(8.5, 5.4));
                shaft.move_to(to_pt(13.8, 0.0));
                shaft.line_to(to_pt(13.8, 3.8));
                if let Ok(path) = shaft.build() {
                    window.paint_path(path, key_color);
                }

                // Metallic specular line along the upper edge of the key shaft.
                let mut shaft_glint = PathBuilder::stroke(px((1.0 * unit).max(1.0)));
                shaft_glint.move_to(to_pt(3.5, -0.8));
                shaft_glint.line_to(to_pt(22.0, -0.8));
                if let Ok(path) = shaft_glint.build() {
                    window.paint_path(path, key_highlight);
                }

                // Beveled circular bow handle on the right end of the key.
                let (bow_dx, bow_dy) = rotate_offset(33.5 * unit, 0.0, visual.key_turn);
                let bow_center = point(px(pivot_x + bow_dx), px(pivot_y + bow_dy));
                if let Some(bow_fill) = filled_circle_path(bow_center, 7.2 * unit) {
                    window.paint_path(
                        bow_fill,
                        hsla_with_alpha(theme.accent, visual.key_opacity * 0.16),
                    );
                }
                if let Some(bow_ring) = circle_path(bow_center, 7.2 * unit, key_stroke) {
                    window.paint_path(bow_ring, key_color);
                }
                if let Some(bow_inner) = circle_path(bow_center, 4.2 * unit, 1.0) {
                    window.paint_path(bow_inner, key_highlight);
                }
            }
        },
    )
    .size(px(size_px))
    .flex_none()
    .into_any_element()
}

/// Paints a smooth radial ambient glow and subtle precision architectural frame behind the console.
pub(super) fn render_lock_screen_backdrop(theme: AppUiColors) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let width = f32::from(bounds.size.width);
            let height = f32::from(bounds.size.height);
            if width < 64.0 || height < 64.0 {
                return;
            }
            let ox = f32::from(bounds.origin.x);
            let oy = f32::from(bounds.origin.y);
            let center = point(px(ox + width * 0.5), px(oy + height * 0.49));
            let min_dim = width.min(height);
            let light = theme.is_light();

            // 14 ultra-low-alpha radial steps create a smooth ambient falloff with zero disc banding.
            let max_r = min_dim * 0.52;
            let min_r = min_dim * 0.10;
            let step_alpha = if light { 0.0085 } else { 0.0095 };
            for i in 0..14 {
                let frac = i as f32 / 13.0;
                let r = lerp(max_r, min_r, frac);
                let falloff = 0.35 + 0.65 * frac * frac;
                if let Some(halo) = filled_circle_path(center, r) {
                    window.paint_path(halo, hsla_with_alpha(theme.accent, step_alpha * falloff));
                }
            }

            // Delicate compass orbital ring with four cardinal registration ticks.
            let orbit_r = min_dim * 0.41;
            if let Some(orbit) = circle_path(center, orbit_r, 1.0) {
                window.paint_path(
                    orbit,
                    hsla_with_alpha(theme.border, if light { 0.20 } else { 0.11 }),
                );
            }
            let mut cardinal = PathBuilder::stroke(px(1.0));
            let tick_len = 8.0;
            for (dx, dy) in [(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)] {
                let p0 = point(
                    center.x + px(dx * (orbit_r - tick_len * 0.5)),
                    center.y + px(dy * (orbit_r - tick_len * 0.5)),
                );
                let p1 = point(
                    center.x + px(dx * (orbit_r + tick_len * 0.5)),
                    center.y + px(dy * (orbit_r + tick_len * 0.5)),
                );
                cardinal.move_to(p0);
                cardinal.line_to(p1);
            }
            if let Ok(path) = cardinal.build() {
                window.paint_path(
                    path,
                    hsla_with_alpha(theme.border, if light { 0.34 } else { 0.22 }),
                );
            }

            // Crisp corner registration marks grounding the full-window canvas.
            let inset = 28.0_f32.min(min_dim * 0.08);
            let tick = 14.0;
            let frame_color = hsla_with_alpha(theme.border, if light { 0.38 } else { 0.24 });
            let left = ox + inset;
            let right = ox + width - inset;
            let top = oy + inset;
            let bottom = oy + height - inset;

            let mut frame = PathBuilder::stroke(px(1.0));
            frame.move_to(point(px(left), px(top + tick)));
            frame.line_to(point(px(left), px(top)));
            frame.line_to(point(px(left + tick), px(top)));

            frame.move_to(point(px(right - tick), px(top)));
            frame.line_to(point(px(right), px(top)));
            frame.line_to(point(px(right), px(top + tick)));

            frame.move_to(point(px(left), px(bottom - tick)));
            frame.line_to(point(px(left), px(bottom)));
            frame.line_to(point(px(left + tick), px(bottom)));

            frame.move_to(point(px(right - tick), px(bottom)));
            frame.line_to(point(px(right), px(bottom)));
            frame.line_to(point(px(right), px(bottom - tick)));
            if let Ok(path) = frame.build() {
                window.paint_path(path, frame_color);
            }
        },
    )
    .absolute()
    .inset_0()
    .size_full()
    .into_any_element()
}
