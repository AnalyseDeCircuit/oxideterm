// OxideTerm modification: provide product-scoped GPU backend selection and deterministic fallback ordering.

#[cfg(not(target_family = "wasm"))]
use anyhow::Context as _;
#[cfg(not(target_family = "wasm"))]
use gpui_util::ResultExt;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use wgpu::TextureFormat;

#[cfg(not(target_family = "wasm"))]
const GPU_BACKEND_ENV: &str = "OXIDETERM_GPU_BACKEND";
#[cfg(not(target_family = "wasm"))]
const GPU_DEVICE_ID_ENV: &str = "OXIDETERM_GPU_DEVICE_ID";

pub struct WgpuContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    backend: WgpuBackend,
    dual_source_blending: bool,
    color_texture_format: wgpu::TextureFormat,
    errors: Arc<DeviceErrorState>,
}

/// Errors reported by wgpu's device-wide callbacks.
///
/// A device has exactly one lost callback and one uncaptured-error callback, so this
/// state is installed once per device and shared by every renderer using it. Renderers
/// keep their own handle so they can keep observing a loss after the context that
/// produced it has been dropped for recovery.
#[derive(Default)]
pub struct DeviceErrorState {
    lost: AtomicBool,
    last_error: Mutex<(u64, Option<String>)>,
}

impl DeviceErrorState {
    fn install(device: &wgpu::Device) -> Arc<Self> {
        let errors = Arc::new(Self::default());
        device.set_device_lost_callback({
            let errors = Arc::clone(&errors);
            move |reason, message| {
                log::error!("wgpu device lost: reason={reason:?}, message={message}");
                if reason != wgpu::DeviceLostReason::Destroyed {
                    errors.lost.store(true, Ordering::Relaxed);
                }
            }
        });
        device.on_uncaptured_error(Arc::new({
            let errors = Arc::clone(&errors);
            move |error| errors.record(error.to_string())
        }));
        errors
    }

    /// Returns true if the GPU device was lost (e.g., due to driver crash, suspend/resume).
    pub fn device_lost(&self) -> bool {
        self.lost.load(Ordering::Relaxed)
    }

    fn record(&self, error: String) {
        let mut last_error = self.last_error.lock();
        last_error.0 = last_error.0.wrapping_add(1);
        last_error.1 = Some(error);
    }

    /// Returns the latest error once per observer, without consuming shared state.
    /// Start each observer at zero, including after switching devices, so errors
    /// raised during construction are observed. Multiple errors between observations
    /// coalesce into the latest error; this is not an error queue.
    pub fn observe_error(&self, generation: &mut u64) -> Option<String> {
        let last_error = self.last_error.lock();
        if *generation == last_error.0 {
            return None;
        }
        *generation = last_error.0;
        last_error.1.clone()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WgpuBackend {
    BrowserWebGpu,
    Gl,
    Native(wgpu::Backend),
}
#[cfg(not(target_family = "wasm"))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum BackendPreference {
    #[default]
    Auto,
    Vulkan,
    OpenGl,
}

#[cfg(not(target_family = "wasm"))]
impl BackendPreference {
    fn enabled_backends(self) -> wgpu::Backends {
        match self {
            Self::Auto => auto_backends(),
            Self::Vulkan => wgpu::Backends::VULKAN,
            Self::OpenGl => wgpu::Backends::GL,
        }
    }
}

#[cfg(all(not(target_family = "wasm"), target_os = "windows"))]
fn auto_backends() -> wgpu::Backends {
    // Preserve the upstream optional Windows WGPU path; OxideTerm's Vulkan/GL fallback is Linux-only.
    wgpu::Backends::DX12
}

#[cfg(all(not(target_family = "wasm"), not(target_os = "windows")))]
fn auto_backends() -> wgpu::Backends {
    wgpu::Backends::VULKAN | wgpu::Backends::GL
}

#[cfg(not(target_family = "wasm"))]
fn parse_backend_preference(value: &str) -> anyhow::Result<BackendPreference> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(BackendPreference::Auto),
        "vulkan" => Ok(BackendPreference::Vulkan),
        "opengl" => Ok(BackendPreference::OpenGl),
        _ => anyhow::bail!("expected one of: auto, vulkan, opengl"),
    }
}

