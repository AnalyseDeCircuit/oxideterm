// OxideTerm modification: export the explicit non-blocking recovery outcome.

mod cosmic_text_system;
mod wgpu_atlas;
mod wgpu_context;
mod wgpu_renderer;

pub use cosmic_text_system::*;
pub use wgpu;
pub use wgpu_atlas::*;
pub use wgpu_context::*;
pub use wgpu_renderer::{GpuContext, WgpuRecoveryStatus, WgpuRenderer, WgpuSurfaceConfig};

/// Actionable diagnostic emitted once when bounded GPU recovery is exhausted.
pub const RECOVERY_EXHAUSTED_MESSAGE: &str = "GPU recovery attempts are exhausted; restart OxideTerm. On Linux, use OXIDETERM_GPU_BACKEND=vulkan or opengl to diagnose backend-specific failures.";
