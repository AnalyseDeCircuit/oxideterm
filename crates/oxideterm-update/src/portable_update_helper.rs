// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    fs, io,
    path::{Component, Path, PathBuf},
    process::Command,
};

#[cfg(not(windows))]
use std::time::Duration;

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};

use crate::{
    InstallPackageKind, NativeInstallOutcome, NativeInstallPlan, NativeInstallStatus,
    NativeUpdateError,
};

pub const PORTABLE_UPDATE_MANIFEST_FILENAME: &str = "portable-update.json";
pub const PORTABLE_UPDATE_STAGING_DIR: &str = ".oxideterm-update-staging";
pub const PORTABLE_UPDATE_BACKUP_DIR: &str = ".oxideterm-update-backup";
pub const PORTABLE_UPDATE_HELPER_SUBCOMMAND: &str = "portable";
const PORTABLE_UPDATE_MANIFEST_FORMAT: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableUpdateManifest {
    pub format_version: u32,
    pub app_executable: String,
    pub update_helper: String,
    pub managed_entries: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortableUpdateHelperOptions {
    pub portable_root: PathBuf,
    pub staging_dir: PathBuf,
    pub backup_dir: PathBuf,
    pub app_exe: PathBuf,
    pub wait_pid: Option<u32>,
    pub launch_after_apply: bool,
}

pub fn portable_update_root() -> Result<PathBuf, NativeUpdateError> {
    let info = oxideterm_portable_runtime::portable_info().map_err(|error| {
        NativeUpdateError::State(format!("resolve portable update directory failed: {error}"))
    })?;
    if !info.is_portable {
        return Err(NativeUpdateError::State(
            "portable update requested outside portable mode".to_string(),
        ));
    }
    Ok(info.host_dir.clone())
}

pub fn portable_update_helper_arguments(options: &PortableUpdateHelperOptions) -> Vec<OsString> {
    let mut args = vec![
        OsString::from(PORTABLE_UPDATE_HELPER_SUBCOMMAND),
        OsString::from("--portable-root"),
        options.portable_root.as_os_str().to_os_string(),
        OsString::from("--staging-dir"),
        options.staging_dir.as_os_str().to_os_string(),
        OsString::from("--backup-dir"),
        options.backup_dir.as_os_str().to_os_string(),
        OsString::from("--app-exe"),
        options.app_exe.as_os_str().to_os_string(),
    ];
    if let Some(wait_pid) = options.wait_pid {
        args.push(OsString::from("--wait-pid"));
        args.push(OsString::from(wait_pid.to_string()));
    }
    if options.launch_after_apply {
        args.push(OsString::from("--launch"));
    }
    args
}

pub fn parse_portable_update_helper_options<I, S>(
    args: I,
) -> Result<PortableUpdateHelperOptions, String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut portable_root = None;
    let mut staging_dir = None;
    let mut backup_dir = None;
    let mut app_exe = None;
    let mut wait_pid = None;
    let mut launch_after_apply = false;
    let mut iter = args.into_iter().map(Into::into).skip(1);

    match iter.next() {
        Some(command) if command == OsStr::new(PORTABLE_UPDATE_HELPER_SUBCOMMAND) => {}
        Some(command) => {
            return Err(format!(
                "unknown update helper command: {}",
                command.to_string_lossy()
            ));
        }
        None => return Err("missing update helper command".to_string()),
    }

    while let Some(arg) = iter.next() {
        match arg.to_string_lossy().as_ref() {
            "--portable-root" => {
                portable_root = Some(PathBuf::from(required_arg(&mut iter, "--portable-root")?));
            }
            "--staging-dir" => {
                staging_dir = Some(PathBuf::from(required_arg(&mut iter, "--staging-dir")?));
            }
            "--backup-dir" => {
                backup_dir = Some(PathBuf::from(required_arg(&mut iter, "--backup-dir")?));
            }
            "--app-exe" => {
                app_exe = Some(PathBuf::from(required_arg(&mut iter, "--app-exe")?));
            }
            "--wait-pid" => {
                let raw = required_arg(&mut iter, "--wait-pid")?;
                wait_pid = Some(
                    raw.to_string_lossy()
                        .parse::<u32>()
                        .map_err(|error| format!("invalid --wait-pid value: {error}"))?,
                );
            }
            "--launch" => launch_after_apply = true,
            unknown => return Err(format!("unknown update helper argument: {unknown}")),
        }
    }

    Ok(PortableUpdateHelperOptions {
        portable_root: portable_root.ok_or_else(|| "missing --portable-root".to_string())?,
        staging_dir: staging_dir.ok_or_else(|| "missing --staging-dir".to_string())?,
        backup_dir: backup_dir.ok_or_else(|| "missing --backup-dir".to_string())?,
        app_exe: app_exe.ok_or_else(|| "missing --app-exe".to_string())?,
        wait_pid,
        launch_after_apply,
    })
}

