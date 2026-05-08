// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use base64::{Engine, engine::general_purpose::STANDARD};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use serde_json::{Value, json};

use crate::buffer::TrzszBuffer;
use crate::escape::{EscapeCode, escape_chars_to_codes, escape_data, unescape_data};
use crate::types::TRZSZ_PROTOCOL_VERSION;
use crate::{TextProgressBar, TrzszError};

pub const DEFAULT_MAX_DATA_CHUNK_SIZE: usize = 10 * 1024 * 1024;

pub trait TrzszFileReader: Send {
    fn close_file(&mut self);
    fn path_id(&self) -> u64;
    fn rel_path(&self) -> &[String];
    fn is_dir(&self) -> bool;
    fn size(&self) -> u64;
    fn read_file(&mut self, max_len: usize) -> Result<Vec<u8>, TrzszError>;
}

pub trait TrzszFileWriter: Send {
    fn close_file(&mut self);
    fn file_name(&self) -> &str;
    fn local_name(&self) -> &str;
    fn is_dir(&self) -> bool;
    fn write_file(&mut self, data: &[u8]) -> Result<(), TrzszError>;
    fn delete_file(&mut self) -> Result<String, TrzszError>;
    fn commit_file(&mut self) -> Result<(), TrzszError> {
        Ok(())
    }
    fn finish_file(&mut self) -> Result<(), TrzszError> {
        Ok(())
    }
    fn abort_file(&mut self) -> Result<(), TrzszError> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct TrzszSaveParam {
    pub root_path: String,
    pub display_name: String,
}

pub struct TrzszTransfer {
    buffer: TrzszBuffer,
    writer: Box<dyn Fn(Vec<u8>) + Send + Sync>,
    is_windows_shell: bool,
    max_data_chunk_size: usize,
    remote_is_windows: bool,
    last_input_time: Instant,
    tmux_output_junk: bool,
    clean_timeout: Duration,
    transfer_config: Value,
    stopped: bool,
    max_chunk_time: Duration,
    protocol_newline: &'static str,
}

#[derive(Clone, Debug)]
pub struct TrzszTransferInput {
    buffer: TrzszBuffer,
}

impl TrzszTransferInput {
    pub fn add_received_data(&self, data: &[u8]) {
        self.buffer.add_buffer(data);
    }

    pub fn stop_transferring(&self) {
        self.buffer.stop_buffer();
    }
}

impl TrzszTransfer {
    pub fn new(
        writer: impl Fn(Vec<u8>) + Send + Sync + 'static,
        is_windows_shell: bool,
        max_data_chunk_size: usize,
    ) -> Self {
        Self {
            buffer: TrzszBuffer::default(),
            writer: Box::new(writer),
            is_windows_shell,
            max_data_chunk_size,
            remote_is_windows: false,
            last_input_time: Instant::now(),
            tmux_output_junk: false,
            clean_timeout: Duration::from_millis(100),
            transfer_config: Value::Object(Default::default()),
            stopped: false,
            max_chunk_time: Duration::ZERO,
            protocol_newline: "\n",
        }
    }

    pub fn add_received_data(&mut self, data: &[u8]) {
        if !self.stopped {
            self.buffer.add_buffer(data);
        }
        self.last_input_time = Instant::now();
    }

    pub fn input_handle(&self) -> TrzszTransferInput {
        TrzszTransferInput {
            buffer: self.buffer.clone(),
        }
    }

    pub fn stop_transferring(&mut self) {
        self.clean_timeout = (self.max_chunk_time * 2).max(Duration::from_millis(500));
        self.stopped = true;
        self.buffer.stop_buffer();
    }

    pub fn set_remote_platform(&mut self, remote_is_windows: bool) {
        if remote_is_windows {
            self.remote_is_windows = true;
            self.protocol_newline = "!\n";
        }
    }

    pub fn send_action(
        &mut self,
        confirm: bool,
        remote_is_windows: bool,
    ) -> Result<(), TrzszError> {
        let mut action = json!({
            "lang": "js",
            "confirm": confirm,
            "version": TRZSZ_PROTOCOL_VERSION,
            "support_dir": true,
        });

        if self.is_windows_shell || remote_is_windows {
            action["binary"] = Value::Bool(false);
            action["newline"] = Value::String("!\n".to_string());
        }
        if remote_is_windows {
            self.set_remote_platform(true);
        }

        self.send_string("ACT", &action.to_string())
    }

