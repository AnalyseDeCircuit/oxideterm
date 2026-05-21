use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use crate::{LocalPreview, LocalPreviewChunk, LocalPreviewMetadata, list_local_archive_contents};

pub const MAX_PREVIEW_SIZE: u64 = 10 * 1024 * 1024;
pub const STREAM_PREVIEW_THRESHOLD: u64 = 256 * 1024;

pub fn read_local_preview(path: &str) -> LocalPreview {
    let path_ref = Path::new(path);
    let Ok(metadata) = std::fs::metadata(path_ref) else {
        return LocalPreview::Error("Unable to read file metadata".to_string());
    };
    let file_name = path_ref
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = local_file_extension(&file_name);
    let file_size = metadata.len();

    if image_extensions().contains(&ext.as_str()) {
        if file_size > MAX_PREVIEW_SIZE {
            return LocalPreview::TooLarge { size: file_size };
        }
        return LocalPreview::Image {
            path: path.to_string(),
            mime_type: mime_type_for_extension(&ext),
        };
    }
    if video_extensions().contains(&ext.as_str()) {
        if file_size > MAX_PREVIEW_SIZE {
            return LocalPreview::TooLarge { size: file_size };
        }
        return LocalPreview::Video {
            path: path.to_string(),
            mime_type: mime_type_for_extension(&ext),
        };
    }
    if audio_extensions().contains(&ext.as_str()) {
        if file_size > MAX_PREVIEW_SIZE {
            return LocalPreview::TooLarge { size: file_size };
        }
        return LocalPreview::Audio {
            path: path.to_string(),
            mime_type: mime_type_for_extension(&ext),
        };
    }
    if font_extensions().contains(&ext.as_str()) {
        if file_size > MAX_PREVIEW_SIZE {
            return LocalPreview::TooLarge { size: file_size };
        }
        return LocalPreview::Font {
            path: path.to_string(),
            mime_type: mime_type_for_extension(&ext),
        };
    }
    if ext == "pdf" {
        return LocalPreview::Unsupported("fileManager.openExternal".to_string());
    }
    if archive_extensions().contains(&ext.as_str()) {
        return match list_local_archive_contents(path) {
            Ok(info) => LocalPreview::Archive { info },
            Err(_) => LocalPreview::Unsupported("fileManager.binaryFile".to_string()),
        };
    }
    if office_extensions().contains(&ext.as_str()) {
        return LocalPreview::Unsupported("fileManager.openExternal".to_string());
    }
    let language = language_for_extension(&ext, &file_name);
    if file_size >= STREAM_PREVIEW_THRESHOLD
        && !markdown_extensions().contains(&ext.as_str())
        && (language.is_some() || text_extensions().contains(&ext.as_str()))
    {
        return match std::fs::File::open(path_ref) {
            Ok(mut file) => {
                let mut sample = vec![0u8; 4096usize.min(file_size as usize)];
                match file.read(&mut sample) {
                    Ok(bytes_read) => {
                        sample.truncate(bytes_read);
                        if looks_binary(&sample) {
                            LocalPreview::Unsupported("fileManager.binaryFile".to_string())
                        } else {
                            LocalPreview::TextStream {
                                path: path.to_string(),
                                size: file_size,
                                language,
                            }
                        }
                    }
                    Err(error) => LocalPreview::Error(error.to_string()),
                }
            }
            Err(error) => LocalPreview::Error(error.to_string()),
        };
    }
    if file_size > MAX_PREVIEW_SIZE {
        return LocalPreview::TooLarge { size: file_size };
    }
    match std::fs::read(path) {
        Ok(bytes) if bytes.is_empty() => LocalPreview::Text {
            content: String::new(),
            language: language_for_extension(&ext, &file_name),
        },
        Ok(bytes) if looks_binary(&bytes) => {
            LocalPreview::Unsupported("fileManager.binaryFile".to_string())
        }
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => {
                if markdown_extensions().contains(&ext.as_str()) {
                    LocalPreview::Markdown { content: text }
                } else {
                    LocalPreview::Text {
                        content: text,
                        language: language_for_extension(&ext, &file_name),
                    }
                }
            }
            Err(error) => {
                let bytes = error.into_bytes();
                let text = String::from_utf8_lossy(&bytes).to_string();
                if looks_binary(text.as_bytes()) {
                    LocalPreview::Unsupported("fileManager.binaryFile".to_string())
                } else {
                    LocalPreview::Text {
                        content: text,
                        language: language_for_extension(&ext, &file_name),
                    }
                }
            }
        },
        Err(error) => LocalPreview::Error(error.to_string()),
    }
}

