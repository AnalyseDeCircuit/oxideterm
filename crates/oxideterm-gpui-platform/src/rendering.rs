use gpui::Window;
use oxideterm_render_policy::DetectedGraphics;

pub fn detect_graphics(window: &Window) -> DetectedGraphics {
    if let Some(specs) = window.gpu_specs() {
        if specs.is_software_emulated {
            DetectedGraphics::software_emulated(
                specs.device_name,
                specs.driver_name,
                specs.driver_info,
            )
        } else {
            DetectedGraphics::hardware(specs.device_name, specs.driver_name, specs.driver_info)
        }
    } else {
        DetectedGraphics::unknown_hardware()
    }
}