#[cfg(not(target_family = "wasm"))]
fn configured_backend_preference() -> BackendPreference {
    match std::env::var(GPU_BACKEND_ENV) {
        Ok(value) => match parse_backend_preference(&value) {
            Ok(preference) => preference,
            Err(error) => {
                log::warn!(
                    "Ignoring invalid {GPU_BACKEND_ENV} value {value:?}: {error}. Falling back to auto."
                );
                BackendPreference::Auto
            }
        },
        Err(std::env::VarError::NotPresent) => BackendPreference::Auto,
        Err(error) => {
            log::warn!("Unable to read {GPU_BACKEND_ENV}: {error}. Falling back to auto.");
            BackendPreference::Auto
        }
    }
}

#[cfg(target_family = "wasm")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WebBackendPreference {
    #[default]
    Auto,
    WebGpu,
    WebGl,
}

#[cfg(target_family = "wasm")]
pub struct PreparedWebGraphics {
    pub context: WgpuContext,
    pub surface: wgpu::Surface<'static>,
}

/// wgpu-core refuses to create a surface when neither the instance nor the surface
/// target carries a display handle, and `SurfaceTarget::Canvas` always passes `None`.
/// The WebGL2 backend never reads the handle (WebGPU bypasses wgpu-core entirely), so
/// a unit web display handle on the instance satisfies the check.
#[cfg(target_family = "wasm")]
#[derive(Debug)]
struct WebDisplaySource;

#[cfg(target_family = "wasm")]
impl raw_window_handle::HasDisplayHandle for WebDisplaySource {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Ok(raw_window_handle::DisplayHandle::web())
    }
}

#[derive(Clone, Copy)]
pub struct CompositorGpuHint {
    pub vendor_id: u32,
    pub device_id: u32,
}

#[cfg(not(target_family = "wasm"))]
fn adapter_priority(
    info: &wgpu::AdapterInfo,
    device_id_filter: Option<u32>,
    compositor_gpu: Option<&CompositorGpuHint>,
) -> (u8, u8, u8, u8) {
    // OpenGL commonly reports device zero, so PCI matching is only reliable for non-zero IDs.
    let device_known = info.device != 0;
    let user_override = match device_id_filter {
        Some(id) if device_known && info.device == id => 0,
        _ => 1,
    };
    let compositor_match = match compositor_gpu {
        Some(hint)
            if device_known && info.vendor == hint.vendor_id && info.device == hint.device_id =>
        {
            0
        }
        _ => 1,
    };
    let type_priority = match info.device_type {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::Other => 2,
        wgpu::DeviceType::VirtualGpu => 3,
        wgpu::DeviceType::Cpu => 4,
    };
    let backend_priority = match info.backend {
        wgpu::Backend::Vulkan => 0,
        _ => 1,
    };

    (
        user_override,
        compositor_match,
        type_priority,
        backend_priority,
    )
}

/// Extra wgpu features and limits that an application can request on top of
/// gpui's baseline.  Pass an instance to the platform via
/// [`gpui::App::set_gpu_requirements`] *before* opening any windows.
#[derive(Clone, Debug, Default)]
pub struct WgpuDeviceRequirements {
    /// Additional [`wgpu::Features`] to enable.  These are OR-ed with gpui's
    /// own required features.
    pub features: wgpu::Features,
    /// Additional [`wgpu::Limits`] to request.  Each field is merged by taking
    /// `max(gpui_limit, app_limit)` for upper-bound limits and
    /// `min(gpui_limit, app_limit)` for alignment/lower-bound limits.
    pub limits: wgpu::Limits,
}

