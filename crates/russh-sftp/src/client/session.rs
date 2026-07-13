use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::{
    error::Error,
    fs::{File, Metadata, ReadDir},
    rawsession::{Limits, SftpResult},
    OwnedSftpWriter, RawSftpSession,
};
use crate::{
    client::Config,
    extensions::{self, Statvfs},
    protocol::{File as SftpNameFile, FileAttributes, OpenFlags, StatusCode},
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

    /// Creates a new session with custom configuration
    pub async fn new_with_config<S>(stream: S, cfg: Config) -> SftpResult<Self>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let max_concurrent_writes = cfg.max_concurrent_writes;
        let max_packet_len = cfg.max_packet_len;
        let session = RawSftpSession::new_with_config(stream, cfg);
        Self::initialize(session, max_concurrent_writes, max_packet_len).await
    }

    /// Creates a new session over separate owned reader and writer transports.
    pub async fn new_owned<R, W>(reader: R, writer: W) -> SftpResult<Self>
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: OwnedSftpWriter,
    {
        Self::new_owned_with_config(reader, writer, Config::default()).await
    }

    /// Creates a new owned-transport session with custom configuration.
    pub async fn new_owned_with_config<R, W>(reader: R, writer: W, cfg: Config) -> SftpResult<Self>
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: OwnedSftpWriter,
    {
        let max_concurrent_writes = cfg.max_concurrent_writes;
        let max_packet_len = cfg.max_packet_len;
        let session = RawSftpSession::new_owned_with_config(reader, writer, cfg);
        Self::initialize(session, max_concurrent_writes, max_packet_len).await
    }

    async fn initialize(
        mut session: RawSftpSession,
        max_concurrent_writes: usize,
        max_packet_len: u32,
    ) -> SftpResult<Self> {
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

    /// Returns server-advertised SFTP limits when the OpenSSH extension is available.
    pub fn advertised_limits(&self) -> Option<Limits> {
        self.features.limits
    }

    /// Returns the packet length cap after applying the local config and server limits.
    pub fn negotiated_packet_len(&self) -> u32 {
        self.features.max_packet_len
    }

    /// Returns the server-advertised open-handle cap when known.
    pub fn advertised_open_handle_limit(&self) -> Option<u64> {
        self.features.limits.and_then(|limits| limits.open_handles)
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
        let path: String = path.into();
        let parent = Arc::from(path.as_str());

        let handle = self.session.opendir(path).await?.handle;
        let mut files = Vec::new();

        loop {
            match self.session.readdir(handle.as_str()).await {
                Ok(name) => {
                    append_name_files(&mut files, name.files);
                }
                Err(Error::Status(status)) if status.status_code == StatusCode::Eof => break,
                Err(err) => {
                    // `read_dir` owns the directory handle once opendir
                    // succeeds. Preserve the original read error, but still
                    // ask the server to close the handle so large failed
                    // listings do not leak remote/server-side handles.
                    let _ = self.session.close(handle).await;
                    return Err(err);
                }
            }
        }

        self.session.close(handle).await?;

        Ok(ReadDir {
            parent,
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

fn append_name_files(files: &mut Vec<(String, Metadata)>, batch: Vec<SftpNameFile>) {
    // Keep the server-provided readdir order and append batches linearly.
    // Rebuilding `new_batch.chain(files).collect()` on every packet becomes
    // quadratic for large directories.
    files.extend(batch.into_iter().map(|file| (file.filename, file.attrs)));
}

#[cfg(test)]
mod tests {
    use super::*;

    impl SftpSession {
        fn for_test_with_limits(limits: Option<Limits>, max_packet_len: u32) -> Self {
            let stream = tokio::io::duplex(64).0;
            Self {
                session: Arc::new(RawSftpSession::new(stream)),
                features: Features {
                    hardlink: false,
                    fsync: false,
                    statvfs: false,
                    limits,
                    max_concurrent_writes: 8,
                    max_packet_len,
                },
            }
        }
    }

    fn named_file(filename: &str) -> SftpNameFile {
        SftpNameFile {
            filename: filename.to_owned(),
            longname: String::new(),
            attrs: FileAttributes::empty(),
        }
    }

    #[test]
    fn read_dir_accumulates_batches_in_server_order() {
        let mut files = Vec::new();

        append_name_files(&mut files, vec![named_file("a"), named_file("b")]);
        append_name_files(&mut files, vec![named_file("c")]);

        let filenames = files
            .into_iter()
            .map(|(filename, _attrs)| filename)
            .collect::<Vec<_>>();
        assert_eq!(filenames, ["a", "b", "c"]);
    }

    #[tokio::test]
    async fn session_exposes_advertised_capacity_hints() {
        let limits = Limits {
            packet_len: Some(65_536),
            read_len: Some(32_768),
            write_len: Some(32_768),
            open_handles: Some(128),
        };
        let session = SftpSession::for_test_with_limits(Some(limits), 65_536);

        let exposed = session
            .advertised_limits()
            .expect("limits should be exposed");
        assert_eq!(exposed.packet_len, Some(65_536));
        assert_eq!(exposed.read_len, Some(32_768));
        assert_eq!(exposed.write_len, Some(32_768));
        assert_eq!(exposed.open_handles, Some(128));
        assert_eq!(session.negotiated_packet_len(), 65_536);
        assert_eq!(session.advertised_open_handle_limit(), Some(128));
    }

    #[tokio::test]
    async fn session_capacity_hints_are_empty_without_limits_extension() {
        let session = SftpSession::for_test_with_limits(None, 262_144);

        assert!(session.advertised_limits().is_none());
        assert_eq!(session.negotiated_packet_len(), 262_144);
        assert_eq!(session.advertised_open_handle_limit(), None);
    }
}
