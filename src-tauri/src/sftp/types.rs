//! SFTP data types

use serde::{Deserialize, Serialize};

/// File entry information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// File name (not full path)
    pub name: String,
    /// Full path
    pub path: String,
    /// File type
    pub file_type: FileType,
    /// File size in bytes
    pub size: u64,
    /// Last modified time (Unix timestamp)
    pub modified: i64,
    /// File permissions (octal string, e.g., "755")
    pub permissions: String,
    /// Owner username (if available)
    pub owner: Option<String>,
    /// Group name (if available)
    pub group: Option<String>,
    /// Is symbolic link
    pub is_symlink: bool,
    /// Symlink target (if is_symlink)
    pub symlink_target: Option<String>,
}

/// File type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileType {
    File,
    Directory,
    Symlink,
    Unknown,
}

impl FileType {
    /// Get icon name for UI
    pub fn icon(&self) -> &'static str {
        match self {
            FileType::File => "file",
            FileType::Directory => "folder",
            FileType::Symlink => "link",
            FileType::Unknown => "file-question",
        }
    }
}

/// What kind of media the asset file contains, so the frontend picks the right player.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetFileKind {
    Image,
    Video,
    Audio,
    Pdf,
    Office,
}

/// File preview content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PreviewContent {
    /// Plain text content (code, config, logs, etc.)
    Text {
        data: String,
        mime_type: Option<String>,
        /// Language hint for syntax highlighting (e.g., "rust", "python", "bash")
        language: Option<String>,
        /// Detected encoding (e.g., "UTF-8", "GBK", "Shift_JIS")
        encoding: String,
        /// Detection confidence (0.0 - 1.0)
        #[serde(default)]
        confidence: f32,
        /// Whether file has BOM (Byte Order Mark)
        #[serde(default)]
        has_bom: bool,
    },
    /// Base64-encoded image content (small images only, ≤ 512 KB)
    Image { data: String, mime_type: String },
    /// Local temp file served via `asset://` protocol.
    /// Frontend should use `convertFileSrc(path)` to build the URL.
    /// This avoids buffering the entire file through IPC.
    AssetFile {
        /// Canonical local path to the temp file (already allowed on asset scope)
        path: String,
        /// MIME type for the content
        mime_type: String,
        /// Kind hint so frontend knows which player/viewer to use
        kind: AssetFileKind,
    },
    /// Hex dump for binary files (incremental loading)
    Hex {
        /// Hex dump string
        data: String,
        /// Total file size
        total_size: u64,
        /// Current offset (for incremental loading)
        offset: u64,
        /// Bytes shown in this chunk
        chunk_size: u64,
        /// Whether there's more data to load
        has_more: bool,
    },
    /// File is too large to preview
    TooLarge {
        size: u64,
        max_size: u64,
        /// Recommend downloading instead
        recommend_download: bool,
    },
    /// File type cannot be previewed
    Unsupported {
        mime_type: String,
        /// Human-readable reason
        reason: String,
    },
}

/// Transfer progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferProgress {
    /// Unique transfer ID
    pub id: String,
    /// Remote file path
    pub remote_path: String,
    /// Local file path
    pub local_path: String,
    /// Transfer direction
    pub direction: TransferDirection,
    /// Current state
    pub state: TransferState,
    /// Total bytes to transfer
    pub total_bytes: u64,
    /// Bytes transferred so far
    pub transferred_bytes: u64,
    /// Transfer speed in bytes/second
    pub speed: u64,
    /// Estimated time remaining in seconds
    pub eta_seconds: Option<u64>,
    /// Error message if failed
    pub error: Option<String>,
}

impl TransferProgress {
    /// Calculate progress percentage (0-100)
    pub fn percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            100.0
        } else {
            (self.transferred_bytes as f64 / self.total_bytes as f64) * 100.0
        }
    }
}

/// Transfer direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferDirection {
    Upload,
    Download,
}

