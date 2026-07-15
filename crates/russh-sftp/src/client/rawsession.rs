use bytes::{BufMut, Bytes, BytesMut};
use dashmap::DashMap as HashMap;
use std::{
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncWrite};

use super::{
    error::Error,
    runtime,
    transport::{
        run_owned_session, run_session, PendingRequest, SessionTransport, SharedRequests,
        TryQueueError,
    },
    Handler, OwnedSftpWriter,
};
use crate::{
    client::Config,
    de,
    extensions::{
        self, FsyncExtension, HardlinkExtension, LimitsExtension, Statvfs, StatvfsExtension,
    },
    protocol::{
        Attrs, Close, Data, Extended, ExtendedReply, FSetStat, FileAttributes, Fstat, Handle, Init,
        Lstat, MkDir, Name, Open, OpenDir, OpenFlags, Packet, Read, ReadDir, ReadLink, RealPath,
        Remove, Rename, RmDir, SetStat, Stat, Status, StatusCode, Symlink, Version, Write,
        SSH_FXP_READ, SSH_FXP_WRITE,
    },
};

pub type SftpResult<T> = Result<T, Error>;

pub(crate) struct SessionInner {
    version: Option<u32>,
    requests: Arc<SharedRequests>,
}

impl SessionInner {
    pub fn reply(&mut self, id: Option<u32>, packet: Packet) -> SftpResult<()> {
        if let Some((_, request)) = self.requests.remove(&id) {
            let validate = if id.is_some() && self.version.is_none() {
                Err(Error::UnexpectedPacket)
            } else if id.is_none() && self.version.is_some() {
                Err(Error::UnexpectedBehavior("Duplicate version".to_owned()))
            } else {
                Ok(())
            };

            request.lifecycle.mark_acknowledged();
            // Ignore send error: receiver was dropped (request timed out).
            let _ = request.response.send(validate.clone().map(|_| packet));

            return validate;
        }

        Err(Error::UnexpectedBehavior(format!(
            "Packet {:?} for unknown recipient",
            id
        )))
    }
}

impl Handler for SessionInner {
    type Error = Error;

    async fn version(&mut self, packet: Version) -> Result<(), Self::Error> {
        let version = packet.version;
        self.reply(None, packet.into())?;
        self.version = Some(version);
        Ok(())
    }

    async fn name(&mut self, name: Name) -> Result<(), Self::Error> {
        self.reply(Some(name.id), name.into())
    }

    async fn status(&mut self, status: Status) -> Result<(), Self::Error> {
        self.reply(Some(status.id), status.into())
    }

    async fn handle(&mut self, handle: Handle) -> Result<(), Self::Error> {
        self.reply(Some(handle.id), handle.into())
    }

    async fn data(&mut self, data: Data) -> Result<(), Self::Error> {
        self.reply(Some(data.id), data.into())
    }

    async fn attrs(&mut self, attrs: Attrs) -> Result<(), Self::Error> {
        self.reply(Some(attrs.id), attrs.into())
    }

