pub mod autostart;
#[cfg(not(target_family = "wasm"))]
pub mod http_client;
#[cfg(target_family = "wasm")]
pub mod http_client {
    pub use gpui_web::FetchHttpClient as AssetHttpClient;
}
pub mod rendering;
pub mod vibrancy;
pub mod window_opacity;

use gpui::{
    Bounds, Pixels, TitlebarOptions, WindowBounds, WindowDecorations, WindowKind, WindowOptions,
    point, px, size,
};
use oxideterm_theme::UiMetrics;

const OXIDETERM_APP_ID: &str = "com.oxideterm.app";

/// Constructs the native GPUI application through the vendored platform boundary.
#[cfg(not(target_family = "wasm"))]
pub fn application() -> gpui::Application {
    gpui_platform::application()
}

/// Constructs a single-threaded GPUI application in the current browser window.
#[cfg(target_family = "wasm")]
pub fn application() -> gpui::Application {
    gpui_web::init_logging();
    let platform = std::rc::Rc::new(gpui_web::WebPlatform::new_with_backend_and_font_fallback(
        false,
        gpui_web::WebBackendPreference::default(),
        gpui_web::CanvasFontFallback::EmojiAndCjk,
    ));
    let http_client = std::sync::Arc::new(platform.fetch_http_client());
    gpui::Application::with_platform(platform).with_http_client(http_client)
}

pub fn window_options(bounds: Bounds<Pixels>) -> WindowOptions {
    window_options_with_bounds(WindowBounds::Windowed(bounds))
}

pub fn workspace_window_options(bounds: Bounds<Pixels>) -> WindowOptions {
    workspace_window_options_with_bounds(WindowBounds::Windowed(bounds))
}

/// Builds options for windows whose custom chrome owns titlebar interactions.
pub fn workspace_window_options_with_bounds(window_bounds: WindowBounds) -> WindowOptions {
    let mut options = window_options_with_bounds(window_bounds);
    // AppKit otherwise reserves the fullscreen top edge for native window movement
    // and can intercept clicks before they reach tabs rendered in that region.
    options.app_owns_titlebar_drag = true;
    options
}

/// Builds normal application window options while preserving a restored state.
pub fn window_options_with_bounds(window_bounds: WindowBounds) -> WindowOptions {
    let metrics = UiMetrics::tauri_default();
    WindowOptions {
        window_bounds: Some(window_bounds),
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
