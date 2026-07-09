// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(unix)]
use std::fs::File;

use oxideterm_connections::oxide_file::EncryptedPluginSetting;
use serde::{Deserialize, Serialize};

const PLUGIN_SETTINGS_FILENAME: &str = "plugin-settings.json";
const PLUGIN_SETTINGS_SCHEMA_VERSION: u32 = 1;
const MAX_ATOMIC_TEMP_ATTEMPTS: usize = 128;
static ATOMIC_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_ATOMIC_REPLACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FAIL_NEXT_RESTORE_DELETE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginSettingsSnapshot {
    version: u32,
    settings: Vec<EncryptedPluginSetting>,
}

enum PluginSettingsFileState {
    Missing,
    Present(Vec<u8>),
}

/// Opaque on-disk state used to restore plugin settings after a failed transaction.
pub struct PluginSettingsCheckpoint {
    state: PluginSettingsFileState,
}

pub fn plugin_settings_path(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join(PLUGIN_SETTINGS_FILENAME)
}

pub fn load_plugin_settings(settings_path: &Path) -> Result<Vec<EncryptedPluginSetting>, String> {
    let path = plugin_settings_path(settings_path);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    if contents.trim().is_empty() {
        return Ok(Vec::new());
    }
    let snapshot =
        serde_json::from_str::<PluginSettingsSnapshot>(&contents).map_err(|err| err.to_string())?;
    Ok(snapshot.settings)
}

pub fn checkpoint_plugin_settings(
    settings_path: &Path,
) -> Result<PluginSettingsCheckpoint, String> {
    let path = plugin_settings_path(settings_path);
    let state = match fs::read(&path) {
        Ok(contents) => PluginSettingsFileState::Present(contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => PluginSettingsFileState::Missing,
        Err(error) => return Err(error.to_string()),
    };
    Ok(PluginSettingsCheckpoint { state })
}

pub fn restore_plugin_settings(
    settings_path: &Path,
    checkpoint: &PluginSettingsCheckpoint,
) -> Result<(), String> {
    let path = plugin_settings_path(settings_path);
    match &checkpoint.state {
        PluginSettingsFileState::Missing => remove_restored_plugin_settings_file(&path),
        PluginSettingsFileState::Present(contents) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            atomic_write_file(&path, contents).map_err(|error| error.to_string())
        }
    }
}

pub fn upsert_plugin_settings(
    settings_path: &Path,
    incoming: &[EncryptedPluginSetting],
) -> Result<usize, String> {
    if incoming.is_empty() {
        return Ok(0);
    }
    let path = plugin_settings_path(settings_path);
    let mut existing = load_plugin_settings(settings_path)?;
    for entry in incoming {
        if let Some(current) = existing
            .iter_mut()
            .find(|candidate| candidate.storage_key == entry.storage_key)
        {
            *current = entry.clone();
        } else {
            existing.push(entry.clone());
        }
    }
    existing.sort_by(|left, right| left.storage_key.cmp(&right.storage_key));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let snapshot = PluginSettingsSnapshot {
        version: PLUGIN_SETTINGS_SCHEMA_VERSION,
        settings: existing,
    };
    let json = serde_json::to_vec_pretty(&snapshot).map_err(|err| err.to_string())?;
    atomic_write_file(&path, &json).map_err(|err| err.to_string())?;
    Ok(incoming.len())
}

pub fn plugin_settings_revision_map(
    settings_path: &Path,
) -> Result<BTreeMap<String, String>, String> {
    let entries = load_plugin_settings(settings_path)?;
    let mut grouped = BTreeMap::<String, Vec<EncryptedPluginSetting>>::new();
    for entry in entries {
        let Some(plugin_id) = plugin_id_from_setting_storage_key(&entry.storage_key) else {
            continue;
        };
        grouped.entry(plugin_id).or_default().push(entry);
    }

    let mut revisions = BTreeMap::new();
    for (plugin_id, mut entries) in grouped {
        entries.sort_by(|left, right| left.storage_key.cmp(&right.storage_key));
        let normalized = entries
            .into_iter()
            .map(|entry| vec![entry.storage_key, entry.serialized_value])
            .collect::<Vec<_>>();
        let json = serde_json::to_string(&normalized).map_err(|err| err.to_string())?;
        revisions.insert(plugin_id, tauri_fnv1a_stable_hash_text(&json));
    }
    Ok(revisions)
}

pub fn plugin_id_from_setting_storage_key(storage_key: &str) -> Option<String> {
    const PREFIX: &str = "oxide-plugin-";
    const SEPARATOR: &str = "-setting-";
    let remainder = storage_key.strip_prefix(PREFIX)?;
    let separator_index = remainder.find(SEPARATOR)?;
    let plugin_id = &remainder[..separator_index];
    let setting_id = &remainder[separator_index + SEPARATOR.len()..];
    (!plugin_id.is_empty() && !setting_id.is_empty()).then(|| plugin_id.to_string())
}

fn tauri_fnv1a_stable_hash_text(text: &str) -> String {
    let mut hash = 2166136261u32;
    for code_unit in text.encode_utf16() {
        hash ^= u32::from(code_unit);
        hash = hash.wrapping_mul(16777619);
    }
    format!("fnv1a-{hash:x}")
}