impl WgpuContext {
    #[cfg(not(target_family = "wasm"))]
    pub fn new(
        instance: wgpu::Instance,
        surface: &wgpu::Surface<'_>,
        compositor_gpu: Option<CompositorGpuHint>,
        extra_requirements: Option<&WgpuDeviceRequirements>,
    ) -> anyhow::Result<Self> {
        Self::new_with_options(instance, surface, compositor_gpu, extra_requirements)
    }

    #[cfg(not(target_family = "wasm"))]
    fn new_with_options(
        instance: wgpu::Instance,
        surface: &wgpu::Surface<'_>,
        compositor_gpu: Option<CompositorGpuHint>,
        extra_requirements: Option<&WgpuDeviceRequirements>,
    ) -> anyhow::Result<Self> {
        let device_id_filter = Self::device_id_filter();

        // Select an adapter by actually testing surface configuration with the real device.
        // This is the only reliable way to determine compatibility on hybrid GPU systems.
        let (adapter, device, queue, dual_source_blending, color_texture_format) =
            gpui::block_on(Self::select_adapter_and_device(
                &instance,
                device_id_filter,
                surface,
                compositor_gpu.as_ref(),
                extra_requirements,
            ))?;

        Ok(Self::from_native_adapter_and_device(
            instance,
            adapter,
            device,
            queue,
            dual_source_blending,
            color_texture_format,
        ))
    }