fn required_arg<I>(iter: &mut I, name: &str) -> Result<OsString, String>
where
    I: Iterator<Item = OsString>,
{
    iter.next()
        .ok_or_else(|| format!("missing value for {name}"))
}

pub fn execute_portable_update(
    plan: &NativeInstallPlan,
) -> Result<NativeInstallOutcome, NativeUpdateError> {
    if plan.package_kind != InstallPackageKind::PortableArchive {
        return Err(NativeUpdateError::State(
            "portable update package is not a supported archive".to_string(),
        ));
    }
    let portable_root = plan.portable_root.as_deref().ok_or_else(|| {
        NativeUpdateError::State("portable update directory is unavailable".to_string())
    })?;
    let staged = stage_portable_update(&plan.package_path, portable_root, plan.process_id)?;

    let options = PortableUpdateHelperOptions {
        portable_root: portable_root.to_path_buf(),
        staging_dir: staged.payload_dir,
        backup_dir: portable_root.join(PORTABLE_UPDATE_BACKUP_DIR),
        app_exe: portable_root.join(staged.manifest.app_executable),
        wait_pid: Some(plan.process_id),
        launch_after_apply: true,
    };
    let mut command = Command::new(&staged.detached_helper);
    configure_background_process(&mut command);
    command
        .args(portable_update_helper_arguments(&options))
        .spawn()
        .map_err(|error| {
            NativeUpdateError::State(format!(
                "launch portable update helper failed: {error}; update package retained at {}",
                plan.package_path.display()
            ))
        })?;

    Ok(NativeInstallOutcome {
        status: NativeInstallStatus::ReplacementScheduled,
        message: plan.summary.clone(),
        should_quit_app: true,
    })
}

struct StagedPortableUpdate {
    payload_dir: PathBuf,
    detached_helper: PathBuf,
    manifest: PortableUpdateManifest,
}

fn stage_portable_update(
    package_path: &Path,
    portable_root: &Path,
    process_id: u32,
) -> Result<StagedPortableUpdate, NativeUpdateError> {
    ensure_portable_root(portable_root)?;
    let staging_root = portable_root.join(PORTABLE_UPDATE_STAGING_DIR);
    remove_reserved_update_path(&staging_root)?;
    fs::create_dir(&staging_root).map_err(|error| {
        NativeUpdateError::State(format!(
            "portable folder is not writable: {}; create update staging directory failed: {error}",
            portable_root.display()
        ))
    })?;

    let archive_dir = staging_root.join("archive");
    fs::create_dir(&archive_dir).map_err(portable_io_error("create archive staging directory"))?;
    if let Err(error) = extract_portable_archive(package_path, &archive_dir) {
        let _ = remove_reserved_update_path(&staging_root);
        return Err(error);
    }

    let package_root = single_archive_root(&archive_dir)?;
    let manifest_path = package_root.join(PORTABLE_UPDATE_MANIFEST_FILENAME);
    let manifest_bytes =
        fs::read(&manifest_path).map_err(portable_io_error("read portable update manifest"))?;
    let manifest: PortableUpdateManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|error| {
            NativeUpdateError::State(format!("parse portable update manifest failed: {error}"))
        })?;
    validate_manifest(&manifest, &package_root)?;

    let payload_dir = staging_root.join("payload");
    fs::create_dir(&payload_dir).map_err(portable_io_error(
        "create portable update payload directory",
    ))?;
    for managed_entry in &manifest.managed_entries {
        let source = package_root.join(managed_entry);
        let destination = payload_dir.join(managed_entry);
        fs::rename(&source, &destination)
            .map_err(portable_io_error("stage managed portable update entry"))?;
    }

    let helper_source = payload_dir.join(&manifest.update_helper);
    let helper_suffix = if cfg!(windows) { ".exe" } else { "" };
    let detached_helper = package_path.with_file_name(format!(
        ".oxideterm-update-helper-{process_id}{helper_suffix}"
    ));
    if detached_helper.exists() {
        fs::remove_file(&detached_helper)
            .map_err(portable_io_error("replace detached portable update helper"))?;
    }
    fs::copy(&helper_source, &detached_helper)
        .map_err(portable_io_error("copy detached portable update helper"))?;
    make_executable(&detached_helper)?;

    fs::remove_dir_all(&archive_dir)
        .map_err(portable_io_error("remove extracted portable archive"))?;
    Ok(StagedPortableUpdate {
        payload_dir,
        detached_helper,
        manifest,
    })
}

