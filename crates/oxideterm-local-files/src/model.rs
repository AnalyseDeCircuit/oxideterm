#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalFileType {
    File,
    Directory,
    Symlink,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalSortField {
    Name,
    Size,
    Modified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalSortDirection {
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalClipboardMode {
    Copy,
    Cut,
}

#[derive(Clone, Debug)]
pub struct LocalFileEntry {
    pub name: String,
    pub path: String,
    pub file_type: LocalFileType,
    pub size: u64,
    pub modified: Option<i64>,
    pub readonly: bool,
    pub symlink_target: Option<String>,
}

impl LocalFileEntry {
    pub fn is_directory_like(&self) -> bool {
        self.file_type == LocalFileType::Directory
    }
}

#[derive(Clone, Debug)]
pub struct LocalDrive {
    pub name: String,
    pub path: String,
    pub drive_type: String,
    pub total_space: u64,
    pub available_space: u64,
    pub read_only: bool,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct LocalBookmark {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(rename = "createdAt")]
    pub created_at: u64,
}

#[derive(Clone, Debug)]
pub enum LocalPreview {
    Loading,
    Text {
        content: String,
        language: Option<String>,
    },
    TextStream {
        path: String,
        size: u64,
        language: Option<String>,
    },
    Markdown {
        content: String,
    },
    Image {
        path: String,
        mime_type: String,
    },
    Video {
        path: String,
        mime_type: String,
    },
    Audio {
        path: String,
        mime_type: String,
    },
    Font {
        path: String,
        mime_type: String,
    },
    Archive {
        info: LocalArchiveInfo,
    },
    TooLarge {
        size: u64,
    },
    Unsupported(String),
    Error(String),
}

#[derive(Clone, Debug)]
pub struct LocalPreviewChunk {
    pub data: Vec<u8>,
    pub eof: bool,
}

#[derive(Clone, Debug)]
pub struct LocalArchiveEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub compressed_size: u64,
    pub modified: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LocalArchiveInfo {
    pub entries: Vec<LocalArchiveEntry>,
    pub total_files: u64,
    pub total_dirs: u64,
    pub total_size: u64,
    pub compressed_size: u64,
}

#[derive(Clone, Debug)]
pub struct LocalPreviewMetadata {
    pub size: u64,
    pub modified: Option<i64>,
    pub created: Option<i64>,
    pub accessed: Option<i64>,
    pub readonly: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub mode: Option<u32>,
    pub mime_type: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LocalChecksumResult {
    pub md5: String,
    pub sha256: String,
}
