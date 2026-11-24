// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[cfg(test)]
use crate::{FileInfo as RemoteFileInfo, FileType as RemoteFileType};
use crate::{
    ListFilter as RemoteListFilter, PreviewContent, RemoteSftpListing, SftpError,
    SftpPreviewSaveResult, SftpSession, SortOrder as RemoteSortOrder,
    remote_listing_from_file_infos,
};
use oxideterm_audit::{AuditCategory, AuditContext, AuditOperation};

// Use short-lived channels for browser I/O so a slow operation cannot hold
// the shared directory owner. A recoverable channel failure gets one retry.
pub async fn load_sftp_listing<F>(
    acquire: impl Fn() -> F,
    path: &str,
) -> Result<RemoteSftpListing, String>
where
    F: Future<Output = Result<SftpSession, String>>,
{
    let session = acquire().await?;
    match list_remote_sftp_once(&session, path).await {
        Err(error) if error.is_channel_recoverable() => {
            let session = acquire().await?;
            list_remote_sftp_once(&session, path)
                .await
                .map_err(|error| error.to_string())
        }
        result => result.map_err(|error| error.to_string()),
    }
}

pub async fn audit_browser_listing(
    path: &str,
    context: Option<AuditContext>,
    listing: impl Future<Output = Result<RemoteSftpListing, String>>,
) -> Result<RemoteSftpListing, String> {
    let mut audit = AuditOperation::in_context(
        context.as_ref(),
        AuditCategory::File,
        "file_browse",
        Some(path),
    );
    let result = if let Some(context) = context {
        context.scope(listing).await
    } else {
        listing.await
    };
    if let Ok(listing) = &result {
        audit.summary(&format!("path={path}; entries={}", listing.files.len()));
    }
    audit.result(&result);
    result
}

pub async fn load_sftp_preview<F>(
    acquire: impl Fn() -> F,
    path: &str,
    offset: Option<u64>,
) -> Result<PreviewContent, String>
where
    F: Future<Output = Result<SftpSession, String>>,
{
    let session = acquire().await?;
    match read_preview(&session, path, offset).await {
        Err(error) if error.is_channel_recoverable() => {
            let session = acquire().await?;
            read_preview(&session, path, offset)
                .await
                .map_err(|error| error.to_string())
        }
        result => result.map_err(|error| error.to_string()),
    }
}

async fn read_preview(
    session: &SftpSession,
    path: &str,
    offset: Option<u64>,
) -> Result<PreviewContent, SftpError> {
    match offset {
        Some(offset) => session.preview_with_offset(path, offset).await,
        None => session.preview(path).await,
    }
}

pub async fn write_sftp_preview(
    sftp: &SftpSession,
    path: &str,
    encoded: &[u8],
    expected_version: Option<&str>,
    target_encoding: &str,
) -> Result<SftpPreviewSaveResult, SftpError> {
    // Saving uses a short-lived SFTP channel so a large write/stat round trip
    // cannot stall the shared directory listing owner.
    let write_result = sftp
        .write_content_checked(path, encoded, expected_version)
        .await?;
    let file_info = sftp.stat(path).await?;
    Ok(SftpPreviewSaveResult {
        version: Some(write_result.version),
        mtime: (file_info.modified > 0).then_some(file_info.modified as u64),
        size: Some(file_info.size),
        encoding_used: target_encoding.to_string(),
        atomic_write: write_result.atomic_write,
    })
}

async fn list_remote_sftp_once(
    sftp: &SftpSession,
    path: &str,
) -> Result<RemoteSftpListing, SftpError> {
    // Tauri's node_sftp_list_dir performs one SFTP path resolution inside
    // list_dir. Native used to canonicalize here and then list_dir canonicalized
    // again, adding a visible RTT on every folder change.
    let (cwd, entries) = sftp
        .list_dir_with_cwd(
            path,
            Some(RemoteListFilter {
                show_hidden: true,
                pattern: None,
                sort: RemoteSortOrder::Name,
            }),
        )
        .await?;
    Ok(remote_listing_from_file_infos(cwd, entries))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_display_names_have_distinct_browser_selection_keys() {
        let entries = ["/\0ff.log", "/\0fe.log"].map(|path| RemoteFileInfo {
            name: "�.log".into(),
            path: path.into(),
            file_type: RemoteFileType::File,
            size: 1,
            modified: 1,
            permissions: "600".into(),
            owner: None,
            group: None,
            is_symlink: false,
            symlink_target: None,
        });
        let listing = remote_listing_from_file_infos("/".into(), entries.into());
        let mut identities = listing
            .files
            .iter()
            .map(|file| (file.name.as_str(), file.path.as_str()))
            .collect::<Vec<_>>();
        identities.sort();
        assert_eq!(
            identities,
            vec![("\0fe.log", "/\0fe.log"), ("\0ff.log", "/\0ff.log")]
        );
        assert!(
            listing
                .files
                .iter()
                .all(|file| crate::display_remote_path(&file.name) == "�.log")
        );
    }
}
