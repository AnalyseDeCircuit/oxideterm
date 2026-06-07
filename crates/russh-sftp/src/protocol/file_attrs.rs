use serde::{de::Visitor, ser::SerializeStruct, Deserialize, Deserializer, Serialize};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::{
    fmt,
    fs::Metadata,
    io::ErrorKind,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::utils;

/// Attributes flags according to the specification
#[derive(Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileAttr(u32);

/// Type according to mode unix
#[derive(Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMode(u32);

/// Type describing permission flags
#[derive(Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilePermissionFlags(u32);

bitflags! {
    impl FileAttr: u32 {
        const SIZE = 0x00000001;
        const UIDGID = 0x00000002;
        const PERMISSIONS = 0x00000004;
        const ACMODTIME = 0x00000008;
        const EXTENDED = 0x80000000;
    }

    impl FileMode: u32 {
        const FIFO = 0x1000;
        const CHR = 0x2000;
        const DIR = 0x4000;
        const NAM = 0x5000;
        const BLK = 0x6000;
        const REG = 0x8000;
        const LNK = 0xA000;
        const SOCK = 0xC000;
    }

    impl FilePermissionFlags: u32 {
        const OTHER_READ = 0o4;
        const OTHER_WRITE = 0o2;
        const OTHER_EXEC = 0o1;
        const GROUP_READ = 0o40;
        const GROUP_WRITE = 0o20;
        const GROUP_EXEC = 0o10;
        const OWNER_READ = 0o400;
        const OWNER_WRITE = 0o200;
        const OWNER_EXEC = 0o100;
    }
}

/// Represents a simplified version of the [`FileMode`]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Dir,
    File,
    Symlink,
    Other,
}

impl FileType {
    /// Returns `true` if the file is a directory
    pub fn is_dir(&self) -> bool {
        matches!(self, Self::Dir)
    }

    /// Returns `true` if the file is a file
    pub fn is_file(&self) -> bool {
        matches!(self, Self::File)
    }

    /// Returns `true` if the file is a symlink
    pub fn is_symlink(&self) -> bool {
        matches!(self, Self::Symlink)
    }

    /// Returns `true` if the file has a distinctive type
    pub fn is_other(&self) -> bool {
        matches!(self, Self::Other)
    }
}

impl From<FileMode> for FileType {
    fn from(mode: FileMode) -> Self {
        if mode == FileMode::DIR {
            FileType::Dir
        } else if mode == FileMode::LNK {
            FileType::Symlink
        } else if mode == FileMode::REG {
            FileType::File
        } else {
            FileType::Other
        }
    }
}

impl From<u32> for FileType {
    fn from(mode: u32) -> Self {
        FileMode::from_bits_truncate(mode).into()
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub struct FilePermissions {
    pub other_exec: bool,
    pub other_read: bool,
    pub other_write: bool,
    pub group_exec: bool,
    pub group_read: bool,
    pub group_write: bool,
    pub owner_exec: bool,
    pub owner_read: bool,
    pub owner_write: bool,
}

impl FilePermissions {
    /// Returns `true` if the file is read-only.
    pub fn is_readonly(&self) -> bool {
        !self.other_write && !self.group_write && !self.owner_write
    }

    /// Clears all flags or sets them to a positive value. Works for unix.
    pub fn set_readonly(&mut self, readonly: bool) {
        self.other_exec = !readonly;
        self.other_write = !readonly;
        self.group_exec = !readonly;
        self.group_write = !readonly;
        self.owner_exec = !readonly;
        self.owner_write = !readonly;

        if readonly {
            self.other_read = true;
            self.group_read = true;
            self.owner_read = true;
        }
    }
}

impl fmt::Display for FilePermissions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}{}{}{}{}{}{}{}",
            if self.owner_read { "r" } else { "-" },
            if self.owner_write { "w" } else { "-" },
            if self.owner_exec { "x" } else { "-" },
            if self.group_read { "r" } else { "-" },
            if self.group_write { "w" } else { "-" },
            if self.group_exec { "x" } else { "-" },
            if self.other_read { "r" } else { "-" },
            if self.other_write { "w" } else { "-" },
            if self.other_exec { "x" } else { "-" },
        )
    }
}

