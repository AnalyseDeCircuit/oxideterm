// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fs::{self, OpenOptions};
use std::path::PathBuf;

use crate::error::TrzszError;
use crate::path_guard::{
    build_download_target_path, canonicalize_existing_root, ensure_within_root,
    sanitize_download_rel_path, validate_api_version, validate_owner_id,
};
use crate::{
    MAX_TRANSFER_CHUNK_SIZE, TrzszCreateDownloadDirectoryDto, TrzszDownloadOpenDto,
    TrzszPreparedDownloadRootDto, TrzszState,
};

pub fn prepare_download_root(
    state: &TrzszState,
    owner_id: &str,
    api_version: u32,
    root_path: String,
) -> Result<TrzszPreparedDownloadRootDto, TrzszError> {
    validate_api_version(api_version)?;
    validate_owner_id(owner_id)?;
    let canonical_root = canonicalize_existing_root(&root_path)?;
    Ok(state.prepare_download_root(owner_id, canonical_root))
}

#[expect(clippy::too_many_arguments)]
pub fn open_save_file(
    state: &TrzszState,
    owner_id: &str,
    api_version: u32,
    root_path: String,
    file_name: String,
    _directory: bool,
    overwrite: bool,
) -> Result<TrzszDownloadOpenDto, TrzszError> {
    validate_api_version(api_version)?;
    validate_owner_id(owner_id)?;

    let prepared_root = state
        .prepared_download_root(owner_id)
        .ok_or(TrzszError::RootNotPrepared)?;
    let requested_root = canonicalize_existing_root(&root_path)?;
    if prepared_root != requested_root {
        return Err(TrzszError::RootMismatch);
    }

    let rel_components = sanitize_download_rel_path(&file_name)?;
    let final_path = build_download_target_path(&prepared_root, &rel_components)?;
    ensure_within_root(&prepared_root, &final_path)?;

    if let Ok(metadata) = fs::symlink_metadata(&final_path) {
        if metadata.file_type().is_symlink() {
            return Err(TrzszError::SymlinkNotAllowed(
                final_path.display().to_string(),
            ));
        }
        if metadata.is_dir() {
            return Err(TrzszError::InvalidPath(format!(
                "Target path resolves to a directory: {}",
                final_path.display()
            )));
        }
        if !overwrite {
            return Err(TrzszError::AlreadyExists(final_path.display().to_string()));
        }
    }

    let local_name = rel_components
        .last()
        .cloned()
        .ok_or_else(|| TrzszError::InvalidPath("Empty file name".to_string()))?;
    let temp_path = build_temp_path(&final_path, &local_name)?;
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp_path)?;

    state.register_download_handle(
        owner_id,
        local_name.clone(),
        local_name,
        prepared_root,
        final_path,
        temp_path,
        overwrite,
        file,
    )
}

