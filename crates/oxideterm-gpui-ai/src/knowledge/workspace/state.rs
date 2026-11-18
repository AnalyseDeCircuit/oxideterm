use super::*;

#[derive(Clone, Default)]
pub(super) struct KnowledgeNavigatorSnapshot {
    pub(super) collections: Arc<Vec<oxideterm_ai::RagCollectionResponse>>,
    pub(super) selected_collection_id: Option<String>,
    pub(super) selected_collection: Option<oxideterm_ai::RagCollectionResponse>,
    pub(super) documents: Arc<Vec<oxideterm_ai::RagDocumentResponse>>,
    pub(super) error: Option<String>,
    pub(super) loaded: bool,
}

/// Registry and async selection state for the single Knowledge workspace tab.
#[derive(Default)]
pub struct KnowledgeWorkspaceEntity {
    pub(super) tab_id: Option<TabId>,
    pub(super) selected_document_id: Option<String>,
    pub editor: Option<Entity<KnowledgeDocumentEditor>>,
    pub(super) cache_visible: bool,
    pub(super) load_generation: u64,
    pub(super) loading: bool,
    pub(super) load_error: Option<String>,
    pub(super) pending_document_id: Option<String>,
    pub(super) switch_after_save: bool,
    pub(super) pending_close: Option<(TabId, AnyWindowHandle)>,
    pub(super) close_after_save: bool,
    pub(super) pending_app_quit: bool,
    pub(super) app_quit_after_save: bool,
    pub(super) _editor_subscription: Option<Subscription>,
    pub(super) navigator_snapshot: KnowledgeNavigatorSnapshot,
    pub(super) navigator_hidden: bool,
    pub(super) mobile_navigator_open: bool,
    pub(super) navigator_width: Option<f32>,
    pub(super) surface_width: Option<f32>,
    pub(super) navigator_resize: Option<(gpui::WindowId, f32, f32)>,
    pub(super) navigator_closing: bool,
    pub(super) navigator_motion_task: Option<Task<()>>,
    pub(super) menu: Option<navigation::KnowledgeMenu>,
    pub(crate) rename: Option<navigation::KnowledgeRename>,
    pub(crate) metadata_task: Option<Task<()>>,
    pub(super) metadata_error: Option<String>,
    pub(super) clipboard: Option<operations::NoteClipboard>,
    pub(super) search_task: Option<Task<()>>,
    pub(super) last_embedding_running: bool,
    pub(super) pending_collection_id: Option<String>,
    pub(super) navigator_refresh_generation: u64,
    pub(super) navigator_refresh_running: bool,
    pub(super) navigator_refresh_requested: bool,
    pub(super) navigator_last_refresh: Option<Instant>,
    pub(crate) navigator_query: Arc<str>,
    pub(crate) navigator_search_window: Option<gpui::WindowId>,
}

impl KnowledgeWorkspaceEntity {
    pub fn app_quit_pending(&self) -> bool {
        self.pending_app_quit
    }

    pub fn tab_id(&self) -> Option<TabId> {
        self.tab_id
    }

    pub fn register_tab(&mut self, tab_id: TabId) {
        self.tab_id = Some(tab_id);
    }

    pub fn close_tab(&mut self, tab_id: TabId) {
        if self.tab_id == Some(tab_id) {
            self.tab_id = None;
            self.selected_document_id = None;
            self.editor = None;
            self.loading = false;
            self.pending_document_id = None;
            self.switch_after_save = false;
            self.pending_close = None;
            self.close_after_save = false;
            self.pending_app_quit = false;
            self.app_quit_after_save = false;
            self._editor_subscription = None;
            self.load_generation = self.load_generation.wrapping_add(1);
            self.navigator_query = Arc::from("");
            self.menu = None;
            self.rename = None;
            self.metadata_task = None;
            self.metadata_error = None;
            self.search_task = None;
            self.navigator_resize = None;
            self.navigator_closing = false;
            self.navigator_motion_task = None;
            self.pending_collection_id = None;
            self.navigator_search_window = None;
        }
    }

    pub(super) fn begin_document_load(&mut self, document_id: String) -> u64 {
        self.load_generation = self.load_generation.wrapping_add(1);
        // A clean editor must disappear before the asynchronous replacement starts. Leaving it
        // interactive would allow a new dirty draft to be created and then overwritten by the
        // pending load result.
        self.editor = None;
        self._editor_subscription = None;
        self.selected_document_id = Some(document_id);
        self.mobile_navigator_open = false;
        self.loading = true;
        self.load_error = None;
        self.load_generation
    }

    pub(super) fn request_document_switch(&mut self, document_id: String) {
        self.pending_document_id = Some(document_id);
        self.switch_after_save = false;
    }

    pub(super) fn cancel_document_switch(&mut self) {
        self.pending_document_id = None;
        self.pending_collection_id = None;
        self.switch_after_save = false;
    }

    pub(super) fn take_pending_document_after_save(&mut self) -> Option<String> {
        if !self.switch_after_save {
            return None;
        }
        self.switch_after_save = false;
        self.pending_document_id.take()
    }

    pub fn request_close(&mut self, tab_id: TabId, window_handle: AnyWindowHandle) {
        self.pending_close = Some((tab_id, window_handle));
        self.close_after_save = false;
    }

