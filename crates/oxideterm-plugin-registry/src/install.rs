// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Package install, extraction, checksum, and update helpers.

use super::*;

pub(crate) fn install_native_plugin_package_bytes(
    settings_path: &Path,
    package_bytes: &[u8],
    checksum: Option<&str>,
    overwrite: bool,
) -> Result<NativePluginUrlInstallResult, String> {
    if package_bytes.len() as u64 > PLUGIN_PACKAGE_MAX_BYTES {
        return Err(format!(
            "Plugin package too large: {} bytes (max {} bytes)",
            package_bytes.len(),
            PLUGIN_PACKAGE_MAX_BYTES
        ));
    }
    let actual_checksum = native_plugin_sha256_hex(package_bytes);
    if let Some(expected_checksum) = checksum {
        verify_native_plugin_checksum(package_bytes, expected_checksum)?;
    }

    let plugins_dir = native_plugins_dir(settings_path);
    fs::create_dir_all(&plugins_dir)
        .map_err(|error| format!("Failed to create plugins directory: {error}"))?;
    let staging_dir = plugins_dir.join(native_plugin_staging_dir_name("url-install"));
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir)
            .map_err(|error| format!("Failed to clean staging dir: {error}"))?;
    }
    fs::create_dir_all(&staging_dir)
        .map_err(|error| format!("Failed to create staging dir: {error}"))?;

    let install_result = (|| {
        extract_native_plugin_zip(package_bytes, &staging_dir)?;
        let source_dir = native_plugin_package_root(&staging_dir)?;
        let manifest = read_native_plugin_manifest_from_dir(&source_dir)?;
        validate_native_plugin_id(&manifest.id)
            .map_err(|error| format!("Invalid plugin ID in manifest: {error}"))?;
        let dest_dir = plugins_dir.join(&manifest.id);
        let backup_dir = plugins_dir.join(format!(".{}-backup", manifest.id));
        let replaced_existing = dest_dir.exists();
        if replaced_existing && !overwrite {
            return Err(format!("PLUGIN_ID_CONFLICT:{}", manifest.id));
        }
        if backup_dir.exists() {
            fs::remove_dir_all(&backup_dir)
                .map_err(|error| format!("Failed to remove stale backup: {error}"))?;
        }
        if dest_dir.exists() {
            fs::rename(&dest_dir, &backup_dir)
                .map_err(|error| format!("Failed to backup old plugin: {error}"))?;
        }

        // Install uses staging + backup under the plugin directory so the final
        // rename is same-filesystem and rollback can restore the previous copy.
        match fs::rename(&source_dir, &dest_dir) {
            Ok(()) => {
                if backup_dir.exists() {
                    fs::remove_dir_all(&backup_dir)
                        .map_err(|error| format!("Failed to remove plugin backup: {error}"))?;
                }
            }
            Err(error) => {
                if backup_dir.exists() {
                    fs::rename(&backup_dir, &dest_dir).map_err(|restore_error| {
                        format!(
                            "Failed to finalize plugin install: {error}. Rollback also failed: {restore_error}"
                        )
                    })?;
                }
                return Err(format!("Failed to finalize plugin install: {error}"));
            }
        }

        Ok(NativePluginUrlInstallResult {
            manifest,
            checksum: actual_checksum,
            replaced_existing,
        })
    })();

    if staging_dir.exists() {
        let _ = fs::remove_dir_all(&staging_dir);
    }
    install_result
}

