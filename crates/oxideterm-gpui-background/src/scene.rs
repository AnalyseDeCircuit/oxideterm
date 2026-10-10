use crate::BackgroundPreferences;
use crate::clock::PlaybackClock;
use chrono::Timelike;
use gpui::{
    App, AppContext, Bounds, Context, Entity, Global, Pixels, Render, RenderImage, Subscription,
    Task, WeakEntity, Window, WindowId, canvas, div, prelude::*,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default)]
pub struct ScenePreferences {
    pub pause_on_input: bool,
    pub parallax: bool,
    pub camera: Option<CameraPreferences>,
    pub day_cycle: bool,
    pub dark: bool,
    /// A preview-only hour; None follows the local clock.
    pub preview_hour: Option<f32>,
    /// Runtime input state, resolved by the scene rather than persisted in settings.
    pub paused: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum CameraMotion {
    Pan,
    ZoomIn,
    ZoomOut,
}

#[derive(Clone, Copy, Debug)]
pub struct CameraPreferences {
    pub motion: CameraMotion,
    pub amount: f32,
    pub speed: f32,
}

pub(crate) struct CameraTransform {
    pub zoom: f32,
    pub x: f32,
    pub y: f32,
}

impl CameraPreferences {
    fn transform(self, elapsed: Duration) -> CameraTransform {
        let phase =
            elapsed.as_secs_f32() * self.speed.clamp(0.1, 2.0) * std::f32::consts::TAU / 120.0;
        let margin = self.amount.clamp(0.0, 1.0) * 0.04;
        match self.motion {
            CameraMotion::Pan => CameraTransform {
                zoom: margin,
                x: phase.sin() * margin * 0.8,
                y: (phase * 0.5).sin() * margin * 0.4,
            },
            CameraMotion::ZoomIn => CameraTransform {
                zoom: (1.0 - phase.cos()) * 0.5 * margin,
                x: 0.0,
                y: 0.0,
            },
            CameraMotion::ZoomOut => CameraTransform {
                zoom: (1.0 + phase.cos()) * 0.5 * margin,
                x: 0.0,
                y: 0.0,
            },
        }
    }
}

#[derive(Default)]
struct InputWindows(HashMap<WindowId, WeakEntity<InputActivity>>);
impl Global for InputWindows {}

struct InputActivity {
    until: Option<Instant>,
    timer: Option<Task<()>>,
}

impl InputActivity {
    fn schedule(&mut self, cx: &mut Context<Self>) {
        if self.timer.is_some() {
            return;
        }
        let Some(until) = self.until else {
            return;
        };
        let delay = until.saturating_duration_since(cx.background_executor().now());
        self.timer = Some(cx.spawn(async move |state, cx| {
            cx.background_executor().timer(delay).await;
            let _ = state.update(cx, |state, cx| {
                state.timer = None;
                if state
                    .until
                    .is_some_and(|until| until > cx.background_executor().now())
                {
                    state.schedule(cx);
                } else {
                    state.until = None;
                    cx.notify();
                }
            });
        }));
    }
}

/// The terminal publishes activity only; no input data enters the background subsystem.
pub fn note_input_activity(window: WindowId, cx: &mut App) {
    let state = cx
        .default_global::<InputWindows>()
        .0
        .get(&window)
        .and_then(WeakEntity::upgrade);
    if let Some(state) = state {
        state.update(cx, |state, cx| {
            let idle = state.until.is_none();
            state.until = Some(cx.background_executor().now() + Duration::from_secs(1));
            if idle {
                cx.notify();
            }
            state.schedule(cx);
        });
    }
}

fn input_state(window: WindowId, cx: &mut App) -> Entity<InputActivity> {
    if let Some(state) = cx
        .default_global::<InputWindows>()
        .0
        .get(&window)
        .and_then(WeakEntity::upgrade)
    {
        return state;
    }
    let state = cx.new(|_| InputActivity {
        until: None,
        timer: None,
    });
    let windows = &mut cx.default_global::<InputWindows>().0;
    windows.retain(|_, state| state.upgrade().is_some());
    windows.insert(window, state.downgrade());
    state
}

struct Scene {
    background: BackgroundPreferences,
    image: Option<Arc<RenderImage>>,
    input: Entity<InputActivity>,
    _subscriptions: Vec<Subscription>,
    source: PathBuf,
    static_image: bool,
    inspect: Option<Task<()>>,
    offset: (f32, f32),
    target: (f32, f32),
    dragging: bool,
    camera_clock: PlaybackClock,
    timer: Option<(bool, Task<()>)>,
}

pub(crate) fn layer(
    background: BackgroundPreferences,
    image: Option<Arc<RenderImage>>,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
    let state = window
        .use_keyed_state("background-scene", cx, |window, cx| {
            cx.new(|cx| {
                let input = input_state(window.window_handle().window_id(), cx);
                let observation = cx.observe(&input, |scene: &mut Scene, _, cx| {
                    scene
                        .camera_clock
                        .set_running(false, cx.background_executor().now());
                    scene.timer = None;
                    cx.notify();
                });
                let activation =
                    cx.observe_window_activation(window, |scene: &mut Scene, _, cx| {
                        scene
                            .camera_clock
                            .set_running(false, cx.background_executor().now());
                        scene.timer = None;
                        cx.notify();
                    });
                let visibility =
                    cx.observe_window_visibility(window, |scene: &mut Scene, _, _, cx| {
                        scene
                            .camera_clock
                            .set_running(false, cx.background_executor().now());
                        scene.timer = None;
                        cx.notify();
                    });
                Scene {
                    background: background.clone(),
                    image: image.clone(),
                    input,
                    _subscriptions: vec![observation, activation, visibility],
                    source: PathBuf::new(),
                    static_image: false,
                    inspect: None,
                    offset: (0.0, 0.0),
                    target: (0.0, 0.0),
                    dragging: false,
                    camera_clock: PlaybackClock::default(),
                    timer: None,
                }
            })
        })
        .read(cx)
        .clone();
    state.update(cx, |state, _| {
        state.background = background;
        state.image = image;
    });
    state.into_any_element()
}

impl Scene {
    fn moving(&self, window: &Window, cx: &App) -> bool {
        self.static_image
            && !cx.reduce_motion()
            && window.is_window_active()
            && !self.dragging
            && !self.background.scene.paused
    }