    async fn extended_reply(&mut self, reply: ExtendedReply) -> Result<(), Self::Error> {
        self.reply(Some(reply.id), reply.into())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Limits {
    pub packet_len: Option<u64>,
    pub read_len: Option<u64>,
    pub write_len: Option<u64>,
    pub open_handles: Option<u64>,
}

impl From<LimitsExtension> for Limits {
    fn from(limits: LimitsExtension) -> Self {
        Self {
            packet_len: (limits.max_packet_len > 0).then_some(limits.max_packet_len),
            read_len: (limits.max_read_len > 0).then_some(limits.max_read_len),
            write_len: (limits.max_write_len > 0).then_some(limits.max_write_len),
            open_handles: (limits.max_open_handles > 0).then_some(limits.max_open_handles),
        }
    }
}

/// Implements raw work with the protocol in request-response format.
/// If the server returns a `Status` packet and it has the code Ok
/// then the packet is returned as Ok in other error cases
/// the packet is stored as Err.
pub struct RawSftpSession {
    transport: SessionTransport,
    next_req_id: AtomicU32,
    handles: AtomicU64,
    timeout: AtomicU64,
    limits: Limits,
}

macro_rules! into_with_status {
    ($result:ident, $packet:ident) => {
        match $result {
            Packet::$packet(p) => Ok(p),
            Packet::Status(p) => Err(p.into()),
            _ => Err(Error::UnexpectedPacket),
        }
    };
}

macro_rules! into_status {
    ($result:ident) => {
        match $result {
            Packet::Status(status) if status.status_code == StatusCode::Ok => Ok(status),
            Packet::Status(status) => Err(status.into()),
            _ => Err(Error::UnexpectedPacket),
        }
    };
}

fn checked_packet_len(parts: &[usize]) -> SftpResult<u32> {
    let mut len = 0usize;
    for part in parts {
        len = len
            .checked_add(*part)
            .ok_or_else(|| Error::Limited("sftp packet length overflow".to_owned()))?;
    }
    u32::try_from(len).map_err(|_| Error::Limited("sftp packet too large".to_owned()))
}

struct WritePacketLayout {
    handle_len: u32,
    data_len: u32,
    packet_len: u32,
    frame_len: usize,
}

fn write_packet_layout(handle: &str, data_len: usize) -> SftpResult<WritePacketLayout> {
    let handle_len =
        u32::try_from(handle.len()).map_err(|_| Error::Limited("handle too large".to_owned()))?;
    let encoded_data_len =
        u32::try_from(data_len).map_err(|_| Error::Limited("write data too large".to_owned()))?;
    let packet_len = checked_packet_len(&[
        1,
        std::mem::size_of::<u32>(),
        std::mem::size_of::<u32>(),
        handle.len(),
        std::mem::size_of::<u64>(),
        std::mem::size_of::<u32>(),
        data_len,
    ])?;
    let frame_len = std::mem::size_of::<u32>()
        .checked_add(packet_len as usize)
        .ok_or_else(|| Error::Limited("sftp frame length overflow".to_owned()))?;

    Ok(WritePacketLayout {
        handle_len,
        data_len: encoded_data_len,
        packet_len,
        frame_len,
    })
}

fn encode_write_packet(id: u32, handle: &str, offset: u64, data: &[u8]) -> SftpResult<Bytes> {
    let layout = write_packet_layout(handle, data.len())?;

    // SFTP frames are length-prefixed outside the packet body. Building the
    // WRITE frame directly avoids allocating an intermediate Write payload.
    let mut bytes = BytesMut::with_capacity(layout.frame_len);
    bytes.put_u32(layout.packet_len);
    bytes.put_u8(SSH_FXP_WRITE);
    bytes.put_u32(id);
    bytes.put_u32(layout.handle_len);
    bytes.put_slice(handle.as_bytes());
    bytes.put_u64(offset);
    bytes.put_u32(layout.data_len);
    bytes.put_slice(data);
    Ok(bytes.freeze())
}

fn encode_read_packet(id: u32, handle: &str, offset: u64, len: u32) -> SftpResult<Bytes> {
    let handle_len =
        u32::try_from(handle.len()).map_err(|_| Error::Limited("handle too large".to_owned()))?;
    let packet_len = checked_packet_len(&[
        1,
        std::mem::size_of::<u32>(),
        std::mem::size_of::<u32>(),
        handle.len(),
        std::mem::size_of::<u64>(),
        std::mem::size_of::<u32>(),
    ])?;

    // Bulk downloads schedule many READ requests. Encoding the frame directly
    // avoids cloning the handle and building a generic packet payload each time.
    let mut bytes = BytesMut::with_capacity(std::mem::size_of::<u32>() + packet_len as usize);
    bytes.put_u32(packet_len);
    bytes.put_u8(SSH_FXP_READ);
    bytes.put_u32(id);
    bytes.put_u32(handle_len);
    bytes.put_slice(handle.as_bytes());
    bytes.put_u64(offset);
    bytes.put_u32(len);
    Ok(bytes.freeze())
}

impl RawSftpSession {
    pub fn new<S>(stream: S) -> Self
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        Self::new_with_config(stream, Config::default())
    }

    pub fn new_with_config<S>(stream: S, cfg: Config) -> Self
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let req_map = Arc::new(HashMap::new());
        let inner = SessionInner {
            version: None,
            requests: req_map.clone(),
        };
        let transport = run_session(stream, inner, req_map, cfg.max_outbound_inflight_bytes);

        Self {
            transport,
            next_req_id: AtomicU32::new(1),
            handles: AtomicU64::new(0),
            timeout: AtomicU64::new(cfg.request_timeout_secs),
            limits: Limits::default(),
        }
    }

