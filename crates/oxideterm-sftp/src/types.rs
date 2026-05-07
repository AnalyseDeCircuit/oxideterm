// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

pub use oxideterm_preview::{PreviewAssetKind as AssetFileKind, PreviewContent};

pub mod constants {
    pub const MAX_TEXT_PREVIEW_SIZE: u64 = 2 * 1024 * 1024;
    pub const MAX_PREVIEW_SIZE: u64 = 50 * 1024 * 1024;
    pub const MAX_MEDIA_PREVIEW_SIZE: u64 = 200 * 1024 * 1024;
    pub const HEX_CHUNK_SIZE: u64 = 16 * 1024;
    pub const STREAMING_PREVIEW_CHUNK_SIZE: usize = 256 * 1024;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub file_type: FileType,
    pub size: u64,
    pub modified: i64,
    pub permissions: String,
    pub owner: Option<String>,
    pub group: Option<String>,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileType {
    File,
    Directory,
    Symlink,
    Unknown,
}

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListFilter {
    #[serde(default)]
    pub show_hidden: bool,
    pub pattern: Option<String>,
    #[serde(default)]
    pub sort: SortOrder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub id: String,
    pub remote_path: String,
    pub local_path: String,
    pub direction: TransferDirection,
    pub state: TransferState,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub speed: u64,
    pub eta_seconds: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferDirection {
    Upload,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferState {
    Pending,
    InProgress,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

pub struct AdaptiveChunkSizer {
    current: usize,
    window_bytes: u64,
    window_start: std::time::Instant,
}

impl AdaptiveChunkSizer {
    pub const MIN_CHUNK: usize = 64 * 1024;
    pub const MAX_CHUNK: usize = 2 * 1024 * 1024;
    const ADAPT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

    pub fn new() -> Self {
        Self {
            current: 256 * 1024,
            window_bytes: 0,
            window_start: std::time::Instant::now(),
        }
    }

    pub fn chunk_size(&self) -> usize {
        self.current
    }

    pub fn record(&mut self, bytes: usize) {
        self.window_bytes += bytes as u64;
        if self.window_start.elapsed() >= Self::ADAPT_INTERVAL {
            let elapsed = self.window_start.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                self.current =
                    Self::throughput_to_chunk((self.window_bytes as f64 / elapsed) as u64);
            }
            self.window_bytes = 0;
            self.window_start = std::time::Instant::now();
        }
    }

    fn throughput_to_chunk(bytes_per_sec: u64) -> usize {
        match bytes_per_sec {
            0..=262_144 => Self::MIN_CHUNK,
            262_145..=1_048_576 => 128 * 1024,
            1_048_577..=10_485_760 => 256 * 1024,
            10_485_761..=52_428_800 => 1_048_576,
            _ => Self::MAX_CHUNK,
        }
    }
}

impl Default for AdaptiveChunkSizer {
    fn default() -> Self {
        Self::new()
    }
}

pub fn extension_to_language(ext: &str) -> Option<String> {
    let language = match ext.to_ascii_lowercase().as_str() {
        "sh" | "bash" | "zsh" | "fish" | "bashrc" | "zshrc" | "profile" | "env" | "envrc" => "bash",
        "conf" | "cfg" | "ini" | "properties" | "editorconfig" => "ini",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "json" | "jsonc" | "json5" => "json",
        "xml" | "svg" | "xsd" | "xsl" => "xml",
        "html" | "htm" | "xhtml" => "html",
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
        "rb" | "rake" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "scala" | "sc" => "scala",
        "r" | "rmd" => "r",
        "lua" => "lua",
        "pl" | "pm" => "perl",
        "sql" => "sql",
        "md" | "markdown" => "markdown",
        "tex" | "latex" => "latex",
        "css" | "scss" | "sass" | "less" => "css",
        "graphql" | "gql" => "graphql",
        "dockerfile" => "docker",
        "makefile" | "mk" => "makefile",
        "cmake" => "cmake",
        "diff" | "patch" => "diff",
        "log" => "log",
        _ => return None,
    };
    Some(language.to_string())
}

pub fn is_text_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "ps1"
            | "bat"
            | "cmd"
            | "bashrc"
            | "zshrc"
            | "profile"
            | "gitconfig"
            | "gitignore"
            | "dockerignore"
            | "conf"
            | "cfg"
            | "ini"
            | "properties"
            | "env"
            | "envrc"
            | "yaml"
            | "yml"
            | "toml"
            | "json"
            | "jsonc"
            | "json5"
            | "xml"
            | "svg"
            | "html"
            | "htm"
            | "rs"
            | "py"
            | "js"
            | "ts"
            | "jsx"
            | "tsx"
            | "c"
            | "h"
            | "cpp"
            | "java"
            | "go"
            | "rb"
            | "php"
            | "swift"
            | "kt"
            | "scala"
            | "r"
            | "lua"
            | "pl"
            | "sql"
            | "txt"
            | "text"
            | "md"
            | "markdown"
            | "rst"
            | "adoc"
            | "org"
            | "tex"
            | "css"
            | "scss"
            | "sass"
            | "less"
            | "dockerfile"
            | "makefile"
            | "mk"
            | "cmake"
            | "gradle"
            | "diff"
            | "patch"
            | "log"
            | "csv"
            | "tsv"
    )
}

pub fn is_office_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" | "odg" | "rtf"
    )
}

