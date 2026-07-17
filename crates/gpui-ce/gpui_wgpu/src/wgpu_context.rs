// OxideTerm modification: provide product-scoped GPU backend selection and deterministic fallback ordering.

#[cfg(not(target_family = "wasm"))]
use anyhow::Context as _;
#[cfg(not(target_family = "wasm"))]
use gpui_util::ResultExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use wgpu::TextureFormat;

#[cfg(not(target_family = "wasm"))]
const GPU_BACKEND_ENV: &str = "OXIDETERM_GPU_BACKEND";
#[cfg(not(target_family = "wasm"))]
const GPU_DEVICE_ID_ENV: &str = "OXIDETERM_GPU_DEVICE_ID";

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

pub struct WgpuContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    dual_source_blending: bool,
    color_texture_format: wgpu::TextureFormat,
    device_lost: Arc<AtomicBool>,
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
        let device_id_filter = match std::env::var(GPU_DEVICE_ID_ENV) {
            Ok(val) => parse_pci_id(&val)
                .with_context(|| {
                    format!(
                        "Failed to parse device ID from `{GPU_DEVICE_ID_ENV}` environment variable"
                    )
                })
                .log_err(),
            Err(std::env::VarError::NotPresent) => None,
            err => {
                err.with_context(|| {
                    format!("Failed to read value of `{GPU_DEVICE_ID_ENV}` environment variable")
                })
                .log_err();
                None
            }
        };

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

        let device_lost = Arc::new(AtomicBool::new(false));
        device.set_device_lost_callback({
            let device_lost = Arc::clone(&device_lost);
            move |reason, message| {
                log::error!("wgpu device lost: reason={reason:?}, message={message}");
                if reason != wgpu::DeviceLostReason::Destroyed {
                    device_lost.store(true, Ordering::Relaxed);
                }
            }
        });

        log::info!(
            "Selected GPU adapter: {:?} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );

        Ok(Self {
            instance,
            adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
            dual_source_blending,
            color_texture_format,
            device_lost,
        })
    }

    #[cfg(target_family = "wasm")]
    pub async fn new_web() -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            display: None,
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to request GPU adapter: {e}"))?;

        log::info!(
            "Selected GPU adapter: {:?} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );

        let device_lost = Arc::new(AtomicBool::new(false));
        let (device, queue, dual_source_blending, color_texture_format) =
            Self::create_device(&adapter, None).await?;

        Ok(Self {
            instance,
            adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
            dual_source_blending,
            color_texture_format,
            device_lost,
        })
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
    pub fn instance(display: Box<dyn wgpu::wgt::WgpuHasDisplayHandle>) -> wgpu::Instance {
        let preference = configured_backend_preference();
        let backends = preference.enabled_backends();
        log::info!("GPU backend preference: {preference:?}; enabled backends: {backends:?}");

        wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            display: Some(display),
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

        // Sort adapters into a single priority order. Tiers (from highest to lowest):
        //
        // 1. OXIDETERM_GPU_DEVICE_ID match — explicit user override
        // 2. Compositor GPU match — the GPU the display server is rendering on
        // 3. Device type (Discrete > Integrated > Other > Virtual > Cpu).
        //    "Other" ranks above "Virtual" because OpenGL seems to count as "Other".
        // 4. Backend — prefer Vulkan over OpenGL within the same tier.
        adapters.sort_by_key(|adapter| {
            let info = adapter.get_info();
            adapter_priority(&info, device_id_filter, compositor_gpu)
        });

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
    pub fn supports_dual_source_blending(&self) -> bool {
        self.dual_source_blending
    }

    pub fn color_texture_format(&self) -> wgpu::TextureFormat {
        self.color_texture_format
    }

    /// Returns true if the GPU device was lost (e.g., due to driver crash, suspend/resume).
    /// When this returns true, the context should be recreated.
    pub fn device_lost(&self) -> bool {
        self.device_lost.load(Ordering::Relaxed)
    }

    /// Returns a clone of the device_lost flag for sharing with renderers.
    pub(crate) fn device_lost_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.device_lost)
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

#[cfg(test)]
mod tests {
    use super::{
        BackendPreference, CompositorGpuHint, adapter_priority, parse_backend_preference,
        parse_pci_id,
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