/// Transfer state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferState {
    /// Waiting in queue
    Pending,
    /// Currently transferring
    InProgress,
    /// Paused by user
    Paused,
    /// Completed successfully
    Completed,
    /// Failed with error
    Failed,
    /// Cancelled by user
    Cancelled,
}

/// Transfer request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRequest {
    /// Session ID to use for transfer
    pub session_id: String,
    /// Remote file path
    pub remote_path: String,
    /// Local file path
    pub local_path: String,
    /// Transfer direction
    pub direction: TransferDirection,
}

/// SFTP operation result for batch operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    /// Successfully processed paths
    pub success: Vec<String>,
    /// Failed paths with error messages
    pub failed: Vec<(String, String)>,
}

/// Sort order for directory listing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    #[default]
    Name,
    NameDesc,
    Size,
    SizeDesc,
    Modified,
    ModifiedDesc,
    Type,
    TypeDesc,
}

/// Filter for directory listing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListFilter {
    /// Show hidden files (starting with .)
    #[serde(default)]
    pub show_hidden: bool,
    /// File name pattern to match (glob-style)
    pub pattern: Option<String>,
    /// Sort order
    #[serde(default)]
    pub sort: SortOrder,
}

/// Constants for SFTP operations
pub mod constants {
    /// Default chunk size for file transfers (256 KB)
    pub const DEFAULT_CHUNK_SIZE: usize = 256 * 1024;

    /// Maximum file size for preview (10 MB)
    pub const MAX_PREVIEW_SIZE: u64 = 10 * 1024 * 1024;

    /// Maximum text preview size (1 MB)
    pub const MAX_TEXT_PREVIEW_SIZE: u64 = 1024 * 1024;

    /// Maximum video/audio preview size (50 MB)
    pub const MAX_MEDIA_PREVIEW_SIZE: u64 = 50 * 1024 * 1024;

    /// Maximum Office document size for conversion (10 MB)
    pub const MAX_OFFICE_CONVERT_SIZE: u64 = 10 * 1024 * 1024;

    /// Hex preview chunk size (16 KB)
    pub const HEX_CHUNK_SIZE: u64 = 16 * 1024;

    /// Maximum concurrent transfers
    pub const MAX_CONCURRENT_TRANSFERS: usize = 3;

    /// Buffer size for streaming transfers
    pub const STREAM_BUFFER_SIZE: usize = 256 * 1024;
}

/// Adaptive chunk size calculator for SFTP transfers.
///
/// Adjusts read/write buffer size based on measured throughput over
/// a 1-second sliding window:
///
/// | Throughput        | Chunk Size |
/// |-------------------|------------|
/// | < 256 KB/s        | 64 KB      |
/// | 256 KB – 1 MB/s   | 128 KB     |
/// | 1 – 10 MB/s       | 256 KB     |
/// | 10 – 50 MB/s      | 1 MB       |
/// | > 50 MB/s         | 2 MB       |
///
/// The buffer is always allocated at [`MAX_CHUNK`] size; only the
/// slice passed to read/write varies.
pub struct AdaptiveChunkSizer {
    current: usize,
    window_bytes: u64,
    window_start: std::time::Instant,
}

impl AdaptiveChunkSizer {
    /// Smallest allowed chunk (64 KB)
    pub const MIN_CHUNK: usize = 64 * 1024;
    /// Largest allowed chunk (2 MB)
    pub const MAX_CHUNK: usize = 2 * 1024 * 1024;
    /// Measurement window (1 second)
    const ADAPT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

    /// Create a new sizer starting at [`DEFAULT_CHUNK_SIZE`](constants::DEFAULT_CHUNK_SIZE).
    pub fn new() -> Self {
        Self {
            current: constants::DEFAULT_CHUNK_SIZE,
            window_bytes: 0,
            window_start: std::time::Instant::now(),
        }
    }

    /// Current chunk size to use for the next read/write.
    #[inline]
    pub fn chunk_size(&self) -> usize {
        self.current
    }

