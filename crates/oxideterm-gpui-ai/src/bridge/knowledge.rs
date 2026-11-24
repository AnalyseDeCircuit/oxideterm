use oxideterm_ai as ai;

pub struct DocumentMetadata {
    pub id: String,
    pub title: String,
    pub collection_id: String,
    pub version: u64,
}
pub enum KnowledgeSaveError {
    Conflict,
    Failed(String),
}

pub trait AiKnowledgeHost: Send + Sync {
    fn list_collections(
        &self,
        scope_filter: Option<&str>,
    ) -> Result<Vec<ai::RagCollectionResponse>, String>;
    fn list_documents(
        &self,
        collection_id: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<ai::RagPaginatedDocuments, String>;
    fn get_document_content(&self, doc_id: &str) -> Result<String, String>;
    fn create_collection(
        &self,
        request: ai::RagCreateCollectionRequest,
    ) -> Result<ai::RagCollectionResponse, String>;
    fn create_blank_document(
        &self,
        request: ai::RagCreateBlankDocumentRequest,
    ) -> Result<ai::RagDocumentResponse, String>;
    fn delete_collection(&self, collection_id: &str) -> Result<(), String>;
    fn remove_document(&self, doc_id: &str) -> Result<(), String>;
    fn add_document(
        &self,
        request: ai::RagAddDocumentRequest,
    ) -> Result<ai::RagDocumentResponse, String>;
    fn search(
        &self,
        request: ai::RagSearchRequest,
    ) -> Result<Vec<ai::RagSearchResultResponse>, String>;
    fn get_collection_stats(&self, collection_id: &str) -> Result<ai::RagStatsResponse, String>;
    fn keyword_index_state(&self) -> ai::RagKeywordIndexState;
    fn document_semantic_index_state(
        &self,
        doc_id: &str,
    ) -> Result<ai::RagSemanticIndexState, String>;
    fn save_document(
        &self,
        doc_id: &str,
        content: String,
        expected_version: Option<u64>,
    ) -> Result<ai::RagDocumentSaveOutcome, KnowledgeSaveError>;
    fn get_document(&self, doc_id: &str) -> Result<ai::RagDocumentContentResponse, String>;
    fn stats_revision(&self) -> u64;
    fn rename_collection(&self, id: &str, name: &str) -> Result<(), String>;
    fn edit_metadata(
        &self,
        id: &str,
        name: Option<&str>,
        collection: Option<&str>,
        version: u64,
    ) -> Result<DocumentMetadata, String>;
    fn transfer(
        &self,
        id: &str,
        destination: String,
        cut: bool,
        copy_title: String,
    ) -> Result<(String, u64, bool), String>;
}

#[cfg(not(target_family = "wasm"))]
impl AiKnowledgeHost for ai::RagStore {
    fn list_collections(
        &self,
        scope_filter: Option<&str>,
    ) -> Result<Vec<ai::RagCollectionResponse>, String> {
        ai::rag_list_collections(self, scope_filter)
    }
    fn list_documents(
        &self,
        collection_id: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<ai::RagPaginatedDocuments, String> {
        ai::rag_list_documents(self, collection_id, offset, limit)
    }
    fn get_document_content(&self, doc_id: &str) -> Result<String, String> {
        ai::rag_get_document_content(self, doc_id)
    }
    fn create_collection(
        &self,
        request: ai::RagCreateCollectionRequest,
    ) -> Result<ai::RagCollectionResponse, String> {
        ai::rag_create_collection(self, request)
    }
    fn create_blank_document(
        &self,
        request: ai::RagCreateBlankDocumentRequest,
    ) -> Result<ai::RagDocumentResponse, String> {
        ai::rag_create_blank_document(self, request)
    }
    fn delete_collection(&self, collection_id: &str) -> Result<(), String> {
        ai::rag_delete_collection(self, collection_id)
    }
    fn remove_document(&self, doc_id: &str) -> Result<(), String> {
        ai::rag_remove_document(self, doc_id)
    }
    fn add_document(
        &self,
        request: ai::RagAddDocumentRequest,
    ) -> Result<ai::RagDocumentResponse, String> {
        ai::rag_add_document(self, request)
    }
    fn search(
        &self,
        request: ai::RagSearchRequest,
    ) -> Result<Vec<ai::RagSearchResultResponse>, String> {
        ai::rag_search(self, request)
    }
    fn get_collection_stats(&self, collection_id: &str) -> Result<ai::RagStatsResponse, String> {
        ai::rag_get_collection_stats(self, collection_id)
    }
    fn keyword_index_state(&self) -> ai::RagKeywordIndexState {
        ai::rag_keyword_index_state(self)
    }
    fn document_semantic_index_state(
        &self,
        doc_id: &str,
    ) -> Result<ai::RagSemanticIndexState, String> {
        ai::rag_document_semantic_index_state(self, doc_id).map_err(|err| err.to_string())
    }
    fn save_document(
        &self,
        doc_id: &str,
        content: String,
        expected_version: Option<u64>,
    ) -> Result<ai::RagDocumentSaveOutcome, KnowledgeSaveError> {
        ai::rag_save_document(self, doc_id, content, expected_version).map_err(|err| match err {
            ai::RagError::VersionConflict { .. } => KnowledgeSaveError::Conflict,
            other => KnowledgeSaveError::Failed(other.to_string()),
        })
    }
    fn get_document(&self, doc_id: &str) -> Result<ai::RagDocumentContentResponse, String> {
        ai::rag_get_document(self, doc_id).map_err(|err| err.to_string())
    }
    fn stats_revision(&self) -> u64 {
        self.collection_stats_revision()
    }
    fn rename_collection(&self, id: &str, name: &str) -> Result<(), String> {
        self.rename_collection(id, name)
            .map_err(|err| err.to_string())
    }
    fn edit_metadata(
        &self,
        id: &str,
        name: Option<&str>,
        collection: Option<&str>,
        version: u64,
    ) -> Result<DocumentMetadata, String> {
        let doc = self
            .edit_document_metadata(id, name, collection, version)
            .map_err(|err| err.to_string())?;
        let _ = self.queue_bm25_rebuild();
        Ok(DocumentMetadata {
            id: doc.id,
            title: doc.title,
            collection_id: doc.collection_id,
            version: doc.version,
        })
    }
    fn transfer(
        &self,
        id: &str,
        destination: String,
        cut: bool,
        copy_title: String,
    ) -> Result<(String, u64, bool), String> {
        let source = self
            .get_doc_metadata(id)
            .map_err(|err| err.to_string())?
            .ok_or_else(|| ai::RagError::DocumentNotFound(id.to_owned()).to_string())?;
        if cut {
            let changed = source.collection_id != destination;
            let doc = if changed {
                self.edit_document_metadata(id, None, Some(&destination), source.version)
                    .map_err(|err| err.to_string())?
            } else {
                source
            };
            Ok((doc.id, doc.version, changed))
        } else {
            ai::rag_copy_document(
                self,
                id,
                destination,
                copy_title.replace("{{title}}", &source.title),
            )
            .map(|doc| (doc.id, doc.version, true))
            .map_err(|err| err.to_string())
        }
    }
}
