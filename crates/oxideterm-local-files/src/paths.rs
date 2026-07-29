use std::path::{Path, PathBuf};

use crate::{LocalPathSegment, LocalSidebarLocation, LocalSidebarLocationKind};

#[cfg(target_os = "macos")]
const MACOS_APPLICATIONS_PATH: &str = "/Applications";

pub fn home_path() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| {
            #[cfg(windows)]
            {
                "C:\\".to_string()
            }
            #[cfg(not(windows))]
            {
                "/".to_string()
            }
        })
}

pub fn local_sidebar_locations() -> Vec<LocalSidebarLocation> {
    let candidates = [
        (
            LocalSidebarLocationKind::Home,
            dirs::home_dir().or_else(|| Some(PathBuf::from(home_path()))),
        ),
        (
            LocalSidebarLocationKind::Applications,
            platform_applications_path(),
        ),
        (LocalSidebarLocationKind::Desktop, dirs::desktop_dir()),
        (LocalSidebarLocationKind::Documents, dirs::document_dir()),
        (LocalSidebarLocationKind::Downloads, dirs::download_dir()),
    ];
    sidebar_locations_from_candidates(candidates, Path::is_dir)
}

fn sidebar_locations_from_candidates<const N: usize>(
    candidates: [(LocalSidebarLocationKind, Option<PathBuf>); N],
    is_directory: impl Fn(&Path) -> bool,
) -> Vec<LocalSidebarLocation> {
    // Only expose real directories so every sidebar row is immediately navigable.
    candidates
        .into_iter()
        .filter_map(|(kind, path)| path.map(|path| (kind, path)))
        .filter(|(_, path)| is_directory(path))
        .map(|(kind, path)| LocalSidebarLocation {
            kind,
            path: path.to_string_lossy().to_string(),
        })
        .collect()
}

fn platform_applications_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Some(PathBuf::from(MACOS_APPLICATIONS_PATH))
    }
    #[cfg(windows)]
    {
        std::env::var_os("ProgramFiles").map(PathBuf::from)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        None
    }
}

pub fn normalize_local_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed == "~" || trimmed == "$HOME" {
        return home_path();
    }
    for prefix in ["~/", "~\\", "$HOME/", "$HOME\\"] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Path::new(&home_path())
                .join(rest)
                .to_string_lossy()
                .to_string();
        }
    }
    if trimmed.is_empty() {
        home_path()
    } else {
        trimmed.to_string()
    }
}

pub fn local_parent_path(path: &str) -> Option<String> {
    let normalized = normalize_local_path(path);
    if let Some(segments) = windows_path_segments(&normalized) {
        // Drive roots have no filesystem parent; their caller may open the drive picker.
        return segments
            .get(segments.len().checked_sub(2)?)
            .map(|segment| segment.full_path.clone());
    }
    let path = Path::new(&normalized);
    path.parent()
        .map(|parent| parent.to_string_lossy().to_string())
        .filter(|parent| !parent.is_empty())
}

pub fn local_path_segments(path: &str) -> Vec<LocalPathSegment> {
    let normalized = normalize_local_path(path);
    if let Some(segments) = windows_path_segments(&normalized) {
        return segments;
    }

    let unix_path = normalized.replace('\\', "/");
    let mut segments = vec![LocalPathSegment {
        name: "/".to_string(),
        full_path: "/".to_string(),
        root_is_drive: false,
    }];
    let mut current = String::from("/");
    for part in unix_path
        .trim_start_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
    {
        current = if current == "/" {
            format!("/{part}")
        } else {
            format!("{current}/{part}")
        };
        segments.push(LocalPathSegment {
            name: part.to_string(),
            full_path: current.clone(),
            root_is_drive: false,
        });
    }
    segments
}

fn windows_path_segments(path: &str) -> Option<Vec<LocalPathSegment>> {
    let normalized = path.replace('/', "\\");
    let bytes = normalized.as_bytes();
    if bytes.len() < 2
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || (bytes.len() > 2 && bytes[2] != b'\\')
    {
        return None;
    }

    // Keep the drive prefix attached to every clickable path instead of producing `/C:`.
    let drive = normalized[..2].to_string();
    let mut current = format!("{drive}\\");
    let mut segments = vec![LocalPathSegment {
        name: drive,
        full_path: current.clone(),
        root_is_drive: true,
    }];
    for part in normalized[2..]
        .trim_start_matches('\\')
        .split('\\')
        .filter(|part| !part.is_empty())
    {
        if !current.ends_with('\\') {
            current.push('\\');
        }
        current.push_str(part);
        segments.push(LocalPathSegment {
            name: part.to_string(),
            full_path: current.clone(),
            root_is_drive: true,
        });
    }
    Some(segments)
}

