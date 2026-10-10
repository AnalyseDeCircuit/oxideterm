mod surface;
use surface::media_layer;

use crate::{
    BackgroundFit, BackgroundPreferences, PlaybackLimits, background_image_layer,
    clock::PlaybackClock,
    gpu_budget::{GpuBudget, GpuLease},
};
use gpui::{
    AnyElement, App, AppContext, Bounds, Context, DevicePixels, DynamicTexture, Entity, Global,
    GpuSubmission, Render, RenderImage, Size, Subscription, Task, WeakEntity, Window, WindowId,
    canvas, div, prelude::*, rgba,
};
use oxideterm_background_media::{
    MediaError, MediaFrame, MediaInfo, MediaReply, MediaStream, OutputParams,
};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::{Rc, Weak},
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Eq, Hash, PartialEq)]
struct SourceKey {
    window: WindowId,
    path: PathBuf,
    fit: BackgroundFit,
    blur: u32,
    limits: PlaybackLimits,
    gpu_processing: bool,
}

#[derive(Default)]
struct Players(HashMap<SourceKey, WeakEntity<Player>>);
impl Global for Players {}

#[derive(Default)]
struct Consumer {
    width: u32,
    height: u32,
    visible: bool,
    paused: bool,
}

struct TextureSlot {
    texture: Arc<DynamicTexture>,
    _lease: Arc<GpuLease>,
}

struct Upload {
    submission: GpuSubmission,
    frame: Arc<MediaFrame>,
    info: MediaInfo,
    slot: usize,
}

struct Player {
    source: SourceKey,
    stream: Option<MediaStream>,
    consumers: Vec<Weak<RefCell<Consumer>>>,
    clock: PlaybackClock,
    current: Option<Arc<MediaFrame>>,
    info: Option<MediaInfo>,
    candidate: Option<Arc<MediaFrame>>,
    candidate_info: Option<MediaInfo>,
    presented_at: Duration,
    slots: Vec<TextureSlot>,
    retired_slots: Option<(Vec<TextureSlot>, usize)>,
    front: usize,
    uploading: Option<Upload>,
    renderer_generation: u64,
    active: bool,
    ended: bool,
    error: Option<MediaError>,
    resource_retries: u8,
    resource_retry: Option<Task<()>>,
    decode: Option<Task<()>>,
    timer: Option<Task<()>>,
    completion: Option<Task<()>>,
    subscriptions: Vec<Subscription>,
    gpu_budget: GpuBudget,
    quality: crate::quality::AutomaticQuality,
    presentation: Option<(GpuSubmission, Arc<MediaFrame>)>,
    sampled_sequence: u64,
    native_lease: Option<Arc<GpuLease>>,
    native_disabled: bool,
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    native_texture: Option<Arc<dyn std::any::Any + Send + Sync>>,
    #[cfg(target_os = "linux")]
    importer: Option<crate::dma_buf::Importer>,
    on_failure: Option<Arc<dyn Fn(crate::BackgroundFailure) + Send + Sync>>,
}

pub fn is_streaming_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["gif", "webp", "mp4", "m4v"]
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}

pub fn background_layer(
    background: BackgroundPreferences,
    image: Option<Arc<RenderImage>>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    if background.scene.pause_on_input
        || background.scene.parallax
        || background.scene.day_cycle
        || background.scene.camera.is_some()
    {
        return crate::scene::layer(background, image, window, cx);
    }
    composed_layer(background, image, None, None, None, None, window, cx)
}

