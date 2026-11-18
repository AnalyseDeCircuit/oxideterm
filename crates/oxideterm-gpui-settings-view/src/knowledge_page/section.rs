use gpui::{AnyElement, App, IntoElement, div};
use oxideterm_ai::{
    AiEmbeddingProviderReason, AiProviderView, RagCollectionResponse, RagPaginatedDocuments,
    RagStatsResponse, RagStore,
};
use oxideterm_i18n::I18n;
use oxideterm_settings::PersistedSettings;
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

pub struct EmbeddingJob {
    pub provider: AiProviderView,
    pub model: String,
    pub requires_api_key: bool,
    pub missing_key_error: String,
    pub retrieval_error: String,
    pub partial_failure: String,
}

impl EmbeddingJob {
    pub fn prepare(settings: &PersistedSettings, i18n: &I18n) -> Result<Self, String> {
        let resolved = oxideterm_ai::resolve_ai_embedding_provider(
            &settings.ai.providers,
            settings.ai.active_provider_id.as_deref(),
            settings.ai.embedding_config.as_ref(),
            None,
        );
        let provider = resolved
            .provider
            .ok_or_else(|| i18n.t("settings_view.knowledge.error_no_embedding_support"))?;
        match resolved.reason {
            AiEmbeddingProviderReason::NoProvider
            | AiEmbeddingProviderReason::UnsupportedProvider => {
                return Err(i18n.t("settings_view.knowledge.error_no_embedding_support"));
            }
            AiEmbeddingProviderReason::MissingModel => {
                return Err(i18n.t("settings_view.knowledge.error_no_embedding_model"));
            }
            AiEmbeddingProviderReason::Ready | AiEmbeddingProviderReason::MissingApiKey => {}
        }
        Ok(Self {
            requires_api_key: oxideterm_ai::ai_embedding_requires_api_key(&provider),
            provider,
            model: resolved.model,
            missing_key_error: i18n.t("settings_view.knowledge.error_no_embedding_api_key"),
            retrieval_error: i18n.t("settings_view.knowledge.error_generate_embeddings"),
            partial_failure: i18n.t("settings_view.knowledge.embedding_partial_failure"),
        })
    }
}