pub fn create_download_directory(
    state: &TrzszState,
    owner_id: &str,
    api_version: u32,
    root_path: String,
    directory_path: String,
    must_create: bool,
) -> Result<TrzszCreateDownloadDirectoryDto, TrzszError> {
    validate_api_version(api_version)?;
    validate_owner_id(owner_id)?;

    let prepared_root = state
        .prepared_download_root(owner_id)
        .ok_or(TrzszError::RootNotPrepared)?;
    let requested_root = canonicalize_existing_root(&root_path)?;
    if prepared_root != requested_root {
        return Err(TrzszError::RootMismatch);
    }

    let rel_components = sanitize_download_rel_path(&directory_path)?;
    if rel_components.len() > 1 {
        let mut parent_path = prepared_root.clone();
        for component in &rel_components[..rel_components.len() - 1] {
            parent_path.push(component);
            match fs::symlink_metadata(&parent_path) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() {
                        return Err(TrzszError::SymlinkNotAllowed(
                            parent_path.display().to_string(),
                        ));
                    }
                    if !metadata.is_dir() {
                        return Err(TrzszError::InvalidPath(format!(
                            "Parent path is not a directory: {}",
                            parent_path.display()
                        )));
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(TrzszError::InvalidPath(format!(
                        "Parent directory does not exist: {}",
                        parent_path.display()
                    )));
                }
                Err(error) => return Err(TrzszError::Io(error)),
            }
        }
    }
    let final_path = build_download_target_path(&prepared_root, &rel_components)?;
    ensure_within_root(&prepared_root, &final_path)?;

    match fs::symlink_metadata(&final_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(TrzszError::SymlinkNotAllowed(
                    final_path.display().to_string(),
                ));
            }
            if metadata.is_dir() {
                if must_create {
                    return Err(TrzszError::AlreadyExists(final_path.display().to_string()));
                }
                return Ok(TrzszCreateDownloadDirectoryDto { created: false });
            }
            return Err(TrzszError::InvalidPath(format!(
                "Target path resolves to a file: {}",
                final_path.display()
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(TrzszError::Io(error)),
    }

    fs::create_dir(&final_path)?;
    state.register_download_directory(owner_id, final_path);
    Ok(TrzszCreateDownloadDirectoryDto { created: true })
}

pub fn commit_download_directory(
    state: &TrzszState,
    owner_id: &str,
    api_version: u32,
    root_path: String,
    directory_path: String,
) -> Result<(), TrzszError> {
    validate_api_version(api_version)?;
    validate_owner_id(owner_id)?;

    let prepared_root = state
        .prepared_download_root(owner_id)
        .ok_or(TrzszError::RootNotPrepared)?;
    let requested_root = canonicalize_existing_root(&root_path)?;
    if prepared_root != requested_root {
        return Err(TrzszError::RootMismatch);
    }

    let rel_components = sanitize_download_rel_path(&directory_path)?;
    let final_path = build_download_target_path(&prepared_root, &rel_components)?;
    ensure_within_root(&prepared_root, &final_path)?;
    state.commit_download_directory(owner_id, &final_path);
    Ok(())
}

pub fn remove_download_directory(
    state: &TrzszState,
    owner_id: &str,
    api_version: u32,
    root_path: String,
    directory_path: String,
) -> Result<(), TrzszError> {
    validate_api_version(api_version)?;
    validate_owner_id(owner_id)?;

    let prepared_root = state
        .prepared_download_root(owner_id)
        .ok_or(TrzszError::RootNotPrepared)?;
    let requested_root = canonicalize_existing_root(&root_path)?;
    if prepared_root != requested_root {
        return Err(TrzszError::RootMismatch);
    }

    let rel_components = sanitize_download_rel_path(&directory_path)?;
    let final_path = build_download_target_path(&prepared_root, &rel_components)?;
    ensure_within_root(&prepared_root, &final_path)?;

    match fs::symlink_metadata(&final_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(TrzszError::SymlinkNotAllowed(
                    final_path.display().to_string(),
                ));
            }
            if !metadata.is_dir() {
                return Err(TrzszError::InvalidPath(format!(
                    "Target path is not a directory: {}",
                    final_path.display()
                )));
            }
            match fs::remove_dir(&final_path) {
                Ok(()) => {
                    state.commit_download_directory(owner_id, &final_path);
                    Ok(())
                }
                Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    state.commit_download_directory(owner_id, &final_path);
                    Ok(())
                }
                Err(error) => Err(TrzszError::Io(error)),
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            state.commit_download_directory(owner_id, &final_path);
            Ok(())
        }
        Err(error) => Err(TrzszError::Io(error)),
    }
}

