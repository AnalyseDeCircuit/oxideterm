mod attrs;
mod close;
mod data;
mod extended;
mod file;
mod file_attrs;
mod fsetstat;
mod fstat;
mod handle;
mod init;
mod lstat;
mod mkdir;
mod name;
mod open;
mod opendir;
mod read;
mod readdir;
mod readlink;
mod realpath;
mod remove;
mod rename;
mod rmdir;
mod setstat;
mod stat;
mod status;
mod symlink;
mod version;
mod write;

use bytes::{Buf, BufMut, Bytes, BytesMut};

use crate::{de, error::Error, ser};

pub use self::{
    attrs::Attrs,
    close::Close,
    data::Data,
    extended::{Extended, ExtendedReply},
    file::File,
    file_attrs::{
        FileAttr, FileAttributes, FileMode, FilePermissionFlags, FilePermissions, FileType,
    },
    fsetstat::FSetStat,
    fstat::Fstat,
    handle::Handle,
    init::Init,
    lstat::Lstat,
    mkdir::MkDir,
    name::Name,
    open::{Open, OpenFlags},
    opendir::OpenDir,
    read::Read,
    readdir::ReadDir,
    readlink::ReadLink,
    realpath::RealPath,
    remove::Remove,
    rename::Rename,
    rmdir::RmDir,
    setstat::SetStat,
    stat::Stat,
    status::{Status, StatusCode},
    symlink::Symlink,
    version::Version,
    write::Write,
};

pub const VERSION: u32 = 3;

const SSH_FXP_INIT: u8 = 1;
const SSH_FXP_VERSION: u8 = 2;
const SSH_FXP_OPEN: u8 = 3;
const SSH_FXP_CLOSE: u8 = 4;
pub(crate) const SSH_FXP_READ: u8 = 5;
pub(crate) const SSH_FXP_WRITE: u8 = 6;
const SSH_FXP_LSTAT: u8 = 7;
const SSH_FXP_FSTAT: u8 = 8;
const SSH_FXP_SETSTAT: u8 = 9;
const SSH_FXP_FSETSTAT: u8 = 10;
const SSH_FXP_OPENDIR: u8 = 11;
const SSH_FXP_READDIR: u8 = 12;
const SSH_FXP_REMOVE: u8 = 13;
const SSH_FXP_MKDIR: u8 = 14;
const SSH_FXP_RMDIR: u8 = 15;
const SSH_FXP_REALPATH: u8 = 16;
const SSH_FXP_STAT: u8 = 17;
const SSH_FXP_RENAME: u8 = 18;
const SSH_FXP_READLINK: u8 = 19;
const SSH_FXP_SYMLINK: u8 = 20;

const SSH_FXP_STATUS: u8 = 101;
const SSH_FXP_HANDLE: u8 = 102;
const SSH_FXP_DATA: u8 = 103;
const SSH_FXP_NAME: u8 = 104;
const SSH_FXP_ATTRS: u8 = 105;

const SSH_FXP_EXTENDED: u8 = 200;
const SSH_FXP_EXTENDED_REPLY: u8 = 201;

fn checked_packet_len(parts: &[usize]) -> Result<u32, Error> {
    let mut len = 0usize;
    for part in parts {
        len = len
            .checked_add(*part)
            .ok_or_else(|| Error::BadMessage("packet length overflow".to_owned()))?;
    }
    u32::try_from(len).map_err(|_| Error::BadMessage("packet too large".to_owned()))
}

fn decode_data_packet(bytes: &mut Bytes) -> Result<Data, Error> {
    let id = bytes.try_get_u32()?;
    let len = bytes.try_get_u32()? as usize;
    if bytes.remaining() < len {
        return Err(Error::BadMessage("no remaining for data".to_owned()));
    }

    // Keep SFTP DATA payload as a Bytes slice so bulk downloads can forward it
    // to callers without first copying the packet body into a Vec.
    Ok(Data {
        id,
        data: bytes.copy_to_bytes(len),
    })
}

fn encode_data_packet(data: Data) -> Result<Bytes, Error> {
    let data_len = u32::try_from(data.data.len())
        .map_err(|_| Error::BadMessage("data too large".to_owned()))?;
    let packet_len = checked_packet_len(&[
        1,
        std::mem::size_of::<u32>(),
        std::mem::size_of::<u32>(),
        data.data.len(),
    ])?;

    // Match the generic SFTP frame format while avoiding a separate DATA
    // payload allocation for Bytes-backed packet bodies.
    let mut bytes = BytesMut::with_capacity(std::mem::size_of::<u32>() + packet_len as usize);
    bytes.put_u32(packet_len);
    bytes.put_u8(SSH_FXP_DATA);
    bytes.put_u32(data.id);
    bytes.put_u32(data_len);
    bytes.put_slice(&data.data);
    Ok(bytes.freeze())
}

pub(crate) trait RequestId: Sized {
    fn get_request_id(&self) -> u32;
}