    #[cfg(all(
        not(target_family = "wasm"),
        any(test, feature = "bench-support", feature = "test-support")
    ))]
    pub(crate) fn new_headless() -> anyhow::Result<(Self, wgpu::TextureFormat)> {
        let instance = Self::instance(None);
        let device_id_filter = Self::device_id_filter();
        let (adapter, device, queue, dual_source_blending, color_texture_format, target_format) =
            gpui::block_on(async {
                let mut adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;
                Self::sort_adapters(&mut adapters, device_id_filter, None);

                for adapter in adapters {
                    let adapter_info = adapter.get_info();
                    let Some(target_format) = Self::headless_target_format(&adapter) else {
                        log::warn!(
                            "Adapter {:?} has no supported headless render target format",
                            adapter_info.name
                        );
                        continue;
                    };

                    match Self::create_device(&adapter, None).await {
                        Ok((device, queue, dual_source_blending, color_texture_format)) => {
                            #[cfg(feature = "bench-support")]
                            if adapter_info.device_type == wgpu::DeviceType::Cpu {
                                log::error!(
                                    "Headless renderer selected software adapter {:?}; \
                                     benchmark results measure CPU software rendering, not hardware GPU rendering",
                                    adapter_info.name
                                );
                            }
                            return Ok((
                                adapter,
                                device,
                                queue,
                                dual_source_blending,
                                color_texture_format,
                                target_format,
                            ));
                        }
                        Err(error) => {
                            log::warn!(
                                "Failed to create a headless device for adapter {:?}: {error:#}",
                                adapter_info.name
                            );
                        }
                    }
                }

                anyhow::bail!("No usable headless GPU adapter found")
            })?;

        Ok((
            Self::from_native_adapter_and_device(
                instance,
                adapter,
                device,
                queue,
                dual_source_blending,
                color_texture_format,
            ),
            target_format,
        ))
    }

    #[cfg(all(
        not(target_family = "wasm"),
        any(test, feature = "bench-support", feature = "test-support")
    ))]
    /// Both candidates are 8-bit RGBA-ordered or BGRA-ordered formats: headless readback
    /// copies rows as 4 bytes per pixel and only swizzles, so no other formats may be added
    /// here without updating it.
    fn headless_target_format(adapter: &wgpu::Adapter) -> Option<wgpu::TextureFormat> {
        let required_usages =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC;
        [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Bgra8Unorm,
        ]
        .into_iter()
        .find(|format| {
            adapter
                .get_texture_format_features(*format)
                .allowed_usages
                .contains(required_usages)
        })
    }

    #[cfg(not(target_family = "wasm"))]
    fn from_native_adapter_and_device(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        dual_source_blending: bool,
        color_texture_format: TextureFormat,
    ) -> Self {
        let errors = DeviceErrorState::install(&device);

        log::info!(
            "Selected GPU adapter: {:?} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );
        let backend = WgpuBackend::Native(adapter.get_info().backend);

        Self {
            instance,
            adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
            backend,
            dual_source_blending,
            color_texture_format,
            errors,
        }
    }

    #[cfg(not(target_family = "wasm"))]
    fn device_id_filter() -> Option<u32> {
        match std::env::var(GPU_DEVICE_ID_ENV) {
            Ok(value) => parse_pci_id(&value)
                .with_context(|| {
                    format!(
                        "Failed to parse device ID from `{GPU_DEVICE_ID_ENV}` environment variable"
                    )
                })
                .log_err(),
            Err(std::env::VarError::NotPresent) => None,
            error => {
                error
                    .with_context(|| {
                        format!(
                            "Failed to read value of `{GPU_DEVICE_ID_ENV}` environment variable"
                        )
                    })
                    .log_err();
                None
            }
        }
    }

    #[cfg(target_family = "wasm")]
    pub async fn new_web(
        canvas: &web_sys::HtmlCanvasElement,
        preference: WebBackendPreference,
    ) -> anyhow::Result<PreparedWebGraphics> {
        Self::new_web_with_backend(canvas, preference).await
    }

    #[cfg(target_family = "wasm")]
    #[allow(clippy::arc_with_non_send_sync)]
    async fn new_web_with_backend(
        canvas: &web_sys::HtmlCanvasElement,
        preference: WebBackendPreference,
    ) -> anyhow::Result<PreparedWebGraphics> {
        let backends = match preference {
            WebBackendPreference::Auto => wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            WebBackendPreference::WebGpu => wgpu::Backends::BROWSER_WEBGPU,
            WebBackendPreference::WebGl => wgpu::Backends::GL,
        };
        let descriptor = wgpu::InstanceDescriptor {
            backends,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            display: Some(Box::new(WebDisplaySource)),
        };
        let instance = if preference == WebBackendPreference::Auto {
            wgpu::util::new_instance_with_webgpu_detection(descriptor).await
        } else {
            wgpu::Instance::new(descriptor)
        };
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|error| {
                anyhow::anyhow!("Failed to create browser graphics surface: {error}")
            })?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|error| {
                anyhow::anyhow!(
                    "Failed to request a {preference:?} adapter compatible with the canvas: {error}"
                )
            })?;
        let adapter_info = adapter.get_info();
        let backend = match adapter_info.backend {
            wgpu::Backend::BrowserWebGpu => WgpuBackend::BrowserWebGpu,
            wgpu::Backend::Gl => WgpuBackend::Gl,
            backend => {
                anyhow::bail!(
                    "Browser graphics initialization selected unexpected backend {backend:?}"
                )
            }
        };

        let (device, queue, dual_source_blending, color_texture_format) =
            Self::create_device(&adapter, None).await?;
        let errors = DeviceErrorState::install(&device);
        log::info!(
            "Browser graphics initialized: requested={preference:?}, selected={backend:?}, \
             adapter={:?}, limits={:?}, dual_source_blending={dual_source_blending}",
            adapter_info.name,
            device.limits(),
        );

        let context = Self {
            instance,
            adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
            backend,
            dual_source_blending,
            color_texture_format,
            errors,
        };
        Ok(PreparedWebGraphics { context, surface })
    }

    async fn create_device(
        adapter: &wgpu::Adapter,
        extra_requirements: Option<&WgpuDeviceRequirements>,
    ) -> anyhow::Result<(wgpu::Device, wgpu::Queue, bool, TextureFormat)> {
        let dual_source_blending = adapter
            .features()
            .contains(wgpu::Features::DUAL_SOURCE_BLENDING);

        let mut required_features = wgpu::Features::empty();
        if dual_source_blending {
            required_features |= wgpu::Features::DUAL_SOURCE_BLENDING;
        } else {
            log::warn!(
                "Dual-source blending not available on this GPU. \
                Subpixel text antialiasing will be disabled."
            );
        }

        let color_atlas_texture_format = Self::select_color_texture_format(adapter)?;

        let mut required_limits = wgpu::Limits::downlevel_defaults()
            .using_resolution(adapter.limits())
            .using_alignment(adapter.limits());

        // Merge application-requested requirements.
        if let Some(reqs) = extra_requirements {
            required_features |= reqs.features;
            required_limits = required_limits.or_better_values_from(&reqs.limits);
        }

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("gpui_device"),
                required_features,
                required_limits,
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create wgpu device: {e}"))?;

        Ok((
            device,
            queue,
            dual_source_blending,
            color_atlas_texture_format,
        ))
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn instance(display: Option<Box<dyn wgpu::wgt::WgpuHasDisplayHandle>>) -> wgpu::Instance {
        let preference = configured_backend_preference();
        let backends = preference.enabled_backends();
        log::info!("GPU backend preference: {preference:?}; enabled backends: {backends:?}");

        wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            display,
        })
    }

    pub fn check_compatible_with_surface(&self, surface: &wgpu::Surface<'_>) -> anyhow::Result<()> {
        let caps = surface.get_capabilities(&self.adapter);
        if caps.formats.is_empty() {
            let info = self.adapter.get_info();
            anyhow::bail!(
                "Adapter {:?} (backend={:?}, device={:#06x}) is not compatible with the \
                 display surface for this window.",
                info.name,
                info.backend,
                info.device,
            );
        }
        Ok(())
    }

    /// Select an adapter and create a device, testing that the surface can actually be configured.
    /// This is the only reliable way to determine compatibility on hybrid GPU systems, where
    /// adapters may report surface compatibility via get_capabilities() but fail when actually
    /// configuring (e.g., NVIDIA reporting Vulkan Wayland support but failing because the
    /// Wayland compositor runs on the Intel GPU).
    #[cfg(not(target_family = "wasm"))]
    async fn select_adapter_and_device(
        instance: &wgpu::Instance,
        device_id_filter: Option<u32>,
        surface: &wgpu::Surface<'_>,
        compositor_gpu: Option<&CompositorGpuHint>,
        extra_requirements: Option<&WgpuDeviceRequirements>,
    ) -> anyhow::Result<(
        wgpu::Adapter,
        wgpu::Device,
        wgpu::Queue,
        bool,
        TextureFormat,
    )> {
        let mut adapters: Vec<_> = instance.enumerate_adapters(wgpu::Backends::all()).await;

        if adapters.is_empty() {
            anyhow::bail!("No GPU adapters found");
        }

        if let Some(device_id) = device_id_filter {
            log::info!("{GPU_DEVICE_ID_ENV} filter: {:#06x}", device_id);
        }

        Self::sort_adapters(&mut adapters, device_id_filter, compositor_gpu);

        // Log all available adapters (in sorted order)
        log::info!("Found {} GPU adapter(s):", adapters.len());
        for adapter in &adapters {
            let info = adapter.get_info();
            log::info!(
                "  - {} (vendor={:#06x}, device={:#06x}, backend={:?}, type={:?})",
                info.name,
                info.vendor,
                info.device,
                info.backend,
                info.device_type,
            );
        }

        // Test each adapter by creating a device and configuring the surface
        for adapter in adapters {
            let info = adapter.get_info();

            log::info!("Testing adapter: {} ({:?})...", info.name, info.backend);

            match Self::try_adapter_with_surface(&adapter, surface, extra_requirements).await {
                Ok((device, queue, dual_source_blending, color_atlas_texture_format)) => {
                    log::info!(
                        "Selected GPU (passed configuration test): {} ({:?})",
                        info.name,
                        info.backend
                    );
                    return Ok((
                        adapter,
                        device,
                        queue,
                        dual_source_blending,
                        color_atlas_texture_format,
                    ));
                }
                Err(e) => {
                    log::info!(
                        "  Adapter {} ({:?}) failed: {}, trying next...",
                        info.name,
                        info.backend,
                        e
                    );
                }
            }
        }

        anyhow::bail!("No GPU adapter found that can configure the display surface")
    }

    /// Sort adapters into a single priority order. Tiers (from highest to lowest):
    ///
    /// 1. OXIDETERM_GPU_DEVICE_ID match — explicit user override
    /// 2. Compositor GPU match — the GPU the display server is rendering on
    /// 3. Device type (Discrete > Integrated > Other > Virtual > Cpu).
    ///    "Other" ranks above "Virtual" because OpenGL seems to count as "Other".
    /// 4. Backend — prefer Vulkan/Metal/Dx12 over GL/etc.
    #[cfg(not(target_family = "wasm"))]
    fn sort_adapters(
        adapters: &mut [wgpu::Adapter],
        device_id_filter: Option<u32>,
        compositor_gpu: Option<&CompositorGpuHint>,
    ) {
        adapters.sort_by_key(|adapter| {
            adapter_priority(&adapter.get_info(), device_id_filter, compositor_gpu)
        });
    }

    /// Try to use an adapter with a surface by creating a device and testing configuration.
    /// Returns the device and queue if successful, allowing them to be reused.
    #[cfg(not(target_family = "wasm"))]
    async fn try_adapter_with_surface(
        adapter: &wgpu::Adapter,
        surface: &wgpu::Surface<'_>,
        extra_requirements: Option<&WgpuDeviceRequirements>,
    ) -> anyhow::Result<(wgpu::Device, wgpu::Queue, bool, TextureFormat)> {
        let caps = surface.get_capabilities(adapter);
        if caps.formats.is_empty() {
            anyhow::bail!("no compatible surface formats");
        }
        if caps.alpha_modes.is_empty() {
            anyhow::bail!("no compatible alpha modes");
        }

        let (device, queue, dual_source_blending, color_atlas_texture_format) =
            Self::create_device(adapter, extra_requirements).await?;
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);

        let test_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: caps.formats[0],
            width: 64,
            height: 64,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(&device, &test_config);

        let error = error_scope.pop().await;
        if let Some(e) = error {
            anyhow::bail!("surface configuration failed: {e}");
        }

        Ok((
            device,
            queue,
            dual_source_blending,
            color_atlas_texture_format,
        ))
    }

    fn select_color_texture_format(adapter: &wgpu::Adapter) -> anyhow::Result<wgpu::TextureFormat> {
        let required_usages = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST;
        let bgra_features = adapter.get_texture_format_features(wgpu::TextureFormat::Bgra8Unorm);
        if bgra_features.allowed_usages.contains(required_usages) {
            return Ok(wgpu::TextureFormat::Bgra8Unorm);
        }

        let rgba_features = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba8Unorm);
        if rgba_features.allowed_usages.contains(required_usages) {
            let info = adapter.get_info();
            log::warn!(
                "Adapter {} ({:?}) does not support Bgra8Unorm atlas textures with usages {:?}; \
                 falling back to Rgba8Unorm atlas textures.",
                info.name,
                info.backend,
                required_usages,
            );
            return Ok(wgpu::TextureFormat::Rgba8Unorm);
        }

        let info = adapter.get_info();
        Err(anyhow::anyhow!(
            "Adapter {} ({:?}, device={:#06x}) does not support a usable color atlas texture \
             format with usages {:?}. Bgra8Unorm allowed usages: {:?}; \
             Rgba8Unorm allowed usages: {:?}.",
            info.name,
            info.backend,
            info.device,
            required_usages,
            bgra_features.allowed_usages,
            rgba_features.allowed_usages,
        ))
    }
    pub fn backend(&self) -> WgpuBackend {
        self.backend
    }

    pub fn uses_webgl_instance_data(&self) -> bool {
        matches!(self.backend, WgpuBackend::Gl) && cfg!(target_family = "wasm")
    }

    pub fn supports_dual_source_blending(&self) -> bool {
        self.dual_source_blending
    }

    pub fn color_texture_format(&self) -> wgpu::TextureFormat {
        self.color_texture_format
    }

    /// Returns true if the GPU device was lost (e.g., due to driver crash, suspend/resume).
    /// When this returns true, the context should be recreated.
    pub fn device_lost(&self) -> bool {
        self.errors.device_lost()
    }

    /// The device-wide error state, shared with renderers on this device.
    pub fn errors(&self) -> &Arc<DeviceErrorState> {
        &self.errors
    }
}