pub fn remove_download_file(
    state: &TrzszState,
    owner_id: &str,
    api_version: u32,
    root_path: String,
    file_path: String,
) -> Result<(), TrzszError> {
    validate_api_version(api_version)?;
    validate_owner_id(owner_id)?;

    let prepared_root = state
        .prepared_download_root(owner_id)
        .ok_or(TrzszError::RootNotPrepared)?;
    let requested_root = canonicalize_existing_root(&root_path)?;
    if prepared_root != requested_root {
        return Err(TrzszError::RootMismatch);
    }

    let rel_components = sanitize_download_rel_path(&file_path)?;
    let final_path = build_download_target_path(&prepared_root, &rel_components)?;
    ensure_within_root(&prepared_root, &final_path)?;

    match fs::symlink_metadata(&final_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(TrzszError::SymlinkNotAllowed(
                    final_path.display().to_string(),
                ));
            }
            if metadata.is_dir() {
                return Err(TrzszError::InvalidPath(format!(
                    "Target path is a directory: {}",
                    final_path.display()
                )));
            }
            match fs::remove_file(&final_path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(TrzszError::Io(error)),
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(TrzszError::Io(error)),
    }
}

pub fn write_download_chunk(
    state: &TrzszState,
    owner_id: &str,
    api_version: u32,
    writer_id: &str,
    data: Vec<u8>,
) -> Result<(), TrzszError> {
    validate_api_version(api_version)?;
    validate_owner_id(owner_id)?;
    if data.len() > MAX_TRANSFER_CHUNK_SIZE {
        return Err(TrzszError::ChunkTooLarge {
            requested: data.len(),
            max: MAX_TRANSFER_CHUNK_SIZE,
        });
    }
    state.write_download_chunk(owner_id, writer_id, &data)
}

pub fn finish_download_file(
    state: &TrzszState,
    owner_id: &str,
    api_version: u32,
    writer_id: &str,
) -> Result<(), TrzszError> {
    validate_api_version(api_version)?;
    validate_owner_id(owner_id)?;
    state.finish_download_handle(owner_id, writer_id)
}

pub fn abort_download_file(
    state: &TrzszState,
    owner_id: &str,
    api_version: u32,
    writer_id: &str,
) -> Result<(), TrzszError> {
    validate_api_version(api_version)?;
    validate_owner_id(owner_id)?;
    state.abort_download_handle(owner_id, writer_id)
}

