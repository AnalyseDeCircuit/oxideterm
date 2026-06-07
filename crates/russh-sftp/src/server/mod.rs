mod handler;
mod reply;

use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

pub use self::handler::Handler;
pub use self::reply::StatusReply;

use crate::{
    error::Error,
    protocol::{Packet, Status, StatusCode},
    utils::read_packet,
};

macro_rules! into_wrap {
    ($id:expr, $handler:expr, $var:ident; $($arg:ident),*) => {
        match $handler.$var($($var.$arg),*).await {
            Err(err) => {
                let StatusReply { status_code, error_message, language_tag } = err.into();
                Packet::Status(Status {
                    id: $id,
                    status_code,
                    error_message: error_message.unwrap_or_else(|| status_code.to_string()),
                    language_tag: language_tag.unwrap_or_else(|| "en-US".to_string()),
                })
            },
            Ok(packet) => packet.into(),
        }
    };
}

/// Configuration for the SFTP server.
#[derive(Clone, Debug)]
pub struct Config {
    /// Maximum allowed size of SFTP packets sent by clients. Default: 256 KiB.
    pub max_client_packet_len: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_client_packet_len: 262144,
        }
    }
}

async fn process_request<H>(packet: Packet, handler: &mut H) -> Packet
where
    H: Handler + Send,
{
    let id = packet.get_request_id();

    match packet {
        Packet::Init(init) => into_wrap!(id, handler, init; version, extensions),
        Packet::Open(open) => into_wrap!(id, handler, open; id, filename, pflags, attrs),
        Packet::Close(close) => into_wrap!(id, handler, close; id, handle),
        Packet::Read(read) => into_wrap!(id, handler, read; id, handle, offset, len),
        Packet::Write(write) => into_wrap!(id, handler, write; id, handle, offset, data),
        Packet::Lstat(lstat) => into_wrap!(id, handler, lstat; id, path),
        Packet::Fstat(fstat) => into_wrap!(id, handler, fstat; id, handle),
        Packet::SetStat(setstat) => into_wrap!(id, handler, setstat; id, path, attrs),
        Packet::FSetStat(fsetstat) => into_wrap!(id, handler, fsetstat; id, handle, attrs),
        Packet::OpenDir(opendir) => into_wrap!(id, handler, opendir; id, path),
        Packet::ReadDir(readdir) => into_wrap!(id, handler, readdir; id, handle),
        Packet::Remove(remove) => into_wrap!(id, handler, remove; id, filename),
        Packet::MkDir(mkdir) => into_wrap!(id, handler, mkdir; id, path, attrs),
        Packet::RmDir(rmdir) => into_wrap!(id, handler, rmdir; id, path),
        Packet::RealPath(realpath) => into_wrap!(id, handler, realpath; id, path),
        Packet::Stat(stat) => into_wrap!(id, handler, stat; id, path),
        Packet::Rename(rename) => into_wrap!(id, handler, rename; id, oldpath, newpath),
        Packet::ReadLink(readlink) => into_wrap!(id, handler, readlink; id, path),
        Packet::Symlink(symlink) => into_wrap!(id, handler, symlink; id, linkpath, targetpath),
        Packet::Extended(extended) => into_wrap!(id, handler, extended; id, request, data),
        _ => Packet::error(0, StatusCode::BadMessage),
    }
}

async fn process_handler<H, S>(stream: &mut S, handler: &mut H, cfg: &Config) -> Result<(), Error>
where
    H: Handler + Send,
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut bytes = read_packet(stream, cfg.max_client_packet_len).await?;

    let response = match Packet::try_from(&mut bytes) {
        Ok(request) => process_request(request, handler).await,
        Err(_) => Packet::error(0, StatusCode::BadMessage),
    };

    let packet = Bytes::try_from(response)?;
    stream.write_all(&packet).await?;
    stream.flush().await?;

    Ok(())
}

/// Run processing stream as SFTP
pub async fn run<S, H>(stream: S, handler: H)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    H: Handler + Send + 'static,
{
    run_with_config(stream, handler, Config::default()).await
}

/// Run processing stream as SFTP with custom configuration
pub async fn run_with_config<S, H>(mut stream: S, mut handler: H, cfg: Config)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    H: Handler + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            match process_handler(&mut stream, &mut handler, &cfg).await {
                Err(Error::UnexpectedEof) => break,
                Err(err) => warn!("{}", err),
                Ok(_) => (),
            }
        }

        debug!("sftp stream ended");
    });
}