    pub(super) fn cancel_close(&mut self) {
        self.pending_close = None;
        self.close_after_save = false;
    }

    pub fn request_app_quit(&mut self) {
        self.pending_app_quit = true;
        self.app_quit_after_save = false;
    }

    pub(super) fn cancel_app_quit(&mut self) {
        self.pending_app_quit = false;
        self.app_quit_after_save = false;
    }

    pub(super) fn take_pending_close_after_save(&mut self) -> Option<(TabId, AnyWindowHandle)> {
        if !self.close_after_save {
            return None;
        }
        self.close_after_save = false;
        self.pending_close.take()
    }

    pub(super) fn confirm_save_before_leaving(&mut self) {
        self.switch_after_save =
            self.pending_document_id.is_some() || self.pending_collection_id.is_some();
        self.close_after_save = self.pending_close.is_some();
        self.app_quit_after_save = self.pending_app_quit;
    }

    pub(super) fn take_pending_document_now(&mut self) -> Option<String> {
        self.switch_after_save = false;
        self.pending_document_id.take()
    }

    pub(super) fn take_pending_close_now(&mut self) -> Option<(TabId, AnyWindowHandle)> {
        self.close_after_save = false;
        self.pending_close.take()
    }

    pub(super) fn take_pending_app_quit_after_save(&mut self) -> bool {
        if !self.app_quit_after_save {
            return false;
        }
        self.app_quit_after_save = false;
        std::mem::take(&mut self.pending_app_quit)
    }

    pub(super) fn take_pending_app_quit_now(&mut self) -> bool {
        self.app_quit_after_save = false;
        std::mem::take(&mut self.pending_app_quit)
    }

    pub(super) fn install_document(
        &mut self,
        generation: u64,
        document_id: &str,
        editor: Entity<KnowledgeDocumentEditor>,
        subscription: Subscription,
    ) -> bool {
        if generation != self.load_generation
            || self.selected_document_id.as_deref() != Some(document_id)
        {
            return false;
        }
        self.editor = Some(editor);
        self._editor_subscription = Some(subscription);
        self.loading = false;
        self.load_error = None;
        true
    }

    pub(super) fn install_load_error(&mut self, generation: u64, error: String) {
        if generation == self.load_generation {
            self.editor = None;
            self.loading = false;
            self.load_error = Some(error);
        }
    }

    pub(super) fn begin_navigator_refresh(&mut self, force: bool) -> Option<u64> {
        if self.navigator_refresh_running {
            // A mutation can land while the previous snapshot is still loading. Retain one
            // follow-up request so the older result cannot leave the navigator permanently stale.
            self.navigator_refresh_requested |= force;
            return None;
        }
        if !force
            && self
                .navigator_last_refresh
                .is_some_and(|updated| updated.elapsed() < KNOWLEDGE_NAVIGATOR_REFRESH_INTERVAL)
        {
            return None;
        }
        self.navigator_refresh_generation = self.navigator_refresh_generation.wrapping_add(1);
        self.navigator_refresh_running = true;
        Some(self.navigator_refresh_generation)
    }

    pub(super) fn install_navigator_snapshot(
        &mut self,
        generation: u64,
        snapshot: KnowledgeNavigatorSnapshot,
    ) -> bool {
        if generation != self.navigator_refresh_generation {
            return false;
        }
        self.navigator_snapshot = snapshot;
        self.navigator_refresh_running = false;
        self.navigator_last_refresh = Some(Instant::now());
        std::mem::take(&mut self.navigator_refresh_requested)
    }

    pub fn insert_created_document(&mut self, document: oxideterm_ai::RagDocumentResponse) {
        if self.navigator_refresh_running {
            // The in-flight snapshot was sampled before this committed mutation. Invalidate its
            // generation so it cannot briefly erase the optimistic row before the forced refresh.
            self.navigator_refresh_generation = self.navigator_refresh_generation.wrapping_add(1);
            self.navigator_refresh_running = false;
            self.navigator_refresh_requested = false;
        }
        if self.navigator_snapshot.selected_collection_id.as_deref()
            != Some(document.collection_id.as_str())
        {
            return;
        }
        // Reflect the committed document immediately; the forced refresh below remains the source
        // of truth for collection counts and any mutations that landed concurrently.
        let mut documents = self.navigator_snapshot.documents.as_ref().clone();
        documents.retain(|existing| existing.id != document.id);
        documents.push(document);
        self.navigator_snapshot.documents = Arc::new(documents);
    }

    pub fn remove_document(&mut self, document_id: &str) {
        if self.selected_document_id.as_deref() == Some(document_id) {
            self.selected_document_id = None;
            self.editor = None;
            self._editor_subscription = None;
            self.pending_document_id = None;
            self.load_generation = self.load_generation.wrapping_add(1);
            self.loading = false;
        }
    }

    pub fn remove_collection(&mut self, collection_id: &str, cx: &App) {
        if self
            .editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).collection_id.as_str() == collection_id)
        {
            self.selected_document_id = None;
            self.editor = None;
            self._editor_subscription = None;
            self.pending_document_id = None;
            self.load_generation = self.load_generation.wrapping_add(1);
            self.loading = false;
        }
    }
}