    /// Record `bytes` transferred; recalculates chunk size once per window.
    pub fn record(&mut self, bytes: usize) {
        self.window_bytes += bytes as u64;

        if self.window_start.elapsed() >= Self::ADAPT_INTERVAL {
            let elapsed = self.window_start.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                let throughput = self.window_bytes as f64 / elapsed;
                self.current = Self::throughput_to_chunk(throughput as u64);
            }
            // Reset window
            self.window_bytes = 0;
            self.window_start = std::time::Instant::now();
        }
    }

    /// Deterministic mapping from bytes/sec → chunk size.
    #[inline]
    fn throughput_to_chunk(bytes_per_sec: u64) -> usize {
        match bytes_per_sec {
            0..=262_144 => Self::MIN_CHUNK,            // < 256 KB/s
            262_145..=1_048_576 => 128 * 1024,         // 256 KB/s – 1 MB/s
            1_048_577..=10_485_760 => 256 * 1024,      // 1 – 10 MB/s
            10_485_761..=52_428_800 => 1_048_576,       // 10 – 50 MB/s
            _ => Self::MAX_CHUNK,                       // > 50 MB/s
        }
    }
}

/// Map file extension to syntax highlighting language
pub fn extension_to_language(ext: &str) -> Option<String> {
    let lang = match ext.to_lowercase().as_str() {
        // Shell scripts
        "sh" | "bash" | "zsh" | "fish" => "bash",
        // Shell config dotfiles (.bashrc -> ext="bashrc", .zshrc -> ext="zshrc")
        "bashrc" | "bash_profile" | "bash_login" | "bash_logout" | "bash_aliases" => "bash",
        "zshrc" | "zprofile" | "zshenv" | "zlogin" | "zlogout" => "bash",
        "profile" | "cshrc" | "tcshrc" | "kshrc" => "bash",
        // Config files
        "conf" | "cfg" | "ini" | "properties" => "ini",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "json" | "jsonc" | "json5" => "json",
        "xml" | "svg" | "xsd" | "xsl" => "xml",
        "html" | "htm" | "xhtml" => "html",
        // Programming languages
        "rs" => "rust",
        "py" | "pyw" | "pyi" => "python",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" | "mts" | "cts" => "typescript",
        "jsx" => "jsx",
        "tsx" => "tsx",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => "cpp",
        "java" => "java",
        "go" => "go",
        "rb" | "rake" | "gemspec" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "scala" | "sc" => "scala",
        "r" | "rmd" => "r",
        "lua" => "lua",
        "pl" | "pm" => "perl",
        "sql" => "sql",
        // Markup/Data
        "md" | "markdown" => "markdown",
        "tex" | "latex" => "latex",
        "css" | "scss" | "sass" | "less" => "css",
        "graphql" | "gql" => "graphql",
        // DevOps/System
        "dockerfile" => "docker",
        "makefile" | "mk" => "makefile",
        "cmake" => "cmake",
        "nginx" => "nginx",
        "diff" | "patch" => "diff",
        "log" => "log",
        // Special files
        "env" | "envrc" => "bash",
        "gitignore" | "dockerignore" => "gitignore",
        "editorconfig" => "ini",
        _ => return None,
    };
    Some(lang.to_string())
}

/// Check if file extension indicates a text/script file
pub fn is_text_extension(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        // Scripts & configs
        "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd" |
        // Dotfiles (e.g., .bashrc -> ext="bashrc", .zshrc -> ext="zshrc")
        "bashrc" | "zshrc" | "profile" | "bash_profile" | "zprofile" |
        "bash_login" | "bash_logout" | "zlogin" | "zlogout" |
        "inputrc" | "vimrc" | "gvimrc" | "exrc" | "nanorc" |
        "tmux" | "screenrc" | "gitconfig" | "gitattributes" |
        "npmrc" | "yarnrc" | "gemrc" | "irbrc" | "pryrc" |
        "curlrc" | "wgetrc" | "netrc" | "mailrc" |
        // Config extensions
        "conf" | "cfg" | "ini" | "properties" | "env" | "envrc" |
        "yaml" | "yml" | "toml" | "json" | "jsonc" | "json5" |
        "xml" | "svg" | "xsd" | "xsl" | "html" | "htm" | "xhtml" |
        // Code
        "rs" | "py" | "pyw" | "pyi" | "js" | "mjs" | "cjs" | "ts" | "mts" |
        "jsx" | "tsx" | "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hxx" |
        "java" | "go" | "rb" | "rake" | "php" | "swift" | "kt" | "kts" |
        "scala" | "r" | "rmd" | "lua" | "pl" | "pm" | "sql" |
        // Text/Docs
        "txt" | "text" | "md" | "markdown" | "rst" | "adoc" | "org" |
        "tex" | "latex" | "css" | "scss" | "sass" | "less" |
        // DevOps
        "dockerfile" | "makefile" | "mk" | "cmake" | "gradle" |
        "gitignore" | "dockerignore" | "editorconfig" |
        "diff" | "patch" | "log" | "csv" | "tsv"
    )
}