pub fn read_local_preview_range(
    path: &str,
    offset: u64,
    length: u64,
) -> Result<LocalPreviewChunk, String> {
    let mut file =
        std::fs::File::open(path).map_err(|error| format!("Failed to open file: {error}"))?;
    let file_len = file
        .metadata()
        .map_err(|error| format!("Failed to get metadata: {error}"))?
        .len();
    if offset >= file_len {
        return Ok(LocalPreviewChunk {
            data: Vec::new(),
            eof: true,
        });
    }

    let safe_len = length.min(1024 * 1024).min(file_len - offset);
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| format!("Failed to seek file: {error}"))?;
    let mut data = vec![0u8; safe_len as usize];
    let bytes_read = file
        .read(&mut data)
        .map_err(|error| format!("Failed to read file: {error}"))?;
    data.truncate(bytes_read);
    Ok(LocalPreviewChunk {
        eof: offset + bytes_read as u64 >= file_len,
        data,
    })
}

pub fn local_preview_metadata(path: &str) -> Option<LocalPreviewMetadata> {
    let path = Path::new(path);
    let metadata = std::fs::metadata(path).ok()?;
    let symlink_metadata = std::fs::symlink_metadata(path).ok();
    let is_symlink = symlink_metadata
        .as_ref()
        .is_some_and(|metadata| metadata.file_type().is_symlink());
    let timestamp = |time: std::io::Result<std::time::SystemTime>| {
        time.ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
    };
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        Some(metadata.permissions().mode())
    };
    #[cfg(not(unix))]
    let mode = None;
    Some(LocalPreviewMetadata {
        size: metadata.len(),
        modified: timestamp(metadata.modified()),
        created: timestamp(metadata.created()),
        accessed: timestamp(metadata.accessed()),
        readonly: metadata.permissions().readonly(),
        is_dir: metadata.is_dir(),
        is_symlink,
        mode,
        mime_type: path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| mime_type_for_extension(&ext.to_lowercase())),
    })
}

pub fn local_file_extension(file_name: &str) -> String {
    if file_name.starts_with('.') && !file_name[1..].contains('.') {
        return String::new();
    }
    Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
        .unwrap_or_default()
}

fn looks_binary(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(4096)];
    if sample.iter().filter(|byte| **byte == 0).count() > 10 {
        return true;
    }
    let non_printable = sample
        .iter()
        .filter(|byte| matches!(**byte, 0x00..=0x08 | 0x0e..=0x1f))
        .count();
    !sample.is_empty() && non_printable > sample.len() / 10
}

fn image_extensions() -> &'static [&'static str] {
    &["png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp"]
}

fn video_extensions() -> &'static [&'static str] {
    &["mp4", "webm", "ogv", "mov", "mkv", "avi", "m4v"]
}

fn audio_extensions() -> &'static [&'static str] {
    &["mp3", "wav", "ogg", "flac", "aac", "m4a", "wma", "opus"]
}

fn font_extensions() -> &'static [&'static str] {
    &["ttf", "otf", "woff", "woff2", "eot"]
}

fn office_extensions() -> &'static [&'static str] {
    &[
        "docx", "xlsx", "pptx", "doc", "xls", "ppt", "odt", "ods", "odp",
    ]
}

