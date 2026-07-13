// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::{Context, Result, anyhow};

const PRE_2_0_SOURCE_VERSION: &str = "pre-2.0.0";
const SNAPSHOT_MARKER_CONTENT: &[u8] = b"OxideTerm migration snapshot complete\n";
const NOTICE_MARKER_CONTENT: &[u8] = b"OxideTerm 2.0 migration notice acknowledged\n";
const MAX_TEMP_PATH_ATTEMPTS: usize = 128;
static TEMP_PATH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
thread_local! {
    static FAIL_BEFORE_SNAPSHOT_COMMIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MigrationSnapshotOutcome {
    Created,
    AlreadyComplete,
    NoSourceData,
}

pub(crate) fn ensure_pre_2_0_migration_snapshot(
    settings_path: &Path,
) -> Result<MigrationSnapshotOutcome> {
    let data_dir = settings_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| anyhow!("settings path has no data directory"))?;
    ensure_versioned_snapshot(data_dir, PRE_2_0_SOURCE_VERSION)
}

pub(crate) fn pre_2_0_migration_notice_pending(settings_path: &Path) -> Result<bool> {
    let data_dir = settings_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| anyhow!("settings path has no data directory"))?;
    let paths = SnapshotPaths::new(data_dir, PRE_2_0_SOURCE_VERSION)?;

    // A backup directory proves mutable data existed before the 2.0 migration.
    // The snapshot completion marker alone also exists for clean installations.
    Ok(paths.snapshot.is_dir() && !paths.notice_marker.exists())
}

pub(crate) fn acknowledge_pre_2_0_migration_notice(settings_path: &Path) -> Result<()> {
    let data_dir = settings_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| anyhow!("settings path has no data directory"))?;
    let paths = SnapshotPaths::new(data_dir, PRE_2_0_SOURCE_VERSION)?;
    write_marker(&paths.notice_marker, NOTICE_MARKER_CONTENT)
}

fn ensure_versioned_snapshot(
    data_dir: &Path,
    source_version: &str,
) -> Result<MigrationSnapshotOutcome> {
    let paths = SnapshotPaths::new(data_dir, source_version)?;
    if paths.marker.exists() {
        return Ok(MigrationSnapshotOutcome::AlreadyComplete);
    }

    // A committed snapshot is complete even if the process stopped before the
    // final marker write. Reuse it instead of creating an unbounded backup.
    if paths.snapshot.exists() {
        write_completion_marker(&paths.marker)?;
        return Ok(MigrationSnapshotOutcome::AlreadyComplete);
    }

    if !data_dir.exists() {
        write_completion_marker(&paths.marker)?;
        return Ok(MigrationSnapshotOutcome::NoSourceData);
    }
    if !data_dir.is_dir() {
        return Err(anyhow!(
            "OxideTerm data path is not a directory: {}",
            data_dir.display()
        ));
    }
    let mut ignored_runtime_paths =
        crate::single_instance::single_instance_runtime_paths_for_data_dir(data_dir).to_vec();
    if let Some(portable_lock_path) = oxideterm_portable_runtime::portable_instance_lock_path()
        .ok()
        .flatten()
    {
        ignored_runtime_paths.push(portable_lock_path);
    }
    if !directory_contains_migration_source_data(data_dir, &ignored_runtime_paths)? {
        write_completion_marker(&paths.marker)?;
        return Ok(MigrationSnapshotOutcome::NoSourceData);
    }

    let temp_snapshot = create_temp_snapshot_path(&paths.snapshot)?;
    let copy_result = (|| {
        let source_permissions = fs::metadata(data_dir)
            .context("failed to inspect migration source directory permissions")?
            .permissions();
        fs::set_permissions(&temp_snapshot, source_permissions)
            .context("failed to protect migration snapshot staging directory")?;
        copy_directory_tree(data_dir, &temp_snapshot)?;
        sync_directory_tree(&temp_snapshot)?;
        fail_before_snapshot_commit_for_tests()?;
        commit_snapshot_directory(&temp_snapshot, &paths.snapshot)?;
        write_completion_marker(&paths.marker)
    })();

    if copy_result.is_err() {
        let _ = fs::remove_dir_all(&temp_snapshot);
    }
    copy_result.map(|_| MigrationSnapshotOutcome::Created)
}

