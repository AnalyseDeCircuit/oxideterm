use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::{
    error::Error,
    fs::{File, Metadata, ReadDir},
    rawsession::{Limits, SftpResult},
    RawSftpSession,
};
use crate::{
    client::Config,
    extensions::{self, Statvfs},
    protocol::{FileAttributes, OpenFlags, StatusCode},
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Features {
    pub hardlink: bool,
    pub fsync: bool,
    pub statvfs: bool,
    pub limits: Option<Limits>,
    pub max_concurrent_writes: usize,
    pub max_packet_len: u32,
}

/// High-level SFTP implementation for easy interaction with a remote file system.
/// Contains most methods similar to the native [filesystem](std::fs)
pub struct SftpSession {
    session: Arc<RawSftpSession>,
    features: Features,
}

impl SftpSession {
    /// Creates a new session by initializing the protocol and extensions
    pub async fn new<S>(stream: S) -> SftpResult<Self>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        Self::new_with_config(stream, Config::default()).await
    }

    /// Creates a new session with timeout opt before the first request
    #[deprecated(note = "use SftpSession::new_with_config with Config::req_timeout_secs instead")]
    pub async fn new_opts<S>(stream: S, timeout: Option<u64>) -> SftpResult<Self>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let mut cfg = Config::default();
        if let Some(secs) = timeout {
            cfg.request_timeout_secs = secs;
        }
        Self::new_with_config(stream, cfg).await
    }

    /// Creates a new session with custom configuration
    pub async fn new_with_config<S>(stream: S, cfg: Config) -> SftpResult<Self>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let max_concurrent_writes = cfg.max_concurrent_writes;
        let max_packet_len = cfg.max_packet_len;
        let mut session = RawSftpSession::new_with_config(stream, cfg);

        let version = session.init().await?;
        let has_extension = |name, ver| version.extensions.get(name).is_some_and(|v| v == ver);

        let mut features = Features {
            hardlink: has_extension(extensions::HARDLINK, "1"),
            fsync: has_extension(extensions::FSYNC, "1"),
            statvfs: has_extension(extensions::STATVFS, "2"),
            limits: None,
            max_concurrent_writes,
            max_packet_len,
        };

        if has_extension(extensions::LIMITS, "1") {
            let limits = Limits::from(session.limits().await?);
            session.set_limits(limits);
            features.limits = Some(limits);
            if let Some(plen) = limits.packet_len {
                features.max_packet_len = (plen as u32).min(max_packet_len);
            }
        }

        Ok(Self {
            session: Arc::new(session),
            features,
        })
    }

    /// Set the maximum response time in seconds.
    /// Default: 10 seconds
    pub fn set_timeout(&self, secs: u64) {
        self.session.set_timeout(secs);
    }

    /// Closes the inner channel stream.
    pub async fn close(&self) -> SftpResult<()> {
        self.session.close_session()
    }

    /// Attempts to open a file in read-only mode.
    pub async fn open<T: Into<String>>(&self, filename: T) -> SftpResult<File> {
        self.open_with_flags(filename, OpenFlags::READ).await
    }

    /// Opens a file in write-only mode.
    ///
    /// This function will create a file if it does not exist, and will truncate it if it does.
    pub async fn create<T: Into<String>>(&self, filename: T) -> SftpResult<File> {
        self.open_with_flags(
            filename,
            OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
        )
        .await
    }

    /// Attempts to open or create the file in the specified mode
    pub async fn open_with_flags<T: Into<String>>(
        &self,
        filename: T,
        flags: OpenFlags,
    ) -> SftpResult<File> {
        self.open_with_flags_and_attributes(filename, flags, FileAttributes::empty())
            .await
    }

    /// Attempts to open or create the file in the specified mode and with specified file attributes
    pub async fn open_with_flags_and_attributes<T: Into<String>>(
        &self,
        filename: T,
        flags: OpenFlags,
        attributes: FileAttributes,
    ) -> SftpResult<File> {
        let handle = self.session.open(filename, flags, attributes).await?.handle;
        Ok(File::new(self.session.clone(), handle, self.features))
    }

    /// Requests the remote party for the absolute from the relative path.
    pub async fn canonicalize<T: Into<String>>(&self, path: T) -> SftpResult<String> {
        let name = self.session.realpath(path).await?;
        match name.files.first() {
            Some(file) => Ok(file.filename.to_owned()),
            None => Err(Error::UnexpectedBehavior("no file".to_owned())),
        }
    }

    /// Creates a new empty directory.
    pub async fn create_dir<T: Into<String>>(&self, path: T) -> SftpResult<()> {
        self.session
            .mkdir(path, FileAttributes::empty())
            .await
            .map(|_| ())
    }

    /// Reads the contents of a file located at the specified path to the end.
    pub async fn read<P: Into<String>>(&self, path: P) -> SftpResult<Vec<u8>> {
        let mut file = self.open(path).await?;
        let mut buffer = Vec::new();

        file.read_to_end(&mut buffer).await?;

        Ok(buffer)
    }

    /// Writes the contents to a file whose path is specified.
    pub async fn write<P: Into<String>>(&self, path: P, data: &[u8]) -> SftpResult<()> {
        let mut file = self.open_with_flags(path, OpenFlags::WRITE).await?;
        file.write_all(data).await?;
        Ok(())
    }

    /// Checks a file or folder exists at the specified path
    pub async fn try_exists<P: Into<String>>(&self, path: P) -> SftpResult<bool> {
        match self.metadata(path).await {
            Ok(_) => Ok(true),
            Err(Error::Status(status)) if status.status_code == StatusCode::NoSuchFile => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Returns an iterator over the entries within a directory.
    pub async fn read_dir<P: Into<String>>(&self, path: P) -> SftpResult<ReadDir> {
        let mut files = vec![];
        let handle = self.session.opendir(path).await?.handle;

        loop {
            match self.session.readdir(handle.as_str()).await {
                Ok(name) => {
                    files = name
                        .files
                        .into_iter()
                        .map(|f| (f.filename, f.attrs))
                        .chain(files.into_iter())
                        .collect();
                }
                Err(Error::Status(status)) if status.status_code == StatusCode::Eof => break,
                Err(err) => return Err(err),
            }
        }

        self.session.close(handle).await?;

        Ok(ReadDir {
            entries: files.into(),
        })
    }

    /// Reads a symbolic link, returning the file that the link points to.
    pub async fn read_link<P: Into<String>>(&self, path: P) -> SftpResult<String> {
        let name = self.session.readlink(path).await?;
        match name.files.first() {
            Some(file) => Ok(file.filename.to_owned()),
            None => Err(Error::UnexpectedBehavior("no file".to_owned())),
        }
    }

    /// Removes the specified folder.
    pub async fn remove_dir<P: Into<String>>(&self, path: P) -> SftpResult<()> {
        self.session.rmdir(path).await.map(|_| ())
    }

    /// Removes the specified file.
    pub async fn remove_file<T: Into<String>>(&self, filename: T) -> SftpResult<()> {
        self.session.remove(filename).await.map(|_| ())
    }

    /// Rename a file or directory to a new name.
    pub async fn rename<O, N>(&self, oldpath: O, newpath: N) -> SftpResult<()>
    where
        O: Into<String>,
        N: Into<String>,
    {
        self.session.rename(oldpath, newpath).await.map(|_| ())
    }

    /// Creates a symlink of the specified target.
    pub async fn symlink<P, T>(&self, path: P, target: T) -> SftpResult<()>
    where
        P: Into<String>,
        T: Into<String>,
    {
        self.session.symlink(path, target).await.map(|_| ())
    }

    /// Queries metadata about the remote file.
    pub async fn metadata<P: Into<String>>(&self, path: P) -> SftpResult<Metadata> {
        Ok(self.session.stat(path).await?.attrs)
    }

    /// Sets metadata for a remote file.
    pub async fn set_metadata<P: Into<String>>(
        &self,
        path: P,
        metadata: Metadata,
    ) -> Result<(), Error> {
        self.session.setstat(path, metadata).await.map(|_| ())
    }

    pub async fn symlink_metadata<P: Into<String>>(&self, path: P) -> SftpResult<Metadata> {
        Ok(self.session.lstat(path).await?.attrs)
    }

    pub async fn hardlink<O, N>(&self, oldpath: O, newpath: N) -> SftpResult<bool>
    where
        O: Into<String>,
        N: Into<String>,
    {
        if !self.features.hardlink {
            return Ok(false);
        }

        self.session.hardlink(oldpath, newpath).await.map(|_| true)
    }

    /// Performs a statvfs on the remote file system path.
    /// Returns [`Ok(None)`] if the remote SFTP server does not support `statvfs@openssh.com` extension v2.
    pub async fn fs_info<P: Into<String>>(&self, path: P) -> SftpResult<Option<Statvfs>> {
        if !self.features.statvfs {
            return Ok(None);
        }

        self.session.statvfs(path).await.map(Some)
    }
}
