//! Terminal Background Image Module
//!
//! Handles user-selected background images for the terminal:
//! - Validates image format (PNG, JPEG, WebP, GIF/APNG)
//! - Resizes oversized images (max 1920px on longest edge)
//! - Converts to WebP for storage efficiency
//! - Stores in `app_data_dir/backgrounds/` (supports multiple images)
//! - Grants asset protocol scope for frontend `asset://` URL access

use std::path::PathBuf;
use tauri::Manager;

/// Max dimension (longest edge) for stored background images.
/// Larger images are resized proportionally to save memory/decode time.
const MAX_DIMENSION: u32 = 1920;

/// Max input file size (20 MB). Reject anything larger before decoding.
const MAX_INPUT_SIZE: u64 = 20 * 1024 * 1024;

/// Get the backgrounds directory path.
fn get_backgrounds_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    Ok(data_dir.join("backgrounds"))
}

/// Response from upload_terminal_background.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundResult {
    /// The stored file path (use with convertFileSrc for asset:// URL)
    pub path: String,
    /// Original file size in bytes
    pub original_size: u64,
    /// Stored file size in bytes (after conversion)
    pub stored_size: u64,
    /// Whether the image was an animated format (GIF/APNG)
    pub animated: bool,
}

/// Upload a background image to the gallery.
///
/// The source file is validated, optionally resized, and stored as WebP/JPEG
/// (or kept as-is for animated GIF/APNG) in `app_data_dir/backgrounds/`.
/// Previous images are NOT deleted — the gallery can hold multiple images.
#[tauri::command]
pub async fn upload_terminal_background(
    app: tauri::AppHandle,
    source_path: String,
) -> Result<BackgroundResult, String> {
    let source = std::path::Path::new(&source_path);

    // Validate source exists
    if !source.exists() {
        return Err("File does not exist".into());
    }

    // Check file size before reading
    let metadata = std::fs::metadata(source)
        .map_err(|e| format!("Cannot read file metadata: {}", e))?;
    let original_size = metadata.len();
    if original_size > MAX_INPUT_SIZE {
        return Err(format!(
            "File too large ({:.1} MB). Maximum is {} MB.",
            original_size as f64 / (1024.0 * 1024.0),
            MAX_INPUT_SIZE / (1024 * 1024)
        ));
    }

    // Detect file type via magic bytes
    let file_type = infer::get_from_path(source)
        .map_err(|e| format!("Cannot detect file type: {}", e))?
        .ok_or_else(|| "Unknown file type. Supported: PNG, JPEG, WebP, GIF".to_string())?;

    let mime = file_type.mime_type();
    let is_animated = mime == "image/gif"; // GIF stays as-is to preserve animation

    // Validate MIME type
    match mime {
        "image/png" | "image/jpeg" | "image/webp" | "image/gif" => {}
        _ => return Err(format!("Unsupported image type: {}. Use PNG, JPEG, WebP, or GIF.", mime)),
    }

    // Ensure backgrounds directory exists
    let bg_dir = get_backgrounds_dir(&app)?;
    std::fs::create_dir_all(&bg_dir)
        .map_err(|e| format!("Failed to create backgrounds dir: {}", e))?;

    // For animated GIF: copy as-is (don't re-encode, preserve animation)
    // For static images: decode → resize if needed → lossy JPEG / lossless WebP
    //
    // Use nanosecond-precision timestamps so that two updates within the same
    // second produce distinct filenames, naturally busting the asset:// cache.
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let dest_path = if is_animated {
        let dest = bg_dir.join(format!("bg_{}.gif", ts));
        std::fs::copy(source, &dest)
            .map_err(|e| format!("Failed to copy GIF: {}", e))?;
        dest
    } else {
        // Process in blocking context to avoid starving the async runtime
        let source_owned = source.to_path_buf();
        let bg_dir_owned = bg_dir.clone();
        tokio::task::spawn_blocking(move || -> Result<PathBuf, String> {
            process_static_image(&source_owned, &bg_dir_owned)
        })
        .await
        .map_err(|e| format!("Image processing task failed: {}", e))??
    };

    // Grant the backgrounds directory on the asset protocol scope
    app.asset_protocol_scope()
        .allow_directory(&bg_dir, false)
        .map_err(|e| format!("Failed to grant backgrounds dir: {}", e))?;

    let stored_size = std::fs::metadata(&dest_path)
        .map(|m| m.len())
        .unwrap_or(0);

    let path_str = dest_path.to_string_lossy().to_string();

    tracing::info!(
        "Terminal background uploaded: {} → {} ({:.0} KB → {:.0} KB, animated={})",
        source_path,
        path_str,
        original_size as f64 / 1024.0,
        stored_size as f64 / 1024.0,
        is_animated,
    );

    Ok(BackgroundResult {
        path: path_str,
        original_size,
        stored_size,
        animated: is_animated,
    })
}

