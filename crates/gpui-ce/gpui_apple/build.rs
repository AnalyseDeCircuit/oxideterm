// Shader bindings must use the same patched GPUI definitions as the renderer.
#![allow(clippy::disallowed_methods, reason = "build scripts are exempt")]

fn main() {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    apple_build::run();
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple_build {
    #[cfg(not(feature = "runtime_shaders"))]
    use std::ffi::OsString;
    use std::{
        env,
        path::{Path, PathBuf},
    };

    use cbindgen::Config;

    pub fn run() {
        let header_path = generate_shader_bindings();

        #[cfg(feature = "runtime_shaders")]
        emit_stitched_shaders(&header_path);
        #[cfg(not(feature = "runtime_shaders"))]
        compile_metal_shaders(&header_path);
    }

    fn generate_shader_bindings() -> PathBuf {
        let output_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("scene.h");

        let gpui_dir = find_gpui_crate_dir();

        let mut config = Config {
            include_guard: Some("SCENE_H".into()),
            language: cbindgen::Language::C,
            no_includes: true,
            ..Default::default()
        };
        config.export.include.extend([
            "Bounds".into(),
            "Corners".into(),
            "Edges".into(),
            "Size".into(),
            "Pixels".into(),
            "PointF".into(),
            "SceneHsla".into(),
            "ContentMask".into(),
            "Uniforms".into(),
            "AtlasTile".into(),
            "PathRasterizationInputIndex".into(),
            "PathVertex_ScaledPixels".into(),
            "PathRasterizationVertex".into(),
            "ShadowInputIndex".into(),
            "Shadow".into(),
            "QuadInputIndex".into(),
            "Underline".into(),
            "UnderlineInputIndex".into(),
            "Quad".into(),
            "BorderStyle".into(),
            "SpriteInputIndex".into(),
            "MonochromeSprite".into(),
            "PolychromeSprite".into(),
            "PathSprite".into(),
            "SurfaceInputIndex".into(),
            "SurfaceBounds".into(),
            "TransformationMatrix".into(),
        ]);
        config.no_includes = true;
        config.enumeration.prefix_with_name = true;
        config
            .export
            .rename
            .insert("SceneHsla".into(), "Hsla".into());

        let mut builder = cbindgen::Builder::new();

        let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

        // Source files from gpui that define types used in shaders
        let gpui_src_paths = [
            gpui_dir.join("src/scene.rs"),
            gpui_dir.join("src/geometry.rs"),
            gpui_dir.join("src/color.rs"),
            gpui_dir.join("src/window.rs"),
            gpui_dir.join("src/platform.rs"),
        ];

        // Source files from this crate
        let local_src_paths = [crate_dir.join("src/metal_renderer.rs")];

        for src_path in gpui_src_paths.iter().chain(local_src_paths.iter()) {
            println!("cargo:rerun-if-changed={}", src_path.display());
            builder = builder.with_src(src_path);
        }

        builder
            .with_config(config)
            .generate()
            .expect("Unable to generate bindings")
            .write_to_file(&output_path);

        output_path
    }

    /// Follow Cargo's patched core dependency so shader and Rust layouts agree.
    fn find_gpui_crate_dir() -> PathBuf {
        gpui::GPUI_MANIFEST_DIR.into()
    }

    /// To enable runtime compilation, we need to "stitch" the shaders file with the generated header
    /// so that it is self-contained.
    #[cfg(feature = "runtime_shaders")]
    fn emit_stitched_shaders(header_path: &Path) {
        fn stitch_header(header: &Path, shader_path: &Path) -> std::io::Result<PathBuf> {
            let header_contents = std::fs::read_to_string(header)?;
            let shader_contents = std::fs::read_to_string(shader_path)?;
            let stitched_contents = format!("{header_contents}\n{shader_contents}");
            let out_path =
                PathBuf::from(env::var("OUT_DIR").unwrap()).join("stitched_shaders.metal");
            std::fs::write(&out_path, stitched_contents)?;
            Ok(out_path)
        }
        let shader_source_path = "./src/shaders.metal";
        let shader_path = PathBuf::from(shader_source_path);
        stitch_header(header_path, &shader_path).unwrap();
        println!("cargo:rerun-if-changed={shader_source_path}");
    }

    #[cfg(not(feature = "runtime_shaders"))]
    fn compile_metal_shaders(header_path: &Path) {
        use std::process::{self, Command};
        println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
        println!("cargo:rerun-if-env-changed=TOOLCHAINS");
        // New Xcode versions ship a placeholder in the default toolchain even
        // when the separate Metal component is installed. Honor explicit user
        // selection; otherwise use that component when xcrun can resolve it.
        let use_metal_toolchain = env::var_os("TOOLCHAINS").is_none()
            && Command::new("xcrun")
                .args(["--toolchain", "Metal", "--find", "metal"])
                .output()
                .is_ok_and(|output| output.status.success());

        println!("cargo:rerun-if-env-changed=SDKROOT");

        // Build scripts run on the host, so the target platform must come from
        // Cargo's environment rather than `cfg!`. The target environment, not the
        // target name, identifies simulators: `x86_64-apple-ios` has no `-sim` suffix.
        let target_os = env::var("CARGO_CFG_TARGET_OS")
            .expect("Cargo sets CARGO_CFG_TARGET_OS for build scripts");
        let target_env = env::var("CARGO_CFG_TARGET_ENV")
            .expect("Cargo sets CARGO_CFG_TARGET_ENV for build scripts");
        let (sdk, minimum_version_argument) = match (target_os.as_str(), target_env.as_str()) {
            ("macos", _) => ("macosx", "-mmacosx-version-min=12.0"),
            ("ios", "sim") => ("iphonesimulator", "-mios-simulator-version-min=15.0"),
            ("ios", "") => ("iphoneos", "-mios-version-min=15.0"),
            _ => {
                println!(
                    "cargo::error=unsupported Metal shader target: target_os={target_os}, target_env={target_env}"
                );
                process::exit(1);
            }
        };

        let sdk = if target_os == "macos" {
            env::var_os("SDKROOT").unwrap_or_else(|| OsString::from(sdk))
        } else {
            OsString::from(sdk)
        };

        let shader_path = "./src/shaders.metal";
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let air_output_path = out_dir.join("shaders.air");
        let metallib_output_path = out_dir.join("shaders.metallib");
        println!("cargo:rerun-if-changed={}", shader_path);

        // The metal compiler records the resolved absolute path of its input
        // unconditionally. Compile a copy staged in OUT_DIR so the recorded
        // location is the build's canonical output directory, never the
        // checkout (corgi rejects artifacts that embed the build path).
        let staged_shader_path = out_dir.join("shaders.metal");
        std::fs::copy(shader_path, &staged_shader_path).unwrap();

        let mut compiler = Command::new("xcrun");
        if use_metal_toolchain {
            compiler.args(["--toolchain", "Metal"]);
        }
        let output = compiler
            .current_dir(&out_dir)
            .arg("-sdk")
            .arg(&sdk)
            .args([
                "metal",
                "-gline-tables-only",
                minimum_version_argument,
                "-MO",
                "-c",
            ])
            .arg(&staged_shader_path)
            .args(["-include", header_path.to_str().unwrap(), "-o"])
            .arg(&air_output_path)
            .output()
            .unwrap();

        if !output.status.success() {
            println!(
                "cargo::error=metal shader compilation failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            process::exit(1);
        }

        let mut linker = Command::new("xcrun");
        if use_metal_toolchain {
            linker.args(["--toolchain", "Metal"]);
        }
        let output = linker
            .current_dir(&out_dir)
            .arg("-sdk")
            .arg(&sdk)
            .arg("metallib")
            .arg(&air_output_path)
            .arg("-o")
            .arg(metallib_output_path)
            .output()
            .unwrap();

        if !output.status.success() {
            println!(
                "cargo::error=metallib compilation failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            process::exit(1);
        }

        // Nothing reads the .air intermediate after this point.
        std::fs::remove_file(&air_output_path).unwrap();
    }
}
