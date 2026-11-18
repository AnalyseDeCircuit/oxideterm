use serde_json::{Value, json};

use super::{
    AiActionResultLite, AiOrchestratorRuntimeSnapshot, ai_runtime_validation_recovery_message,
    ai_transfer_path_looks_directory,
};
use crate::tool_result_protocol::truncate_for_model;

pub struct AiFileRead<'a> {
    pub path: &'a str,
    pub ide: bool,
    pub directory: bool,
}

pub struct AiFileWrite<'a> {
    pub path: &'a str,
    pub content: &'a str,
    pub expected_hash: Option<&'a str>,
    pub ide: bool,
}

#[derive(Clone, Copy)]
pub enum AiTransferDirection {
    Upload,
    Download,
}

impl AiTransferDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Upload => "upload",
            Self::Download => "download",
        }
    }
}

pub struct AiFileTransfer<'a> {
    pub direction: AiTransferDirection,
    pub source: &'a str,
    pub destination: &'a str,
    pub directory: bool,
    pub id: String,
}

impl AiOrchestratorRuntimeSnapshot {
    pub fn decode_file_read<'a>(
        &self,
        args: &'a Value,
    ) -> Result<AiFileRead<'a>, AiActionResultLite> {
        let resource = args
            .get("resource")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(path) = resource_path(args, "path") else {
            return Err(self.fail(
                "Resource path is required.",
                "missing_path",
                "read_resource requires path for file or directory resources.",
                "read",
            ));
        };
        Ok(AiFileRead {
            path,
            ide: resource == "ide",
            directory: matches!(resource, "directory" | "sftp"),
        })
    }

    pub fn decode_file_write<'a>(
        &self,
        args: &'a Value,
    ) -> Result<AiFileWrite<'a>, AiActionResultLite> {
        let (Some(path), Some(content)) = (
            resource_path(args, "path"),
            args.get("content").and_then(Value::as_str),
        ) else {
            return Err(self.fail(
                "Path and content are required.",
                "missing_file_write_args",
                "write_resource(file) requires path and content.",
                "write",
            ));
        };
        if args
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(self
                .ok(
                    format!("Dry-run file write {path}."),
                    "Dry-run only; file was not changed.",
                    Value::Null,
                    "write",
                )
                .with_verified(false));
        }
        Ok(AiFileWrite {
            path,
            content,
            expected_hash: resource_path(args, "expected_hash"),
            ide: args.get("resource").and_then(Value::as_str) == Some("ide"),
        })
    }

    pub fn decode_file_transfer<'a>(
        &self,
        args: &'a Value,
    ) -> Result<AiFileTransfer<'a>, AiActionResultLite> {
        let direction = match args.get("direction").and_then(Value::as_str) {
            Some("upload") => AiTransferDirection::Upload,
            Some("download") => AiTransferDirection::Download,
            _ => {
                return Err(self.fail(
                    "Transfer direction is required.",
                    "missing_transfer_direction",
                    "direction must be upload or download.",
                    "write",
                ));
            }
        };
        let (Some(source), Some(destination)) = (
            resource_path(args, "source_path"),
            resource_path(args, "destination_path"),
        ) else {
            return Err(self.fail(
                "Transfer paths are required.",
                "missing_transfer_path",
                "transfer_resource requires source_path and destination_path.",
                "write",
            ));
        };
        Ok(AiFileTransfer {
            direction,
            source,
            destination,
            id: uuid::Uuid::new_v4().to_string(),
            directory: ai_transfer_path_looks_directory(source)
                || ai_transfer_path_looks_directory(destination),
        })
    }

    pub fn missing_ide_file_owner(&self, write: bool) -> AiActionResultLite {
        self.fail(
            "IDE capability is unavailable.",
            "runtime_capability_unavailable",
            if write {
                "Rediscover the current IDE workspace before writing a file."
            } else {
                "Rediscover the current IDE workspace before reading a file."
            },
            if write { "write" } else { "read" },
        )
    }

    pub fn missing_sftp_owner(&self, write: bool) -> AiActionResultLite {
        self.fail(
            "SFTP capability is unavailable.",
            "runtime_capability_unavailable",
            if write {
                "Rediscover the current SFTP session before writing."
            } else {
                "Rediscover the current SFTP session before reading."
            },
            if write { "write" } else { "read" },
        )
    }

    pub fn ide_file_owner_unavailable(&self, write: bool) -> AiActionResultLite {
        self.fail(
            if write {
                "IDE file write failed."
            } else {
                "IDE file read failed."
            },
            "resource_disconnected",
            "The current IDE file owner is unavailable. Rediscover it before retrying.",
            if write { "write" } else { "read" },
        )
    }

    pub fn ide_file_read(
        &self,
        path: &str,
        content: String,
        hash: String,
        size: u64,
        mtime: u64,
        encoding: String,
    ) -> AiActionResultLite {
        let output = truncate_for_model(content.clone(), 12_000);
        let data = json!({ "path": path, "content": content, "hash": hash, "contentHash": hash,
            "size": size, "mtime": mtime, "encoding": encoding, "source": "ide-surface-agent" });
        self.ok(format!("Read IDE file {path}."), output, data, "read")
    }

    pub fn ide_file_written(
        &self,
        path: &str,
        hash: String,
        size: u64,
        mtime: u64,
        atomic: bool,
    ) -> AiActionResultLite {
        let data = json!({ "path": path, "size": size, "mtime": mtime, "hash": hash,
            "contentHash": hash, "atomicWrite": atomic, "source": "ide-surface-agent" });
        self.ok(
            format!("Wrote IDE file {path}."),
            serde_json::to_string_pretty(&data).unwrap_or_else(|_| format!("{path} written.")),
            data,
            "write",
        )
    }

    pub fn sftp_file_read(&self, request: AiFileRead<'_>, data: Value) -> AiActionResultLite {
        let output = truncate_for_model(
            serde_json::to_string_pretty(&data).unwrap_or_default(),
            12_000,
        );
        let summary = if request.directory {
            format!("Listed {} entries.", data.as_array().map_or(0, Vec::len))
        } else {
            format!("Read remote file preview {}.", request.path)
        };
        self.ok(summary, output, data, "read")
    }

    pub fn sftp_session_changed(
        &self,
        operation: AiSftpOperation,
        during_operation: bool,
        post_user_approval: bool,
    ) -> AiActionResultLite {
        let (summary, risk) = match (operation, during_operation) {
            (AiSftpOperation::Read, false) => ("SFTP session changed before the read.", "read"),
            (AiSftpOperation::Read, true) => ("SFTP session changed during the read.", "read"),
            (AiSftpOperation::Write, false) => ("SFTP session changed before the write.", "write"),
            (AiSftpOperation::Write, true) => ("SFTP session changed during the write.", "write"),
            (AiSftpOperation::Transfer, _) => {
                ("SFTP session changed before transfer start.", "write")
            }
        };
        self.fail(
            summary,
            if post_user_approval {
                "runtime_state_changed_after_approval"
            } else if during_operation {
                "resource_disconnected"
            } else {
                "runtime_owner_replaced"
            },
            ai_runtime_validation_recovery_message(post_user_approval),
            risk,
        )
    }

    pub fn sftp_file_written(
        &self,
        path: &str,
        file: AiSftpWrittenFile,
        content: &str,
    ) -> AiActionResultLite {
        let hash = ai_hash_text_content(content, "utf-8");
        let data = json!({ "path": file.path, "size": file.size, "mtime": file.mtime,
            "hash": hash, "contentHash": hash, "atomicWrite": file.atomic });
        self.ok(
            format!("Wrote remote file {path}."),
            serde_json::to_string_pretty(&data).unwrap_or_else(|_| format!("{path} written.")),
            data,
            "write",
        )
    }

    pub fn sftp_write_failed(
        &self,
        error: AiRemoteFileWriteError<impl std::fmt::Display>,
        post_user_approval: bool,
    ) -> AiActionResultLite {
        match error {
            AiRemoteFileWriteError::OwnerReplaced => {
                self.sftp_session_changed(AiSftpOperation::Write, false, post_user_approval)
            }
            AiRemoteFileWriteError::ExpectedHashMismatch => self.fail(
                "Remote file changed before writing.",
                "expected_hash_mismatch",
                "File changed before writing. Read the current file before retrying.",
                "write",
            ),
            AiRemoteFileWriteError::ExpectedFileMissing { path } => self.fail(
                "Cannot verify write precondition.",
                "expected_file_missing",
                format!("Cannot verify write precondition for {path}: file does not exist."),
                "write",
            ),
            AiRemoteFileWriteError::ExistingFileNotText { path } => self.fail(
                "Cannot verify existing file.",
                "existing_file_not_text",
                format!("Cannot safely verify existing file {path}: it is not valid UTF-8 text."),
                "write",
            ),
            AiRemoteFileWriteError::Sftp(error) => self.fail(
                "Remote file write failed.",
                "remote_file_write_failed",
                error.to_string(),
                "write",
            ),
        }
    }

    pub fn sftp_transfer_result(
        &self,
        request: &AiFileTransfer<'_>,
        transfer: AiTransferOutcome,
    ) -> AiActionResultLite {
        let direction = request.direction.as_str();
        let data = match transfer {
            AiTransferOutcome::File { bytes } => json!({
                "transferId": request.id, "direction": direction, "sourcePath": request.source,
                "destinationPath": request.destination, "directory": false, "result": { "bytes": bytes },
            }),
            AiTransferOutcome::Directory { strategy, transfer } => json!({
                "transferId": request.id, "strategy": strategy, "transfer": transfer,
            }),
        };
        self.ok(
            if request.directory {
                format!("Started {direction} directory transfer.")
            } else {
                format!("Completed {direction} transfer.")
            },
            serde_json::to_string_pretty(&data)
                .unwrap_or_else(|_| format!("transfer_id={}", request.id)),
            data,
            "write",
        )
    }
}