    pub fn new_owned<R, W>(reader: R, writer: W) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: OwnedSftpWriter,
    {
        Self::new_owned_with_config(reader, writer, Config::default())
    }

    pub fn new_owned_with_config<R, W>(reader: R, writer: W, cfg: Config) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: OwnedSftpWriter,
    {
        let req_map = Arc::new(HashMap::new());
        let inner = SessionInner {
            version: None,
            requests: req_map.clone(),
        };
        let transport = run_owned_session(
            reader,
            writer,
            inner,
            req_map,
            cfg.max_outbound_inflight_bytes,
        );

        Self {
            transport,
            next_req_id: AtomicU32::new(1),
            handles: AtomicU64::new(0),
            timeout: AtomicU64::new(cfg.request_timeout_secs),
            limits: Limits::default(),
        }
    }

    /// Set the maximum response time in seconds.
    /// Default: 10 seconds
    pub fn set_timeout(&self, secs: u64) {
        self.timeout.store(secs, Ordering::Relaxed);
    }

    /// Setting limits. For the `limits@openssh.com` extension
    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    async fn send(&self, id: Option<u32>, packet: Packet) -> SftpResult<PendingRequest> {
        let bytes = Bytes::try_from(packet)?;
        self.send_encoded(id, bytes).await
    }

    async fn send_encoded(&self, id: Option<u32>, bytes: Bytes) -> SftpResult<PendingRequest> {
        if let Some(max_len) = self.limits.packet_len {
            if bytes.len() as u64 > max_len {
                return Err(Error::Limited("packet exceeds server limit".to_owned()));
            }
        }
        self.transport.queue(id, bytes).await
    }

    async fn request(&self, id: Option<u32>, packet: Packet) -> SftpResult<Packet> {
        let rx = self.send(id, packet).await?;
        let timeout = self.timeout.load(Ordering::Relaxed);

        match runtime::timeout(Duration::from_secs(timeout), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(Error::UnexpectedBehavior("sender dropped".into())),
            Err(error) => {
                // A timed-out sent request has an unknown remote outcome. End
                // this live SFTP session so its waiter and byte permit cannot
                // permanently consume the shared in-flight budget.
                self.transport.close();
                Err(error)
            }
        }
    }

    fn use_next_id(&self) -> u32 {
        self.next_req_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Closes the inner channel stream. Called by [`Drop`]
    pub fn close_session(&self) -> SftpResult<()> {
        self.transport.close();
        Ok(())
    }

    pub async fn init(&self) -> SftpResult<Version> {
        let result = self.request(None, Init::default().into()).await?;
        if let Packet::Version(version) = result {
            Ok(version)
        } else {
            Err(Error::UnexpectedPacket)
        }
    }

    pub async fn open<T: Into<String>>(
        &self,
        filename: T,
        flags: OpenFlags,
        attrs: FileAttributes,
    ) -> SftpResult<Handle> {
        if self
            .limits
            .open_handles
            .is_some_and(|h| self.handles.load(Ordering::SeqCst) >= h)
        {
            return Err(Error::Limited("handle limit reached".to_owned()));
        }

        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                Open {
                    id,
                    filename: filename.into(),
                    pflags: flags,
                    attrs,
                }
                .into(),
            )
            .await?;

        if let Packet::Handle(_) = result {
            self.handles.fetch_add(1, Ordering::SeqCst);
        }

        into_with_status!(result, Handle)
    }