    fn schedule(&mut self, visible: bool, window: &Window, cx: &mut Context<Self>) {
        let camera_active = self.moving(window, cx)
            && self
                .background
                .scene
                .camera
                .is_some_and(|camera| camera.amount > 0.0);
        self.camera_clock.set_running(
            camera_active && visible && window.is_visible() && !window.is_minimized(),
            cx.background_executor().now(),
        );
        if !visible || !window.is_visible() || window.is_minimized() || !window.is_window_active() {
            self.timer = None;
            return;
        }
        let parallax_active = self.background.scene.parallax
            && self.moving(window, cx)
            && ((self.target.0 - self.offset.0).abs() + (self.target.1 - self.offset.1).abs()
                > 0.01);
        let moving = parallax_active || camera_active;
        let day_cycle =
            self.background.scene.day_cycle && self.background.scene.preview_hour.is_none();
        if !moving && !day_cycle {
            self.timer = None;
            return;
        }
        if self
            .timer
            .as_ref()
            .is_some_and(|(motion, _)| *motion == moving)
        {
            return;
        }
        let delay = if moving {
            Duration::from_millis(if parallax_active { 16 } else { 33 })
        } else {
            Duration::from_secs(60)
        };
        self.timer = Some((
            moving,
            cx.spawn(async move |scene, cx| {
                cx.background_executor().timer(delay).await;
                let _ = scene.update(cx, |scene, cx| {
                    scene.timer = None;
                    scene
                        .camera_clock
                        .set_running(false, cx.background_executor().now());
                    if moving && !scene.dragging && !scene.background.scene.paused {
                        scene.offset.0 += (scene.target.0 - scene.offset.0) * 0.22;
                        scene.offset.1 += (scene.target.1 - scene.offset.1) * 0.22;
                    }
                    // Scheduling resumes in prepaint only while this scene is actually mounted.
                    cx.notify();
                });
            }),
        ));
    }

