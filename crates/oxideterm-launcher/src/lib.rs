// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Platform application launcher core.
//!
//! This crate mirrors the Tauri launcher backend: macOS scans installed `.app`
//! bundles, caches 64px PNG icons, and launches apps through `open`. GPUI keeps
//! only the UI state and calls into this crate so the scan/cache semantics do
//! not drift with view code.

use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use oxideterm_settings::default_settings_path;

const CACHE_MAX_AGE_SECS: u64 = 7 * 86_400; // Tauri icon cache freshness.
const APP_DIRS: &[&str] = &[
    "/Applications",
    "/System/Applications",
    "/System/Applications/Utilities",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LauncherAppEntry {
    pub name: String,
    pub path: String,
    pub bundle_id: Option<String>,
    pub icon_path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LauncherListResponse {
    pub apps: Vec<LauncherAppEntry>,
    pub icon_dir: Option<String>,
}

pub fn count_label(filtered: usize, total: usize) -> String {
    if filtered != total {
        format!("{filtered}/{total}")
    } else {
        total.to_string()
    }
}

pub fn filter_apps(apps: &[LauncherAppEntry], query: &str) -> Vec<LauncherAppEntry> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return apps.to_vec();
    }
    apps.iter()
        .filter(|app| {
            app.name.to_ascii_lowercase().contains(&query)
                || app
                    .bundle_id
                    .as_ref()
                    .is_some_and(|bundle_id| bundle_id.to_ascii_lowercase().contains(&query))
        })
        .cloned()
        .collect()
}

pub fn icon_cache_dir() -> PathBuf {
    default_settings_path()
        .parent()
        .map(|parent| parent.join("launcher_icons"))
        .unwrap_or_else(|| PathBuf::from("launcher_icons"))
}

pub fn clear_icon_cache() -> Result<(), String> {
    let icon_cache_dir = icon_cache_dir();
    if icon_cache_dir.exists() {
        fs::remove_dir_all(&icon_cache_dir)
            .map_err(|error| format!("Failed to clear icon cache: {error}"))?;
    }
    Ok(())
}

pub fn list_apps() -> Result<LauncherListResponse, String> {
    #[cfg(target_os = "macos")]
    {
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
    #[cfg(not(target_os = "macos"))]
    {
        Ok(LauncherListResponse {
            apps: Vec::new(),
            icon_dir: None,
        })
    }
}

pub fn launch_app(path: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|error| format!("Failed to launch '{path}': {error}"))?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err("Not supported on this platform".to_string())
    }
}

#[cfg(target_os = "macos")]
fn scan_dirs() -> Vec<PathBuf> {
    let mut dirs = APP_DIRS.iter().map(PathBuf::from).collect::<Vec<_>>();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join("Applications"));
    }
    dirs
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
fn cache_key_for_path(app_path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    app_path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(target_os = "macos")]
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
        "let size = NSSize(width: 64, height: 64)".to_string(),
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
  let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 64, pixelsHigh: 64, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
  NSGraphicsContext.saveGraphicsState()
  NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
  img.draw(in: NSRect(origin: .zero, size: size))
  NSGraphicsContext.restoreGraphicsState()
  let png = rep.representation(using: .png, properties: [:])!
  try png.write(to: URL(fileURLWithPath: "{png}"))
}} catch {{}}"#,
            app = app_escaped,
            png = png_escaped,
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
    fn filter_matches_tauri_name_or_bundle_id() {
        let apps = vec![
            LauncherAppEntry {
                name: "Safari".to_string(),
                path: "/Applications/Safari.app".to_string(),
                bundle_id: Some("com.apple.Safari".to_string()),
                icon_path: None,
            },
            LauncherAppEntry {
                name: "Calendar".to_string(),
                path: "/System/Applications/Calendar.app".to_string(),
                bundle_id: Some("com.apple.iCal".to_string()),
                icon_path: None,
            },
        ];
        assert_eq!(filter_apps(&apps, "saf").len(), 1);
        assert_eq!(filter_apps(&apps, "ical")[0].name, "Calendar");
        assert_eq!(filter_apps(&apps, "missing").len(), 0);
    }

    #[test]
    fn count_label_matches_tauri_header() {
        assert_eq!(count_label(4, 10), "4/10");
        assert_eq!(count_label(10, 10), "10");
    }
}