/// Process a static image: decode, resize if needed, encode to an in-memory
/// buffer, then write to disk exactly once.
///
/// - Images **without alpha** → JPEG lossy (quality 85). Terminal backgrounds are
///   displayed behind ≥3 % opacity + optional blur, so sub-pixel lossy artefacts
///   are completely invisible. JPEG is dramatically smaller than lossless WebP for
///   photographic / high-detail content (typically 5-10×).
/// - Images **with alpha** → Lossless WebP to preserve transparency.
///
/// Uses a timestamp-based filename (`bg_{unix_secs}.{ext}`) so that WebView's
/// HTTP cache is naturally busted every time the user changes their wallpaper.
fn process_static_image(source: &std::path::Path, bg_dir: &std::path::Path) -> Result<PathBuf, String> {
    use image::GenericImageView;
    use std::io::Cursor;

    let img = image::open(source)
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    let (w, h) = img.dimensions();

    // Resize if either dimension exceeds MAX_DIMENSION
    let img = if w > MAX_DIMENSION || h > MAX_DIMENSION {
        tracing::info!(
            "Resizing background image from {}x{} to fit within {}px",
            w, h, MAX_DIMENSION
        );
        img.resize(MAX_DIMENSION, MAX_DIMENSION, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    // Timestamp for cache-busting filename (nanosecond precision)
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    // Encode to an in-memory buffer, then write to disk once.
    let has_alpha = img.color().has_alpha();

    let (buf, ext) = if has_alpha {
        // Alpha channel present → lossless WebP
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        let mut cursor = Cursor::new(Vec::new());
        let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut cursor);
        encoder
            .encode(&rgba, width, height, image::ExtendedColorType::Rgba8)
            .map_err(|e| format!("WebP encoding failed: {}", e))?;
        (cursor.into_inner(), "webp")
    } else {
        // No alpha → lossy JPEG (quality 85)
        let rgb = img.to_rgb8();
        let (width, height) = rgb.dimensions();
        let mut cursor = Cursor::new(Vec::new());
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 85);
        encoder
            .encode(&rgb, width, height, image::ExtendedColorType::Rgb8)
            .map_err(|e| format!("JPEG encoding failed: {}", e))?;
        (cursor.into_inner(), "jpg")
    };

    let dest = bg_dir.join(format!("bg_{}.{}", ts, ext));
    std::fs::write(&dest, &buf)
        .map_err(|e| format!("Failed to write output file: {}", e))?;

    Ok(dest)
}

