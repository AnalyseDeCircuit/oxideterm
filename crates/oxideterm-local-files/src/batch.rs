use super::{
    ArchiveEntryOutcome, LocalBatchProgress, LocalClipboardMode, compress_local_files,
    copy_recursively_with_progress, extract_local_archive, local_file_audit_context,
    unique_copy_path, would_move_directory_into_itself,
};
use oxideterm_audit::{AuditContext, AuditSource};
use std::path::Path;

pub fn paste_local_files(
    sources: Vec<String>,
    destination: String,
    mode: LocalClipboardMode,
    total: usize,
    audit_context: Option<AuditContext>,
    mut notify: impl FnMut(LocalBatchProgress),
) -> Result<(), String> {
    let mut audit = oxideterm_audit::AuditOperation::in_context(
        audit_context.as_ref(),
        oxideterm_audit::AuditCategory::File,
        if mode == LocalClipboardMode::Cut {
            "file_move"
        } else {
            "file_copy"
        },
        Some(&format!(
            "destination={destination}; sources={}",
            sources.len()
        )),
    );
    let child_context = audit_context.map(|mut context| {
        context.parent_id = audit.id().map(str::to_owned);
        context
    });
    let done = std::cell::Cell::new(0usize);
    let bytes = std::cell::Cell::new(0u64);
    let copied = std::cell::Cell::new(false);
    let mut attempted_source = None;
    let mut failed_object = None;
    let result: Result<(), String> = (|| {
        for source in &sources {
            attempted_source = Some(source.as_str());
            let source_path = std::path::Path::new(source);
            let Some(name) = source_path.file_name() else {
                return Err("Source has no file name".to_string());
            };
            let target = unique_copy_path(&std::path::Path::new(&destination).join(name));
            if mode == LocalClipboardMode::Cut
                && would_move_directory_into_itself(source_path, &target)
            {
                return Err("cannot move a folder into itself".to_string());
            }
            let mut progress = |path: &std::path::Path, copied_bytes: Option<u64>| {
                if let Some(copied_bytes) = copied_bytes {
                    done.set(done.get() + 1);
                    bytes.set(bytes.get().saturating_add(copied_bytes));
                    copied.set(true);
                } else {
                    failed_object = Some(path.to_string_lossy().into_owned());
                }
                if let Some(context) = child_context.as_ref() {
                    let relative = path
                        .strip_prefix(source_path)
                        .unwrap_or_else(|_| std::path::Path::new(""));
                    let copied_to = if relative.as_os_str().is_empty() {
                        target.clone()
                    } else {
                        target.join(relative)
                    };
                    context
                        .operation(
                            oxideterm_audit::AuditCategory::File,
                            "file_copy",
                            Some(&format!("{} → {}", path.display(), copied_to.display())),
                        )
                        .finish(
                            if copied_bytes.is_some() {
                                oxideterm_audit::AuditOutcome::Succeeded
                            } else {
                                oxideterm_audit::AuditOutcome::Failed
                            },
                            oxideterm_audit::AuditEvidence::Protocol,
                            None,
                            copied_bytes,
                        );
                }
                if copied_bytes.is_none() {
                    return;
                }
                let file_name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                notify(LocalBatchProgress {
                    current: done.get(),
                    file_name,
                });
            };
            if mode == LocalClipboardMode::Cut {
                match std::fs::rename(source_path, &target) {
                    Ok(()) => {
                        done.set(done.get() + 1);
                        if let Some(context) = child_context.as_ref() {
                            context
                                .operation(
                                    oxideterm_audit::AuditCategory::File,
                                    "file_move",
                                    Some(&format!(
                                        "{} → {}",
                                        source_path.display(),
                                        target.display()
                                    )),
                                )
                                .finish(
                                    oxideterm_audit::AuditOutcome::Succeeded,
                                    oxideterm_audit::AuditEvidence::Protocol,
                                    None,
                                    None,
                                );
                        }
                        notify(LocalBatchProgress {
                            current: done.get(),
                            file_name: name.to_string_lossy().into_owned(),
                        });
                        Ok(())
                    }
                    Err(_) => {
                        copy_recursively_with_progress(source_path, &target, &mut progress)
                            .map_err(|error| error.to_string())?;
                        let removal = if source_path.is_dir() {
                            std::fs::remove_dir_all(source_path)
                        } else {
                            std::fs::remove_file(source_path)
                        };
                        if removal.is_ok() {
                            if let Some(context) = child_context.as_ref() {
                                context
                                    .operation(
                                        oxideterm_audit::AuditCategory::File,
                                        "file_move",
                                        Some(&format!(
                                            "{} → {}",
                                            source_path.display(),
                                            target.display()
                                        )),
                                    )
                                    .finish(
                                        oxideterm_audit::AuditOutcome::Succeeded,
                                        oxideterm_audit::AuditEvidence::Protocol,
                                        None,
                                        None,
                                    );
                            }
                        } else if let Some(context) = child_context.as_ref() {
                            context
                                .operation(
                                    oxideterm_audit::AuditCategory::File,
                                    "file_move",
                                    Some(&format!(
                                        "{} → {}",
                                        source_path.display(),
                                        target.display()
                                    )),
                                )
                                .finish(
                                    oxideterm_audit::AuditOutcome::Partial,
                                    oxideterm_audit::AuditEvidence::Protocol,
                                    None,
                                    None,
                                );
                        }
                        removal.map_err(|error| error.to_string())
                    }
                }
            } else {
                copy_recursively_with_progress(source_path, &target, &mut progress)
                    .map_err(|error| error.to_string())
            }?;
        }
        Ok(())
    })();
    audit.summary(&format!("destination={destination}; sources={}; completed_objects={}; estimated_objects={total}; failed_object={}; copied_bytes={}", sources.len(), done.get(), if result.is_err() { failed_object.as_deref().or(attempted_source).unwrap_or("unknown") } else { "none" }, if copied.get() { bytes.get().to_string() } else { "unknown".into() }));
    audit.finish(
        if result.is_ok() {
            oxideterm_audit::AuditOutcome::Succeeded
        } else if done.get() > 0 {
            oxideterm_audit::AuditOutcome::Partial
        } else {
            oxideterm_audit::AuditOutcome::Failed
        },
        oxideterm_audit::AuditEvidence::Protocol,
        None,
        copied.get().then_some(bytes.get()),
    );
    result
}

