use gpui::{
    AnyElement, Image, ImageFormat, ImageSource, ObjectFit, WeakEntity, div, img, prelude::*,
};
use std::rc::Rc;
mod library;
pub(crate) use library::Library;
use std::sync::{Arc, OnceLock};

use crate::host::Workspace;
use oxideterm_settings::BackgroundScope;
use oxideterm_settings_model::browser_preferences::{
    BundledWallpaper, WallpaperId, WallpaperSettings,
};
use oxideterm_workspace::{TabKind, tab_background_key};

impl Workspace {
    pub(crate) fn wallpaper_layer(&self, bg: &WallpaperSettings) -> Option<AnyElement> {
        let image = match bg.image {
            WallpaperId::Bundled(b) => source(b),
            WallpaperId::Imported(id) => self.wallpaper_library.full_source(id)?,
        };
        Some(layer(image, bg.opacity as f32))
    }
    pub(crate) fn window_wallpaper(&self) -> Option<&WallpaperSettings> {
        self.wallpaper
            .as_ref()
            .filter(|bg| bg.scope == BackgroundScope::Window)
    }
    pub(crate) fn wallpaper_content(&self, kind: &TabKind, body: AnyElement) -> AnyElement {
        let Some(bg) = self
            .wallpaper
            .as_ref()
            .filter(|bg| bg.includes(tab_background_key(kind)))
        else {
            return body;
        };
        // TerminalView already tints the canvas and paints explicit ANSI cell backgrounds.
        let body = if matches!(
            kind,
            TabKind::LocalTerminal | TabKind::SshTerminal | TabKind::MoshTerminal
        ) {
            body
        } else {
            oxideterm_gpui_ui::tab_canvas_surface(&self.tokens, body)
        };
        if bg.scope == BackgroundScope::Window {
            return body;
        }
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(gpui::rgb(self.tokens.ui.bg))
            .children(self.wallpaper_layer(bg))
            .child(div().relative().size_full().child(body))
            .into_any_element()
    }
}

pub(crate) fn gallery(
    demo: bool,
    owner: WeakEntity<Workspace>,
) -> oxideterm_gpui_settings_view::surface::BrowserWallpaperGallery {
    fn common(key: &str) -> bool {
        matches!(
            key,
            "terminal"
                | "local_terminal"
                | "notification_center"
                | "settings"
                | "file_manager"
                | "ide"
                | "plugin_manager"
        )
    }
    fn demo_tab(key: &str) -> bool {
        common(key)
            || matches!(
                key,
                "cloud_sync" | "session_manager" | "runtime" | "topology" | "forwards" | "sftp"
            )
    }
    oxideterm_gpui_settings_view::surface::BrowserWallpaperGallery {
        source,
        request: Rc::new(move |action, cx| {
            let _ = owner.update(cx, |h, cx| h.wallpaper_action(action, cx));
        }),
        supports_tab: if demo { demo_tab } else { common },
    }
}

fn source(wallpaper: BundledWallpaper) -> ImageSource {
    static IMAGES: [OnceLock<Arc<Image>>; 3] = [const { OnceLock::new() }; 3];
    IMAGES[wallpaper as usize]
        .get_or_init(|| {
            let (format, bytes): (ImageFormat, &[u8]) = match wallpaper {
                BundledWallpaper::Ambient => (
                    ImageFormat::Png,
                    include_bytes!(
                        "../../oxideterm-gpui-app/resources/backgrounds/oxide-ambient-v1.png"
                    ),
                ),
                BundledWallpaper::Nocturne => (
                    ImageFormat::Webp,
                    include_bytes!(
                        "../../oxideterm-gpui-app/resources/backgrounds/oxide-nocturne-v1.webp"
                    ),
                ),
                BundledWallpaper::Verdant => (
                    ImageFormat::Webp,
                    include_bytes!(
                        "../../oxideterm-gpui-app/resources/backgrounds/oxide-verdant-v1.webp"
                    ),
                ),
            };
            Arc::new(Image::from_bytes(format, bytes.to_vec()))
        })
        .clone()
        .into()
}

fn layer(image: ImageSource, opacity: f32) -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .overflow_hidden()
        .opacity(opacity)
        .child(
            img(image)
                .id("wallpaper-playback")
                .size_full()
                .object_fit(ObjectFit::Cover),
        )
        .into_any_element()
}