    pub fn recv_config(&mut self) -> Result<Value, TrzszError> {
        let buffer = self.recv_string("CFG", true)?;
        self.transfer_config = serde_json::from_str(&buffer)
            .map_err(|error| TrzszError::InvalidState(error.to_string()))?;
        self.tmux_output_junk = self.transfer_config["tmux_output_junk"] == Value::Bool(true);
        Ok(self.transfer_config.clone())
    }

    pub fn client_exit(&mut self, message: &str) -> Result<(), TrzszError> {
        self.send_string("EXIT", message)
    }

    pub fn client_error(&mut self, error: &TrzszError) -> Result<(), TrzszError> {
        self.clean_input(self.clean_timeout);
        self.send_string("FAIL", &error.to_string())
    }

    pub fn send_files(
        &mut self,
        mut files: Vec<Box<dyn TrzszFileReader>>,
        progress: Option<&mut TextProgressBar>,
    ) -> Result<Vec<String>, TrzszError> {
        let binary = self.config_bool("binary");
        let directory = self.config_bool("directory");
        let max_buffer_size = self
            .config_u64("bufsize")
            .map(|value| value as usize)
            .unwrap_or(self.max_data_chunk_size)
            .min(self.max_data_chunk_size);
        let escape_codes = self.config_escape_codes();

        let mut progress = progress;
        self.send_file_num(files.len(), progress.as_deref_mut())?;

        let mut remote_names = Vec::new();
        for file in &mut files {
            let remote_name =
                self.send_file_name(file.as_mut(), directory, progress.as_deref_mut())?;
            if !remote_names.contains(&remote_name) {
                remote_names.push(remote_name);
            }

            if file.is_dir() {
                continue;
            }

            let size = file.size();
            self.send_file_size(size, progress.as_deref_mut())?;
            let digest = self.send_file_data(
                file.as_mut(),
                size,
                binary,
                &escape_codes,
                max_buffer_size,
                progress.as_deref_mut(),
            )?;
            file.close_file();
            self.send_file_md5(&digest, progress.as_deref_mut())?;
        }

        Ok(remote_names)
    }

    pub fn recv_files(
        &mut self,
        save_param: &TrzszSaveParam,
        mut open_save_file: impl FnMut(
            &TrzszSaveParam,
            &str,
            bool,
            bool,
        ) -> Result<Box<dyn TrzszFileWriter>, TrzszError>,
        progress: Option<&mut TextProgressBar>,
    ) -> Result<Vec<String>, TrzszError> {
        let binary = self.config_bool("binary");
        let directory = self.config_bool("directory");
        let overwrite = false;
        let timeout = self
            .config_u64("timeout")
            .map(|seconds| Duration::from_secs(seconds.max(1)))
            .unwrap_or(Duration::from_secs(100));
        let escape_codes = self.config_escape_codes();

        let mut progress = progress;
        let num = self.recv_file_num(progress.as_deref_mut())?;
        let mut local_names = Vec::new();
        let mut created_files: Vec<Box<dyn TrzszFileWriter>> = Vec::new();

        for _ in 0..num {
            let mut file = self.recv_file_name(
                save_param,
                &mut open_save_file,
                directory,
                overwrite,
                progress.as_deref_mut(),
            )?;
            if !local_names.iter().any(|name| name == file.local_name()) {
                local_names.push(file.local_name().to_string());
            }

            if !file.is_dir() {
                let size = self.recv_file_size(progress.as_deref_mut())?;
                match self.recv_file_data(
                    file.as_mut(),
                    size,
                    binary,
                    &escape_codes,
                    timeout,
                    progress.as_deref_mut(),
                ) {
                    Ok(digest) => {
                        file.close_file();
                        self.recv_file_md5(&digest, progress.as_deref_mut())?;
                        file.finish_file()?;
                    }
                    Err(error) => {
                        let _ = file.abort_file();
                        return Err(error);
                    }
                }
            }

            created_files.push(file);
        }

        for file in &mut created_files {
            file.commit_file()?;
        }

        Ok(local_names)
    }

