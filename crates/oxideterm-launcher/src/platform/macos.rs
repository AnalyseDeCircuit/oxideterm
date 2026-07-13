// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use crate::{
    LauncherAppEntry, LauncherListResponse,
    cache::{CACHE_MAX_AGE_SECS, icon_cache_dir},
};

const APP_DIRS: &[&str] = &[
    "/Applications",
    "/System/Applications",
    "/System/Applications/Utilities",
];
const APP_ICON_PIXEL_SIZE: u32 = 128; // A 64-point icon needs a 2x backing image on Retina displays.

pub fn list_apps() -> Result<LauncherListResponse, String> {
    let icon_cache_dir = icon_cache_dir();
    fs::create_dir_all(&icon_cache_dir)
        .map_err(|error| format!("Failed to create icon cache directory: {error}"))?;
    let icon_dir = icon_cache_dir.to_str().map(str::to_string);
    let mut entries = Vec::new();
    for dir in scan_dirs() {
        if dir.exists() {
            scan_directory(&dir, &mut entries);
        }
    }
    entries.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });
    entries.dedup_by(|left, right| left.path == right.path);
    batch_extract_icons(&mut entries, &icon_cache_dir);
    Ok(LauncherListResponse {
        apps: entries,
        icon_dir,
    })
}

pub fn launch_app(path: &str) -> Result<(), String> {
    Command::new("open")
        .arg(path)
        .spawn()
        .map_err(|error| format!("Failed to launch '{path}': {error}"))?;
    Ok(())
}

fn scan_dirs() -> Vec<PathBuf> {
    let mut dirs = APP_DIRS.iter().map(PathBuf::from).collect::<Vec<_>>();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join("Applications"));
    }
    dirs
}

fn scan_directory(dir: &Path, entries: &mut Vec<LauncherAppEntry>) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.ends_with(".app") {
            continue;
        }
        if let Some(entry) = build_app_entry(&path) {
            entries.push(entry);
        }
    }
}

fn build_app_entry(app_path: &Path) -> Option<LauncherAppEntry> {
    let info_plist = app_path.join("Contents/Info.plist");
    if !info_plist.exists() {
        return None;
    }
    let fallback_name = app_path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("Unknown")
        .to_string();
    let name = mdls_raw(app_path, "kMDItemDisplayName")
        .and_then(|name| {
            let name = name.strip_suffix(".app").unwrap_or(&name).to_string();
            (!name.is_empty()).then_some(name)
        })
        .unwrap_or(fallback_name);
    let bundle_id = mdls_raw(app_path, "kMDItemCFBundleIdentifier");
    Some(LauncherAppEntry {
        name,
        path: app_path.to_string_lossy().to_string(),
        bundle_id,
        icon_path: None,
    })
}

fn mdls_raw(app_path: &Path, key: &str) -> Option<String> {
    let output = Command::new("mdls")
        .args(["-name", key, "-raw"])
        .arg(app_path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() || value == "(null)" {
        None
    } else {
        Some(value)
    }
}

fn cache_key_for_path(app_path: &str) -> String {
    cache_key_for_path_at_size(app_path, APP_ICON_PIXEL_SIZE)
}

fn cache_key_for_path_at_size(app_path: &str, pixel_size: u32) -> String {
    let mut hasher = DefaultHasher::new();
    // The output size is part of the key so quality changes never reuse stale PNG files.
    app_path.hash(&mut hasher);
    pixel_size.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn batch_extract_icons(entries: &mut [LauncherAppEntry], icon_cache_dir: &Path) {
    let mut needed = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let png_path = icon_cache_dir.join(format!("{}.png", cache_key_for_path(&entry.path)));
        if png_path.exists()
            && png_path
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|elapsed| elapsed < Duration::from_secs(CACHE_MAX_AGE_SECS))
        {
            continue;
        }
        needed.push((index, entry.path.clone(), png_path));
    }

    for entry in entries.iter_mut() {
        let png_path = icon_cache_dir.join(format!("{}.png", cache_key_for_path(&entry.path)));
        if png_path.exists() {
            entry.icon_path = Some(png_path.to_string_lossy().to_string());
        }
    }

    if needed.is_empty() {
        return;
    }

    let mut swift_lines = vec![
        "import AppKit".to_string(),
        "let ws = NSWorkspace.shared".to_string(),
        format!(
            "let size = NSSize(width: {0}, height: {0})",
            APP_ICON_PIXEL_SIZE
        ),
    ];
    for (_index, app_path, png_path) in &needed {
        let app_escaped = app_path.replace('\\', "\\\\").replace('"', "\\\"");
        let png_escaped = png_path
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        swift_lines.push(format!(
            r#"do {{
  let img = ws.icon(forFile: "{app}")
  img.size = size
  let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: {pixel_size}, pixelsHigh: {pixel_size}, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
  NSGraphicsContext.saveGraphicsState()
  NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
  NSGraphicsContext.current?.imageInterpolation = .high
  img.draw(in: NSRect(origin: .zero, size: size))
  NSGraphicsContext.restoreGraphicsState()
  let png = rep.representation(using: .png, properties: [:])!
  try png.write(to: URL(fileURLWithPath: "{png}"))
}} catch {{}}"#,
            app = app_escaped,
            png = png_escaped,
            pixel_size = APP_ICON_PIXEL_SIZE,
        ));
    }

    let _ = Command::new("swift")
        .args(["-e", &swift_lines.join("\n")])
        .output();

    for (index, _app_path, png_path) in needed {
        if png_path.exists() {
            entries[index].icon_path = Some(png_path.to_string_lossy().to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_cache_key_changes_with_output_resolution() {
        let app_path = "/Applications/OxideTerm.app";

        assert_ne!(
            cache_key_for_path_at_size(app_path, 64),
            cache_key_for_path_at_size(app_path, 128)
        );
        assert_eq!(
            cache_key_for_path(app_path),
            cache_key_for_path_at_size(app_path, APP_ICON_PIXEL_SIZE)
        );
    }
}