    pub async fn close<H: Into<String>>(&self, handle: H) -> SftpResult<Status> {
        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                Close {
                    id,
                    handle: handle.into(),
                }
                .into(),
            )
            .await?;

        if let Packet::Status(status) = &result {
            if status.status_code == StatusCode::Ok
                && self
                    .handles
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |h| {
                        if h > 0 {
                            Some(h - 1)
                        } else {
                            None
                        }
                    })
                    .is_err()
            {
                warn!("attempt to close more handles than exist");
            }
        }

        into_status!(result)
    }

    /// Sends a close packet without awaiting the server's acknowledgement.
    pub(crate) fn close_nowait(&self, handle: String) -> SftpResult<()> {
        let id = self.use_next_id();
        let bytes = Bytes::try_from(Packet::Close(Close { id, handle }))?;
        let retry_bytes = bytes.clone();
        match self.transport.try_queue(Some(id), bytes) {
            Ok(pending) => {
                // File destructors cannot await queue capacity. Detach a
                // successfully queued CLOSE so dropping the local handle does
                // not cancel it before the session writer can send it.
                pending.detach();
                Ok(())
            }
            Err(TryQueueError::Full { .. }) => {
                let transport = self.transport.clone();
                runtime::spawn(async move {
                    // The close remains owned by this live session while it
                    // waits for existing sent writes to release byte budget.
                    if let Ok(pending) = transport.queue(Some(id), retry_bytes).await {
                        pending.detach();
                    }
                });
                Ok(())
            }
            Err(TryQueueError::Sftp(error)) => Err(error),
        }
    }

    pub async fn read<H: Into<String>>(
        &self,
        handle: H,
        offset: u64,
        len: u32,
    ) -> SftpResult<Data> {
        if self.limits.read_len.is_some_and(|r| len as u64 > r) {
            return Err(Error::Limited("read limit reached".to_owned()));
        }

        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                Read {
                    id,
                    handle: handle.into(),
                    offset,
                    len,
                }
                .into(),
            )
            .await?;

        into_with_status!(result, Data)
    }

    /// Sends a read packet without awaiting the server's response.
    pub(crate) async fn read_nowait_raw(
        &self,
        handle: &str,
        offset: u64,
        len: u32,
    ) -> SftpResult<PendingRequest> {
        if self.limits.read_len.is_some_and(|r| len as u64 > r) {
            return Err(Error::Limited("read limit reached".to_owned()));
        }

        let id = self.use_next_id();
        let bytes = encode_read_packet(id, handle, offset, len)?;
        self.send_encoded(Some(id), bytes).await
    }

    pub async fn write<H: Into<String>>(
        &self,
        handle: H,
        offset: u64,
        data: Vec<u8>,
    ) -> SftpResult<Status> {
        if self.limits.write_len.is_some_and(|w| data.len() as u64 > w) {
            return Err(Error::Limited("write limit reached".to_owned()));
        }

        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                Write {
                    id,
                    handle: handle.into(),
                    offset,
                    data,
                }
                .into(),
            )
            .await?;

        into_status!(result)
    }

    /// Sends a raw write packet without routing bulk data through the generic
    /// packet serializer. This keeps the upload hot path to one data copy into
    /// the final outgoing SFTP frame.
    pub(crate) async fn write_nowait_raw(
        &self,
        handle: &str,
        offset: u64,
        data: &[u8],
    ) -> SftpResult<PendingRequest> {
        if self.limits.write_len.is_some_and(|w| data.len() as u64 > w) {
            return Err(Error::Limited("write limit reached".to_owned()));
        }

        let layout = write_packet_layout(handle, data.len())?;
        if self
            .limits
            .packet_len
            .is_some_and(|max_len| layout.frame_len as u64 > max_len)
        {
            return Err(Error::Limited("packet exceeds server limit".to_owned()));
        }

        // Reserve the live-session byte budget before copying upload data into
        // its final SFTP frame, so capacity waiters cannot accumulate frames
        // outside the bounded queue.
        let reservation = self.transport.reserve(layout.frame_len).await?;
        let id = self.use_next_id();
        let bytes = encode_write_packet(id, handle, offset, data)?;
        self.transport
            .queue_reserved(Some(id), bytes, reservation)
            .await
    }

    pub(crate) fn try_write_nowait_raw(
        &self,
        handle: &str,
        offset: u64,
        data: &[u8],
    ) -> Result<PendingRequest, TryQueueError> {
        if self.limits.write_len.is_some_and(|w| data.len() as u64 > w) {
            return Err(Error::Limited("write limit reached".to_owned()).into());
        }

        let layout = write_packet_layout(handle, data.len())?;
        if self
            .limits
            .packet_len
            .is_some_and(|max_len| layout.frame_len as u64 > max_len)
        {
            return Err(Error::Limited("packet exceeds server limit".to_owned()).into());
        }

        let reservation = self.transport.try_reserve(layout.frame_len)?;
        let id = self.use_next_id();
        let bytes = encode_write_packet(id, handle, offset, data)?;
        self.transport
            .try_queue_reserved(Some(id), bytes, reservation)
    }

    pub(crate) fn register_outbound_capacity_waker(
        &self,
        required_bytes: usize,
        cx: &std::task::Context<'_>,
    ) {
        self.transport.register_capacity_waker(required_bytes, cx);
    }

    pub async fn lstat<P: Into<String>>(&self, path: P) -> SftpResult<Attrs> {
        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                Lstat {
                    id,
                    path: path.into(),
                }
                .into(),
            )
            .await?;

        into_with_status!(result, Attrs)
    }

    pub async fn fstat<H: Into<String>>(&self, handle: H) -> SftpResult<Attrs> {
        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                Fstat {
                    id,
                    handle: handle.into(),
                }
                .into(),
            )
            .await?;

        into_with_status!(result, Attrs)
    }

    pub async fn setstat<P: Into<String>>(
        &self,
        path: P,
        attrs: FileAttributes,
    ) -> SftpResult<Status> {
        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                SetStat {
                    id,
                    path: path.into(),
                    attrs,
                }
                .into(),
            )
            .await?;

        into_status!(result)
    }

    pub async fn fsetstat<H: Into<String>>(
        &self,
        handle: H,
        attrs: FileAttributes,
    ) -> SftpResult<Status> {
        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                FSetStat {
                    id,
                    handle: handle.into(),
                    attrs,
                }
                .into(),
            )
            .await?;

        into_status!(result)
    }

    pub async fn opendir<P: Into<String>>(&self, path: P) -> SftpResult<Handle> {
        if self
            .limits
            .open_handles
            .is_some_and(|h| self.handles.load(Ordering::SeqCst) >= h)
        {
            return Err(Error::Limited("Handle limit reached".to_owned()));
        }

        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                OpenDir {
                    id,
                    path: path.into(),
                }
                .into(),
            )
            .await?;

        if let Packet::Handle(_) = result {
            self.handles.fetch_add(1, Ordering::SeqCst);
        }

        into_with_status!(result, Handle)
    }

    pub async fn readdir<H: Into<String>>(&self, handle: H) -> SftpResult<Name> {
        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                ReadDir {
                    id,
                    handle: handle.into(),
                }
                .into(),
            )
            .await?;

        into_with_status!(result, Name)
    }

    pub async fn remove<T: Into<String>>(&self, filename: T) -> SftpResult<Status> {
        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                Remove {
                    id,
                    filename: filename.into(),
                }
                .into(),
            )
            .await?;

        into_status!(result)
    }

    pub async fn mkdir<P: Into<String>>(
        &self,
        path: P,
        attrs: FileAttributes,
    ) -> SftpResult<Status> {
        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                MkDir {
                    id,
                    path: path.into(),
                    attrs,
                }
                .into(),
            )
            .await?;

        into_status!(result)
    }

    pub async fn rmdir<P: Into<String>>(&self, path: P) -> SftpResult<Status> {
        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                RmDir {
                    id,
                    path: path.into(),
                }
                .into(),
            )
            .await?;

        into_status!(result)
    }

    pub async fn realpath<P: Into<String>>(&self, path: P) -> SftpResult<Name> {
        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                RealPath {
                    id,
                    path: path.into(),
                }
                .into(),
            )
            .await?;

        into_with_status!(result, Name)
    }

    pub async fn stat<P: Into<String>>(&self, path: P) -> SftpResult<Attrs> {
        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                Stat {
                    id,
                    path: path.into(),
                }
                .into(),
            )
            .await?;

        into_with_status!(result, Attrs)
    }

    pub async fn rename<O, N>(&self, oldpath: O, newpath: N) -> SftpResult<Status>
    where
        O: Into<String>,
        N: Into<String>,
    {
        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                Rename {
                    id,
                    oldpath: oldpath.into(),
                    newpath: newpath.into(),
                }
                .into(),
            )
            .await?;

        into_status!(result)
    }

    pub async fn readlink<P: Into<String>>(&self, path: P) -> SftpResult<Name> {
        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                ReadLink {
                    id,
                    path: path.into(),
                }
                .into(),
            )
            .await?;

        into_with_status!(result, Name)
    }

    pub async fn symlink<P, T>(&self, path: P, target: T) -> SftpResult<Status>
    where
        P: Into<String>,
        T: Into<String>,
    {
        let id = self.use_next_id();
        let result = self
            .request(
                Some(id),
                Symlink {
                    id,
                    linkpath: path.into(),
                    targetpath: target.into(),
                }
                .into(),
            )
            .await?;

        into_status!(result)
    }

    /// Equivalent to `SSH_FXP_EXTENDED`. Allows protocol expansion.
    /// The extension can return any packet, so it's not specific
    pub async fn extended<R: Into<String>>(&self, request: R, data: Vec<u8>) -> SftpResult<Packet> {
        let id = self.use_next_id();
        self.request(
            Some(id),
            Extended {
                id,
                request: request.into(),
                data,
            }
            .into(),
        )
        .await
    }

    pub async fn limits(&self) -> SftpResult<LimitsExtension> {
        match self.extended(extensions::LIMITS, vec![]).await? {
            Packet::ExtendedReply(reply) => {
                Ok(de::from_bytes::<LimitsExtension>(&mut reply.data.into())?)
            }
            Packet::Status(status) if status.status_code != StatusCode::Ok => {
                Err(Error::Status(status))
            }
            _ => Err(Error::UnexpectedPacket),
        }
    }

    pub async fn hardlink<O, N>(&self, oldpath: O, newpath: N) -> SftpResult<Status>
    where
        O: Into<String>,
        N: Into<String>,
    {
        let result = self
            .extended(
                extensions::HARDLINK,
                HardlinkExtension {
                    oldpath: oldpath.into(),
                    newpath: newpath.into(),
                }
                .try_into()?,
            )
            .await?;

        into_status!(result)
    }

    pub async fn fsync<H: Into<String>>(&self, handle: H) -> SftpResult<Status> {
        let result = self
            .extended(
                extensions::FSYNC,
                FsyncExtension {
                    handle: handle.into(),
                }
                .try_into()?,
            )
            .await?;

        into_status!(result)
    }

    pub async fn statvfs<P>(&self, path: P) -> SftpResult<Statvfs>
    where
        P: Into<String>,
    {
        let result = self
            .extended(
                extensions::STATVFS,
                StatvfsExtension { path: path.into() }.try_into()?,
            )
            .await?;

        match result {
            Packet::ExtendedReply(reply) => Ok(de::from_bytes::<Statvfs>(&mut reply.data.into())?),
            Packet::Status(status) if status.status_code != StatusCode::Ok => {
                Err(Error::Status(status))
            }
            _ => Err(Error::UnexpectedPacket),
        }
    }
}

