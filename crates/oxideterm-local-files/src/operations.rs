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

use super::{join_local_path, local_file_audit_context, local_file_operation, validate_local_name};

pub fn create_local_folder(current_path: &str, name: &str) -> Result<(), String> {
    let audit = local_file_operation("file_mkdir", &join_local_path(current_path, name));
    let result = validate_local_name(name)
        .map(|_| join_local_path(current_path, name))
        .and_then(|path| std::fs::create_dir(&path).map_err(|error| error.to_string()));
    audit.result(&result);
    result
}

pub fn create_local_file(current_path: &str, name: &str) -> Result<(), String> {
    let audit = local_file_operation(
        "file_save",
        &format!(
            "path={}; mode=create_new",
            join_local_path(current_path, name)
        ),
    );
    let result = validate_local_name(name)
        .map(|_| join_local_path(current_path, name))
        .and_then(|path| {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map(|_| ())
                .map_err(|error| error.to_string())
        });
    audit.finish(
        if result.is_ok() {
            oxideterm_audit::AuditOutcome::Succeeded
        } else {
            oxideterm_audit::AuditOutcome::Failed
        },
        oxideterm_audit::AuditEvidence::Protocol,
        None,
        result.is_ok().then_some(0),
    );
    result
}

pub fn rename_local_entry(
    current_path: &str,
    old_name: &str,
    new_name: &str,
) -> Result<(), String> {
    let old_path = join_local_path(current_path, old_name);
    let new_path = join_local_path(current_path, new_name);
    let audit = local_file_operation("file_rename", &format!("{old_path} → {new_path}"));
    let result = validate_local_name(new_name)
        .and_then(|_| std::fs::rename(&old_path, &new_path).map_err(|error| error.to_string()));
    audit.result(&result);
    result
}

pub fn delete_local_entries(current_path: &str, names: &[String]) -> Option<String> {
    let context = local_file_audit_context(oxideterm_audit::AuditSource::User);
    let mut audit = oxideterm_audit::AuditOperation::in_context(
        context.as_ref(),
        oxideterm_audit::AuditCategory::File,
        "file_delete_recursive",
        Some(&format!("path={current_path}; requested={}", names.len())),
    );
    let child_context = context.map(|mut context| {
        context.parent_id = audit.id().map(str::to_owned);
        context
    });
    let mut error = None;
    let mut removed = 0usize;
    let mut failed_path = None;
    for name in names {
        let path = join_local_path(current_path, name);
        let path_ref = std::path::Path::new(&path);
        let is_directory = path_ref.is_dir();
        let child = oxideterm_audit::AuditOperation::in_context(
            child_context.as_ref(),
            oxideterm_audit::AuditCategory::File,
            if is_directory {
                "file_delete_recursive"
            } else {
                "file_delete"
            },
            Some(&path),
        );
        let result = if is_directory {
            std::fs::remove_dir_all(path_ref)
        } else {
            std::fs::remove_file(path_ref)
        };
        child.result(&result);
        if let Err(err) = result {
            error = Some(err.to_string());
            failed_path = Some(path);
            break;
        }
        removed += 1;
    }
    audit.summary(&format!("path={current_path}; requested={}; removed={removed}; unprocessed={}; failed_path={}; nested_items=unavailable:remove_dir_all", names.len(), names.len().saturating_sub(removed + usize::from(error.is_some())), failed_path.as_deref().unwrap_or("none")));
    audit.finish(
        if error.is_none() {
            oxideterm_audit::AuditOutcome::Succeeded
        } else if removed > 0 {
            oxideterm_audit::AuditOutcome::Partial
        } else {
            oxideterm_audit::AuditOutcome::Failed
        },
        oxideterm_audit::AuditEvidence::Protocol,
        None,
        None,
    );
    error
}
