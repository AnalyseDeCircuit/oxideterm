use std::path::Path;

pub fn copy_recursively(source: &Path, target: &Path) -> std::io::Result<()> {
    copy_recursively_with_progress(source, target, &mut |_, _| {})
}

pub fn local_operation_unit_count(path: &Path) -> usize {
    if !path.is_dir() {
        return 1;
    }
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.path() != path)
        .count()
        .saturating_add(1)
}

/// Emits one result per object: copied bytes for success, or `None` for the failed path.
pub fn copy_recursively_with_progress(
    source: &Path,
    target: &Path,
    progress: &mut impl FnMut(&Path, Option<u64>),
) -> std::io::Result<()> {
    let metadata = std::fs::symlink_metadata(source).inspect_err(|_| progress(source, None))?;
    if metadata.is_dir() {
        std::fs::create_dir_all(target).inspect_err(|_| progress(source, None))?;
        let entries = std::fs::read_dir(source).inspect_err(|_| progress(source, None))?;
        for entry in entries {
            let entry = entry.inspect_err(|_| progress(source, None))?;
            copy_recursively_with_progress(
                &entry.path(),
                &target.join(entry.file_name()),
                progress,
            )?;
        }
        progress(source, Some(0));
    } else {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).inspect_err(|_| progress(source, None))?;
        }
        let copied = std::fs::copy(source, target).inspect_err(|_| progress(source, None))?;
        progress(source, Some(copied));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recursive_copy_reports_completed_file_bytes() {
        let root = std::env::temp_dir().join(format!(
            "oxideterm-copy-audit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        let target = root.join("target");
        std::fs::create_dir_all(source.join("nested")).unwrap();
        std::fs::write(source.join("nested/data.txt"), b"example").unwrap();

        let mut completed = Vec::new();
        copy_recursively_with_progress(&source, &target, &mut |path, bytes| {
            completed.push((path.strip_prefix(&source).unwrap().to_path_buf(), bytes));
        })
        .unwrap();

        assert_eq!(
            std::fs::read(target.join("nested/data.txt")).unwrap(),
            b"example"
        );
        assert!(completed.contains(&(Path::new("nested/data.txt").to_path_buf(), Some(7))));
        assert!(completed.contains(&(Path::new("nested").to_path_buf(), Some(0))));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recursive_copy_reports_the_failed_object() {
        let missing = std::env::temp_dir().join(format!(
            "oxideterm-copy-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut events = Vec::new();
        let result = copy_recursively_with_progress(
            &missing,
            &missing.with_extension("copy"),
            &mut |path, bytes| {
                events.push((path.to_path_buf(), bytes));
            },
        );
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
        assert_eq!(events, vec![(missing, None)]);
    }
}
