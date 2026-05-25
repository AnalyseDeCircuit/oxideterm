// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Knowledge settings import and dialog models.

use std::{fs, path::Path, path::PathBuf};

pub const KNOWLEDGE_MAX_IMPORT_FILE_SIZE: u64 = 5 * 1024 * 1024;
pub const KNOWLEDGE_IMPORT_EXTENSIONS: &[&str] = &["md", "txt", "markdown"];
pub const KNOWLEDGE_EMBEDDING_BATCH_SIZE: usize = 32;

#[derive(Clone, Debug)]
pub enum KnowledgeDeleteTarget {
    Collection,
    Document,
}

#[derive(Clone, Debug)]
pub struct KnowledgeDeleteConfirm {
    pub target: KnowledgeDeleteTarget,
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct KnowledgeExternalEdit {
    pub doc_id: String,
    pub path: PathBuf,
    pub version: u64,
}

pub fn import_knowledge_file(
    store: &oxideterm_ai::RagStore,
    collection_id: &str,
    path: &Path,
) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > KNOWLEDGE_MAX_IMPORT_FILE_SIZE {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("document");
        return Err(format!(
            "File \"{file_name}\" exceeds 5 MB limit ({} MB)",
            (metadata.len() as f64 / 1024.0 / 1024.0).round() as u64
        ));
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document")
        .to_string();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !KNOWLEDGE_IMPORT_EXTENSIONS.contains(&extension.as_str()) {
        return Err(format!("Unsupported document type: {file_name}"));
    }
    let format = match extension.as_str() {
        "md" | "markdown" => "markdown",
        "txt" => "plaintext",
        _ => "plaintext",
    };
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    oxideterm_ai::rag_add_document(
        store,
        oxideterm_ai::RagAddDocumentRequest {
            collection_id: collection_id.to_string(),
            title: file_name,
            content,
            format: format.to_string(),
            source_path: Some(path.to_string_lossy().to_string()),
        },
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knowledge_extension_allowlist_is_lowercase() {
        assert!(KNOWLEDGE_IMPORT_EXTENSIONS.contains(&"md"));
        assert!(!KNOWLEDGE_IMPORT_EXTENSIONS.contains(&"pdf"));
    }
}
