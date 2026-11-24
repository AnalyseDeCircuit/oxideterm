use std::path::Path;

pub const MAX_PREVIEW_SIZE: u64 = 10 * 1024 * 1024;
pub const STREAM_PREVIEW_THRESHOLD: u64 = 256 * 1024;

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
        "sqlite" | "sqlite3" | "db" | "db3" => "application/vnd.sqlite3",
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