    fn pointer(
        &mut self,
        position: gpui::Point<Pixels>,
        bounds: Bounds<Pixels>,
        dragging: bool,
        cx: &mut Context<Self>,
    ) {
        if (!self.background.scene.parallax && self.background.scene.camera.is_none())
            || !self.static_image
            || cx.reduce_motion()
        {
            return;
        }
        let target = if dragging || !self.background.scene.parallax {
            self.target
        } else {
            if bounds.contains(&position) {
                (
                    ((position.x - bounds.origin.x).as_f32() / bounds.size.width.as_f32() - 0.5)
                        * 12.0,
                    ((position.y - bounds.origin.y).as_f32() / bounds.size.height.as_f32() - 0.5)
                        * 12.0,
                )
            } else {
                (0.0, 0.0)
            }
        };
        if self.dragging == dragging && self.target == target {
            return;
        }
        self.dragging = dragging;
        self.target = target;
        // Keep an in-flight animation tick so frequent pointer events cannot postpone it.
        if dragging || self.timer.as_ref().is_some_and(|(motion, _)| !motion) {
            self.camera_clock
                .set_running(false, cx.background_executor().now());
            self.timer = None;
        }
        cx.notify();
    }
}

impl Render for Scene {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.source != self.background.path {
            self.camera_clock = PlaybackClock::default();
            self.source = self.background.path.clone();
            self.static_image =
                !self.source.as_os_str().is_empty() && !crate::is_streaming_source(&self.source);
            self.inspect = None;
            if crate::is_streaming_source(&self.source) {
                let path = self.source.clone();
                let inspect = cx
                    .background_executor()
                    .spawn(async move { oxideterm_background_media::is_animated_media(&path) });
                self.inspect = Some(cx.spawn(async move |scene, cx| {
                    let result = inspect.await;
                    let _ = scene.update(cx, |scene, cx| {
                        scene.static_image = matches!(result, Ok(false));
                        scene.inspect = None;
                        cx.notify();
                    });
                }));
            }
        }
        self.background.scene.paused =
            self.background.scene.pause_on_input && self.input.read(cx).until.is_some();
        if !self.background.scene.parallax || cx.reduce_motion() || !self.static_image {
            self.offset = (0.0, 0.0);
            self.target = (0.0, 0.0);
        }
        let mut background = self.background.clone();
        let lighting = background.scene.day_cycle.then(|| {
            let now = chrono::Local::now();
            let hour = background.scene.preview_hour.unwrap_or(
                now.hour() as f32 + now.minute() as f32 / 60.0 + now.second() as f32 / 3600.0,
            );
            daylight(hour, background.scene.dark)
        });
        if let Some((color, strength)) = lighting
            && let Some(effect) = &mut background.effect
        {
            effect.colors = effect
                .colors
                .map(|base| mix_color(base, color, strength * 2.0));
        }
        let offset = (background.scene.parallax && self.static_image && !cx.reduce_motion())
            .then_some(self.offset);
        let camera = background
            .scene
            .camera
            .filter(|_| self.static_image && !cx.reduce_motion())
            .map(|camera| {
                camera.transform(self.camera_clock.position(cx.background_executor().now()))
            });
        let lighting = lighting.map(|(color, strength)| (color, strength * background.opacity));
        let scene = cx.entity().downgrade();
        let paint_scene = scene.clone();
        div()
            .absolute()
            .inset_0()
            .overflow_hidden()
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let visible = bounds.intersects(&window.content_mask().bounds);
                        let _ = scene.update(cx, |scene, cx| scene.schedule(visible, window, cx));
                    },
                    move |bounds, _, window, _| {
                        let scene = paint_scene.clone();
                        window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, _, cx| {
                            if phase == gpui::DispatchPhase::Capture {
                                let _ = scene.update(cx, |scene, cx| {
                                    scene.pointer(event.position, bounds, true, cx)
                                });
                            }
                        });
                        let scene = paint_scene.clone();
                        window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                            if phase == gpui::DispatchPhase::Capture {
                                let _ = scene.update(cx, |scene, cx| {
                                    scene.pointer(
                                        event.position,
                                        bounds,
                                        event.pressed_button.is_some(),
                                        cx,
                                    )
                                });
                            }
                        });
                        let scene = paint_scene.clone();
                        window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _, cx| {
                            if phase == gpui::DispatchPhase::Capture {
                                let _ = scene.update(cx, |scene, cx| {
                                    scene.pointer(event.position, bounds, false, cx)
                                });
                            }
                        });
                        let scene = paint_scene.clone();
                        window.on_mouse_event(move |_: &gpui::MouseExitEvent, phase, _, cx| {
                            if phase == gpui::DispatchPhase::Capture {
                                let _ = scene.update(cx, |scene, cx| {
                                    if scene.dragging || scene.target == (0.0, 0.0) {
                                        return;
                                    }
                                    scene.target = (0.0, 0.0);
                                    scene
                                        .camera_clock
                                        .set_running(false, cx.background_executor().now());
                                    scene.timer = None;
                                    cx.notify();
                                });
                            }
                        });
                    },
                )
                .absolute()
                .inset_0(),
            )
            .child(crate::player::composed_layer(
                background,
                self.image.clone(),
                offset,
                camera,
                lighting,
                window,
                cx,
            ))
    }
}