fn ensure_portable_root(portable_root: &Path) -> Result<(), NativeUpdateError> {
    let metadata = fs::symlink_metadata(portable_root)
        .map_err(portable_io_error("read portable folder metadata"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(NativeUpdateError::State(format!(
            "portable update directory must be a real directory: {}",
            portable_root.display()
        )));
    }
    Ok(())
}

fn extract_portable_archive(
    package_path: &Path,
    destination: &Path,
) -> Result<(), NativeUpdateError> {
    let file_name = package_path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if file_name.ends_with(".zip") {
        extract_zip_archive(package_path, destination)
    } else if file_name.ends_with(".tar.gz") || file_name.ends_with(".tgz") {
        extract_tar_gz_archive(package_path, destination)
    } else {
        Err(NativeUpdateError::State(format!(
            "unsupported portable update archive: {}",
            package_path.display()
        )))
    }
}

fn extract_zip_archive(package_path: &Path, destination: &Path) -> Result<(), NativeUpdateError> {
    let file = fs::File::open(package_path).map_err(portable_io_error("open portable archive"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        NativeUpdateError::State(format!("read portable ZIP archive failed: {error}"))
    })?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            NativeUpdateError::State(format!("read portable ZIP entry failed: {error}"))
        })?;
        let enclosed = entry.enclosed_name().ok_or_else(|| {
            NativeUpdateError::State(format!(
                "portable ZIP contains an unsafe path: {}",
                entry.name()
            ))
        })?;
        let output_path = destination.join(enclosed);
        if zip_entry_is_symlink(&entry) {
            return Err(NativeUpdateError::State(format!(
                "portable ZIP contains an unsupported link: {}",
                entry.name()
            )));
        }
        if entry.is_dir() {
            fs::create_dir_all(&output_path)
                .map_err(portable_io_error("create portable ZIP directory"))?;
            continue;
        }
        if !entry.is_file() {
            return Err(NativeUpdateError::State(format!(
                "portable ZIP contains an unsupported entry: {}",
                entry.name()
            )));
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .map_err(portable_io_error("create portable ZIP entry parent"))?;
        }
        let mut output = fs::File::create(&output_path)
            .map_err(portable_io_error("create portable ZIP file"))?;
        io::copy(&mut entry, &mut output)
            .map_err(portable_io_error("extract portable ZIP file"))?;
    }
    Ok(())
}

fn zip_entry_is_symlink(entry: &zip::read::ZipFile<'_>) -> bool {
    const UNIX_FILE_TYPE_MASK: u32 = 0o170000;
    const UNIX_SYMLINK_TYPE: u32 = 0o120000;
    entry
        .unix_mode()
        .is_some_and(|mode| mode & UNIX_FILE_TYPE_MASK == UNIX_SYMLINK_TYPE)
}

fn extract_tar_gz_archive(
    package_path: &Path,
    destination: &Path,
) -> Result<(), NativeUpdateError> {
    let file = fs::File::open(package_path).map_err(portable_io_error("open portable archive"))?;
    let mut archive = tar::Archive::new(GzDecoder::new(file));
    let entries = archive.entries().map_err(|error| {
        NativeUpdateError::State(format!("read portable tar archive failed: {error}"))
    })?;
    for entry in entries {
        let mut entry = entry.map_err(|error| {
            NativeUpdateError::State(format!("read portable tar entry failed: {error}"))
        })?;
        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() && !entry_type.is_dir() {
            return Err(NativeUpdateError::State(
                "portable tar archive contains a link or unsupported entry".to_string(),
            ));
        }
        let unpacked = entry.unpack_in(destination).map_err(|error| {
            NativeUpdateError::State(format!("extract portable tar entry failed: {error}"))
        })?;
        if !unpacked {
            return Err(NativeUpdateError::State(
                "portable tar archive contains an unsafe path".to_string(),
            ));
        }
    }
    Ok(())
}