/// List all background images in the gallery.
///
/// Returns paths sorted by modification time (newest first).
#[tauri::command]
pub async fn list_terminal_backgrounds(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let bg_dir = get_backgrounds_dir(&app)?;

    if !bg_dir.exists() {
        return Ok(vec![]);
    }

    // Grant scope so frontend can show previews
    app.asset_protocol_scope()
        .allow_directory(&bg_dir, false)
        .map_err(|e| format!("Failed to grant backgrounds dir: {}", e))?;

    let mut files: Vec<(PathBuf, std::time::SystemTime)> = vec![];
    let entries = std::fs::read_dir(&bg_dir)
        .map_err(|e| format!("Failed to read backgrounds directory: {}", e))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();
        if path.is_file() {
            let mtime = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            files.push((path, mtime));
        }
    }

    // Sort newest first
    files.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(files.into_iter().map(|(p, _)| p.to_string_lossy().to_string()).collect())
}

/// Delete a specific background image from the gallery.
#[tauri::command]
pub async fn delete_terminal_background(
    app: tauri::AppHandle,
    path: String,
) -> Result<(), String> {
    // Canonicalize both paths to prevent directory-traversal attacks
    let bg_dir = get_backgrounds_dir(&app)?;
    let canonical_dir = bg_dir.canonicalize()
        .map_err(|e| format!("Cannot resolve backgrounds dir: {}", e))?;
    let file = std::path::Path::new(&path);
    let canonical_file = file.canonicalize()
        .map_err(|e| format!("Cannot resolve target path: {}", e))?;

    if !canonical_file.starts_with(&canonical_dir) {
        return Err("Refusing to delete file outside the backgrounds directory".into());
    }

    if canonical_file.is_file() {
        std::fs::remove_file(&canonical_file)
            .map_err(|e| format!("Failed to delete background: {}", e))?;
        tracing::info!("Deleted terminal background: {}", path);
    }
    Ok(())
}

/// Clear all terminal background images.
#[tauri::command]
pub async fn clear_terminal_background(app: tauri::AppHandle) -> Result<(), String> {
    let bg_dir = get_backgrounds_dir(&app)?;
    if bg_dir.exists() {
        let mut failures: Vec<String> = Vec::new();
        let entries = std::fs::read_dir(&bg_dir)
            .map_err(|e| format!("Failed to read backgrounds directory: {}", e))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();
            if path.is_file() {
                if let Err(e) = std::fs::remove_file(&path) {
                    failures.push(format!("{}: {}", path.display(), e));
                }
            }
        }
        if !failures.is_empty() {
            return Err(format!("Failed to remove {} file(s): {}", failures.len(), failures.join("; ")));
        }
    }

    // NOTE: Tauri 2.x `AssetProtocolScope` does not expose a runtime API to
    // revoke a previously granted `allow_directory`.  The scope entry will be
    // cleaned up on the next application restart.  This is acceptable because
    // the directory is now empty, so there is nothing for a malicious actor to
    // read through the `asset://` protocol.  If Tauri adds a `deny_directory`
    // or `revoke_scope` API in the future, call it here.

    tracing::info!("Terminal background cleared");
    Ok(())
}

/// Initialize the terminal background on app startup.
///
/// Re-grants the backgrounds directory on the asset protocol scope and returns
/// all background image paths (newest first). The frontend uses the stored
/// `backgroundImage` setting to determine which one is active.
#[tauri::command]
pub async fn init_terminal_background(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let bg_dir = get_backgrounds_dir(&app)?;

    if !bg_dir.exists() {
        return Ok(vec![]);
    }

    // Re-grant scope (idempotent — no error if already granted)
    app.asset_protocol_scope()
        .allow_directory(&bg_dir, false)
        .map_err(|e| format!("Failed to grant backgrounds dir: {}", e))?;

    // Collect all background files, sorted newest first
    let mut files: Vec<(PathBuf, std::time::SystemTime)> = vec![];
    let entries = std::fs::read_dir(&bg_dir)
        .map_err(|e| format!("Failed to read backgrounds directory: {}", e))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();
        if path.is_file() {
            let mtime = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            files.push((path, mtime));
        }
    }

    files.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(files.into_iter().map(|(p, _)| p.to_string_lossy().to_string()).collect())
}
