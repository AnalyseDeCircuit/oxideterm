// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::ser::SerializeStruct;
use thiserror::Error;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TrzszErrorCode {
    InvalidApiVersion,
    InvalidOwnerId,
    InvalidPath,
    UnauthorizedPath,
    DirectoryNotAllowed,
    SymlinkNotAllowed,
    ReservedName,
    RootNotPrepared,
    RootMismatch,
    HandleNotFound,
    HandleOwnerMismatch,
    AlreadyExists,
    MaxFileCountExceeded,
    MaxTotalBytesExceeded,
    InvalidState,
    InvalidOffset,
    ChunkTooLarge,
    Io,
}

impl TrzszErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidApiVersion => "invalid_api_version",
            Self::InvalidOwnerId => "invalid_owner_id",
            Self::InvalidPath => "invalid_path",
            Self::UnauthorizedPath => "unauthorized_path",
            Self::DirectoryNotAllowed => "directory_not_allowed",
            Self::SymlinkNotAllowed => "symlink_not_allowed",
            Self::ReservedName => "reserved_name",
            Self::RootNotPrepared => "root_not_prepared",
            Self::RootMismatch => "root_mismatch",
            Self::HandleNotFound => "handle_not_found",
            Self::HandleOwnerMismatch => "handle_owner_mismatch",
            Self::AlreadyExists => "already_exists",
            Self::MaxFileCountExceeded => "max_file_count_exceeded",
            Self::MaxTotalBytesExceeded => "max_total_bytes_exceeded",
            Self::InvalidState => "invalid_state",
            Self::InvalidOffset => "invalid_offset",
            Self::ChunkTooLarge => "chunk_too_large",
            Self::Io => "io_error",
        }
    }
}

#[derive(Debug, Error)]
pub enum TrzszError {
    #[error("Unsupported trzsz apiVersion {got}, expected {expected}")]
    InvalidApiVersion { expected: u32, got: u32 },

    #[error("Invalid ownerId")]
    InvalidOwnerId,

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Path is not authorized for this owner: {0}")]
    UnauthorizedPath(String),

    #[error("Directory upload is not allowed: {0}")]
    DirectoryNotAllowed(String),

    #[error("Symlink is not allowed: {0}")]
    SymlinkNotAllowed(String),

    #[error("Reserved file name is not allowed: {0}")]
    ReservedName(String),

    #[error("Download root is not prepared for this owner")]
    RootNotPrepared,

    #[error("Download root does not match the prepared root")]
    RootMismatch,

    #[error("Handle not found: {0}")]
    HandleNotFound(String),

    #[error("Handle does not belong to the specified owner")]
    HandleOwnerMismatch,

    #[error("Target already exists and overwrite is disabled: {0}")]
    AlreadyExists(String),

    #[error("Selected {selected} files, which exceeds the current limit of {max}")]
    MaxFileCountExceeded { selected: usize, max: usize },

    #[error("Selected {selected} bytes, which exceeds the current limit of {max}")]
    MaxTotalBytesExceeded { selected: u64, max: u64 },

    #[error("Invalid trzsz state: {0}")]
    InvalidState(String),

    #[error("Invalid read offset {offset} for file of size {size}")]
    InvalidOffset { offset: u64, size: u64 },

    #[error("Requested chunk length {requested} exceeds the maximum {max}")]
    ChunkTooLarge { requested: usize, max: usize },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl TrzszError {
    pub const fn code(&self) -> TrzszErrorCode {
        match self {
            Self::InvalidApiVersion { .. } => TrzszErrorCode::InvalidApiVersion,
            Self::InvalidOwnerId => TrzszErrorCode::InvalidOwnerId,
            Self::InvalidPath(_) => TrzszErrorCode::InvalidPath,
            Self::UnauthorizedPath(_) => TrzszErrorCode::UnauthorizedPath,
            Self::DirectoryNotAllowed(_) => TrzszErrorCode::DirectoryNotAllowed,
            Self::SymlinkNotAllowed(_) => TrzszErrorCode::SymlinkNotAllowed,
            Self::ReservedName(_) => TrzszErrorCode::ReservedName,
            Self::RootNotPrepared => TrzszErrorCode::RootNotPrepared,
            Self::RootMismatch => TrzszErrorCode::RootMismatch,
            Self::HandleNotFound(_) => TrzszErrorCode::HandleNotFound,
            Self::HandleOwnerMismatch => TrzszErrorCode::HandleOwnerMismatch,
            Self::AlreadyExists(_) => TrzszErrorCode::AlreadyExists,
            Self::MaxFileCountExceeded { .. } => TrzszErrorCode::MaxFileCountExceeded,
            Self::MaxTotalBytesExceeded { .. } => TrzszErrorCode::MaxTotalBytesExceeded,
            Self::InvalidState(_) => TrzszErrorCode::InvalidState,
            Self::InvalidOffset { .. } => TrzszErrorCode::InvalidOffset,
            Self::ChunkTooLarge { .. } => TrzszErrorCode::ChunkTooLarge,
            Self::Io(_) => TrzszErrorCode::Io,
        }
    }

    pub fn detail(&self) -> Option<String> {
        match self {
            Self::InvalidApiVersion { expected, got } => {
                Some(format!("expected={expected}, got={got}"))
            }
            Self::InvalidPath(path)
            | Self::UnauthorizedPath(path)
            | Self::DirectoryNotAllowed(path)
            | Self::SymlinkNotAllowed(path)
            | Self::ReservedName(path)
            | Self::HandleNotFound(path)
            | Self::AlreadyExists(path)
            | Self::InvalidState(path) => Some(path.clone()),
            Self::MaxFileCountExceeded { selected, max } => {
                Some(format!("selected={selected}, max={max}"))
            }
            Self::MaxTotalBytesExceeded { selected, max } => {
                Some(format!("selected={selected}, max={max}"))
            }
            Self::InvalidOffset { offset, size } => Some(format!("offset={offset}, size={size}")),
            Self::ChunkTooLarge { requested, max } => {
                Some(format!("requested={requested}, max={max}"))
            }
            Self::Io(error) => Some(error.to_string()),
            Self::InvalidOwnerId
            | Self::RootNotPrepared
            | Self::RootMismatch
            | Self::HandleOwnerMismatch => None,
        }
    }
}

impl serde::Serialize for TrzszError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("TrzszErrorDto", 3)?;
        state.serialize_field("code", self.code().as_str())?;
        state.serialize_field("message", &self.to_string())?;
        state.serialize_field("detail", &self.detail())?;
        state.end()
    }
}