pub fn join_local_path(base: &str, name: &str) -> String {
    Path::new(base).join(name).to_string_lossy().to_string()
}

pub fn validate_local_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("name is empty".to_string());
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed == "." || trimmed == ".." {
        return Err("invalid name".to_string());
    }
    if trimmed.contains("..") {
        return Err("invalid name".to_string());
    }
    Ok(())
}

pub fn unique_copy_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "copy".to_string());
    let ext = path
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
        .unwrap_or_default();
    for index in 1..=100 {
        let candidate = parent.join(format!("{stem} ({index}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{stem} (copy){ext}"))
}

pub fn would_move_directory_into_itself(source: &Path, target: &Path) -> bool {
    let Ok(source) = source.canonicalize() else {
        return false;
    };
    let target = target
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .unwrap_or_else(|| target.to_path_buf());
    target.starts_with(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_locations_keep_only_available_directories_in_semantic_order() {
        let existing = [
            PathBuf::from("/home/alice"),
            PathBuf::from("/Applications"),
            PathBuf::from("/home/alice/Documents"),
            PathBuf::from("/home/alice/Downloads"),
        ];

        let locations = sidebar_locations_from_candidates(
            [
                (
                    LocalSidebarLocationKind::Home,
                    Some(PathBuf::from("/home/alice")),
                ),
                (
                    LocalSidebarLocationKind::Applications,
                    Some(PathBuf::from("/Applications")),
                ),
                (
                    LocalSidebarLocationKind::Desktop,
                    Some(PathBuf::from("/home/alice/Desktop")),
                ),
                (
                    LocalSidebarLocationKind::Documents,
                    Some(PathBuf::from("/home/alice/Documents")),
                ),
                (
                    LocalSidebarLocationKind::Downloads,
                    Some(PathBuf::from("/home/alice/Downloads")),
                ),
            ],
            |path| existing.iter().any(|candidate| candidate == path),
        );

        assert_eq!(
            locations
                .iter()
                .map(|location| location.kind)
                .collect::<Vec<_>>(),
            vec![
                LocalSidebarLocationKind::Home,
                LocalSidebarLocationKind::Applications,
                LocalSidebarLocationKind::Documents,
                LocalSidebarLocationKind::Downloads,
            ]
        );
    }

    #[test]
    fn sidebar_locations_preserve_redirected_system_paths() {
        let redirected_desktop = PathBuf::from(r"C:\Users\alice\OneDrive\Desktop");

        let locations = sidebar_locations_from_candidates(
            [(
                LocalSidebarLocationKind::Desktop,
                Some(redirected_desktop.clone()),
            )],
            |_| true,
        );

        assert_eq!(locations[0].path, redirected_desktop.to_string_lossy());
    }

    #[test]
    fn windows_drive_segments_remain_absolute_on_every_platform() {
        let segments = local_path_segments(r"D:\Projects\OxideTerm");

        assert_eq!(
            segments
                .iter()
                .map(|segment| segment.full_path.as_str())
                .collect::<Vec<_>>(),
            [r"D:\", r"D:\Projects", r"D:\Projects\OxideTerm"]
        );
        assert!(segments.iter().all(|segment| segment.root_is_drive));
    }

    #[test]
    fn windows_drive_parent_never_becomes_a_unix_style_path() {
        assert_eq!(
            local_parent_path(r"D:\Projects\OxideTerm").as_deref(),
            Some(r"D:\Projects")
        );
        assert_eq!(local_parent_path(r"D:\Projects").as_deref(), Some(r"D:\"));
        assert_eq!(local_parent_path(r"D:\"), None);
    }

    #[test]
    fn validate_local_name_rejects_traversal_and_separators() {
        assert!(validate_local_name("notes.txt").is_ok());
        assert!(validate_local_name("../notes.txt").is_err());
        assert!(validate_local_name("folder/notes.txt").is_err());
        assert!(validate_local_name("..").is_err());
    }
}