    fn clean_input(&mut self, _timeout: Duration) {
        self.stopped = true;
        self.buffer.drain_buffer();
        self.last_input_time = Instant::now();
    }

    fn send_line(&self, kind: &str, buffer: impl AsRef<[u8]>) {
        let mut line = Vec::new();
        line.extend_from_slice(format!("#{kind}:").as_bytes());
        line.extend_from_slice(buffer.as_ref());
        line.extend_from_slice(self.protocol_newline.as_bytes());
        (self.writer)(line);
    }

    fn recv_line(&self, expect_type: &str, may_have_junk: bool) -> Result<String, TrzszError> {
        if self.stopped {
            return Err(TrzszError::InvalidState("Stopped".to_string()));
        }

        if self.is_windows_shell || self.remote_is_windows {
            let line = self.buffer.read_line_on_windows()?;
            if let Some(index) = line.rfind(&format!("#{expect_type}:")) {
                return Ok(line[index..].to_string());
            }
            let fallback_index = line.rfind('#');
            return Ok(if fallback_index.is_some_and(|index| index > 0) {
                line[fallback_index.expect("checked")..].to_string()
            } else {
                line
            });
        }

        let mut line = self.buffer.read_line()?;
        if self.tmux_output_junk || may_have_junk {
            while line.ends_with('\r') {
                line.pop();
                line.push_str(&self.buffer.read_line()?);
            }

            if let Some(index) = line.rfind(&format!("#{expect_type}:")) {
                line = line[index..].to_string();
            } else if let Some(index) = line.rfind('#')
                && index > 0
            {
                line = line[index..].to_string();
            }

            line = strip_tmux_status_line(&line);
        }

        Ok(line)
    }

    fn recv_check(&self, expect_type: &str, may_have_junk: bool) -> Result<String, TrzszError> {
        let line = self.recv_line(expect_type, may_have_junk)?;
        let Some(separator_index) = line.find(':') else {
            return Err(TrzszError::InvalidState(format!(
                "Missing colon in trzsz line: {line}"
            )));
        };
        if separator_index < 1 {
            return Err(TrzszError::InvalidState(format!(
                "Invalid trzsz line: {line}"
            )));
        }

        let kind = &line[1..separator_index];
        let buffer = &line[separator_index + 1..];
        if kind != expect_type {
            return Err(TrzszError::InvalidState(format!("{kind}: {buffer}")));
        }
        Ok(buffer.to_string())
    }

    fn send_integer(&self, kind: &str, value: u64) {
        self.send_line(kind, value.to_string());
    }

    fn recv_integer(&self, kind: &str, may_have_junk: bool) -> Result<u64, TrzszError> {
        self.recv_check(kind, may_have_junk)?
            .parse()
            .map_err(|error| TrzszError::InvalidState(format!("Invalid integer: {error}")))
    }

    fn check_integer(&self, expect: u64) -> Result<(), TrzszError> {
        let result = self.recv_integer("SUCC", false)?;
        if result != expect {
            return Err(TrzszError::InvalidState(format!(
                "Integer check [{result}] <> [{expect}]"
            )));
        }
        Ok(())
    }

    fn send_string(&self, kind: &str, value: &str) -> Result<(), TrzszError> {
        self.send_line(kind, encode_buffer(value.as_bytes())?);
        Ok(())
    }

    fn recv_string(&self, kind: &str, may_have_junk: bool) -> Result<String, TrzszError> {
        let decoded = decode_buffer(&self.recv_check(kind, may_have_junk)?)?;
        String::from_utf8(decoded).map_err(|error| TrzszError::InvalidState(error.to_string()))
    }

    fn send_binary(&self, kind: &str, buffer: &[u8]) -> Result<(), TrzszError> {
        self.send_line(kind, encode_buffer(buffer)?);
        Ok(())
    }

    fn recv_binary(&self, kind: &str, may_have_junk: bool) -> Result<Vec<u8>, TrzszError> {
        decode_buffer(&self.recv_check(kind, may_have_junk)?)
    }

    fn check_binary(&self, expect: &[u8]) -> Result<(), TrzszError> {
        let result = self.recv_binary("SUCC", false)?;
        if result != expect {
            return Err(TrzszError::InvalidState("Binary check failed".to_string()));
        }
        Ok(())
    }