fn directory_contains_migration_source_data(
    data_dir: &Path,
    ignored_runtime_paths: &[PathBuf],
) -> Result<bool> {
    for entry in fs::read_dir(data_dir)
        .with_context(|| format!("failed to inspect migration source {}", data_dir.display()))?
    {
        let path = entry
            .context("failed to inspect migration source entry")?
            .path();
        if !ignored_runtime_paths.contains(&path) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Debug)]
struct SnapshotPaths {
    snapshot: PathBuf,
    marker: PathBuf,
    notice_marker: PathBuf,
}

impl SnapshotPaths {
    fn new(data_dir: &Path, source_version: &str) -> Result<Self> {
        let parent = data_dir
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| anyhow!("OxideTerm data directory has no parent"))?;
        let data_name = data_dir
            .file_name()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| anyhow!("OxideTerm data directory has no name"))?;
        let safe_version = safe_path_component(source_version)?;

        let mut snapshot_name = OsString::from(data_name);
        snapshot_name.push(format!(".migration-backup-{safe_version}"));
        let snapshot = parent.join(snapshot_name);

        let mut marker_name = snapshot.as_os_str().to_os_string();
        marker_name.push(".complete");
        let mut notice_marker_name = snapshot.as_os_str().to_os_string();
        notice_marker_name.push(".notice-complete");
        Ok(Self {
            snapshot,
            marker: PathBuf::from(marker_name),
            notice_marker: PathBuf::from(notice_marker_name),
        })
    }
}

fn safe_path_component(value: &str) -> Result<&str> {
    let is_safe = !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains(['/', '\\'])
        && Path::new(value).components().count() == 1;
    is_safe
        .then_some(value)
        .ok_or_else(|| anyhow!("migration source version is not a safe path component"))
}

fn create_temp_snapshot_path(snapshot_path: &Path) -> Result<PathBuf> {
    let parent = snapshot_path
        .parent()
        .ok_or_else(|| anyhow!("migration snapshot path has no parent"))?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create migration snapshot parent {}",
            parent.display()
        )
    })?;
    let snapshot_name = snapshot_path
        .file_name()
        .ok_or_else(|| anyhow!("migration snapshot path has no name"))?;

    for _ in 0..MAX_TEMP_PATH_ATTEMPTS {
        let sequence = TEMP_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut temp_name = OsString::from(".");
        temp_name.push(snapshot_name);
        temp_name.push(format!(".{}.{sequence}.tmp", std::process::id()));
        let temp_path = parent.join(temp_name);
        match fs::create_dir(&temp_path) {
            Ok(()) => return Ok(temp_path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to create migration snapshot staging directory {}",
                        temp_path.display()
                    )
                });
            }
        }
    }

    Err(anyhow!(
        "migration snapshot staging path attempts exhausted"
    ))
}

fn copy_directory_tree(source: &Path, destination: &Path) -> Result<()> {
    for entry in fs::read_dir(source)
        .with_context(|| format!("failed to read migration source {}", source.display()))?
    {
        let entry = entry.context("failed to read migration source entry")?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path).with_context(|| {
            format!(
                "failed to inspect migration source entry {}",
                source_path.display()
            )
        })?;

        if metadata.is_dir() {
            fs::create_dir(&destination_path).with_context(|| {
                format!(
                    "failed to create migration snapshot directory {}",
                    destination_path.display()
                )
            })?;
            copy_directory_tree(&source_path, &destination_path)?;
            fs::set_permissions(&destination_path, metadata.permissions()).with_context(|| {
                format!(
                    "failed to preserve migration snapshot permissions for {}",
                    destination_path.display()
                )
            })?;
        } else if metadata.is_file() {
            copy_file(&source_path, &destination_path, metadata.permissions())?;
        } else if metadata.file_type().is_symlink() {
            copy_symlink(&source_path, &destination_path)?;
        } else {
            return Err(anyhow!(
                "unsupported migration source entry type: {}",
                source_path.display()
            ));
        }
    }
    Ok(())
}

