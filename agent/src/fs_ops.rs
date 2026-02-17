//! File system operations — POSIX-native, no SFTP limitations.
//!
//! All operations use standard library I/O with real POSIX semantics:
//! - `rename` atomically overwrites the target
//! - `canonicalize` works on any existing path component
//! - Recursive operations use native directory walking

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::protocol::*;

// ═══════════════════════════════════════════════════════════════════════════
// Base64 decoder (minimal, no external crate)
// ═══════════════════════════════════════════════════════════════════════════

/// Decode standard base64 (RFC 4648) to bytes.
fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    const TABLE: [u8; 256] = {
        let mut t = [0xFFu8; 256];
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut i = 0;
        while i < 64 {
            t[alphabet[i] as usize] = i as u8;
            i += 1;
        }
        t
    };

    let bytes: Vec<u8> = input.bytes().filter(|&b| b != b'\n' && b != b'\r' && b != b' ').collect();
    if bytes.len() % 4 != 0 {
        return Err("Invalid base64 length".into());
    }

    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut buf = [0u8; 4];
        let mut pad = 0;
        for (i, &b) in chunk.iter().enumerate() {
            if b == b'=' {
                buf[i] = 0;
                pad += 1;
            } else {
                let val = TABLE[b as usize];
                if val == 0xFF {
                    return Err(format!("Invalid base64 character: {}", b as char));
                }
                buf[i] = val;
            }
        }
        let triple = ((buf[0] as u32) << 18) | ((buf[1] as u32) << 12) | ((buf[2] as u32) << 6) | (buf[3] as u32);
        out.push((triple >> 16) as u8);
        if pad < 2 { out.push((triple >> 8) as u8); }
        if pad < 1 { out.push(triple as u8); }
    }
    Ok(out)
}

