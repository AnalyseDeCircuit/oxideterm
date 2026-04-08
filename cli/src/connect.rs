// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! IPC connection to the OxideTerm GUI process.
//!
//! - macOS/Linux: Unix Domain Socket at `~/.oxideterm/oxt.sock`
//! - Windows: Named Pipe at `\\.\pipe\OxideTerm-CLI-{username}`

use crate::protocol;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);
const MAX_RESPONSE_BYTES: usize = 4_194_304;

/// A connection to the running OxideTerm GUI.
pub struct IpcConnection {
    #[cfg(unix)]
    stream: std::os::unix::net::UnixStream,
    #[cfg(windows)]
    stream: PipeStream,
    pending: Vec<u8>,
}

#[cfg(windows)]
struct PipeStream {
    handle: std::fs::File,
}

impl IpcConnection {
    /// Connect to the running OxideTerm GUI.
    pub fn connect(custom_path: Option<&str>, timeout_ms: u64) -> Result<Self, String> {
        #[cfg(unix)]
        let timeout = std::time::Duration::from_millis(timeout_ms);
        #[cfg(not(unix))]
        let _ = timeout_ms;

        #[cfg(unix)]
        {
            let path = if let Some(p) = custom_path {
                std::path::PathBuf::from(p)
            } else if let Ok(p) = std::env::var("OXIDETERM_SOCK") {
                std::path::PathBuf::from(p)
            } else {
                dirs::home_dir()
                    .ok_or("Cannot determine home directory")?
                    .join(".oxideterm")
                    .join("oxt.sock")
            };

            if !path.exists() {
                return Err(format!(
                    "OxideTerm is not running (socket not found: {})\n\
                     Start OxideTerm first, or use 'oxt list connections' for saved data.",
                    path.display()
                ));
            }

            // Verify socket ownership matches current user (prevent interception)
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let metadata =
                    std::fs::metadata(&path).map_err(|e| format!("Cannot stat socket: {e}"))?;
                let socket_uid = metadata.uid();
                let current_uid = unsafe { libc::getuid() };
                if socket_uid != current_uid {
                    return Err(format!(
                        "Socket ownership mismatch: socket owned by uid {socket_uid}, \
                         but current user is uid {current_uid}. \
                         This may indicate a security issue."
                    ));
                }
            }

            let stream = std::os::unix::net::UnixStream::connect(&path)
                .map_err(|e| format!("Failed to connect to OxideTerm: {e}"))?;
            stream
                .set_read_timeout(Some(timeout))
                .map_err(|e| format!("Failed to set timeout: {e}"))?;
            stream
                .set_write_timeout(Some(timeout))
                .map_err(|e| format!("Failed to set timeout: {e}"))?;

            Ok(Self {
                stream,
                pending: Vec::new(),
            })
        }

        #[cfg(windows)]
        {
            let pipe_name = if let Some(p) = custom_path {
                p.to_string()
            } else if let Ok(p) = std::env::var("OXIDETERM_PIPE") {
                p
            } else {
                format!(r"\\.\pipe\OxideTerm-CLI-{}", whoami::username())
            };

            use std::fs::OpenOptions;
            let handle = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&pipe_name)
                .map_err(|e| {
                    format!(
                        "OxideTerm is not running (pipe not found: {pipe_name})\n\
                         Start OxideTerm first. Error: {e}"
                    )
                })?;

