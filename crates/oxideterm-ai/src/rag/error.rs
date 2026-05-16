// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RagError {
    #[error("Database error: {0}")]
    Database(#[from] redb::DatabaseError),

    #[error("Table error: {0}")]
    Table(#[from] redb::TableError),

    #[error("Transaction error: {0}")]
    Transaction(#[from] redb::TransactionError),

    #[error("Commit error: {0}")]
    Commit(#[from] redb::CommitError),

    #[error("Storage error: {0}")]
    Storage(#[from] redb::StorageError),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Compression error: {0}")]
    Compression(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Collection not found: {0}")]
    CollectionNotFound(String),

    #[error("Document not found: {0}")]
    DocumentNotFound(String),

    #[error("Chunk not found: {0}")]
    ChunkNotFound(String),

    #[error("Operation cancelled")]
    Cancelled,

    #[error("Version conflict: expected {expected}, found {actual}")]
    VersionConflict { expected: u64, actual: u64 },

    #[error("HNSW index error: {0}")]
    HnswIndex(String),

    #[error("Duplicate document: content hash {0} already exists in this collection")]
    DuplicateDocument(String),
}

impl From<rmp_serde::encode::Error> for RagError {
    fn from(e: rmp_serde::encode::Error) -> Self {
        RagError::Serialization(e.to_string())
    }
}

impl From<rmp_serde::decode::Error> for RagError {
    fn from(e: rmp_serde::decode::Error) -> Self {
        RagError::Serialization(e.to_string())
    }
}

impl serde::Serialize for RagError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
