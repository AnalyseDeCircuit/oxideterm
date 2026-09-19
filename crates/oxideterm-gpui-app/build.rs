use std::{env, fs, path::PathBuf};

fn main() {
    compress_maple_fonts();
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

fn compress_maple_fonts() {
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must set OUT_DIR"));
    for style in ["Regular", "Bold", "Italic", "BoldItalic"] {
        let name = format!("MapleMono-NF-CN-Subset-{style}.ttf");
        let source = PathBuf::from("resources/fonts/MapleMono").join(&name);
        println!("cargo:rerun-if-changed={}", source.display());
        let bytes = fs::read(&source).expect("failed to read bundled MapleMono font");
        // Keep each face independent so registering one style never expands the whole family.
        let compressed =
            zstd::bulk::compress(&bytes, 19).expect("failed to compress bundled MapleMono font");
        fs::write(output.join(format!("{name}.zst")), compressed)
            .expect("failed to write compressed MapleMono font");
    }
}
