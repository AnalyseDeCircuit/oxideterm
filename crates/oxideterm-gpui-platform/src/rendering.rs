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
    fn software_adapter_takes_precedence_over_virtual_marker() {
        let detected = detected_graphics_from_specs(Some(gpu_specs(true, true)));
        assert_eq!(detected.kind, GraphicsKind::SoftwareEmulated);
    }

    #[test]
    fn virtual_adapter_remains_distinct_from_hardware_and_software() {
        let detected = detected_graphics_from_specs(Some(gpu_specs(false, true)));
        assert_eq!(detected.kind, GraphicsKind::VirtualGpu);
    }
}