fn single_archive_root(archive_dir: &Path) -> Result<PathBuf, NativeUpdateError> {
    let entries = fs::read_dir(archive_dir)
        .map_err(portable_io_error("read portable archive root"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(portable_io_error("read portable archive entry"))?;
    if entries.len() != 1 || !entries[0].path().is_dir() {
        return Err(NativeUpdateError::State(
            "portable update archive must contain one package directory".to_string(),
        ));
    }
    Ok(entries[0].path())
}

fn validate_manifest(
    manifest: &PortableUpdateManifest,
    package_root: &Path,
) -> Result<(), NativeUpdateError> {
    if manifest.format_version != PORTABLE_UPDATE_MANIFEST_FORMAT {
        return Err(NativeUpdateError::State(format!(
            "unsupported portable update manifest format: {}",
            manifest.format_version
        )));
    }
    let app_path = validated_relative_path(&manifest.app_executable, "app executable")?;
    let helper_path = validated_relative_path(&manifest.update_helper, "update helper")?;
    if !package_root.join(&app_path).is_file() {
        return Err(NativeUpdateError::State(
            "portable update app executable is missing".to_string(),
        ));
    }
    if !package_root.join(&helper_path).is_file() {
        return Err(NativeUpdateError::State(
            "portable update helper is missing".to_string(),
        ));
    }

    let mut managed = HashSet::new();
    for entry in &manifest.managed_entries {
        let path = validated_top_level_entry(entry)?;
        if is_reserved_user_entry(&path) {
            return Err(NativeUpdateError::State(format!(
                "portable update manifest may not manage user data: {entry}"
            )));
        }
        if !managed.insert(path.clone()) {
            return Err(NativeUpdateError::State(format!(
                "portable update manifest contains a duplicate entry: {entry}"
            )));
        }
        if !package_root.join(&path).exists() {
            return Err(NativeUpdateError::State(format!(
                "portable update managed entry is missing: {entry}"
            )));
        }
    }

    for required in [
        app_path.components().next(),
        helper_path.components().next(),
        Some(Component::Normal(OsStr::new(
            PORTABLE_UPDATE_MANIFEST_FILENAME,
        ))),
    ] {
        let Some(Component::Normal(name)) = required else {
            return Err(NativeUpdateError::State(
                "portable update manifest contains an invalid required path".to_string(),
            ));
        };
        if !managed.contains(Path::new(name)) {
            return Err(NativeUpdateError::State(format!(
                "portable update manifest does not manage required entry: {}",
                name.to_string_lossy()
            )));
        }
    }
    Ok(())
}

fn validated_relative_path(value: &str, label: &str) -> Result<PathBuf, NativeUpdateError> {
    let path = PathBuf::from(value);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(NativeUpdateError::State(format!(
            "portable update {label} path is unsafe: {value}"
        )));
    }
    Ok(path)
}

fn validated_top_level_entry(value: &str) -> Result<PathBuf, NativeUpdateError> {
    let path = validated_relative_path(value, "managed entry")?;
    if path.components().count() != 1 {
        return Err(NativeUpdateError::State(format!(
            "portable update managed entry must be top-level: {value}"
        )));
    }
    Ok(path)
}

fn is_reserved_user_entry(path: &Path) -> bool {
    matches!(
        path.to_str(),
        Some("data" | "portable.json" | PORTABLE_UPDATE_STAGING_DIR | PORTABLE_UPDATE_BACKUP_DIR)
    )
}

pub fn run_portable_update_helper(options: PortableUpdateHelperOptions) -> Result<(), String> {
    validate_helper_paths(&options)?;
    if let Some(wait_pid) = options.wait_pid {
        wait_for_process_exit(wait_pid);
    }

    apply_staged_portable_update(&options).map_err(|error| {
        format!(
            "apply staged portable update in {} failed: {error}",
            options.portable_root.display()
        )
    })?;
    if options.launch_after_apply {
        let mut command = Command::new(&options.app_exe);
        configure_background_process(&mut command);
        if let Err(launch_error) = command.spawn() {
            let rollback_result = rollback_completed_portable_update(&options);
            return Err(match rollback_result {
                Ok(()) => format!(
                    "launch updated portable app {} failed and the update was rolled back: {launch_error}",
                    options.app_exe.display()
                ),
                Err(rollback_error) => format!(
                    "launch updated portable app {} failed: {launch_error}; rollback also failed: {rollback_error}",
                    options.app_exe.display()
                ),
            });
        }
    }
    Ok(())
}

fn validate_helper_paths(options: &PortableUpdateHelperOptions) -> Result<(), String> {
    let root = fs::canonicalize(&options.portable_root)
        .map_err(|error| format!("resolve portable update root failed: {error}"))?;
    for (label, path) in [
        ("staging directory", &options.staging_dir),
        ("backup directory", &options.backup_dir),
        ("app executable", &options.app_exe),
    ] {
        let parent = if path.exists() {
            path.as_path()
        } else {
            path.parent()
                .ok_or_else(|| format!("{label} has no parent: {}", path.display()))?
        };
        let resolved_parent =
            fs::canonicalize(parent).map_err(|error| format!("resolve {label} failed: {error}"))?;
        if !resolved_parent.starts_with(&root) {
            return Err(format!(
                "{label} must stay inside the portable root: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

pub fn apply_staged_portable_update(options: &PortableUpdateHelperOptions) -> io::Result<()> {
    let manifest_path = options.staging_dir.join(PORTABLE_UPDATE_MANIFEST_FILENAME);
    let manifest_bytes = fs::read(&manifest_path)?;
    let manifest: PortableUpdateManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    validate_manifest(&manifest, &options.staging_dir)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;

    remove_reserved_update_path_io(&options.backup_dir)?;
    fs::create_dir(&options.backup_dir)?;

    let mut applied = Vec::new();
    for managed_entry in ordered_managed_entries(&manifest) {
        let mut replacement = PortableReplacement {
            source: options.staging_dir.join(&managed_entry),
            target: options.portable_root.join(&managed_entry),
            backup: options.backup_dir.join(&managed_entry),
            target_existed: false,
            installed: false,
        };
        if let Err(error) = apply_portable_replacement(&mut replacement) {
            let applied_rollback = rollback_portable_replacements(applied);
            let current_rollback = rollback_portable_replacements(vec![replacement]);
            return match (applied_rollback, current_rollback) {
                (Ok(()), Ok(())) => Err(error),
                (applied_result, current_result) => Err(io::Error::other(format!(
                    "{error}; rollback failed: applied={applied_result:?}, current={current_result:?}"
                ))),
            };
        }
        applied.push(replacement);
    }

    if let Some(staging_root) = options.staging_dir.parent() {
        remove_reserved_update_path_io(staging_root)?;
    }
    Ok(())
}

fn rollback_completed_portable_update(options: &PortableUpdateHelperOptions) -> io::Result<()> {
    let manifest_path = options
        .portable_root
        .join(PORTABLE_UPDATE_MANIFEST_FILENAME);
    let manifest_bytes = fs::read(manifest_path)?;
    let manifest: PortableUpdateManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mut first_error = None;
    for managed_entry in ordered_managed_entries(&manifest).into_iter().rev() {
        let target = options.portable_root.join(&managed_entry);
        let backup = options.backup_dir.join(&managed_entry);
        if target.exists() {
            capture_first_error(&mut first_error, remove_path(&target));
        }
        if backup.exists() {
            capture_first_error(&mut first_error, fs::rename(&backup, &target));
        }
    }
    if first_error.is_none() {
        capture_first_error(
            &mut first_error,
            remove_reserved_update_path_io(&options.backup_dir),
        );
    }
    first_error.map_or(Ok(()), Err)
}

fn ordered_managed_entries(manifest: &PortableUpdateManifest) -> Vec<PathBuf> {
    let app_top_level = Path::new(&manifest.app_executable)
        .components()
        .next()
        .and_then(|component| match component {
            Component::Normal(name) => Some(PathBuf::from(name)),
            _ => None,
        });
    let mut entries = manifest
        .managed_entries
        .iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    // Replacing the running program last leaves the old executable available
    // until every resource and helper directory has been installed.
    entries.sort_by_key(|entry| Some(entry) == app_top_level.as_ref());
    entries
}

#[derive(Debug)]
struct PortableReplacement {
    source: PathBuf,
    target: PathBuf,
    backup: PathBuf,
    target_existed: bool,
    installed: bool,
}

fn apply_portable_replacement(replacement: &mut PortableReplacement) -> io::Result<()> {
    if replacement.backup.exists() {
        remove_path(&replacement.backup)?;
    }
    if replacement.target.exists() {
        fs::rename(&replacement.target, &replacement.backup)?;
        replacement.target_existed = true;
    }
    fs::rename(&replacement.source, &replacement.target)?;
    replacement.installed = true;
    Ok(())
}

fn rollback_portable_replacements(mut replacements: Vec<PortableReplacement>) -> io::Result<()> {
    let mut first_error = None;
    while let Some(replacement) = replacements.pop() {
        if replacement.installed && replacement.target.exists() {
            capture_first_error(&mut first_error, remove_path(&replacement.target));
        }
        if replacement.target_existed && replacement.backup.exists() {
            capture_first_error(
                &mut first_error,
                fs::rename(&replacement.backup, &replacement.target),
            );
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn capture_first_error(first_error: &mut Option<io::Error>, result: io::Result<()>) {
    if first_error.is_none() {
        if let Err(error) = result {
            *first_error = Some(error);
        }
    }
}

pub fn confirm_applied_portable_update(portable_root: &Path) -> io::Result<()> {
    remove_reserved_update_path_io(&portable_root.join(PORTABLE_UPDATE_BACKUP_DIR))
}

fn remove_reserved_update_path(path: &Path) -> Result<(), NativeUpdateError> {
    remove_reserved_update_path_io(path).map_err(portable_io_error("clear portable update path"))
}

fn remove_reserved_update_path_io(path: &Path) -> io::Result<()> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.file_type().is_symlink() {
        return Err(io::Error::other(format!(
            "refusing to follow update path symlink: {}",
            path.display()
        )));
    }
    remove_path(path)
}

fn remove_path(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn portable_io_error(action: &'static str) -> impl FnOnce(io::Error) -> NativeUpdateError {
    move |error| NativeUpdateError::State(format!("{action} failed: {error}"))
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), NativeUpdateError> {
    use std::os::unix::fs::PermissionsExt as _;

    let mut permissions = fs::metadata(path)
        .map_err(portable_io_error("read update helper permissions"))?
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
        .map_err(portable_io_error("mark portable update helper executable"))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), NativeUpdateError> {
    Ok(())
}

#[cfg(windows)]
fn configure_background_process(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure_background_process(_command: &mut Command) {}

#[cfg(windows)]
fn wait_for_process_exit(pid: u32) {
    use windows::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };

    let Ok(handle) = (unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }) else {
        return;
    };
    loop {
        if unsafe { WaitForSingleObject(handle, 1000) } == WAIT_OBJECT_0 {
            break;
        }
    }
    let _ = unsafe { CloseHandle(handle) };
}

#[cfg(unix)]
fn wait_for_process_exit(pid: u32) {
    loop {
        let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
        if result != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EPERM) {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(not(any(unix, windows)))]
fn wait_for_process_exit(_pid: u32) {
    std::thread::sleep(Duration::from_millis(200));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn manifest() -> PortableUpdateManifest {
        PortableUpdateManifest {
            format_version: PORTABLE_UPDATE_MANIFEST_FORMAT,
            app_executable: "oxideterm-native".to_string(),
            update_helper: "tools/oxideterm-update-helper".to_string(),
            managed_entries: vec![
                "resources".to_string(),
                "tools".to_string(),
                "portable".to_string(),
                PORTABLE_UPDATE_MANIFEST_FILENAME.to_string(),
                "oxideterm-native".to_string(),
            ],
        }
    }

    fn write_staging(staging_dir: &Path, manifest: &PortableUpdateManifest) {
        fs::create_dir_all(staging_dir.join("resources")).unwrap();
        fs::create_dir_all(staging_dir.join("tools")).unwrap();
        fs::write(staging_dir.join("resources/config.json"), "new resources").unwrap();
        fs::write(
            staging_dir.join("tools/oxideterm-update-helper"),
            "new helper",
        )
        .unwrap();
        fs::write(staging_dir.join("portable"), "").unwrap();
        fs::write(staging_dir.join("oxideterm-native"), "new app").unwrap();
        fs::write(
            staging_dir.join(PORTABLE_UPDATE_MANIFEST_FILENAME),
            serde_json::to_vec(manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_zip_entry(archive: &mut zip::ZipWriter<fs::File>, name: &str, bytes: &[u8]) {
        archive
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(bytes).unwrap();
    }

    #[test]
    fn manifest_rejects_user_owned_data() {
        let temp = tempfile::tempdir().unwrap();
        let mut invalid = manifest();
        invalid.managed_entries.push("data".to_string());
        write_staging(temp.path(), &invalid);

        let error = validate_manifest(&invalid, temp.path()).unwrap_err();

        assert!(error.to_string().contains("may not manage user data"));
    }

    #[test]
    fn portable_replacement_preserves_user_data_and_unknown_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("data/plugins")).unwrap();
        fs::create_dir_all(root.join("data/skills/server-audit")).unwrap();
        fs::create_dir_all(root.join("custom-data")).unwrap();
        fs::create_dir_all(root.join("resources")).unwrap();
        fs::create_dir_all(root.join("tools")).unwrap();
        fs::write(root.join("data/keystore.vault"), "secret").unwrap();
        fs::write(root.join("data/plugins/example.wasm"), "plugin").unwrap();
        fs::write(root.join("data/skills/server-audit/SKILL.md"), "user skill").unwrap();
        fs::write(root.join("custom-data/settings.json"), "settings").unwrap();
        fs::write(root.join("portable.json"), r#"{"dataDir":"custom-data"}"#).unwrap();
        fs::write(root.join("user-note.txt"), "keep me").unwrap();
        fs::write(root.join("resources/config.json"), "old resources").unwrap();
        fs::write(root.join("tools/oxideterm-update-helper"), "old helper").unwrap();
        fs::write(root.join("portable"), "").unwrap();
        fs::write(root.join("oxideterm-native"), "old app").unwrap();

        let staging_root = root.join(PORTABLE_UPDATE_STAGING_DIR);
        let staging_dir = staging_root.join("payload");
        let update_manifest = manifest();
        write_staging(&staging_dir, &update_manifest);
        let options = PortableUpdateHelperOptions {
            portable_root: root.to_path_buf(),
            staging_dir,
            backup_dir: root.join(PORTABLE_UPDATE_BACKUP_DIR),
            app_exe: root.join("oxideterm-native"),
            wait_pid: None,
            launch_after_apply: false,
        };

        apply_staged_portable_update(&options).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("oxideterm-native")).unwrap(),
            "new app"
        );
        assert_eq!(
            fs::read_to_string(root.join("resources/config.json")).unwrap(),
            "new resources"
        );
        assert_eq!(
            fs::read_to_string(root.join("data/keystore.vault")).unwrap(),
            "secret"
        );
        assert_eq!(
            fs::read_to_string(root.join("data/plugins/example.wasm")).unwrap(),
            "plugin"
        );
        assert_eq!(
            fs::read_to_string(root.join("data/skills/server-audit/SKILL.md")).unwrap(),
            "user skill"
        );
        assert_eq!(
            fs::read_to_string(root.join("custom-data/settings.json")).unwrap(),
            "settings"
        );
        assert!(root.join("portable.json").exists());
        assert_eq!(
            fs::read_to_string(root.join("user-note.txt")).unwrap(),
            "keep me"
        );
        assert!(root.join(PORTABLE_UPDATE_BACKUP_DIR).exists());
        assert!(!root.join(PORTABLE_UPDATE_STAGING_DIR).exists());
    }

    #[test]
    fn confirmed_portable_update_removes_retained_backup() {
        let temp = tempfile::tempdir().unwrap();
        let backup = temp.path().join(PORTABLE_UPDATE_BACKUP_DIR);
        fs::create_dir(&backup).unwrap();
        fs::write(backup.join("oxideterm-native"), "old app").unwrap();

        confirm_applied_portable_update(temp.path()).unwrap();
        confirm_applied_portable_update(temp.path()).unwrap();

        assert!(!backup.exists());
    }

    #[test]
    fn completed_portable_update_can_roll_back_before_restart() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("resources")).unwrap();
        fs::create_dir_all(root.join("tools")).unwrap();
        fs::write(root.join("resources/config.json"), "old resources").unwrap();
        fs::write(root.join("tools/oxideterm-update-helper"), "old helper").unwrap();
        fs::write(root.join("portable"), "").unwrap();
        fs::write(root.join("oxideterm-native"), "old app").unwrap();

        let staging_root = root.join(PORTABLE_UPDATE_STAGING_DIR);
        let staging_dir = staging_root.join("payload");
        write_staging(&staging_dir, &manifest());
        let options = PortableUpdateHelperOptions {
            portable_root: root.to_path_buf(),
            staging_dir,
            backup_dir: root.join(PORTABLE_UPDATE_BACKUP_DIR),
            app_exe: root.join("oxideterm-native"),
            wait_pid: None,
            launch_after_apply: false,
        };
        apply_staged_portable_update(&options).unwrap();

        rollback_completed_portable_update(&options).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("oxideterm-native")).unwrap(),
            "old app"
        );
        assert_eq!(
            fs::read_to_string(root.join("resources/config.json")).unwrap(),
            "old resources"
        );
        assert!(!root.join(PORTABLE_UPDATE_BACKUP_DIR).exists());
        assert!(!root.join(PORTABLE_UPDATE_MANIFEST_FILENAME).exists());
    }

    #[test]
    fn portable_replacement_rolls_back_applied_entries_after_failure() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let backup_dir = root.join(PORTABLE_UPDATE_BACKUP_DIR);
        let staging_dir = root.join(PORTABLE_UPDATE_STAGING_DIR);
        fs::create_dir(&backup_dir).unwrap();
        fs::create_dir(&staging_dir).unwrap();
        let target = root.join("oxideterm-native");
        let source = staging_dir.join("oxideterm-native");
        fs::write(&target, "old app").unwrap();
        fs::write(&source, "new app").unwrap();

        let mut applied = PortableReplacement {
            source,
            target: target.clone(),
            backup: backup_dir.join("oxideterm-native"),
            target_existed: false,
            installed: false,
        };
        apply_portable_replacement(&mut applied).unwrap();
        let mut failing = PortableReplacement {
            source: staging_dir.join("missing"),
            target: root.join("resources"),
            backup: backup_dir.join("resources"),
            target_existed: false,
            installed: false,
        };

        let error = apply_portable_replacement(&mut failing).unwrap_err();
        rollback_portable_replacements(vec![applied]).unwrap();

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(fs::read_to_string(target).unwrap(), "old app");
    }

    #[test]
    fn unsafe_relative_paths_are_rejected() {
        assert!(validated_relative_path("../data", "test").is_err());
        assert!(validated_relative_path("/tmp/app", "test").is_err());
        assert!(validated_top_level_entry("resources/agents").is_err());
    }

    #[test]
    fn staging_uses_only_manifest_managed_entries() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("portable-root");
        fs::create_dir(&root).unwrap();
        let package_path = temp.path().join("OxideTerm_portable.zip");
        let file = fs::File::create(&package_path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let package_root = "OxideTerm_portable";
        let update_manifest = manifest();
        write_zip_entry(
            &mut archive,
            &format!("{package_root}/{}", PORTABLE_UPDATE_MANIFEST_FILENAME),
            &serde_json::to_vec(&update_manifest).unwrap(),
        );
        write_zip_entry(
            &mut archive,
            &format!("{package_root}/oxideterm-native"),
            b"new app",
        );
        write_zip_entry(
            &mut archive,
            &format!("{package_root}/resources/config.json"),
            b"new resources",
        );
        write_zip_entry(
            &mut archive,
            &format!("{package_root}/tools/oxideterm-update-helper"),
            b"new helper",
        );
        write_zip_entry(&mut archive, &format!("{package_root}/portable"), b"");
        write_zip_entry(
            &mut archive,
            &format!("{package_root}/data/plugins/example.wasm"),
            b"user data placeholder",
        );
        write_zip_entry(
            &mut archive,
            &format!("{package_root}/data/skills/example/SKILL.md"),
            b"packaged skill placeholder",
        );
        archive.finish().unwrap();

        let staged = stage_portable_update(&package_path, &root, 42).unwrap();

        assert!(staged.payload_dir.join("oxideterm-native").is_file());
        assert!(staged.payload_dir.join("resources/config.json").is_file());
        assert!(!staged.payload_dir.join("data").exists());
        assert!(staged.detached_helper.is_file());
    }

    #[test]
    fn zip_extraction_rejects_parent_traversal() {
        let temp = tempfile::tempdir().unwrap();
        let package_path = temp.path().join("unsafe.zip");
        let file = fs::File::create(&package_path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        write_zip_entry(&mut archive, "../escaped", b"unsafe");
        archive.finish().unwrap();
        let destination = temp.path().join("extract");
        fs::create_dir(&destination).unwrap();

        let error = extract_zip_archive(&package_path, &destination).unwrap_err();

        assert!(error.to_string().contains("unsafe path"));
        assert!(!temp.path().join("escaped").exists());
    }
}