/// Check if MIME type indicates video
pub fn is_video_mime(mime: &str) -> bool {
    mime.starts_with("video/")
}

/// Check if MIME type indicates audio
pub fn is_audio_mime(mime: &str) -> bool {
    mime.starts_with("audio/")
}

/// Check if file is an Office document
pub fn is_office_extension(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        // Microsoft Office
        "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" |
        // LibreOffice
        "odt" | "ods" | "odp" | "odg" |
        // Legacy
        "rtf"
    )
}

/// Check if file is a PDF
pub fn is_pdf_extension(ext: &str) -> bool {
    ext.eq_ignore_ascii_case("pdf")
}

/// Check if bytes are likely text content (not binary)
/// Uses heuristics similar to how Git detects binary files:
/// - Check for NUL bytes (binary indicator)
/// - Check ratio of printable vs non-printable characters
pub fn is_likely_text_content(bytes: &[u8]) -> bool {
    // Empty files are considered text
    if bytes.is_empty() {
        return true;
    }

    // Sample size: check first 8KB or entire file if smaller
    let sample_size = bytes.len().min(8192);
    let sample = &bytes[..sample_size];

    // Count different character types
    let mut nul_count = 0;
    let mut control_count = 0;
    let mut high_byte_count = 0;

    for &byte in sample {
        match byte {
            // NUL byte - strong binary indicator
            0x00 => nul_count += 1,
            // Common text control characters (tab, newline, carriage return)
            0x09 | 0x0A | 0x0D => {}
            // Other control characters (0x01-0x08, 0x0B-0x0C, 0x0E-0x1F)
            0x01..=0x08 | 0x0B..=0x0C | 0x0E..=0x1F => control_count += 1,
            // DEL character
            0x7F => control_count += 1,
            // High bytes (could be UTF-8 continuation or binary)
            0x80..=0xFF => high_byte_count += 1,
            // Printable ASCII (0x20-0x7E)
            _ => {}
        }
    }

    // If there are any NUL bytes, it's likely binary
    // (text files almost never contain NUL)
    if nul_count > 0 {
        return false;
    }

    // If more than 10% are control characters, it's likely binary
    let control_ratio = control_count as f64 / sample_size as f64;
    if control_ratio > 0.10 {
        return false;
    }

    // Try to validate as UTF-8 - if it's valid UTF-8, treat as text
    if std::str::from_utf8(bytes).is_ok() {
        return true;
    }

    // If high bytes exist but not valid UTF-8, could still be text in other encoding
    // (e.g., Latin-1, GB2312, Shift-JIS)
    // Use chardetng to detect - if it finds a reasonable encoding, treat as text
    if high_byte_count > 0 {
        use chardetng::EncodingDetector;
        let mut detector = EncodingDetector::new();
        detector.feed(sample, true);
        let _encoding = detector.guess(None, true);
        // chardetng always returns an encoding, so we accept it if:
        // - No NUL bytes
        // - Reasonable control character ratio
        return true;
    }

    // Pure ASCII with minimal control characters - definitely text
    true
}