    fn send_data(
        &self,
        data: &[u8],
        binary: bool,
        escape_codes: &[EscapeCode],
    ) -> Result<(), TrzszError> {
        if !binary {
            return self.send_binary("DATA", data);
        }

        let escaped = escape_data(data, escape_codes);
        self.send_line("DATA", escaped.len().to_string());
        (self.writer)(escaped);
        Ok(())
    }

    fn recv_data(
        &mut self,
        binary: bool,
        escape_codes: &[EscapeCode],
        _timeout: Duration,
    ) -> Result<Vec<u8>, TrzszError> {
        if !binary {
            return self.recv_binary("DATA", false);
        }

        let size = self.recv_integer("DATA", false)? as usize;
        Ok(unescape_data(&self.buffer.read_binary(size)?, escape_codes))
    }

    fn send_file_num(
        &self,
        num: usize,
        progress: Option<&mut TextProgressBar>,
    ) -> Result<(), TrzszError> {
        self.send_integer("NUM", num as u64);
        self.check_integer(num as u64)?;
        if let Some(progress) = progress {
            progress.on_num(num);
        }
        Ok(())
    }

    fn send_file_name(
        &self,
        file: &mut dyn TrzszFileReader,
        directory: bool,
        progress: Option<&mut TextProgressBar>,
    ) -> Result<String, TrzszError> {
        let rel_path = file.rel_path();
        let file_name = rel_path
            .last()
            .ok_or_else(|| TrzszError::InvalidPath("empty trzsz rel path".to_string()))?;
        if directory {
            self.send_string(
                "NAME",
                &json!({
                    "path_id": file.path_id(),
                    "path_name": rel_path,
                    "is_dir": file.is_dir(),
                })
                .to_string(),
            )?;
        } else {
            self.send_string("NAME", file_name)?;
        }

        let remote_name = self.recv_string("SUCC", false)?;
        if let Some(progress) = progress {
            progress.on_name(file_name);
        }
        Ok(remote_name)
    }

    fn send_file_size(
        &self,
        size: u64,
        progress: Option<&mut TextProgressBar>,
    ) -> Result<(), TrzszError> {
        self.send_integer("SIZE", size);
        self.check_integer(size)?;
        if let Some(progress) = progress {
            progress.on_size(size);
        }
        Ok(())
    }

    fn send_file_data(
        &mut self,
        file: &mut dyn TrzszFileReader,
        size: u64,
        binary: bool,
        escape_codes: &[EscapeCode],
        max_buffer_size: usize,
        progress: Option<&mut TextProgressBar>,
    ) -> Result<[u8; 16], TrzszError> {
        let mut progress = progress;
        let mut step = 0;
        if let Some(progress) = progress.as_deref_mut() {
            progress.on_step(step);
        }

        let mut buffer_size = 1024;
        let mut context = md5::Context::new();
        while step < size {
            let begin_time = Instant::now();
            let data = file.read_file(buffer_size)?;
            if data.is_empty() {
                return Err(TrzszError::InvalidState(format!(
                    "Unexpected EOF while reading {}",
                    file.rel_path().join("/")
                )));
            }
            self.send_data(&data, binary, escape_codes)?;
            context.consume(&data);
            self.check_integer(data.len() as u64)?;
            step += data.len() as u64;
            if let Some(progress) = progress.as_deref_mut() {
                progress.on_step(step);
            }

            let chunk_time = begin_time.elapsed();
            if data.len() == buffer_size
                && chunk_time < Duration::from_millis(500)
                && buffer_size < max_buffer_size
            {
                buffer_size = (buffer_size * 2).min(max_buffer_size);
            } else if chunk_time >= Duration::from_millis(2000) && buffer_size > 1024 {
                buffer_size = 1024;
            }
            self.max_chunk_time = self.max_chunk_time.max(chunk_time);
        }

        Ok(context.compute().0)
    }

    fn send_file_md5(
        &self,
        digest: &[u8; 16],
        progress: Option<&mut TextProgressBar>,
    ) -> Result<(), TrzszError> {
        self.send_binary("MD5", digest)?;
        self.check_binary(digest)?;
        if let Some(progress) = progress {
            progress.on_done();
        }
        Ok(())
    }

