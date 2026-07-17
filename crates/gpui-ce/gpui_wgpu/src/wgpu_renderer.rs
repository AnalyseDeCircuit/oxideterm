// OxideTerm vendor modification: preserve virtual-adapter metadata for render policy.
// OxideTerm modification: coordinate non-blocking device-loss recovery across windows.
// OxideTerm modification: use product-owned font rendering environment variables.
use crate::{CompositorGpuHint, WgpuAtlas, WgpuContext, WgpuDeviceRequirements};
use bytemuck::{Pod, Zeroable};
use gpui::{
    AtlasTextureId, AtlasTextureKind, AtlasTile, BackdropFilter, Background, BorderStyle, Bounds,
    Corners, DevicePixels, Edges, FilterBoundary, GpuSpecs, Hsla, LinearColorStop,
    MonochromeSprite, PaintSurface, Path, PolychromeSprite, PrimitiveBatch, Quad, ScaledFilter,
    ScaledPixels, Scene, Shadow, Size, SubpixelSprite, TransformationMatrix, Underline,
    get_gamma_correction_ratios,
};
use log::warn;

/// The largest blur radius in a scene-space filter chain, in device pixels — used to size the
/// blur kernel and the dilated region the blur passes are scissored to.
///
/// The `match` is exhaustive on purpose: adding a [`ScaledFilter`] variant breaks it here,
/// forcing this backend to handle (or deliberately ignore) the new filter rather than silently
/// dropping it.
fn max_blur_radius(filters: &[ScaledFilter]) -> f32 {
    filters.iter().fold(0.0, |acc, filter| match filter {
        ScaledFilter::Blur(radius) => acc.max(radius.0),
    })
}
#[cfg(not(target_family = "wasm"))]
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::cell::{Ref, RefCell};
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
#[cfg(not(target_family = "wasm"))]
use std::time::{Duration, Instant};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GlobalParams {
    viewport_size: [f32; 2],
    premultiplied_alpha: u32,
    pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct PodBounds {
    origin: [f32; 2],
    size: [f32; 2],
}

impl From<Bounds<ScaledPixels>> for PodBounds {
    fn from(bounds: Bounds<ScaledPixels>) -> Self {
        Self {
            origin: [bounds.origin.x.0, bounds.origin.y.0],
            size: [bounds.size.width.0, bounds.size.height.0],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SurfaceParams {
    bounds: PodBounds,
    content_mask: PodBounds,
}

/// Uniform passed to the blur pipelines. The same struct drives the downsample, separable
/// gaussian, and composite passes; fields not relevant to a given pass are left zero.
#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct BlurParams {
    /// Composite target rectangle, in device pixels (composite pass only).
    bounds: PodBounds,
    /// Clip rectangle, in device pixels (composite pass only).
    content_mask: PodBounds,
    /// Rounded-corner radii (tl, tr, br, bl), in device pixels (composite pass only).
    corner_radii: [f32; 4],
    /// Per-tap sampling step in UV space (gaussian passes only): (1/width, 0) or (0, 1/height).
    direction: [f32; 2],
    /// Gaussian sigma, in the (half-resolution) blur texture's pixels.
    sigma: f32,
    /// Element opacity, multiplied into the composited result.
    opacity: f32,
    /// Number of taps to each side of center (gaussian passes only).
    tap_count: f32,
    /// Spacing between taps in pixels; >1 lets `tap_count` taps span very large radii without
    /// truncating the gaussian (see #6 in review).
    tap_step: f32,
    /// 1.0 to clip the composite to the rounded rect (backdrop — the panel has a defined shape),
    /// 0.0 to let the blurred result fade out on its own (content `filter` — it bleeds past the
    /// element bounds like CSS, so the fade isn't sharply truncated at the box edge).
    clip_rounded: f32,
    /// 1.0 = snapped 2:1 box downsample (anchor the half-res grid to a fixed 2px grid at the
    /// origin, so a stationary element blurs identically at every window size); 0.0 = 1:1 copy
    /// (the scene blit, which must not downsample). Downsample pass only.
    downsample: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GammaParams {
    gamma_ratios: [f32; 4],
    grayscale_enhanced_contrast: f32,
    subpixel_enhanced_contrast: f32,
    is_bgr: u32,
    pad: u32,
}

// Storage-buffer data is represented by dedicated Pod types. Scene structures may contain Rust
// enums, bools, private fields, or implicit padding and must never be uploaded as raw memory.
#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct GpuCorners {
    top_left: f32,
    top_right: f32,
    bottom_right: f32,
    bottom_left: f32,
}

impl From<&Corners<ScaledPixels>> for GpuCorners {
    fn from(corners: &Corners<ScaledPixels>) -> Self {
        Self {
            top_left: corners.top_left.0,
            top_right: corners.top_right.0,
            bottom_right: corners.bottom_right.0,
            bottom_left: corners.bottom_left.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct GpuEdges {
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
}

impl From<&Edges<ScaledPixels>> for GpuEdges {
    fn from(edges: &Edges<ScaledPixels>) -> Self {
        Self {
            top: edges.top.0,
            right: edges.right.0,
            bottom: edges.bottom.0,
            left: edges.left.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct GpuHsla {
    h: f32,
    s: f32,
    l: f32,
    a: f32,
}

impl From<Hsla> for GpuHsla {
    fn from(color: Hsla) -> Self {
        Self {
            h: color.h,
            s: color.s,
            l: color.l,
            a: color.a,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct GpuLinearColorStop {
    color: GpuHsla,
    percentage: f32,
}

impl From<LinearColorStop> for GpuLinearColorStop {
    fn from(stop: LinearColorStop) -> Self {
        Self {
            color: stop.color.into(),
            percentage: stop.percentage,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct GpuBackground {
    tag: u32,
    color_space: u32,
    solid: GpuHsla,
    gradient_angle_or_pattern_height: f32,
    colors: [GpuLinearColorStop; 2],
    pad: u32,
}

impl From<&Background> for GpuBackground {
    fn from(background: &Background) -> Self {
        let (tag, color_space, solid, gradient_angle_or_pattern_height, colors) =
            background.shader_components();
        Self {
            tag,
            color_space,
            solid: solid.into(),
            gradient_angle_or_pattern_height,
            colors: colors.map(Into::into),
            pad: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct GpuAtlasTextureId {
    index: u32,
    kind: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct GpuAtlasBounds {
    origin: [i32; 2],
    size: [i32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct GpuAtlasTile {
    texture_id: GpuAtlasTextureId,
    tile_id: u32,
    padding: u32,
    bounds: GpuAtlasBounds,
}

impl From<&AtlasTile> for GpuAtlasTile {
    fn from(tile: &AtlasTile) -> Self {
        let kind = match tile.texture_id.kind {
            AtlasTextureKind::Monochrome => 0,
            AtlasTextureKind::Polychrome => 1,
            AtlasTextureKind::Subpixel => 2,
        };
        Self {
            texture_id: GpuAtlasTextureId {
                index: tile.texture_id.index,
                kind,
            },
            tile_id: tile.tile_id.0,
            padding: tile.padding,
            bounds: GpuAtlasBounds {
                origin: [tile.bounds.origin.x.0, tile.bounds.origin.y.0],
                size: [tile.bounds.size.width.0, tile.bounds.size.height.0],
            },
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct GpuTransformationMatrix {
    rotation_scale: [[f32; 2]; 2],
    translation: [f32; 2],
}

impl From<TransformationMatrix> for GpuTransformationMatrix {
    fn from(matrix: TransformationMatrix) -> Self {
        Self {
            rotation_scale: matrix.rotation_scale,
            translation: matrix.translation,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct GpuQuad {
    order: u32,
    border_style: u32,
    bounds: PodBounds,
    content_mask: PodBounds,
    background: GpuBackground,
    border_color: GpuHsla,
    corner_radii: GpuCorners,
    border_widths: GpuEdges,
}

impl From<&Quad> for GpuQuad {
    fn from(quad: &Quad) -> Self {
        let border_style = match quad.border_style {
            BorderStyle::Solid => 0,
            BorderStyle::Dashed => 1,
        };
        Self {
            order: quad.order,
            border_style,
            bounds: quad.bounds.into(),
            content_mask: quad.content_mask.bounds.into(),
            background: (&quad.background).into(),
            border_color: quad.border_color.into(),
            corner_radii: (&quad.corner_radii).into(),
            border_widths: (&quad.border_widths).into(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct GpuShadow {
    order: u32,
    blur_radius: f32,
    bounds: PodBounds,
    corner_radii: GpuCorners,
    content_mask: PodBounds,
    color: GpuHsla,
    element_bounds: PodBounds,
    element_corner_radii: GpuCorners,
    inset: u32,
    pad: u32,
}

impl From<&Shadow> for GpuShadow {
    fn from(shadow: &Shadow) -> Self {
        Self {
            order: shadow.order,
            blur_radius: shadow.blur_radius.0,
            bounds: shadow.bounds.into(),
            corner_radii: (&shadow.corner_radii).into(),
            content_mask: shadow.content_mask.bounds.into(),
            color: shadow.color.into(),
            element_bounds: shadow.element_bounds.into(),
            element_corner_radii: (&shadow.element_corner_radii).into(),
            inset: shadow.inset,
            pad: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct GpuUnderline {
    order: u32,
    pad: u32,
    bounds: PodBounds,
    content_mask: PodBounds,
    color: GpuHsla,
    thickness: f32,
    wavy: u32,
}

impl From<&Underline> for GpuUnderline {
    fn from(underline: &Underline) -> Self {
        Self {
            order: underline.order,
            pad: 0,
            bounds: underline.bounds.into(),
            content_mask: underline.content_mask.bounds.into(),
            color: underline.color.into(),
            thickness: underline.thickness.0,
            wavy: underline.wavy,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct GpuTextSprite {
    order: u32,
    pad: u32,
    bounds: PodBounds,
    content_mask: PodBounds,
    color: GpuHsla,
    tile: GpuAtlasTile,
    transformation: GpuTransformationMatrix,
}

impl From<&MonochromeSprite> for GpuTextSprite {
    fn from(sprite: &MonochromeSprite) -> Self {
        Self {
            order: sprite.order,
            pad: 0,
            bounds: sprite.bounds.into(),
            content_mask: sprite.content_mask.bounds.into(),
            color: sprite.color.into(),
            tile: (&sprite.tile).into(),
            transformation: sprite.transformation.into(),
        }
    }
}

impl From<&SubpixelSprite> for GpuTextSprite {
    fn from(sprite: &SubpixelSprite) -> Self {
        Self {
            order: sprite.order,
            pad: 0,
            bounds: sprite.bounds.into(),
            content_mask: sprite.content_mask.bounds.into(),
            color: sprite.color.into(),
            tile: (&sprite.tile).into(),
            transformation: sprite.transformation.into(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct GpuPolychromeSprite {
    order: u32,
    pad: u32,
    grayscale: u32,
    opacity: f32,
    bounds: PodBounds,
    content_mask: PodBounds,
    corner_radii: GpuCorners,
    tile: GpuAtlasTile,
}

impl From<&PolychromeSprite> for GpuPolychromeSprite {
    fn from(sprite: &PolychromeSprite) -> Self {
        Self {
            order: sprite.order,
            pad: 0,
            grayscale: u32::from(sprite.grayscale),
            opacity: sprite.opacity,
            bounds: sprite.bounds.into(),
            content_mask: sprite.content_mask.bounds.into(),
            corner_radii: (&sprite.corner_radii).into(),
            tile: (&sprite.tile).into(),
        }
    }
}

#[derive(Default)]
struct GpuInstanceScratch {
    quads: Vec<GpuQuad>,
    shadows: Vec<GpuShadow>,
    underlines: Vec<GpuUnderline>,
    monochrome_sprites: Vec<GpuTextSprite>,
    subpixel_sprites: Vec<GpuTextSprite>,
    polychrome_sprites: Vec<GpuPolychromeSprite>,
    path_sprites: Vec<PathSprite>,
    path_vertices: Vec<PathRasterizationVertex>,
}

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct PathSprite {
    bounds: PodBounds,
}

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct PathRasterizationVertex {
    xy_position: [f32; 2],
    st_position: [f32; 2],
    color: GpuBackground,
    bounds: PodBounds,
}

pub struct WgpuSurfaceConfig {
    pub size: Size<DevicePixels>,
    pub transparent: bool,
    /// Preferred presentation mode. When `Some`, the renderer will use this
    /// mode if supported by the surface, falling back to `Fifo`.
    /// When `None`, defaults to `Fifo` (VSync).
    ///
    /// Mobile platforms may prefer `Mailbox` (triple-buffering) to avoid
    /// blocking in `get_current_texture()` during lifecycle transitions.
    pub preferred_present_mode: Option<wgpu::PresentMode>,
}

/// Outcome of a device-loss recovery poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WgpuRecoveryStatus {
    /// The renderer now owns resources created from the recovered device.
    Recovered,
    /// Recovery is already running or is waiting for its retry deadline.
    Deferred,
    /// The bounded recovery policy is exhausted and requires an application restart.
    Failed,
}

struct WgpuPipelines {
    quads: wgpu::RenderPipeline,
    shadows: wgpu::RenderPipeline,
    path_rasterization: wgpu::RenderPipeline,
    paths: wgpu::RenderPipeline,
    underlines: wgpu::RenderPipeline,
    mono_sprites: wgpu::RenderPipeline,
    subpixel_sprites: Option<wgpu::RenderPipeline>,
    poly_sprites: wgpu::RenderPipeline,
    #[allow(dead_code)]
    surfaces: wgpu::RenderPipeline,
    /// Copies a source texture into the (smaller) target with one bilinear tap. Used both to
    /// downsample the scene into the half-resolution blur texture and to blit the offscreen
    /// scene into the swapchain at the end of the frame.
    blur_downsample: wgpu::RenderPipeline,
    /// One axis of a separable gaussian blur; direction is supplied per draw via [`BlurParams`].
    blur: wgpu::RenderPipeline,
    /// Composites a blurred texture into a rounded rectangle (with clip + opacity).
    blur_composite: wgpu::RenderPipeline,
}

struct WgpuBindGroupLayouts {
    globals: wgpu::BindGroupLayout,
    instances: wgpu::BindGroupLayout,
    instances_with_texture: wgpu::BindGroupLayout,
    surfaces: wgpu::BindGroupLayout,
    blur: wgpu::BindGroupLayout,
}

/// Shared GPU context and recovery state used by all windows on one platform client.
#[derive(Clone, Default)]
pub struct GpuContext {
    shared: Rc<RefCell<SharedGpuContext>>,
}

impl GpuContext {
    /// Creates an empty shared context that will be initialized by the first window.
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrows the active context for platform capability queries.
    pub fn borrow(&self) -> Ref<'_, Option<WgpuContext>> {
        Ref::map(self.shared.borrow(), |shared| &shared.context)
    }
}

#[derive(Default)]
struct SharedGpuContext {
    context: Option<WgpuContext>,
    #[cfg(not(target_family = "wasm"))]
    recovery: SharedRecoveryState,
}

#[cfg(not(target_family = "wasm"))]
#[derive(Default)]
struct SharedRecoveryState {
    rebuilding: bool,
    backoff: RecoveryBackoff,
    exhaustion_reported: bool,
}

#[cfg(not(target_family = "wasm"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecoveryDeferral {
    Rebuilding,
    CoolingDown(Duration),
    Exhausted { report: bool },
}

#[cfg(not(target_family = "wasm"))]
impl From<RecoveryDeferral> for WgpuRecoveryStatus {
    fn from(deferral: RecoveryDeferral) -> Self {
        match deferral {
            RecoveryDeferral::Exhausted { report: true } => Self::Failed,
            RecoveryDeferral::Rebuilding
            | RecoveryDeferral::CoolingDown(_)
            | RecoveryDeferral::Exhausted { report: false } => Self::Deferred,
        }
    }
}

#[cfg(not(target_family = "wasm"))]
impl SharedRecoveryState {
    fn begin_rebuild(&mut self, now: Instant) -> Result<(), RecoveryDeferral> {
        if self.rebuilding {
            return Err(RecoveryDeferral::Rebuilding);
        }
        if self.backoff.is_exhausted() {
            let report = !std::mem::replace(&mut self.exhaustion_reported, true);
            return Err(RecoveryDeferral::Exhausted { report });
        }
        if !self.backoff.can_attempt(now) {
            let delay = self
                .backoff
                .retry_delay(now)
                .expect("unavailable recovery attempt has a cooldown");
            return Err(RecoveryDeferral::CoolingDown(delay));
        }

        self.rebuilding = true;
        Ok(())
    }

    fn record_failure(&mut self, now: Instant) {
        self.rebuilding = false;
        self.backoff.record_failure(now);
    }

    fn record_success(&mut self) {
        self.rebuilding = false;
        self.backoff.reset();
        self.exhaustion_reported = false;
    }
}

#[cfg(not(target_family = "wasm"))]
const INITIAL_RECOVERY_DELAY: Duration = Duration::from_millis(100);
#[cfg(not(target_family = "wasm"))]
const MAX_RECOVERY_DELAY: Duration = Duration::from_secs(5);
#[cfg(not(target_family = "wasm"))]
const MAX_RECOVERY_BACKOFF_SHIFT: u32 = 6;
#[cfg(not(target_family = "wasm"))]
const MAX_RECOVERY_FAILURES: u32 = 12;

/// Bounds recovery attempts without ever sleeping on the render thread.
#[cfg(not(target_family = "wasm"))]
#[derive(Clone, Copy, Debug, Default)]
struct RecoveryBackoff {
    consecutive_failures: u32,
    retry_not_before: Option<Instant>,
}

#[cfg(not(target_family = "wasm"))]
impl RecoveryBackoff {
    fn retry_delay(self, now: Instant) -> Option<Duration> {
        self.retry_not_before
            .and_then(|deadline| deadline.checked_duration_since(now))
            .filter(|delay| !delay.is_zero())
    }

    fn can_attempt(self, now: Instant) -> bool {
        !self.is_exhausted() && self.retry_delay(now).is_none()
    }

    fn is_exhausted(self) -> bool {
        self.consecutive_failures >= MAX_RECOVERY_FAILURES
    }

    fn record_failure(&mut self, now: Instant) {
        let shift = self.consecutive_failures.min(MAX_RECOVERY_BACKOFF_SHIFT);
        let multiplier = 1_u32 << shift;
        let delay = INITIAL_RECOVERY_DELAY
            .saturating_mul(multiplier)
            .min(MAX_RECOVERY_DELAY);
        self.consecutive_failures = self
            .consecutive_failures
            .saturating_add(1)
            .min(MAX_RECOVERY_FAILURES);
        self.retry_not_before = now.checked_add(delay);
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

/// GPU resources that must be dropped together during device recovery.
struct WgpuResources {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: wgpu::Surface<'static>,
    pipelines: WgpuPipelines,
    bind_group_layouts: WgpuBindGroupLayouts,
    atlas_sampler: wgpu::Sampler,
    surface_sampler: wgpu::Sampler,
    #[allow(dead_code)]
    surface_uniform_buffer: wgpu::Buffer,
    /// One reused uniform buffer holding [`BlurParams`] for every blur pass in a frame, each at a
    /// distinct (alignment-strided) offset. Avoids allocating a buffer per pass; distinct offsets
    /// mean `write_buffer`'s last-write-at-submit semantics don't clobber earlier passes.
    blur_params_buffer: wgpu::Buffer,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    path_globals_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    path_intermediate_texture: Option<wgpu::Texture>,
    path_intermediate_view: Option<wgpu::TextureView>,
    path_msaa_texture: Option<wgpu::Texture>,
    path_msaa_view: Option<wgpu::TextureView>,
    /// Blur offscreen targets. Allocated lazily (only when a frame actually uses a blur filter)
    /// so apps that never blur pay no extra VRAM. `None`/empty until first use.
    ///
    /// Full-resolution offscreen color target the scene is rendered into so that blur passes
    /// can sample already-painted content; blitted to the swapchain at the end of the frame.
    scene_color_texture: Option<wgpu::Texture>,
    scene_color_view: Option<wgpu::TextureView>,
    /// Half-resolution ping/pong targets for the downsample + separable gaussian passes.
    blur_ping_texture: Option<wgpu::Texture>,
    blur_ping_view: Option<wgpu::TextureView>,
    blur_pong_texture: Option<wgpu::Texture>,
    blur_pong_view: Option<wgpu::TextureView>,
    /// Full-resolution offscreen targets a content-filter (`filter`) group renders into before
    /// being blurred and composited back. One per nesting level (indexed by depth) so nested
    /// content blurs isolate correctly, up to [`MAX_FILTER_DEPTH`]; deeper nests render inline.
    group_textures: Vec<wgpu::Texture>,
    group_views: Vec<wgpu::TextureView>,
}

impl WgpuResources {
    fn invalidate_intermediate_textures(&mut self) {
        self.path_intermediate_texture = None;
        self.path_intermediate_view = None;
        self.path_msaa_texture = None;
        self.path_msaa_view = None;
        self.scene_color_texture = None;
        self.scene_color_view = None;
        self.blur_ping_texture = None;
        self.blur_ping_view = None;
        self.blur_pong_texture = None;
        self.blur_pong_view = None;
        self.group_textures.clear();
        self.group_views.clear();
    }
}

/// Number of content-filter (`filter`) nesting levels that get their own isolated group texture.
/// Two covers the realistic "a blurred element inside another blurred element" case; deeper nests
/// render inline (unblurred at the inner level) rather than allocating unbounded VRAM.
const MAX_FILTER_DEPTH: usize = 2;

/// Number of [`BlurParams`] slots in the shared blur-params buffer (one per blur pass per frame).
/// Each frame uses 4 passes per backdrop/group plus one blit; 256 covers dozens of filters.
const BLUR_PARAMS_SLOTS: u64 = 256;

pub struct WgpuRenderer {
    /// Shared GPU context for device recovery coordination (unused on WASM).
    #[allow(dead_code)]
    context: Option<GpuContext>,
    /// Compositor GPU hint for adapter selection (unused on WASM).
    #[allow(dead_code)]
    compositor_gpu: Option<CompositorGpuHint>,
    /// Application-requested extra wgpu features/limits, stored for device recovery.
    #[allow(dead_code)]
    extra_requirements: Option<WgpuDeviceRequirements>,
    resources: Option<WgpuResources>,
    surface_config: wgpu::SurfaceConfiguration,
    atlas: Arc<WgpuAtlas>,
    path_globals_offset: u64,
    gamma_offset: u64,
    /// Reused, fully initialized Pod vectors for scene-to-GPU instance conversion.
    instance_scratch: RefCell<GpuInstanceScratch>,
    instance_buffer_capacity: u64,
    max_buffer_size: u64,
    storage_buffer_alignment: u64,
    /// Stride between [`BlurParams`] slots in `blur_params_buffer`, and a per-frame bump cursor
    /// (in slots) handed out to blur passes. Cell so the `&self` blur helpers can advance it.
    blur_params_stride: u64,
    blur_params_slot: std::cell::Cell<u64>,
    rendering_params: RenderingParameters,
    is_bgr: bool,
    dual_source_blending: bool,
    adapter_info: wgpu::AdapterInfo,
    transparent_alpha_mode: wgpu::CompositeAlphaMode,
    opaque_alpha_mode: wgpu::CompositeAlphaMode,
    max_texture_size: u32,
    last_error: Arc<Mutex<Option<String>>>,
    failed_frame_count: u32,
    device_lost: std::sync::Arc<std::sync::atomic::AtomicBool>,
    surface_configured: bool,
    needs_redraw: bool,
    #[cfg(not(target_family = "wasm"))]
    recovery_backoff: RecoveryBackoff,
    #[cfg(not(target_family = "wasm"))]
    recovery_exhaustion_reported: bool,
}

impl WgpuRenderer {
    fn resources(&self) -> &WgpuResources {
        self.resources
            .as_ref()
            .expect("GPU resources not available")
    }

    fn resources_mut(&mut self) -> &mut WgpuResources {
        self.resources
            .as_mut()
            .expect("GPU resources not available")
    }

    /// Creates a new WgpuRenderer from raw window handles.
    ///
    /// The `gpu_context` is a shared reference that coordinates GPU context across
    /// multiple windows. The first window to create a renderer will initialize the
    /// context; subsequent windows will share it.
    ///
    /// # Safety
    /// The caller must ensure that the window handle remains valid for the lifetime
    /// of the returned renderer.
    #[cfg(not(target_family = "wasm"))]
    pub fn new<W>(
        gpu_context: GpuContext,
        window: &W,
        config: WgpuSurfaceConfig,
        compositor_gpu: Option<CompositorGpuHint>,
        extra_requirements: Option<WgpuDeviceRequirements>,
    ) -> anyhow::Result<Self>
    where
        W: HasWindowHandle + HasDisplayHandle + std::fmt::Debug + Send + Sync + Clone + 'static,
    {
        let window_handle = window
            .window_handle()
            .map_err(|e| anyhow::anyhow!("Failed to get window handle: {e}"))?;

        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            // Fall back to the display handle already provided via InstanceDescriptor::display.
            raw_display_handle: None,
            raw_window_handle: window_handle.as_raw(),
        };

        // Use the existing context's instance if available, otherwise create a new one.
        // The surface must be created with the same instance that will be used for
        // adapter selection, otherwise wgpu will panic.
        let instance = gpu_context
            .shared
            .borrow()
            .context
            .as_ref()
            .map(|ctx| ctx.instance.clone())
            .unwrap_or_else(|| WgpuContext::instance(Box::new(window.clone())));

        // Safety: The caller guarantees that the window handle is valid for the
        // lifetime of this renderer. In practice, the RawWindow struct is created
        // from the native window handles and the surface is dropped before the window.
        let surface = unsafe {
            instance
                .create_surface_unsafe(target)
                .map_err(|e| anyhow::anyhow!("Failed to create surface: {e}"))?
        };

        let mut shared_context = gpu_context.shared.borrow_mut();
        let context = match shared_context.context.as_mut() {
            Some(context) => {
                context.check_compatible_with_surface(&surface)?;
                context
            }
            None => shared_context.context.insert(WgpuContext::new(
                instance,
                &surface,
                compositor_gpu,
                extra_requirements.as_ref(),
            )?),
        };

        let atlas = Arc::new(WgpuAtlas::from_context(context));

        Self::new_internal(
            Some(gpu_context.clone()),
            context,
            surface,
            config,
            compositor_gpu,
            extra_requirements,
            atlas,
        )
    }

    #[cfg(target_family = "wasm")]
    pub fn new_from_canvas(
        context: &WgpuContext,
        canvas: &web_sys::HtmlCanvasElement,
        config: WgpuSurfaceConfig,
    ) -> anyhow::Result<Self> {
        let surface = context
            .instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|e| anyhow::anyhow!("Failed to create surface: {e}"))?;

        let atlas = Arc::new(WgpuAtlas::from_context(context));

        Self::new_internal(None, context, surface, config, None, None, atlas)
    }

    fn new_internal(
        gpu_context: Option<GpuContext>,
        context: &WgpuContext,
        surface: wgpu::Surface<'static>,
        config: WgpuSurfaceConfig,
        compositor_gpu: Option<CompositorGpuHint>,
        extra_requirements: Option<WgpuDeviceRequirements>,
        atlas: Arc<WgpuAtlas>,
    ) -> anyhow::Result<Self> {
        let surface_caps = surface.get_capabilities(&context.adapter);
        let preferred_formats = [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Rgba8Unorm,
        ];
        let surface_format = preferred_formats
            .iter()
            .find(|f| surface_caps.formats.contains(f))
            .copied()
            .or_else(|| surface_caps.formats.iter().find(|f| !f.is_srgb()).copied())
            .or_else(|| surface_caps.formats.first().copied())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Surface reports no supported texture formats for adapter {:?}",
                    context.adapter.get_info().name
                )
            })?;

        let pick_alpha_mode =
            |preferences: &[wgpu::CompositeAlphaMode]| -> anyhow::Result<wgpu::CompositeAlphaMode> {
                preferences
                    .iter()
                    .find(|p| surface_caps.alpha_modes.contains(p))
                    .copied()
                    .or_else(|| surface_caps.alpha_modes.first().copied())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Surface reports no supported alpha modes for adapter {:?}",
                            context.adapter.get_info().name
                        )
                    })
            };

        let transparent_alpha_mode = pick_alpha_mode(&[
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::Inherit,
        ])?;

        let opaque_alpha_mode = pick_alpha_mode(&[
            wgpu::CompositeAlphaMode::Opaque,
            wgpu::CompositeAlphaMode::Inherit,
        ])?;

        let alpha_mode = if config.transparent {
            transparent_alpha_mode
        } else {
            opaque_alpha_mode
        };

        let device = Arc::clone(&context.device);
        let max_texture_size = device.limits().max_texture_dimension_2d;

        let requested_width = config.size.width.0 as u32;
        let requested_height = config.size.height.0 as u32;
        let clamped_width = requested_width.min(max_texture_size);
        let clamped_height = requested_height.min(max_texture_size);

        if clamped_width != requested_width || clamped_height != requested_height {
            warn!(
                "Requested surface size ({}, {}) exceeds maximum texture dimension {}. \
                 Clamping to ({}, {}). Window content may not fill the entire window.",
                requested_width, requested_height, max_texture_size, clamped_width, clamped_height
            );
        }

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: clamped_width.max(1),
            height: clamped_height.max(1),
            present_mode: config
                .preferred_present_mode
                .filter(|mode| surface_caps.present_modes.contains(mode))
                .unwrap_or(wgpu::PresentMode::Fifo),
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        // Configure the surface immediately. The adapter selection process already validated
        // that this adapter can successfully configure this surface.
        surface.configure(&context.device, &surface_config);

        let queue = Arc::clone(&context.queue);
        let dual_source_blending = context.supports_dual_source_blending();

        let rendering_params = RenderingParameters::new(&context.adapter, surface_format);
        let bind_group_layouts = Self::create_bind_group_layouts(&device);
        let pipelines = Self::create_pipelines(
            &device,
            &bind_group_layouts,
            surface_format,
            alpha_mode,
            rendering_params.path_sample_count,
            dual_source_blending,
        );

        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let surface_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("surface_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let surface_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("surface_uniform_buffer"),
            size: std::mem::size_of::<SurfaceParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_alignment = device.limits().min_uniform_buffer_offset_alignment as u64;
        // Shared blur-params buffer: BLUR_PARAMS_SLOTS slots, each one alignment stride apart.
        let blur_params_stride =
            (std::mem::size_of::<BlurParams>() as u64).next_multiple_of(uniform_alignment);
        let blur_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blur_params_buffer"),
            size: blur_params_stride * BLUR_PARAMS_SLOTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let globals_size = std::mem::size_of::<GlobalParams>() as u64;
        let gamma_size = std::mem::size_of::<GammaParams>() as u64;
        let path_globals_offset = globals_size.next_multiple_of(uniform_alignment);
        let gamma_offset = (path_globals_offset + globals_size).next_multiple_of(uniform_alignment);

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals_buffer"),
            size: gamma_offset + gamma_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let max_buffer_size = device.limits().max_buffer_size;
        let storage_buffer_alignment = device.limits().min_storage_buffer_offset_alignment as u64;
        let initial_instance_buffer_capacity = 2 * 1024 * 1024;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance_buffer"),
            size: initial_instance_buffer_capacity,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals_bind_group"),
            layout: &bind_group_layouts.globals,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &globals_buffer,
                        offset: 0,
                        size: Some(NonZeroU64::new(globals_size).unwrap()),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &globals_buffer,
                        offset: gamma_offset,
                        size: Some(NonZeroU64::new(gamma_size).unwrap()),
                    }),
                },
            ],
        });

        let path_globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("path_globals_bind_group"),
            layout: &bind_group_layouts.globals,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &globals_buffer,
                        offset: path_globals_offset,
                        size: Some(NonZeroU64::new(globals_size).unwrap()),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &globals_buffer,
                        offset: gamma_offset,
                        size: Some(NonZeroU64::new(gamma_size).unwrap()),
                    }),
                },
            ],
        });

        let adapter_info = context.adapter.get_info();

        let last_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let last_error_clone = Arc::clone(&last_error);
        device.on_uncaptured_error(Arc::new(move |error| {
            let mut guard = last_error_clone.lock().unwrap();
            *guard = Some(error.to_string());
        }));

        let resources = WgpuResources {
            device,
            queue,
            surface,
            pipelines,
            bind_group_layouts,
            atlas_sampler,
            surface_sampler,
            surface_uniform_buffer,
            blur_params_buffer,
            globals_buffer,
            globals_bind_group,
            path_globals_bind_group,
            instance_buffer,
            // Defer intermediate texture creation to first draw call via ensure_intermediate_textures().
            // This avoids panics when the device/surface is in an invalid state during initialization.
            path_intermediate_texture: None,
            path_intermediate_view: None,
            path_msaa_texture: None,
            path_msaa_view: None,
            scene_color_texture: None,
            scene_color_view: None,
            blur_ping_texture: None,
            blur_ping_view: None,
            blur_pong_texture: None,
            blur_pong_view: None,
            group_textures: Vec::new(),
            group_views: Vec::new(),
        };

        Ok(Self {
            context: gpu_context,
            compositor_gpu,
            extra_requirements,
            resources: Some(resources),
            surface_config,
            atlas,
            path_globals_offset,
            gamma_offset,
            instance_scratch: RefCell::new(GpuInstanceScratch::default()),
            instance_buffer_capacity: initial_instance_buffer_capacity,
            max_buffer_size,
            storage_buffer_alignment,
            blur_params_stride,
            blur_params_slot: std::cell::Cell::new(0),
            rendering_params,
            is_bgr: false,
            dual_source_blending,
            adapter_info,
            transparent_alpha_mode,
            opaque_alpha_mode,
            max_texture_size,
            last_error,
            failed_frame_count: 0,
            device_lost: context.device_lost_flag(),
            surface_configured: true,
            needs_redraw: false,
            #[cfg(not(target_family = "wasm"))]
            recovery_backoff: RecoveryBackoff::default(),
            #[cfg(not(target_family = "wasm"))]
            recovery_exhaustion_reported: false,
        })
    }

    fn create_bind_group_layouts(device: &wgpu::Device) -> WgpuBindGroupLayouts {
        let globals =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("globals_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(
                                std::mem::size_of::<GlobalParams>() as u64
                            ),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(
                                std::mem::size_of::<GammaParams>() as u64
                            ),
                        },
                        count: None,
                    },
                ],
            });

        let storage_buffer_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

        let instances = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("instances_layout"),
            entries: &[storage_buffer_entry(0)],
        });

        let instances_with_texture =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("instances_with_texture_layout"),
                entries: &[
                    storage_buffer_entry(0),
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let surfaces = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("surfaces_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(
                            std::mem::size_of::<SurfaceParams>() as u64
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let blur = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(std::mem::size_of::<BlurParams>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        WgpuBindGroupLayouts {
            globals,
            instances,
            instances_with_texture,
            surfaces,
            blur,
        }
    }

    fn create_pipelines(
        device: &wgpu::Device,
        layouts: &WgpuBindGroupLayouts,
        surface_format: wgpu::TextureFormat,
        alpha_mode: wgpu::CompositeAlphaMode,
        path_sample_count: u32,
        dual_source_blending: bool,
    ) -> WgpuPipelines {
        // Diagnostic guard: verify the device actually has
        // DUAL_SOURCE_BLENDING. We have a crash report (ZED-5G1) where a
        // feature mismatch caused a wgpu-hal abort, but we haven't
        // identified the code path that produces the mismatch. This
        // guard prevents the crash and logs more evidence.
        // Remove this check once:
        // a) We find and fix the root cause, or
        // b) There are no reports of this warning appearing for some time.
        let device_has_feature = device
            .features()
            .contains(wgpu::Features::DUAL_SOURCE_BLENDING);
        if dual_source_blending && !device_has_feature {
            log::error!(
                "BUG: dual_source_blending flag is true but device does not \
                 have DUAL_SOURCE_BLENDING enabled (device features: {:?}). \
                 Falling back to mono text rendering. Please report this at \
                 https://github.com/zed-industries/zed/issues",
                device.features(),
            );
        }
        let dual_source_blending = dual_source_blending && device_has_feature;

        let base_shader_source = include_str!("shaders.wgsl");
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gpui_shaders"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(base_shader_source)),
        });

        let subpixel_shader_source = include_str!("shaders_subpixel.wgsl");
        let subpixel_shader_module = if dual_source_blending {
            let combined = format!(
                "enable dual_source_blending;\n{base_shader_source}\n{subpixel_shader_source}"
            );
            Some(device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("gpui_subpixel_shaders"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Owned(combined)),
            }))
        } else {
            None
        };

        let blend_mode = match alpha_mode {
            wgpu::CompositeAlphaMode::PreMultiplied => {
                wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING
            }
            _ => wgpu::BlendState::ALPHA_BLENDING,
        };

        let color_target = wgpu::ColorTargetState {
            format: surface_format,
            blend: Some(blend_mode),
            write_mask: wgpu::ColorWrites::ALL,
        };

        let create_pipeline = |name: &str,
                               vs_entry: &str,
                               fs_entry: &str,
                               globals_layout: &wgpu::BindGroupLayout,
                               data_layout: &wgpu::BindGroupLayout,
                               topology: wgpu::PrimitiveTopology,
                               color_targets: &[Option<wgpu::ColorTargetState>],
                               sample_count: u32,
                               module: &wgpu::ShaderModule| {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(&format!("{name}_layout")),
                bind_group_layouts: &[Some(globals_layout), Some(data_layout)],
                immediate_size: 0,
            });

            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(name),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some(vs_entry),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(fs_entry),
                    targets: color_targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: sample_count,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview_mask: None,
                cache: None,
            })
        };

        let quads = create_pipeline(
            "quads",
            "vs_quad",
            "fs_quad",
            &layouts.globals,
            &layouts.instances,
            wgpu::PrimitiveTopology::TriangleStrip,
            &[Some(color_target.clone())],
            1,
            &shader_module,
        );

        let shadows = create_pipeline(
            "shadows",
            "vs_shadow",
            "fs_shadow",
            &layouts.globals,
            &layouts.instances,
            wgpu::PrimitiveTopology::TriangleStrip,
            &[Some(color_target.clone())],
            1,
            &shader_module,
        );

        let path_rasterization = create_pipeline(
            "path_rasterization",
            "vs_path_rasterization",
            "fs_path_rasterization",
            &layouts.globals,
            &layouts.instances,
            wgpu::PrimitiveTopology::TriangleList,
            &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            path_sample_count,
            &shader_module,
        );

        let paths_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let paths = create_pipeline(
            "paths",
            "vs_path",
            "fs_path",
            &layouts.globals,
            &layouts.instances_with_texture,
            wgpu::PrimitiveTopology::TriangleStrip,
            &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(paths_blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            1,
            &shader_module,
        );

        let underlines = create_pipeline(
            "underlines",
            "vs_underline",
            "fs_underline",
            &layouts.globals,
            &layouts.instances,
            wgpu::PrimitiveTopology::TriangleStrip,
            &[Some(color_target.clone())],
            1,
            &shader_module,
        );

        let mono_sprites = create_pipeline(
            "mono_sprites",
            "vs_mono_sprite",
            "fs_mono_sprite",
            &layouts.globals,
            &layouts.instances_with_texture,
            wgpu::PrimitiveTopology::TriangleStrip,
            &[Some(color_target.clone())],
            1,
            &shader_module,
        );

        let subpixel_sprites = if let Some(subpixel_module) = &subpixel_shader_module {
            let subpixel_blend = wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Src1,
                    dst_factor: wgpu::BlendFactor::OneMinusSrc1,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
            };

            Some(create_pipeline(
                "subpixel_sprites",
                "vs_subpixel_sprite",
                "fs_subpixel_sprite",
                &layouts.globals,
                &layouts.instances_with_texture,
                wgpu::PrimitiveTopology::TriangleStrip,
                &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(subpixel_blend),
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
                1,
                subpixel_module,
            ))
        } else {
            None
        };

        let poly_sprites = create_pipeline(
            "poly_sprites",
            "vs_poly_sprite",
            "fs_poly_sprite",
            &layouts.globals,
            &layouts.instances_with_texture,
            wgpu::PrimitiveTopology::TriangleStrip,
            &[Some(color_target.clone())],
            1,
            &shader_module,
        );

        let surfaces = create_pipeline(
            "surfaces",
            "vs_surface",
            "fs_surface",
            &layouts.globals,
            &layouts.surfaces,
            wgpu::PrimitiveTopology::TriangleStrip,
            &[Some(color_target)],
            1,
            &shader_module,
        );

        // Blur pipelines all sample one texture into another; the downsample and gaussian passes
        // overwrite their (intermediate) target, while the composite blends over the scene.
        let no_blend_target = wgpu::ColorTargetState {
            format: surface_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        };

        let blur_downsample = create_pipeline(
            "blur_downsample",
            "vs_blur_fullscreen",
            "fs_blur_downsample",
            &layouts.globals,
            &layouts.blur,
            wgpu::PrimitiveTopology::TriangleList,
            &[Some(no_blend_target.clone())],
            1,
            &shader_module,
        );

        let blur = create_pipeline(
            "blur",
            "vs_blur_fullscreen",
            "fs_blur",
            &layouts.globals,
            &layouts.blur,
            wgpu::PrimitiveTopology::TriangleList,
            &[Some(no_blend_target)],
            1,
            &shader_module,
        );

        // The blurred sample is premultiplied (blurring against the transparent, rgb=0 region
        // around the source scales rgb with the fading alpha), so the composite outputs
        // premultiplied and blends premultiplied — straight alpha blending would multiply rgb by
        // alpha a second time and darken the faded edges. Independent of the window's alpha mode.
        let premultiplied_target = wgpu::ColorTargetState {
            format: surface_format,
            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        };
        let blur_composite = create_pipeline(
            "blur_composite",
            "vs_blur_composite",
            "fs_blur_composite",
            &layouts.globals,
            &layouts.blur,
            wgpu::PrimitiveTopology::TriangleStrip,
            &[Some(premultiplied_target)],
            1,
            &shader_module,
        );

        WgpuPipelines {
            quads,
            shadows,
            path_rasterization,
            paths,
            underlines,
            mono_sprites,
            subpixel_sprites,
            poly_sprites,
            surfaces,
            blur_downsample,
            blur,
            blur_composite,
        }
    }

    fn create_path_intermediate(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("path_intermediate"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    fn create_msaa_if_needed(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        sample_count: u32,
    ) -> Option<(wgpu::Texture, wgpu::TextureView)> {
        if sample_count <= 1 {
            return None;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("path_msaa"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Some((texture, view))
    }

    pub fn update_drawable_size(&mut self, size: Size<DevicePixels>) {
        let width = size.width.0 as u32;
        let height = size.height.0 as u32;

        if width != self.surface_config.width || height != self.surface_config.height {
            let clamped_width = width.min(self.max_texture_size);
            let clamped_height = height.min(self.max_texture_size);

            if clamped_width != width || clamped_height != height {
                warn!(
                    "Requested surface size ({}, {}) exceeds maximum texture dimension {}. \
                     Clamping to ({}, {}). Window content may not fill the entire window.",
                    width, height, self.max_texture_size, clamped_width, clamped_height
                );
            }

            self.surface_config.width = clamped_width.max(1);
            self.surface_config.height = clamped_height.max(1);
            let surface_config = self.surface_config.clone();

            // GPU resources may not exist yet, skip rather than panicking
            let Some(resources) = self.resources.as_mut() else {
                return;
            };

            // Wait for any in-flight GPU work to complete before destroying textures
            if let Err(e) = resources.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            }) {
                warn!("Failed to poll device during resize: {e:?}");
            }

            // Destroy old textures before allocating new ones to avoid GPU memory spikes
            if let Some(ref texture) = resources.path_intermediate_texture {
                texture.destroy();
            }
            if let Some(ref texture) = resources.path_msaa_texture {
                texture.destroy();
            }
            for texture in [
                &resources.scene_color_texture,
                &resources.blur_ping_texture,
                &resources.blur_pong_texture,
            ]
            .into_iter()
            .flatten()
            {
                texture.destroy();
            }
            for texture in &resources.group_textures {
                texture.destroy();
            }

            resources
                .surface
                .configure(&resources.device, &surface_config);

            // Invalidate intermediate textures - they will be lazily recreated
            // in draw() after we confirm the surface is healthy. This avoids
            // panics when the device/surface is in an invalid state during resize.
            resources.invalidate_intermediate_textures();
        }
    }

    fn ensure_intermediate_textures(&mut self) {
        if self.resources().path_intermediate_texture.is_some() {
            return;
        }

        let format = self.surface_config.format;
        let width = self.surface_config.width;
        let height = self.surface_config.height;
        let path_sample_count = self.rendering_params.path_sample_count;
        let resources = self.resources_mut();

        let (t, v) = Self::create_path_intermediate(&resources.device, format, width, height);
        resources.path_intermediate_texture = Some(t);
        resources.path_intermediate_view = Some(v);

        let (path_msaa_texture, path_msaa_view) = Self::create_msaa_if_needed(
            &resources.device,
            format,
            width,
            height,
            path_sample_count,
        )
        .map(|(t, v)| (Some(t), Some(v)))
        .unwrap_or((None, None));
        resources.path_msaa_texture = path_msaa_texture;
        resources.path_msaa_view = path_msaa_view;
    }

    /// Lazily allocate the blur offscreen targets — the full-res scene texture, half-res
    /// ping/pong, and one full-res group texture per nesting level. Called only on frames that
    /// actually use a blur filter, so non-blurring apps never pay this VRAM. A no-op once
    /// allocated (invalidated alongside the path intermediates on resize / device loss).
    fn ensure_blur_textures(&mut self) {
        if self.resources().scene_color_texture.is_some() {
            return;
        }
        let format = self.surface_config.format;
        let width = self.surface_config.width;
        let height = self.surface_config.height;
        let blur_width = (width / 2).max(1);
        let blur_height = (height / 2).max(1);
        let resources = self.resources_mut();

        let (t, v) = Self::create_path_intermediate(&resources.device, format, width, height);
        resources.scene_color_texture = Some(t);
        resources.scene_color_view = Some(v);
        let (t, v) =
            Self::create_path_intermediate(&resources.device, format, blur_width, blur_height);
        resources.blur_ping_texture = Some(t);
        resources.blur_ping_view = Some(v);
        let (t, v) =
            Self::create_path_intermediate(&resources.device, format, blur_width, blur_height);
        resources.blur_pong_texture = Some(t);
        resources.blur_pong_view = Some(v);

        for _ in 0..MAX_FILTER_DEPTH {
            let (t, v) = Self::create_path_intermediate(&resources.device, format, width, height);
            resources.group_textures.push(t);
            resources.group_views.push(v);
        }
    }

    pub fn set_subpixel_layout(&mut self, is_bgr: bool) {
        self.is_bgr = is_bgr;
    }

    pub fn update_transparency(&mut self, transparent: bool) {
        let new_alpha_mode = if transparent {
            self.transparent_alpha_mode
        } else {
            self.opaque_alpha_mode
        };

        if new_alpha_mode != self.surface_config.alpha_mode {
            self.surface_config.alpha_mode = new_alpha_mode;
            let surface_config = self.surface_config.clone();
            let path_sample_count = self.rendering_params.path_sample_count;
            let dual_source_blending = self.dual_source_blending;
            let resources = self.resources_mut();
            resources
                .surface
                .configure(&resources.device, &surface_config);
            resources.pipelines = Self::create_pipelines(
                &resources.device,
                &resources.bind_group_layouts,
                surface_config.format,
                surface_config.alpha_mode,
                path_sample_count,
                dual_source_blending,
            );
        }
    }

    #[allow(dead_code)]
    pub fn viewport_size(&self) -> Size<DevicePixels> {
        Size {
            width: DevicePixels(self.surface_config.width as i32),
            height: DevicePixels(self.surface_config.height as i32),
        }
    }

    pub fn sprite_atlas(&self) -> &Arc<WgpuAtlas> {
        &self.atlas
    }

    pub fn supports_dual_source_blending(&self) -> bool {
        self.dual_source_blending
    }

    pub fn gpu_context(&self) -> (Arc<wgpu::Device>, Arc<wgpu::Queue>) {
        let resources = self.resources();
        (resources.device.clone(), resources.queue.clone())
    }

    pub fn gpu_specs(&self) -> GpuSpecs {
        GpuSpecs {
            is_software_emulated: self.adapter_info.device_type == wgpu::DeviceType::Cpu,
            is_virtual_gpu: self.adapter_info.device_type == wgpu::DeviceType::VirtualGpu,
            device_name: self.adapter_info.name.clone(),
            driver_name: self.adapter_info.driver.clone(),
            driver_info: self.adapter_info.driver_info.clone(),
        }
    }

    pub fn max_texture_size(&self) -> u32 {
        self.max_texture_size
    }

    pub fn draw(&mut self, scene: &Scene) -> bool {
        // Bail out early if the surface has been unconfigured (e.g. during
        // Android background/rotation transitions).  Attempting to acquire
        // a texture from an unconfigured surface can block indefinitely on
        // some drivers (Adreno).
        if !self.surface_configured {
            return false;
        }

        let last_error = self.last_error.lock().unwrap().take();
        if let Some(error) = last_error {
            self.failed_frame_count += 1;
            log::error!(
                "GPU error during frame (failure {} of 10): {error}",
                self.failed_frame_count
            );

            if self.failed_frame_count > 5 {
                // Recoverable validation errors must not terminate the application. Clear only
                // cached resources here; the device-loss callback owns full device recovery.
                if let Some(res) = self.resources.as_mut() {
                    res.invalidate_intermediate_textures();
                }
                self.atlas.clear();
                self.needs_redraw = true;
                self.failed_frame_count = 0;
                return false;
            }
        } else {
            self.failed_frame_count = 0;
        }

        self.atlas.before_frame();

        let frame = match self.resources().surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                // Textures must be destroyed before the surface can be reconfigured.
                drop(frame);
                let surface_config = self.surface_config.clone();
                let resources = self.resources_mut();
                resources
                    .surface
                    .configure(&resources.device, &surface_config);
                return false;
            }
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                let surface_config = self.surface_config.clone();
                let resources = self.resources_mut();
                resources
                    .surface
                    .configure(&resources.device, &surface_config);
                return false;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return false;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                *self.last_error.lock().unwrap() =
                    Some("Surface texture validation error".to_string());
                return false;
            }
        };

        // Now that we know the surface is healthy, ensure intermediate textures exist
        self.ensure_intermediate_textures();

        // Blur is the only thing that needs the offscreen scene texture; allocate it (and the
        // ping/pong/group targets) lazily so non-blurring apps pay no extra VRAM or blit.
        let use_offscreen =
            !scene.backdrop_filters.is_empty() || !scene.filter_boundaries.is_empty();
        if use_offscreen {
            self.ensure_blur_textures();
        }

        let frame_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let gamma_params = GammaParams {
            gamma_ratios: self.rendering_params.gamma_ratios,
            grayscale_enhanced_contrast: self.rendering_params.grayscale_enhanced_contrast,
            subpixel_enhanced_contrast: self.rendering_params.subpixel_enhanced_contrast,
            is_bgr: self.is_bgr as u32,
            pad: 0,
        };

        let globals = GlobalParams {
            viewport_size: [
                self.surface_config.width as f32,
                self.surface_config.height as f32,
            ],
            premultiplied_alpha: if self.surface_config.alpha_mode
                == wgpu::CompositeAlphaMode::PreMultiplied
            {
                1
            } else {
                0
            },
            pad: 0,
        };

        let path_globals = GlobalParams {
            premultiplied_alpha: 0,
            ..globals
        };

        {
            let resources = self.resources();
            resources.queue.write_buffer(
                &resources.globals_buffer,
                0,
                bytemuck::bytes_of(&globals),
            );
            resources.queue.write_buffer(
                &resources.globals_buffer,
                self.path_globals_offset,
                bytemuck::bytes_of(&path_globals),
            );
            resources.queue.write_buffer(
                &resources.globals_buffer,
                self.gamma_offset,
                bytemuck::bytes_of(&gamma_params),
            );
        }

        loop {
            let mut instance_offset: u64 = 0;
            // Reset the blur-params bump cursor each (re)render of the scene.
            self.blur_params_slot.set(0);
            let mut overflow = false;

            let mut encoder =
                self.resources()
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("main_encoder"),
                    });

            // When the scene contains blur filters, render into the offscreen scene texture (so
            // filters can sample already-painted content mid-frame) and blit to the swapchain at
            // the end; otherwise render straight to the swapchain. `use_offscreen` and the blur
            // textures were computed/allocated above.
            let scene_color_view = if use_offscreen {
                Some(
                    self.resources()
                        .scene_color_view
                        .as_ref()
                        .expect("scene_color_view allocated by ensure_blur_textures")
                        .clone(),
                )
            } else {
                None
            };
            // The active render target. While inside a content-filter (`filter`) group it points
            // at a group texture so the group renders in isolation.
            let mut current_target = match &scene_color_view {
                Some(view) => view.clone(),
                None => frame_view.clone(),
            };
            // One group texture per nesting depth; empty when not blurring.
            let group_views = if use_offscreen {
                self.resources().group_views.clone()
            } else {
                Vec::new()
            };
            // (boundary, parent target to composite back into, whether this level is isolated).
            let mut filter_stack: Vec<(FilterBoundary, wgpu::TextureView, bool)> = Vec::new();

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("main_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &current_target,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    ..Default::default()
                });

                for batch in scene.batches() {
                    let ok = match batch {
                        PrimitiveBatch::Quads(range) => {
                            self.draw_quads(&scene.quads[range], &mut instance_offset, &mut pass)
                        }
                        PrimitiveBatch::Shadows(range) => self.draw_shadows(
                            &scene.shadows[range],
                            &mut instance_offset,
                            &mut pass,
                        ),
                        PrimitiveBatch::Paths(range) => {
                            let paths = &scene.paths[range];
                            if paths.is_empty() {
                                continue;
                            }

                            drop(pass);

                            let did_draw = self.draw_paths_to_intermediate(
                                &mut encoder,
                                paths,
                                &mut instance_offset,
                            );

                            pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("main_pass_continued"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &current_target,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                    depth_slice: None,
                                })],
                                depth_stencil_attachment: None,
                                ..Default::default()
                            });

                            if did_draw {
                                self.draw_paths_from_intermediate(
                                    paths,
                                    &mut instance_offset,
                                    &mut pass,
                                )
                            } else {
                                false
                            }
                        }
                        PrimitiveBatch::Underlines(range) => self.draw_underlines(
                            &scene.underlines[range],
                            &mut instance_offset,
                            &mut pass,
                        ),
                        PrimitiveBatch::MonochromeSprites { texture_id, range } => self
                            .draw_monochrome_sprites(
                                &scene.monochrome_sprites[range],
                                texture_id,
                                &mut instance_offset,
                                &mut pass,
                            ),
                        PrimitiveBatch::SubpixelSprites { texture_id, range } => self
                            .draw_subpixel_sprites(
                                &scene.subpixel_sprites[range],
                                texture_id,
                                &mut instance_offset,
                                &mut pass,
                            ),
                        PrimitiveBatch::PolychromeSprites { texture_id, range } => self
                            .draw_polychrome_sprites(
                                &scene.polychrome_sprites[range],
                                texture_id,
                                &mut instance_offset,
                                &mut pass,
                            ),
                        PrimitiveBatch::Surfaces(range) => {
                            self.draw_surfaces(&scene.surfaces[range], &mut pass)
                        }
                        PrimitiveBatch::BackdropFilters(range) => {
                            // Interrupt the current pass, blur the content painted so far behind
                            // each backdrop's rounded rect, then resume drawing on top.
                            drop(pass);
                            for filter in &scene.backdrop_filters[range] {
                                self.draw_backdrop_filter(&mut encoder, filter, &current_target);
                            }
                            pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("main_pass_continued"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &current_target,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                    depth_slice: None,
                                })],
                                depth_stencil_attachment: None,
                                ..Default::default()
                            });
                            true
                        }
                        PrimitiveBatch::FilterBoundary(ix) => {
                            let boundary = scene.filter_boundaries[ix].clone();
                            if boundary.is_start {
                                // Each isolated nesting level uses its own group texture from the
                                // pool (indexed by current isolation depth). Beyond the pool size
                                // (MAX_FILTER_DEPTH) deeper filters render inline without isolation
                                // rather than corrupting an outer group.
                                let depth = filter_stack.iter().filter(|entry| entry.2).count();
                                if depth < group_views.len() {
                                    drop(pass);
                                    let parent = current_target.clone();
                                    current_target = group_views[depth].clone();
                                    filter_stack.push((boundary, parent, true));
                                    pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                        label: Some("filter_group"),
                                        color_attachments: &[Some(
                                            wgpu::RenderPassColorAttachment {
                                                view: &current_target,
                                                resolve_target: None,
                                                ops: wgpu::Operations {
                                                    load: wgpu::LoadOp::Clear(
                                                        wgpu::Color::TRANSPARENT,
                                                    ),
                                                    store: wgpu::StoreOp::Store,
                                                },
                                                depth_slice: None,
                                            },
                                        )],
                                        depth_stencil_attachment: None,
                                        ..Default::default()
                                    });
                                } else {
                                    filter_stack.push((boundary, current_target.clone(), false));
                                }
                            } else if let Some((boundary, parent, isolated)) = filter_stack.pop() {
                                if isolated {
                                    drop(pass);
                                    self.blur_and_composite(
                                        &mut encoder,
                                        &current_target,
                                        &parent,
                                        boundary.bounds,
                                        boundary.content_mask.bounds,
                                        [
                                            boundary.corner_radii.top_left.0,
                                            boundary.corner_radii.top_right.0,
                                            boundary.corner_radii.bottom_right.0,
                                            boundary.corner_radii.bottom_left.0,
                                        ],
                                        max_blur_radius(&boundary.filters),
                                        boundary.opacity,
                                        false,
                                    );
                                    current_target = parent;
                                    pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                        label: Some("main_pass_continued"),
                                        color_attachments: &[Some(
                                            wgpu::RenderPassColorAttachment {
                                                view: &current_target,
                                                resolve_target: None,
                                                ops: wgpu::Operations {
                                                    load: wgpu::LoadOp::Load,
                                                    store: wgpu::StoreOp::Store,
                                                },
                                                depth_slice: None,
                                            },
                                        )],
                                        depth_stencil_attachment: None,
                                        ..Default::default()
                                    });
                                }
                            }
                            true
                        }
                    };
                    if !ok {
                        overflow = true;
                        break;
                    }
                }
            }

            if overflow {
                drop(encoder);
                if self.instance_buffer_capacity >= self.max_buffer_size {
                    log::error!(
                        "instance buffer size grew too large: {}",
                        self.instance_buffer_capacity
                    );
                    frame.present();
                    return true;
                }
                self.grow_instance_buffer();
                continue;
            }

            // Present the offscreen scene by copying it into the swapchain texture. Skipped when
            // rendering went straight to the swapchain (no filters this frame).
            if let Some(scene_color_view) = &scene_color_view {
                self.blit_to_frame(&mut encoder, scene_color_view, &frame_view);
            }

            self.resources()
                .queue
                .submit(std::iter::once(encoder.finish()));
            frame.present();
            return true;
        }
    }

    fn draw_quads(
        &self,
        quads: &[Quad],
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let mut scratch = self.instance_scratch.borrow_mut();
        scratch.quads.clear();
        scratch.quads.extend(quads.iter().map(GpuQuad::from));
        self.draw_instances(
            scratch.quads.as_slice(),
            &self.resources().pipelines.quads,
            instance_offset,
            pass,
        )
    }

    fn draw_shadows(
        &self,
        shadows: &[Shadow],
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let mut scratch = self.instance_scratch.borrow_mut();
        scratch.shadows.clear();
        scratch.shadows.extend(shadows.iter().map(GpuShadow::from));
        self.draw_instances(
            scratch.shadows.as_slice(),
            &self.resources().pipelines.shadows,
            instance_offset,
            pass,
        )
    }

    fn draw_underlines(
        &self,
        underlines: &[Underline],
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let mut scratch = self.instance_scratch.borrow_mut();
        scratch.underlines.clear();
        scratch
            .underlines
            .extend(underlines.iter().map(GpuUnderline::from));
        self.draw_instances(
            scratch.underlines.as_slice(),
            &self.resources().pipelines.underlines,
            instance_offset,
            pass,
        )
    }

    fn draw_monochrome_sprites(
        &self,
        sprites: &[MonochromeSprite],
        texture_id: AtlasTextureId,
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let tex_info = self.atlas.get_texture_info(texture_id);
        let mut scratch = self.instance_scratch.borrow_mut();
        scratch.monochrome_sprites.clear();
        scratch
            .monochrome_sprites
            .extend(sprites.iter().map(GpuTextSprite::from));
        self.draw_instances_with_texture(
            scratch.monochrome_sprites.as_slice(),
            &tex_info.view,
            &self.resources().pipelines.mono_sprites,
            instance_offset,
            pass,
        )
    }

    fn draw_subpixel_sprites(
        &self,
        sprites: &[SubpixelSprite],
        texture_id: AtlasTextureId,
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let tex_info = self.atlas.get_texture_info(texture_id);
        let mut scratch = self.instance_scratch.borrow_mut();
        scratch.subpixel_sprites.clear();
        scratch
            .subpixel_sprites
            .extend(sprites.iter().map(GpuTextSprite::from));
        let resources = self.resources();
        let pipeline = resources
            .pipelines
            .subpixel_sprites
            .as_ref()
            .unwrap_or(&resources.pipelines.mono_sprites);
        self.draw_instances_with_texture(
            scratch.subpixel_sprites.as_slice(),
            &tex_info.view,
            pipeline,
            instance_offset,
            pass,
        )
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn draw_surfaces(&self, surfaces: &[PaintSurface], pass: &mut wgpu::RenderPass<'_>) -> bool {
        let resources = self.resources();
        for surface in surfaces {
            let Some(wgpu_texture) = surface.texture.downcast_ref::<wgpu::Texture>() else {
                continue;
            };

            let texture_view = wgpu_texture.create_view(&wgpu::TextureViewDescriptor::default());

            let params = SurfaceParams {
                bounds: surface.bounds.into(),
                content_mask: surface.content_mask.bounds.into(),
            };

            resources.queue.write_buffer(
                &resources.surface_uniform_buffer,
                0,
                bytemuck::bytes_of(&params),
            );

            let bind_group = resources
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("surface_bind_group"),
                    layout: &resources.bind_group_layouts.surfaces,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: resources.surface_uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&texture_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&resources.surface_sampler),
                        },
                    ],
                });

            pass.set_pipeline(&resources.pipelines.surfaces);
            pass.set_bind_group(0, &resources.globals_bind_group, &[]);
            pass.set_bind_group(1, &bind_group, &[]);
            pass.draw(0..4, 0..1);
        }
        true
    }

    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    fn draw_surfaces(&self, _surfaces: &[PaintSurface], _pass: &mut wgpu::RenderPass<'_>) -> bool {
        true
    }

    /// Build a bind group for a blur pass. Writes `params` into the next slot of the shared
    /// `blur_params_buffer` (no per-pass allocation) and references that slot, the source texture,
    /// and the filtering sampler. Distinct per-pass offsets keep `write_buffer`'s
    /// last-write-at-submit semantics from clobbering earlier passes within a frame.
    fn make_blur_bind_group(
        &self,
        params: BlurParams,
        source: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        let resources = self.resources();
        let slot = self.blur_params_slot.get() % BLUR_PARAMS_SLOTS;
        self.blur_params_slot.set(slot + 1);
        let offset = slot * self.blur_params_stride;
        resources.queue.write_buffer(
            &resources.blur_params_buffer,
            offset,
            bytemuck::bytes_of(&params),
        );
        resources
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blur_bind_group"),
                layout: &resources.bind_group_layouts.blur,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &resources.blur_params_buffer,
                            offset,
                            size: NonZeroU64::new(std::mem::size_of::<BlurParams>() as u64),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(source),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&resources.surface_sampler),
                    },
                ],
            })
    }

    /// Run a full-screen (3-vertex) blur pass that overwrites `target` by sampling `source`.
    /// `scissor` (x, y, w, h, in `target` pixels) limits fragment work to the region that
    /// actually feeds the composite — the element bounds dilated by the kernel radius.
    fn run_blur_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        label: &str,
        pipeline: &wgpu::RenderPipeline,
        target: &wgpu::TextureView,
        source: &wgpu::TextureView,
        params: BlurParams,
        scissor: [u32; 4],
    ) {
        let bind_group = self.make_blur_bind_group(params, source);
        let resources = self.resources();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &resources.globals_bind_group, &[]);
        pass.set_bind_group(1, &bind_group, &[]);
        pass.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3]);
        pass.draw(0..3, 0..1);
    }

    /// Blur `source` (full-resolution) and composite the result into `target`, clipped to
    /// `bounds`/`corner_radii`/`content_mask` and modulated by `opacity`. Shared by the backdrop
    /// and content-filter paths. Uses the half-resolution ping/pong textures as scratch.
    #[allow(clippy::too_many_arguments)]
    fn blur_and_composite(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
        bounds: Bounds<ScaledPixels>,
        content_mask: Bounds<ScaledPixels>,
        corner_radii: [f32; 4],
        blur_radius: f32,
        opacity: f32,
        // Backdrop clips to the rounded rect; content (`filter`) bleeds past its bounds.
        clip_rounded: bool,
    ) {
        // Sigma is halved because the blur runs at half resolution.
        let sigma = (blur_radius * 0.5).max(0.0);
        if sigma <= 0.0 {
            return;
        }
        // Span ±3σ. If that needs more than 32 taps, spread the taps apart (tap_step > 1) rather
        // than truncating the kernel — keeps very large radii from clipping (review #6).
        let ideal_taps = (3.0 * sigma).ceil();
        let tap_count = ideal_taps.clamp(1.0, 32.0);
        let tap_step = (ideal_taps / tap_count).max(1.0);
        let full_w = self.surface_config.width;
        let full_h = self.surface_config.height;
        let blur_width = (full_w / 2).max(1) as f32;
        let blur_height = (full_h / 2).max(1) as f32;

        // Limit the half-res passes to the element bounds dilated by the kernel radius (3·sigma,
        // full-res) — outside that the composite never samples, so there's no reason to blur it.
        let dilation = 3.0 * blur_radius;
        let hw = (full_w / 2).max(1);
        let hh = (full_h / 2).max(1);
        let x0 = (((bounds.origin.x.0 - dilation) * 0.5).floor().max(0.0) as u32).min(hw);
        let y0 = (((bounds.origin.y.0 - dilation) * 0.5).floor().max(0.0) as u32).min(hh);
        let x1 = ((((bounds.origin.x.0 + bounds.size.width.0 + dilation) * 0.5)
            .ceil()
            .max(0.0) as u32)
            .min(hw))
        .max(x0);
        let y1 = ((((bounds.origin.y.0 + bounds.size.height.0 + dilation) * 0.5)
            .ceil()
            .max(0.0) as u32)
            .min(hh))
        .max(y0);
        let scissor = [x0, y0, x1 - x0, y1 - y0];
        if scissor[2] == 0 || scissor[3] == 0 {
            return;
        }

        // Owned handles so the passes below don't borrow `self`.
        let (ping, pong) = {
            let resources = self.resources();
            match (
                resources.blur_ping_view.as_ref(),
                resources.blur_pong_view.as_ref(),
            ) {
                (Some(ping), Some(pong)) => (ping.clone(), pong.clone()),
                _ => return,
            }
        };

        // Downsample source -> ping, then separable gaussian ping -> pong -> ping.
        self.run_blur_pass(
            encoder,
            "blur_downsample",
            &self.resources().pipelines.blur_downsample,
            &ping,
            source,
            BlurParams {
                downsample: 1.0,
                ..Default::default()
            },
            scissor,
        );
        self.run_blur_pass(
            encoder,
            "blur_horizontal",
            &self.resources().pipelines.blur,
            &pong,
            &ping,
            BlurParams {
                direction: [1.0 / blur_width, 0.0],
                sigma,
                tap_count,
                tap_step,
                ..Default::default()
            },
            scissor,
        );
        self.run_blur_pass(
            encoder,
            "blur_vertical",
            &self.resources().pipelines.blur,
            &ping,
            &pong,
            BlurParams {
                direction: [0.0, 1.0 / blur_height],
                sigma,
                tap_count,
                tap_step,
                ..Default::default()
            },
            scissor,
        );

        // Composite the blurred result into the target (loads existing content). For content blur
        // the quad covers the dilated region so the blur can fade out past the element box (no
        // sharp clip); for backdrop the quad is the element bounds and the shader clips to the
        // rounded rect.
        let composite_bounds = if clip_rounded {
            bounds
        } else {
            bounds.dilate(ScaledPixels(dilation))
        };
        let params = BlurParams {
            bounds: composite_bounds.into(),
            content_mask: content_mask.into(),
            corner_radii,
            opacity,
            clip_rounded: if clip_rounded { 1.0 } else { 0.0 },
            ..Default::default()
        };
        let bind_group = self.make_blur_bind_group(params, &ping);
        let resources = self.resources();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blur_composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(&resources.pipelines.blur_composite);
        pass.set_bind_group(0, &resources.globals_bind_group, &[]);
        pass.set_bind_group(1, &bind_group, &[]);
        pass.draw(0..4, 0..1);
    }

    /// Blur the scene painted so far behind `filter.bounds` and composite it back as frosted glass.
    fn draw_backdrop_filter(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        filter: &BackdropFilter,
        scene_color_view: &wgpu::TextureView,
    ) {
        self.blur_and_composite(
            encoder,
            scene_color_view,
            scene_color_view,
            filter.bounds,
            filter.content_mask.bounds,
            [
                filter.corner_radii.top_left.0,
                filter.corner_radii.top_right.0,
                filter.corner_radii.bottom_right.0,
                filter.corner_radii.bottom_left.0,
            ],
            max_blur_radius(&filter.filters),
            filter.opacity,
            true,
        );
    }

    /// Copy the offscreen scene texture into the swapchain texture.
    fn blit_to_frame(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
        frame_view: &wgpu::TextureView,
    ) {
        let bind_group = self.make_blur_bind_group(BlurParams::default(), source);
        let resources = self.resources();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene_blit"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: frame_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(&resources.pipelines.blur_downsample);
        pass.set_bind_group(0, &resources.globals_bind_group, &[]);
        pass.set_bind_group(1, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    fn draw_polychrome_sprites(
        &self,
        sprites: &[PolychromeSprite],
        texture_id: AtlasTextureId,
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let tex_info = self.atlas.get_texture_info(texture_id);
        let mut scratch = self.instance_scratch.borrow_mut();
        scratch.polychrome_sprites.clear();
        scratch
            .polychrome_sprites
            .extend(sprites.iter().map(GpuPolychromeSprite::from));
        self.draw_instances_with_texture(
            scratch.polychrome_sprites.as_slice(),
            &tex_info.view,
            &self.resources().pipelines.poly_sprites,
            instance_offset,
            pass,
        )
    }

    fn draw_instances<T: Pod>(
        &self,
        instances: &[T],
        pipeline: &wgpu::RenderPipeline,
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        if instances.is_empty() {
            return true;
        }
        let Some((offset, size)) = self.write_to_instance_buffer(instance_offset, instances) else {
            return false;
        };
        let resources = self.resources();
        let bind_group = resources
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &resources.bind_group_layouts.instances,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.instance_binding(offset, size),
                }],
            });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &resources.globals_bind_group, &[]);
        pass.set_bind_group(1, &bind_group, &[]);
        pass.draw(0..4, 0..instances.len() as u32);
        true
    }

    fn draw_instances_with_texture<T: Pod>(
        &self,
        instances: &[T],
        texture_view: &wgpu::TextureView,
        pipeline: &wgpu::RenderPipeline,
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        if instances.is_empty() {
            return true;
        }
        let Some((offset, size)) = self.write_to_instance_buffer(instance_offset, instances) else {
            return false;
        };
        let resources = self.resources();
        let bind_group = resources
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &resources.bind_group_layouts.instances_with_texture,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.instance_binding(offset, size),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&resources.atlas_sampler),
                    },
                ],
            });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &resources.globals_bind_group, &[]);
        pass.set_bind_group(1, &bind_group, &[]);
        pass.draw(0..4, 0..instances.len() as u32);
        true
    }

    fn draw_paths_from_intermediate(
        &self,
        paths: &[Path<ScaledPixels>],
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let first_path = &paths[0];
        let mut scratch = self.instance_scratch.borrow_mut();
        scratch.path_sprites.clear();
        if paths.last().map(|p| &p.order) == Some(&first_path.order) {
            scratch
                .path_sprites
                .extend(paths.iter().map(|path| PathSprite {
                    bounds: path.clipped_bounds().into(),
                }));
        } else {
            let mut bounds = first_path.clipped_bounds();
            for path in paths.iter().skip(1) {
                bounds = bounds.union(&path.clipped_bounds());
            }
            scratch.path_sprites.push(PathSprite {
                bounds: bounds.into(),
            });
        }

        let resources = self.resources();
        let Some(path_intermediate_view) = resources.path_intermediate_view.as_ref() else {
            return true;
        };

        self.draw_instances_with_texture(
            scratch.path_sprites.as_slice(),
            path_intermediate_view,
            &resources.pipelines.paths,
            instance_offset,
            pass,
        )
    }

    fn draw_paths_to_intermediate(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        paths: &[Path<ScaledPixels>],
        instance_offset: &mut u64,
    ) -> bool {
        let (vertex_offset, vertex_size, vertex_count) = {
            let mut scratch = self.instance_scratch.borrow_mut();
            scratch.path_vertices.clear();
            for path in paths {
                let bounds = path.clipped_bounds().into();
                scratch
                    .path_vertices
                    .extend(path.vertices.iter().map(|vertex| PathRasterizationVertex {
                        xy_position: [vertex.xy_position.x.0, vertex.xy_position.y.0],
                        st_position: [vertex.st_position.x, vertex.st_position.y],
                        color: (&path.color).into(),
                        bounds,
                    }));
            }

            if scratch.path_vertices.is_empty() {
                return true;
            }

            let Some((vertex_offset, vertex_size)) =
                self.write_to_instance_buffer(instance_offset, scratch.path_vertices.as_slice())
            else {
                return false;
            };
            (
                vertex_offset,
                vertex_size,
                scratch.path_vertices.len() as u32,
            )
        };

        let resources = self.resources();
        let data_bind_group = resources
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("path_rasterization_bind_group"),
                layout: &resources.bind_group_layouts.instances,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.instance_binding(vertex_offset, vertex_size),
                }],
            });

        let Some(path_intermediate_view) = resources.path_intermediate_view.as_ref() else {
            return true;
        };

        let (target_view, resolve_target) = if let Some(ref msaa_view) = resources.path_msaa_view {
            (msaa_view, Some(path_intermediate_view))
        } else {
            (path_intermediate_view, None)
        };

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("path_rasterization_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            pass.set_pipeline(&resources.pipelines.path_rasterization);
            pass.set_bind_group(0, &resources.path_globals_bind_group, &[]);
            pass.set_bind_group(1, &data_bind_group, &[]);
            pass.draw(0..vertex_count, 0..1);
        }

        true
    }

    fn grow_instance_buffer(&mut self) {
        let new_capacity = (self.instance_buffer_capacity * 2).min(self.max_buffer_size);
        log::info!("increased instance buffer size to {}", new_capacity);
        let resources = self.resources_mut();
        resources.instance_buffer = resources.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance_buffer"),
            size: new_capacity,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.instance_buffer_capacity = new_capacity;
    }

    fn write_to_instance_buffer<T: Pod>(
        &self,
        instance_offset: &mut u64,
        instances: &[T],
    ) -> Option<(u64, NonZeroU64)> {
        // Pod is the compile-time gate that prevents scene structs with padding or Rust-only
        // representations from reaching the storage buffer.
        let data = bytemuck::cast_slice(instances);
        let offset = (*instance_offset).next_multiple_of(self.storage_buffer_alignment);
        let size = (data.len() as u64).max(16);
        if offset + size > self.instance_buffer_capacity {
            return None;
        }
        let resources = self.resources();
        resources
            .queue
            .write_buffer(&resources.instance_buffer, offset, data);
        *instance_offset = offset + size;
        Some((offset, NonZeroU64::new(size).expect("size is at least 16")))
    }

    fn instance_binding(&self, offset: u64, size: NonZeroU64) -> wgpu::BindingResource<'_> {
        wgpu::BindingResource::Buffer(wgpu::BufferBinding {
            buffer: &self.resources().instance_buffer,
            offset,
            size: Some(size),
        })
    }

    /// Mark the surface as unconfigured so rendering is skipped until a new
    /// surface is provided via [`replace_surface`](Self::replace_surface).
    ///
    /// This does **not** drop the renderer — the device, queue, atlas, and
    /// pipelines stay alive.  Use this when the native window is destroyed
    /// (e.g. Android `TerminateWindow`) but you intend to re-create the
    /// surface later without losing cached atlas textures.
    pub fn unconfigure_surface(&mut self) {
        self.surface_configured = false;
        // Drop intermediate textures since they reference the old surface size.
        if let Some(res) = self.resources.as_mut() {
            res.invalidate_intermediate_textures();
        }
    }

    /// Replace the wgpu surface with a new one (e.g. after Android destroys
    /// and recreates the native window).  Keeps the device, queue, atlas, and
    /// all pipelines intact so cached `AtlasTextureId`s remain valid.
    ///
    /// The `instance` **must** be the same [`wgpu::Instance`] that was used to
    /// create the adapter and device (i.e. from the [`WgpuContext`]).  Using a
    /// different instance will cause a "Device does not exist" panic because
    /// the wgpu device is bound to its originating instance.
    #[cfg(not(target_family = "wasm"))]
    pub fn replace_surface<W: HasWindowHandle>(
        &mut self,
        window: &W,
        config: WgpuSurfaceConfig,
        instance: &wgpu::Instance,
    ) -> anyhow::Result<()> {
        let window_handle = window
            .window_handle()
            .map_err(|e| anyhow::anyhow!("Failed to get window handle: {e}"))?;

        let surface = create_surface(instance, window_handle.as_raw())?;

        let width = (config.size.width.0 as u32).max(1);
        let height = (config.size.height.0 as u32).max(1);

        let alpha_mode = if config.transparent {
            self.transparent_alpha_mode
        } else {
            self.opaque_alpha_mode
        };

        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface_config.alpha_mode = alpha_mode;
        if let Some(mode) = config.preferred_present_mode {
            self.surface_config.present_mode = mode;
        }

        {
            let res = self
                .resources
                .as_mut()
                .expect("GPU resources not available");
            surface.configure(&res.device, &self.surface_config);
            res.surface = surface;

            // Invalidate intermediate textures — they'll be recreated lazily.
            res.invalidate_intermediate_textures();
        }

        self.surface_configured = true;

        Ok(())
    }

    pub fn destroy(&mut self) {
        // Release surface-bound GPU resources eagerly so the underlying native
        // window can be destroyed before the renderer itself is dropped.
        self.resources.take();
    }

    /// Returns true if the GPU device was lost and recovery is needed.
    pub fn device_lost(&self) -> bool {
        self.device_lost.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Returns true if a redraw is needed because GPU state was cleared.
    /// Calling this method clears the flag.
    pub fn needs_redraw(&mut self) -> bool {
        std::mem::take(&mut self.needs_redraw)
    }

    /// Recovers from a lost GPU device by recreating the renderer with a new context.
    ///
    /// Call this after detecting `device_lost()` returns true.
    ///
    /// This method coordinates recovery across multiple windows:
    /// - The first window to call this will recreate the shared context
    /// - Subsequent windows will adopt the already-recovered context
    #[cfg(not(target_family = "wasm"))]
    pub fn recover<W>(&mut self, window: &W) -> anyhow::Result<WgpuRecoveryStatus>
    where
        W: HasWindowHandle + HasDisplayHandle + std::fmt::Debug + Send + Sync + Clone + 'static,
    {
        let gpu_context = self
            .context
            .as_ref()
            .expect("recover requires gpu_context")
            .clone();
        let now = Instant::now();

        let window_handle = window
            .window_handle()
            .map_err(|e| anyhow::anyhow!("Failed to get window handle: {e}"))?;

        enum RecoveryAction {
            AdoptRecoveredContext,
            RebuildSharedContext,
        }

        let recovery_action = {
            let mut shared = gpu_context.shared.borrow_mut();
            if shared
                .context
                .as_ref()
                .is_some_and(|context| !context.device_lost())
            {
                RecoveryAction::AdoptRecoveredContext
            } else {
                if let Err(deferral) = shared.recovery.begin_rebuild(now) {
                    return Ok(deferral.into());
                }
                RecoveryAction::RebuildSharedContext
            }
        };

        let recovered_surface = match recovery_action {
            RecoveryAction::AdoptRecoveredContext => {
                if self.recovery_backoff.is_exhausted() {
                    let report = !std::mem::replace(&mut self.recovery_exhaustion_reported, true);
                    return Ok(WgpuRecoveryStatus::from(RecoveryDeferral::Exhausted {
                        report,
                    }));
                }
                if self.recovery_backoff.retry_delay(now).is_some() {
                    return Ok(WgpuRecoveryStatus::Deferred);
                }
                None
            }
            RecoveryAction::RebuildSharedContext => {
                log::warn!("GPU device lost, recreating context...");

                // Create the replacement outside the shared RefCell borrow. This keeps other
                // windows responsive and lets them observe that one rebuild is already active.
                let replacement = (|| {
                    let instance = WgpuContext::instance(Box::new(window.clone()));
                    let surface = create_surface(&instance, window_handle.as_raw())?;
                    // Recovery must allow CPU adapters so a VM or a machine with a failed
                    // hardware driver can keep rendering through the WGPU software path.
                    let context = WgpuContext::new(
                        instance,
                        &surface,
                        self.compositor_gpu,
                        self.extra_requirements.as_ref(),
                    )?;
                    anyhow::Ok((context, surface))
                })();

                match replacement {
                    Ok((context, surface)) => {
                        let mut shared = gpu_context.shared.borrow_mut();
                        shared.context = Some(context);
                        shared.recovery.record_success();
                        Some(surface)
                    }
                    Err(error) => {
                        let mut shared = gpu_context.shared.borrow_mut();
                        shared.recovery.record_failure(Instant::now());
                        return Err(error.context("Recreating shared GPU context"));
                    }
                }
            }
        };

        let config = WgpuSurfaceConfig {
            size: gpui::Size {
                width: gpui::DevicePixels(self.surface_config.width as i32),
                height: gpui::DevicePixels(self.surface_config.height as i32),
            },
            transparent: self.surface_config.alpha_mode != wgpu::CompositeAlphaMode::Opaque,
            preferred_present_mode: Some(self.surface_config.present_mode),
        };
        let extra_reqs = self.extra_requirements.clone();
        let compositor_gpu = self.compositor_gpu;
        let atlas = self.atlas.clone();
        let replacement = (|| {
            let shared = gpu_context.shared.borrow();
            let context = shared.context.as_ref().expect("context should exist");
            let surface = match recovered_surface {
                Some(surface) => surface,
                None => create_surface(&context.instance, window_handle.as_raw())?,
            };

            Self::new_internal(
                Some(gpu_context.clone()),
                context,
                surface,
                config,
                compositor_gpu,
                extra_reqs,
                atlas.clone(),
            )
        })();

        let replacement = match replacement {
            Ok(replacement) => replacement,
            Err(error) => {
                self.recovery_backoff.record_failure(Instant::now());
                return Err(error.context("Rebuilding window GPU resources"));
            }
        };

        // Advance the atlas generation only after every replacement resource was created.
        // Failed recovery attempts therefore cannot invalidate otherwise recoverable handles.
        {
            let shared = gpu_context.shared.borrow();
            atlas.handle_device_lost(shared.context.as_ref().expect("context should exist"));
        }
        *self = replacement;

        log::info!("GPU recovery complete");
        Ok(WgpuRecoveryStatus::Recovered)
    }
}