fn mix_color(a: u32, b: u32, amount: f32) -> u32 {
    [16, 8, 0].into_iter().fold(0, |color, shift| {
        let a = ((a >> shift) & 255) as f32;
        let b = ((b >> shift) & 255) as f32;
        color | (((a + (b - a) * amount).round() as u32) << shift)
    })
}

fn daylight(hour: f32, dark: bool) -> (u32, f32) {
    let stops = [
        (0.0, 0x183052, 0.22),
        (6.0, 0xf3be82, 0.14),
        (12.0, 0xdceafa, 0.04),
        (18.0, 0xb77585, 0.16),
        (24.0, 0x183052, 0.22),
    ];
    let hour = hour.rem_euclid(24.0);
    let pair = stops.windows(2).find(|pair| hour < pair[1].0).unwrap();
    let amount = (hour - pair[0].0) / (pair[1].0 - pair[0].0);
    let opacity = pair[0].2 + (pair[1].2 - pair[0].2) * amount;
    (
        mix_color(pair[0].1, pair[1].1, amount),
        if dark { opacity * 0.7 } else { opacity },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_stays_cropped_and_reverses_zoom() {
        for motion in [
            CameraMotion::Pan,
            CameraMotion::ZoomIn,
            CameraMotion::ZoomOut,
        ] {
            let camera = CameraPreferences {
                motion,
                amount: 1.0,
                speed: 1.0,
            };
            for seconds in [0, 15, 30, 60, 90, 120, 240] {
                let transform = camera.transform(Duration::from_secs(seconds));
                assert!(
                    transform.x.abs() <= transform.zoom && transform.y.abs() <= transform.zoom,
                    "{motion:?} must keep all edges covered at {seconds}s"
                );
                assert!((0.0..=0.04).contains(&transform.zoom));
            }
        }
        let camera = CameraPreferences {
            motion: CameraMotion::ZoomIn,
            amount: 1.0,
            speed: 1.0,
        };
        for (seconds, expected) in [(0, 0.0), (30, 0.02), (60, 0.04), (90, 0.02), (120, 0.0)] {
            assert!(
                (camera.transform(Duration::from_secs(seconds)).zoom - expected).abs() < 0.00001
            );
        }
        let reversed = CameraPreferences {
            motion: CameraMotion::ZoomOut,
            ..camera
        };
        assert_eq!(reversed.transform(Duration::ZERO).zoom, 0.04);
        assert!(reversed.transform(Duration::from_secs(60)).zoom < 0.00001);
    }

    #[test]
    fn daylight_interpolates_and_wraps_at_midnight() {
        for (hour, dark, color, opacity) in [
            (0.0, false, 0x183052, 0.22),
            (6.0, false, 0xf3be82, 0.14),
            (9.0, false, 0xe8d4be, 0.09),
            (12.0, false, 0xdceafa, 0.04),
            (18.0, true, 0xb77585, 0.112),
            (24.0, false, 0x183052, 0.22),
        ] {
            let actual = daylight(hour, dark);
            assert_eq!(actual.0, color);
            assert!((actual.1 - opacity).abs() < 0.00001);
        }
    }

    #[gpui::test]
    fn repeated_input_extends_pause_only_in_its_window(cx: &mut gpui::TestAppContext) {
        let other = cx
            .add_empty_window()
            .update(|window, cx| input_state(window.window_handle().window_id(), cx));
        let first = cx.add_empty_window();
        let activity =
            first.update(|window, cx| input_state(window.window_handle().window_id(), cx));
        first.update(|window, cx| note_input_activity(window.window_handle().window_id(), cx));
        first.run_until_parked();
        first.executor().advance_clock(Duration::from_millis(800));
        first.update(|window, cx| note_input_activity(window.window_handle().window_id(), cx));
        first.run_until_parked();
        first.executor().advance_clock(Duration::from_millis(300));
        first.run_until_parked();
        first.update(|_, cx| {
            assert_eq!(
                activity.read(cx).until,
                Some(cx.background_executor().now() + Duration::from_millis(700))
            );
            assert_eq!(other.read(cx).until, None);
        });
        first.executor().advance_clock(Duration::from_millis(700));
        first.run_until_parked();
        first.update(|_, cx| assert_eq!(activity.read(cx).until, None));
    }
}