impl Drop for RawSftpSession {
    fn drop(&mut self) {
        let _ = self.close_session();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    #[test]
    fn raw_write_packet_matches_generic_packet_encoding() {
        let id = 42;
        let handle = "remote-handle";
        let offset = 9_876;
        let data = b"upload-payload";

        let raw = encode_write_packet(id, handle, offset, data).expect("raw write packet encodes");
        let generic = Bytes::try_from(Packet::Write(Write {
            id,
            handle: handle.to_owned(),
            offset,
            data: data.to_vec(),
        }))
        .expect("generic write packet encodes");

        assert_eq!(raw, generic);
    }

    #[test]
    fn raw_read_packet_matches_generic_packet_encoding() {
        let id = 17;
        let handle = "remote-handle";
        let offset = 1_024;
        let len = 131_072;

        let raw = encode_read_packet(id, handle, offset, len).expect("raw read packet encodes");
        let generic = Bytes::try_from(Packet::Read(Read {
            id,
            handle: handle.to_owned(),
            offset,
            len,
        }))
        .expect("generic read packet encodes");

        assert_eq!(raw, generic);
    }

    #[tokio::test]
    async fn raw_write_waits_for_budget_before_allocating_request_id() {
        const SESSION_BUDGET_BYTES: usize = 128;
        let (client_stream, _server_stream) = tokio::io::duplex(256);
        let session = RawSftpSession::new_with_config(
            client_stream,
            Config {
                max_outbound_inflight_bytes: SESSION_BUDGET_BYTES,
                ..Config::default()
            },
        );
        let reservation = session
            .transport
            .reserve(SESSION_BUDGET_BYTES)
            .await
            .expect("test reserves the complete session budget");
        let next_request_id = session.next_req_id.load(Ordering::Relaxed);

        let result = session.try_write_nowait_raw("handle", 0, b"payload");

        assert!(matches!(result, Err(TryQueueError::Full { .. })));
        assert_eq!(session.next_req_id.load(Ordering::Relaxed), next_request_id);
        drop(reservation);
    }

    #[tokio::test]
    async fn close_waits_for_session_budget_instead_of_being_dropped() {
        const SESSION_BUDGET_BYTES: usize = 128;
        let (client_stream, mut server_stream) = tokio::io::duplex(256);
        let session = RawSftpSession::new_with_config(
            client_stream,
            Config {
                max_outbound_inflight_bytes: SESSION_BUDGET_BYTES,
                ..Config::default()
            },
        );
        let reservation = session
            .transport
            .reserve(SESSION_BUDGET_BYTES)
            .await
            .expect("test reserves the complete session budget");

        session
            .close_nowait("handle".to_owned())
            .expect("close is retained by the live session");
        assert!(
            tokio::time::timeout(Duration::from_millis(10), server_stream.read_u32())
                .await
                .is_err()
        );

        drop(reservation);
        let packet_len = tokio::time::timeout(Duration::from_millis(100), server_stream.read_u32())
            .await
            .expect("close sends after budget is released")
            .expect("close frame length is readable");
        let mut body = vec![0; packet_len as usize];
        server_stream
            .read_exact(&mut body)
            .await
            .expect("close frame body is readable");
        let mut body = Bytes::from(body);
        assert!(matches!(Packet::try_from(&mut body), Ok(Packet::Close(_))));
    }
}
