use crate::AuditError;
use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};

/// Held until every writer of this instance has finished its database work. PID reuse and wall-clock
/// changes cannot make another running writer appear abandoned.
pub(crate) struct InstanceLease {
    file: File,
    path: PathBuf,
    pub(crate) id: String,
}

impl InstanceLease {
    pub(crate) fn directory(database: &Path) -> Result<PathBuf, AuditError> {
        let mut name = database
            .file_name()
            .ok_or(AuditError::Storage)?
            .to_os_string();
        name.push(".instances");
        Ok(database.with_file_name(name))
    }

    pub(crate) fn try_acquire(database: &Path, id: &str) -> Result<Option<Self>, AuditError> {
        let id = uuid::Uuid::parse_str(id)
            .map_err(|_| AuditError::Integrity)?
            .to_string();
        let directory = Self::directory(database)?;
        std::fs::create_dir_all(&directory).map_err(|_| AuditError::Storage)?;
        let path = directory.join(format!("{id}.lock"));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path).map_err(|_| AuditError::Storage)?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(Self { file, path, id })),
            Err(error) if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {
                Ok(None)
            }
            Err(_) => Err(AuditError::Storage),
        }
    }
}

impl Drop for InstanceLease {
    fn drop(&mut self) {
        // IDs are never reused. Once this lease's database work has ended, a
        // late recovery contender can only find an already-terminal operation.
        let _ = std::fs::remove_file(&self.path);
        let _ = FileExt::unlock(&self.file);
    }
}
