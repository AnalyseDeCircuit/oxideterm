use gpui::{GpuSpecs, Window};
use oxideterm_render_policy::DetectedGraphics;

pub fn detect_graphics(window: &Window) -> DetectedGraphics {
    detected_graphics_from_specs(window.gpu_specs())
}

fn detected_graphics_from_specs(specs: Option<GpuSpecs>) -> DetectedGraphics {
    if let Some(specs) = specs {
        if specs.is_software_emulated {
            DetectedGraphics::software_emulated(
                specs.device_name,
                specs.driver_name,
                specs.driver_info,
            )
        } else if specs.is_virtual_gpu {
            DetectedGraphics::virtual_gpu(specs.device_name, specs.driver_name, specs.driver_info)
        } else {
            DetectedGraphics::hardware(specs.device_name, specs.driver_name, specs.driver_info)
        }
    } else {
        DetectedGraphics::unknown_hardware()
    }
}

#[cfg(test)]
mod tests {
    use oxideterm_render_policy::GraphicsKind;

    use super::*;

    fn gpu_specs(is_software_emulated: bool, is_virtual_gpu: bool) -> GpuSpecs {
        GpuSpecs {
            is_software_emulated,
            is_virtual_gpu,
            device_name: "test adapter".to_string(),
            driver_name: "test driver".to_string(),
            driver_info: "test driver info".to_string(),
        }
    }

    #[test]
    fn adapter_kind_is_classified_from_gpu_specs() {
        // Classification order matters because software adapters may also report as virtual.
        let cases = [
            (Some(gpu_specs(true, true)), GraphicsKind::SoftwareEmulated),
            (Some(gpu_specs(false, true)), GraphicsKind::VirtualGpu),
            (Some(gpu_specs(false, false)), GraphicsKind::HardwareGpu),
            (None, GraphicsKind::UnknownHardware),
        ];

        for (specs, expected) in cases {
            assert_eq!(detected_graphics_from_specs(specs).kind, expected);
        }
    }
}