fn copy_file(source: &Path, destination: &Path, permissions: fs::Permissions) -> Result<()> {
    let mut source_file = File::open(source)
        .with_context(|| format!("failed to open migration source file {}", source.display()))?;
    let mut destination_file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .with_context(|| {
            format!(
                "failed to create migration snapshot file {}",
                destination.display()
            )
        })?;
    io::copy(&mut source_file, &mut destination_file)
        .with_context(|| format!("failed to copy migration source file {}", source.display()))?;
    destination_file.flush()?;
    fs::set_permissions(destination, permissions)?;
    destination_file.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn copy_symlink(source: &Path, destination: &Path) -> Result<()> {
    let target = fs::read_link(source)
        .with_context(|| format!("failed to read symlink {}", source.display()))?;
    std::os::unix::fs::symlink(target, destination)
        .with_context(|| format!("failed to copy symlink {}", source.display()))
}

#[cfg(windows)]
fn copy_symlink(source: &Path, destination: &Path) -> Result<()> {
    let target = fs::read_link(source)
        .with_context(|| format!("failed to read symlink {}", source.display()))?;
    let target_metadata = fs::metadata(source)
        .with_context(|| format!("failed to inspect symlink target {}", source.display()))?;
    if target_metadata.is_dir() {
        std::os::windows::fs::symlink_dir(target, destination)
    } else {
        std::os::windows::fs::symlink_file(target, destination)
    }
    .with_context(|| format!("failed to copy symlink {}", source.display()))
}

fn sync_directory_tree(path: &Path) -> Result<()> {
    for entry in fs::read_dir(path)
        .with_context(|| format!("failed to read snapshot directory {}", path.display()))?
    {
        let entry = entry.context("failed to read snapshot entry during sync")?;
        if entry
            .file_type()
            .context("failed to inspect snapshot entry during sync")?
            .is_dir()
        {
            sync_directory_tree(&entry.path())?;
        }
    }
    sync_directory(path)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .with_context(|| format!("failed to open directory for sync {}", path.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync directory {}", path.display()))
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> Result<()> {
    // Windows does not support opening directories through std::fs::File.
    Ok(())
}

fn commit_snapshot_directory(staged: &Path, snapshot: &Path) -> Result<()> {
    match fs::rename(staged, snapshot) {
        Ok(()) => {
            if let Some(parent) = snapshot.parent() {
                sync_directory(parent)?;
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists && snapshot.is_dir() => {
            fs::remove_dir_all(staged).with_context(|| {
                format!(
                    "failed to discard duplicate migration snapshot {}",
                    staged.display()
                )
            })
        }
        Err(error) => Err(error)
            .with_context(|| format!("failed to commit migration snapshot {}", snapshot.display())),
    }
}

fn write_completion_marker(marker: &Path) -> Result<()> {
    write_marker(marker, SNAPSHOT_MARKER_CONTENT)
}

fn write_marker(marker: &Path, content: &[u8]) -> Result<()> {
    if marker.exists() {
        return Ok(());
    }
    let parent = marker
        .parent()
        .ok_or_else(|| anyhow!("migration marker path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temp_marker = unique_temp_file_path(marker, "marker")?;
    let write_result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_marker)?;
        file.write_all(content)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        match fs::rename(&temp_marker, marker) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && marker.is_file() => {
                fs::remove_file(&temp_marker)?;
            }
            Err(error) => return Err(error.into()),
        }
        sync_directory(parent)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_marker);
    }
    write_result.with_context(|| {
        format!(
            "failed to write migration snapshot marker {}",
            marker.display()
        )
    })
}

fn unique_temp_file_path(path: &Path, kind: &str) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("temporary path has no parent"))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("temporary path has no file name"))?;
    let sequence = TEMP_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut temp_name = OsString::from(".");
    temp_name.push(file_name);
    temp_name.push(format!(".{}.{sequence}.{kind}.tmp", std::process::id()));
    Ok(parent.join(temp_name))
}

#[cfg(test)]
fn fail_before_snapshot_commit_for_tests() -> io::Result<()> {
    FAIL_BEFORE_SNAPSHOT_COMMIT.with(|fail| {
        if fail.replace(false) {
            Err(io::Error::other("injected failure before snapshot commit"))
        } else {
            Ok(())
        }
    })
}

#[cfg(not(test))]
fn fail_before_snapshot_commit_for_tests() -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(name: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "oxideterm-migration-snapshot-{name}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn snapshot_is_created_once_and_preserves_original_contents() {
        let root = TestDirectory::new("once");
        let data_dir = root.path().join("data");
        fs::create_dir_all(data_dir.join("nested")).unwrap();
        fs::write(data_dir.join("settings.json"), b"old settings").unwrap();
        fs::write(data_dir.join("nested/connections.json"), b"old connections").unwrap();

        assert_eq!(
            ensure_versioned_snapshot(&data_dir, PRE_2_0_SOURCE_VERSION).unwrap(),
            MigrationSnapshotOutcome::Created
        );
        let paths = SnapshotPaths::new(&data_dir, PRE_2_0_SOURCE_VERSION).unwrap();
        assert_eq!(
            fs::read(paths.snapshot.join("settings.json")).unwrap(),
            b"old settings"
        );
        assert_eq!(
            fs::read(paths.snapshot.join("nested/connections.json")).unwrap(),
            b"old connections"
        );
        assert!(paths.marker.is_file());

        fs::write(data_dir.join("settings.json"), b"new settings").unwrap();
        assert_eq!(
            ensure_versioned_snapshot(&data_dir, PRE_2_0_SOURCE_VERSION).unwrap(),
            MigrationSnapshotOutcome::AlreadyComplete
        );
        assert_eq!(
            fs::read(paths.snapshot.join("settings.json")).unwrap(),
            b"old settings"
        );
        let backup_count = fs::read_dir(root.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("data.migration-backup-")
                    && entry.path().is_dir()
            })
            .count();
        assert_eq!(backup_count, 1);
    }

    #[test]
    fn committed_snapshot_without_marker_is_reused() {
        let root = TestDirectory::new("marker-recovery");
        let data_dir = root.path().join("data");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("settings.json"), b"old settings").unwrap();
        let paths = SnapshotPaths::new(&data_dir, PRE_2_0_SOURCE_VERSION).unwrap();
        fs::create_dir_all(&paths.snapshot).unwrap();
        fs::write(paths.snapshot.join("settings.json"), b"committed snapshot").unwrap();

        assert_eq!(
            ensure_versioned_snapshot(&data_dir, PRE_2_0_SOURCE_VERSION).unwrap(),
            MigrationSnapshotOutcome::AlreadyComplete
        );
        assert!(paths.marker.is_file());
        assert_eq!(
            fs::read(paths.snapshot.join("settings.json")).unwrap(),
            b"committed snapshot"
        );
    }

    #[test]
    fn failure_before_commit_leaves_source_and_final_paths_unchanged() {
        let root = TestDirectory::new("failure");
        let data_dir = root.path().join("data");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("settings.json"), b"original").unwrap();
        let paths = SnapshotPaths::new(&data_dir, PRE_2_0_SOURCE_VERSION).unwrap();
        FAIL_BEFORE_SNAPSHOT_COMMIT.with(|fail| fail.set(true));

        let error = ensure_versioned_snapshot(&data_dir, PRE_2_0_SOURCE_VERSION).unwrap_err();
        assert!(error.to_string().contains("injected failure"));
        assert_eq!(
            fs::read(data_dir.join("settings.json")).unwrap(),
            b"original"
        );
        assert!(!paths.snapshot.exists());
        assert!(!paths.marker.exists());
        assert_eq!(
            fs::read_dir(root.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
                .count(),
            0
        );
    }

    #[test]
    fn absent_source_is_marked_complete_without_creating_a_backup() {
        let root = TestDirectory::new("no-source");
        let data_dir = root.path().join("data");
        let paths = SnapshotPaths::new(&data_dir, PRE_2_0_SOURCE_VERSION).unwrap();

        assert_eq!(
            ensure_versioned_snapshot(&data_dir, PRE_2_0_SOURCE_VERSION).unwrap(),
            MigrationSnapshotOutcome::NoSourceData
        );
        assert!(!paths.snapshot.exists());
        assert!(paths.marker.is_file());
    }

    #[test]
    fn empty_or_runtime_only_source_does_not_create_a_backup() {
        let root = TestDirectory::new("runtime-only");
        let data_dir = root.path().join("data");
        let runtime_lock = data_dir.join(".runtime.lock");
        let runtime_state = data_dir.join(".runtime.json");
        fs::create_dir_all(&data_dir).unwrap();

        assert!(!directory_contains_migration_source_data(&data_dir, &[]).unwrap());

        fs::write(&runtime_lock, b"lock").unwrap();
        fs::write(&runtime_state, b"state").unwrap();
        assert!(
            !directory_contains_migration_source_data(
                &data_dir,
                &[runtime_lock.clone(), runtime_state]
            )
            .unwrap()
        );
        assert!(directory_contains_migration_source_data(&data_dir, &[]).unwrap());

        fs::write(data_dir.join("settings.json"), b"legacy settings").unwrap();
        assert!(directory_contains_migration_source_data(&data_dir, &[runtime_lock]).unwrap());
    }

    #[test]
    fn current_single_instance_files_do_not_identify_a_fresh_install_as_legacy() {
        let root = TestDirectory::new("single-instance-only");
        let data_dir = root.path().join("data");
        fs::create_dir_all(&data_dir).unwrap();
        for runtime_path in
            crate::single_instance::single_instance_runtime_paths_for_data_dir(&data_dir)
        {
            fs::write(runtime_path, b"current runtime state").unwrap();
        }

        assert_eq!(
            ensure_versioned_snapshot(&data_dir, PRE_2_0_SOURCE_VERSION).unwrap(),
            MigrationSnapshotOutcome::NoSourceData
        );
        let paths = SnapshotPaths::new(&data_dir, PRE_2_0_SOURCE_VERSION).unwrap();
        assert!(!paths.snapshot.exists());
    }

    #[test]
    fn migration_notice_only_opens_for_existing_snapshot() {
        let root = TestDirectory::new("notice-source");
        let data_dir = root.path().join("data");
        let settings_path = data_dir.join("settings.json");

        assert_eq!(
            ensure_pre_2_0_migration_snapshot(&settings_path).unwrap(),
            MigrationSnapshotOutcome::NoSourceData
        );
        assert!(!pre_2_0_migration_notice_pending(&settings_path).unwrap());

        let second_root = TestDirectory::new("notice-existing");
        let second_data_dir = second_root.path().join("data");
        let second_settings_path = second_data_dir.join("settings.json");
        fs::create_dir_all(&second_data_dir).unwrap();
        fs::write(&second_settings_path, b"legacy settings").unwrap();

        assert_eq!(
            ensure_pre_2_0_migration_snapshot(&second_settings_path).unwrap(),
            MigrationSnapshotOutcome::Created
        );
        assert!(pre_2_0_migration_notice_pending(&second_settings_path).unwrap());
    }

    #[test]
    fn acknowledged_migration_notice_stays_closed() {
        let root = TestDirectory::new("notice-acknowledged");
        let data_dir = root.path().join("data");
        let settings_path = data_dir.join("settings.json");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(&settings_path, b"legacy settings").unwrap();

        ensure_pre_2_0_migration_snapshot(&settings_path).unwrap();
        acknowledge_pre_2_0_migration_notice(&settings_path).unwrap();

        assert!(!pre_2_0_migration_notice_pending(&settings_path).unwrap());
        let paths = SnapshotPaths::new(&data_dir, PRE_2_0_SOURCE_VERSION).unwrap();
        assert_eq!(
            fs::read(paths.notice_marker).unwrap(),
            NOTICE_MARKER_CONTENT
        );
    }
}