fn archive_extensions() -> &'static [&'static str] {
    &["zip", "jar", "war", "ear", "apk", "xpi", "crx", "epub"]
}

fn markdown_extensions() -> &'static [&'static str] {
    &["md", "markdown", "mdx"]
}

fn text_extensions() -> &'static [&'static str] {
    &["txt", "log", "ini", "conf", "cfg", "env"]
}

fn shell_config_file(file_name: &str) -> bool {
    matches!(
        file_name,
        ".bashrc"
            | ".bash_profile"
            | ".bash_login"
            | ".bash_logout"
            | ".bash_aliases"
            | ".zshrc"
            | ".zshenv"
            | ".zprofile"
            | ".zlogin"
            | ".zlogout"
            | ".profile"
            | ".tcshrc"
            | ".cshrc"
            | ".kshrc"
            | ".fishrc"
            | ".vimrc"
            | ".gvimrc"
            | ".exrc"
            | ".nanorc"
            | ".gitconfig"
            | ".gitignore"
            | ".gitattributes"
            | ".editorconfig"
            | ".prettierrc"
            | ".eslintrc"
            | ".stylelintrc"
            | ".npmrc"
            | ".yarnrc"
            | ".nvmrc"
            | ".node-version"
            | ".python-version"
            | ".env"
            | ".env.local"
            | ".env.development"
            | ".env.production"
            | ".htaccess"
            | ".dockerignore"
            | ".hgignore"
            | "Makefile"
            | "Dockerfile"
            | "Vagrantfile"
            | "Procfile"
            | "Gemfile"
            | "Rakefile"
            | "CMakeLists.txt"
            | "Cargo.toml"
            | "package.json"
            | "tsconfig.json"
    )
}

fn language_for_extension(ext: &str, file_name: &str) -> Option<String> {
    if shell_config_file(file_name) {
        return Some("bash".to_string());
    }
    let language = match ext {
        "js" => "javascript",
        "jsx" => "jsx",
        "ts" => "typescript",
        "tsx" => "tsx",
        "py" => "python",
        "rs" => "rust",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "kt" => "kotlin",
        "scala" => "scala",
        "sh" | "bash" | "zsh" => "bash",
        "fish" => "fish",
        "ps1" => "powershell",
        "sql" => "sql",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" => "scss",
        "sass" => "sass",
        "less" => "less",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "vue" => "vue",
        "svelte" => "svelte",
        _ => return None,
    };
    Some(language.to_string())
}

pub fn mime_type_for_extension(ext: &str) -> String {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "opus" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" | "aac" => "audio/mp4",
        "wma" => "audio/x-ms-wma",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "zip" | "jar" | "war" | "ear" | "apk" | "xpi" | "crx" | "epub" => "application/zip",
        "js" => "text/javascript",
        "ts" => "text/typescript",
        "json" => "application/json",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "md" | "markdown" | "mdx" => "text/markdown",
        "txt" | "log" | "ini" | "conf" | "cfg" | "env" => "text/plain",
        "py" => "text/x-python",
        "rs" => "text/x-rust",
        "go" => "text/x-go",
        "java" => "text/x-java",
        "c" | "h" => "text/x-c",
        "cpp" | "hpp" | "cc" => "text/x-c++",
        "sh" | "bash" | "zsh" => "text/x-shellscript",
        "yaml" | "yml" => "text/yaml",
        "toml" => "text/x-toml",
        _ => "application/octet-stream",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_ignores_single_dotfiles() {
        assert_eq!(local_file_extension(".zshrc"), "");
        assert_eq!(local_file_extension("archive.tar.gz"), "gz");
    }

    #[test]
    fn mime_table_keeps_media_and_text_types() {
        assert_eq!(mime_type_for_extension("png"), "image/png");
        assert_eq!(mime_type_for_extension("mp4"), "video/mp4");
        assert_eq!(mime_type_for_extension("rs"), "text/x-rust");
        assert_eq!(
            mime_type_for_extension("unknown"),
            "application/octet-stream"
        );
    }
}