pub enum AiTransferOutcome {
    File { bytes: u64 },
    Directory { strategy: Value, transfer: Value },
}

pub struct AiSftpWrittenFile {
    pub path: String,
    pub size: u64,
    pub mtime: i64,
    pub atomic: bool,
}
pub enum AiSftpOperation {
    Read,
    Write,
    Transfer,
}

fn resource_path<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

pub fn validate_sftp_write_precondition<E>(
    path: &str,
    mut bytes: zeroize::Zeroizing<Vec<u8>>,
    expected: &str,
) -> Result<(), AiRemoteFileWriteError<E>> {
    let text =
        std::str::from_utf8(&bytes).map_err(|_| AiRemoteFileWriteError::ExistingFileNotText {
            path: path.to_owned(),
        })?;
    let actual = ai_hash_text_content(text, "utf-8");
    zeroize::Zeroize::zeroize(&mut *bytes);
    if actual != expected {
        return Err(AiRemoteFileWriteError::ExpectedHashMismatch);
    }
    Ok(())
}

fn ai_hash_text_content(content: &str, encoding: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(encoding.as_bytes());
    hasher.update([0]);
    hasher.update(content.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}
pub enum AiRemoteFileWriteError<E> {
    OwnerReplaced,
    ExpectedHashMismatch,
    ExpectedFileMissing { path: String },
    ExistingFileNotText { path: String },
    Sftp(E),
}

impl<E> std::fmt::Debug for AiRemoteFileWriteError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Paths, content hashes, and transport details are deliberately absent
        // because this error can cross task and diagnostic boundaries.
        let variant = match self {
            Self::OwnerReplaced => "OwnerReplaced",
            Self::ExpectedHashMismatch => "ExpectedHashMismatch",
            Self::ExpectedFileMissing { .. } => "ExpectedFileMissing",
            Self::ExistingFileNotText { .. } => "ExistingFileNotText",
            Self::Sftp(_) => "Sftp",
        };
        formatter
            .debug_struct("AiRemoteFileWriteError")
            .field("variant", &variant)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_write_error_debug_omits_sensitive_details() {
        let path = "/private/runtime/secret.txt";
        let error = AiRemoteFileWriteError::<String>::ExpectedFileMissing {
            path: path.to_string(),
        };

        let debug = format!("{error:?}");

        assert!(debug.contains("ExpectedFileMissing"));
        assert!(!debug.contains(path));
    }
}