    fn recv_file_num(&self, progress: Option<&mut TextProgressBar>) -> Result<usize, TrzszError> {
        let num = self.recv_integer("NUM", false)? as usize;
        self.send_integer("SUCC", num as u64);
        if let Some(progress) = progress {
            progress.on_num(num);
        }
        Ok(num)
    }

    fn recv_file_name(
        &self,
        save_param: &TrzszSaveParam,
        open_save_file: &mut impl FnMut(
            &TrzszSaveParam,
            &str,
            bool,
            bool,
        ) -> Result<Box<dyn TrzszFileWriter>, TrzszError>,
        directory: bool,
        overwrite: bool,
        progress: Option<&mut TextProgressBar>,
    ) -> Result<Box<dyn TrzszFileWriter>, TrzszError> {
        let file_name = self.recv_string("NAME", false)?;
        let file = open_save_file(save_param, &file_name, directory, overwrite)?;
        self.send_string("SUCC", file.local_name())?;
        if let Some(progress) = progress {
            progress.on_name(file.file_name());
        }
        Ok(file)
    }

    fn recv_file_size(&self, progress: Option<&mut TextProgressBar>) -> Result<u64, TrzszError> {
        let size = self.recv_integer("SIZE", false)?;
        self.send_integer("SUCC", size);
        if let Some(progress) = progress {
            progress.on_size(size);
        }
        Ok(size)
    }

    fn recv_file_data(
        &mut self,
        file: &mut dyn TrzszFileWriter,
        size: u64,
        binary: bool,
        escape_codes: &[EscapeCode],
        timeout: Duration,
        progress: Option<&mut TextProgressBar>,
    ) -> Result<[u8; 16], TrzszError> {
        let mut progress = progress;
        let mut step = 0;
        if let Some(progress) = progress.as_deref_mut() {
            progress.on_step(step);
        }
        let mut context = md5::Context::new();
        while step < size {
            let begin_time = Instant::now();
            let data = self.recv_data(binary, escape_codes, timeout)?;
            file.write_file(&data)?;
            step += data.len() as u64;
            if let Some(progress) = progress.as_deref_mut() {
                progress.on_step(step);
            }
            self.send_integer("SUCC", data.len() as u64);
            context.consume(&data);
            self.max_chunk_time = self.max_chunk_time.max(begin_time.elapsed());
        }
        Ok(context.compute().0)
    }

    fn recv_file_md5(
        &self,
        digest: &[u8; 16],
        progress: Option<&mut TextProgressBar>,
    ) -> Result<(), TrzszError> {
        let expected_digest = self.recv_binary("MD5", false)?;
        if digest != expected_digest.as_slice() {
            return Err(TrzszError::InvalidState("Check MD5 failed".to_string()));
        }

        self.send_binary("SUCC", digest)?;
        if let Some(progress) = progress {
            progress.on_done();
        }
        Ok(())
    }

    fn config_bool(&self, key: &str) -> bool {
        self.transfer_config[key] == Value::Bool(true)
    }

    fn config_u64(&self, key: &str) -> Option<u64> {
        self.transfer_config[key].as_u64()
    }

    fn config_escape_codes(&self) -> Vec<EscapeCode> {
        let Some(values) = self.transfer_config["escape_chars"].as_array() else {
            return Vec::new();
        };
        let escape_chars: Vec<Vec<String>> = values
            .iter()
            .filter_map(|value| {
                value.as_array().map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect()
                })
            })
            .collect();
        escape_chars_to_codes(&escape_chars)
    }
}

fn encode_buffer(buffer: &[u8]) -> Result<String, TrzszError> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(buffer)?;
    Ok(STANDARD.encode(encoder.finish()?))
}

#[cfg(test)]
pub mod tests_support {
    pub fn encode_buffer_for_tests(buffer: &[u8]) -> String {
        super::encode_buffer(buffer).expect("encode trzsz fixture")
    }
}

fn decode_buffer(buffer: &str) -> Result<Vec<u8>, TrzszError> {
    let bytes = STANDARD
        .decode(buffer)
        .map_err(|error| TrzszError::InvalidState(error.to_string()))?;
    let mut decoder = ZlibDecoder::new(bytes.as_slice());
    let mut output = Vec::new();
    decoder.read_to_end(&mut output)?;
    Ok(output)
}