macro_rules! impl_request_id {
    ($packet:ty) => {
        impl RequestId for $packet {
            fn get_request_id(&self) -> u32 {
                self.id
            }
        }
    };
}

macro_rules! impl_packet_for {
    ($name:ident) => {
        impl From<$name> for Packet {
            fn from(input: $name) -> Self {
                Self::$name(input)
            }
        }
    };
}

pub(crate) use impl_packet_for;
pub(crate) use impl_request_id;

#[derive(Debug)]
pub enum Packet {
    Init(Init),
    Version(Version),
    Open(Open),
    Close(Close),
    Read(Read),
    Write(Write),
    Lstat(Lstat),
    Fstat(Fstat),
    SetStat(SetStat),
    FSetStat(FSetStat),
    OpenDir(OpenDir),
    ReadDir(ReadDir),
    Remove(Remove),
    MkDir(MkDir),
    RmDir(RmDir),
    RealPath(RealPath),
    Stat(Stat),
    Rename(Rename),
    ReadLink(ReadLink),
    Symlink(Symlink),
    Status(Status),
    Handle(Handle),
    Data(Data),
    Name(Name),
    Attrs(Attrs),
    Extended(Extended),
    ExtendedReply(ExtendedReply),
}

impl Packet {
    pub fn get_request_id(&self) -> u32 {
        match self {
            Self::Open(open) => open.get_request_id(),
            Self::Close(close) => close.get_request_id(),
            Self::Read(read) => read.get_request_id(),
            Self::Write(write) => write.get_request_id(),
            Self::Lstat(lstat) => lstat.get_request_id(),
            Self::Fstat(fstat) => fstat.get_request_id(),
            Self::SetStat(setstat) => setstat.get_request_id(),
            Self::FSetStat(fsetstat) => fsetstat.get_request_id(),
            Self::OpenDir(opendir) => opendir.get_request_id(),
            Self::ReadDir(readdir) => readdir.get_request_id(),
            Self::Remove(remove) => remove.get_request_id(),
            Self::MkDir(mkdir) => mkdir.get_request_id(),
            Self::RmDir(rmdir) => rmdir.get_request_id(),
            Self::RealPath(realpath) => realpath.get_request_id(),
            Self::Stat(stat) => stat.get_request_id(),
            Self::Rename(rename) => rename.get_request_id(),
            Self::ReadLink(readlink) => readlink.get_request_id(),
            Self::Symlink(symlink) => symlink.get_request_id(),
            Self::Extended(extended) => extended.get_request_id(),
            _ => 0,
        }
    }

    pub fn status(id: u32, status_code: StatusCode, msg: &str, tag: &str) -> Self {
        Packet::Status(Status {
            id,
            status_code,
            error_message: msg.to_string(),
            language_tag: tag.to_string(),
        })
    }

    pub fn error(id: u32, status_code: StatusCode) -> Self {
        Self::status(id, status_code, &status_code.to_string(), "en-US")
    }
}

impl TryFrom<&mut Bytes> for Packet {
    type Error = Error;

    fn try_from(bytes: &mut Bytes) -> Result<Self, Self::Error> {
        let r#type = bytes.try_get_u8()?;
        debug!("packet type {}", r#type);

        let request = match r#type {
            SSH_FXP_INIT => Self::Init(de::from_bytes(bytes)?),
            SSH_FXP_VERSION => Self::Version(de::from_bytes(bytes)?),
            SSH_FXP_OPEN => Self::Open(de::from_bytes(bytes)?),
            SSH_FXP_CLOSE => Self::Close(de::from_bytes(bytes)?),
            SSH_FXP_READ => Self::Read(de::from_bytes(bytes)?),
            SSH_FXP_WRITE => Self::Write(de::from_bytes(bytes)?),
            SSH_FXP_LSTAT => Self::Lstat(de::from_bytes(bytes)?),
            SSH_FXP_FSTAT => Self::Fstat(de::from_bytes(bytes)?),
            SSH_FXP_SETSTAT => Self::SetStat(de::from_bytes(bytes)?),
            SSH_FXP_FSETSTAT => Self::FSetStat(de::from_bytes(bytes)?),
            SSH_FXP_OPENDIR => Self::OpenDir(de::from_bytes(bytes)?),
            SSH_FXP_READDIR => Self::ReadDir(de::from_bytes(bytes)?),
            SSH_FXP_REMOVE => Self::Remove(de::from_bytes(bytes)?),
            SSH_FXP_MKDIR => Self::MkDir(de::from_bytes(bytes)?),
            SSH_FXP_RMDIR => Self::RmDir(de::from_bytes(bytes)?),
            SSH_FXP_REALPATH => Self::RealPath(de::from_bytes(bytes)?),
            SSH_FXP_STAT => Self::Stat(de::from_bytes(bytes)?),
            SSH_FXP_RENAME => Self::Rename(de::from_bytes(bytes)?),
            SSH_FXP_READLINK => Self::ReadLink(de::from_bytes(bytes)?),
            SSH_FXP_SYMLINK => Self::Symlink(de::from_bytes(bytes)?),
            SSH_FXP_STATUS => Self::Status(de::from_bytes(bytes)?),
            SSH_FXP_HANDLE => Self::Handle(de::from_bytes(bytes)?),
            SSH_FXP_DATA => Self::Data(decode_data_packet(bytes)?),
            SSH_FXP_NAME => Self::Name(de::from_bytes(bytes)?),
            SSH_FXP_ATTRS => Self::Attrs(de::from_bytes(bytes)?),
            SSH_FXP_EXTENDED => Self::Extended(de::from_bytes(bytes)?),
            SSH_FXP_EXTENDED_REPLY => Self::ExtendedReply(de::from_bytes(bytes)?),
            _ => return Err(Error::BadMessage("unknown type".to_owned())),
        };

        Ok(request)
    }
}

