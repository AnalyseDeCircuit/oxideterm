use std::path::{Path, PathBuf};

pub fn home_path() -> String {
    std::env::var("HOME").unwrap_or_else(|_| {
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

pub fn normalize_local_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed == "~" {
        return home_path();
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return Path::new(&home_path())
            .join(rest)
            .to_string_lossy()
            .to_string();
    }
    if trimmed.is_empty() {
        home_path()
    } else {
        trimmed.to_string()
    }
}

pub fn local_parent_path(path: &str) -> Option<String> {
    let path = Path::new(path);
    path.parent()
        .map(|parent| parent.to_string_lossy().to_string())
        .filter(|parent| !parent.is_empty())
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
    fn validate_local_name_rejects_traversal_and_separators() {
        assert!(validate_local_name("notes.txt").is_ok());
        assert!(validate_local_name("../notes.txt").is_err());
        assert!(validate_local_name("folder/notes.txt").is_err());
        assert!(validate_local_name("..").is_err());
    }
}
