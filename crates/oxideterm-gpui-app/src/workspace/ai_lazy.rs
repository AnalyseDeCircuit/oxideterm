use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use oxideterm_settings::default_settings_path;

#[derive(Clone)]
pub(super) struct LazyAiRagStore {
    data_dir: PathBuf,
    store: Arc<OnceLock<Arc<oxideterm_ai::RagStore>>>,
}

impl LazyAiRagStore {
    pub(super) fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            store: Arc::new(OnceLock::new()),
        }
    }

    pub(super) fn default() -> Self {
        Self::new(default_rag_data_dir())
    }

    pub(super) fn get(&self) -> Arc<oxideterm_ai::RagStore> {
        self.store
            .get_or_init(|| open_rag_store_or_fallback(&self.data_dir))
            .clone()
    }
}

fn open_rag_store_or_fallback(data_dir: &PathBuf) -> Arc<oxideterm_ai::RagStore> {
    if let Err(error) = std::fs::create_dir_all(data_dir) {
        eprintln!("failed to create AI RAG data directory: {error}");
    }
    match oxideterm_ai::RagStore::new(data_dir) {
        Ok(store) => Arc::new(store),
        Err(error) => {
            eprintln!("failed to load AI RAG store: {error}");
            let fallback_dir = std::env::temp_dir().join(format!(
                "oxideterm-rag-unavailable-{}",
                uuid::Uuid::new_v4()
            ));
            // The fallback keeps AI and Knowledge UI usable when the configured
            // redb file is corrupt or locked, matching the previous startup path.
            std::fs::create_dir_all(&fallback_dir)
                .expect("failed to create fallback AI RAG data directory");
            Arc::new(
                oxideterm_ai::RagStore::new(&fallback_dir)
                    .expect("failed to open fallback AI RAG store"),
            )
        }
    }
}

fn default_rag_data_dir() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