impl From<FilePermissionFlags> for FilePermissions {
    fn from(flags: FilePermissionFlags) -> Self {
        Self {
            other_read: flags.contains(FilePermissionFlags::OTHER_READ),
            other_write: flags.contains(FilePermissionFlags::OTHER_WRITE),
            other_exec: flags.contains(FilePermissionFlags::OTHER_EXEC),
            group_read: flags.contains(FilePermissionFlags::GROUP_READ),
            group_write: flags.contains(FilePermissionFlags::GROUP_WRITE),
            group_exec: flags.contains(FilePermissionFlags::GROUP_EXEC),
            owner_read: flags.contains(FilePermissionFlags::OWNER_READ),
            owner_write: flags.contains(FilePermissionFlags::OWNER_WRITE),
            owner_exec: flags.contains(FilePermissionFlags::OWNER_EXEC),
        }
    }
}

impl From<u32> for FilePermissions {
    fn from(mode: u32) -> Self {
        FilePermissionFlags::from_bits_truncate(mode).into()
    }
}

/// Used in the implementation of other packets.
/// Implements most [`Metadata`] methods
///
/// The fields `user` and `group` are string names of users and groups for
/// clients that can be displayed in longname. Can be omitted.
///
/// The `flags` field is omitted because it is set by itself depending on the fields
#[derive(Debug, Clone)]
pub struct FileAttributes {
    pub size: Option<u64>,
    pub uid: Option<u32>,
    pub user: Option<String>,
    pub gid: Option<u32>,
    pub group: Option<String>,
    pub permissions: Option<u32>,
    pub atime: Option<u32>,
    pub mtime: Option<u32>,
}

macro_rules! impl_fn_type {
    ($get_name:ident, $set_name:ident, $doc_name:expr, $flag:ident) => {
        #[doc = "Returns `true` if is a "]
        #[doc = $doc_name]
        pub fn $get_name(&self) -> bool {
            self.permissions.map_or(false, |b| {
                FileMode::from_bits_truncate(b).contains(FileMode::$flag)
            })
        }

        #[doc = "Set flag if is a "]
        #[doc = $doc_name]
        #[doc = " or not"]
        pub fn $set_name(&mut self, $get_name: bool) {
            match $get_name {
                true => self.set_type(FileMode::$flag),
                false => self.remove_type(FileMode::$flag),
            }
        }
    };
}

impl FileAttributes {
    impl_fn_type!(is_dir, set_dir, "dir", DIR);
    impl_fn_type!(is_regular, set_regular, "regular", REG);
    impl_fn_type!(is_symlink, set_symlink, "symlink", LNK);
    impl_fn_type!(is_character, set_character, "character", CHR);
    impl_fn_type!(is_block, set_block, "block", BLK);
    impl_fn_type!(is_fifo, set_fifo, "fifo", FIFO);

    /// Set mode flag
    pub fn set_type(&mut self, mode: FileMode) {
        let perms = self.permissions.unwrap_or(0);
        self.permissions = Some(perms | mode.bits());
    }

    /// Remove mode flag
    pub fn remove_type(&mut self, mode: FileMode) {
        let perms = self.permissions.unwrap_or(0);
        self.permissions = Some(perms & !mode.bits());
    }

    /// Returns the file type
    pub fn file_type(&self) -> FileType {
        FileMode::from_bits_truncate(self.permissions.unwrap_or_default()).into()
    }

    /// Returns `true` if is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the size of the file
    pub fn len(&self) -> u64 {
        self.size.unwrap_or(0)
    }

    /// Returns the permissions of the file this metadata is for.
    pub fn permissions(&self) -> FilePermissions {
        FilePermissionFlags::from_bits_truncate(self.permissions.unwrap_or_default()).into()
    }

    /// Returns the last access time
    pub fn accessed(&self) -> std::io::Result<SystemTime> {
        match self.atime {
            Some(time) => Ok(UNIX_EPOCH + Duration::from_secs(time as u64)),
            None => Err(ErrorKind::InvalidData.into()),
        }
    }

    /// Returns the last modification time
    pub fn modified(&self) -> std::io::Result<SystemTime> {
        match self.mtime {
            Some(time) => Ok(UNIX_EPOCH + Duration::from_secs(time as u64)),
            None => Err(ErrorKind::InvalidData.into()),
        }
    }