/// Encode bytes to standard base64 (RFC 4648).
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18 & 0x3F) as usize] as char);
        out.push(ALPHABET[(triple >> 12 & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(triple >> 6 & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Compute SHA-256 hex digest of a byte slice (minimal, no external crate).
///
/// This is a pure-Rust SHA-256 implementation to avoid adding dependencies.
/// Performance is adequate for the file sizes we handle (<10MB).
pub fn sha256_hex(data: &[u8]) -> String {
    // SHA-256 constants
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
        0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
        0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
        0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
        0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
        0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    // Pre-processing: padding
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    // Process each 512-bit block
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    h.iter()
        .map(|x| format!("{:08x}", x))
        .collect::<Vec<_>>()
        .join("")
}

/// Get mtime as unix timestamp (seconds since epoch).
fn mtime_secs(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Get permissions as octal string (e.g. "755").
fn perms_octal(metadata: &fs::Metadata) -> String {
    format!("{:o}", metadata.permissions().mode() & 0o7777)
}

/// Classify file type from metadata.
fn file_type_str(metadata: &fs::Metadata) -> &'static str {
    let ft = metadata.file_type();
    if ft.is_dir() {
        "directory"
    } else if ft.is_symlink() {
        "symlink"
    } else if ft.is_file() {
        "file"
    } else {
        "other"
    }
}

fn map_io_error(e: &io::Error) -> (i32, String) {
    match e.kind() {
        io::ErrorKind::NotFound => (ERR_NOT_FOUND, e.to_string()),
        io::ErrorKind::PermissionDenied => (ERR_PERMISSION, e.to_string()),
        io::ErrorKind::AlreadyExists => (ERR_ALREADY_EXISTS, e.to_string()),
        _ => (ERR_IO, e.to_string()),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Operations
// ═══════════════════════════════════════════════════════════════════════════

/// Read file content + compute hash.
pub fn read_file(params: ReadFileParams) -> Result<ReadFileResult, (i32, String)> {
    let path = Path::new(&params.path);
    let metadata = fs::metadata(path).map_err(|e| map_io_error(&e))?;

    if !metadata.is_file() {
        return Err((ERR_IO, format!("Not a regular file: {}", params.path)));
    }

    let size = metadata.len();
    if size > params.max_size {
        return Err((
            ERR_IO,
            format!(
                "File too large: {} bytes (max: {} bytes)",
                size, params.max_size
            ),
        ));
    }

    let mut file = fs::File::open(path).map_err(|e| map_io_error(&e))?;
    let mut content_bytes = Vec::with_capacity(size as usize);
    file.read_to_end(&mut content_bytes)
        .map_err(|e| map_io_error(&e))?;

    let hash = sha256_hex(&content_bytes);
    let mtime = mtime_secs(&metadata);

    // Compress large files (>32KB) with zstd for faster transfer over SSH
    const COMPRESS_THRESHOLD: u64 = 32 * 1024;
    if size > COMPRESS_THRESHOLD {
        if let Ok(compressed) = zstd::stream::encode_all(content_bytes.as_slice(), 3) {
            // Only use compression if it actually saves space
            if compressed.len() < content_bytes.len() {
                return Ok(ReadFileResult {
                    content: base64_encode(&compressed),
                    hash,
                    size,
                    mtime,
                    encoding: "zstd+base64".to_string(),
                });
            }
        }
    }

    let content = String::from_utf8_lossy(&content_bytes).into_owned();

    Ok(ReadFileResult {
        content,
        hash,
        size,
        mtime,
        encoding: "plain".to_string(),
    })
}

/// Write file content with POSIX atomic rename.
///
/// Strategy:
/// 1. Write to a temporary file in the same directory
/// 2. `std::fs::rename()` atomically replaces the target (POSIX guarantee)
///
/// This is the key advantage over SFTP: POSIX rename **always** overwrites
/// the target atomically, no need for remove-then-rename workarounds.
pub fn write_file(params: WriteFileParams) -> Result<WriteFileResult, (i32, String)> {
    let path = Path::new(&params.path);

    // Optimistic lock: if caller provided expected hash, verify it
    if let Some(ref expected_hash) = params.expect_hash {
        if let Ok(metadata) = fs::metadata(path) {
            if metadata.is_file() {
                let mut existing = Vec::new();
                if let Ok(mut f) = fs::File::open(path) {
                    let _ = f.read_to_end(&mut existing);
                }
                let current_hash = sha256_hex(&existing);
                if &current_hash != expected_hash {
                    return Err((
                        ERR_CONFLICT,
                        format!(
                            "CONFLICT: File modified externally (expected hash: {}, actual: {})",
                            expected_hash, current_hash
                        ),
                    ));
                }
            }
        }
    }

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            return Err((
                ERR_NOT_FOUND,
                format!("Parent directory does not exist: {}", parent.display()),
            ));
        }
    }

    // Decode content based on encoding
    let content_bytes = match params.encoding.as_str() {
        "zstd+base64" => {
            let compressed = base64_decode(&params.content)
                .map_err(|e| (ERR_INVALID_PARAMS, format!("Base64 decode error: {}", e)))?;
            zstd::stream::decode_all(compressed.as_slice())
                .map_err(|e| (ERR_IO, format!("Zstd decompress error: {}", e)))?
        }
        "plain" | "" => params.content.into_bytes(),
        other => {
            return Err((
                ERR_INVALID_PARAMS,
                format!("Unsupported encoding: {}", other),
            ));
        }
    };

    // Write to temp file in the same directory (same filesystem for rename)
    let parent = path.parent().unwrap_or(Path::new("/"));
    let temp_name = format!(
        ".{}.oxtmp.{}",
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".into()),
        std::process::id()
    );
    let temp_path = parent.join(&temp_name);

    // Write content to temp file
    {
        let mut file = fs::File::create(&temp_path).map_err(|e| map_io_error(&e))?;
        file.write_all(&content_bytes)
            .map_err(|e| map_io_error(&e))?;
        file.sync_all().map_err(|e| map_io_error(&e))?;
    }

    // Preserve original permissions if the file already exists
    if let Ok(original_meta) = fs::metadata(path) {
        let _ = fs::set_permissions(&temp_path, original_meta.permissions());
    }

    // Atomic rename: POSIX guarantees this overwrites the target
    fs::rename(&temp_path, path).map_err(|e| {
        // Clean up temp file on failure
        let _ = fs::remove_file(&temp_path);
        map_io_error(&e)
    })?;

    // Read back metadata
    let metadata = fs::metadata(path).map_err(|e| map_io_error(&e))?;
    let hash = sha256_hex(&content_bytes);

    Ok(WriteFileResult {
        hash,
        size: metadata.len(),
        mtime: mtime_secs(&metadata),
        atomic: true,
    })
}

/// Get file/directory metadata.
pub fn stat(params: StatParams) -> Result<StatResult, (i32, String)> {
    let path = Path::new(&params.path);

    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(StatResult {
            exists: true,
            file_type: Some(file_type_str(&metadata).to_string()),
            size: Some(metadata.len()),
            mtime: Some(mtime_secs(&metadata)),
            permissions: Some(perms_octal(&metadata)),
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(StatResult {
            exists: false,
            file_type: None,
            size: None,
            mtime: None,
            permissions: None,
        }),
        Err(e) => Err(map_io_error(&e)),
    }
}

/// List directory contents (single level).
pub fn list_dir(params: ListDirParams) -> Result<Vec<FileEntry>, (i32, String)> {
    let path = Path::new(&params.path);
    let mut entries = Vec::new();

    let read_dir = fs::read_dir(path).map_err(|e| map_io_error(&e))?;

    for entry_result in read_dir {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        let name = entry.file_name().to_string_lossy().to_string();
        let entry_path = entry.path();

        // Use symlink_metadata to not follow symlinks
        let metadata = match fs::symlink_metadata(&entry_path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        entries.push(FileEntry {
            name,
            path: entry_path.to_string_lossy().to_string(),
            file_type: file_type_str(&metadata).to_string(),
            size: metadata.len(),
            mtime: Some(mtime_secs(&metadata)),
            permissions: Some(perms_octal(&metadata)),
            children: None,
        });
    }

    // Sort: directories first, then alphabetical
    entries.sort_by(|a, b| {
        let a_is_dir = a.file_type == "directory";
        let b_is_dir = b.file_type == "directory";
        match (a_is_dir, b_is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(entries)
}

/// Recursive directory listing with depth and count limits.
pub fn list_tree(params: ListTreeParams) -> Result<ListTreeResult, (i32, String)> {
    let path = Path::new(&params.path);
    let mut count: u32 = 0;
    let entries = list_tree_recursive(path, 0, params.max_depth, params.max_entries, &mut count)?;
    let truncated = count >= params.max_entries;
    Ok(ListTreeResult {
        entries,
        truncated,
        total_scanned: count,
    })
}

fn list_tree_recursive(
    dir: &Path,
    depth: u32,
    max_depth: u32,
    max_entries: u32,
    count: &mut u32,
) -> Result<Vec<FileEntry>, (i32, String)> {
    let read_dir = fs::read_dir(dir).map_err(|e| map_io_error(&e))?;
    let mut entries = Vec::new();

    for entry_result in read_dir {
        if *count >= max_entries {
            break;
        }

        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden dirs that are typically large/irrelevant
        if name == ".git" || name == "node_modules" || name == ".hg" || name == "__pycache__" {
            // Still include the entry, but don't recurse into it
            let entry_path = entry.path();
            if let Ok(metadata) = fs::symlink_metadata(&entry_path) {
                *count += 1;
                entries.push(FileEntry {
                    name,
                    path: entry_path.to_string_lossy().to_string(),
                    file_type: file_type_str(&metadata).to_string(),
                    size: metadata.len(),
                    mtime: Some(mtime_secs(&metadata)),
                    permissions: Some(perms_octal(&metadata)),
                    children: None, // Don't recurse
                });
            }
            continue;
        }

        let entry_path = entry.path();
        let metadata = match fs::symlink_metadata(&entry_path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        *count += 1;

        let children = if metadata.is_dir() && depth < max_depth {
            match list_tree_recursive(&entry_path, depth + 1, max_depth, max_entries, count) {
                Ok(c) => Some(c),
                Err(_) => None, // Permission errors etc. — just omit children
            }
        } else {
            None
        };

        entries.push(FileEntry {
            name,
            path: entry_path.to_string_lossy().to_string(),
            file_type: file_type_str(&metadata).to_string(),
            size: metadata.len(),
            mtime: Some(mtime_secs(&metadata)),
            permissions: Some(perms_octal(&metadata)),
            children,
        });
    }

    // Sort: directories first, then alphabetical
    entries.sort_by(|a, b| {
        let a_is_dir = a.file_type == "directory";
        let b_is_dir = b.file_type == "directory";
        match (a_is_dir, b_is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(entries)
}

/// Create directory (optionally recursive).
pub fn mkdir(params: MkdirParams) -> Result<(), (i32, String)> {
    let path = Path::new(&params.path);
    if params.recursive {
        fs::create_dir_all(path).map_err(|e| map_io_error(&e))
    } else {
        fs::create_dir(path).map_err(|e| map_io_error(&e))
    }
}

/// Remove file or directory.
pub fn remove(params: RemoveParams) -> Result<(), (i32, String)> {
    let path = Path::new(&params.path);
    let metadata = fs::symlink_metadata(path).map_err(|e| map_io_error(&e))?;

    if metadata.is_dir() {
        if params.recursive {
            fs::remove_dir_all(path).map_err(|e| map_io_error(&e))
        } else {
            fs::remove_dir(path).map_err(|e| map_io_error(&e))
        }
    } else {
        fs::remove_file(path).map_err(|e| map_io_error(&e))
    }
}

/// Rename/move file or directory (POSIX atomic overwrite).
pub fn rename(params: RenameParams) -> Result<(), (i32, String)> {
    fs::rename(&params.old_path, &params.new_path).map_err(|e| map_io_error(&e))
}

/// Change file permissions.
pub fn chmod(params: ChmodParams) -> Result<(), (i32, String)> {
    let mode = u32::from_str_radix(&params.mode, 8).map_err(|_| {
        (
            ERR_INVALID_PARAMS,
            format!("Invalid permission mode: {}", params.mode),
        )
    })?;
    let path = Path::new(&params.path);
    let perms = fs::Permissions::from_mode(mode);
    fs::set_permissions(path, perms).map_err(|e| map_io_error(&e))
}

/// Search files using grep-like functionality (pure Rust, no external grep).
pub fn grep(params: GrepParams) -> Result<Vec<GrepMatch>, (i32, String)> {
    let root = PathBuf::from(&params.path);
    let mut results = Vec::new();
    grep_recursive(&root, &params, &mut results)?;
    Ok(results)
}

fn grep_recursive(
    dir: &Path,
    params: &GrepParams,
    results: &mut Vec<GrepMatch>,
) -> Result<(), (i32, String)> {
    if results.len() >= params.max_results as usize {
        return Ok(());
    }

    let read_dir = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(()), // Skip unreadable directories
    };

    for entry_result in read_dir {
        if results.len() >= params.max_results as usize {
            return Ok(());
        }

        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        let name = entry.file_name().to_string_lossy().to_string();

        // Skip ignored patterns
        if params.ignore.iter().any(|ig| name == *ig)
            || name == ".git"
            || name == "node_modules"
            || name == ".hg"
            || name == "__pycache__"
            || name == "target"
        {
            continue;
        }

        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if metadata.is_dir() {
            grep_recursive(&path, params, results)?;
        } else if metadata.is_file() && metadata.len() < 1_000_000 {
            // Only search files < 1MB
            grep_file(&path, params, results);
        }
    }

    Ok(())
}

fn grep_file(path: &Path, params: &GrepParams, results: &mut Vec<GrepMatch>) {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return, // Skip binary/unreadable files
    };

    // Note: is_regex field is parsed but full regex matching requires a regex crate.
    // Currently we use simple string matching for all patterns.
    // TODO: Add regex crate for true regex support.

    let pattern = if params.case_sensitive {
        params.pattern.clone()
    } else {
        params.pattern.to_lowercase()
    };

    for (line_idx, line) in content.lines().enumerate() {
        if results.len() >= params.max_results as usize {
            return;
        }

        let search_line = if params.case_sensitive {
            line.to_string()
        } else {
            line.to_lowercase()
        };

        // Find all matches in line
        let mut search_from = 0;
        while search_from < search_line.len() {
            if let Some(col) = search_line[search_from..].find(&pattern) {
                results.push(GrepMatch {
                    path: path.to_string_lossy().to_string(),
                    line: (line_idx + 1) as u32,
                    column: (search_from + col + 1) as u32,
                    text: line.to_string(),
                });
                // Move past this match to find the next one
                search_from += col + pattern.len().max(1);
                if results.len() >= params.max_results as usize {
                    return;
                }
            } else {
                break;
            }
        }
    }
}

/// Get git status for a project directory.
pub fn git_status(params: GitStatusParams) -> Result<GitStatusResult, (i32, String)> {
    let path = Path::new(&params.path);

    // Read branch from .git/HEAD
    let head_path = path.join(".git/HEAD");
    let branch = match fs::read_to_string(&head_path) {
        Ok(content) => {
            if let Some(branch) = content.trim().strip_prefix("ref: refs/heads/") {
                branch.to_string()
            } else {
                // Detached HEAD
                content.trim().chars().take(7).collect()
            }
        }
        Err(_) => "unknown".to_string(),
    };

    // Run git status --porcelain
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain", "-uall"])
        .current_dir(path)
        .output();

    let files = match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .filter_map(|line| {
                    if line.len() < 4 {
                        return None;
                    }
                    let status = line[..2].trim().to_string();
                    let file_path = line[3..].to_string();
                    Some(GitFileEntry {
                        path: file_path,
                        status,
                    })
                })
                .collect()
        }
        _ => Vec::new(),
    };

    Ok(GitStatusResult { branch, files })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_empty() {
        let hash = sha256_hex(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_hello() {
        let hash = sha256_hex(b"hello");
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }
}
