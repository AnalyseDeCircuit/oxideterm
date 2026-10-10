use std::{path::PathBuf, sync::Arc};

use oxideterm_settings::AppIconVariant;

pub(crate) const APP_ICON_PREVIEW_SIZE: f32 = 42.0;

pub(crate) fn app_icon_preview_source(variant: AppIconVariant) -> gpui::ImageSource {
    let path = app_icon_variant_resource_path(variant);
    gpui::ImageSource::Custom(Arc::new(move |window, cx| {
        // Minify before atlas upload: a single bilinear GPU sample misses fine icon edges.
        let pixels = (APP_ICON_PREVIEW_SIZE * window.scale_factor())
            .round()
            .max(1.0) as u32;
        window.use_asset::<gpui::AssetLogger<AppIconPreview>>(&(path.clone(), pixels), cx)
    }))
}

enum AppIconPreview {}

impl gpui::Asset for AppIconPreview {
    type Source = (PathBuf, u32);
    type Output = Result<Arc<gpui::RenderImage>, gpui::ImageCacheError>;

    fn load(
        (path, pixels): Self::Source,
        _: &mut gpui::App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        async move {
            // The asset cache owns loading and reuse per path/DPI; the shared poster decoder
            // performs alpha-aware downsampling off the UI thread and accounts for its memory.
            let (_, mut frame) = oxideterm_background_media::decode_poster(
                &path,
                oxideterm_background_media::OutputParams {
                    width: pixels,
                    height: pixels,
                    fit: oxideterm_background_media::BackgroundFit::Contain,
                    blur: 0.0,
                    limits: Default::default(),
                },
            )
            .map_err(|error| gpui::ImageCacheError::Other(Arc::new(error.into())))?;
            let pixels = std::mem::take(&mut frame.pixels);
            let pixels = image::RgbaImage::from_raw(frame.width, frame.height, pixels)
                .expect("poster decoder returns validated BGRA dimensions");
            Ok(Arc::new(
                gpui::RenderImage::new(vec![image::Frame::new(pixels)]).retaining(frame),
            ))
        }
    }
}

pub(crate) const APP_ICON_VARIANTS: &[AppIconVariant] = &[
    AppIconVariant::Default,
    AppIconVariant::WhiteBlue,
    AppIconVariant::WhiteGraphite,
    AppIconVariant::WhiteGreen,
    AppIconVariant::WhitePurple,
    AppIconVariant::WhiteRed,
    AppIconVariant::FilledOrange,
    AppIconVariant::FilledBlue,
    AppIconVariant::FilledGraphite,
    AppIconVariant::FilledGreen,
    AppIconVariant::FilledPurple,
    AppIconVariant::FilledRed,
];

pub(crate) fn app_icon_variant_file_name(variant: AppIconVariant) -> &'static str {
    match variant {
        AppIconVariant::Default => "default.png",
        AppIconVariant::WhiteBlue => "white-blue.png",
        AppIconVariant::WhiteGraphite => "white-graphite.png",
        AppIconVariant::WhiteGreen => "white-green.png",
        AppIconVariant::WhitePurple => "white-purple.png",
        AppIconVariant::WhiteRed => "white-red.png",
        AppIconVariant::FilledOrange => "filled-orange.png",
        AppIconVariant::FilledBlue => "filled-blue.png",
        AppIconVariant::FilledGraphite => "filled-graphite.png",
        AppIconVariant::FilledGreen => "filled-green.png",
        AppIconVariant::FilledPurple => "filled-purple.png",
        AppIconVariant::FilledRed => "filled-red.png",
    }
}

#[cfg(target_os = "windows")]
fn app_icon_variant_ico_file_name(variant: AppIconVariant) -> String {
    app_icon_variant_file_name(variant).replace(".png", ".ico")
}

pub(crate) fn app_icon_variant_resource_path(variant: AppIconVariant) -> PathBuf {
    let file_name = app_icon_variant_file_name(variant);
    for root in app_icon_resource_roots() {
        let candidate = root.join("variants").join(file_name);
        if candidate.exists() {
            return candidate;
        }
    }

    // Development runs from the workspace root should still show previews even
    // before package resources are copied next to the executable.
    PathBuf::from("crates")
        .join("oxideterm-gpui-app")
        .join("resources")
        .join("icons")
        .join("variants")
        .join(file_name)
}

fn app_icon_resource_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        roots.push(exe_dir.join("resources").join("icons"));
        roots.push(exe_dir.join("..").join("Resources").join("icons"));
        roots.push(exe_dir.join("icons"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(
            cwd.join("crates")
                .join("oxideterm-gpui-app")
                .join("resources")
                .join("icons"),
        );
    }
    roots
}

