// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use std::io::Read;

const MAX_FRAME_BYTES: u64 = 150 * 1024 * 1024;

/// The PTY bootstrap disables echo and canonical input before entering this loop.
pub(super) fn serve(token: &str) -> io::Result<()> {
    if token.len() != 32 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid session",
        ));
    }
    let mut input = BufReader::new(io::stdin().lock());
    let mut output = io::stdout().lock();
    writeln!(output, "\x1e{token}:READY")?;
    output.flush()?;
    loop {
        let mut line = Zeroizing::new(Vec::new());
        let count = input
            .by_ref()
            .take(MAX_FRAME_BYTES + 1)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            return Ok(());
        }
        if count as u64 > MAX_FRAME_BYTES || !line.ends_with(b"\n") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid frame size",
            ));
        }
        let mut envelope: Value = serde_json::from_slice(&line)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid request"))?;
        if envelope.get("session").and_then(Value::as_str) != Some(token) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Wrong session"));
        }
        envelope.as_object_mut().unwrap().remove("session");
        let request: Request = serde_json::from_value(envelope)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid request"))?;
        let shutdown = request.method == "sys/shutdown";
        let response = handle_request(request);
        let bytes = Zeroizing::new(serde_json::to_vec(&response)?);
        if bytes.len() as u64 > MAX_FRAME_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Response exceeds frame limit",
            ));
        }
        writeln!(output, "\x1e{token}:{}", bytes.len())?;
        output.write_all(&bytes)?;
        output.write_all(b"\n")?;
        output.flush()?;
        if shutdown {
            return Ok(());
        }
    }
}

pub(super) fn project(path: &str) -> Result<Value, RpcError> {
    let path =
        fs::canonicalize(normalize_path(path)).map_err(|e| map_io_error(e, Path::new(path)))?;
    if !path.is_dir() {
        return Err(rpc_error(ERR_INVALID_PARAMS, "Path is not a directory"));
    }
    Ok(
        json!({"rootPath": path.to_string_lossy(), "name": path.file_name().map(|v| v.to_string_lossy()).unwrap_or("/".into()), "isGitRepo": path.join(".git").exists(), "gitBranch": null, "fileCount": 0}),
    )
}

/// File classification must not hash or transfer the entire file before rejecting binary content.
pub(super) fn read_file_prefix(path: &str) -> Result<Value, RpcError> {
    let path = normalize_path(path);
    let file = File::open(&path).map_err(|e| map_io_error(e, &path))?;
    let mut prefix = Zeroizing::new(Vec::new());
    file.take(8192)
        .read_to_end(&mut prefix)
        .map_err(|e| map_io_error(e, &path))?;
    Ok(json!(
        base64::engine::general_purpose::STANDARD.encode(&*prefix)
    ))
}

pub(super) fn create_file(params: WriteFileParams) -> Result<WriteFileResult, RpcError> {
    let path = normalize_path(&params.path);
    let bytes = Zeroizing::new(decode_content(&params.content, &params.encoding)?);
    if bytes.len() as u64 > MAX_EDITABLE_FILE_SIZE {
        return Err(rpc_error(ERR_INVALID_PARAMS, "File exceeds IDE size limit"));
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| map_io_error(e, &path))?;
    file.write_all(&bytes).map_err(|e| map_io_error(e, &path))?;
    let meta = file.metadata().map_err(|e| map_io_error(e, &path))?;
    Ok(WriteFileResult {
        hash: hash_bytes(&bytes),
        size: meta.len(),
        mtime: mtime_secs(&meta),
        atomic: false,
    })
}

#[derive(Deserialize)]
pub(super) struct IdeWriteParams {
    #[serde(flatten)]
    file: WriteFileParams,
    #[serde(default)]
    create_new: bool,
}

/// IDE saves preserve the target owner and permissions without replacing symlinks.
pub(super) fn write_file(params: IdeWriteParams) -> Result<WriteFileResult, RpcError> {
    if params.create_new {
        return create_file(params.file);
    }
    let original = normalize_path(&params.file.path);
    let path = match fs::symlink_metadata(&original) {
        Ok(meta) if meta.file_type().is_symlink() => {
            fs::canonicalize(&original).map_err(|e| map_io_error(e, &original))?
        }
        _ => original,
    };
    let bytes = Zeroizing::new(decode_content(&params.file.content, &params.file.encoding)?);
    if bytes.len() as u64 > MAX_EDITABLE_FILE_SIZE {
        return Err(rpc_error(ERR_INVALID_PARAMS, "File exceeds IDE size limit"));
    }
    if let Some(expected) = params.file.expect_hash {
        let current = Zeroizing::new(
            fs::read(&path)
                .map_err(|_| rpc_error(ERR_ALREADY_EXISTS, "CONFLICT: File no longer readable"))?,
        );
        if hash_bytes(&current) != expected {
            return Err(rpc_error(
                ERR_ALREADY_EXISTS,
                "CONFLICT: File modified externally",
            ));
        }
    }
    static NEXT_SWAP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let parent = path.parent().unwrap_or(Path::new("."));
    let swap = parent.join(format!(
        ".oxideterm-shell-{}-{}.tmp",
        std::process::id(),
        NEXT_SWAP.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let mut file = create_save_temp(&swap).map_err(|e| map_io_error(e, parent))?;
    let result = (|| {
        file.write_all(&bytes).map_err(|e| map_io_error(e, &path))?;
        if let Ok(metadata) = fs::metadata(&path) {
            #[cfg(unix)]
            {
                use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
                let created = file.metadata().map_err(|e| map_io_error(e, &path))?;
                if (created.uid(), created.gid()) != (metadata.uid(), metadata.gid()) {
                    // A privileged shell must preserve the target's owner, rather than
                    // leaving a root-owned replacement after editing another user's file.
                    if unsafe { libc::fchown(file.as_raw_fd(), metadata.uid(), metadata.gid()) }
                        != 0
                    {
                        return Err(map_io_error(io::Error::last_os_error(), &path));
                    }
                }
            }
            file.set_permissions(metadata.permissions())
                .map_err(|e| map_io_error(e, &path))?;
        }
        file.sync_all().map_err(|e| map_io_error(e, &path))?;
        fs::rename(&swap, &path).map_err(|e| map_io_error(e, &path))?;
        let metadata = fs::metadata(&path).map_err(|e| map_io_error(e, &path))?;
        Ok(WriteFileResult {
            hash: hash_bytes(&bytes),
            size: metadata.len(),
            mtime: mtime_secs(&metadata),
            atomic: true,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&swap);
    }
    result
}

fn create_save_temp(path: &Path) -> io::Result<File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Restrict access at creation, before any private content reaches disk.
        options.mode(0o600);
    }
    options.open(path)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn save_temp_is_private_before_content_is_written() {
        let root = env::temp_dir().join(format!("oxideterm-private-save-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let path = root.join("swap");
        let file = create_save_temp(&path).unwrap();
        let mode = file.metadata().unwrap().permissions().mode() & 0o777;
        drop(file);
        fs::remove_dir_all(root).unwrap();
        assert_eq!(mode, 0o600);
    }
}
