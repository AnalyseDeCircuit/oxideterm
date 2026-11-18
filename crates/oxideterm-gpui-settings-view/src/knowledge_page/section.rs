use gpui::{AnyElement, App, IntoElement, div};
use oxideterm_ai::{RagCollectionResponse, RagPaginatedDocuments, RagStatsResponse, RagStore};
use oxideterm_settings_model::SettingsSectionId;

use super::KnowledgeSettingsPage;

pub enum KnowledgeSection {
    Empty,
    Error(String),
    Collections(Vec<RagCollectionResponse>, Option<String>),
    Embedding,
    Documents {
        collection: RagCollectionResponse,
        page: usize,
        documents: RagPaginatedDocuments,
        stats: Option<RagStatsResponse>,
    },
}

pub fn selected_collection<'a>(
    collections: &'a [RagCollectionResponse],
    selected: Option<&str>,
) -> Option<&'a RagCollectionResponse> {
    selected
        .and_then(|id| collections.iter().find(|collection| collection.id == id))
        .or_else(|| collections.first())
}

impl KnowledgeSection {
    pub fn load(
        section: SettingsSectionId,
        store: &RagStore,
        selected: Option<&str>,
        error: Option<&str>,
        document_page: impl FnOnce(
            &str,
        ) -> (
            Result<(usize, RagPaginatedDocuments), String>,
            Option<RagStatsResponse>,
        ),
    ) -> Self {
        match section {
            SettingsSectionId::KnowledgeError => {
                error.map_or(Self::Empty, |error| Self::Error(error.to_owned()))
            }
            SettingsSectionId::KnowledgeEmbeddingConfig => Self::Embedding,
            SettingsSectionId::KnowledgeCollections => {
                let collections =
                    oxideterm_ai::rag_list_collections(store, None).unwrap_or_default();
                let selected = selected_collection(&collections, selected)
                    .map(|collection| collection.id.clone());
                Self::Collections(collections, selected)
            }
            SettingsSectionId::KnowledgeDocuments => {
                let collections =
                    oxideterm_ai::rag_list_collections(store, None).unwrap_or_default();
                let Some(collection) = selected_collection(&collections, selected) else {
                    return Self::Empty;
                };
                let (page, stats) = document_page(&collection.id);
                match page {
                    Ok((page, documents)) => Self::Documents {
                        collection: collection.clone(),
                        page,
                        documents,
                        stats,
                    },
                    Err(error) => Self::Error(error),
                }
            }
            _ => Self::Empty,
        }
    }

    pub fn render(self, page: &KnowledgeSettingsPage<'_>, cx: &App) -> AnyElement {
        match self {
            Self::Empty => div().into_any_element(),
            Self::Error(error) => page.knowledge_error_row(&error),
            Self::Collections(collections, selected) => {
                page.knowledge_collections_card(&collections, selected.as_deref(), cx)
            }
            Self::Embedding => page.knowledge_embedding_config_section(cx),
            Self::Documents {
                collection,
                page: index,
                documents,
                stats,
            } => page.knowledge_documents_card(&collection, documents, index, stats, true, cx),
        }
    }
}
