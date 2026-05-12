use std::path::Path;

pub fn copy_recursively(source: &Path, target: &Path) -> std::io::Result<()> {
    copy_recursively_with_progress(source, target, &mut |_| {})
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

pub fn copy_recursively_with_progress(
    source: &Path,
    target: &Path,
    progress: &mut impl FnMut(&Path),
) -> std::io::Result<()> {
    let metadata = std::fs::symlink_metadata(source)?;
    if metadata.is_dir() {
        std::fs::create_dir_all(target)?;
        progress(source);
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_recursively_with_progress(
                &entry.path(),
                &target.join(entry.file_name()),
                progress,
            )?;
        }
    } else {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, target)?;
        progress(source);
    }
    Ok(())
}
