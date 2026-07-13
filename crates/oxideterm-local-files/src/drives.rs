use std::path::Path;

#[cfg(unix)]
use std::collections::HashMap;
#[cfg(not(unix))]
use std::{collections::HashSet, path::PathBuf};

use crate::LocalDrive;

pub fn local_drives() -> Vec<LocalDrive> {
    let mut drives = platform_local_drives();
    drives.sort_by(|left, right| {
        let left_system = if left.drive_type == "system" { 0 } else { 1 };
        let right_system = if right.drive_type == "system" { 0 } else { 1 };
        left_system
            .cmp(&right_system)
            .then_with(|| left.path.cmp(&right.path))
    });
    if drives.is_empty() {
        drives.push(LocalDrive {
            name: "System".to_string(),
            path: home_path_root(),
            drive_type: "system".to_string(),
            total_space: 0,
            available_space: 0,
            read_only: false,
        });
    }
    drives
}

fn home_path_root() -> String {
    #[cfg(windows)]
    {
        "C:\\".to_string()
    }
    #[cfg(not(windows))]
    {
        "/".to_string()
    }
}

fn platform_local_drives() -> Vec<LocalDrive> {
    use sysinfo::Disks;

    let disks = Disks::new_with_refreshed_list();
    let mut drives: Vec<LocalDrive> = Vec::new();

    #[cfg(unix)]
    let mut seen_dev_ids: HashMap<u64, usize> = HashMap::new();
    #[cfg(not(unix))]
    let mut seen_mount_points: HashSet<PathBuf> = HashSet::new();

    for disk in disks.list() {
        let mount_point = disk.mount_point().to_path_buf();

        #[cfg(unix)]
        let unix_dev_id = {
            use std::os::unix::fs::MetadataExt;
            match std::fs::metadata(&mount_point) {
                Ok(metadata) => {
                    let dev = metadata.dev();
                    if let Some(&existing_idx) = seen_dev_ids.get(&dev) {
                        if mount_point.as_os_str().len() < drives[existing_idx].path.len() {
                            drives[existing_idx].path = mount_point.to_string_lossy().to_string();
                            drives[existing_idx].name = drive_display_name(disk, &mount_point);
                        }
                        continue;
                    }
                    Some(dev)
                }
                Err(_) => None,
            }
        };
        #[cfg(not(unix))]
        {
            let canonical = mount_point
                .canonicalize()
                .unwrap_or_else(|_| mount_point.clone());
            if !seen_mount_points.insert(canonical) {
                continue;
            }
        }

        let mount = mount_point.to_string_lossy();
        if is_pseudo_mount(&mount) {
            continue;
        }

        #[cfg(unix)]
        if let Some(dev) = unix_dev_id {
            seen_dev_ids.insert(dev, drives.len());
        }

        let read_only = if cfg!(target_os = "macos") && mount == "/" {
            !std::fs::metadata("/Users")
                .map(|metadata| !metadata.permissions().readonly())
                .unwrap_or(false)
        } else {
            disk.is_read_only()
        };

        drives.push(LocalDrive {
            name: drive_display_name(disk, &mount_point),
            path: mount.to_string(),
            drive_type: classify_disk(disk).to_string(),
            total_space: disk.total_space(),
            available_space: disk.available_space(),
            read_only,
        });
    }
    drives
}

fn is_pseudo_mount(mount: &str) -> bool {
    mount.starts_with("/proc")
        || mount.starts_with("/sys")
        || mount.starts_with("/dev")
        || mount.starts_with("/snap")
        || mount == "/boot"
        || mount == "/boot/efi"
        || is_blocked_run_mount(mount)
}

fn is_blocked_run_mount(mount: &str) -> bool {
    if !mount.starts_with("/run") {
        return false;
    }
    if mount.starts_with("/run/media/") || mount.starts_with("/run/mount/") {
        return false;
    }
    mount.starts_with("/run/user/") && !mount.contains("/gvfs/")
        || (!mount.starts_with("/run/user/"))
}

fn drive_display_name(disk: &sysinfo::Disk, mount_point: &Path) -> String {
    let raw_name = disk.name().to_string_lossy().to_string();
    if !raw_name.is_empty() {
        return raw_name;
    }
    let mount = mount_point.to_string_lossy();
    mount_point
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            if mount == "/" {
                "System".to_string()
            } else {
                mount.to_string()
            }
        })
}

fn classify_disk(disk: &sysinfo::Disk) -> &'static str {
    use sysinfo::DiskKind;

    let mount = disk.mount_point().to_string_lossy();
    #[cfg(not(windows))]
    if mount == "/" {
        return "system";
    }
    #[cfg(windows)]
    if mount.to_uppercase().starts_with("C:") {
        return "system";
    }
    if mount.contains("://") || mount.starts_with("//") {
        return "network";
    }
    match disk.kind() {
        DiskKind::HDD | DiskKind::SSD => "local",
        _ => "removable",
    }
}

pub fn directory_stats(path: &Path) -> (u64, u64, u64) {
    let mut files = 0;
    let mut dirs = 0;
    let mut size = 0;
    let Ok(entries) = std::fs::read_dir(path) else {
        return (files, dirs, size);
    };
    for entry in entries.flatten() {
        let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if metadata.is_dir() {
            dirs += 1;
            let nested = directory_stats(&entry.path());
            files += nested.0;
            dirs += nested.1;
            size += nested.2;
        } else {
            files += 1;
            size += metadata.len();
        }
    }
    (files, dirs, size)
}