#[cfg(not(target_family = "wasm"))]
fn create_surface(
    instance: &wgpu::Instance,
    raw_window_handle: raw_window_handle::RawWindowHandle,
) -> anyhow::Result<wgpu::Surface<'static>> {
    unsafe {
        instance
            .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                // Fall back to the display handle already provided via InstanceDescriptor::display.
                raw_display_handle: None,
                raw_window_handle,
            })
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

struct RenderingParameters {
    path_sample_count: u32,
    gamma_ratios: [f32; 4],
    grayscale_enhanced_contrast: f32,
    subpixel_enhanced_contrast: f32,
}

const FONT_GAMMA_ENV: &str = "OXIDETERM_FONTS_GAMMA";
const FONT_GRAYSCALE_CONTRAST_ENV: &str = "OXIDETERM_FONTS_GRAYSCALE_ENHANCED_CONTRAST";
const FONT_SUBPIXEL_CONTRAST_ENV: &str = "OXIDETERM_FONTS_SUBPIXEL_ENHANCED_CONTRAST";

impl RenderingParameters {
    fn new(adapter: &wgpu::Adapter, surface_format: wgpu::TextureFormat) -> Self {
        use std::env;

        let format_features = adapter.get_texture_format_features(surface_format);
        let path_sample_count = [4, 2, 1]
            .into_iter()
            .find(|&n| format_features.flags.sample_count_supported(n))
            .unwrap_or(1);

        let gamma = env::var(FONT_GAMMA_ENV)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.8_f32)
            .clamp(1.0, 2.2);
        let gamma_ratios = get_gamma_correction_ratios(gamma);

