// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use std::io::{Read, Seek, SeekFrom};

const CHUNK: usize = 32 * 1024;

fn download_version(metadata: &fs::Metadata) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        format!(
            "{}:{}:{}:{}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        )
    }
    #[cfg(not(unix))]
    {
        format!(
            "{:?}:{:?}",
            metadata.created().ok(),
            metadata.modified().ok()
        )
    }
}

// The request loop owns the stage until commit, cancellation or end of input.
pub(super) struct Upload {
    id: String,
    path: PathBuf,
    stage: PathBuf,
    file: File,
}

impl Drop for Upload {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.stage);
    }
}

pub(super) fn dispatch(
    method: &str,
    mut args: Value,
    active: &mut Option<Upload>,
) -> Result<Value, RpcError> {
    let invalid = || rpc_error(ERR_INVALID_PARAMS, "Invalid transfer parameters");
    let io = |e| map_io_error(e, "transfer");
    if method == "transfer/read" {
        let path = args["path"].as_str().ok_or_else(invalid)?;
        let offset = args["offset"].as_u64().ok_or_else(invalid)?;
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NONBLOCK);
        let mut source = options.open(path).map_err(io)?;
        let metadata = source.metadata().map_err(io)?;
        if !metadata.is_file() {
            return Err(rpc_error(ERR_INVALID_PARAMS, "Expected a regular file"));
        }
        let version = download_version(&metadata);
        source.seek(SeekFrom::Start(offset)).map_err(io)?;
        let mut bytes = Zeroizing::new(Vec::with_capacity(CHUNK));
        (&mut source)
            .take(CHUNK as u64)
            .read_to_end(&mut bytes)
            .map_err(io)?;
        if download_version(&source.metadata().map_err(io)?) != version {
            return Err(rpc_error(ERR_IO, "Remote file changed during download"));
        }
        return Ok(json!({
            "size": metadata.len(),
            "version": version,
            "content": base64::engine::general_purpose::STANDARD.encode(&*bytes),
        }));
    }
    let id = args["id"].as_str().ok_or_else(invalid)?.to_string();
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid());
    }
    if method == "transfer/begin" {
        if active.is_some() {
            return Err(invalid());
        }
        let path = PathBuf::from(args["path"].as_str().ok_or_else(invalid)?);
        let stage = path.with_file_name(format!(".oxideterm-{id}.tmp"));
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let file = options.open(&stage).map_err(io)?;
        *active = Some(Upload {
            id,
            path,
            stage,
            file,
        });
        return Ok(Value::Null);
    }
    let upload = active
        .as_mut()
        .filter(|upload| upload.id == id)
        .ok_or_else(invalid)?;
    match method {
        "transfer/append" => {
            let Value::String(content) = args["content"].take() else {
                return Err(invalid());
            };
            let content = Zeroizing::new(content);
            if content.len() > CHUNK.div_ceil(3) * 4 {
                return Err(invalid());
            }
            let bytes = Zeroizing::new(
                base64::engine::general_purpose::STANDARD
                    .decode(content.as_bytes())
                    .map_err(|_| invalid())?,
            );
            if bytes.len() > CHUNK
                || upload.file.stream_position().map_err(io)?
                    != args["offset"].as_u64().ok_or_else(invalid)?
            {
                return Err(invalid());
            }
            upload.file.write_all(&bytes).map_err(io)?;
        }
        "transfer/commit" => {
            if upload.file.stream_position().map_err(io)?
                != args["size"].as_u64().ok_or_else(invalid)?
            {
                return Err(invalid());
            }
            upload.file.sync_all().map_err(io)?;
            if args["overwrite"].as_bool().ok_or_else(invalid)? {
                match fs::symlink_metadata(&upload.path) {
                    Ok(metadata) if metadata.is_file() => {
                        ide::preserve_file_attributes(&upload.file, &metadata).map_err(io)?;
                    }
                    Ok(_) => return Err(invalid()),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(io(error)),
                }
                fs::rename(&upload.stage, &upload.path).map_err(io)?;
            } else {
                fs::hard_link(&upload.stage, &upload.path).map_err(io)?;
            }
            active.take();
        }
        "transfer/abort" => {
            active.take();
        }
        _ => {
            return Err(rpc_error(
                ERR_METHOD_NOT_FOUND,
                "Unknown transfer operation",
            ));
        }
    }
    Ok(Value::Null)
}