#[cfg(target_os = "macos")]
fn app_icon_variant_png(variant: AppIconVariant) -> &'static [u8] {
    match variant {
        AppIconVariant::Default => include_bytes!("../resources/icons/variants/default.png"),
        AppIconVariant::WhiteBlue => include_bytes!("../resources/icons/variants/white-blue.png"),
        AppIconVariant::WhiteGraphite => {
            include_bytes!("../resources/icons/variants/white-graphite.png")
        }
        AppIconVariant::WhiteGreen => {
            include_bytes!("../resources/icons/variants/white-green.png")
        }
        AppIconVariant::WhitePurple => {
            include_bytes!("../resources/icons/variants/white-purple.png")
        }
        AppIconVariant::WhiteRed => include_bytes!("../resources/icons/variants/white-red.png"),
        AppIconVariant::FilledOrange => {
            include_bytes!("../resources/icons/variants/filled-orange.png")
        }
        AppIconVariant::FilledBlue => include_bytes!("../resources/icons/variants/filled-blue.png"),
        AppIconVariant::FilledGraphite => {
            include_bytes!("../resources/icons/variants/filled-graphite.png")
        }
        AppIconVariant::FilledGreen => {
            include_bytes!("../resources/icons/variants/filled-green.png")
        }
        AppIconVariant::FilledPurple => {
            include_bytes!("../resources/icons/variants/filled-purple.png")
        }
        AppIconVariant::FilledRed => include_bytes!("../resources/icons/variants/filled-red.png"),
    }
}

#[cfg(target_os = "windows")]
fn app_icon_variant_ico_resource_path(variant: AppIconVariant) -> PathBuf {
    let file_name = app_icon_variant_ico_file_name(variant);
    for root in app_icon_resource_roots() {
        let candidate = root.join("variants").join(&file_name);
        if candidate.exists() {
            return candidate;
        }
    }

    // Keep cargo run behavior aligned with packaged Windows resources.
    PathBuf::from("crates")
        .join("oxideterm-gpui-app")
        .join("resources")
        .join("icons")
        .join("variants")
        .join(file_name)
}

#[cfg(target_os = "macos")]
pub(crate) fn install_runtime_app_icon(variant: AppIconVariant) {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };

    // Cargo-bundle uses the icon metadata for packaged apps; this keeps
    // development runs and runtime variants visually aligned with the setting.
    let bytes = app_icon_variant_png(variant);
    let data = unsafe { NSData::dataWithBytes_length(bytes.as_ptr().cast(), bytes.len()) };
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        eprintln!("failed to decode bundled OxideTerm application icon");
        return;
    };

    unsafe {
        NSApplication::sharedApplication(main_thread).setApplicationIconImage(Some(&image));
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn install_runtime_app_icon(variant: AppIconVariant) {
    let icon_path = app_icon_variant_ico_resource_path(variant);
    if let Err(error) = oxideterm_desktop_presence::set_application_icon(&icon_path) {
        eprintln!("failed to apply Windows application icon: {error:#}");
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn install_runtime_app_icon(_variant: AppIconVariant) {
    // Linux desktop shells resolve the installed icon through desktop metadata.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn icon_preview_minification_filters_detail_and_preserves_transparent_edges(
        cx: &mut gpui::TestAppContext,
    ) {
        let directory = tempfile::tempdir().unwrap();
        for (name, colors, expected) in [
            (
                "stripes",
                [[0, 0, 0, 255], [255, 255, 255, 255]],
                [128, 128, 128, 255],
            ),
            (
                "alpha",
                [[255, 0, 0, 255], [0, 0, 255, 0]],
                [0, 0, 255, 128],
            ),
        ] {
            let path = directory.path().join(format!("{name}.png"));
            image::RgbaImage::from_fn(512, 512, |x, _| image::Rgba(colors[x as usize % 2]))
                .save(&path)
                .unwrap();
            // Physical sizes for 100%, 125%, 150%, and 200% display scaling.
            for pixels in [42, 53, 63, 84] {
                let source = (path.clone(), pixels);
                cx.update(|cx| cx.fetch_asset::<AppIconPreview>(&source));
                cx.run_until_parked();
                let preview = cx
                    .update(|cx| cx.fetch_asset::<AppIconPreview>(&source))
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    preview.size(0),
                    gpui::size(
                        gpui::DevicePixels(pixels as i32),
                        gpui::DevicePixels(pixels as i32)
                    )
                );
                let center = ((pixels / 2 * pixels + pixels / 2) * 4) as usize;
                let actual = &preview.as_bytes(0).unwrap()[center..center + 4];
                for (&actual, expected) in actual.iter().zip(expected) {
                    assert!(
                        actual.abs_diff(expected) <= 3,
                        "{name} at {pixels}px must average fine detail without leaking transparent blue"
                    );
                }
            }
        }
    }
}