    /// Creates a structure with omitted attributes
    pub fn empty() -> Self {
        Self {
            size: None,
            uid: None,
            user: None,
            gid: None,
            group: None,
            permissions: None,
            atime: None,
            mtime: None,
        }
    }
}

/// For packets which require dummy attributes
impl Default for FileAttributes {
    fn default() -> Self {
        Self {
            size: Some(0),
            uid: Some(0),
            user: None,
            gid: Some(0),
            group: None,
            permissions: Some(0o777 | FileMode::DIR.bits()),
            atime: Some(0),
            mtime: Some(0),
        }
    }
}

/// For simple conversion of [`Metadata`] into [`FileAttributes`]
impl From<&Metadata> for FileAttributes {
    fn from(metadata: &Metadata) -> Self {
        let mut attrs = Self {
            size: Some(metadata.len()),
            #[cfg(unix)]
            uid: Some(metadata.uid()),
            #[cfg(unix)]
            gid: Some(metadata.gid()),
            #[cfg(windows)]
            permissions: Some(if metadata.permissions().readonly() {
                0o555
            } else {
                0o777
            }),
            #[cfg(unix)]
            permissions: Some(metadata.mode()),
            atime: Some(utils::unix(metadata.accessed().unwrap_or(UNIX_EPOCH))),
            mtime: Some(utils::unix(metadata.modified().unwrap_or(UNIX_EPOCH))),
            ..Default::default()
        };

        attrs.set_dir(metadata.is_dir());
        attrs.set_regular(!metadata.is_dir());

        attrs
    }
}

impl Serialize for FileAttributes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut attrs = FileAttr::default();
        let mut field_count = 1;

        if self.size.is_some() {
            attrs |= FileAttr::SIZE;
            field_count += 1;
        }

        if self.uid.is_some() || self.gid.is_some() {
            attrs |= FileAttr::UIDGID;
            field_count += 2;
        }

        if self.permissions.is_some() {
            attrs |= FileAttr::PERMISSIONS;
            field_count += 1;
        }

        if self.atime.is_some() || self.mtime.is_some() {
            attrs |= FileAttr::ACMODTIME;
            field_count += 2;
        }

        let mut s = serializer.serialize_struct("FileAttributes", field_count)?;
        s.serialize_field("attrs", &attrs)?;

        if let Some(size) = self.size {
            s.serialize_field("size", &size)?;
        }

        if self.uid.is_some() || self.gid.is_some() {
            s.serialize_field("uid", &self.uid.unwrap_or(0))?;
            s.serialize_field("gid", &self.gid.unwrap_or(0))?;
        }

        if let Some(permissions) = self.permissions {
            s.serialize_field("permissions", &permissions)?;
        }

        if self.atime.is_some() || self.mtime.is_some() {
            s.serialize_field("atime", &self.atime.unwrap_or(0))?;
            s.serialize_field("mtime", &self.mtime.unwrap_or(0))?;
        }

        // todo: extended implementation

        s.end()
    }
}

impl<'de> Deserialize<'de> for FileAttributes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct FileAttributesVisitor;

        impl<'de> Visitor<'de> for FileAttributesVisitor {
            type Value = FileAttributes;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("file attributes")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let attrs = FileAttr::from_bits_truncate(seq.next_element::<u32>()?.unwrap_or(0));

                Ok(FileAttributes {
                    size: if attrs.contains(FileAttr::SIZE) {
                        seq.next_element::<u64>()?
                    } else {
                        None
                    },
                    uid: if attrs.contains(FileAttr::UIDGID) {
                        seq.next_element::<u32>()?
                    } else {
                        None
                    },
                    user: None,
                    gid: if attrs.contains(FileAttr::UIDGID) {
                        seq.next_element::<u32>()?
                    } else {
                        None
                    },
                    group: None,
                    permissions: if attrs.contains(FileAttr::PERMISSIONS) {
                        seq.next_element::<u32>()?
                    } else {
                        None
                    },
                    atime: if attrs.contains(FileAttr::ACMODTIME) {
                        seq.next_element::<u32>()?
                    } else {
                        None
                    },
                    mtime: if attrs.contains(FileAttr::ACMODTIME) {
                        seq.next_element::<u32>()?
                    } else {
                        None
                    },
                })
            }
        }

        deserializer.deserialize_any(FileAttributesVisitor)
    }
}