#[cfg(not(target_family = "wasm"))]
fn parse_pci_id(id: &str) -> anyhow::Result<u32> {
    let mut id = id.trim();

    if id.starts_with("0x") || id.starts_with("0X") {
        id = &id[2..];
    }
    let is_hex_string = id.chars().all(|c| c.is_ascii_hexdigit());
    let is_4_chars = id.len() == 4;
    anyhow::ensure!(
        is_4_chars && is_hex_string,
        "Expected a 4 digit PCI ID in hexadecimal format"
    );

    u32::from_str_radix(id, 16).context("parsing PCI ID as hex")
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::{
        BackendPreference, CompositorGpuHint, DeviceErrorState, adapter_priority,
        parse_backend_preference, parse_pci_id,
    };

    fn adapter_info(
        vendor: u32,
        device: u32,
        device_type: wgpu::DeviceType,
        backend: wgpu::Backend,
    ) -> wgpu::AdapterInfo {
        wgpu::AdapterInfo {
            name: "test-adapter".to_string(),
            vendor,
            device,
            device_type,
            device_pci_bus_id: String::new(),
            driver: String::new(),
            driver_info: String::new(),
            backend,
            subgroup_min_size: 0,
            subgroup_max_size: 0,
            transient_saves_memory: false,
        }
    }

    #[test]
    fn device_errors_are_observed_independently() {
        let errors = DeviceErrorState::default();
        let mut first = 0;
        let mut second = 0;
        assert_eq!(errors.observe_error(&mut first), None);
        errors.record("first".into());
        assert_eq!(errors.observe_error(&mut first).as_deref(), Some("first"));
        assert_eq!(errors.observe_error(&mut first), None);
        assert_eq!(errors.observe_error(&mut second).as_deref(), Some("first"));
        assert_eq!(errors.observe_error(&mut second), None);

        errors.record("second".into());
        assert_eq!(errors.observe_error(&mut second).as_deref(), Some("second"));
        assert_eq!(errors.observe_error(&mut first).as_deref(), Some("second"));
        assert_eq!(errors.observe_error(&mut first), None);
        assert_eq!(errors.observe_error(&mut second), None);
    }

    #[test]
    fn device_errors_coalesce_and_new_observers_see_latest() {
        let errors = DeviceErrorState::default();
        errors.record("older".into());
        errors.record("latest".into());
        let mut generation = 0;
        assert_eq!(
            errors.observe_error(&mut generation).as_deref(),
            Some("latest")
        );
        assert_eq!(errors.observe_error(&mut generation), None);

        let replacement = DeviceErrorState::default();
        replacement.record("replacement".into());
        generation = 0;
        assert_eq!(
            replacement.observe_error(&mut generation).as_deref(),
            Some("replacement")
        );
        assert_eq!(replacement.observe_error(&mut generation), None);
    }

    #[test]
    fn test_parse_device_id() {
        assert!(parse_pci_id("0xABCD").is_ok());
        assert!(parse_pci_id("ABCD").is_ok());
        assert!(parse_pci_id("abcd").is_ok());
        assert!(parse_pci_id("1234").is_ok());
        assert!(parse_pci_id("123").is_err());
        assert_eq!(
            parse_pci_id(&format!("{:x}", 0x1234)).unwrap(),
            parse_pci_id(&format!("{:X}", 0x1234)).unwrap(),
        );

        assert_eq!(
            parse_pci_id(&format!("{:#x}", 0x1234)).unwrap(),
            parse_pci_id(&format!("{:#X}", 0x1234)).unwrap(),
        );
    }

    #[test]
    fn backend_preference_parses_supported_values() {
        assert_eq!(
            parse_backend_preference(" auto ").unwrap(),
            BackendPreference::Auto
        );
        assert_eq!(
            parse_backend_preference("VULKAN").unwrap(),
            BackendPreference::Vulkan
        );
        assert_eq!(
            parse_backend_preference("opengl").unwrap(),
            BackendPreference::OpenGl
        );
        assert!(parse_backend_preference("dx12").is_err());
    }

    #[test]
    fn backend_preference_enables_the_expected_backends() {
        #[cfg(target_os = "windows")]
        assert_eq!(
            BackendPreference::Auto.enabled_backends(),
            wgpu::Backends::DX12
        );
        #[cfg(not(target_os = "windows"))]
        assert_eq!(
            BackendPreference::Auto.enabled_backends(),
            wgpu::Backends::VULKAN | wgpu::Backends::GL
        );
        assert_eq!(
            BackendPreference::Vulkan.enabled_backends(),
            wgpu::Backends::VULKAN
        );
        assert_eq!(
            BackendPreference::OpenGl.enabled_backends(),
            wgpu::Backends::GL
        );
    }

    #[test]
    fn explicit_device_precedes_compositor_and_device_type() {
        let compositor = CompositorGpuHint {
            vendor_id: 0x1234,
            device_id: 0x2000,
        };
        let explicit_cpu = adapter_info(0x1234, 0x1000, wgpu::DeviceType::Cpu, wgpu::Backend::Gl);
        let compositor_discrete = adapter_info(
            0x1234,
            0x2000,
            wgpu::DeviceType::DiscreteGpu,
            wgpu::Backend::Vulkan,
        );

        assert!(
            adapter_priority(&explicit_cpu, Some(0x1000), Some(&compositor))
                < adapter_priority(&compositor_discrete, Some(0x1000), Some(&compositor))
        );
    }

    #[test]
    fn compositor_precedes_device_type() {
        let compositor = CompositorGpuHint {
            vendor_id: 0x1234,
            device_id: 0x2000,
        };
        let compositor_virtual = adapter_info(
            0x1234,
            0x2000,
            wgpu::DeviceType::VirtualGpu,
            wgpu::Backend::Gl,
        );
        let other_discrete = adapter_info(
            0x1234,
            0x3000,
            wgpu::DeviceType::DiscreteGpu,
            wgpu::Backend::Vulkan,
        );

        assert!(
            adapter_priority(&compositor_virtual, None, Some(&compositor))
                < adapter_priority(&other_discrete, None, Some(&compositor))
        );
    }

    #[test]
    fn device_type_precedes_backend_and_cpu_remains_ranked() {
        let integrated_gl = adapter_info(
            0x1234,
            0x1000,
            wgpu::DeviceType::IntegratedGpu,
            wgpu::Backend::Gl,
        );
        let virtual_vulkan = adapter_info(
            0x1234,
            0x2000,
            wgpu::DeviceType::VirtualGpu,
            wgpu::Backend::Vulkan,
        );
        let cpu_vulkan = adapter_info(0x1234, 0x3000, wgpu::DeviceType::Cpu, wgpu::Backend::Vulkan);

        assert!(
            adapter_priority(&integrated_gl, None, None)
                < adapter_priority(&virtual_vulkan, None, None)
        );
        assert!(
            adapter_priority(&virtual_vulkan, None, None)
                < adapter_priority(&cpu_vulkan, None, None)
        );
    }

    #[test]
    fn vulkan_precedes_opengl_within_the_same_tier() {
        let vulkan = adapter_info(
            0x1234,
            0x1000,
            wgpu::DeviceType::VirtualGpu,
            wgpu::Backend::Vulkan,
        );
        let opengl = adapter_info(
            0x1234,
            0x1000,
            wgpu::DeviceType::VirtualGpu,
            wgpu::Backend::Gl,
        );

        assert!(adapter_priority(&vulkan, None, None) < adapter_priority(&opengl, None, None));
    }
}
