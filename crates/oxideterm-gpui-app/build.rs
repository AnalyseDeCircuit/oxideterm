use std::env;

fn main() {
    println!("cargo:rerun-if-changed=resources/windows/app.rc");
    println!("cargo:rerun-if-changed=resources/icons/icon.ico");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    // Tauri embeds the Windows app icon from tauri.conf.json bundle metadata.
    // The native binary has to embed the same resource explicitly so Explorer,
    // Start Menu shortcuts, and installers do not fall back to a blank icon.
    embed_resource::compile("resources/windows/app.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("failed to embed OxideTerm Windows application resources");
}
