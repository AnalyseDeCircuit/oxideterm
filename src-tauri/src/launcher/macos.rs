//! macOS Application Scanner
//!
//! Scans standard application directories for `.app` bundles.
//! - Uses `mdls` (Spotlight) to get the system-localized display name.
//! - Uses `NSWorkspace` (via a tiny Swift helper) to extract icons that work
//!   with both `.icns` files and Asset Catalogs (`.car`).

use super::AppEntry;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use tauri::Manager;

/// Directories to scan for `.app` bundles.
const APP_DIRS: &[&str] = &[
    "/Applications",
    "/System/Applications",
    "/System/Applications/Utilities",
];

/// List all installed macOS applications.
/// Spawns blocking work on a dedicated thread to avoid tying up the async runtime.
/// Returns `(apps, icon_cache_dir_path)`.
pub async fn list_applications(
    app: &tauri::AppHandle,
) -> Result<(Vec<AppEntry>, Option<String>), Box<dyn std::error::Error + Send + Sync>> {
    let icon_cache_dir = get_icon_cache_dir(app)?;
    std::fs::create_dir_all(&icon_cache_dir)?;

    let icon_dir_str = icon_cache_dir
        .to_str()
        .map(|s| s.to_string());

    // Move all blocking FS/subprocess work off the async runtime
    let entries = tokio::task::spawn_blocking(move || {
        // Also scan ~/Applications
        let home_apps = dirs::home_dir().map(|h| h.join("Applications"));

        let mut entries = Vec::new();

        for dir in APP_DIRS
            .iter()
            .map(PathBuf::from)
            .chain(home_apps.into_iter())
        {
            if !dir.exists() {
                continue;
            }
            scan_directory(&dir, &mut entries);
        }

        // Sort by localized name (case-insensitive)
        entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        // Deduplicate by path
        entries.dedup_by(|a, b| a.path == b.path);

        // Batch-extract all icons that are not yet cached
        batch_extract_icons(&mut entries, &icon_cache_dir);

        entries
    })
    .await
    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

    Ok((entries, icon_dir_str))
}

/// Scan a directory for `.app` bundles (one level deep).
fn scan_directory(dir: &Path, entries: &mut Vec<AppEntry>) {
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };

    for entry in read.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.ends_with(".app") {
            continue;
        }

        if let Some(app_entry) = build_app_entry(&path) {
            entries.push(app_entry);
        }
    }
}

/// Build an `AppEntry` for a single `.app` bundle.
/// Icon extraction is deferred to `batch_extract_icons`.
fn build_app_entry(app_path: &Path) -> Option<AppEntry> {
    let info_plist = app_path.join("Contents/Info.plist");
    if !info_plist.exists() {
        return None;
    }

    // ── Localized display name via Spotlight (mdls) ─────────────────────
    let display_name = get_localized_name(app_path).unwrap_or_else(|| {
        // Fallback: parse Info.plist directly
        plist::Value::from_file(&info_plist)
            .ok()
            .and_then(|v| {
                let dict = v.as_dictionary()?;
                dict.get("CFBundleDisplayName")
                    .and_then(|v| v.as_string())
                    .or_else(|| dict.get("CFBundleName").and_then(|v| v.as_string()))
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| {
                app_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown")
                    .to_string()
            })
    });

    // ── Bundle ID from Info.plist ────────────────────────────────────────
    let bundle_id = plist::Value::from_file(&info_plist)
        .ok()
        .and_then(|v| {
            v.as_dictionary()?
                .get("CFBundleIdentifier")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string())
        });

    Some(AppEntry {
        name: display_name,
        path: app_path.to_string_lossy().to_string(),
        bundle_id,
        icon_path: None, // filled in by batch_extract_icons
    })
}

/// Get the system-localized display name via `mdls` (Spotlight metadata).
/// Returns the name in the user's current system language (e.g. "日历" for Calendar).
fn get_localized_name(app_path: &Path) -> Option<String> {
    let output = std::process::Command::new("mdls")
        .args(["-name", "kMDItemDisplayName", "-raw"])
        .arg(app_path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // mdls returns "(null)" if the attribute is missing
    if name.is_empty() || name == "(null)" {
        return None;
    }

    // Strip the ".app" suffix if present in the display name
    Some(name.strip_suffix(".app").unwrap_or(&name).to_string())
}

/// Compute a collision-safe cache key from an app path using a hash.
fn cache_key_for_path(app_path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    app_path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Batch-extract icons for all apps that don't have a cached PNG yet.
/// Uses a single Swift process to export all icons in one go via NSWorkspace,
/// which is dramatically faster than spawning one swift process per app.
fn batch_extract_icons(entries: &mut [AppEntry], icon_cache_dir: &Path) {
    // Determine which apps need icon extraction
    let mut needed: Vec<(usize, String, PathBuf)> = Vec::new(); // (index, app_path, png_path)

    for (i, entry) in entries.iter().enumerate() {
        let cache_key = cache_key_for_path(&entry.path);
        let png_path = icon_cache_dir.join(format!("{}.png", cache_key));

        // Reuse cached icon if fresh (< 7 days)
        if png_path.exists() {
            if let Ok(meta) = png_path.metadata() {
                if let Ok(modified) = meta.modified() {
                    if modified.elapsed().unwrap_or_default().as_secs() < 7 * 86400 {
                        // Re-use cached
                        continue;
                    }
                }
            }
        }

        needed.push((i, entry.path.clone(), png_path));
    }

    // First pass: fill in already-cached entries
    for entry in entries.iter_mut() {
        let cache_key = cache_key_for_path(&entry.path);
        let png_path = icon_cache_dir.join(format!("{}.png", cache_key));
        if png_path.exists() {
            entry.icon_path = Some(png_path.to_string_lossy().to_string());
        }
    }

    if needed.is_empty() {
        return;
    }

    // Build a Swift script that processes all needed icons in one invocation
    let mut swift_lines = Vec::new();
    swift_lines.push("import AppKit".to_string());
    swift_lines.push("let ws = NSWorkspace.shared".to_string());
    swift_lines.push("let size = NSSize(width: 128, height: 128)".to_string());

    for (_idx, app_path, png_path) in &needed {
        let app_escaped = app_path.replace('\\', "\\\\").replace('"', "\\\"");
        let png_escaped = png_path
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        swift_lines.push(format!(
            r#"do {{
  let img = ws.icon(forFile: "{app}")
  img.size = size
  let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 128, pixelsHigh: 128, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
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

    let swift_code = swift_lines.join("\n");

    // Run the batch Swift script
    let result = std::process::Command::new("swift")
        .args(["-e", &swift_code])
        .output();

    if let Ok(output) = result {
        if !output.status.success() {
            eprintln!(
                "[launcher] Swift batch icon extraction warnings: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    // Assign generated icon paths
    for (idx, _app_path, png_path) in &needed {
        if png_path.exists() {
            entries[*idx].icon_path = Some(png_path.to_string_lossy().to_string());
        }
    }
}

/// Get the icon cache directory under Tauri's app data dir.
fn get_icon_cache_dir(
    app: &tauri::AppHandle,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    Ok(data_dir.join("launcher_icons"))
}