fn atomic_write_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "plugin settings path has no file name",
        )
    })?;
    let (temporary_path, mut temporary_file) = (0..MAX_ATOMIC_TEMP_ATTEMPTS)
        .find_map(|_| {
            let sequence = ATOMIC_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let mut temporary_name = OsString::from(".");
            temporary_name.push(file_name);
            temporary_name.push(format!(".{}.{sequence}.tmp", std::process::id()));
            let temporary_path = parent.join(temporary_name);
            match OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary_path)
            {
                Ok(file) => Some(Ok((temporary_path, file))),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(error)),
            }
        })
        .transpose()?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "plugin settings atomic temp path exhausted",
            )
        })?;

    // The destination changes only after the complete replacement is durable.
    let write_result = (|| {
        temporary_file.write_all(contents)?;
        temporary_file.flush()?;
        temporary_file.sync_all()?;
        drop(temporary_file);
        fail_before_atomic_replace_for_tests()?;
        atomic_replace_file(&temporary_path, path)?;
        sync_parent_directory(parent)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    write_result
}

fn remove_restored_plugin_settings_file(path: &Path) -> Result<(), String> {
    fail_before_restore_delete_for_tests().map_err(|error| error.to_string())?;
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    sync_parent_directory(parent).map_err(|error| error.to_string())
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> io::Result<()> {
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path) -> io::Result<()> {
    // MoveFileExW uses WRITE_THROUGH on Windows; opening directories requires
    // platform-specific sharing flags and does not add durability here.
    Ok(())
}

#[cfg(test)]
fn fail_before_atomic_replace_for_tests() -> io::Result<()> {
    FAIL_NEXT_ATOMIC_REPLACE.with(|fail| {
        if fail.replace(false) {
            Err(io::Error::other(
                "injected plugin settings atomic replace failure",
            ))
        } else {
            Ok(())
        }
    })
}

#[cfg(not(test))]
fn fail_before_atomic_replace_for_tests() -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
fn fail_before_restore_delete_for_tests() -> io::Result<()> {
    FAIL_NEXT_RESTORE_DELETE.with(|fail| {
        if fail.replace(false) {
            Err(io::Error::other(
                "injected plugin settings restore delete failure",
            ))
        } else {
            Ok(())
        }
    })
}

#[cfg(not(test))]
fn fail_before_restore_delete_for_tests() -> io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replaced = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let sequence = ATOMIC_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "oxideterm-plugin-settings-test-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("temporary directory should be created");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            // Tests must not leave encrypted fixture data in the system temp directory.
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn encrypted_setting(storage_key: &str, serialized_value: &str) -> EncryptedPluginSetting {
        EncryptedPluginSetting {
            storage_key: storage_key.to_string(),
            serialized_value: serialized_value.to_string(),
        }
    }

    #[test]
    fn upsert_failure_keeps_existing_file_and_removes_temporary_file() {
        let directory = TestDirectory::new();
        let settings_path = directory.path().join("settings.json");
        let plugin_path = plugin_settings_path(&settings_path);
        upsert_plugin_settings(
            &settings_path,
            &[encrypted_setting(
                "oxide-plugin-a-setting-token",
                "old-ciphertext",
            )],
        )
        .expect("initial plugin settings should be saved");
        let original = fs::read(&plugin_path).expect("initial file should be readable");

        FAIL_NEXT_ATOMIC_REPLACE.with(|fail| fail.set(true));
        let error = upsert_plugin_settings(
            &settings_path,
            &[encrypted_setting(
                "oxide-plugin-a-setting-token",
                "new-ciphertext",
            )],
        )
        .expect_err("injected replacement failure should be returned");

        assert!(!error.contains("new-ciphertext"));
        assert_eq!(
            fs::read(&plugin_path).expect("original file should remain readable"),
            original
        );
        let temporary_files = fs::read_dir(directory.path())
            .expect("directory should be readable")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(temporary_files, 0);
    }

    #[test]
    fn checkpoint_restores_present_file_byte_for_byte() {
        let directory = TestDirectory::new();
        let settings_path = directory.path().join("settings.json");
        let plugin_path = plugin_settings_path(&settings_path);
        let original = b"{\n  \"futureField\": \"opaque-ciphertext\"\n}\n";
        fs::write(&plugin_path, original).expect("original file should be written");
        let checkpoint = checkpoint_plugin_settings(&settings_path)
            .expect("present plugin settings should be checkpointed");
        fs::write(&plugin_path, b"replacement").expect("replacement should be written");

        restore_plugin_settings(&settings_path, &checkpoint)
            .expect("present checkpoint should be restored");

        assert_eq!(
            fs::read(&plugin_path).expect("restored file should be readable"),
            original
        );
    }

    #[test]
    fn checkpoint_restores_missing_file_state() {
        let directory = TestDirectory::new();
        let settings_path = directory.path().join("settings.json");
        let plugin_path = plugin_settings_path(&settings_path);
        let checkpoint = checkpoint_plugin_settings(&settings_path)
            .expect("missing plugin settings should be checkpointed");
        fs::write(&plugin_path, b"new file").expect("new file should be written");

        restore_plugin_settings(&settings_path, &checkpoint)
            .expect("missing checkpoint should remove the new file");

        assert!(!plugin_path.exists());
    }

    #[test]
    fn missing_restore_failure_keeps_new_file() {
        let directory = TestDirectory::new();
        let settings_path = directory.path().join("settings.json");
        let plugin_path = plugin_settings_path(&settings_path);
        let checkpoint = checkpoint_plugin_settings(&settings_path)
            .expect("missing plugin settings should be checkpointed");
        fs::write(&plugin_path, b"new file").expect("new file should be written");

        FAIL_NEXT_RESTORE_DELETE.with(|fail| fail.set(true));
        restore_plugin_settings(&settings_path, &checkpoint)
            .expect_err("injected delete failure should be returned");

        assert_eq!(
            fs::read(&plugin_path).expect("new file should remain readable"),
            b"new file"
        );
    }
}
