// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::SftpFileOperation;
use oxideterm_local_files::LocalDrive;
use oxideterm_sftp::SftpFileEntry;
use std::{io, path::Path};

/// Local paths belong to the host. Remote paths and transfers use the workspace's
/// existing request/result boundary, including when both panes show remote files.
pub trait SftpLocalHost {
    fn home(&self) -> String;
    fn download_dir(&self) -> String;
    fn drives(&self) -> Vec<LocalDrive>;
    fn list(&self, path: &str) -> io::Result<Vec<SftpFileEntry>>;
    fn mutate(&self, directory: &str, operation: SftpFileOperation) -> io::Result<()>;
    fn read_text(&self, path: &str) -> io::Result<String>;
    fn font_bytes(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn image(&self, path: &str) -> gpui::ImageSource;
}

#[cfg(not(target_family = "wasm"))]
pub(crate) struct DesktopLocalHost;

#[cfg(not(target_family = "wasm"))]
impl SftpLocalHost for DesktopLocalHost {
    fn home(&self) -> String {
        crate::home_path()
    }

    fn download_dir(&self) -> String {
        crate::default_download_path()
    }

    fn drives(&self) -> Vec<LocalDrive> {
        oxideterm_local_files::local_drives()
    }

    fn list(&self, path: &str) -> io::Result<Vec<SftpFileEntry>> {
        crate::list_local_files(path)
    }

    fn mutate(&self, directory: &str, operation: SftpFileOperation) -> io::Result<()> {
        operation.apply_local(directory)
    }

    fn read_text(&self, path: &str) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn font_bytes(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn image(&self, path: &str) -> gpui::ImageSource {
        Path::new(path).into()
    }
}

pub(crate) struct BrowseAudit {
    #[cfg(not(target_family = "wasm"))]
    context: Option<oxideterm_audit::AuditContext>,
}

impl BrowseAudit {
    // Capture the caller before a folder picker yields; audit only the selected path.
    pub(crate) fn capture() -> Self {
        Self {
            #[cfg(not(target_family = "wasm"))]
            context: oxideterm_local_files::local_file_audit_context(
                oxideterm_audit::AuditSource::User,
            ),
        }
    }

    pub(crate) fn run<T, E>(
        self,
        path: String,
        browse: impl FnOnce(String) -> Result<T, E>,
    ) -> Result<T, E> {
        #[cfg(not(target_family = "wasm"))]
        let audit = oxideterm_audit::AuditOperation::in_context(
            self.context.as_ref(),
            oxideterm_audit::AuditCategory::File,
            "file_browse",
            Some(&path),
        );
        let result = browse(path);
        #[cfg(not(target_family = "wasm"))]
        audit.result(&result);
        result
    }
}
