// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderProfile {
    #[default]
    Auto,
    Quality,
    LowPower,
    Compatibility,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphicsKind {
    HardwareGpu,
    IntegratedGpu,
    SoftwareEmulated,
    UnknownHardware,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetectedGraphics {
    pub kind: GraphicsKind,
    pub device_name: String,
    pub driver_name: String,
    pub driver_info: String,
}

impl DetectedGraphics {
    pub fn unknown_hardware() -> Self {
        Self {
            kind: GraphicsKind::UnknownHardware,
            device_name: String::new(),
            driver_name: String::new(),
            driver_info: String::new(),
        }
    }

    pub fn software_emulated(
        device_name: impl Into<String>,
        driver_name: impl Into<String>,
        driver_info: impl Into<String>,
    ) -> Self {
        Self {
            kind: GraphicsKind::SoftwareEmulated,
            device_name: device_name.into(),
            driver_name: driver_name.into(),
            driver_info: driver_info.into(),
        }
    }

    pub fn hardware(
        device_name: impl Into<String>,
        driver_name: impl Into<String>,
        driver_info: impl Into<String>,
    ) -> Self {
        let device_name = device_name.into();
        let kind = if looks_like_integrated_gpu(&device_name) {
            GraphicsKind::IntegratedGpu
        } else {
            GraphicsKind::HardwareGpu
        };
        Self {
            kind,
            device_name,
            driver_name: driver_name.into(),
            driver_info: driver_info.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectiveRenderProfile {
    Quality,
    LowPower,
    Compatibility,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalGraphicsPolicy {
    pub decode_images: bool,
    pub show_placeholders: bool,
    pub pixel_limit: usize,
    pub storage_limit_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalDrainPolicy {
    pub interactive_bytes: usize,
    pub normal_bytes: usize,
    pub throughput_bytes: usize,
    pub max_events: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectiveRenderPolicy {
    pub profile: EffectiveRenderProfile,
    pub allow_vibrancy: bool,
    pub allow_background_images: bool,
    pub allow_background_blur: bool,
    pub allow_animations: bool,
    pub terminal_graphics: TerminalGraphicsPolicy,
    pub image_cache_bytes: usize,
    pub drain: TerminalDrainPolicy,
}

impl EffectiveRenderPolicy {
    pub const fn quality() -> Self {
        Self {
            profile: EffectiveRenderProfile::Quality,
            allow_vibrancy: true,
            allow_background_images: true,
            allow_background_blur: true,
            allow_animations: true,
            terminal_graphics: TerminalGraphicsPolicy {
                decode_images: true,
                show_placeholders: true,
                pixel_limit: 16_777_216,
                storage_limit_bytes: 16 * 1024 * 1024,
            },
            image_cache_bytes: 64 * 1024 * 1024,
            drain: TerminalDrainPolicy {
                interactive_bytes: 32 * 1024,
                normal_bytes: 128 * 1024,
                throughput_bytes: 256 * 1024,
                max_events: 512,
            },
        }
    }

    pub const fn low_power() -> Self {
        Self {
            profile: EffectiveRenderProfile::LowPower,
            allow_vibrancy: true,
            allow_background_images: true,
            allow_background_blur: false,
            allow_animations: false,
            terminal_graphics: TerminalGraphicsPolicy {
                decode_images: true,
                show_placeholders: true,
                pixel_limit: 4_194_304,
                storage_limit_bytes: 8 * 1024 * 1024,
            },
            image_cache_bytes: 24 * 1024 * 1024,
            drain: TerminalDrainPolicy {
                interactive_bytes: 24 * 1024,
                normal_bytes: 96 * 1024,
                throughput_bytes: 192 * 1024,
                max_events: 384,
            },
        }
    }

    pub const fn compatibility() -> Self {
        Self {
            profile: EffectiveRenderProfile::Compatibility,
            allow_vibrancy: false,
            allow_background_images: false,
            allow_background_blur: false,
            allow_animations: false,
            terminal_graphics: TerminalGraphicsPolicy {
                decode_images: false,
                show_placeholders: true,
                pixel_limit: 1,
                storage_limit_bytes: 0,
            },
            image_cache_bytes: 8 * 1024 * 1024,
            drain: TerminalDrainPolicy {
                interactive_bytes: 16 * 1024,
                normal_bytes: 64 * 1024,
                throughput_bytes: 128 * 1024,
                max_events: 256,
            },
        }
    }
}

pub fn compute_render_policy(
    profile: RenderProfile,
    detected_graphics: &DetectedGraphics,
) -> EffectiveRenderPolicy {
    match profile {
        RenderProfile::Quality => EffectiveRenderPolicy::quality(),
        RenderProfile::LowPower => EffectiveRenderPolicy::low_power(),
        RenderProfile::Compatibility => EffectiveRenderPolicy::compatibility(),
        RenderProfile::Auto => match detected_graphics.kind {
            GraphicsKind::SoftwareEmulated | GraphicsKind::Unsupported => {
                EffectiveRenderPolicy::compatibility()
            }
            GraphicsKind::HardwareGpu
            | GraphicsKind::IntegratedGpu
            | GraphicsKind::UnknownHardware => EffectiveRenderPolicy::quality(),
        },
    }
}

fn looks_like_integrated_gpu(device_name: &str) -> bool {
    let name = device_name.to_ascii_lowercase();
    name.contains("intel")
        || name.contains("iris")
        || name.contains("uhd")
        || name.contains("integrated")
        || name.contains("apple")
        || name.contains("m1")
        || name.contains("m2")
        || name.contains("m3")
        || name.contains("m4")
        || name.contains("radeon graphics")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_software_emulation_uses_compatibility() {
        let detected = DetectedGraphics::software_emulated("llvmpipe", "mesa", "software");
        assert_eq!(
            compute_render_policy(RenderProfile::Auto, &detected).profile,
            EffectiveRenderProfile::Compatibility
        );
    }

    #[test]
    fn auto_unknown_hardware_uses_quality() {
        assert_eq!(
            compute_render_policy(RenderProfile::Auto, &DetectedGraphics::unknown_hardware())
                .profile,
            EffectiveRenderProfile::Quality
        );
    }

    #[test]
    fn explicit_profiles_override_detection() {
        let detected = DetectedGraphics::software_emulated("llvmpipe", "mesa", "software");
        assert_eq!(
            compute_render_policy(RenderProfile::LowPower, &detected).profile,
            EffectiveRenderProfile::LowPower
        );
        assert_eq!(
            compute_render_policy(RenderProfile::Quality, &detected).profile,
            EffectiveRenderProfile::Quality
        );
    }

    #[test]
    fn compatibility_disables_expensive_visuals_but_keeps_placeholders() {
        let policy = EffectiveRenderPolicy::compatibility();
        assert!(!policy.allow_vibrancy);
        assert!(!policy.allow_background_images);
        assert!(!policy.terminal_graphics.decode_images);
        assert!(policy.terminal_graphics.show_placeholders);
    }
}
