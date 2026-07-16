// Doing `if let` gives you nice scoping with passes/encoders
#![allow(irrefutable_let_patterns)]

use super::{BladeAtlas, BladeContext};
use crate::{
    AtlasTextureKind, AtlasTile, BackdropBlur as SceneBackdropBlur, Background, Bounds,
    DevicePixels, GpuSpecs, MonochromeSprite, Path, Point,
    PolychromeSprite as ScenePolychromeSprite, PrimitiveBatch, Quad, ScaledPixels, Scene, Shadow,
    Size, Underline, backdrop_blur_batch_signature, backdrop_blur_work_area,
};
use blade_graphics as gpu;
use blade_graphics::traits::RenderEncoder;
use blade_util::{BufferBelt, BufferBeltDescriptor};
use bytemuck::{Pod, Zeroable};
#[cfg(target_os = "macos")]
use media::core_video::CVMetalTextureCache;
use std::sync::Arc;

const MAX_FRAME_TIME_MS: u32 = 10000;
const BACKDROP_BLUR_DOWNSAMPLE: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GlobalParams {
    viewport_size: [f32; 2],
    premultiplied_alpha: u32,
    pad: u32,
}

//Note: we can't use `Bounds` directly here because
// it doesn't implement Pod + Zeroable
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
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

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PodCorners {
    top_left: f32,
    top_right: f32,
    bottom_right: f32,
    bottom_left: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
// Blade reflects this Rust type name against the matching WGSL structure.
struct BackdropBlur {
    bounds: PodBounds,
    content_mask: PodBounds,
    corner_radii: PodCorners,
    overlay_color: [f32; 4],
}

impl From<&SceneBackdropBlur> for BackdropBlur {
    fn from(backdrop_blur: &SceneBackdropBlur) -> Self {
        Self {
            bounds: backdrop_blur.bounds.into(),
            content_mask: backdrop_blur.content_mask.bounds.into(),
            corner_radii: PodCorners {
                top_left: backdrop_blur.corner_radii.top_left.0,
                top_right: backdrop_blur.corner_radii.top_right.0,
                bottom_right: backdrop_blur.corner_radii.bottom_right.0,
                bottom_left: backdrop_blur.corner_radii.bottom_left.0,
            },
            overlay_color: [
                backdrop_blur.overlay_color.h,
                backdrop_blur.overlay_color.s,
                backdrop_blur.overlay_color.l,
                backdrop_blur.overlay_color.a,
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BackdropBlurPassParams {
    texture_size: [f32; 2],
    direction: [f32; 2],
    radius: f32,
    pad_0: f32,
    pad_1: f32,
    pad_2: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PodAtlasTextureId {
    index: u32,
    kind: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PodAtlasBounds {
    origin: [i32; 2],
    size: [i32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PodAtlasTile {
    texture_id: PodAtlasTextureId,
    tile_id: u32,
    padding: u32,
    bounds: PodAtlasBounds,
}

impl From<&AtlasTile> for PodAtlasTile {
    fn from(tile: &AtlasTile) -> Self {
        let kind = match tile.texture_id.kind {
            AtlasTextureKind::Monochrome => 0,
            AtlasTextureKind::Polychrome => 1,
        };
        Self {
            texture_id: PodAtlasTextureId {
                index: tile.texture_id.index,
                kind,
            },
            tile_id: tile.tile_id.0,
            padding: tile.padding,
            bounds: PodAtlasBounds {
                origin: [tile.bounds.origin.x.0, tile.bounds.origin.y.0],
                size: [tile.bounds.size.width.0, tile.bounds.size.height.0],
            },
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
// Blade reads grayscale as a WGSL u32, so never upload Rust bool padding.
struct PolychromeSprite {
    order: u32,
    pad: u32,
    grayscale: u32,
    opacity: f32,
    bounds: PodBounds,
    content_mask: PodBounds,
    corner_radii: PodCorners,
    tile: PodAtlasTile,
}

impl From<&ScenePolychromeSprite> for PolychromeSprite {
    fn from(sprite: &ScenePolychromeSprite) -> Self {
        Self {
            order: sprite.order,
            pad: sprite.pad,
            grayscale: u32::from(sprite.grayscale),
            opacity: sprite.opacity,
            bounds: sprite.bounds.into(),
            content_mask: sprite.content_mask.bounds.into(),
            corner_radii: PodCorners {
                top_left: sprite.corner_radii.top_left.0,
                top_right: sprite.corner_radii.top_right.0,
                bottom_right: sprite.corner_radii.bottom_right.0,
                bottom_left: sprite.corner_radii.bottom_left.0,
            },
            tile: PodAtlasTile::from(&sprite.tile),
        }
    }
}

#[derive(blade_macros::ShaderData)]
struct ShaderQuadsData {
    globals: GlobalParams,
    b_quads: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
struct ShaderShadowsData {
    globals: GlobalParams,
    b_shadows: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
struct ShaderPathRasterizationData {
    globals: GlobalParams,
    b_path_vertices: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
struct ShaderPathsData {
    globals: GlobalParams,
    t_sprite: gpu::TextureView,
    s_sprite: gpu::Sampler,
    b_path_sprites: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
struct ShaderUnderlinesData {
    globals: GlobalParams,
    b_underlines: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
struct ShaderMonoSpritesData {
    globals: GlobalParams,
    gamma_ratios: [f32; 4],
    grayscale_enhanced_contrast: f32,
    t_sprite: gpu::TextureView,
    s_sprite: gpu::Sampler,
    b_mono_sprites: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
struct ShaderPolySpritesData {
    globals: GlobalParams,
    t_sprite: gpu::TextureView,
    s_sprite: gpu::Sampler,
    b_poly_sprites: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
struct ShaderSurfacesData {
    globals: GlobalParams,
    surface_locals: SurfaceParams,
    t_y: gpu::TextureView,
    t_cb_cr: gpu::TextureView,
    s_surface: gpu::Sampler,
}

#[derive(blade_macros::ShaderData)]
struct ShaderBackdropDownsampleData {
    t_source: gpu::TextureView,
    s_source: gpu::Sampler,
}

#[derive(blade_macros::ShaderData)]
struct ShaderBackdropBlurPassData {
    pass_params: BackdropBlurPassParams,
    t_source: gpu::TextureView,
    s_source: gpu::Sampler,
}

#[derive(blade_macros::ShaderData)]
struct ShaderBackdropCompositeData {
    globals: GlobalParams,
    t_original: gpu::TextureView,
    t_blur: gpu::TextureView,
    s_source: gpu::Sampler,
    b_backdrop_blurs: gpu::BufferPiece,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[repr(C)]
struct PathSprite {
    bounds: Bounds<ScaledPixels>,
}

#[derive(Clone, Debug)]
#[repr(C)]
struct PathRasterizationVertex {
    xy_position: Point<ScaledPixels>,
    st_position: Point<f32>,
    color: Background,
    bounds: Bounds<ScaledPixels>,
}

struct BladePipelines {
    quads: gpu::RenderPipeline,
    shadows: gpu::RenderPipeline,
    path_rasterization: gpu::RenderPipeline,
    paths: gpu::RenderPipeline,
    underlines: gpu::RenderPipeline,
    mono_sprites: gpu::RenderPipeline,
    poly_sprites: gpu::RenderPipeline,
    surfaces: gpu::RenderPipeline,
    backdrop_downsample: gpu::RenderPipeline,
    backdrop_blur: gpu::RenderPipeline,
    backdrop_composite: gpu::RenderPipeline,
}

impl BladePipelines {
    fn new(gpu: &gpu::Context, surface_info: gpu::SurfaceInfo, path_sample_count: u32) -> Self {
        use gpu::ShaderData as _;

        log::info!(
            "Initializing Blade pipelines for surface {:?}",
            surface_info
        );
        let shader = gpu.create_shader(gpu::ShaderDesc {
            source: include_str!("shaders.wgsl"),
        });
        shader.check_struct_size::<GlobalParams>();
        shader.check_struct_size::<SurfaceParams>();
        shader.check_struct_size::<Quad>();
        shader.check_struct_size::<Shadow>();
        shader.check_struct_size::<PathRasterizationVertex>();
        shader.check_struct_size::<PathSprite>();
        shader.check_struct_size::<Underline>();
        shader.check_struct_size::<MonochromeSprite>();
        shader.check_struct_size::<PolychromeSprite>();
        shader.check_struct_size::<BackdropBlur>();
        shader.check_struct_size::<BackdropBlurPassParams>();

        // See https://apoorvaj.io/alpha-compositing-opengl-blending-and-premultiplied-alpha/
        let blend_mode = match surface_info.alpha {
            gpu::AlphaMode::Ignored => gpu::BlendState::ALPHA_BLENDING,
            gpu::AlphaMode::PreMultiplied => gpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
            gpu::AlphaMode::PostMultiplied => gpu::BlendState::ALPHA_BLENDING,
        };
        let color_targets = &[gpu::ColorTargetState {
            format: surface_info.format,
            blend: Some(blend_mode),
            write_mask: gpu::ColorWrites::default(),
        }];
        let unblended_color_targets = &[gpu::ColorTargetState {
            format: surface_info.format,
            blend: None,
            write_mask: gpu::ColorWrites::default(),
        }];

        Self {
            quads: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "quads",
                data_layouts: &[&ShaderQuadsData::layout()],
                vertex: shader.at("vs_quad"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_quad")),
                color_targets,
                multisample_state: gpu::MultisampleState::default(),
            }),
            shadows: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "shadows",
                data_layouts: &[&ShaderShadowsData::layout()],
                vertex: shader.at("vs_shadow"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_shadow")),
                color_targets,
                multisample_state: gpu::MultisampleState::default(),
            }),
            path_rasterization: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "path_rasterization",
                data_layouts: &[&ShaderPathRasterizationData::layout()],
                vertex: shader.at("vs_path_rasterization"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_path_rasterization")),
                // The original implementation was using ADDITIVE blende mode,
                // I don't know why
                // color_targets: &[gpu::ColorTargetState {
                //     format: PATH_TEXTURE_FORMAT,
                //     blend: Some(gpu::BlendState::ADDITIVE),
                //     write_mask: gpu::ColorWrites::default(),
                // }],
                color_targets: &[gpu::ColorTargetState {
                    format: surface_info.format,
                    blend: Some(gpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: gpu::ColorWrites::default(),
                }],
                multisample_state: gpu::MultisampleState {
                    sample_count: path_sample_count,
                    ..Default::default()
                },
            }),
            paths: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "paths",
                data_layouts: &[&ShaderPathsData::layout()],
                vertex: shader.at("vs_path"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_path")),
                color_targets: &[gpu::ColorTargetState {
                    format: surface_info.format,
                    blend: Some(gpu::BlendState {
                        color: gpu::BlendComponent::OVER,
                        alpha: gpu::BlendComponent::ADDITIVE,
                    }),
                    write_mask: gpu::ColorWrites::default(),
                }],
                multisample_state: gpu::MultisampleState::default(),
            }),
            underlines: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "underlines",
                data_layouts: &[&ShaderUnderlinesData::layout()],
                vertex: shader.at("vs_underline"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_underline")),
                color_targets,
                multisample_state: gpu::MultisampleState::default(),
            }),
            mono_sprites: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "mono-sprites",
                data_layouts: &[&ShaderMonoSpritesData::layout()],
                vertex: shader.at("vs_mono_sprite"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_mono_sprite")),
                color_targets,
                multisample_state: gpu::MultisampleState::default(),
            }),
            poly_sprites: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "poly-sprites",
                data_layouts: &[&ShaderPolySpritesData::layout()],
                vertex: shader.at("vs_poly_sprite"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_poly_sprite")),
                color_targets,
                multisample_state: gpu::MultisampleState::default(),
            }),
            surfaces: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "surfaces",
                data_layouts: &[&ShaderSurfacesData::layout()],
                vertex: shader.at("vs_surface"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_surface")),
                color_targets,
                multisample_state: gpu::MultisampleState::default(),
            }),
            backdrop_downsample: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "backdrop_downsample",
                data_layouts: &[&ShaderBackdropDownsampleData::layout()],
                vertex: shader.at("vs_backdrop_fullscreen"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_backdrop_downsample")),
                color_targets: unblended_color_targets,
                multisample_state: gpu::MultisampleState::default(),
            }),
            backdrop_blur: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "backdrop_blur",
                data_layouts: &[&ShaderBackdropBlurPassData::layout()],
                vertex: shader.at("vs_backdrop_fullscreen"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_backdrop_blur_pass")),
                color_targets: unblended_color_targets,
                multisample_state: gpu::MultisampleState::default(),
            }),
            backdrop_composite: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "backdrop_composite",
                data_layouts: &[&ShaderBackdropCompositeData::layout()],
                vertex: shader.at("vs_backdrop_composite"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_backdrop_composite")),
                color_targets: unblended_color_targets,
                multisample_state: gpu::MultisampleState::default(),
            }),
        }
    }

    fn destroy(&mut self, gpu: &gpu::Context) {
        gpu.destroy_render_pipeline(&mut self.quads);
        gpu.destroy_render_pipeline(&mut self.shadows);
        gpu.destroy_render_pipeline(&mut self.path_rasterization);
        gpu.destroy_render_pipeline(&mut self.paths);
        gpu.destroy_render_pipeline(&mut self.underlines);
        gpu.destroy_render_pipeline(&mut self.mono_sprites);
        gpu.destroy_render_pipeline(&mut self.poly_sprites);
        gpu.destroy_render_pipeline(&mut self.surfaces);
        gpu.destroy_render_pipeline(&mut self.backdrop_downsample);
        gpu.destroy_render_pipeline(&mut self.backdrop_blur);
        gpu.destroy_render_pipeline(&mut self.backdrop_composite);
    }
}

pub struct BladeSurfaceConfig {
    pub size: gpu::Extent,
    pub transparent: bool,
}

//Note: we could see some of these fields moved into `BladeContext`
// so that they are shared between windows. E.g. `pipelines`.
// But that is complicated by the fact that pipelines depend on
// the format and alpha mode.
pub struct BladeRenderer {
    gpu: Arc<gpu::Context>,
    surface: gpu::Surface,
    surface_config: gpu::SurfaceConfig,
    command_encoder: gpu::CommandEncoder,
    last_sync_point: Option<gpu::SyncPoint>,
    pipelines: BladePipelines,
    instance_belt: BufferBelt,
    atlas: Arc<BladeAtlas>,
    atlas_sampler: gpu::Sampler,
    backdrop_sampler: gpu::Sampler,
    #[cfg(target_os = "macos")]
    core_video_texture_cache: CVMetalTextureCache,
    path_intermediate_texture: gpu::Texture,
    path_intermediate_texture_view: gpu::TextureView,
    path_intermediate_msaa_texture: Option<gpu::Texture>,
    path_intermediate_msaa_texture_view: Option<gpu::TextureView>,
    backdrop_blur_textures: Option<BackdropBlurTextures>,
    backdrop_blur_cache_signature: Option<u64>,
    polychrome_sprite_scratch: Vec<PolychromeSprite>,
    rendering_parameters: RenderingParameters,
}

impl BladeRenderer {
    pub fn new<I: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle>(
        context: &BladeContext,
        window: &I,
        config: BladeSurfaceConfig,
    ) -> anyhow::Result<Self> {
        let surface_config = gpu::SurfaceConfig {
            size: config.size,
            usage: gpu::TextureUsage::TARGET | gpu::TextureUsage::COPY,
            display_sync: gpu::DisplaySync::Recent,
            color_space: gpu::ColorSpace::Srgb,
            allow_exclusive_full_screen: false,
            transparent: config.transparent,
        };
        let surface = context
            .gpu
            .create_surface_configured(window, surface_config)
            .map_err(|err| anyhow::anyhow!("Failed to create surface: {err:?}"))?;

        let command_encoder = context.gpu.create_command_encoder(gpu::CommandEncoderDesc {
            name: "main",
            buffer_count: 2,
        });
        let rendering_parameters = RenderingParameters::from_env(context);
        let pipelines = BladePipelines::new(
            &context.gpu,
            surface.info(),
            rendering_parameters.path_sample_count,
        );
        let instance_belt = BufferBelt::new(BufferBeltDescriptor {
            memory: gpu::Memory::Shared,
            min_chunk_size: 0x1000,
            alignment: 0x40, // Vulkan `minStorageBufferOffsetAlignment` on Intel Xe
        });
        let atlas = Arc::new(BladeAtlas::new(&context.gpu));
        let atlas_sampler = context.gpu.create_sampler(gpu::SamplerDesc {
            name: "path rasterization sampler",
            mag_filter: gpu::FilterMode::Linear,
            min_filter: gpu::FilterMode::Linear,
            ..Default::default()
        });
        let backdrop_sampler = context.gpu.create_sampler(gpu::SamplerDesc {
            name: "backdrop blur sampler",
            mag_filter: gpu::FilterMode::Linear,
            min_filter: gpu::FilterMode::Linear,
            address_modes: [gpu::AddressMode::ClampToEdge; 3],
            ..Default::default()
        });

        let (path_intermediate_texture, path_intermediate_texture_view) =
            create_path_intermediate_texture(
                &context.gpu,
                surface.info().format,
                config.size.width,
                config.size.height,
            );
        let (path_intermediate_msaa_texture, path_intermediate_msaa_texture_view) =
            create_msaa_texture_if_needed(
                &context.gpu,
                surface.info().format,
                config.size.width,
                config.size.height,
                rendering_parameters.path_sample_count,
            )
            .unzip();

        #[cfg(target_os = "macos")]
        let core_video_texture_cache = unsafe {
            CVMetalTextureCache::new(
                objc2::rc::Retained::as_ptr(&context.gpu.metal_device()) as *mut _
            )
            .unwrap()
        };

        Ok(Self {
            gpu: Arc::clone(&context.gpu),
            surface,
            surface_config,
            command_encoder,
            last_sync_point: None,
            pipelines,
            instance_belt,
            atlas,
            atlas_sampler,
            backdrop_sampler,
            #[cfg(target_os = "macos")]
            core_video_texture_cache,
            path_intermediate_texture,
            path_intermediate_texture_view,
            path_intermediate_msaa_texture,
            path_intermediate_msaa_texture_view,
            backdrop_blur_textures: None,
            backdrop_blur_cache_signature: None,
            polychrome_sprite_scratch: Vec::new(),
            rendering_parameters,
        })
    }

    fn wait_for_gpu(&mut self) {
        if let Some(last_sp) = self.last_sync_point.take()
            && !self.gpu.wait_for(&last_sp, MAX_FRAME_TIME_MS)
        {
            log::error!("GPU hung");
            #[cfg(target_os = "linux")]
            if self.gpu.device_information().driver_name == "radv" {
                log::error!(
                    "there's a known bug with amdgpu/radv, try setting ZED_PATH_SAMPLE_COUNT=0 as a workaround"
                );
                log::error!(
                    "if that helps you're running into https://github.com/zed-industries/zed/issues/26143"
                );
            }
            log::error!(
                "your device information is: {:?}",
                self.gpu.device_information()
            );
            while !self.gpu.wait_for(&last_sp, MAX_FRAME_TIME_MS) {}
        }
    }

    pub fn update_drawable_size(&mut self, size: Size<DevicePixels>) {
        self.update_drawable_size_impl(size, false);
    }

    /// Like `update_drawable_size` but skips the check that the size has changed. This is useful in
    /// cases like restoring a window from minimization where the size is the same but the
    /// renderer's swap chain needs to be recreated.
    #[cfg_attr(
        any(target_os = "macos", target_os = "linux", target_os = "freebsd"),
        allow(dead_code)
    )]
    pub fn update_drawable_size_even_if_unchanged(&mut self, size: Size<DevicePixels>) {
        self.update_drawable_size_impl(size, true);
    }

    fn update_drawable_size_impl(&mut self, size: Size<DevicePixels>, always_resize: bool) {
        let gpu_size = gpu::Extent {
            width: size.width.0 as u32,
            height: size.height.0 as u32,
            depth: 1,
        };

        if always_resize || gpu_size != self.surface_config.size {
            self.wait_for_gpu();
            self.surface_config.size = gpu_size;
            self.gpu
                .reconfigure_surface(&mut self.surface, self.surface_config);
            self.gpu.destroy_texture(self.path_intermediate_texture);
            self.gpu
                .destroy_texture_view(self.path_intermediate_texture_view);
            if let Some(msaa_texture) = self.path_intermediate_msaa_texture {
                self.gpu.destroy_texture(msaa_texture);
            }
            if let Some(msaa_view) = self.path_intermediate_msaa_texture_view {
                self.gpu.destroy_texture_view(msaa_view);
            }
            self.destroy_backdrop_blur_textures();
            let (path_intermediate_texture, path_intermediate_texture_view) =
                create_path_intermediate_texture(
                    &self.gpu,
                    self.surface.info().format,
                    gpu_size.width,
                    gpu_size.height,
                );
            self.path_intermediate_texture = path_intermediate_texture;
            self.path_intermediate_texture_view = path_intermediate_texture_view;
            let (path_intermediate_msaa_texture, path_intermediate_msaa_texture_view) =
                create_msaa_texture_if_needed(
                    &self.gpu,
                    self.surface.info().format,
                    gpu_size.width,
                    gpu_size.height,
                    self.rendering_parameters.path_sample_count,
                )
                .unzip();
            self.path_intermediate_msaa_texture = path_intermediate_msaa_texture;
            self.path_intermediate_msaa_texture_view = path_intermediate_msaa_texture_view;
        }
    }

    pub fn update_transparency(&mut self, transparent: bool) {
        if transparent != self.surface_config.transparent {
            self.wait_for_gpu();
            self.surface_config.transparent = transparent;
            self.gpu
                .reconfigure_surface(&mut self.surface, self.surface_config);
            self.pipelines.destroy(&self.gpu);
            self.pipelines = BladePipelines::new(
                &self.gpu,
                self.surface.info(),
                self.rendering_parameters.path_sample_count,
            );
        }
    }

    #[cfg_attr(
        any(target_os = "macos", feature = "wayland", target_os = "windows"),
        allow(dead_code)
    )]
    pub fn viewport_size(&self) -> gpu::Extent {
        self.surface_config.size
    }

    pub fn sprite_atlas(&self) -> &Arc<BladeAtlas> {
        &self.atlas
    }

    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn gpu_specs(&self) -> GpuSpecs {
        let info = self.gpu.device_information();

        GpuSpecs {
            is_software_emulated: info.is_software_emulated,
            device_name: info.device_name.clone(),
            driver_name: info.driver_name.clone(),
            driver_info: info.driver_info.clone(),
        }
    }

    #[cfg(target_os = "macos")]
    pub fn layer(&self) -> metal::MetalLayer {
        unsafe { foreign_types::ForeignType::from_ptr(self.layer_ptr()) }
    }

    #[cfg(target_os = "macos")]
    pub fn layer_ptr(&self) -> *mut metal::CAMetalLayer {
        objc2::rc::Retained::as_ptr(&self.surface.metal_layer()) as *mut _
    }

    #[profiling::function]
    fn draw_paths_to_intermediate(
        &mut self,
        paths: &[Path<ScaledPixels>],
        width: f32,
        height: f32,
    ) {
        self.command_encoder
            .init_texture(self.path_intermediate_texture);
        if let Some(msaa_texture) = self.path_intermediate_msaa_texture {
            self.command_encoder.init_texture(msaa_texture);
        }

        let target = if let Some(msaa_view) = self.path_intermediate_msaa_texture_view {
            gpu::RenderTarget {
                view: msaa_view,
                init_op: gpu::InitOp::Clear(gpu::TextureColor::TransparentBlack),
                finish_op: gpu::FinishOp::ResolveTo(self.path_intermediate_texture_view),
            }
        } else {
            gpu::RenderTarget {
                view: self.path_intermediate_texture_view,
                init_op: gpu::InitOp::Clear(gpu::TextureColor::TransparentBlack),
                finish_op: gpu::FinishOp::Store,
            }
        };
        if let mut pass = self.command_encoder.render(
            "rasterize paths",
            gpu::RenderTargetSet {
                colors: &[target],
                depth_stencil: None,
            },
        ) {
            let globals = GlobalParams {
                viewport_size: [width, height],
                premultiplied_alpha: 0,
                pad: 0,
            };
            let mut encoder = pass.with(&self.pipelines.path_rasterization);

            let mut vertices = Vec::new();
            for path in paths {
                vertices.extend(path.vertices.iter().map(|v| PathRasterizationVertex {
                    xy_position: v.xy_position,
                    st_position: v.st_position,
                    color: path.color,
                    bounds: path.clipped_bounds(),
                }));
            }
            let vertex_buf = unsafe { self.instance_belt.alloc_typed(&vertices, &self.gpu) };
            encoder.bind(
                0,
                &ShaderPathRasterizationData {
                    globals,
                    b_path_vertices: vertex_buf,
                },
            );
            encoder.draw(0, vertices.len() as u32, 0, 1);
        }
    }

    fn destroy_backdrop_blur_textures(&mut self) {
        self.backdrop_blur_cache_signature = None;
        let Some(textures) = self.backdrop_blur_textures.take() else {
            return;
        };
        self.gpu
            .destroy_texture_view(textures.snapshot_texture_view);
        self.gpu.destroy_texture(textures.snapshot_texture);
        self.gpu
            .destroy_texture_view(textures.downsample_texture_view);
        self.gpu.destroy_texture(textures.downsample_texture);
        self.gpu
            .destroy_texture_view(textures.blur_temp_texture_view);
        self.gpu.destroy_texture(textures.blur_temp_texture);
        self.gpu.destroy_texture_view(textures.blur_texture_view);
        self.gpu.destroy_texture(textures.blur_texture);
    }

    fn ensure_backdrop_blur_textures(&mut self) -> &BackdropBlurTextures {
        if self.backdrop_blur_textures.is_none() {
            self.backdrop_blur_textures = Some(create_backdrop_blur_textures(
                &self.gpu,
                self.surface.info().format,
                self.surface_config.size.width,
                self.surface_config.size.height,
            ));
        }
        self.backdrop_blur_textures.as_ref().unwrap()
    }

    fn prepare_backdrop_blur_texture(
        &mut self,
        backdrop_blurs: &[SceneBackdropBlur],
        frame_texture: gpu::Texture,
    ) {
        let size = self.surface_config.size;
        let Some(work_area) = backdrop_blur_work_area(
            backdrop_blurs,
            size.width as i32,
            size.height as i32,
            BACKDROP_BLUR_DOWNSAMPLE as f32,
        ) else {
            return;
        };
        let cache_signature = backdrop_blur_batch_signature(
            backdrop_blurs,
            size.width as i32,
            size.height as i32,
            work_area,
        );
        if self.backdrop_blur_textures.is_some()
            && self.backdrop_blur_cache_signature == Some(cache_signature)
        {
            return;
        }

        let blur_size = backdrop_blur_texture_size(size);
        let blur_radius = backdrop_blurs
            .iter()
            .map(|backdrop_blur| backdrop_blur.blur_radius.0)
            .fold(0.0, f32::max)
            / BACKDROP_BLUR_DOWNSAMPLE as f32;
        let textures = self.ensure_backdrop_blur_textures();
        let snapshot_texture = textures.snapshot_texture;
        let snapshot_texture_view = textures.snapshot_texture_view;
        let downsample_texture = textures.downsample_texture;
        let downsample_texture_view = textures.downsample_texture_view;
        let blur_temp_texture = textures.blur_temp_texture;
        let blur_temp_texture_view = textures.blur_temp_texture_view;
        let blur_texture = textures.blur_texture;
        let blur_texture_view = textures.blur_texture_view;

        self.command_encoder.init_texture(snapshot_texture);
        {
            let mut transfer = self.command_encoder.transfer("copy backdrop snapshot");
            transfer.copy_texture_to_texture(
                backdrop_texture_piece(frame_texture, work_area.source_x, work_area.source_y),
                backdrop_texture_piece(snapshot_texture, work_area.source_x, work_area.source_y),
                gpu::Extent {
                    width: work_area.source_width,
                    height: work_area.source_height,
                    depth: 1,
                },
            );
        }

        self.command_encoder.init_texture(downsample_texture);
        if let mut pass = self.command_encoder.render(
            "downsample backdrop",
            gpu::RenderTargetSet {
                colors: &[gpu::RenderTarget {
                    view: downsample_texture_view,
                    init_op: gpu::InitOp::DontCare,
                    finish_op: gpu::FinishOp::Store,
                }],
                depth_stencil: None,
            },
        ) {
            let mut encoder = pass.with(&self.pipelines.backdrop_downsample);
            set_backdrop_blur_scissor(&mut encoder, work_area);
            encoder.bind(
                0,
                &ShaderBackdropDownsampleData {
                    t_source: snapshot_texture_view,
                    s_source: self.backdrop_sampler,
                },
            );
            encoder.draw(0, 4, 0, 1);
        }

        self.draw_backdrop_blur_pass(
            downsample_texture_view,
            blur_temp_texture,
            blur_temp_texture_view,
            blur_size,
            work_area,
            [1.0, 0.0],
            blur_radius,
        );
        self.draw_backdrop_blur_pass(
            blur_temp_texture_view,
            blur_texture,
            blur_texture_view,
            blur_size,
            work_area,
            [0.0, 1.0],
            blur_radius,
        );

        // Reuse stable modal backdrops instead of recapturing and blurring the
        // same framebuffer on every redraw.
        self.backdrop_blur_cache_signature = Some(cache_signature);
    }

    fn draw_backdrop_blur_pass(
        &mut self,
        source_view: gpu::TextureView,
        destination_texture: gpu::Texture,
        destination_view: gpu::TextureView,
        destination_size: gpu::Extent,
        work_area: crate::BackdropBlurWorkArea,
        direction: [f32; 2],
        radius: f32,
    ) {
        self.command_encoder.init_texture(destination_texture);
        if let mut pass = self.command_encoder.render(
            "blur backdrop",
            gpu::RenderTargetSet {
                colors: &[gpu::RenderTarget {
                    view: destination_view,
                    init_op: gpu::InitOp::DontCare,
                    finish_op: gpu::FinishOp::Store,
                }],
                depth_stencil: None,
            },
        ) {
            let mut encoder = pass.with(&self.pipelines.backdrop_blur);
            set_backdrop_blur_scissor(&mut encoder, work_area);
            encoder.bind(
                0,
                &ShaderBackdropBlurPassData {
                    pass_params: BackdropBlurPassParams {
                        texture_size: [
                            destination_size.width as f32,
                            destination_size.height as f32,
                        ],
                        direction,
                        radius,
                        pad_0: 0.0,
                        pad_1: 0.0,
                        pad_2: 0.0,
                    },
                    t_source: source_view,
                    s_source: self.backdrop_sampler,
                },
            );
            encoder.draw(0, 4, 0, 1);
        }
    }

    pub fn destroy(&mut self) {
        self.wait_for_gpu();
        self.atlas.destroy();
        self.gpu.destroy_sampler(self.atlas_sampler);
        self.gpu.destroy_sampler(self.backdrop_sampler);
        self.instance_belt.destroy(&self.gpu);
        self.gpu.destroy_command_encoder(&mut self.command_encoder);
        self.pipelines.destroy(&self.gpu);
        self.gpu.destroy_surface(&mut self.surface);
        self.gpu.destroy_texture(self.path_intermediate_texture);
        self.gpu
            .destroy_texture_view(self.path_intermediate_texture_view);
        if let Some(msaa_texture) = self.path_intermediate_msaa_texture {
            self.gpu.destroy_texture(msaa_texture);
        }
        if let Some(msaa_view) = self.path_intermediate_msaa_texture_view {
            self.gpu.destroy_texture_view(msaa_view);
        }
        self.destroy_backdrop_blur_textures();
    }

    pub fn draw(&mut self, scene: &Scene) {
        if scene.backdrop_blurs.is_empty() && self.backdrop_blur_textures.is_some() {
            self.wait_for_gpu();
            self.destroy_backdrop_blur_textures();
        }
        self.command_encoder.start();
        self.atlas.before_frame(&mut self.command_encoder);

        let frame = {
            profiling::scope!("acquire frame");
            self.surface.acquire_frame()
        };
        self.command_encoder.init_texture(frame.texture());

        let globals = GlobalParams {
            viewport_size: [
                self.surface_config.size.width as f32,
                self.surface_config.size.height as f32,
            ],
            premultiplied_alpha: match self.surface.info().alpha {
                gpu::AlphaMode::Ignored | gpu::AlphaMode::PostMultiplied => 0,
                gpu::AlphaMode::PreMultiplied => 1,
            },
            pad: 0,
        };

        let mut pass = self.command_encoder.render(
            "main",
            gpu::RenderTargetSet {
                colors: &[gpu::RenderTarget {
                    view: frame.texture_view(),
                    init_op: gpu::InitOp::Clear(gpu::TextureColor::TransparentBlack),
                    finish_op: gpu::FinishOp::Store,
                }],
                depth_stencil: None,
            },
        );

        profiling::scope!("render pass");
        for batch in scene.batches() {
            match batch {
                PrimitiveBatch::Quads(quads) => {
                    let instance_buf = unsafe { self.instance_belt.alloc_typed(quads, &self.gpu) };
                    let mut encoder = pass.with(&self.pipelines.quads);
                    encoder.bind(
                        0,
                        &ShaderQuadsData {
                            globals,
                            b_quads: instance_buf,
                        },
                    );
                    encoder.draw(0, 4, 0, quads.len() as u32);
                }
                PrimitiveBatch::BackdropBlurs(backdrop_blurs) => {
                    drop(pass);
                    self.prepare_backdrop_blur_texture(backdrop_blurs, frame.texture());
                    pass = self.command_encoder.render(
                        "main",
                        gpu::RenderTargetSet {
                            colors: &[gpu::RenderTarget {
                                view: frame.texture_view(),
                                init_op: gpu::InitOp::Load,
                                finish_op: gpu::FinishOp::Store,
                            }],
                            depth_stencil: None,
                        },
                    );
                    let backdrop_blurs = backdrop_blurs
                        .iter()
                        .map(BackdropBlur::from)
                        .collect::<Vec<_>>();
                    let instance_buf =
                        unsafe { self.instance_belt.alloc_typed(&backdrop_blurs, &self.gpu) };
                    let Some(textures) = self.backdrop_blur_textures.as_ref() else {
                        continue;
                    };
                    let mut encoder = pass.with(&self.pipelines.backdrop_composite);
                    encoder.bind(
                        0,
                        &ShaderBackdropCompositeData {
                            globals,
                            t_original: textures.snapshot_texture_view,
                            t_blur: textures.blur_texture_view,
                            s_source: self.backdrop_sampler,
                            b_backdrop_blurs: instance_buf,
                        },
                    );
                    encoder.draw(0, 4, 0, backdrop_blurs.len() as u32);
                }
                PrimitiveBatch::Shadows(shadows) => {
                    let instance_buf =
                        unsafe { self.instance_belt.alloc_typed(shadows, &self.gpu) };
                    let mut encoder = pass.with(&self.pipelines.shadows);
                    encoder.bind(
                        0,
                        &ShaderShadowsData {
                            globals,
                            b_shadows: instance_buf,
                        },
                    );
                    encoder.draw(0, 4, 0, shadows.len() as u32);
                }
                PrimitiveBatch::Paths(paths) => {
                    let Some(first_path) = paths.first() else {
                        continue;
                    };
                    drop(pass);
                    self.draw_paths_to_intermediate(
                        paths,
                        self.surface_config.size.width as f32,
                        self.surface_config.size.height as f32,
                    );
                    pass = self.command_encoder.render(
                        "main",
                        gpu::RenderTargetSet {
                            colors: &[gpu::RenderTarget {
                                view: frame.texture_view(),
                                init_op: gpu::InitOp::Load,
                                finish_op: gpu::FinishOp::Store,
                            }],
                            depth_stencil: None,
                        },
                    );
                    let mut encoder = pass.with(&self.pipelines.paths);
                    // When copying paths from the intermediate texture to the drawable,
                    // each pixel must only be copied once, in case of transparent paths.
                    //
                    // If all paths have the same draw order, then their bounds are all
                    // disjoint, so we can copy each path's bounds individually. If this
                    // batch combines different draw orders, we perform a single copy
                    // for a minimal spanning rect.
                    let sprites = if paths.last().unwrap().order == first_path.order {
                        paths
                            .iter()
                            .map(|path| PathSprite {
                                bounds: path.clipped_bounds(),
                            })
                            .collect()
                    } else {
                        let mut bounds = first_path.clipped_bounds();
                        for path in paths.iter().skip(1) {
                            bounds = bounds.union(&path.clipped_bounds());
                        }
                        vec![PathSprite { bounds }]
                    };
                    let instance_buf =
                        unsafe { self.instance_belt.alloc_typed(&sprites, &self.gpu) };
                    encoder.bind(
                        0,
                        &ShaderPathsData {
                            globals,
                            t_sprite: self.path_intermediate_texture_view,
                            s_sprite: self.atlas_sampler,
                            b_path_sprites: instance_buf,
                        },
                    );
                    encoder.draw(0, 4, 0, sprites.len() as u32);
                }
                PrimitiveBatch::Underlines(underlines) => {
                    let instance_buf =
                        unsafe { self.instance_belt.alloc_typed(underlines, &self.gpu) };
                    let mut encoder = pass.with(&self.pipelines.underlines);
                    encoder.bind(
                        0,
                        &ShaderUnderlinesData {
                            globals,
                            b_underlines: instance_buf,
                        },
                    );
                    encoder.draw(0, 4, 0, underlines.len() as u32);
                }
                PrimitiveBatch::MonochromeSprites {
                    texture_id,
                    sprites,
                } => {
                    let tex_info = self.atlas.get_texture_info(texture_id);
                    let instance_buf =
                        unsafe { self.instance_belt.alloc_typed(sprites, &self.gpu) };
                    let mut encoder = pass.with(&self.pipelines.mono_sprites);
                    encoder.bind(
                        0,
                        &ShaderMonoSpritesData {
                            globals,
                            gamma_ratios: self.rendering_parameters.gamma_ratios,
                            grayscale_enhanced_contrast: self
                                .rendering_parameters
                                .grayscale_enhanced_contrast,
                            t_sprite: tex_info.raw_view,
                            s_sprite: self.atlas_sampler,
                            b_mono_sprites: instance_buf,
                        },
                    );
                    encoder.draw(0, 4, 0, sprites.len() as u32);
                }
                PrimitiveBatch::PolychromeSprites {
                    texture_id,
                    sprites,
                } => {
                    let tex_info = self.atlas.get_texture_info(texture_id);
                    // The upload belt copies the POD slice immediately, so
                    // reuse this CPU conversion buffer across image batches.
                    self.polychrome_sprite_scratch.clear();
                    self.polychrome_sprite_scratch
                        .extend(sprites.iter().map(PolychromeSprite::from));
                    let instance_buf = self
                        .instance_belt
                        .alloc_pod(&self.polychrome_sprite_scratch, &self.gpu);
                    let mut encoder = pass.with(&self.pipelines.poly_sprites);
                    encoder.bind(
                        0,
                        &ShaderPolySpritesData {
                            globals,
                            t_sprite: tex_info.raw_view,
                            s_sprite: self.atlas_sampler,
                            b_poly_sprites: instance_buf,
                        },
                    );
                    encoder.draw(0, 4, 0, self.polychrome_sprite_scratch.len() as u32);
                }
                PrimitiveBatch::Surfaces(surfaces) => {
                    let mut _encoder = pass.with(&self.pipelines.surfaces);

                    for surface in surfaces {
                        #[cfg(not(target_os = "macos"))]
                        {
                            let _ = surface;
                            continue;
                        };

                        #[cfg(target_os = "macos")]
                        {
                            let (t_y, t_cb_cr) = unsafe {
                                use core_foundation::base::TCFType as _;
                                use std::ptr;

                                assert_eq!(
                                        surface.image_buffer.get_pixel_format(),
                                        core_video::pixel_buffer::kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
                                    );

                                let y_texture = self
                                    .core_video_texture_cache
                                    .create_texture_from_image(
                                        surface.image_buffer.as_concrete_TypeRef(),
                                        ptr::null(),
                                        metal::MTLPixelFormat::R8Unorm,
                                        surface.image_buffer.get_width_of_plane(0),
                                        surface.image_buffer.get_height_of_plane(0),
                                        0,
                                    )
                                    .unwrap();
                                let cb_cr_texture = self
                                    .core_video_texture_cache
                                    .create_texture_from_image(
                                        surface.image_buffer.as_concrete_TypeRef(),
                                        ptr::null(),
                                        metal::MTLPixelFormat::RG8Unorm,
                                        surface.image_buffer.get_width_of_plane(1),
                                        surface.image_buffer.get_height_of_plane(1),
                                        1,
                                    )
                                    .unwrap();
                                (
                                    gpu::TextureView::from_metal_texture(
                                        &objc2::rc::Retained::retain(
                                            foreign_types::ForeignTypeRef::as_ptr(
                                                y_texture.as_texture_ref(),
                                            )
                                                as *mut objc2::runtime::ProtocolObject<
                                                    dyn objc2_metal::MTLTexture,
                                                >,
                                        )
                                        .unwrap(),
                                        gpu::TexelAspects::COLOR,
                                    ),
                                    gpu::TextureView::from_metal_texture(
                                        &objc2::rc::Retained::retain(
                                            foreign_types::ForeignTypeRef::as_ptr(
                                                cb_cr_texture.as_texture_ref(),
                                            )
                                                as *mut objc2::runtime::ProtocolObject<
                                                    dyn objc2_metal::MTLTexture,
                                                >,
                                        )
                                        .unwrap(),
                                        gpu::TexelAspects::COLOR,
                                    ),
                                )
                            };

                            _encoder.bind(
                                0,
                                &ShaderSurfacesData {
                                    globals,
                                    surface_locals: SurfaceParams {
                                        bounds: surface.bounds.into(),
                                        content_mask: surface.content_mask.bounds.into(),
                                    },
                                    t_y,
                                    t_cb_cr,
                                    s_surface: self.atlas_sampler,
                                },
                            );

                            _encoder.draw(0, 4, 0, 1);
                        }
                    }
                }
            }
        }
        drop(pass);

        self.command_encoder.present(frame);
        let sync_point = self.gpu.submit(&mut self.command_encoder);

        profiling::scope!("finish");
        self.instance_belt.flush(&sync_point);
        self.atlas.after_frame(&sync_point);

        self.wait_for_gpu();
        self.last_sync_point = Some(sync_point);
    }
}

fn create_path_intermediate_texture(
    gpu: &gpu::Context,
    format: gpu::TextureFormat,
    width: u32,
    height: u32,
) -> (gpu::Texture, gpu::TextureView) {
    let texture = gpu.create_texture(gpu::TextureDesc {
        name: "path intermediate",
        format,
        size: gpu::Extent {
            width,
            height,
            depth: 1,
        },
        array_layer_count: 1,
        mip_level_count: 1,
        sample_count: 1,
        dimension: gpu::TextureDimension::D2,
        usage: gpu::TextureUsage::COPY | gpu::TextureUsage::RESOURCE | gpu::TextureUsage::TARGET,
        external: None,
    });
    let texture_view = gpu.create_texture_view(
        texture,
        gpu::TextureViewDesc {
            name: "path intermediate view",
            format,
            dimension: gpu::ViewDimension::D2,
            subresources: &Default::default(),
        },
    );
    (texture, texture_view)
}

struct BackdropBlurTextures {
    snapshot_texture: gpu::Texture,
    snapshot_texture_view: gpu::TextureView,
    downsample_texture: gpu::Texture,
    downsample_texture_view: gpu::TextureView,
    blur_temp_texture: gpu::Texture,
    blur_temp_texture_view: gpu::TextureView,
    blur_texture: gpu::Texture,
    blur_texture_view: gpu::TextureView,
}

fn create_backdrop_blur_textures(
    gpu: &gpu::Context,
    format: gpu::TextureFormat,
    width: u32,
    height: u32,
) -> BackdropBlurTextures {
    let (snapshot_texture, snapshot_texture_view) =
        create_backdrop_texture(gpu, "backdrop snapshot", format, width, height);
    let blur_size = backdrop_blur_texture_size(gpu::Extent {
        width,
        height,
        depth: 1,
    });
    let (downsample_texture, downsample_texture_view) = create_backdrop_texture(
        gpu,
        "backdrop downsample",
        format,
        blur_size.width,
        blur_size.height,
    );
    let (blur_temp_texture, blur_temp_texture_view) = create_backdrop_texture(
        gpu,
        "backdrop blur temp",
        format,
        blur_size.width,
        blur_size.height,
    );
    let (blur_texture, blur_texture_view) = create_backdrop_texture(
        gpu,
        "backdrop blur",
        format,
        blur_size.width,
        blur_size.height,
    );

    BackdropBlurTextures {
        snapshot_texture,
        snapshot_texture_view,
        downsample_texture,
        downsample_texture_view,
        blur_temp_texture,
        blur_temp_texture_view,
        blur_texture,
        blur_texture_view,
    }
}

fn create_backdrop_texture(
    gpu: &gpu::Context,
    name: &'static str,
    format: gpu::TextureFormat,
    width: u32,
    height: u32,
) -> (gpu::Texture, gpu::TextureView) {
    let texture = gpu.create_texture(gpu::TextureDesc {
        name,
        format,
        size: gpu::Extent {
            width: width.max(1),
            height: height.max(1),
            depth: 1,
        },
        array_layer_count: 1,
        mip_level_count: 1,
        sample_count: 1,
        dimension: gpu::TextureDimension::D2,
        usage: gpu::TextureUsage::COPY | gpu::TextureUsage::RESOURCE | gpu::TextureUsage::TARGET,
        external: None,
    });
    let texture_view = gpu.create_texture_view(
        texture,
        gpu::TextureViewDesc {
            name,
            format,
            dimension: gpu::ViewDimension::D2,
            subresources: &Default::default(),
        },
    );
    (texture, texture_view)
}

fn backdrop_blur_texture_size(size: gpu::Extent) -> gpu::Extent {
    gpu::Extent {
        width: (size.width + BACKDROP_BLUR_DOWNSAMPLE - 1) / BACKDROP_BLUR_DOWNSAMPLE,
        height: (size.height + BACKDROP_BLUR_DOWNSAMPLE - 1) / BACKDROP_BLUR_DOWNSAMPLE,
        depth: 1,
    }
}

fn backdrop_texture_piece(texture: gpu::Texture, x: u32, y: u32) -> gpu::TexturePiece {
    gpu::TexturePiece {
        origin: [x, y, 0],
        ..texture.into()
    }
}

fn set_backdrop_blur_scissor(
    encoder: &mut impl RenderEncoder,
    work_area: crate::BackdropBlurWorkArea,
) {
    encoder.set_scissor_rect(&gpu::ScissorRect {
        x: work_area.blur_x as i32,
        y: work_area.blur_y as i32,
        w: work_area.blur_width,
        h: work_area.blur_height,
    });
}

fn create_msaa_texture_if_needed(
    gpu: &gpu::Context,
    format: gpu::TextureFormat,
    width: u32,
    height: u32,
    sample_count: u32,
) -> Option<(gpu::Texture, gpu::TextureView)> {
    if sample_count <= 1 {
        return None;
    }
    let texture_msaa = gpu.create_texture(gpu::TextureDesc {
        name: "path intermediate msaa",
        format,
        size: gpu::Extent {
            width,
            height,
            depth: 1,
        },
        array_layer_count: 1,
        mip_level_count: 1,
        sample_count,
        dimension: gpu::TextureDimension::D2,
        usage: gpu::TextureUsage::TARGET,
        external: None,
    });
    let texture_view_msaa = gpu.create_texture_view(
        texture_msaa,
        gpu::TextureViewDesc {
            name: "path intermediate msaa view",
            format,
            dimension: gpu::ViewDimension::D2,
            subresources: &Default::default(),
        },
    );

    Some((texture_msaa, texture_view_msaa))
}

/// A set of parameters that can be set using a corresponding environment variable.
struct RenderingParameters {
    // Env var: ZED_PATH_SAMPLE_COUNT
    // workaround for https://github.com/zed-industries/zed/issues/26143
    path_sample_count: u32,

    // Env var: ZED_FONTS_GAMMA
    // Allowed range [1.0, 2.2], other values are clipped
    // Default: 1.8
    gamma_ratios: [f32; 4],
    // Env var: ZED_FONTS_GRAYSCALE_ENHANCED_CONTRAST
    // Allowed range: [0.0, ..), other values are clipped
    // Default: 1.0
    grayscale_enhanced_contrast: f32,
}

impl RenderingParameters {
    fn from_env(context: &BladeContext) -> Self {
        use std::env;

        let path_sample_count = env::var("ZED_PATH_SAMPLE_COUNT")
            .ok()
            .and_then(|v| v.parse().ok())
            .or_else(|| {
                [4, 2, 1]
                    .into_iter()
                    .find(|&n| (context.gpu.capabilities().sample_count_mask & n) != 0)
            })
            .unwrap_or(1);
        let gamma = env::var("ZED_FONTS_GAMMA")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.8_f32)
            .clamp(1.0, 2.2);
        let gamma_ratios = Self::get_gamma_ratios(gamma);
        let grayscale_enhanced_contrast = env::var("ZED_FONTS_GRAYSCALE_ENHANCED_CONTRAST")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.0_f32)
            .max(0.0);

        Self {
            path_sample_count,
            gamma_ratios,
            grayscale_enhanced_contrast,
        }
    }

    // Gamma ratios for brightening/darkening edges for better contrast
    // https://github.com/microsoft/terminal/blob/1283c0f5b99a2961673249fa77c6b986efb5086c/src/renderer/atlas/dwrite.cpp#L50
    fn get_gamma_ratios(gamma: f32) -> [f32; 4] {
        const GAMMA_INCORRECT_TARGET_RATIOS: [[f32; 4]; 13] = [
            [0.0000 / 4.0, 0.0000 / 4.0, 0.0000 / 4.0, 0.0000 / 4.0], // gamma = 1.0
            [0.0166 / 4.0, -0.0807 / 4.0, 0.2227 / 4.0, -0.0751 / 4.0], // gamma = 1.1
            [0.0350 / 4.0, -0.1760 / 4.0, 0.4325 / 4.0, -0.1370 / 4.0], // gamma = 1.2
            [0.0543 / 4.0, -0.2821 / 4.0, 0.6302 / 4.0, -0.1876 / 4.0], // gamma = 1.3
            [0.0739 / 4.0, -0.3963 / 4.0, 0.8167 / 4.0, -0.2287 / 4.0], // gamma = 1.4
            [0.0933 / 4.0, -0.5161 / 4.0, 0.9926 / 4.0, -0.2616 / 4.0], // gamma = 1.5
            [0.1121 / 4.0, -0.6395 / 4.0, 1.1588 / 4.0, -0.2877 / 4.0], // gamma = 1.6
            [0.1300 / 4.0, -0.7649 / 4.0, 1.3159 / 4.0, -0.3080 / 4.0], // gamma = 1.7
            [0.1469 / 4.0, -0.8911 / 4.0, 1.4644 / 4.0, -0.3234 / 4.0], // gamma = 1.8
            [0.1627 / 4.0, -1.0170 / 4.0, 1.6051 / 4.0, -0.3347 / 4.0], // gamma = 1.9
            [0.1773 / 4.0, -1.1420 / 4.0, 1.7385 / 4.0, -0.3426 / 4.0], // gamma = 2.0
            [0.1908 / 4.0, -1.2652 / 4.0, 1.8650 / 4.0, -0.3476 / 4.0], // gamma = 2.1
            [0.2031 / 4.0, -1.3864 / 4.0, 1.9851 / 4.0, -0.3501 / 4.0], // gamma = 2.2
        ];

        const NORM13: f32 = ((0x10000 as f64) / (255.0 * 255.0) * 4.0) as f32;
        const NORM24: f32 = ((0x100 as f64) / (255.0) * 4.0) as f32;

        let index = ((gamma * 10.0).round() as usize).clamp(10, 22) - 10;
        let ratios = GAMMA_INCORRECT_TARGET_RATIOS[index];

        [
            ratios[0] * NORM13,
            ratios[1] * NORM24,
            ratios[2] * NORM13,
            ratios[3] * NORM24,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AtlasTextureId, BackgroundTag, BorderStyle, ColorSpace, ContentMask, Corners, Edges, Hsla,
        LinearColorStop, TileId, TransformationMatrix,
    };
    use std::collections::BTreeSet;

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

    fn assert_all_buffer_structs_are_audited(module: &naga::Module) {
        let mut actual = BTreeSet::new();
        for (_, variable) in module.global_variables.iter() {
            if matches!(
                variable.space,
                naga::AddressSpace::Uniform | naga::AddressSpace::Storage { .. }
            ) {
                collect_buffer_structs(module, variable.ty, &mut actual);
            }
        }
        let expected = BTreeSet::from_iter(
            [
                "AtlasBounds",
                "AtlasTextureId",
                "AtlasTile",
                "BackdropBlur",
                "BackdropBlurPassParams",
                "Background",
                "Bounds",
                "Corners",
                "Edges",
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
            ]
            .map(str::to_string),
        );

        assert_eq!(
            actual, expected,
            "the Blade buffer layout audit is incomplete"
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
        let naga::TypeInner::Array { base, stride, .. } = module.types[variable.ty].inner else {
            panic!("shader storage '{global_name}' is not an array");
        };
        assert_eq!(module.types[base].name.as_deref(), Some(item_name));
        assert_eq!(
            stride as usize,
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

    fn assert_background_layout(module: &naga::Module) {
        // Background's final pad is private to the color module. repr(C) places
        // it directly after the fixed color-stop array, which supplies its
        // exact host offset without widening that field's visibility.
        let pad_offset =
            std::mem::offset_of!(Background, colors) + std::mem::size_of::<[LinearColorStop; 2]>();
        assert_shader_struct_layout::<Background>(
            module,
            "Background",
            &[
                ("tag", std::mem::offset_of!(Background, tag)),
                ("color_space", std::mem::offset_of!(Background, color_space)),
                ("solid", std::mem::offset_of!(Background, solid)),
                (
                    "gradient_angle_or_pattern_height",
                    std::mem::offset_of!(Background, gradient_angle_or_pattern_height),
                ),
                ("colors", std::mem::offset_of!(Background, colors)),
                ("pad", pad_offset),
            ],
        );
    }

    #[test]
    fn blade_shader_buffer_contract_matches_rust() {
        let module = naga::front::wgsl::parse_str(include_str!("shaders.wgsl"))
            .expect("Blade WGSL should parse");
        let validation_flags =
            naga::valid::ValidationFlags::all() ^ naga::valid::ValidationFlags::BINDINGS;
        naga::valid::Validator::new(validation_flags, naga::valid::Capabilities::empty())
            .validate(&module)
            .expect("Blade WGSL should pass startup validation");

        assert_all_buffer_structs_are_audited(&module);

        // Audit every host structure reachable from a uniform or storage
        // binding, including nested structures and every field offset.
        assert_struct_layout!(&module, GlobalParams => "GlobalParams" {
            viewport_size, premultiplied_alpha, pad
        });
        assert_struct_layout!(&module, Bounds<ScaledPixels> => "Bounds" { origin, size });
        assert_struct_layout!(&module, PodBounds => "Bounds" { origin, size });
        assert_struct_layout!(&module, Corners<ScaledPixels> => "Corners" {
            top_left, top_right, bottom_right, bottom_left
        });
        assert_struct_layout!(&module, PodCorners => "Corners" {
            top_left, top_right, bottom_right, bottom_left
        });
        assert_struct_layout!(&module, Edges<ScaledPixels> => "Edges" {
            top, right, bottom, left
        });
        assert_struct_layout!(&module, Hsla => "Hsla" { h, s, l, a });
        assert_struct_layout!(&module, LinearColorStop => "LinearColorStop" {
            color, percentage
        });
        assert_background_layout(&module);
        assert_struct_layout!(&module, AtlasTextureId => "AtlasTextureId" { index, kind });
        assert_struct_layout!(&module, PodAtlasTextureId => "AtlasTextureId" { index, kind });
        assert_struct_layout!(&module, Bounds<DevicePixels> => "AtlasBounds" { origin, size });
        assert_struct_layout!(&module, PodAtlasBounds => "AtlasBounds" { origin, size });
        assert_struct_layout!(&module, AtlasTile => "AtlasTile" {
            texture_id, tile_id, padding, bounds
        });
        assert_struct_layout!(&module, PodAtlasTile => "AtlasTile" {
            texture_id, tile_id, padding, bounds
        });
        assert_struct_layout!(&module, TransformationMatrix => "TransformationMatrix" {
            rotation_scale, translation
        });
        assert_struct_layout!(&module, Quad => "Quad" {
            order, border_style, bounds, content_mask, background, border_color,
            corner_radii, border_widths
        });
        assert_struct_layout!(&module, Shadow => "Shadow" {
            order, blur_radius, bounds, corner_radii, content_mask, color
        });
        assert_struct_layout!(&module, PathRasterizationVertex => "PathRasterizationVertex" {
            xy_position, st_position, color, bounds
        });
        assert_struct_layout!(&module, PathSprite => "PathSprite" { bounds });
        assert_struct_layout!(&module, BackdropBlurPassParams => "BackdropBlurPassParams" {
            texture_size, direction, radius, pad_0, pad_1, pad_2
        });
        assert_struct_layout!(&module, BackdropBlur => "BackdropBlur" {
            bounds, content_mask, corner_radii, overlay_color
        });
        assert_struct_layout!(&module, Underline => "Underline" {
            order, pad, bounds, content_mask, color, thickness, wavy
        });
        assert_struct_layout!(&module, MonochromeSprite => "MonochromeSprite" {
            order, pad, bounds, content_mask, color, tile, transformation
        });
        assert_struct_layout!(&module, PolychromeSprite => "PolychromeSprite" {
            order, pad, grayscale, opacity, bounds, content_mask, corner_radii, tile
        });
        assert_struct_layout!(&module, SurfaceParams => "SurfaceParams" {
            bounds, content_mask
        });

        // Storage arrays must advance by the exact Rust item size, while direct
        // uniforms must expose the exact byte width uploaded by ShaderData.
        assert_storage_array_stride::<Quad>(&module, "b_quads", "Quad");
        assert_storage_array_stride::<Shadow>(&module, "b_shadows", "Shadow");
        assert_storage_array_stride::<PathRasterizationVertex>(
            &module,
            "b_path_vertices",
            "PathRasterizationVertex",
        );
        assert_storage_array_stride::<PathSprite>(&module, "b_path_sprites", "PathSprite");
        assert_storage_array_stride::<BackdropBlur>(&module, "b_backdrop_blurs", "BackdropBlur");
        assert_storage_array_stride::<Underline>(&module, "b_underlines", "Underline");
        assert_storage_array_stride::<MonochromeSprite>(
            &module,
            "b_mono_sprites",
            "MonochromeSprite",
        );
        assert_storage_array_stride::<PolychromeSprite>(
            &module,
            "b_poly_sprites",
            "PolychromeSprite",
        );
        assert_uniform_size::<GlobalParams>(&module, "globals");
        assert_uniform_size::<[f32; 4]>(&module, "gamma_ratios");
        assert_uniform_size::<f32>(&module, "grayscale_enhanced_contrast");
        assert_uniform_size::<BackdropBlurPassParams>(&module, "pass_params");
        assert_uniform_size::<SurfaceParams>(&module, "surface_locals");

        // WGSL exposes these host wrappers and C-layout enums as 32-bit
        // scalars. Check both width and discriminants used by shader branches.
        assert_eq!(std::mem::size_of::<ScaledPixels>(), 4);
        assert_eq!(std::mem::size_of::<DevicePixels>(), 4);
        assert_eq!(std::mem::size_of::<TileId>(), 4);
        assert_eq!(std::mem::size_of::<ContentMask<ScaledPixels>>(), 16);
        assert_eq!(std::mem::offset_of!(ContentMask<ScaledPixels>, bounds), 0);
        assert_eq!(std::mem::size_of::<BorderStyle>(), 4);
        assert_eq!(BorderStyle::Solid as u32, 0);
        assert_eq!(BorderStyle::Dashed as u32, 1);
        assert_eq!(std::mem::size_of::<BackgroundTag>(), 4);
        assert_eq!(BackgroundTag::Solid as u32, 0);
        assert_eq!(BackgroundTag::LinearGradient as u32, 1);
        assert_eq!(BackgroundTag::PatternSlash as u32, 2);
        assert_eq!(std::mem::size_of::<ColorSpace>(), 4);
        assert_eq!(ColorSpace::Srgb as u32, 0);
        assert_eq!(ColorSpace::Oklab as u32, 1);
        assert_eq!(std::mem::size_of::<AtlasTextureKind>(), 4);
        assert_eq!(AtlasTextureKind::Monochrome as u32, 0);
        assert_eq!(AtlasTextureKind::Polychrome as u32, 1);
    }
}
