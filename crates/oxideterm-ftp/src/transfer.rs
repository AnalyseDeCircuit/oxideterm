use std::{future::Future, path::Path, time::Duration};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::{
    Error, FtpSession, Result,
    session::{COMMAND_TIMEOUT, validate_path},
};

const CHUNK_BYTES: usize = 256 * 1024;
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug)]
pub struct TransferProgress {
    pub completed: u64,
    pub total: Option<u64>,
}

impl FtpSession {
    pub async fn read(
        &mut self,
        path: &str,
        limit: usize,
        cancel: &CancellationToken,
    ) -> Result<Vec<u8>> {
        validate_path(path)?;
        self.operate(cancel, None, |mut stream| async move {
            let mut reader = tokio::time::timeout(COMMAND_TIMEOUT, stream.retr_as_stream(path))
                .await
                .map_err(|_| Error::Timeout)??;
            let mut result = Zeroizing::new(Vec::new());
            let mut buffer = Zeroizing::new(vec![0; CHUNK_BYTES.min(limit.saturating_add(1))]);
            loop {
                let count = tokio::time::timeout(IDLE_TIMEOUT, reader.read(&mut buffer))
                    .await
                    .map_err(|_| Error::Timeout)??;
                if count == 0 {
                    break;
                }
                if count > limit.saturating_sub(result.len()) {
                    return Err(Error::TooLarge);
                }
                result.extend_from_slice(&buffer[..count]);
            }
            tokio::time::timeout(COMMAND_TIMEOUT, reader.finish())
                .await
                .map_err(|_| Error::Timeout)??;
            Ok((stream, std::mem::take(&mut *result)))
        })
        .await
    }

    pub async fn write(
        &mut self,
        path: &str,
        content: &[u8],
        cancel: &CancellationToken,
    ) -> Result<()> {
        validate_path(path)?;
        let temporary = remote_temporary_path(path);
        self.pending_upload = Some(temporary.clone());
        let result = self
            .operate(cancel, None, |mut stream| async move {
                let mut writer =
                    tokio::time::timeout(COMMAND_TIMEOUT, stream.put_with_stream(&temporary))
                        .await
                        .map_err(|_| Error::Timeout)??;
                for chunk in content.chunks(CHUNK_BYTES) {
                    tokio::time::timeout(IDLE_TIMEOUT, writer.write_all(chunk))
                        .await
                        .map_err(|_| Error::Timeout)??;
                }
                tokio::time::timeout(COMMAND_TIMEOUT, writer.finish())
                    .await
                    .map_err(|_| Error::Timeout)??;
                // Never delete the previous destination to work around a failed rename.
                tokio::time::timeout(COMMAND_TIMEOUT, stream.rename(temporary.as_str(), path))
                    .await
                    .map_err(|_| Error::Timeout)??;
                Ok((stream, ()))
            })
            .await;
        if result.is_ok() {
            self.pending_upload = None;
        }
        result
    }

    pub async fn upload<Fut: Future<Output = Result<()>>>(
        &mut self,
        local: &Path,
        remote: &str,
        cancel: &CancellationToken,
        progress: impl Fn(TransferProgress) -> Fut,
    ) -> Result<u64> {
        validate_path(remote)?;
        let mut file = tokio::fs::File::open(local).await?;
        let total = file.metadata().await?.len();
        let temporary = remote_temporary_path(remote);
        self.pending_upload = Some(temporary.clone());
        let result = self
            .operate(cancel, None, |mut stream| async move {
                let mut writer =
                    tokio::time::timeout(COMMAND_TIMEOUT, stream.put_with_stream(&temporary))
                        .await
                        .map_err(|_| Error::Timeout)??;
                let mut buffer = Zeroizing::new(vec![0; CHUNK_BYTES]);
                let mut completed = 0;
                loop {
                    let count = file.read(&mut buffer).await?;
                    if count == 0 {
                        break;
                    }
                    tokio::time::timeout(IDLE_TIMEOUT, writer.write_all(&buffer[..count]))
                        .await
                        .map_err(|_| Error::Timeout)??;
                    completed += count as u64;
                    progress(TransferProgress {
                        completed,
                        total: Some(total),
                    })
                    .await?;
                }
                tokio::time::timeout(COMMAND_TIMEOUT, writer.finish())
                    .await
                    .map_err(|_| Error::Timeout)??;
                if completed != total {
                    return Err(Error::Protocol);
                }
                tokio::time::timeout(COMMAND_TIMEOUT, stream.rename(temporary.as_str(), remote))
                    .await
                    .map_err(|_| Error::Timeout)??;
                Ok((stream, completed))
            })
            .await;
        if result.is_ok() {
            self.pending_upload = None;
        }
        result
    }