#[allow(dead_code)]
pub(crate) fn extract_native_plugin_zip(package_bytes: &[u8], dest: &Path) -> Result<(), String> {
    let cursor = Cursor::new(package_bytes);
    let mut archive =
        ZipArchive::new(cursor).map_err(|error| format!("Invalid ZIP archive: {error}"))?;
    if archive.len() > PLUGIN_PACKAGE_MAX_ENTRIES {
        return Err(format!(
            "Plugin archive contains too many entries: {} (max {})",
            archive.len(),
            PLUGIN_PACKAGE_MAX_ENTRIES
        ));
    }

    let mut extracted_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| format!("Failed to read ZIP entry {index}: {error}"))?;
        if native_plugin_zip_entry_is_symlink(file.unix_mode()) {
            return Err(format!(
                "Plugin archive contains unsupported symlink entry: {}",
                file.name()
            ));
        }
        let relative_path = file
            .enclosed_name()
            .ok_or_else(|| format!("Plugin archive entry escapes target dir: {}", file.name()))?
            .to_path_buf();
        let out_path = dest.join(relative_path);
        if file.is_dir() {
            fs::create_dir_all(&out_path)
                .map_err(|error| format!("Failed to create dir {:?}: {error}", out_path))?;
            continue;
        }
        extracted_bytes = extracted_bytes
            .checked_add(file.size())
            .ok_or_else(|| "Plugin archive extracted size overflowed".to_string())?;
        if extracted_bytes > PLUGIN_PACKAGE_MAX_EXTRACTED_BYTES {
            return Err(format!(
                "Plugin archive expands to {} bytes (max {} bytes)",
                extracted_bytes, PLUGIN_PACKAGE_MAX_EXTRACTED_BYTES
            ));
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to create parent dir {:?}: {error}", parent))?;
        }
        let mut out_file = fs::File::create(&out_path)
            .map_err(|error| format!("Failed to create file {:?}: {error}", out_path))?;
        std::io::copy(&mut file, &mut out_file)
            .map_err(|error| format!("Failed to write file {:?}: {error}", out_path))?;
    }
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn native_plugin_package_root(staging_dir: &Path) -> Result<PathBuf, String> {
    if staging_dir.join(PLUGIN_MANIFEST_FILENAME).exists() {
        return Ok(staging_dir.to_path_buf());
    }
    let mut candidates = Vec::new();
    for entry in fs::read_dir(staging_dir)
        .map_err(|error| format!("Failed to read staging directory: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Failed to read staging entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Failed to inspect staging entry: {error}"))?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() && entry.path().join(PLUGIN_MANIFEST_FILENAME).exists() {
            candidates.push(entry.path());
        }
    }
    match candidates.len() {
        1 => Ok(candidates.remove(0)),
        0 => Err("No plugin.json found in package (checked root and subdirectories)".to_string()),
        _ => Err("Multiple nested plugin.json files found in package".to_string()),
    }
}

#[allow(dead_code)]
pub(crate) fn read_native_plugin_manifest_from_dir(
    plugin_dir: &Path,
) -> Result<NativePluginManifest, String> {
    let manifest_path = plugin_dir.join(PLUGIN_MANIFEST_FILENAME);
    let manifest_json = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("Failed to read plugin.json: {error}"))?;
    serde_json::from_str(&manifest_json).map_err(|error| format!("Invalid plugin.json: {error}"))
}

#[allow(dead_code)]
pub(crate) fn verify_native_plugin_checksum(
    package_bytes: &[u8],
    expected: &str,
) -> Result<(), String> {
    let actual = native_plugin_sha256_hex(package_bytes);
    let expected_hex = expected
        .strip_prefix("sha256:")
        .unwrap_or(expected)
        .to_lowercase();
    if actual != expected_hex {
        return Err(format!(
            "Checksum mismatch: expected {expected_hex}, got {actual}"
        ));
    }
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn native_plugin_sha256_hex(package_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(package_bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[allow(dead_code)]
pub(crate) fn native_plugin_zip_entry_is_symlink(unix_mode: Option<u32>) -> bool {
    unix_mode.is_some_and(|mode| (mode & 0o170000) == 0o120000)
}

#[allow(dead_code)]
pub(crate) fn native_plugin_staging_dir_name(prefix: &str) -> String {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!(".{prefix}-{timestamp_ms}")
}

#[allow(dead_code)]
pub(crate) fn native_plugin_version_is_newer(new_version: &str, old_version: &str) -> bool {
    let new_parts = native_plugin_version_parts(new_version);
    let old_parts = native_plugin_version_parts(old_version);
    for index in 0..new_parts.len().max(old_parts.len()) {
        let new_part = new_parts.get(index).copied().unwrap_or(0);
        let old_part = old_parts.get(index).copied().unwrap_or(0);
        if new_part > old_part {
            return true;
        }
        if new_part < old_part {
            return false;
        }
    }
    false
}

#[allow(dead_code)]
pub(crate) fn native_plugin_version_parts(version: &str) -> Vec<u32> {
    version
        .split('.')
        .filter_map(|part| part.parse::<u32>().ok())
        .collect()
}

#[allow(dead_code)]
pub(crate) fn validate_native_plugin_package_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|error| format!("Invalid URL: {error}"))?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        scheme => Err(format!(
            "Unsupported URL scheme: {scheme}. Only http and https are allowed."
        )),
    }
}
