pub mod rendering;
pub mod vibrancy;

use gpui::{
    Bounds, Pixels, TitlebarOptions, WindowBounds, WindowDecorations, WindowKind, WindowOptions,
    point, px, size,
};
use oxideterm_theme::UiMetrics;

const OXIDETERM_APP_ID: &str = "com.analysecircuit.OxideTerm";

pub fn window_options(bounds: Bounds<Pixels>) -> WindowOptions {
    let metrics = UiMetrics::tauri_default();
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(point(
                px(metrics.traffic_light_x),
                px(metrics.traffic_light_y),
            )),
        }),
        kind: WindowKind::Normal,
        is_movable: true,
        is_resizable: true,
        is_minimizable: true,
        window_decorations: Some(WindowDecorations::Client),
        window_min_size: Some(size(
            px(metrics.window_min_width),
            px(metrics.window_min_height),
        )),
        // Linux compositors use app_id to associate runtime windows with the
        // desktop file and package icon generated from the bundle metadata.
        app_id: Some(OXIDETERM_APP_ID.to_string()),
        ..Default::default()
    }
}