    pub async fn download<Fut: Future<Output = Result<()>>>(
        &mut self,
        remote: &str,
        local: &Path,
        replace: bool,
        cancel: &CancellationToken,
        progress: impl Fn(TransferProgress) -> Fut,
    ) -> Result<u64> {
        validate_path(remote)?;
        let parent = local.parent().ok_or(Error::InvalidInput)?;
        let parent = parent.to_owned();
        let (file, temporary) = tokio::task::spawn_blocking(move || {
            tempfile::Builder::new()
                .prefix(".oxideterm-")
                .suffix(".part")
                .tempfile_in(parent)
                .map(|file| file.into_parts())
        })
        .await
        .map_err(|_| Error::Protocol)??;
        let mut file = tokio::fs::File::from_std(file);
        let outcome = self
            .operate(cancel, None, |mut stream| async move {
                let total = match tokio::time::timeout(COMMAND_TIMEOUT, stream.size(remote))
                    .await
                    .map_err(|_| Error::Timeout)?
                {
                    Ok(size) => Some(size as u64),
                    Err(suppaftp::FtpError::UnexpectedResponse(_)) => None,
                    Err(error) => return Err(error.into()),
                };
                let mut reader =
                    tokio::time::timeout(COMMAND_TIMEOUT, stream.retr_as_stream(remote))
                        .await
                        .map_err(|_| Error::Timeout)??;
                let mut completed = 0;
                let mut buffer = Zeroizing::new(vec![0; CHUNK_BYTES]);
                loop {
                    let count = tokio::time::timeout(IDLE_TIMEOUT, reader.read(&mut buffer))
                        .await
                        .map_err(|_| Error::Timeout)??;
                    if count == 0 {
                        break;
                    }
                    file.write_all(&buffer[..count]).await?;
                    completed += count as u64;
                    progress(TransferProgress { completed, total }).await?;
                }
                tokio::time::timeout(COMMAND_TIMEOUT, reader.finish())
                    .await
                    .map_err(|_| Error::Timeout)??;
                if total.is_some_and(|size| size != completed) {
                    return Err(Error::Protocol);
                }
                file.sync_all().await?;
                Ok((stream, completed))
            })
            .await;
        let completed = outcome?;
        let destination = local.to_owned();
        tokio::task::spawn_blocking(move || {
            // The TempPath guard also removes partial data when the caller drops its future.
            if replace {
                oxideterm_atomic_file::durable_replace(&temporary, &destination)
            } else {
                std::fs::hard_link(&temporary, &destination)
            }
        })
        .await
        .map_err(|_| Error::Protocol)??;
        Ok(completed)
    }

    pub async fn upload_directory<Fut: Future<Output = Result<()>>>(
        &mut self,
        local: &Path,
        remote: &str,
        cancel: &CancellationToken,
        progress: impl Fn(TransferProgress) -> Fut,
    ) -> Result<u64> {
        let mut pending = vec![(local.to_owned(), remote.to_owned())];
        let mut completed = 0;
        while let Some((local, remote)) = pending.pop() {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let metadata = tokio::fs::symlink_metadata(&local).await?;
            if metadata.is_symlink() {
                return Err(Error::InvalidInput);
            }
            if metadata.is_dir() {
                match self.stat(&remote, cancel).await? {
                    Some(entry) if entry.kind == crate::EntryKind::Directory => {}
                    Some(_) => return Err(Error::Server(550)),
                    None => self.mkdir(&remote, cancel).await?,
                }
                let mut entries = tokio::fs::read_dir(&local).await?;
                while let Some(entry) = entries.next_entry().await? {
                    let name = entry
                        .file_name()
                        .into_string()
                        .map_err(|_| Error::InvalidInput)?;
                    crate::listing::validate_entry_name(&name)?;
                    pending.push((
                        entry.path(),
                        format!("{}/{}", remote.trim_end_matches('/'), name),
                    ));
                }
            } else {
                let base = completed;
                completed += self
                    .upload(&local, &remote, cancel, |value| {
                        progress(TransferProgress {
                            completed: base + value.completed,
                            total: None,
                        })
                    })
                    .await?;
            }
        }
        Ok(completed)
    }

    pub async fn download_directory<Fut: Future<Output = Result<()>>>(
        &mut self,
        remote: &str,
        local: &Path,
        cancel: &CancellationToken,
        progress: impl Fn(TransferProgress) -> Fut,
    ) -> Result<u64> {
        let mut pending = vec![(remote.to_owned(), local.to_owned())];
        let mut completed = 0;
        while let Some((remote, local)) = pending.pop() {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            match tokio::fs::symlink_metadata(&local).await {
                Ok(metadata) if !metadata.is_dir() || metadata.is_symlink() => {
                    return Err(Error::InvalidInput);
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    tokio::fs::create_dir(&local).await?
                }
                Err(error) => return Err(error.into()),
            }
            for entry in self.list(&remote, cancel).await? {
                let source = format!("{}/{}", remote.trim_end_matches('/'), entry.name);
                let destination = local.join(&entry.name);
                match entry.kind {
                    crate::EntryKind::Directory => pending.push((source, destination)),
                    crate::EntryKind::Symlink => return Err(Error::InvalidInput),
                    crate::EntryKind::File => {
                        let base = completed;
                        completed += self
                            .download(&source, &destination, true, cancel, |value| {
                                progress(TransferProgress {
                                    completed: base + value.completed,
                                    total: None,
                                })
                            })
                            .await?;
                    }
                }
            }
        }
        Ok(completed)
    }
}

fn remote_temporary_path(path: &str) -> String {
    let parent = path.rsplit_once('/').map(|(parent, _)| parent);
    let filename = format!(".oxideterm-{}.part", uuid::Uuid::new_v4());
    parent.map_or(filename.clone(), |parent| format!("{parent}/{filename}"))
}
