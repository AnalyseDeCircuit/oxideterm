// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Knowledge settings import and dialog models.

use std::{fmt, path::PathBuf};
#[cfg(not(target_family = "wasm"))]
use std::{fs, path::Path};

pub const KNOWLEDGE_MAX_IMPORT_FILE_SIZE: u64 = 5 * 1024 * 1024;
pub const KNOWLEDGE_IMPORT_EXTENSIONS: &[&str] = &["md", "txt", "markdown"];
pub const KNOWLEDGE_EMBEDDING_BATCH_SIZE: usize = 32;

#[derive(Clone, Debug)]
pub enum KnowledgeDeleteTarget {
    Collection,
    Document,
}

#[derive(Clone)]
pub struct KnowledgeDeleteConfirm {
    pub target: KnowledgeDeleteTarget,
    pub id: String,
    pub name: String,
}

impl fmt::Debug for KnowledgeDeleteConfirm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KnowledgeDeleteConfirm")
            .field("target", &self.target)
            .field("id", &self.id)
            .field("name", &"<redacted>")
            .finish()
    }
}

#[derive(Clone)]
pub struct KnowledgeExternalEdit {
    pub doc_id: String,
    pub path: PathBuf,
    pub version: u64,
}

impl fmt::Debug for KnowledgeExternalEdit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KnowledgeExternalEdit")
            .field("doc_id", &self.doc_id)
            .field("path", &"<redacted>")
            .field("version", &self.version)
            .finish()
    }
}

#[cfg(not(target_family = "wasm"))]
pub fn import_knowledge_file(
    store: &oxideterm_ai::RagStore,
    collection_id: &str,
    path: &Path,
) -> Result<(), String> {
    oxideterm_ai::rag_add_document(store, read_knowledge_import_file(collection_id, path)?)
        .map(|_| ())
}

#[cfg(not(target_family = "wasm"))]
pub fn read_knowledge_import_file(
    collection_id: &str,
    path: &Path,
) -> Result<oxideterm_ai::RagAddDocumentRequest, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document");
    let mut request = knowledge_import_request(
        collection_id,
        file_name,
        path.extension().and_then(|ext| ext.to_str()),
        metadata.len(),
        || fs::read_to_string(path).map_err(|error| error.to_string()),
    )?;
    request.source_path = Some(path.to_string_lossy().into_owned());
    Ok(request)
}

pub fn prepare_knowledge_import_text(
    collection_id: &str,
    name: &str,
    content: String,
) -> Result<oxideterm_ai::RagAddDocumentRequest, String> {
    knowledge_import_request(
        collection_id,
        name,
        std::path::Path::new(name)
            .extension()
            .and_then(|ext| ext.to_str()),
        content.len() as u64,
        || Ok(content),
    )
}

fn knowledge_import_request(
    collection_id: &str,
    file_name: &str,
    extension: Option<&str>,
    size: u64,
    read: impl FnOnce() -> Result<String, String>,
) -> Result<oxideterm_ai::RagAddDocumentRequest, String> {
    if size > KNOWLEDGE_MAX_IMPORT_FILE_SIZE {
        return Err(format!(
            "File \"{file_name}\" exceeds 5 MB limit ({} MB)",
            (size as f64 / 1024.0 / 1024.0).round() as u64
        ));
    }

    let extension = extension.unwrap_or_default().to_ascii_lowercase();
    if !KNOWLEDGE_IMPORT_EXTENSIONS.contains(&extension.as_str()) {
        return Err(format!("Unsupported document type: {file_name}"));
    }
    let format = match extension.as_str() {
        "md" | "markdown" => "markdown",
        "txt" => "plaintext",
        _ => "plaintext",
    };
    Ok(oxideterm_ai::RagAddDocumentRequest {
        collection_id: collection_id.to_string(),
        title: file_name.to_string(),
        content: read()?,
        format: format.to_string(),
        source_path: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knowledge_debug_output_redacts_user_names_and_paths() {
        let secret = "private-customer-secret";
        let confirm = KnowledgeDeleteConfirm {
            target: KnowledgeDeleteTarget::Document,
            id: "document-id".to_string(),
            name: secret.to_string(),
        };
        let edit = KnowledgeExternalEdit {
            doc_id: "document-id".to_string(),
            path: PathBuf::from(format!("/tmp/{secret}.md")),
            version: 1,
        };

        assert!(!format!("{confirm:?}").contains(secret));
        assert!(!format!("{edit:?}").contains(secret));
    }
}