            Ok(Self {
                stream: PipeStream { handle },
                pending: Vec::new(),
            })
        }
    }

    /// Send a JSON-RPC request and wait for the response.
    pub fn call(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let req = protocol::Request::new(id, method, params);

        let mut buf = serde_json::to_vec(&req).map_err(|e| format!("Serialize error: {e}"))?;
        buf.push(b'\n');

        self.write_all(&buf)?;
        self.flush()?;

        let mut ignore_chunks = |_text: &str| {};
        self.read_response(id, &mut ignore_chunks)
    }

    fn write_all(&mut self, buf: &[u8]) -> Result<(), String> {
        #[cfg(unix)]
        {
            self.stream
                .write_all(buf)
                .map_err(|e| format!("Write error: {e}"))
        }
        #[cfg(windows)]
        {
            self.stream
                .handle
                .write_all(buf)
                .map_err(|e| format!("Write error: {e}"))
        }
    }

    fn flush(&mut self) -> Result<(), String> {
        #[cfg(unix)]
        {
            self.stream.flush().map_err(|e| format!("Flush error: {e}"))
        }
        #[cfg(windows)]
        {
            self.stream
                .handle
                .flush()
                .map_err(|e| format!("Flush error: {e}"))
        }
    }

    fn read_response<F>(
        &mut self,
        expected_id: u64,
        on_chunk: &mut F,
    ) -> Result<serde_json::Value, String>
    where
        F: FnMut(&str),
    {
        #[cfg(unix)]
        {
            let stream = &mut self.stream;
            let pending = &mut self.pending;
            read_response_from_reader(stream, pending, expected_id, on_chunk)
        }
        #[cfg(windows)]
        {
            let handle = &mut self.stream.handle;
            let pending = &mut self.pending;
            read_response_from_reader(handle, pending, expected_id, on_chunk)
        }
    }

    /// Send a JSON-RPC request and read streaming notifications.
    ///
    /// The server may send notifications (lines without `id` but with `method`)
    /// before the final response (line with matching `id`). Each notification
    /// with method `stream_chunk` has `params.text` which is passed to `on_chunk`.
    ///
    /// On Unix, the read timeout is temporarily extended to 180s to accommodate
    /// slow first-token latency from AI APIs, then restored when done.
    pub fn call_streaming<F>(
        &mut self,
        method: &str,
        params: serde_json::Value,
        mut on_chunk: F,
    ) -> Result<serde_json::Value, String>
    where
        F: FnMut(&str),
    {
        let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let req = protocol::Request::new(id, method, params);

        let mut buf = serde_json::to_vec(&req).map_err(|e| format!("Serialize error: {e}"))?;
        buf.push(b'\n');
        self.write_all(&buf)?;
        self.flush()?;

        // Extend read timeout for streaming (AI APIs may take >30s for first token)
        #[cfg(unix)]
        const STREAMING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);
        #[cfg(unix)]
        let original_timeout = self.stream.read_timeout().ok().flatten();
        #[cfg(unix)]
        self.stream
            .set_read_timeout(Some(STREAMING_TIMEOUT))
            .map_err(|e| format!("Failed to set streaming timeout: {e}"))?;

        let result = self.read_response(id, &mut on_chunk);

        // Restore original read timeout
        #[cfg(unix)]
        {
            let _ = self.stream.set_read_timeout(original_timeout);
        }

        result
    }
}

fn read_response_from_reader<R, F>(
    reader: &mut R,
    pending: &mut Vec<u8>,
    expected_id: u64,
    on_chunk: &mut F,
) -> Result<serde_json::Value, String>
where
    R: Read,
    F: FnMut(&str),
{
    loop {
        let line = read_line_from_reader(reader, pending)?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let obj: serde_json::Value =
            serde_json::from_str(trimmed).map_err(|e| format!("Invalid response: {e}"))?;

        if obj.get("method").is_some() && obj.get("id").is_none() {
            let method_name = obj.get("method").and_then(|v| v.as_str()).unwrap_or("");
            if method_name == "stream_chunk" {
                if let Some(text) = obj
                    .get("params")
                    .and_then(|p| p.get("text"))
                    .and_then(|t| t.as_str())
                {
                    on_chunk(text);
                }
            }
            continue;
        }

        let resp: protocol::Response =
            serde_json::from_value(obj).map_err(|e| format!("Invalid response: {e}"))?;

        if resp.id != expected_id {
            continue;
        }

        if let Some(err) = resp.error {
            return Err(format!("[{}] {}", err.code, err.message));
        }

        return resp
            .result
            .ok_or_else(|| "Empty response from server".to_string());
    }
}

fn read_line_from_reader<R: Read>(reader: &mut R, pending: &mut Vec<u8>) -> Result<String, String> {
    loop {
        if let Some(line) = take_complete_line(pending)? {
            return Ok(line);
        }

        if pending.len() > MAX_RESPONSE_BYTES {
            return Err(format!(
                "Response exceeded {} bytes without a newline",
                MAX_RESPONSE_BYTES
            ));
        }

        let mut chunk = [0u8; 4096];
        let read = reader
            .read(&mut chunk)
            .map_err(|e| format!("Read error (is OxideTerm running?): {e}"))?;

        if read == 0 {
            if pending.is_empty() {
                return Err("Connection closed while waiting for response".to_string());
            }
            return Err("Connection closed before a full response line was received".to_string());
        }

        pending.extend_from_slice(&chunk[..read]);
    }
}

