use crate::clock::PlaybackClock;
use gpui::{
    AnyElement, App, Bounds, BoxShadow, Context, Corners, Hsla, PathBuilder, Pixels, Render,
    Subscription, Task, Window, canvas, div, linear_color_stop, linear_gradient, point, prelude::*,
    px, rgba, size,
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratedEffectKind {
    Mineral,
    Fog,
    Tide,
    Meteor,
    Particles,
    Caustics,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeneratedEffectPreferences {
    pub kind: GeneratedEffectKind,
    pub strength: f32,
    pub sheen: f32,
    pub max_fps: Option<u32>,
    pub colors: [u32; 2],
    pub speed: f32,
    pub size: f32,
    pub brightness: f32,
    pub roughness: f32,
    pub direction: f32,
    pub particle_count: u32,
}

struct EffectSurface {
    preferences: GeneratedEffectPreferences,
    opacity: f32,
    max_fps: u32,
    clock: PlaybackClock,
    visible: bool,
    paused: bool,
    timer: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

pub(crate) fn effect_layer(
    preferences: GeneratedEffectPreferences,
    paused: bool,
    over_media: bool,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let surface = window
        .use_keyed_state("generated-background", cx, |window, cx| {
            cx.new(|cx| EffectSurface::new(preferences, window, cx))
        })
        .read(cx)
        .clone();
    surface.update(cx, |surface, _| {
        surface.paused = paused;
        surface.preferences = preferences;
        let strength = preferences.strength.clamp(0.0, 1.0);
        // Detailed media needs a steeper response while preserving zero and full strength.
        surface.opacity = if over_media {
            strength * (2.0 - strength)
        } else {
            strength
        };
        surface.max_fps = preferences.max_fps.unwrap_or(30).clamp(1, 30);
    });
    surface.into_any_element()
}

impl EffectSurface {
    fn new(
        preferences: GeneratedEffectPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![
            cx.observe_window_activation(window, |this: &mut EffectSurface, window, cx| {
                this.sync(window, cx);
                cx.notify();
            }),
            cx.observe_window_visibility(window, |this: &mut EffectSurface, _, window, cx| {
                this.sync(window, cx);
                cx.notify();
            }),
        ];
        Self {
            preferences,
            opacity: preferences.strength.clamp(0.0, 1.0),
            max_fps: preferences.max_fps.unwrap_or(30).clamp(1, 30),
            clock: PlaybackClock::default(),
            visible: false,
            paused: false,
            timer: None,
            _subscriptions: subscriptions,
        }
    }
    fn sync(&mut self, window: &Window, cx: &mut Context<Self>) {
        let active = self.visible
            && !self.paused
            && self.opacity > 0.0
            && (self.preferences.kind == GeneratedEffectKind::Mineral
                || self.preferences.brightness > 0.0)
            && self.preferences.speed > 0.0
            && (self.preferences.kind != GeneratedEffectKind::Mineral
                || self.preferences.sheen > 0.0)
            && window.is_window_active()
            && window.is_visible()
            && !window.is_minimized()
            && !cx.reduce_motion();
        self.clock.set_running(active, Instant::now());
        if !active {
            self.timer = None;
            return;
        }
        if self.timer.is_none() {
            let interval = Duration::from_secs_f64(1.0 / self.max_fps as f64);
            // The mounted surface owns this one-shot repaint; unmounting cancels it.
            self.timer = Some(cx.spawn(async move |surface, cx| {
                cx.background_executor().timer(interval).await;
                let _ = surface.update(cx, |surface, cx| {
                    surface.timer = None;
                    cx.notify();
                });
            }));
        }
    }

    fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window) {
        if bounds.is_empty()
            || self.opacity <= 0.0
            || (self.preferences.kind != GeneratedEffectKind::Mineral
                && self.preferences.brightness <= 0.0)
        {
            return;
        }
        let phase = self.clock.position(Instant::now()).as_secs_f32()
            * self.preferences.speed.clamp(0.0, 3.0)
            * 0.07;
        match self.preferences.kind {
            GeneratedEffectKind::Fog => paint_fog(bounds, self.preferences, phase, window),
            GeneratedEffectKind::Tide => paint_tide(bounds, self.preferences, phase, window),
            GeneratedEffectKind::Meteor => paint_meteors(bounds, self.preferences, phase, window),
            GeneratedEffectKind::Particles => {
                paint_particles(bounds, self.preferences, phase, window)
            }
            GeneratedEffectKind::Caustics => {
                paint_caustics(bounds, self.preferences, phase, window)
            }
            GeneratedEffectKind::Mineral => {
                let colors: [Hsla; 2] = self
                    .preferences
                    .colors
                    .map(|color| rgba(((color & 0xffffff) << 8) | 0xff).into());
                let background = gpui::procedural_noise(
                    colors,
                    self.preferences.size.clamp(0.3, 2.0) * 3.0 * window.scale_factor(),
                    self.preferences.roughness,
                    phase,
                    self.preferences.sheen,
                );
                window.paint_quad(gpui::fill(bounds, background));
            }
        }
    }
}

impl Render for EffectSurface {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let surface = cx.entity();
        let paint_surface = surface.clone();
        div()
            .absolute()
            .inset_0()
            .overflow_hidden()
            .opacity(self.opacity)
            .child(
                canvas(
                    move |bounds, window, cx| {
                        surface.update(cx, |surface, cx| {
                            surface.visible = bounds.intersects(&window.content_mask().bounds);
                            surface.sync(window, cx);
                        });
                    },
                    move |bounds, _, window, cx| {
                        paint_surface.update(cx, |surface, _| {
                            if surface.visible {
                                surface.paint(bounds, window);
                            }
                        });
                    },
                )
                .size_full(),
            )
    }
}

fn paint_glow(
    bounds: Bounds<Pixels>,
    color: u32,
    alpha: f32,
    blur: f32,
    radius: f32,
    window: &mut Window,
) {
    window.paint_drop_shadows(
        bounds,
        Corners::all(px(radius)),
        &[BoxShadow {
            // A light core keeps the theme hue visible against detailed media.
            color: rgba((color & 0xffffff) << 8)
                .blend(rgba(0xffffff40))
                .alpha(alpha)
                .into(),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(blur),
            spread_radius: px(0.0),
            inset: false,
        }],
    );
}

fn paint_fog(
    bounds: Bounds<Pixels>,
    preferences: GeneratedEffectPreferences,
    phase: f32,
    window: &mut Window,
) {
    let width = bounds.size.width.as_f32();
    let height = bounds.size.height.as_f32();
    let diameter = width.min(height) * 0.65 * preferences.size.clamp(0.3, 2.0);
    // Offset orbits leave the center quiet. GPUI's native shadow shaders produce the soft fields.
    for (index, (x, y)) in [
        (
            0.12 + 0.16 * phase.sin(),
            0.18 + 0.13 * (phase * 0.73).cos(),
        ),
        (
            0.85 + 0.14 * (phase * 0.81 + 1.4).sin(),
            0.78 + 0.17 * (phase * 0.61).cos(),
        ),
        (
            0.62 + 0.24 * (phase * 0.53 + 2.8).sin(),
            0.08 + 0.14 * (phase * 0.89 + 0.7).cos(),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let glow = Bounds::new(
            bounds.origin
                + point(
                    px(x * width - diameter * 0.5),
                    px(y * height - diameter * 0.5),
                ),
            size(px(diameter), px(diameter)),
        );
        paint_glow(
            glow,
            preferences.colors[index % 2],
            preferences.brightness,
            diameter * 0.18,
            diameter * 0.5,
            window,
        );
    }
}

fn paint_tide(
    bounds: Bounds<Pixels>,
    preferences: GeneratedEffectPreferences,
    phase: f32,
    window: &mut Window,
) {
    let width = bounds.size.width.as_f32();
    let height = bounds.size.height.as_f32();
    let angle = preferences.direction.clamp(0.0, 360.0).to_radians();
    let (dy, dx) = angle.sin_cos();
    let span = width.hypot(height);
    let band = width.min(height) * 0.18 * preferences.size.clamp(0.3, 2.0);
    // Overlapping soft fields form two travelling bands without an offscreen texture.
    for layer in 0..2 {
        let travel = (phase * 0.42 + layer as f32 * std::f32::consts::PI).sin() * span * 0.55;
        for segment in -3..=3 {
            let along = segment as f32 * span / 6.0;
            let bend = (along / span * 4.0 + phase + layer as f32).sin() * band * 0.65;
            let center = bounds.center()
                + point(
                    px(dx * travel - dy * along + dx * bend),
                    px(dy * travel + dx * along + dy * bend),
                );
            let glow = Bounds::new(
                center - point(px(band), px(band)),
                size(px(band * 2.0), px(band * 2.0)),
            );
            paint_glow(
                glow,
                preferences.colors[layer],
                preferences.brightness,
                band * 0.26,
                band,
                window,
            );
        }
    }
}

fn paint_particles(
    bounds: Bounds<Pixels>,
    preferences: GeneratedEffectPreferences,
    phase: f32,
    window: &mut Window,
) {
    let width = bounds.size.width.as_f32();
    let height = bounds.size.height.as_f32();
    for index in 0..preferences.particle_count.clamp(4, 24) {
        let seed = index as f32;
        let depth = (index % 3) as f32 / 2.0;
        let progress = (seed * 0.381966 + phase * (0.07 + depth * 0.08)).fract();
        let x = (seed * 0.618034).fract() + (phase * 0.4 + seed * 1.7).sin() * 0.045;
        let y = 1.05 - progress * 1.1;
        let radius = (width.min(height) * 0.0035).clamp(0.8, 3.5)
            * preferences.size.clamp(0.3, 2.0)
            * (0.6 + depth * 2.0);
        let alpha = preferences.brightness.clamp(0.0, 1.0)
            * (progress.min(1.0 - progress) * 10.0).min(1.0)
            * (0.55 + 0.15 * (phase + seed).sin());
        let dot = Bounds::new(
            bounds.origin + point(px(x * width - radius), px(y * height - radius)),
            size(px(radius * 2.0), px(radius * 2.0)),
        );
        let color = preferences.colors[index as usize % 2];
        paint_glow(
            dot,
            color,
            alpha,
            radius * (0.3 + depth * 1.4),
            radius,
            window,
        );
        // Distant dust has a defined core; the foreground remains softly out of focus.
        if depth < 1.0 {
            window.paint_quad(
                gpui::fill(
                    dot,
                    rgba((color << 8) | 255)
                        .blend(rgba(0xffffff99))
                        .alpha(alpha * (1.0 - depth)),
                )
                .corner_radii(px(radius)),
            );
        }
    }
}

fn paint_caustics(
    bounds: Bounds<Pixels>,
    preferences: GeneratedEffectPreferences,
    phase: f32,
    window: &mut Window,
) {
    let width = bounds.size.width.as_f32();
    let height = bounds.size.height.as_f32();
    let scale = preferences.size.clamp(0.3, 2.0);
    let lines = (5.0 / scale).round().clamp(3.0, 12.0) as usize;
    let stroke = (width.min(height) * 0.003).clamp(0.7, 2.5);
    let span = width.hypot(height) * 1.2;
    // Two gently warped families form an open light mesh; no background readback is needed.
    for family in 0..2 {
        let angle: f32 = if family == 0 { 0.45 } else { 1.85 };
        let (sin, cos) = angle.sin_cos();
        for line in 0..=lines {
            let seed = line as f32 * 1.73 + family as f32 * 2.41;
            let base = line as f32 / lines as f32;
            for (spread, opacity) in [(3.5, 0.10), (1.0, 0.36)] {
                let mut path = PathBuilder::stroke(px(stroke * spread));
                for segment in 0..=32 {
                    let along = segment as f32 / 32.0;
                    let wave = (along * 9.0 / scale + phase * 0.8 + seed).sin() * 0.075
                        + (along * 17.0 / scale - phase * 0.51 + seed * 2.0).sin() * 0.035;
                    let cross = base + wave + (phase * 0.23 + seed).sin() * 0.035;
                    let x = (along - 0.5) * span;
                    let y = (cross - 0.5) * span;
                    let point =
                        bounds.center() + point(px(x * cos - y * sin), px(x * sin + y * cos));
                    if segment == 0 {
                        path.move_to(point);
                    } else {
                        path.line_to(point);
                    }
                }
                let color = rgba((preferences.colors[family] << 8) | 255).blend(rgba(0xffffff88));
                window.paint_path(
                    path.build().expect("caustic curve has finite coordinates"),
                    color.alpha(opacity * preferences.brightness.clamp(0.0, 1.0)),
                );
            }
        }
    }
}

fn paint_meteors(
    bounds: Bounds<Pixels>,
    preferences: GeneratedEffectPreferences,
    phase: f32,
    window: &mut Window,
) {
    let width = bounds.size.width.as_f32();
    let height = bounds.size.height.as_f32();
    let angle = preferences.direction.clamp(0.0, 360.0);
    let (dy, dx) = angle.to_radians().sin_cos();
    let span = width.hypot(height);
    let length = width.min(height) * 0.18 * preferences.size.clamp(0.3, 2.0);
    let radius = (width.min(height) * 0.0022).clamp(1.25, 3.0);
    // Staggered lanes keep the field sparse. The extended travel hides cycle resets offscreen.
    for (index, (offset, lane)) in [(0.43, -0.28), (0.12, 0.29), (0.76, -0.12)]
        .into_iter()
        .enumerate()
    {
        let progress = (phase * 2.0 + offset).fract();
        let travel = (progress - 0.5) * span * 3.5;
        let across = lane * width.min(height);
        let head =
            bounds.center() + point(px(dx * travel - dy * across), px(dy * travel + dx * across));
        let tail = head - point(px(dx * length), px(dy * length));
        let normal = point(px(-dy * radius), px(dx * radius));
        let alpha =
            preferences.brightness.clamp(0.0, 1.0) * (progress.min(1.0 - progress) * 6.0).min(1.0);
        let color = (preferences.colors[index % 2] & 0xffffff) << 8;
        let highlight = rgba(color).blend(rgba(0xffffff66));
        let mut path = PathBuilder::fill();
        path.move_to(tail);
        path.line_to(head + normal);
        path.line_to(head - normal);
        path.close();
        window.paint_path(
            path.build().expect("meteor tail is a finite triangle"),
            linear_gradient(
                (angle + 90.0) % 360.0,
                linear_color_stop(highlight.alpha(0.0), 0.0),
                linear_color_stop(highlight.alpha(alpha), 1.0),
            ),
        );
        let glow = Bounds::new(
            head - point(px(radius), px(radius)),
            size(px(radius * 2.0), px(radius * 2.0)),
        );
        paint_glow(
            glow,
            preferences.colors[index % 2],
            alpha * 0.6,
            radius * 3.0,
            radius,
            window,
        );
        window.paint_quad(gpui::fill(glow, rgba(0xffffffff).alpha(alpha)).corner_radii(px(radius)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn generated_effects_pause_on_deactivation_and_reduced_motion(cx: &mut gpui::TestAppContext) {
        for kind in [
            GeneratedEffectKind::Mineral,
            GeneratedEffectKind::Fog,
            GeneratedEffectKind::Tide,
            GeneratedEffectKind::Meteor,
            GeneratedEffectKind::Particles,
            GeneratedEffectKind::Caustics,
        ] {
            let (surface, cx) = cx.add_window_view(move |window, cx| {
                EffectSurface::new(
                    GeneratedEffectPreferences {
                        kind,
                        strength: 0.4,
                        sheen: 0.5,
                        max_fps: None,
                        colors: [0x8090aa, 0x90aa80],
                        speed: 1.0,
                        size: 1.0,
                        brightness: 0.6,
                        roughness: 0.5,
                        direction: 90.0,
                        particle_count: 12,
                    },
                    window,
                    cx,
                )
            });
            cx.simulate_resize(size(px(640.0), px(360.0)));
            cx.update(|window, _| window.activate_window());
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let active = cx.update(|window, _| {
                (
                    window.is_window_active(),
                    window.is_visible(),
                    window.is_minimized(),
                )
            });
            surface.read_with(cx, |surface, _| {
                assert!(
                    surface.timer.is_some(),
                    "{kind:?} must animate: surface visible={}, window={active:?}",
                    surface.visible
                )
            });
            cx.deactivate_window();
            cx.run_until_parked();
            surface.read_with(cx, |surface, _| {
                assert!(
                    surface.timer.is_none(),
                    "{kind:?} must cancel repaint work on deactivation"
                );
                let now = Instant::now();
                assert_eq!(
                    surface.clock.position(now),
                    surface.clock.position(now + Duration::from_secs(5))
                );
            });
            cx.update(|window, cx| {
                window.activate_window();
                cx.set_reduce_motion(true);
                window.draw(cx).clear(cx);
            });
            cx.run_until_parked();
            surface.read_with(cx, |surface, _| {
                assert!(
                    surface.timer.is_none(),
                    "{kind:?} must stay still with reduced motion"
                )
            });
            cx.update(|window, cx| {
                cx.set_reduce_motion(false);
                window.draw(cx).clear(cx);
            });
            surface.read_with(cx, |surface, _| {
                assert!(
                    surface.timer.is_some(),
                    "{kind:?} must resume after reduced motion"
                )
            });
            if kind == GeneratedEffectKind::Mineral {
                surface.update(cx, |surface, _| surface.preferences.sheen = 0.0);
                cx.update(|window, cx| window.draw(cx).clear(cx));
                surface.read_with(cx, |surface, _| {
                    assert!(
                        surface.timer.is_none(),
                        "grain alone must not schedule animation"
                    );
                    let now = Instant::now();
                    assert_eq!(
                        surface.clock.position(now),
                        surface.clock.position(now + Duration::from_secs(5))
                    );
                });
            }
        }
    }
}