fn strip_tmux_status_line(buffer: &str) -> String {
    let mut next_buffer = buffer.to_string();
    loop {
        let Some(begin_index) = next_buffer.find("\x1bP=") else {
            return next_buffer;
        };
        let mut buffer_index = begin_index + 3;
        let Some(mid_index) = next_buffer[buffer_index..].find("\x1bP=") else {
            return next_buffer[..begin_index].to_string();
        };
        buffer_index += mid_index + 3;
        let Some(end_index) = next_buffer[buffer_index..].find("\x1b\\") else {
            return next_buffer[..begin_index].to_string();
        };
        buffer_index += end_index + 2;
        next_buffer = format!(
            "{}{}",
            &next_buffer[..begin_index],
            &next_buffer[buffer_index..]
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Debug)]
    struct MemoryReader {
        rel_path: Vec<String>,
        data: Vec<u8>,
        offset: usize,
    }

    impl TrzszFileReader for MemoryReader {
        fn close_file(&mut self) {}
        fn path_id(&self) -> u64 {
            1
        }
        fn rel_path(&self) -> &[String] {
            &self.rel_path
        }
        fn is_dir(&self) -> bool {
            false
        }
        fn size(&self) -> u64 {
            self.data.len() as u64
        }
        fn read_file(&mut self, max_len: usize) -> Result<Vec<u8>, TrzszError> {
            let end = (self.offset + max_len).min(self.data.len());
            let data = self.data[self.offset..end].to_vec();
            self.offset = end;
            Ok(data)
        }
    }

    #[test]
    fn sends_action_and_receives_config() {
        let output = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        let output_clone = output.clone();
        let mut transfer = TrzszTransfer::new(
            move |data| output_clone.lock().unwrap().push(data),
            false,
            DEFAULT_MAX_DATA_CHUNK_SIZE,
        );
        transfer.send_action(true, false).unwrap();
        let action_line = String::from_utf8(output.lock().unwrap()[0].clone()).unwrap();
        assert!(action_line.starts_with("#ACT:"));
        assert!(action_line.ends_with('\n'));

        let config = json!({"binary": false, "directory": false});
        let encoded = encode_buffer(config.to_string().as_bytes()).unwrap();
        transfer.add_received_data(format!("#CFG:{encoded}\n").as_bytes());
        assert_eq!(transfer.recv_config().unwrap(), config);
    }

    #[test]
    fn send_files_writes_protocol_and_reads_success_acks() {
        let output = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        let output_clone = output.clone();
        let mut transfer = TrzszTransfer::new(
            move |data| output_clone.lock().unwrap().push(data),
            false,
            DEFAULT_MAX_DATA_CHUNK_SIZE,
        );
        transfer.transfer_config = json!({"binary": false, "directory": false});
        let remote_name = encode_buffer(b"remote.txt").unwrap();
        let digest = md5::compute(b"hello").0;
        transfer.add_received_data(format!("#SUCC:1\n#SUCC:{remote_name}\n#SUCC:5\n").as_bytes());
        transfer.add_received_data(b"#SUCC:5\n");
        transfer
            .add_received_data(format!("#SUCC:{}\n", encode_buffer(&digest).unwrap()).as_bytes());

        let names = transfer
            .send_files(
                vec![Box::new(MemoryReader {
                    rel_path: vec!["hello.txt".to_string()],
                    data: b"hello".to_vec(),
                    offset: 0,
                })],
                None,
            )
            .unwrap();
        assert_eq!(names, vec!["remote.txt"]);
        let writes = output.lock().unwrap();
        assert!(
            String::from_utf8(writes[0].clone())
                .unwrap()
                .starts_with("#NUM:1")
        );
        assert!(
            String::from_utf8(writes[1].clone())
                .unwrap()
                .starts_with("#NAME:")
        );
        assert!(
            String::from_utf8(writes[2].clone())
                .unwrap()
                .starts_with("#SIZE:5")
        );
        assert!(
            String::from_utf8(writes[3].clone())
                .unwrap()
                .starts_with("#DATA:")
        );
        assert!(
            String::from_utf8(writes[4].clone())
                .unwrap()
                .starts_with("#MD5:")
        );
    }
}
