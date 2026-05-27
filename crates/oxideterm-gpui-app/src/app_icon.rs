#[cfg(target_os = "macos")]
pub(crate) fn install_runtime_app_icon() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    const APP_ICON_PNG: &[u8] = include_bytes!("../resources/icons/icon.png");

    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };

    // Cargo-bundle uses the icon metadata for packaged apps; this keeps
    // development runs launched by `cargo run` visually aligned with Tauri too.
    let data =
        unsafe { NSData::dataWithBytes_length(APP_ICON_PNG.as_ptr().cast(), APP_ICON_PNG.len()) };
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        eprintln!("failed to decode bundled OxideTerm application icon");
        return;
    };

    unsafe {
        NSApplication::sharedApplication(main_thread).setApplicationIconImage(Some(&image));
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn install_runtime_app_icon() {
    // Windows and Linux receive their application icon through packaging
    // metadata and desktop/installer resources rather than GPUI window options.
}