        let grayscale_enhanced_contrast = env::var(FONT_GRAYSCALE_CONTRAST_ENV)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.0_f32)
            .max(0.0);

        let subpixel_enhanced_contrast = env::var(FONT_SUBPIXEL_CONTRAST_ENV)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.5_f32)
            .max(0.0);

        Self {
            path_sample_count,
            gamma_ratios,
            grayscale_enhanced_contrast,
            subpixel_enhanced_contrast,
        }
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use std::{
        collections::BTreeSet,
        time::{Duration, Instant},
    };
    use wgpu::naga;

    fn shader_struct<'a>(module: &'a naga::Module, name: &str) -> &'a naga::TypeInner {
        module
            .types
            .iter()
            .find_map(|(_, shader_type)| {
                (shader_type.name.as_deref() == Some(name)).then_some(&shader_type.inner)
            })
            .unwrap_or_else(|| panic!("shader struct '{name}' was not found"))
    }

    fn assert_shader_struct_layout<T>(
        module: &naga::Module,
        shader_name: &str,
        host_members: &[(&str, usize)],
    ) {
        let naga::TypeInner::Struct { members, span } = shader_struct(module, shader_name) else {
            panic!("shader type '{shader_name}' is not a struct");
        };

        assert_eq!(
            std::mem::size_of::<T>(),
            *span as usize,
            "host struct for '{shader_name}' has the wrong size"
        );
        assert_eq!(
            host_members.len(),
            members.len(),
            "host struct for '{shader_name}' has the wrong field count"
        );
        for ((host_name, host_offset), shader_member) in host_members.iter().zip(members) {
            assert_eq!(
                Some(*host_name),
                shader_member.name.as_deref(),
                "host and WGSL fields differ in '{shader_name}'"
            );
            assert_eq!(
                *host_offset, shader_member.offset as usize,
                "host field '{shader_name}.{host_name}' has the wrong offset"
            );
        }
    }

    macro_rules! assert_struct_layout {
        ($module:expr, $host:ty => $shader:literal { $($field:ident),+ $(,)? }) => {
            assert_shader_struct_layout::<$host>(
                $module,
                $shader,
                &[$((stringify!($field), std::mem::offset_of!($host, $field))),+],
            );
        };
    }

    fn collect_buffer_structs(
        module: &naga::Module,
        type_handle: naga::Handle<naga::Type>,
        names: &mut BTreeSet<String>,
    ) {
        let shader_type = &module.types[type_handle];
        match &shader_type.inner {
            naga::TypeInner::Struct { members, .. } => {
                if let Some(name) = &shader_type.name {
                    names.insert(name.clone());
                }
                for member in members {
                    collect_buffer_structs(module, member.ty, names);
                }
            }
            naga::TypeInner::Array { base, .. }
            | naga::TypeInner::BindingArray { base, .. }
            | naga::TypeInner::Pointer { base, .. } => {
                collect_buffer_structs(module, *base, names);
            }
            _ => {}
        }
    }

    fn assert_all_buffer_structs_are_audited(module: &naga::Module, expected: &[&str]) {
        let mut actual = BTreeSet::new();
        for (_, variable) in module.global_variables.iter() {
            if matches!(
                variable.space,
                naga::AddressSpace::Uniform | naga::AddressSpace::Storage { .. }
            ) {
                collect_buffer_structs(module, variable.ty, &mut actual);
            }
        }
        let expected = expected.iter().map(|name| (*name).to_string()).collect();
        assert_eq!(
            actual, expected,
            "the WGPU buffer layout audit is incomplete"
        );
    }

    fn assert_all_buffer_globals_are_audited(module: &naga::Module, expected: &[&str]) {
        let actual = module
            .global_variables
            .iter()
            .filter_map(|(_, variable)| {
                matches!(
                    variable.space,
                    naga::AddressSpace::Uniform | naga::AddressSpace::Storage { .. }
                )
                .then(|| {
                    variable
                        .name
                        .clone()
                        .expect("buffer globals must have stable names")
                })
            })
            .collect::<BTreeSet<_>>();
        let expected = expected.iter().map(|name| (*name).to_string()).collect();
        assert_eq!(
            actual, expected,
            "the WGPU buffer binding audit is incomplete"
        );
    }

    fn global_variable<'a>(module: &'a naga::Module, name: &str) -> &'a naga::GlobalVariable {
        module
            .global_variables
            .iter()
            .find_map(|(_, variable)| (variable.name.as_deref() == Some(name)).then_some(variable))
            .unwrap_or_else(|| panic!("shader global '{name}' was not found"))
    }

    fn assert_storage_array_stride<T>(module: &naga::Module, global_name: &str, item_name: &str) {
        let variable = global_variable(module, global_name);
        assert!(matches!(variable.space, naga::AddressSpace::Storage { .. }));
        let naga::TypeInner::Array { base, stride, .. } = &module.types[variable.ty].inner else {
            panic!("shader storage '{global_name}' is not an array");
        };
        assert_eq!(module.types[*base].name.as_deref(), Some(item_name));
        assert_eq!(
            *stride as usize,
            std::mem::size_of::<T>(),
            "storage array '{global_name}' has the wrong item stride"
        );
    }

    fn assert_uniform_size<T>(module: &naga::Module, global_name: &str) {
        let variable = global_variable(module, global_name);
        assert_eq!(variable.space, naga::AddressSpace::Uniform);
        let mut layouter = naga::proc::Layouter::default();
        layouter
            .update(module.to_ctx())
            .expect("WGSL type layout should resolve");
        assert_eq!(
            layouter[variable.ty].size as usize,
            std::mem::size_of::<T>(),
            "uniform '{global_name}' has the wrong host size"
        );
    }

    fn parse_and_validate_shader(
        source: &str,
        capabilities: naga::valid::Capabilities,
    ) -> naga::Module {
        let module = naga::front::wgsl::parse_str(source).expect("GPUI WGSL should parse");
        // Bindings are intentionally reused by mutually exclusive pipelines in one WGSL module.
        let validation_flags =
            naga::valid::ValidationFlags::all() ^ naga::valid::ValidationFlags::BINDINGS;
        naga::valid::Validator::new(validation_flags, capabilities)
            .validate(&module)
            .expect("GPUI WGSL should pass startup validation");
        module
    }

    #[test]
    fn wgpu_shader_buffer_contract_matches_rust() {
        let module = parse_and_validate_shader(
            include_str!("shaders.wgsl"),
            naga::valid::Capabilities::empty(),
        );
        assert_all_buffer_structs_are_audited(
            &module,
            &[
                "AtlasBounds",
                "AtlasTextureId",
                "AtlasTile",
                "Background",
                "BlurParams",
                "Bounds",
                "Corners",
                "Edges",
                "GammaParams",
                "GlobalParams",
                "Hsla",
                "LinearColorStop",
                "MonochromeSprite",
                "PathRasterizationVertex",
                "PathSprite",
                "PolychromeSprite",
                "Quad",
                "Shadow",
                "SurfaceParams",
                "TransformationMatrix",
                "Underline",
            ],
        );
        assert_all_buffer_globals_are_audited(
            &module,
            &[
                "b_mono_sprites",
                "b_path_sprites",
                "b_path_vertices",
                "b_poly_sprites",
                "b_quads",
                "b_shadows",
                "b_underlines",
                "blur_locals",
                "gamma_params",
                "globals",
                "surface_locals",
            ],
        );

        // Audit every structure reachable from a uniform or storage binding, including all
        // nested structures. The Pod derives additionally reject implicit host padding.
        assert_struct_layout!(&module, GlobalParams => "GlobalParams" {
            viewport_size, premultiplied_alpha, pad
        });
        assert_struct_layout!(&module, GammaParams => "GammaParams" {
            gamma_ratios, grayscale_enhanced_contrast, subpixel_enhanced_contrast, is_bgr, pad
        });
        assert_struct_layout!(&module, PodBounds => "Bounds" { origin, size });
        assert_struct_layout!(&module, GpuCorners => "Corners" {
            top_left, top_right, bottom_right, bottom_left
        });
        assert_struct_layout!(&module, GpuEdges => "Edges" { top, right, bottom, left });
        assert_struct_layout!(&module, GpuHsla => "Hsla" { h, s, l, a });
        assert_struct_layout!(&module, GpuLinearColorStop => "LinearColorStop" {
            color, percentage
        });
        assert_struct_layout!(&module, GpuBackground => "Background" {
            tag, color_space, solid, gradient_angle_or_pattern_height, colors, pad
        });
        assert_struct_layout!(&module, GpuAtlasTextureId => "AtlasTextureId" { index, kind });
        assert_struct_layout!(&module, GpuAtlasBounds => "AtlasBounds" { origin, size });
        assert_struct_layout!(&module, GpuAtlasTile => "AtlasTile" {
            texture_id, tile_id, padding, bounds
        });
        assert_struct_layout!(&module, GpuTransformationMatrix => "TransformationMatrix" {
            rotation_scale, translation
        });
        assert_struct_layout!(&module, GpuQuad => "Quad" {
            order, border_style, bounds, content_mask, background, border_color,
            corner_radii, border_widths
        });
        assert_struct_layout!(&module, GpuShadow => "Shadow" {
            order, blur_radius, bounds, corner_radii, content_mask, color, element_bounds,
            element_corner_radii, inset, pad
        });
        assert_struct_layout!(&module, PathRasterizationVertex => "PathRasterizationVertex" {
            xy_position, st_position, color, bounds
        });
        assert_struct_layout!(&module, PathSprite => "PathSprite" { bounds });
        assert_struct_layout!(&module, GpuUnderline => "Underline" {
            order, pad, bounds, content_mask, color, thickness, wavy
        });
        assert_struct_layout!(&module, GpuTextSprite => "MonochromeSprite" {
            order, pad, bounds, content_mask, color, tile, transformation
        });
        assert_struct_layout!(&module, GpuPolychromeSprite => "PolychromeSprite" {
            order, pad, grayscale, opacity, bounds, content_mask, corner_radii, tile
        });
        assert_struct_layout!(&module, SurfaceParams => "SurfaceParams" {
            bounds, content_mask
        });
        assert_struct_layout!(&module, BlurParams => "BlurParams" {
            bounds, content_mask, corner_radii, direction, sigma, opacity, tap_count, tap_step,
            clip_rounded, downsample
        });

        assert_storage_array_stride::<GpuQuad>(&module, "b_quads", "Quad");
        assert_storage_array_stride::<GpuShadow>(&module, "b_shadows", "Shadow");
        assert_storage_array_stride::<PathRasterizationVertex>(
            &module,
            "b_path_vertices",
            "PathRasterizationVertex",
        );
        assert_storage_array_stride::<PathSprite>(&module, "b_path_sprites", "PathSprite");
        assert_storage_array_stride::<GpuUnderline>(&module, "b_underlines", "Underline");
        assert_storage_array_stride::<GpuTextSprite>(&module, "b_mono_sprites", "MonochromeSprite");
        assert_storage_array_stride::<GpuPolychromeSprite>(
            &module,
            "b_poly_sprites",
            "PolychromeSprite",
        );
        assert_uniform_size::<GlobalParams>(&module, "globals");
        assert_uniform_size::<GammaParams>(&module, "gamma_params");
        assert_uniform_size::<SurfaceParams>(&module, "surface_locals");
        assert_uniform_size::<BlurParams>(&module, "blur_locals");
    }

    #[test]
    fn subpixel_shader_buffer_contract_matches_rust() {
        let combined = format!(
            "enable dual_source_blending;\n{}\n{}",
            include_str!("shaders.wgsl"),
            include_str!("shaders_subpixel.wgsl")
        );
        let module =
            parse_and_validate_shader(&combined, naga::valid::Capabilities::DUAL_SOURCE_BLENDING);
        assert_all_buffer_structs_are_audited(
            &module,
            &[
                "AtlasBounds",
                "AtlasTextureId",
                "AtlasTile",
                "Background",
                "BlurParams",
                "Bounds",
                "Corners",
                "Edges",
                "GammaParams",
                "GlobalParams",
                "Hsla",
                "LinearColorStop",
                "MonochromeSprite",
                "PathRasterizationVertex",
                "PathSprite",
                "PolychromeSprite",
                "Quad",
                "Shadow",
                "SubpixelSprite",
                "SurfaceParams",
                "TransformationMatrix",
                "Underline",
            ],
        );
        assert_all_buffer_globals_are_audited(
            &module,
            &[
                "b_mono_sprites",
                "b_path_sprites",
                "b_path_vertices",
                "b_poly_sprites",
                "b_quads",
                "b_shadows",
                "b_subpixel_sprites",
                "b_underlines",
                "blur_locals",
                "gamma_params",
                "globals",
                "surface_locals",
            ],
        );
        assert_struct_layout!(&module, GpuTextSprite => "SubpixelSprite" {
            order, pad, bounds, content_mask, color, tile, transformation
        });
        assert_storage_array_stride::<GpuTextSprite>(
            &module,
            "b_subpixel_sprites",
            "SubpixelSprite",
        );
    }

    #[test]
    fn polychrome_grayscale_is_encoded_as_initialized_u32() {
        let mut sprite = PolychromeSprite {
            order: 7,
            pad: u32::MAX,
            grayscale: true,
            opacity: 0.5,
            bounds: Bounds::default(),
            content_mask: gpui::ContentMask::default(),
            corner_radii: Corners::default(),
            tile: AtlasTile {
                texture_id: AtlasTextureId {
                    index: 3,
                    kind: AtlasTextureKind::Polychrome,
                },
                tile_id: gpui::TileId(9),
                padding: 2,
                bounds: Bounds::default(),
            },
        };

        let encoded = GpuPolychromeSprite::from(&sprite);
        assert_eq!(encoded.grayscale, 1);
        assert_eq!(encoded.pad, 0);
        assert_eq!(encoded.tile.texture_id.kind, 1);

        sprite.grayscale = false;
        assert_eq!(GpuPolychromeSprite::from(&sprite).grayscale, 0);
    }

    #[test]
    fn scene_enum_encodings_match_wgsl_discriminants() {
        let solid = gpui::solid_background(Hsla::default());
        let linear = gpui::linear_gradient(
            90.0,
            gpui::linear_color_stop(Hsla::default(), 0.0),
            gpui::linear_color_stop(Hsla::default(), 1.0),
        )
        .color_space(gpui::ColorSpace::Oklab);
        let pattern = gpui::pattern_slash(Hsla::default(), 1.0, 1.0);
        let checkerboard = gpui::checkerboard(Hsla::default(), 2.0);
        assert_eq!(GpuBackground::from(&solid).tag, 0);
        assert_eq!(GpuBackground::from(&linear).tag, 1);
        assert_eq!(GpuBackground::from(&linear).color_space, 1);
        assert_eq!(GpuBackground::from(&pattern).tag, 2);
        assert_eq!(GpuBackground::from(&checkerboard).tag, 3);

        let mut quad = Quad::default();
        assert_eq!(GpuQuad::from(&quad).border_style, 0);
        quad.border_style = BorderStyle::Dashed;
        assert_eq!(GpuQuad::from(&quad).border_style, 1);

        let mut tile = AtlasTile {
            texture_id: AtlasTextureId {
                index: 0,
                kind: AtlasTextureKind::Monochrome,
            },
            tile_id: gpui::TileId(0),
            padding: 0,
            bounds: Bounds::default(),
        };
        assert_eq!(GpuAtlasTile::from(&tile).texture_id.kind, 0);
        tile.texture_id.kind = AtlasTextureKind::Polychrome;
        assert_eq!(GpuAtlasTile::from(&tile).texture_id.kind, 1);
        tile.texture_id.kind = AtlasTextureKind::Subpixel;
        assert_eq!(GpuAtlasTile::from(&tile).texture_id.kind, 2);
    }

    #[test]
    fn recovery_backoff_defers_without_blocking_until_deadline() {
        let now = Instant::now();
        let mut backoff = RecoveryBackoff::default();

        assert!(backoff.can_attempt(now));
        backoff.record_failure(now);

        assert!(!backoff.can_attempt(now));
        assert_eq!(backoff.retry_delay(now), Some(INITIAL_RECOVERY_DELAY));
        assert!(backoff.can_attempt(now + INITIAL_RECOVERY_DELAY));
    }

    #[test]
    fn recovery_backoff_caps_delay_and_failure_state() {
        let now = Instant::now();
        let mut backoff = RecoveryBackoff::default();

        for _ in 0..MAX_RECOVERY_FAILURES {
            backoff.record_failure(now);
        }

        assert_eq!(backoff.retry_delay(now), Some(MAX_RECOVERY_DELAY));
        assert_eq!(backoff.consecutive_failures, MAX_RECOVERY_FAILURES);
        assert!(backoff.is_exhausted());
        assert!(!backoff.can_attempt(now + MAX_RECOVERY_DELAY - Duration::from_millis(1)));
        assert!(!backoff.can_attempt(now + MAX_RECOVERY_DELAY));
    }

    #[test]
    fn successful_recovery_resets_backoff() {
        let now = Instant::now();
        let mut backoff = RecoveryBackoff::default();
        backoff.record_failure(now);

        backoff.reset();

        assert!(backoff.can_attempt(now));
        assert_eq!(backoff.consecutive_failures, 0);
        assert_eq!(backoff.retry_not_before, None);
    }

    #[test]
    fn shared_recovery_allows_only_one_rebuild_during_cooldown_cycle() {
        let now = Instant::now();
        let mut recovery = SharedRecoveryState::default();

        assert_eq!(recovery.begin_rebuild(now), Ok(()));
        assert_eq!(
            recovery.begin_rebuild(now),
            Err(RecoveryDeferral::Rebuilding)
        );

        recovery.record_failure(now);
        assert_eq!(
            recovery.begin_rebuild(now),
            Err(RecoveryDeferral::CoolingDown(INITIAL_RECOVERY_DELAY))
        );
        assert_eq!(recovery.begin_rebuild(now + INITIAL_RECOVERY_DELAY), Ok(()));
    }

    #[test]
    fn normal_recovery_deferrals_map_to_non_error_status() {
        assert_eq!(
            WgpuRecoveryStatus::from(RecoveryDeferral::Rebuilding),
            WgpuRecoveryStatus::Deferred
        );
        assert_eq!(
            WgpuRecoveryStatus::from(RecoveryDeferral::CoolingDown(Duration::from_secs(1))),
            WgpuRecoveryStatus::Deferred
        );
        assert_eq!(
            WgpuRecoveryStatus::from(RecoveryDeferral::Exhausted { report: true }),
            WgpuRecoveryStatus::Failed
        );
        assert_eq!(
            WgpuRecoveryStatus::from(RecoveryDeferral::Exhausted { report: false }),
            WgpuRecoveryStatus::Deferred
        );
    }

    #[test]
    fn shared_recovery_reports_terminal_exhaustion_only_once() {
        let now = Instant::now();
        let mut recovery = SharedRecoveryState::default();
        for _ in 0..MAX_RECOVERY_FAILURES {
            recovery.record_failure(now);
        }

        assert_eq!(
            recovery.begin_rebuild(now + MAX_RECOVERY_DELAY),
            Err(RecoveryDeferral::Exhausted { report: true })
        );
        assert_eq!(
            recovery.begin_rebuild(now + MAX_RECOVERY_DELAY),
            Err(RecoveryDeferral::Exhausted { report: false })
        );
    }
}