pub fn is_likely_text_content(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    let sample = &bytes[..bytes.len().min(8192)];
    if sample.contains(&0) {
        return false;
    }
    let control = sample
        .iter()
        .filter(|&&byte| matches!(byte, 0x01..=0x08 | 0x0b..=0x0c | 0x0e..=0x1f | 0x7f))
        .count();
    if control as f64 / sample.len() as f64 > 0.10 {
        return false;
    }
    std::str::from_utf8(bytes).is_ok() || sample.iter().any(|byte| *byte >= 0x80)
}

pub fn generate_hex_dump(data: &[u8], offset: u64) -> String {
    use std::fmt::Write;

    let mut result = String::new();
    for (i, chunk) in data.chunks(16).enumerate() {
        let address = offset + (i * 16) as u64;
        let _ = write!(result, "{address:08X}  ");
        for (j, byte) in chunk.iter().enumerate() {
            if j == 8 {
                result.push(' ');
            }
            let _ = write!(result, "{byte:02X} ");
        }
        for j in chunk.len()..16 {
            if j == 8 {
                result.push(' ');
            }
            result.push_str("   ");
        }
        result.push_str(" |");
        for byte in chunk {
            result.push(if (0x20..0x7f).contains(byte) {
                *byte as char
            } else {
                '.'
            });
        }
        result.push_str("|\n");
    }
    result
}

pub fn detect_and_decode(bytes: &[u8]) -> (String, String, f32, bool) {
    let (has_bom, bom_encoding) = check_bom(bytes);
    if let Some(encoding) = bom_encoding {
        let (text, _, _) = encoding.decode(bytes);
        return (text.into_owned(), encoding.name().to_string(), 1.0, true);
    }

    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);
    let confidence = if encoding == encoding_rs::UTF_8 {
        if std::str::from_utf8(bytes).is_ok() {
            1.0
        } else {
            0.8
        }
    } else {
        0.7
    };
    let (text, _, had_errors) = encoding.decode(bytes);
    (
        text.into_owned(),
        encoding.name().to_string(),
        if had_errors {
            confidence * 0.8
        } else {
            confidence
        },
        has_bom,
    )
}

fn check_bom(bytes: &[u8]) -> (bool, Option<&'static encoding_rs::Encoding>) {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return (true, Some(encoding_rs::UTF_8));
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        return (true, Some(encoding_rs::UTF_16BE));
    }
    if bytes.starts_with(&[0xff, 0xfe]) {
        return (true, Some(encoding_rs::UTF_16LE));
    }
    (false, None)
}

pub fn encode_to_encoding(text: &str, encoding_name: &str) -> Vec<u8> {
    let encoding =
        encoding_rs::Encoding::for_label(encoding_name.as_bytes()).unwrap_or(encoding_rs::UTF_8);
    if encoding == encoding_rs::UTF_8 {
        return text.as_bytes().to_vec();
    }
    let (encoded, _, _) = encoding.encode(text);
    encoded.into_owned()
}
