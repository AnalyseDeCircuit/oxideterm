use std::path::PathBuf;

use oxideterm_settings::AppIconVariant;

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

    #[test]
    fn every_app_icon_variant_has_a_windows_icon_resource() {
        for variant in APP_ICON_VARIANTS {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources")
                .join("icons")
                .join("variants")
                .join(app_icon_variant_ico_file_name(*variant));
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));

            // ICO files begin with a reserved word, the icon type, and count.
            assert_eq!(
                &bytes[..4],
                &[0, 0, 1, 0],
                "invalid ICO: {}",
                path.display()
            );
            assert!(
                u16::from_le_bytes([bytes[4], bytes[5]]) >= 6,
                "missing Windows icon sizes: {}",
                path.display()
            );
        }
    }
}