impl TryFrom<Packet> for Bytes {
    type Error = Error;

    fn try_from(packet: Packet) -> Result<Self, Self::Error> {
        let (r#type, payload): (u8, Bytes) = match packet {
            Packet::Init(init) => (SSH_FXP_INIT, ser::to_bytes(&init)?),
            Packet::Version(version) => (SSH_FXP_VERSION, ser::to_bytes(&version)?),
            Packet::Open(open) => (SSH_FXP_OPEN, ser::to_bytes(&open)?),
            Packet::Close(close) => (SSH_FXP_CLOSE, ser::to_bytes(&close)?),
            Packet::Read(read) => (SSH_FXP_READ, ser::to_bytes(&read)?),
            Packet::Write(write) => (SSH_FXP_WRITE, ser::to_bytes(&write)?),
            Packet::Lstat(stat) => (SSH_FXP_LSTAT, ser::to_bytes(&stat)?),
            Packet::Fstat(stat) => (SSH_FXP_FSTAT, ser::to_bytes(&stat)?),
            Packet::SetStat(setstat) => (SSH_FXP_SETSTAT, ser::to_bytes(&setstat)?),
            Packet::FSetStat(setstat) => (SSH_FXP_FSETSTAT, ser::to_bytes(&setstat)?),
            Packet::OpenDir(opendir) => (SSH_FXP_OPENDIR, ser::to_bytes(&opendir)?),
            Packet::ReadDir(readdir) => (SSH_FXP_READDIR, ser::to_bytes(&readdir)?),
            Packet::Remove(remove) => (SSH_FXP_REMOVE, ser::to_bytes(&remove)?),
            Packet::MkDir(mkdir) => (SSH_FXP_MKDIR, ser::to_bytes(&mkdir)?),
            Packet::RmDir(rmdir) => (SSH_FXP_RMDIR, ser::to_bytes(&rmdir)?),
            Packet::RealPath(realpath) => (SSH_FXP_REALPATH, ser::to_bytes(&realpath)?),
            Packet::Stat(stat) => (SSH_FXP_STAT, ser::to_bytes(&stat)?),
            Packet::Rename(rename) => (SSH_FXP_RENAME, ser::to_bytes(&rename)?),
            Packet::ReadLink(readlink) => (SSH_FXP_READLINK, ser::to_bytes(&readlink)?),
            Packet::Symlink(symlink) => (SSH_FXP_SYMLINK, ser::to_bytes(&symlink)?),
            Packet::Status(status) => (SSH_FXP_STATUS, ser::to_bytes(&status)?),
            Packet::Handle(handle) => (SSH_FXP_HANDLE, ser::to_bytes(&handle)?),
            Packet::Data(data) => return encode_data_packet(data),
            Packet::Name(name) => (SSH_FXP_NAME, ser::to_bytes(&name)?),
            Packet::Attrs(attrs) => (SSH_FXP_ATTRS, ser::to_bytes(&attrs)?),
            Packet::Extended(extended) => (SSH_FXP_EXTENDED, ser::to_bytes(&extended)?),
            Packet::ExtendedReply(reply) => (SSH_FXP_EXTENDED_REPLY, ser::to_bytes(&reply)?),
        };

        let length = payload.len() as u32 + 1;
        let mut bytes = BytesMut::new();
        bytes.put_u32(length);
        bytes.put_u8(r#type);
        bytes.put_slice(&payload);
        Ok(bytes.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_packet_round_trips_bytes_payload() {
        let payload = Bytes::from_static(b"download-payload");
        let encoded = Bytes::try_from(Packet::Data(Data {
            id: 7,
            data: payload.clone(),
        }))
        .expect("data packet encodes");
        let mut body = encoded.slice(4..);

        let decoded = Packet::try_from(&mut body).expect("data packet decodes");
        match decoded {
            Packet::Data(data) => {
                assert_eq!(data.id, 7);
                assert_eq!(data.data, payload);
            }
            other => panic!("unexpected packet: {other:?}"),
        }
    }
}