pub(crate) fn composed_layer(
    background: BackgroundPreferences,
    image: Option<Arc<RenderImage>>,
    offset: Option<(f32, f32)>,
    effect_offset: Option<(f32, f32)>,
    camera: Option<crate::scene::CameraTransform>,
    lighting: Option<(u32, f32)>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let paused = background.scene.paused;
    let effect = background.effect.filter(|effect| effect.strength > 0.0);
    let readability = background
        .readability
        .filter(|overlay| overlay.opacity > 0.0);
    if effect.is_none()
        && readability.is_none()
        && offset.is_none()
        && camera.is_none()
        && lighting.is_none()
    {
        return media_layer(background, image, window, cx);
    }
    let mut media = media_layer(background, image, window, cx);
    if let Some(camera) = camera {
        media = div()
            .absolute()
            .left(gpui::relative(-camera.zoom + camera.x))
            .right(gpui::relative(-camera.zoom - camera.x))
            .top(gpui::relative(-camera.zoom + camera.y))
            .bottom(gpui::relative(-camera.zoom - camera.y))
            .child(media)
            .into_any_element();
    }
    div()
        .absolute()
        .inset_0()
        .overflow_hidden()
        .child(parallax_layer(media, offset, 1.0))
        .when_some(effect, |layer, effect| {
            let particles = matches!(
                effect.kind,
                crate::GeneratedEffectKind::Particles | crate::GeneratedEffectKind::TideParticles
            );
            layer.child(parallax_layer(
                crate::effect::effect_layer(
                    effect,
                    paused,
                    if particles { effect_offset } else { None },
                    window,
                    cx,
                ),
                if particles { None } else { effect_offset },
                1.6,
            ))
        })
        .when_some(lighting, |layer, (color, strength)| {
            layer.child(div().absolute().inset_0().bg(rgba(
                (color << 8) | (strength.clamp(0.0, 1.0) * 255.0).round() as u32,
            )))
        })
        .when_some(readability, |layer, overlay| {
            layer.child(div().absolute().inset_0().bg(rgba(
                ((overlay.color & 0xffffff) << 8)
                    | (overlay.opacity.clamp(0.0, 1.0) * 255.0).round() as u32,
            )))
        })
        .into_any_element()
}

fn parallax_layer(layer: AnyElement, offset: Option<(f32, f32)>, depth: f32) -> AnyElement {
    let Some((x, y)) = offset else {
        return layer;
    };
    let margin = 8.0 * depth;
    div()
        .absolute()
        .left(gpui::px(-margin + x * depth))
        .right(gpui::px(-margin - x * depth))
        .top(gpui::px(-margin + y * depth))
        .bottom(gpui::px(-margin - y * depth))
        .child(layer)
        .into_any_element()
}

impl Player {
    fn new(
        source: SourceKey,
        on_failure: Option<Arc<dyn Fn(crate::BackgroundFailure) + Send + Sync>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut player = Self {
            source,
            stream: None,
            consumers: Vec::new(),
            clock: PlaybackClock::default(),
            current: None,
            info: None,
            candidate: None,
            candidate_info: None,
            presented_at: Duration::ZERO,
            slots: Vec::new(),
            retired_slots: None,
            front: 0,
            uploading: None,
            renderer_generation: window.renderer_resource_generation(),
            active: false,
            ended: false,
            error: None,
            resource_retries: 0,
            resource_retry: None,
            decode: None,
            timer: None,
            completion: None,
            subscriptions: Vec::new(),
            gpu_budget: GpuBudget::default(),
            quality: Default::default(),
            presentation: None,
            sampled_sequence: 0,
            native_lease: None,
            native_disabled: false,
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            native_texture: None,
            #[cfg(target_os = "linux")]
            importer: None,
            on_failure,
        };
        player
            .subscriptions
            .push(cx.observe_window_activation(window, |player, window, cx| {
                player.sync(window, cx);
                cx.notify();
            }));
        player.subscriptions.push(
            cx.observe_window_visibility(window, |player, _, window, cx| {
                player.sync(window, cx);
                cx.notify();
            }),
        );
        let weak = cx.weak_entity();
        let window_id = window.window_handle().window_id();
        player
            .subscriptions
            .push(cx.on_window_closed(move |cx, closed| {
                if closed == window_id {
                    let weak = weak.clone();
                    cx.defer(move |cx| {
                        let _ = weak.update(cx, |player, _| player.stop());
                    });
                }
            }));
        player
            .subscriptions
            .push(cx.on_release_in(window, |player, window, _| {
                for slot in player.slots.drain(..) {
                    let _ = window.drop_dynamic_texture(slot.texture);
                }
                if let Some((slots, _)) = player.retired_slots.take() {
                    for slot in slots {
                        let _ = window.drop_dynamic_texture(slot.texture);
                    }
                }
                player.stop();
            }));
        player
    }

