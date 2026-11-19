use super::*;
pub(in crate::workspace::sftp) use oxideterm_sftp::join_remote_path as join_sftp_path;

pub(in crate::workspace::sftp) async fn load_remote_sftp_listing(
    backend: SftpRemoteBackend,
    path: &str,
) -> Result<RemoteSftpListing, String> {
    load_remote_sftp_listing_inner(backend, path, true).await
}

pub(in crate::workspace::sftp) async fn load_remote_sftp_completion_listing(
    backend: SftpRemoteBackend,
    path: &str,
) -> Result<RemoteSftpListing, String> {
    // Typeahead is observational and must not replace the visible SFTP cwd.
    load_remote_sftp_listing_inner(backend, path, false).await
}

async fn load_remote_sftp_listing_inner(
    backend: SftpRemoteBackend,
    path: &str,
    update_ready_path: bool,
) -> Result<RemoteSftpListing, String> {
    if let SftpRemoteBackend::Ftp { runtime } = &backend {
        return runtime.listing(path).await;
    }
    let listing =
        oxideterm_sftp::load_sftp_listing(|| backend.acquire_transfer_sftp(), path).await?;
    if update_ready_path
        && let SftpRemoteBackend::Node {
            router, node_id, ..
        } = &backend
    {
        let connection = router
            .resolve_connection(node_id)
            .await
            .map_err(|error| error.to_string())?;
        router
            .mark_sftp_ready_from_listing(
                node_id,
                connection.handle.connection_id(),
                Some(listing.cwd.clone()),
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(listing)
}

pub(in crate::workspace::sftp) async fn load_remote_sftp_preview(
    backend: SftpRemoteBackend,
    path: &str,
) -> Result<PreviewContent, String> {
    if let SftpRemoteBackend::Ftp { runtime } = &backend {
        return runtime.preview(path, 0).await;
    }
    oxideterm_sftp::load_sftp_preview(|| backend.acquire_transfer_sftp(), path, None).await
}

pub(in crate::workspace::sftp) async fn load_remote_sftp_preview_hex(
    backend: SftpRemoteBackend,
    path: &str,
    offset: u64,
) -> Result<PreviewContent, String> {
    if let SftpRemoteBackend::Ftp { runtime } = &backend {
        return runtime.preview(path, offset).await;
    }
    oxideterm_sftp::load_sftp_preview(|| backend.acquire_transfer_sftp(), path, Some(offset)).await
}

pub(in crate::workspace::sftp) async fn save_remote_sftp_preview(
    backend: SftpRemoteBackend,
    path: &str,
    expected_version: Option<&str>,
    content: &str,
    encoding: &str,
    line_ending: TextLineEnding,
) -> Result<SftpPreviewSaveResult, SftpError> {
    let (encoded, encoding) = oxideterm_sftp::encode_preview_text(content, encoding, line_ending);
    if let SftpRemoteBackend::Ftp { runtime } = &backend {
        return runtime
            .save(path, &encoded, encoding)
            .await
            .map_err(SftpError::WriteError);
    }
    let session = backend
        .acquire_transfer_sftp()
        .await
        .map_err(SftpError::ChannelError)?;
    oxideterm_sftp::write_sftp_preview(&session, path, &encoded, expected_version, encoding).await
}