fn take_complete_line(pending: &mut Vec<u8>) -> Result<Option<String>, String> {
    let Some(newline_idx) = pending.iter().position(|byte| *byte == b'\n') else {
        return Ok(None);
    };

    if newline_idx + 1 > MAX_RESPONSE_BYTES {
        return Err(format!(
            "Response exceeded {} bytes before newline",
            MAX_RESPONSE_BYTES
        ));
    }

    let line_bytes: Vec<u8> = pending.drain(..=newline_idx).collect();
    let line =
        String::from_utf8(line_bytes).map_err(|e| format!("Response was not valid UTF-8: {e}"))?;
    Ok(Some(line))
}

#[cfg(test)]
mod tests {
    use super::{read_line_from_reader, read_response_from_reader, MAX_RESPONSE_BYTES};
    use std::io::{self, Cursor, Read};

    struct ChunkedReader {
        chunks: Vec<Vec<u8>>,
        index: usize,
    }

    impl ChunkedReader {
        fn new(chunks: Vec<&[u8]>) -> Self {
            Self {
                chunks: chunks.into_iter().map(|chunk| chunk.to_vec()).collect(),
                index: 0,
            }
        }
    }

    impl Read for ChunkedReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.index >= self.chunks.len() {
                return Ok(0);
            }

            let chunk = &self.chunks[self.index];
            let len = chunk.len().min(buf.len());
            buf[..len].copy_from_slice(&chunk[..len]);
            self.index += 1;
            Ok(len)
        }
    }

    #[test]
    fn preserves_buffered_lines_across_reads() {
        let mut reader = Cursor::new(
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"first\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":\"second\"}\n"
                .to_vec(),
        );
        let mut pending = Vec::new();

        let first = read_line_from_reader(&mut reader, &mut pending).unwrap();
        let second = read_line_from_reader(&mut reader, &mut pending).unwrap();

        assert!(first.contains("\"id\":1"));
        assert!(second.contains("\"id\":2"));
        assert!(pending.is_empty());
    }

    #[test]
    fn handles_incremental_reads_until_newline() {
        let mut reader = ChunkedReader::new(vec![
            b"{\"jsonrpc\":\"2.0\",",
            b"\"id\":7,\"result\":true}\n",
        ]);
        let mut pending = Vec::new();

        let line = read_line_from_reader(&mut reader, &mut pending).unwrap();

        assert!(line.contains("\"id\":7"));
        assert!(pending.is_empty());
    }

    #[test]
    fn rejects_oversized_line_without_newline() {
        let mut reader = Cursor::new(vec![b'a'; MAX_RESPONSE_BYTES + 1]);
        let mut pending = Vec::new();

        let err = read_line_from_reader(&mut reader, &mut pending).unwrap_err();

        assert!(err.contains("Response exceeded"));
    }

    #[test]
    fn streaming_waits_for_matching_response_id() {
        let payload = concat!(
            "{\"jsonrpc\":\"2.0\",\"method\":\"stream_chunk\",\"params\":{\"text\":\"hel\"}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":41,\"result\":\"stale\"}\n",
            "{\"jsonrpc\":\"2.0\",\"method\":\"stream_chunk\",\"params\":{\"text\":\"lo\"}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":42,\"result\":{\"done\":true}}\n"
        );
        let mut reader = Cursor::new(payload.as_bytes().to_vec());
        let mut pending = Vec::new();
        let mut chunks = Vec::new();

        let result = read_response_from_reader(&mut reader, &mut pending, 42, &mut |text| {
            chunks.push(text.to_string())
        })
        .unwrap();

        assert_eq!(chunks, vec!["hel", "lo"]);
        assert_eq!(result["done"], true);
    }

    #[test]
    fn streaming_returns_matching_error_response() {
        let payload = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":4,\"error\":{\"code\":500,\"message\":\"boom\"}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":5,\"error\":{\"code\":401,\"message\":\"denied\"}}\n"
        );
        let mut reader = Cursor::new(payload.as_bytes().to_vec());
        let mut pending = Vec::new();

        let err =
            read_response_from_reader(&mut reader, &mut pending, 5, &mut |_text| {}).unwrap_err();

        assert_eq!(err, "[401] denied");
    }
}