fn build_temp_path(final_path: &PathBuf, local_name: &str) -> Result<PathBuf, TrzszError> {
    let parent = final_path.parent().ok_or_else(|| {
        TrzszError::InvalidPath(format!(
            "Final path has no parent: {}",
            final_path.display()
        ))
    })?;
    let stem = format!(".{local_name}.oxide-trzsz-{}.part", uuid::Uuid::new_v4());
    Ok(parent.join(stem))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::Duration;

    use tempfile::tempdir;

    use crate::{TRZSZ_API_VERSION, TrzszState};

    use super::{
        abort_download_file, commit_download_directory, create_download_directory,
        finish_download_file, open_save_file, prepare_download_root, write_download_chunk,
    };

    #[test]
    fn writes_and_finishes_download_via_temp_file() {
        let temp = tempdir().expect("tempdir");
        let state = TrzszState::new_for_tests(Duration::from_secs(60));
        prepare_download_root(
            state.as_ref(),
            "owner-1",
            TRZSZ_API_VERSION,
            temp.path().to_string_lossy().to_string(),
        )
        .expect("prepare root");

        let open = open_save_file(
            state.as_ref(),
            "owner-1",
            TRZSZ_API_VERSION,
            temp.path().to_string_lossy().to_string(),
            "nested/file.txt".to_string(),
            true,
            false,
        )
        .expect("open save file");

        write_download_chunk(
            state.as_ref(),
            "owner-1",
            TRZSZ_API_VERSION,
            &open.writer_id,
            b"hello".to_vec(),
        )
        .expect("write chunk");
        finish_download_file(
            state.as_ref(),
            "owner-1",
            TRZSZ_API_VERSION,
            &open.writer_id,
        )
        .expect("finish download");

        assert_eq!(
            fs::read(temp.path().join("nested").join("file.txt")).expect("read file"),
            b"hello"
        );
        assert!(!PathBuf::from(open.temp_path).exists());
    }

    #[test]
    fn abort_removes_temp_file() {
        let temp = tempdir().expect("tempdir");
        let state = TrzszState::new_for_tests(Duration::from_secs(60));
        prepare_download_root(
            state.as_ref(),
            "owner-1",
            TRZSZ_API_VERSION,
            temp.path().to_string_lossy().to_string(),
        )
        .expect("prepare root");

        let open = open_save_file(
            state.as_ref(),
            "owner-1",
            TRZSZ_API_VERSION,
            temp.path().to_string_lossy().to_string(),
            "file.txt".to_string(),
            false,
            false,
        )
        .expect("open save file");

        write_download_chunk(
            state.as_ref(),
            "owner-1",
            TRZSZ_API_VERSION,
            &open.writer_id,
            b"hello".to_vec(),
        )
        .expect("write chunk");
        abort_download_file(
            state.as_ref(),
            "owner-1",
            TRZSZ_API_VERSION,
            &open.writer_id,
        )
        .expect("abort download");

        assert!(!PathBuf::from(open.temp_path).exists());
        assert!(!temp.path().join("file.txt").exists());
    }

    #[test]
    fn prepared_root_must_match_open_root() {
        let temp = tempdir().expect("tempdir");
        let other = tempdir().expect("other tempdir");
        let state = TrzszState::new_for_tests(Duration::from_secs(60));
        prepare_download_root(
            state.as_ref(),
            "owner-1",
            TRZSZ_API_VERSION,
            temp.path().to_string_lossy().to_string(),
        )
        .expect("prepare root");

        let error = open_save_file(
            state.as_ref(),
            "owner-1",
            TRZSZ_API_VERSION,
            other.path().to_string_lossy().to_string(),
            "file.txt".to_string(),
            false,
            false,
        )
        .expect_err("root mismatch should be rejected");

        assert!(error.to_string().contains("does not match"));
    }

    #[test]
    fn commit_download_directory_keeps_created_directory_out_of_owner_cleanup() {
        let temp = tempdir().expect("tempdir");
        let state = TrzszState::new_for_tests(Duration::from_secs(60));
        prepare_download_root(
            state.as_ref(),
            "owner-1",
            TRZSZ_API_VERSION,
            temp.path().to_string_lossy().to_string(),
        )
        .expect("prepare root");

        create_download_directory(
            state.as_ref(),
            "owner-1",
            TRZSZ_API_VERSION,
            temp.path().to_string_lossy().to_string(),
            "kept".to_string(),
            false,
        )
        .expect("create directory");
        commit_download_directory(
            state.as_ref(),
            "owner-1",
            TRZSZ_API_VERSION,
            temp.path().to_string_lossy().to_string(),
            "kept".to_string(),
        )
        .expect("commit directory");

        state.cleanup_owner("owner-1");

        assert!(temp.path().join("kept").is_dir());
    }

    #[test]
    fn cleanup_owner_reports_partial_directory_cleanup_failure() {
        let temp = tempdir().expect("tempdir");
        let state = TrzszState::new_for_tests(Duration::from_secs(60));
        prepare_download_root(
            state.as_ref(),
            "owner-1",
            TRZSZ_API_VERSION,
            temp.path().to_string_lossy().to_string(),
        )
        .expect("prepare root");

        create_download_directory(
            state.as_ref(),
            "owner-1",
            TRZSZ_API_VERSION,
            temp.path().to_string_lossy().to_string(),
            "dirty".to_string(),
            false,
        )
        .expect("create directory");
        std::fs::write(temp.path().join("dirty").join("leftover.txt"), b"leftover")
            .expect("dirty directory content");

        let cleanup = state.cleanup_owner("owner-1");

        assert_eq!(cleanup.cleanup_errors, 1);
    }
}