pub fn duplicate_local_files(
    paths: Vec<String>,
    total: usize,
    audit_context: Option<AuditContext>,
    mut notify: impl FnMut(LocalBatchProgress),
) -> Result<(), String> {
    let mut done = 0usize;
    let mut bytes = 0u64;
    let mut audit = oxideterm_audit::AuditOperation::in_context(
        audit_context.as_ref(),
        oxideterm_audit::AuditCategory::File,
        "file_copy",
        Some(&format!("sources={}; mode=duplicate", paths.len())),
    );
    let child_context = audit_context.map(|mut context| {
        context.parent_id = audit.id().map(str::to_owned);
        context
    });
    let mut attempted_source = None;
    let mut failed_object = None;
    let result: Result<(), String> = (|| {
        for path in &paths {
            attempted_source = Some(path.as_str());
            let source = std::path::Path::new(&path);
            let target = unique_copy_path(source);
            let mut progress = |path: &std::path::Path, copied_bytes: Option<u64>| {
                if let Some(copied_bytes) = copied_bytes {
                    done += 1;
                    bytes = bytes.saturating_add(copied_bytes);
                } else {
                    failed_object = Some(path.to_string_lossy().into_owned());
                }
                if let Some(context) = child_context.as_ref() {
                    let relative = path
                        .strip_prefix(source)
                        .unwrap_or_else(|_| std::path::Path::new(""));
                    let copied_to = if relative.as_os_str().is_empty() {
                        target.clone()
                    } else {
                        target.join(relative)
                    };
                    context
                        .operation(
                            oxideterm_audit::AuditCategory::File,
                            "file_copy",
                            Some(&format!("{} → {}", path.display(), copied_to.display())),
                        )
                        .finish(
                            if copied_bytes.is_some() {
                                oxideterm_audit::AuditOutcome::Succeeded
                            } else {
                                oxideterm_audit::AuditOutcome::Failed
                            },
                            oxideterm_audit::AuditEvidence::Protocol,
                            None,
                            copied_bytes,
                        );
                }
                if copied_bytes.is_none() {
                    return;
                }
                let file_name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                notify(LocalBatchProgress {
                    current: done,
                    file_name,
                });
            };
            copy_recursively_with_progress(source, &target, &mut progress)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    })();
    audit.summary(&format!("sources={}; completed_objects={done}; estimated_objects={total}; failed_object={}; copied_bytes={bytes}", paths.len(), if result.is_err() { failed_object.as_deref().or(attempted_source).unwrap_or("unknown") } else { "none" }));
    audit.finish(
        if result.is_ok() {
            oxideterm_audit::AuditOutcome::Succeeded
        } else if done > 0 {
            oxideterm_audit::AuditOutcome::Partial
        } else {
            oxideterm_audit::AuditOutcome::Failed
        },
        oxideterm_audit::AuditEvidence::Protocol,
        None,
        (done > 0).then_some(bytes),
    );
    result
}

pub fn copy_dropped_local_files(
    sources: Vec<String>,
    destination: String,
    total: usize,
    audit_context: Option<AuditContext>,
    mut notify: impl FnMut(LocalBatchProgress),
) -> Result<(), String> {
    let mut done = 0usize;
    let mut bytes = 0u64;
    let mut audit = oxideterm_audit::AuditOperation::in_context(
        audit_context.as_ref(),
        oxideterm_audit::AuditCategory::File,
        "file_copy",
        Some(&format!(
            "destination={destination}; sources={}; mode=drop",
            sources.len()
        )),
    );
    let child_context = audit_context.map(|mut context| {
        context.parent_id = audit.id().map(str::to_owned);
        context
    });
    let mut attempted_source = None;
    let mut failed_object = None;
    let result: Result<(), String> = (|| {
        for source in &sources {
            attempted_source = Some(source.as_str());
            let source_path = std::path::Path::new(source);
            let Some(name) = source_path.file_name() else {
                return Err("Source has no file name".to_string());
            };
            let target = unique_copy_path(&std::path::Path::new(&destination).join(name));
            let mut progress = |path: &std::path::Path, copied_bytes: Option<u64>| {
                if let Some(copied_bytes) = copied_bytes {
                    done += 1;
                    bytes = bytes.saturating_add(copied_bytes);
                } else {
                    failed_object = Some(path.to_string_lossy().into_owned());
                }
                if let Some(context) = child_context.as_ref() {
                    let relative = path
                        .strip_prefix(source_path)
                        .unwrap_or_else(|_| std::path::Path::new(""));
                    let copied_to = if relative.as_os_str().is_empty() {
                        target.clone()
                    } else {
                        target.join(relative)
                    };
                    context
                        .operation(
                            oxideterm_audit::AuditCategory::File,
                            "file_copy",
                            Some(&format!("{} → {}", path.display(), copied_to.display())),
                        )
                        .finish(
                            if copied_bytes.is_some() {
                                oxideterm_audit::AuditOutcome::Succeeded
                            } else {
                                oxideterm_audit::AuditOutcome::Failed
                            },
                            oxideterm_audit::AuditEvidence::Protocol,
                            None,
                            copied_bytes,
                        );
                }
                if copied_bytes.is_none() {
                    return;
                }
                let file_name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                notify(LocalBatchProgress {
                    current: done,
                    file_name,
                });
            };
            copy_recursively_with_progress(source_path, &target, &mut progress)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    })();
    audit.summary(&format!("destination={destination}; sources={}; completed_objects={done}; estimated_objects={total}; failed_object={}; copied_bytes={bytes}", sources.len(), if result.is_err() { failed_object.as_deref().or(attempted_source).unwrap_or("unknown") } else { "none" }));
    audit.finish(
        if result.is_ok() {
            oxideterm_audit::AuditOutcome::Succeeded
        } else if done > 0 {
            oxideterm_audit::AuditOutcome::Partial
        } else {
            oxideterm_audit::AuditOutcome::Failed
        },
        oxideterm_audit::AuditEvidence::Protocol,
        None,
        (done > 0).then_some(bytes),
    );
    result
}

pub fn compress_local_files_audited(paths: &[String], archive_path: &Path) -> Result<(), String> {
    let context = local_file_audit_context(AuditSource::User);
    let mut audit = oxideterm_audit::AuditOperation::in_context(
        context.as_ref(),
        oxideterm_audit::AuditCategory::File,
        "file_archive_create",
        Some(&format!(
            "archive={}; sources={}",
            archive_path.display(),
            paths.len()
        )),
    );
    let child_context = context.map(|mut context| {
        context.parent_id = audit.id().map(str::to_owned);
        context
    });
    let mut completed = 0usize;
    let mut skipped = 0usize;
    let mut input_bytes = 0u64;
    let mut failed_object = None;
    let result = compress_local_files(
        paths,
        &archive_path.to_string_lossy(),
        &mut |path, outcome| {
            let (audit_outcome, bytes) = match outcome {
                ArchiveEntryOutcome::Completed(bytes) => {
                    completed += 1;
                    input_bytes = input_bytes.saturating_add(bytes);
                    (oxideterm_audit::AuditOutcome::Succeeded, Some(bytes))
                }
                ArchiveEntryOutcome::Skipped => {
                    skipped += 1;
                    (oxideterm_audit::AuditOutcome::Unchanged, None)
                }
                ArchiveEntryOutcome::Failed => {
                    failed_object = Some(path.to_string_lossy().into_owned());
                    (oxideterm_audit::AuditOutcome::Failed, None)
                }
            };
            if let Some(context) = child_context.as_ref() {
                context
                    .operation(
                        oxideterm_audit::AuditCategory::File,
                        "file_archive_create",
                        Some(&format!("{} → {}", path.display(), archive_path.display())),
                    )
                    .finish(
                        audit_outcome,
                        oxideterm_audit::AuditEvidence::Protocol,
                        None,
                        bytes,
                    );
            }
        },
    );
    let archive_bytes = result
        .as_ref()
        .ok()
        .and_then(|_| std::fs::metadata(&archive_path).ok())
        .map(|metadata| metadata.len());
    audit.summary(&format!("archive={}; requested_sources={}; completed_entries={completed}; skipped_entries={skipped}; failed_object={}; input_bytes={input_bytes}", archive_path.display(), paths.len(), failed_object.as_deref().unwrap_or("none")));
    audit.finish(
        if result.is_ok() && skipped == 0 {
            oxideterm_audit::AuditOutcome::Succeeded
        } else if completed > 0 || result.is_ok() {
            oxideterm_audit::AuditOutcome::Partial
        } else {
            oxideterm_audit::AuditOutcome::Failed
        },
        oxideterm_audit::AuditEvidence::Protocol,
        None,
        archive_bytes,
    );
    result
}

pub fn extract_local_archive_audited(archive: &str, current_path: &str) -> Result<(), String> {
    let context = local_file_audit_context(AuditSource::User);
    let mut audit = oxideterm_audit::AuditOperation::in_context(
        context.as_ref(),
        oxideterm_audit::AuditCategory::File,
        "file_archive_extract",
        Some(&format!("archive={}; destination={current_path}", archive)),
    );
    let child_context = context.map(|mut context| {
        context.parent_id = audit.id().map(str::to_owned);
        context
    });
    let mut completed = 0usize;
    let mut skipped = 0usize;
    let mut written_bytes = 0u64;
    let mut failed_object = None;
    let result = extract_local_archive(archive, current_path, &mut |path, outcome| {
        let (audit_outcome, bytes) = match outcome {
            ArchiveEntryOutcome::Completed(bytes) => {
                completed += 1;
                written_bytes = written_bytes.saturating_add(bytes);
                (oxideterm_audit::AuditOutcome::Succeeded, Some(bytes))
            }
            ArchiveEntryOutcome::Skipped => {
                skipped += 1;
                (oxideterm_audit::AuditOutcome::Unchanged, None)
            }
            ArchiveEntryOutcome::Failed => {
                failed_object = Some(path.to_string_lossy().into_owned());
                (oxideterm_audit::AuditOutcome::Failed, None)
            }
        };
        if let Some(context) = child_context.as_ref() {
            context
                .operation(
                    oxideterm_audit::AuditCategory::File,
                    "file_archive_extract",
                    Some(&format!("{} → {}", archive, path.display())),
                )
                .finish(
                    audit_outcome,
                    oxideterm_audit::AuditEvidence::Protocol,
                    None,
                    bytes,
                );
        }
    });
    audit.summary(&format!("archive={}; destination={current_path}; completed_entries={completed}; skipped_entries={skipped}; failed_object={}", archive, failed_object.as_deref().unwrap_or("none")));
    audit.finish(
        if result.is_ok() && skipped == 0 {
            oxideterm_audit::AuditOutcome::Succeeded
        } else if completed > 0 || result.is_ok() {
            oxideterm_audit::AuditOutcome::Partial
        } else {
            oxideterm_audit::AuditOutcome::Failed
        },
        oxideterm_audit::AuditEvidence::Protocol,
        None,
        (completed > 0).then_some(written_bytes),
    );
    result
}