    fn output(&mut self) -> OutputParams {
        self.consumers
            .retain(|consumer| consumer.strong_count() > 0);
        let mut width = 1;
        let mut height = 1;
        for consumer in self.consumers.iter().filter_map(Weak::upgrade) {
            let consumer = consumer.borrow();
            if consumer.visible {
                width = width.max(consumer.width);
                height = height.max(consumer.height);
            }
        }
        let limits = self
            .quality
            .limits(self.source.limits, !self.source.gpu_processing);
        if self.source.gpu_processing {
            // Apply the shared resolution limit once, then sample at each consumer's actual size.
            (width, height) = self
                .info
                .as_ref()
                .map_or((8192, 8192), |info| (info.width, info.height));
        }
        OutputParams {
            width,
            height,
            fit: self.source.fit,
            blur: self.source.blur as f32 / 1000.0,
            limits,
        }
    }

    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let visible = self
            .consumers
            .iter()
            .filter_map(Weak::upgrade)
            .any(|consumer| consumer.borrow().visible);
        self.active = visible
            && self
                .consumers
                .iter()
                .filter_map(Weak::upgrade)
                .any(|consumer| {
                    let consumer = consumer.borrow();
                    consumer.visible && !consumer.paused
                })
            && window.is_window_active()
            && window.is_visible()
            && !window.is_minimized()
            && !cx.reduce_motion();
        self.clock.set_running(
            self.active && self.current.is_some() && !self.ended,
            Instant::now(),
        );
        if let Some(stream) = &self.stream
            && self.current.is_some()
        {
            if self.active {
                stream.resume();
            } else {
                stream.pause();
            }
        }
        if !self.active {
            self.timer = None;
            // Paused media still needs a poster when first mounted, including reduced-motion mode.
            if !visible || !window.is_visible() || window.is_minimized() || self.current.is_some() {
                return;
            }
        }
        if self.error.is_some() || self.ended {
            return;
        }
        if self.stream.is_none() {
            let device = self.native_device(window);
            let stream = if let Some(device) = device {
                MediaStream::spawn_native(self.source.path.clone(), 1, device)
            } else if self.source.gpu_processing {
                MediaStream::spawn(self.source.path.clone(), 1)
            } else {
                #[cfg(target_os = "macos")]
                let stream = MediaStream::spawn(self.source.path.clone(), 1);
                #[cfg(not(target_os = "macos"))]
                let stream = MediaStream::spawn_software(self.source.path.clone(), 1);
                stream
            };
            match stream {
                Ok(stream) => self.stream = Some(stream),
                Err(error) => {
                    self.fail(error, cx);
                    return;
                }
            }
        }
        if self.candidate.is_none() && self.decode.is_none() && self.uploading.is_none() {
            let output = self.output();
            let mut earliest = self.clock.position(Instant::now());
            if self.current.is_some()
                && let Some(fps) = output.limits.max_fps
            {
                earliest =
                    earliest.max(self.presented_at + Duration::from_secs_f64(1.0 / fps as f64));
            }
            let stream = self.stream.as_mut().expect("active stream");
            if stream.request_next(earliest, output) {
                let response = stream.recv();
                self.decode = Some(cx.spawn(async move |player, cx| {
                    let reply = response.await;
                    let _ = player.update(cx, |player, cx| {
                        player.decode = None;
                        match reply {
                            Some(MediaReply::Frame { info, frame }) => {
                                player.candidate_info = Some(info);
                                player.candidate = Some(Arc::new(frame));
                            }
                            Some(MediaReply::End) => {
                                player.ended = true;
                                player.stream = None;
                            }
                            Some(MediaReply::Error(error)) => player.fail(error, cx),
                            None => player.fail(
                                MediaError::Decode(
                                    "background worker stopped before replying".into(),
                                ),
                                cx,
                            ),
                        }
                        cx.notify();
                    });
                }));
            }
        }
        if self.active
            && let Some(timestamp) = self
                .uploading
                .as_ref()
                .map(|upload| upload.frame.timestamp)
                .or_else(|| self.candidate.as_ref().map(|frame| frame.timestamp))
        {
            let remaining = self
                .presentation_time(timestamp)
                .saturating_sub(self.clock.position(Instant::now()));
            if !remaining.is_zero() && self.timer.is_none() {
                self.timer = Some(cx.spawn(async move |player, cx| {
                    cx.background_executor().timer(remaining).await;
                    let _ = player.update(cx, |player, cx| {
                        player.timer = None;
                        cx.notify();
                    });
                }));
            }
        }
    }

    fn paint(
        &mut self,
        bounds: Bounds<gpui::Pixels>,
        alignment: (f32, f32),
        fit: BackgroundFit,
        blur: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((submission, frame)) = &self.presentation
            && let Some(elapsed) = submission.elapsed()
        {
            if self.active {
                let fps = self
                    .quality
                    .limits(self.source.limits, !self.source.gpu_processing)
                    .max_fps
                    .unwrap();
                self.quality.observe(
                    frame.processing_time.max(elapsed),
                    frame.duration,
                    fps,
                    Instant::now(),
                );
            }
            self.presentation = None;
        }
        let renderer_changed = window.renderer_resource_generation() != self.renderer_generation;
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        if renderer_changed {
            // External textures belong to the lost device; obtain a new decoder/import context.
            self.stream = None;
            self.decode = None;
            if self
                .current
                .as_ref()
                .is_some_and(|frame| frame.native.is_some())
            {
                self.current = None;
            }
            self.candidate = self.current.clone();
            self.candidate_info = self.info.clone();
            self.native_texture = None;
            self.native_lease = None;
            self.presentation = None;
            self.uploading = None;
            self.completion = None;
            for slot in self.slots.drain(..) {
                let _ = window.drop_dynamic_texture(slot.texture);
            }
            if let Some((slots, _)) = self.retired_slots.take() {
                for slot in slots {
                    let _ = window.drop_dynamic_texture(slot.texture);
                }
            }
            self.renderer_generation = window.renderer_resource_generation();
            self.native_disabled = false;
            #[cfg(target_os = "linux")]
            {
                self.importer = None;
            }
        }
        let restoring = renderer_changed && self.current.is_some();
        if restoring {
            // A pending texture belongs to the lost renderer; retain the visible CPU frame for restoration.
            self.uploading = None;
            self.completion = None;
            self.candidate = self.current.clone();
            self.candidate_info = self.info.clone();
        }
        if self
            .candidate
            .as_ref()
            .is_some_and(|frame| frame.native.is_some())
            && (self.current.is_none()
                || restoring
                || (self.active
                    && self.presentation_time(self.candidate.as_ref().unwrap().timestamp)
                        <= self.clock.position(Instant::now())))
        {
            let frame = self.candidate.take().unwrap();
            match self
                .gpu_budget
                .reserve(frame.native.as_ref().unwrap().byte_len())
            {
                Ok(lease) => {
                    let lease = Arc::new(lease);
                    #[cfg(target_os = "linux")]
                    match self
                        .importer
                        .as_ref()
                        .ok_or(MediaError::Unsupported)
                        .and_then(|importer| importer.import(frame.clone(), lease.clone()))
                    {
                        Ok(view) => self.native_texture = Some(view),
                        Err(error) => {
                            self.use_cpu_frames(error);
                            self.paint_current(bounds, alignment, fit, blur, window);
                            self.sync(window, cx);
                            return;
                        }
                    }
                    #[cfg(target_os = "windows")]
                    {
                        self.native_texture =
                            Some(Arc::new(frame.native.as_ref().unwrap().view.clone()));
                    }
                    self.native_lease = Some(lease);
                    self.current = Some(frame);
                    self.resource_retries = 0;
                    self.info = self.candidate_info.take();
                    self.presented_at = self.clock.position(Instant::now());
                    self.renderer_generation = window.renderer_resource_generation();
                    self.clock.set_running(self.active, Instant::now());
                }
                Err(error) => self.use_cpu_frames(error),
            }
        }
        if let Some(upload) = &self.uploading
            && upload.submission.is_complete()
            && (self
                .current
                .as_ref()
                .is_none_or(|current| Arc::ptr_eq(current, &upload.frame))
                || (self.active
                    && self.presentation_time(upload.frame.timestamp)
                        <= self.clock.position(Instant::now())))
        {
            let upload = self.uploading.take().expect("completed upload");
            self.front = upload.slot;
            self.current = Some(upload.frame);
            self.resource_retries = 0;
            self.native_lease = None;
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            {
                self.native_texture = None;
            }
            self.ended |= !upload.info.animated;
            if !upload.info.animated {
                self.stream = None;
            }
            self.info = Some(upload.info);
            self.presented_at = self.clock.position(Instant::now());
            self.clock
                .set_running(self.active && !self.ended, Instant::now());
            self.completion = None;
            if let Some((slots, _)) = self.retired_slots.take() {
                for slot in slots {
                    let _ = window.drop_dynamic_texture(slot.texture);
                }
            }
        }
        if self.uploading.is_none()
            && self.error.is_none()
            && self.candidate.as_ref().is_some_and(|frame| {
                if frame.native.is_some() {
                    return false;
                }
                let _ = frame;
                true
            })
            && (restoring || self.active || self.current.is_none())
        {
            // Prepare the back texture before its deadline; flip only when its timestamp is due.
            let candidate = self.candidate.take().expect("decoded frame");
            if let Err(error) = self.upload(candidate, window, cx) {
                self.fail(error, cx);
            }
        }
        self.paint_current(bounds, alignment, fit, blur, window);
        if self.presentation.is_none()
            && let Some(frame) = &self.current
            && frame.sequence != self.sampled_sequence
            && let Ok(submission) = window.stream_presentation()
        {
            self.sampled_sequence = frame.sequence;
            self.presentation = Some((submission, frame.clone()));
        }
        self.sync(window, cx);
    }

    fn upload(
        &mut self,
        frame: Arc<MediaFrame>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), MediaError> {
        let size = Size::new(
            DevicePixels(frame.width as i32),
            DevicePixels(frame.height as i32),
        );
        let generation = window.renderer_resource_generation();
        if generation != self.renderer_generation {
            for slot in self.slots.drain(..) {
                let _ = window.drop_dynamic_texture(slot.texture);
            }
            if let Some((slots, _)) = self.retired_slots.take() {
                for slot in slots {
                    let _ = window.drop_dynamic_texture(slot.texture);
                }
            }
        }
        self.renderer_generation = generation;
        if self.slots.is_empty() || self.slots[0].texture.size() != size {
            let leases = [
                self.gpu_budget.reserve(frame.pixels.len())?,
                self.gpu_budget.reserve(frame.pixels.len())?,
            ];
            let slots = leases
                .into_iter()
                .map(|lease| TextureSlot {
                    texture: Arc::new(DynamicTexture::new(size)),
                    _lease: Arc::new(lease),
                })
                .collect();
            let old = std::mem::replace(&mut self.slots, slots);
            if !old.is_empty() {
                self.retired_slots = Some((old, self.front));
            }
        }
        let slot = if self.current.is_some() && self.retired_slots.is_none() {
            1 - self.front
        } else {
            0
        };
        let (cpu_staging, gpu_staging) = window
            .stream_texture_staging_bytes(size)
            .map_err(|error| MediaError::Decode(error.to_string()))?;
        let staging = frame.memory_budget().reserve(cpu_staging)?;
        let gpu_staging = self.gpu_budget.reserve(gpu_staging)?;
        let submission = window
            .update_stream_texture(&self.slots[slot].texture, &frame.pixels)
            .map_err(|error| MediaError::Decode(error.to_string()))?;
        submission.retain(frame.clone());
        submission.retain(self.slots[slot]._lease.clone());
        submission.retain(staging);
        submission.retain(gpu_staging);
        let info = self
            .candidate_info
            .take()
            .or_else(|| self.info.clone())
            .expect("frame includes source metadata");
        self.uploading = Some(Upload {
            submission: submission.clone(),
            frame,
            info,
            slot,
        });
        self.completion = Some(cx.spawn_in(window, async move |player, cx| {
            while !submission.is_complete() {
                cx.background_executor()
                    .timer(Duration::from_millis(8))
                    .await;
                if cx
                    .update(|window, _| window.poll_gpu_submissions())
                    .is_err()
                {
                    return;
                }
            }
            let _ = player.update(cx, |player, cx| {
                // Future frames already have a presentation timer; completion need not redraw them early.
                if player.current.is_none()
                    || player.uploading.as_ref().is_some_and(|upload| {
                        player.presentation_time(upload.frame.timestamp)
                            <= player.clock.position(Instant::now())
                    })
                {
                    cx.notify();
                }
            });
        }));
        Ok(())
    }

    fn stop(&mut self) {
        self.resource_retry = None;
        self.stream = None;
        self.decode = None;
        self.timer = None;
        self.completion = None;
        self.candidate = None;
        self.current = None;
        self.presentation = None;
        self.native_lease = None;
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        {
            self.native_texture = None;
        }
        #[cfg(target_os = "linux")]
        {
            self.importer = None;
        }
        self.slots.clear();
        self.retired_slots = None;
        self.ended = true;
    }

    fn freeze(&mut self) {
        self.resource_retry = None;
        if matches!(self.error, Some(MediaError::ResourceExhausted)) {
            // Admission can recover while another source is selected, even after retries ran out.
            // A returning consumer gets a fresh bounded attempt while the last frame stays visible.
            self.error = None;
            self.resource_retries = 0;
        }
        self.stream = None;
        self.decode = None;
        self.timer = None;
        self.candidate = None;
        self.active = false;
        self.presentation = None;
        self.clock.set_running(false, Instant::now());
    }

    fn presentation_time(&self, timestamp: Duration) -> Duration {
        if self.current.is_some()
            && let Some(fps) = self
                .quality
                .limits(self.source.limits, !self.source.gpu_processing)
                .max_fps
        {
            timestamp.max(self.presented_at + Duration::from_secs_f64(1.0 / fps as f64))
        } else {
            timestamp
        }
    }

    fn paint_current(
        &self,
        bounds: Bounds<gpui::Pixels>,
        alignment: (f32, f32),
        fit: BackgroundFit,
        blur: f32,
        window: &mut Window,
    ) {
        let blur_radius = |image_width: f32| {
            if !self.source.gpu_processing || blur <= 0.01 {
                return 0.0;
            }
            let Some(info) = &self.info else { return 0.0 };
            let width = (bounds.size.width.as_f32() * window.scale_factor())
                .ceil()
                .max(1.0) as u32;
            let height = (bounds.size.height.as_f32() * window.scale_factor())
                .ceil()
                .max(1.0) as u32;
            let reference = oxideterm_background_media::output_dimensions(
                (info.width, info.height),
                OutputParams {
                    width,
                    height,
                    fit,
                    blur: 0.0,
                    limits: self.quality.limits(self.source.limits, false),
                },
            );
            reference.map_or(0.0, |(width, _)| blur * image_width / width as f32)
        };
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        if let Some(frame) = &self.current
            && frame.native.is_some()
            && let Some(texture) = &self.native_texture
        {
            let dimensions = if fit == BackgroundFit::Tile {
                self.info
                    .as_ref()
                    .map_or((frame.width, frame.height), |info| {
                        (info.width, info.height)
                    })
            } else {
                (frame.width, frame.height)
            };
            let size = Size::new(
                DevicePixels(dimensions.0 as i32),
                DevicePixels(dimensions.1 as i32),
            );
            let image_bounds =
                crate::layer::aligned_background_bounds(bounds, size, fit, alignment);
            let radius = blur_radius(image_bounds.size.width.as_f32());
            #[cfg(target_os = "windows")]
            let rotation = frame.native.as_ref().unwrap().rotation;
            #[cfg(target_os = "linux")]
            let rotation = 0;
            window.with_filter_layer(
                bounds,
                gpui::Corners::default(),
                &[gpui::Filter::Blur(gpui::px(radius))],
                |window| {
                    window.paint_video_texture(
                        image_bounds,
                        texture.clone(),
                        size,
                        rotation,
                        Size::new(
                            DevicePixels(frame.width as i32),
                            DevicePixels(frame.height as i32),
                        ),
                    );
                },
            );
            if let Ok(presentation) = window.stream_presentation() {
                presentation.retain(frame.clone());
                if let Some(lease) = &self.native_lease {
                    presentation.retain(lease.clone());
                }
            }
            return;
        }
        #[cfg(target_os = "macos")]
        if let Some(frame) = &self.current
            && let Some(native) = &frame.native
        {
            use core_foundation::base::TCFType;
            // Bridge a retained immutable CF object; the GPU submission below owns its lifetime.
            let buffer = unsafe {
                core_video::pixel_buffer::CVPixelBuffer::wrap_under_get_rule(
                    std::ptr::from_ref(&*native.buffer).cast_mut().cast(),
                )
            };
            let (width, height) = if fit == BackgroundFit::Tile {
                self.info
                    .as_ref()
                    .map_or((frame.width, frame.height), |info| {
                        (info.width, info.height)
                    })
            } else {
                (frame.width, frame.height)
            };
            let image_bounds = crate::layer::aligned_background_bounds(
                bounds,
                Size::new(DevicePixels(width as i32), DevicePixels(height as i32)),
                fit,
                alignment,
            );
            let radius = blur_radius(image_bounds.size.width.as_f32());
            window.with_filter_layer(
                bounds,
                gpui::Corners::default(),
                &[gpui::Filter::Blur(gpui::px(radius))],
                |window| {
                    window.paint_video_surface(
                        image_bounds,
                        buffer,
                        u32::from(native.rotation),
                        Size::new(
                            DevicePixels(frame.width as i32),
                            DevicePixels(frame.height as i32),
                        ),
                    );
                },
            );
            if let Ok(presentation) = window.stream_presentation() {
                presentation.retain(frame.clone());
                if let Some(lease) = &self.native_lease {
                    presentation.retain(lease.clone());
                }
            }
            return;
        }
        let (slots, front) = self
            .retired_slots
            .as_ref()
            .map(|(slots, front)| (slots, *front))
            .unwrap_or((&self.slots, self.front));
        if self.current.is_some()
            && let Some(slot) = slots.get(front)
        {
            let size = if fit == BackgroundFit::Tile {
                self.info
                    .as_ref()
                    .map(|info| {
                        Size::new(
                            DevicePixels(info.width as i32),
                            DevicePixels(info.height as i32),
                        )
                    })
                    .unwrap_or_else(|| slot.texture.size())
            } else {
                slot.texture.size()
            };
            let image_bounds =
                crate::layer::aligned_background_bounds(bounds, size, fit, alignment);
            let radius = blur_radius(image_bounds.size.width.as_f32());
            window.with_filter_layer(
                bounds,
                gpui::Corners::default(),
                &[gpui::Filter::Blur(gpui::px(radius))],
                |window| {
                    let _ = window.paint_dynamic_texture(
                        image_bounds,
                        gpui::Corners::default(),
                        slot.texture.clone(),
                        false,
                    );
                },
            );
            if let Ok(presentation) = window.stream_presentation() {
                presentation.retain(slot._lease.clone());
            }
        }
    }

    fn fail(&mut self, error: MediaError, cx: &mut Context<Self>) {
        if self.error.is_some() {
            return;
        }
        self.stream = None;
        self.timer = None;
        self.clock.set_running(false, Instant::now());
        if matches!(error, MediaError::ResourceExhausted) && self.resource_retries < 3 {
            // Cancelled decoders and submitted frames release their budgets asynchronously.
            // Bound the wait so a source that cannot fit still reports its real failure.
            let delay = Duration::from_millis(100 << self.resource_retries);
            self.resource_retries += 1;
            self.error = Some(error);
            self.resource_retry = Some(cx.spawn(async move |player, cx| {
                cx.background_executor().timer(delay).await;
                let _ = player.update(cx, |player, cx| {
                    player.resource_retry = None;
                    player.error = None;
                    cx.notify();
                });
            }));
            return;
        }
        if let Some(handler) = &self.on_failure {
            let kind = match error {
                MediaError::Unsupported => crate::BackgroundFailure::Unsupported,
                MediaError::ResourceExhausted => crate::BackgroundFailure::ResourceExhausted,
                MediaError::InvalidOutput => crate::BackgroundFailure::InvalidOutput,
                MediaError::Cancelled => {
                    self.error = Some(error);
                    return;
                }
                MediaError::Decode(_) => crate::BackgroundFailure::Decode,
            };
            handler(kind);
        }
        self.error = Some(error);
    }

    fn use_cpu_frames(&mut self, error: MediaError) {
        log::debug!("Native video import unavailable; using CPU delivery: {error}");
        self.native_disabled = true;
        self.stream = None;
        self.decode = None;
        self.candidate = None;
        self.candidate_info = None;
        #[cfg(target_os = "linux")]
        {
            self.importer = None;
        }
    }

    fn native_device(
        &mut self,
        window: &Window,
    ) -> Option<oxideterm_background_media::NativeVideoDevice> {
        if !self.source.gpu_processing || self.native_disabled {
            return None;
        }
        #[cfg(target_os = "macos")]
        {
            let _ = window;
            Some(oxideterm_background_media::NativeVideoDevice::Metal)
        }
        #[cfg(target_os = "windows")]
        {
            window
                .gpu_context()?
                .downcast::<windows::Win32::Graphics::Direct3D11::ID3D11Device>()
                .ok()
                .map(|device| oxideterm_background_media::NativeVideoDevice::DirectX(*device))
        }
        #[cfg(target_os = "linux")]
        {
            self.importer = crate::dma_buf::Importer::new(window.gpu_context()?);
            self.importer
                .as_ref()
                .map(|_| oxideterm_background_media::NativeVideoDevice::DmaBuf)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};

    #[gpui::test]
    fn resource_pressure_reports_once_per_activation_after_bounded_retries(
        cx: &mut gpui::TestAppContext,
    ) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let failures = Arc::new(AtomicUsize::new(0));
        let reported = failures.clone();
        let window = cx.add_empty_window();
        let player = window.update(|window, cx| {
            cx.new(|cx| {
                Player::new(
                    SourceKey {
                        window: window.window_handle().window_id(),
                        path: "resource-pressure.webp".into(),
                        fit: BackgroundFit::Cover,
                        blur: 0,
                        limits: Default::default(),
                        gpu_processing: false,
                    },
                    Some(Arc::new(move |failure| {
                        assert!(matches!(
                            failure,
                            crate::BackgroundFailure::ResourceExhausted
                        ));
                        reported.fetch_add(1, Ordering::SeqCst);
                    })),
                    window,
                    cx,
                )
            })
        });
        for activation in 1..=2 {
            for delay in [100, 200, 400] {
                player.update(&mut *window, |player, cx| {
                    player.fail(MediaError::ResourceExhausted, cx)
                });
                window.run_until_parked();
                assert_eq!(
                    failures.load(Ordering::SeqCst),
                    activation - 1,
                    "temporary pressure must have time to clear"
                );
                window
                    .executor()
                    .advance_clock(Duration::from_millis(delay));
                window.run_until_parked();
            }
            for _ in 0..2 {
                player.update(&mut *window, |player, cx| {
                    player.fail(MediaError::ResourceExhausted, cx)
                });
            }
            assert_eq!(
                failures.load(Ordering::SeqCst),
                activation,
                "persistent admission failures must be reported, without repeating notices"
            );
            player.update(&mut *window, |player, _| player.freeze());
        }
    }

    #[gpui::test]
    fn uploaded_frames_wait_for_their_deadline_and_paused_restores_do_not(
        cx: &mut gpui::TestAppContext,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("blue.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 255, 255]))
            .save(&path)
            .unwrap();
        let output = OutputParams {
            width: 2,
            height: 2,
            fit: BackgroundFit::Cover,
            blur: 0.0,
            limits: Default::default(),
        };
        let (mut info, blue) = oxideterm_background_media::decode_poster(&path, output).unwrap();
        info.animated = true;
        let (_, mut red) = oxideterm_background_media::decode_poster(&path, output).unwrap();
        red.timestamp = Duration::from_millis(500);
        for pixel in red.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[0, 0, 255, 255]);
        }
        let window = cx.add_empty_window();
        let player = window.update(|window, cx| {
            cx.new(|cx| {
                let mut player = Player::new(
                    SourceKey {
                        window: window.window_handle().window_id(),
                        path,
                        fit: BackgroundFit::Cover,
                        blur: 0,
                        limits: Default::default(),
                        gpu_processing: false,
                    },
                    None,
                    window,
                    cx,
                );
                player.current = Some(Arc::new(blue));
                player.info = Some(info.clone());
                // The test supplies completed uploads; no decoder work should run during paint.
                player.ended = true;
                let submission = GpuSubmission::default();
                submission.complete();
                player.uploading = Some(Upload {
                    submission,
                    frame: Arc::new(red),
                    info,
                    slot: 0,
                });
                player
            })
        });
        for (elapsed, expected) in [
            (Duration::ZERO, [255, 0, 0, 255]),
            (Duration::from_secs(1), [0, 0, 255, 255]),
        ] {
            player.update(&mut *window, |player, _| {
                player.active = true;
                player.clock.set_running(true, Instant::now() - elapsed);
            });
            window.draw(point(px(0.0), px(0.0)), size(px(64.0), px(64.0)), |_, _| {
                let player = player.clone();
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        player.update(cx, |player, cx| {
                            player.paint(bounds, (0.5, 0.5), BackgroundFit::Cover, 0.0, window, cx)
                        });
                    },
                )
                .size_full()
                .into_any_element()
            });
            window.update(|_, cx| {
                assert_eq!(
                    &player.read(cx).current.as_ref().unwrap().pixels[..4],
                    &expected
                )
            });
        }
        player.update(&mut *window, |player, _| {
            player.source.limits.max_fps = Some(1);
            player.active = false;
            let submission = GpuSubmission::default();
            submission.complete();
            player.uploading = Some(Upload {
                submission,
                frame: player.current.as_ref().unwrap().clone(),
                info: player.info.as_ref().unwrap().clone(),
                slot: 1,
            });
        });
        window.draw(point(px(0.0), px(0.0)), size(px(64.0), px(64.0)), |_, _| {
            let player = player.clone();
            canvas(
                |_, _, _| (),
                move |bounds, _, window, cx| {
                    player.update(cx, |player, cx| {
                        player.paint(bounds, (0.5, 0.5), BackgroundFit::Cover, 0.0, window, cx)
                    });
                },
            )
            .size_full()
            .into_any_element()
        });
        window.update(|_, cx| {
            assert_eq!(
                player.read(cx).front,
                1,
                "a restored texture must be visible even while playback is paused"
            );
        });
    }
}
