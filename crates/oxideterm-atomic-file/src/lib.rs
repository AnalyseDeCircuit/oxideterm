// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Durable atomic file replacement primitives without business-layer dependencies.

mod platform;
mod temporary;

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
};

use temporary::{create_temporary_file, parent_directory};

/// Writes `contents` durably and atomically replaces `path`.
pub fn durable_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    durable_write_with_before_replace(path, contents, || Ok(()))
}

/// Writes `contents` durably, then invokes `before_replace` immediately before replacement.
///
/// The callback is intentionally narrow so callers can preserve fault-injection semantics
/// without exposing temporary paths or platform replacement details.
pub fn durable_write_with_before_replace<F>(
    path: &Path,
    contents: &[u8],
    before_replace: F,
) -> io::Result<()>
where
    F: FnOnce() -> io::Result<()>,
{
    let parent = parent_directory(path);
    fs::create_dir_all(parent)?;
    let (temporary_path, mut temporary_file) = create_temporary_file(path)?;

    // The destination remains unchanged until all temporary contents are durable.
    let write_result = (|| {
        temporary_file.write_all(contents)?;
        temporary_file.flush()?;
        temporary_file.sync_all()?;
        drop(temporary_file);
        before_replace()?;
        replace_and_sync_parent(&temporary_path, path, parent)
    })();

    if write_result.is_err() {
        // Replacement removes the temporary path on success, so cleanup is harmless
        // when only the final parent-directory sync failed.
        let _ = fs::remove_file(&temporary_path);
    }
    write_result
}

/// Durably replaces `destination` with an existing file from the same directory.
pub fn durable_replace(source: &Path, destination: &Path) -> io::Result<()> {
    let destination_parent = parent_directory(destination);
    if parent_directory(source) != destination_parent {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "durable replacement requires source and destination in the same directory",
        ));
    }

    // Sync the caller-provided source before making it visible at the destination.
    // FlushFileBuffers requires write access on Windows, even for an already-written source.
    OpenOptions::new()
        .read(true)
        .write(cfg!(windows))
        .open(source)?
        .sync_all()?;
    replace_and_sync_parent(source, destination, destination_parent)
}

/// Removes `path` if present and durably records the directory entry change.
pub fn durable_remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => platform::sync_directory(parent_directory(path)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Persists a batch of directory-entry changes with the platform's required handle flags.
pub fn sync_directory(directory: &Path) -> io::Result<()> {
    platform::sync_directory(directory)
}

fn replace_and_sync_parent(source: &Path, destination: &Path, parent: &Path) -> io::Result<()> {
    platform::atomic_replace(source, destination)?;
    platform::sync_directory(parent)
}

#[cfg(test)]
mod tests {
    use std::{fs, io};

    use super::*;

    #[test]
    fn durable_file_lifecycle_preserves_contents_on_failure_and_cleans_up() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested").join("state.json");

        durable_write(&path, b"old").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"old");

        durable_write(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        let error = durable_write_with_before_replace(&path, b"uncommitted", || {
            Err(io::Error::other("injected failure"))
        })
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        durable_remove(&path).unwrap();
        durable_remove(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn durable_replace_requires_one_directory() {
        let directory = tempfile::tempdir().unwrap();
        let other_directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.tmp");
        let destination = other_directory.path().join("state.json");
        fs::write(&source, b"state").unwrap();

        let error = durable_replace(&source, &destination).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(source.exists());
        assert!(!destination.exists());
    }

    #[test]
    fn durable_replace_moves_source_over_destination() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.tmp");
        let destination = directory.path().join("state.json");
        fs::write(&source, b"new").unwrap();
        fs::write(&destination, b"old").unwrap();

        durable_replace(&source, &destination).unwrap();

        assert!(!source.exists());
        assert_eq!(fs::read(destination).unwrap(), b"new");
    }
}