/// Generate hex dump from bytes
pub fn generate_hex_dump(data: &[u8], offset: u64) -> String {
    use std::fmt::Write;

    let mut result = String::new();
    let bytes_per_line = 16;

    for (i, chunk) in data.chunks(bytes_per_line).enumerate() {
        let addr = offset + (i * bytes_per_line) as u64;

        // Address
        write!(result, "{:08X}  ", addr).unwrap();

        // Hex bytes
        for (j, byte) in chunk.iter().enumerate() {
            if j == 8 {
                result.push(' ');
            }
            write!(result, "{:02X} ", byte).unwrap();
        }

        // Padding for incomplete lines
        for j in chunk.len()..bytes_per_line {
            if j == 8 {
                result.push(' ');
            }
            result.push_str("   ");
        }

        // ASCII representation
        result.push_str(" |");
        for byte in chunk {
            let c = if *byte >= 0x20 && *byte < 0x7F {
                *byte as char
            } else {
                '.'
            };
            result.push(c);
        }
        result.push_str("|\n");
    }

    result
}

/// Detect encoding and decode bytes to UTF-8 string
///
/// Uses chardetng for encoding detection and encoding_rs for conversion.
/// Returns: (decoded_text, encoding_name, confidence, has_bom)
pub fn detect_and_decode(bytes: &[u8]) -> (String, String, f32, bool) {
    use chardetng::EncodingDetector;

    // Check for BOM first
    let (has_bom, bom_encoding) = check_bom(bytes);

    if let Some(encoding) = bom_encoding {
        // If BOM detected, use that encoding directly
        let (cow, _, _) = encoding.decode(bytes);
        return (cow.into_owned(), encoding.name().to_string(), 1.0, true);
    }

    // Use chardetng for detection
    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);

    // Guess with allow_utf8 = true to prefer UTF-8 when valid
    let encoding = detector.guess(None, true);

    // Calculate rough confidence based on encoding detection
    // chardetng doesn't expose confidence directly, so we estimate:
    // - UTF-8 that validates perfectly = high confidence
    // - Other encodings = medium confidence
    let confidence = if encoding == encoding_rs::UTF_8 {
        // Check if it's actually valid UTF-8
        if std::str::from_utf8(bytes).is_ok() {
            1.0
        } else {
            0.8 // Probably UTF-8 with some invalid sequences
        }
    } else {
        0.7 // Other encoding detected
    };

    // Decode using the detected encoding
    let (cow, _, had_errors) = encoding.decode(bytes);

    // Adjust confidence if there were decoding errors
    let final_confidence = if had_errors {
        confidence * 0.8
    } else {
        confidence
    };

    (
        cow.into_owned(),
        encoding.name().to_string(),
        final_confidence,
        has_bom,
    )
}

/// Check for Byte Order Mark (BOM) at the start of bytes
/// Returns (has_bom, Option<encoding>)
fn check_bom(bytes: &[u8]) -> (bool, Option<&'static encoding_rs::Encoding>) {
    use encoding_rs::{UTF_16BE, UTF_16LE, UTF_8};

    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        return (true, Some(UTF_8));
    }
    if bytes.len() >= 2 {
        if bytes[0] == 0xFE && bytes[1] == 0xFF {
            return (true, Some(UTF_16BE));
        }
        if bytes[0] == 0xFF && bytes[1] == 0xFE {
            return (true, Some(UTF_16LE));
        }
    }
    (false, None)
}

/// Encode UTF-8 string to target encoding
///
/// Returns the encoded bytes. If encoding fails, returns UTF-8 bytes as fallback.
pub fn encode_to_encoding(text: &str, encoding_name: &str) -> Vec<u8> {
    // Find the encoding by name
    let encoding =
        encoding_rs::Encoding::for_label(encoding_name.as_bytes()).unwrap_or(encoding_rs::UTF_8);

    // If target is UTF-8, just return the bytes directly
    if encoding == encoding_rs::UTF_8 {
        return text.as_bytes().to_vec();
    }

    // Encode to target encoding
    let (cow, _, _) = encoding.encode(text);
    cow.into_owned()
}
