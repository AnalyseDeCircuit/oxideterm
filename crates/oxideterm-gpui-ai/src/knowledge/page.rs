use std::{
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use gpui::{Context, EventEmitter, Task};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_settings_model::SettingsInput;

#[cfg(test)]
mod tests;

fn knowledge_audit(target: &str, action: &str) -> oxideterm_audit::AuditOperation {
    let context = oxideterm_audit::AuditContext::current_request()
        .or_else(oxideterm_audit::AuditContext::current)
        .map(|mut context| {
            if context.source == oxideterm_audit::AuditSource::Application {
                context.source = oxideterm_audit::AuditSource::User;
            }
            context.target = Some(oxideterm_audit::redact(target));
            context
        });
    oxideterm_audit::AuditOperation::in_context(
        context.as_ref(),
        oxideterm_audit::AuditCategory::File,
        action,
        None,
    )
}

pub enum KnowledgeExternalSyncOutcome {
    NoEdit,
    NoChanges,
    Updated,
    Failed,
}

struct KnowledgeCollectionStats {
    key: (String, u64),
    value: oxideterm_ai::RagStatsResponse,
}

/// Page-local Knowledge state whose lifetime follows the AI workspace entity.
///
/// The type intentionally has no `Debug` implementation because drafts and
/// external-edit metadata can describe user-owned document content.
pub struct KnowledgePageState {
    stats_request: Option<(String, u64)>,
    stats: Option<KnowledgeCollectionStats>,
    stats_task: Option<Task<()>>,
    pub(super) selected_collection_id: Option<String>,
    pub(super) document_page: usize,
    pub(super) create_dialog_open: bool,
    pub(super) new_document_dialog_open: bool,
    pub(super) embedding_config_expanded: bool,
    pub(super) new_collection_name: String,
    pub(super) new_collection_connection_id: Option<String>,
    pub(super) new_document_collection_id: Option<String>,
    pub(super) new_document_owner_window_id: Option<gpui::WindowId>,
    pub(super) new_document_open_in_workspace: bool,
    pub(super) new_document_title: String,
    pub(super) new_document_format: String,
    pub(super) new_document_error: Option<String>,
    pub(super) import_progress: Option<(usize, usize)>,
    pub(super) embedding_progress: Option<(usize, usize)>,
    pub(super) delete_confirm: Option<oxideterm_settings_model::KnowledgeDeleteConfirm>,
    pub(super) external_edit: Option<oxideterm_settings_model::KnowledgeExternalEdit>,
    pub(super) error: Option<String>,
    pub(super) create_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) document_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) create_exit_task: Option<Task<()>>,
    pub(super) document_exit_task: Option<Task<()>>,
}

impl Default for KnowledgePageState {
    fn default() -> Self {
        Self {
            stats_request: None,
            stats: None,
            stats_task: None,
            selected_collection_id: None,
            document_page: 0,
            create_dialog_open: false,
            new_document_dialog_open: false,
            embedding_config_expanded: false,
            new_collection_name: String::new(),
            new_collection_connection_id: None,
            new_document_collection_id: None,
            new_document_owner_window_id: None,
            new_document_open_in_workspace: false,
            new_document_title: String::new(),
            new_document_format: "markdown".to_string(),
            new_document_error: None,
            import_progress: None,
            embedding_progress: None,
            delete_confirm: None,
            external_edit: None,
            error: None,
            create_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            document_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            create_exit_task: None,
            document_exit_task: None,
        }
    }
}

impl KnowledgePageState {
    fn input_value_mut(&mut self, input: SettingsInput) -> Option<&mut String> {
        match input {
            SettingsInput::KnowledgeCollectionName => Some(&mut self.new_collection_name),
            SettingsInput::KnowledgeDocumentTitle => Some(&mut self.new_document_title),
            _ => None,
        }
    }
}

pub enum KnowledgePageEvent {
    ModalClosed,
    ReindexFailed,
}

pub struct KnowledgePage {
    state: KnowledgePageState,
    focused_settings_input: Option<SettingsInput>,
    store: Rc<dyn Fn() -> Arc<oxideterm_ai::RagStore>>,
    task_runtime: Arc<tokio::runtime::Runtime>,
    key_store: oxideterm_ai::AiProviderKeyStore,
    knowledge_import_task: Option<Task<()>>,
    knowledge_embedding_task: Option<Task<()>>,
    reindex_task: Option<Task<()>>,
    reindex_cancel: Option<Arc<AtomicBool>>,
    reindex_progress: Option<(usize, usize)>,
}

impl EventEmitter<KnowledgePageEvent> for KnowledgePage {}

pub(super) struct KnowledgeDocumentsChanged;
impl EventEmitter<KnowledgeDocumentsChanged> for KnowledgePage {}

impl Drop for KnowledgePage {
    fn drop(&mut self) {
        self.cancel_knowledge_reindex();
    }
}

impl KnowledgePage {
    pub fn new(
        store: Rc<dyn Fn() -> Arc<oxideterm_ai::RagStore>>,
        task_runtime: Arc<tokio::runtime::Runtime>,
        key_store: oxideterm_ai::AiProviderKeyStore,
    ) -> Self {
        Self {
            state: KnowledgePageState::default(),
            focused_settings_input: None,
            store,
            task_runtime,
            key_store,
            knowledge_import_task: None,
            knowledge_embedding_task: None,
            reindex_task: None,
            reindex_cancel: None,
            reindex_progress: None,
        }
    }

    pub fn rag_store(&self) -> Arc<oxideterm_ai::RagStore> {
        (self.store)()
    }

    pub fn knowledge_reindex_progress(&self) -> Option<(usize, usize)> {
        self.reindex_progress
    }

    pub fn request_knowledge_reindex(
        &mut self,
        collection_id: String,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.reindex_task.is_some() {
            return false;
        }
        self.state.error = None;
        self.reindex_progress = Some((0, 0));
        let cancel = Arc::new(AtomicBool::new(false));
        self.reindex_cancel = Some(cancel.clone());
        let store = (self.store)();
        let (deliver, mut deliveries) = tokio::sync::mpsc::unbounded_channel();
        let worker = self.task_runtime.spawn_blocking(move || {
            let mut last_emitted = 0;
            oxideterm_ai::rag_reindex_collection_with_progress(
                &store,
                &collection_id,
                Some(&cancel),
                Some(&mut |current, total| {
                    if current == total || current.saturating_sub(last_emitted) >= 10 {
                        let _ = deliver.send((current, total));
                        last_emitted = current;
                    }
                }),
            )
            .is_err()
        });
        self.reindex_task = Some(cx.spawn(async move |page, cx| {
            while let Some(progress) = deliveries.recv().await {
                if page
                    .update(cx, |page, cx| {
                        page.reindex_progress = Some(progress);
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
            }
            let failed = worker.await.unwrap_or(true);
            let _ = page.update(cx, |page, cx| {
                page.reindex_task = None;
                page.reindex_progress = None;
                page.reindex_cancel = None;
                if failed {
                    cx.emit(KnowledgePageEvent::ReindexFailed);
                }
                cx.notify();
            });
        }));
        cx.notify();
        true
    }

    pub fn cancel_knowledge_reindex(&self) -> bool {
        let Some(cancel) = self.reindex_cancel.as_ref() else {
            return false;
        };
        cancel.store(true, Ordering::Relaxed);
        true
    }

    pub fn owns_settings_input(input: SettingsInput) -> bool {
        matches!(
            input,
            SettingsInput::KnowledgeCollectionName | SettingsInput::KnowledgeDocumentTitle
        )
    }

    pub fn focused_settings_input(&self) -> Option<SettingsInput> {
        self.focused_settings_input
    }

    pub fn settings_input_value(&self, input: SettingsInput) -> Option<&str> {
        match input {
            SettingsInput::KnowledgeCollectionName => {
                return Some(&self.state.new_collection_name);
            }
            SettingsInput::KnowledgeDocumentTitle => {
                return Some(&self.state.new_document_title);
            }
            _ => None,
        }
    }

    pub fn focus_settings_input(&mut self, input: SettingsInput, cx: &mut Context<Self>) -> bool {
        if !Self::owns_settings_input(input) {
            return false;
        }
        if self.focused_settings_input == Some(input) {
            return true;
        }

        self.clear_focused_settings_input();
        self.focused_settings_input = Some(input);
        cx.notify();
        true
    }

    pub fn blur_settings_input(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.clear_focused_settings_input();
        if changed {
            cx.notify();
        }
        changed
    }

    pub fn replace_settings_input(
        &mut self,
        input: SettingsInput,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.focused_settings_input != Some(input) {
            return false;
        }
        let Some(value) = self.state.input_value_mut(input) else {
            return false;
        };
        replace_utf16(value, replacement_range, text);
        cx.notify();
        true
    }

    pub fn pop_settings_input(&mut self, input: SettingsInput, cx: &mut Context<Self>) -> bool {
        if self.focused_settings_input != Some(input) {
            return false;
        }
        let changed = self
            .state
            .input_value_mut(input)
            .is_some_and(|value| value.pop().is_some());
        if changed {
            cx.notify();
        }
        true
    }

    fn clear_focused_settings_input(&mut self) -> bool {
        self.focused_settings_input.take().is_some()
    }

    pub fn knowledge_stats_cache_key(&self) -> Option<&(String, u64)> {
        self.state.stats.as_ref().map(|stats| &stats.key)
    }

    pub fn request_knowledge_collection_stats(
        &mut self,
        collection_id: &str,
        cx: &mut Context<Self>,
    ) -> Option<oxideterm_ai::RagStatsResponse> {
        let store = (self.store)();
        let key = (collection_id.to_owned(), store.collection_stats_revision());
        self.state.stats_request = Some(key.clone());
        if let Some(stats) = &self.state.stats
            && stats.key == key
        {
            return Some(stats.value.clone());
        }
        // One worker at a time, even when selection or data changes during a read.
        // Only the most recently requested collection/revision can publish its result.
        if self.state.stats_task.is_none() {
            let task = cx.spawn(async move |entity, cx| {
                let read_store = store.clone();
                let collection = key.0.clone();
                let value = cx
                    .background_executor()
                    .spawn(async move {
                        oxideterm_ai::rag_get_collection_stats(&read_store, &collection).ok()
                    })
                    .await;
                let _ = entity.update(cx, |entity, cx| {
                    let current = entity.state.stats_request.as_ref() == Some(&key)
                        && store.collection_stats_revision() == key.1;
                    let loaded = value.is_some();
                    if current {
                        entity.state.stats =
                            value.map(|value| KnowledgeCollectionStats { key, value });
                    }
                    entity.state.stats_task = None;
                    // A failed read can retry on the next user-driven render without a retry loop.
                    if loaded || !current {
                        entity.emit_knowledge_page_changed(cx);
                    }
                });
            });
            // Dropping the entity cancels delivery; the worker owns no UI or document content.
            self.state.stats_task = Some(task);
        }
        None
    }

    fn emit_knowledge_page_changed(&self, cx: &mut Context<Self>) {
        cx.notify();
    }

    pub fn knowledge_selected_collection_id(&self) -> Option<&str> {
        self.state.selected_collection_id.as_deref()
    }

    pub fn knowledge_document_page_index(&self) -> usize {
        self.state.document_page
    }

    pub fn set_knowledge_document_page(&mut self, collection_id: String, page: usize) {
        self.state.selected_collection_id = Some(collection_id);
        self.state.document_page = page;
    }

    pub fn knowledge_document_page(
        &self,
        collection_id: &str,
    ) -> Result<(usize, oxideterm_ai::RagPaginatedDocuments), String> {
        let page = if self.knowledge_selected_collection_id() == Some(collection_id) {
            self.state.document_page
        } else {
            0
        };
        read_knowledge_document_page(&(self.store)(), collection_id, page)
    }

    pub fn knowledge_create_dialog_open(&self) -> bool {
        self.state.create_dialog_open
    }

    pub fn knowledge_document_dialog_open(&self) -> bool {
        self.state.new_document_dialog_open
    }

    pub fn knowledge_create_dialog_phase(&self) -> oxideterm_gpui_ui::motion::ExitPhase {
        self.state.create_presence.phase()
    }

    pub fn knowledge_document_dialog_phase(&self) -> oxideterm_gpui_ui::motion::ExitPhase {
        self.state.document_presence.phase()
    }

    pub fn knowledge_embedding_config_expanded(&self) -> bool {
        self.state.embedding_config_expanded
    }

    pub fn knowledge_new_collection_name(&self) -> &str {
        &self.state.new_collection_name
    }

    pub fn knowledge_new_collection_connection_id(&self) -> Option<&str> {
        self.state.new_collection_connection_id.as_deref()
    }

    pub fn knowledge_new_document_title(&self) -> &str {
        &self.state.new_document_title
    }

    pub fn knowledge_new_document_format(&self) -> &str {
        &self.state.new_document_format
    }

    pub fn knowledge_new_document_error(&self) -> Option<&str> {
        self.state.new_document_error.as_deref()
    }

    pub fn knowledge_document_dialog_owned_by(&self, window_id: gpui::WindowId) -> bool {
        self.state.new_document_dialog_open
            && self.state.new_document_owner_window_id == Some(window_id)
    }

    pub fn transfer_knowledge_document_dialog_owner(
        &mut self,
        source_window_id: gpui::WindowId,
        destination_window_id: gpui::WindowId,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.knowledge_document_dialog_owned_by(source_window_id) {
            return false;
        }
        self.state.new_document_owner_window_id = Some(destination_window_id);
        self.emit_knowledge_page_changed(cx);
        true
    }

    pub fn dismiss_knowledge_document_dialog_for_window(
        &mut self,
        window_id: gpui::WindowId,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.knowledge_document_dialog_owned_by(window_id) {
            return false;
        }
        self.clear_knowledge_document_dialog();
        self.emit_knowledge_page_changed(cx);
        true
    }

    pub fn knowledge_import_progress(&self) -> Option<(usize, usize)> {
        self.state.import_progress
    }

    pub fn knowledge_embedding_progress(&self) -> Option<(usize, usize)> {
        self.state.embedding_progress
    }

    pub fn knowledge_delete_confirm(
        &self,
    ) -> Option<&oxideterm_settings_model::KnowledgeDeleteConfirm> {
        self.state.delete_confirm.as_ref()
    }

    pub fn knowledge_external_edit(
        &self,
    ) -> Option<&oxideterm_settings_model::KnowledgeExternalEdit> {
        self.state.external_edit.as_ref()
    }

    pub fn knowledge_error(&self) -> Option<&str> {
        self.state.error.as_deref()
    }

    pub fn knowledge_create_presence(&self) -> oxideterm_gpui_ui::motion::ExitPresence {
        self.state.create_presence
    }

    pub fn knowledge_document_presence(&self) -> oxideterm_gpui_ui::motion::ExitPresence {
        self.state.document_presence
    }

    pub fn select_knowledge_collection(&mut self, collection_id: String) {
        self.state.selected_collection_id = Some(collection_id);
        self.state.document_page = 0;
    }

    pub fn set_knowledge_document_format(&mut self, format: String) {
        self.state.new_document_format = format;
    }

    pub fn set_knowledge_collection_connection_id(&mut self, connection_id: Option<String>) {
        self.state.new_collection_connection_id = connection_id;
    }

    pub fn toggle_knowledge_embedding_config(&mut self) {
        self.state.embedding_config_expanded = !self.state.embedding_config_expanded;
    }

    pub fn expand_knowledge_embedding_config(&mut self) {
        self.state.embedding_config_expanded = true;
        self.state.error = None;
    }

    pub fn set_knowledge_error(&mut self, error: String) {
        self.state.error = Some(error);
    }

    pub fn clear_knowledge_error(&mut self) {
        self.state.error = None;
    }

    pub fn request_delete_knowledge_collection(&mut self, id: String, name: String) {
        self.state.delete_confirm = Some(oxideterm_settings_model::KnowledgeDeleteConfirm {
            target: oxideterm_settings_model::KnowledgeDeleteTarget::Collection,
            id,
            name,
        });
    }

    pub fn request_delete_knowledge_document(&mut self, id: String, name: String) {
        self.state.delete_confirm = Some(oxideterm_settings_model::KnowledgeDeleteConfirm {
            target: oxideterm_settings_model::KnowledgeDeleteTarget::Document,
            id,
            name,
        });
    }

    pub fn clear_knowledge_delete_confirm(&mut self) {
        self.state.delete_confirm = None;
    }

    pub fn take_knowledge_delete_confirm(
        &mut self,
    ) -> Option<oxideterm_settings_model::KnowledgeDeleteConfirm> {
        self.state.delete_confirm.take()
    }

    pub fn open_knowledge_create_dialog(&mut self) {
        self.state.create_exit_task = None;
        self.state.create_presence.reopen();
        self.state.create_dialog_open = true;
    }

    pub fn close_knowledge_create_dialog(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let Some(generation) = self.state.create_presence.begin_exit() else {
            return;
        };
        if delay.is_zero() {
            self.finish_knowledge_create_dialog_exit(generation);
            self.emit_knowledge_page_changed(cx);
            return;
        }
        let task = cx.spawn(async move |entity, cx| {
            gpui::Timer::after(delay).await;
            let _ = entity.update(cx, |entity, cx| {
                entity.finish_knowledge_create_dialog_exit(generation);
                cx.emit(KnowledgePageEvent::ModalClosed);
                entity.emit_knowledge_page_changed(cx);
            });
        });
        self.state.create_exit_task = Some(task);
        self.emit_knowledge_page_changed(cx);
    }

    fn finish_knowledge_create_dialog_exit(&mut self, generation: u64) {
        if self.state.create_presence.finish_exit(generation) {
            self.state.create_dialog_open = false;
            self.state.new_collection_name.clear();
            self.state.new_collection_connection_id = None;
            self.state.create_presence.reopen();
            self.state.create_exit_task = None;
            if self.focused_settings_input == Some(SettingsInput::KnowledgeCollectionName) {
                self.clear_focused_settings_input();
            }
        }
    }

    pub fn open_knowledge_document_dialog(
        &mut self,
        collection_id: String,
        owner_window_id: gpui::WindowId,
        open_in_workspace: bool,
    ) {
        if self.state.new_document_dialog_open {
            // One shared draft cannot safely be moved to another window while
            // it is visible. Keep the existing owner and draft intact.
            return;
        }
        self.state.document_exit_task = None;
        self.state.document_presence.reopen();
        // Bind creation to the section that opened the dialog so a concurrent navigator refresh
        // cannot redirect the new document into a different collection.
        self.state.selected_collection_id = Some(collection_id.clone());
        self.state.new_document_collection_id = Some(collection_id);
        self.state.new_document_owner_window_id = Some(owner_window_id);
        self.state.new_document_open_in_workspace = open_in_workspace;
        self.state.new_document_error = None;
        self.state.new_document_dialog_open = true;
    }

    pub fn close_knowledge_document_dialog(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let Some(generation) = self.state.document_presence.begin_exit() else {
            return;
        };
        if delay.is_zero() {
            self.finish_knowledge_document_dialog_exit(generation);
            self.emit_knowledge_page_changed(cx);
            return;
        }
        let task = cx.spawn(async move |entity, cx| {
            gpui::Timer::after(delay).await;
            let _ = entity.update(cx, |entity, cx| {
                entity.finish_knowledge_document_dialog_exit(generation);
                cx.emit(KnowledgePageEvent::ModalClosed);
                entity.emit_knowledge_page_changed(cx);
            });
        });
        self.state.document_exit_task = Some(task);
        self.emit_knowledge_page_changed(cx);
    }

    fn finish_knowledge_document_dialog_exit(&mut self, generation: u64) {
        if self.state.document_presence.finish_exit(generation) {
            self.clear_knowledge_document_dialog();
        }
    }

    fn clear_knowledge_document_dialog(&mut self) {
        self.state.new_document_dialog_open = false;
        self.state.new_document_collection_id = None;
        self.state.new_document_owner_window_id = None;
        self.state.new_document_open_in_workspace = false;
        self.state.new_document_title.clear();
        self.state.new_document_error = None;
        self.state.document_presence.reopen();
        self.state.document_exit_task = None;
        if self.focused_settings_input == Some(SettingsInput::KnowledgeDocumentTitle) {
            self.clear_focused_settings_input();
        }
    }

    pub fn create_knowledge_collection(&mut self, error_message: String) -> bool {
        let name = self.state.new_collection_name.trim().to_string();
        if name.is_empty() {
            return false;
        }
        let store = (self.store)();
        let scope = match self.state.new_collection_connection_id.clone() {
            Some(connection_id) => oxideterm_ai::RagDocScopeRequest::Connection { connection_id },
            None => oxideterm_ai::RagDocScopeRequest::Global,
        };
        let mut audit = knowledge_audit("new_collection", "knowledge_collection_create");
        match oxideterm_ai::rag_create_collection(
            &store,
            oxideterm_ai::RagCreateCollectionRequest { name, scope },
        ) {
            Ok(collection) => {
                audit.summary(&collection.id);
                audit.finish(
                    oxideterm_audit::AuditOutcome::Succeeded,
                    oxideterm_audit::AuditEvidence::Protocol,
                    None,
                    None,
                );
                self.select_knowledge_collection(collection.id);
                self.state.new_collection_name.clear();
                self.state.new_collection_connection_id = None;
                self.state.error = None;
                true
            }
            Err(_) => {
                audit.finish(
                    oxideterm_audit::AuditOutcome::Failed,
                    oxideterm_audit::AuditEvidence::Protocol,
                    None,
                    None,
                );
                self.state.error = Some(error_message);
                false
            }
        }
    }

    pub fn create_blank_knowledge_document(
        &mut self,
        error_message: String,
    ) -> Option<(oxideterm_ai::RagDocumentResponse, bool)> {
        let store = (self.store)();
        let Some(collection_id) = self.state.new_document_collection_id.clone() else {
            self.state.new_document_error = Some(error_message);
            return None;
        };
        let title = self.state.new_document_title.trim().to_string();
        if title.is_empty() {
            return None;
        }
        let mut audit = knowledge_audit(&collection_id, "knowledge_document_create");
        match oxideterm_ai::rag_create_blank_document(
            &store,
            oxideterm_ai::RagCreateBlankDocumentRequest {
                collection_id,
                title,
                format: self.state.new_document_format.clone(),
            },
        ) {
            Ok(document) => {
                audit.summary(&document.id);
                audit.finish(
                    oxideterm_audit::AuditOutcome::Succeeded,
                    oxideterm_audit::AuditEvidence::Protocol,
                    None,
                    None,
                );
                self.state.new_document_title.clear();
                self.state.new_document_error = None;
                Some((document, self.state.new_document_open_in_workspace))
            }
            Err(_) => {
                audit.finish(
                    oxideterm_audit::AuditOutcome::Failed,
                    oxideterm_audit::AuditEvidence::Protocol,
                    None,
                    None,
                );
                self.state.new_document_error = Some(error_message);
                None
            }
        }
    }

    pub fn delete_knowledge_collection(
        &mut self,
        collection_id: &str,
        error_message: String,
    ) -> bool {
        let audit = knowledge_audit(collection_id, "knowledge_collection_delete");
        if oxideterm_ai::rag_delete_collection(&(self.store)(), collection_id).is_err() {
            audit.finish(
                oxideterm_audit::AuditOutcome::Failed,
                oxideterm_audit::AuditEvidence::Protocol,
                None,
                None,
            );
            self.state.error = Some(error_message);
            return false;
        }
        audit.finish(
            oxideterm_audit::AuditOutcome::Succeeded,
            oxideterm_audit::AuditEvidence::Protocol,
            None,
            None,
        );
        if self.state.selected_collection_id.as_deref() == Some(collection_id) {
            self.state.selected_collection_id = None;
        }
        self.state.external_edit = None;
        self.state.error = None;
        true
    }

    pub fn delete_knowledge_document(&mut self, document_id: &str, error_message: String) -> bool {
        let audit = knowledge_audit(document_id, "knowledge_document_delete");
        if oxideterm_ai::rag_remove_document(&(self.store)(), document_id).is_err() {
            audit.finish(
                oxideterm_audit::AuditOutcome::Failed,
                oxideterm_audit::AuditEvidence::Protocol,
                None,
                None,
            );
            self.state.error = Some(error_message);
            return false;
        }
        audit.finish(
            oxideterm_audit::AuditOutcome::Succeeded,
            oxideterm_audit::AuditEvidence::Protocol,
            None,
            None,
        );
        if self
            .state
            .external_edit
            .as_ref()
            .is_some_and(|edit| edit.doc_id == document_id)
        {
            self.state.external_edit = None;
        }
        self.state.error = None;
        true
    }

    pub fn prepare_knowledge_external_edit(
        &mut self,
        document_id: &str,
        edit_dir: PathBuf,
        error_message: String,
    ) -> Option<(PathBuf, oxideterm_settings_model::KnowledgeExternalEdit)> {
        if uuid::Uuid::parse_str(document_id).is_err() {
            self.state.error = Some(error_message);
            return None;
        }
        let store = (self.store)();
        let document = oxideterm_ai::rag_list_collections(&store, None)
            .ok()
            .into_iter()
            .flatten()
            .find_map(|collection| {
                oxideterm_ai::rag_list_documents(&store, &collection.id, None, Some(500))
                    .ok()
                    .and_then(|page| {
                        page.documents
                            .into_iter()
                            .find(|document| document.id == document_id)
                    })
            });
        let Some(document) = document else {
            self.state.error = Some(error_message);
            return None;
        };
        let Ok(content) = oxideterm_ai::rag_get_document_content(&store, document_id) else {
            self.state.error = Some(error_message);
            return None;
        };
        if std::fs::create_dir_all(&edit_dir).is_err() {
            self.state.error = Some(error_message);
            return None;
        }
        #[cfg(unix)]
        if set_private_permissions(&edit_dir, 0o700).is_err() {
            self.state.error = Some(error_message);
            return None;
        }
        let extension = if document.format == "plaintext" {
            "txt"
        } else {
            "md"
        };
        let path = edit_dir.join(format!("{}.{}", document.id, extension));
        if std::fs::write(&path, content).is_err() {
            self.state.error = Some(error_message);
            return None;
        }
        #[cfg(unix)]
        if set_private_permissions(&path, 0o600).is_err() {
            let _ = std::fs::remove_file(&path);
            self.state.error = Some(error_message);
            return None;
        }
        Some((
            path.clone(),
            oxideterm_settings_model::KnowledgeExternalEdit {
                doc_id: document.id,
                path,
                version: document.version,
            },
        ))
    }

    pub fn finish_knowledge_external_open(
        &mut self,
        edit: oxideterm_settings_model::KnowledgeExternalEdit,
        opened: bool,
        error_message: String,
    ) {
        if opened {
            self.state.external_edit = Some(edit);
            self.state.error = None;
        } else {
            let _ = std::fs::remove_file(&edit.path);
            self.state.error = Some(error_message);
        }
    }

    pub fn sync_knowledge_external_edit(
        &mut self,
        error_message: String,
    ) -> KnowledgeExternalSyncOutcome {
        let Some(edit) = self.state.external_edit.clone() else {
            return KnowledgeExternalSyncOutcome::NoEdit;
        };
        let Ok(content) = std::fs::read_to_string(&edit.path) else {
            let _ = std::fs::remove_file(&edit.path);
            self.state.external_edit = None;
            self.state.error = Some(error_message);
            return KnowledgeExternalSyncOutcome::Failed;
        };
        let store = (self.store)();
        match oxideterm_ai::rag_get_document_content(&store, &edit.doc_id) {
            Ok(current) if current == content => {
                let _ = std::fs::remove_file(&edit.path);
                self.state.external_edit = None;
                self.state.error = None;
                return KnowledgeExternalSyncOutcome::NoChanges;
            }
            Ok(_) => {}
            Err(_) => {
                self.state.error = Some(error_message);
                return KnowledgeExternalSyncOutcome::Failed;
            }
        }
        match oxideterm_ai::rag_update_document(&store, &edit.doc_id, content, Some(edit.version)) {
            Ok(_) => {
                let _ = std::fs::remove_file(&edit.path);
                self.state.external_edit = None;
                self.state.error = None;
                KnowledgeExternalSyncOutcome::Updated
            }
            Err(error) => {
                if error.contains("Version conflict") {
                    self.state.external_edit = None;
                }
                self.state.error = Some(error_message);
                KnowledgeExternalSyncOutcome::Failed
            }
        }
    }

    pub fn start_knowledge_import(
        &mut self,
        paths: impl std::future::Future<Output = Option<Vec<PathBuf>>> + 'static,
        collection_id: String,
        error_message: String,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.knowledge_import_task.is_some() {
            return false;
        }
        let store = (self.store)();
        let audit_context = oxideterm_audit::AuditContext::current_request()
            .or_else(oxideterm_audit::AuditContext::current);
        let task = cx.spawn(async move |entity, cx| {
            let Some(paths) = paths.await.filter(|paths| !paths.is_empty()) else {
                let _ = entity.update(cx, |entity, _cx| {
                    entity.knowledge_import_task = None;
                });
                return;
            };
            let total = paths.len();
            let mut audit = audit_context.as_ref().map(|context| {
                let mut context = context.clone();
                if context.source == oxideterm_audit::AuditSource::Application {
                    context.source = oxideterm_audit::AuditSource::User;
                }
                context.target = Some(oxideterm_audit::redact(&collection_id));
                context.operation(
                    oxideterm_audit::AuditCategory::File,
                    "knowledge_import",
                    None,
                )
            });
            let _ = entity.update(cx, |entity, cx| {
                entity.state.import_progress = Some((0, total));
                entity.state.error = None;
                entity.emit_knowledge_page_changed(cx);
            });
            let mut failed = false;
            let mut imported = 0usize;
            for (index, path) in paths.iter().enumerate() {
                if oxideterm_settings_model::import_knowledge_file(&store, &collection_id, path)
                    .is_err()
                {
                    failed = true;
                } else {
                    imported += 1;
                }
                let current = index + 1;
                let _ = entity.update(cx, |entity, cx| {
                    entity.state.import_progress = Some((current, total));
                    cx.notify();
                });
                if failed {
                    break;
                }
            }
            if let Some(audit) = audit.as_mut() {
                audit.summary(&format!("files={imported}/{total}"));
            }
            if let Some(audit) = audit {
                audit.finish(
                    if failed {
                        if imported > 0 {
                            oxideterm_audit::AuditOutcome::Partial
                        } else {
                            oxideterm_audit::AuditOutcome::Failed
                        }
                    } else {
                        oxideterm_audit::AuditOutcome::Succeeded
                    },
                    oxideterm_audit::AuditEvidence::Protocol,
                    None,
                    None,
                );
            }
            let _ = entity.update(cx, |entity, cx| {
                entity.state.import_progress = None;
                entity.state.error = failed.then_some(error_message);
                entity.knowledge_import_task = None;
                if imported != 0 {
                    cx.emit(KnowledgeDocumentsChanged);
                }
                entity.emit_knowledge_page_changed(cx);
            });
        });
        self.knowledge_import_task = Some(task);
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start_knowledge_embeddings(
        &mut self,
        collection_id: String,
        provider: oxideterm_ai::AiProviderView,
        model: String,
        requires_api_key: bool,
        missing_key_error: String,
        embedding_error: String,
        partial_failure_template: String,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.knowledge_embedding_task.is_some() {
            return false;
        }
        self.state.error = None;
        let store = (self.store)();
        let key_store = self.key_store.clone();
        let provider_id = provider.id.clone();
        let task_runtime = self.task_runtime.clone();
        let task = cx.spawn(async move |entity, cx| {
            let api_key = if requires_api_key {
                let key = task_runtime
                    .spawn_blocking(move || key_store.get_provider_key(&provider_id).ok().flatten())
                    .await
                    .ok()
                    .flatten();
                match key {
                    Some(key) if !key.trim().is_empty() => {
                        Some(oxideterm_ai::SharedAiProviderKey::new(key))
                    }
                    _ => {
                        let _ = entity.update(cx, |entity, cx| {
                            entity.state.embedding_config_expanded = true;
                            entity.state.error = Some(missing_key_error);
                            entity.knowledge_embedding_task = None;
                            entity.emit_knowledge_page_changed(cx);
                        });
                        return;
                    }
                }
            } else {
                None
            };
            let mut pending =
                match oxideterm_ai::rag_get_pending_embeddings(&store, &collection_id, Some(500)) {
                    Ok(pending) => pending,
                    Err(_) => {
                        let _ = entity.update(cx, |entity, cx| {
                            entity.state.error = Some(embedding_error);
                            entity.knowledge_embedding_task = None;
                            entity.emit_knowledge_page_changed(cx);
                        });
                        return;
                    }
                };
            if pending.is_empty() {
                let _ = entity.update(cx, |entity, _cx| {
                    entity.knowledge_embedding_task = None;
                });
                return;
            }
            let total = pending.len();
            let _ = entity.update(cx, |entity, cx| {
                entity.state.embedding_config_expanded = true;
                entity.state.embedding_progress = Some((0, total));
                entity.state.error = None;
                entity.emit_knowledge_page_changed(cx);
            });
            let mut processed = 0usize;
            let mut failed_count = 0usize;
            while !pending.is_empty() {
                let batch_len = pending
                    .len()
                    .min(oxideterm_settings_model::KNOWLEDGE_EMBEDDING_BATCH_SIZE);
                let mut batch = pending.drain(..batch_len).collect::<Vec<_>>();
                // Move raw chunk allocations into the provider boundary; do
                // not clone or pre-sanitize user document content here.
                let texts = batch
                    .iter_mut()
                    .map(|pending| std::mem::take(&mut pending.content))
                    .collect::<Vec<_>>();
                match oxideterm_ai::embed_texts(&provider, api_key.as_ref(), &model, texts).await {
                    Ok(vectors) => {
                        let embeddings = batch
                            .into_iter()
                            .zip(vectors)
                            .map(|(pending, vector)| oxideterm_ai::RagEmbeddingInputRequest {
                                chunk_id: pending.chunk_id,
                                vector,
                            })
                            .collect::<Vec<_>>();
                        if oxideterm_ai::rag_store_embeddings(
                            &store,
                            oxideterm_ai::RagStoreEmbeddingsRequest {
                                embeddings,
                                model_name: model.clone(),
                            },
                        )
                        .is_err()
                        {
                            failed_count += batch_len;
                        }
                    }
                    Err(_) => failed_count += batch_len,
                }
                processed += batch_len;
                let _ = entity.update(cx, |entity, cx| {
                    entity.state.embedding_progress = Some((processed, total));
                    cx.notify();
                });
            }
            let _ = entity.update(cx, |entity, cx| {
                entity.state.embedding_progress = None;
                entity.state.error = if failed_count == 0 {
                    None
                } else {
                    Some(
                        partial_failure_template
                            .replace("{{failed}}", &failed_count.to_string())
                            .replace("{{total}}", &total.to_string()),
                    )
                };
                entity.knowledge_embedding_task = None;
                entity.emit_knowledge_page_changed(cx);
            });
        });
        self.knowledge_embedding_task = Some(task);
        true
    }
}

#[cfg(unix)]
fn set_private_permissions(path: &std::path::Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(mode);
    std::fs::set_permissions(path, permissions)
}

pub const KNOWLEDGE_DOCUMENT_PAGE_SIZE: usize = 100;

fn read_knowledge_document_page(
    store: &oxideterm_ai::RagStore,
    collection_id: &str,
    page: usize,
) -> Result<(usize, oxideterm_ai::RagPaginatedDocuments), String> {
    let documents = oxideterm_ai::rag_list_documents(
        store,
        collection_id,
        Some(page * KNOWLEDGE_DOCUMENT_PAGE_SIZE),
        Some(KNOWLEDGE_DOCUMENT_PAGE_SIZE),
    )?;
    let last_page = documents.total.saturating_sub(1) / KNOWLEDGE_DOCUMENT_PAGE_SIZE;
    // Deleting the final document on a page returns the view to the last existing page.
    if page > last_page {
        return read_knowledge_document_page(store, collection_id, last_page);
    }
    Ok((page, documents))
}

#[cfg(test)]
mod settings_pagination_tests {
    use super::*;

    #[test]
    fn knowledge_pages_include_documents_after_the_first_hundred_and_recover_after_deletion() {
        let directory = tempfile::tempdir().unwrap();
        let store = oxideterm_ai::RagStore::new(directory.path()).unwrap();
        let collection = oxideterm_ai::rag_create_collection(
            &store,
            oxideterm_ai::RagCreateCollectionRequest {
                name: "Pagination".into(),
                scope: oxideterm_ai::RagDocScopeRequest::Global,
            },
        )
        .unwrap();
        let mut ids = Vec::new();
        for index in 0..101 {
            ids.push(
                oxideterm_ai::rag_create_blank_document(
                    &store,
                    oxideterm_ai::RagCreateBlankDocumentRequest {
                        collection_id: collection.id.clone(),
                        title: format!("Document {index}"),
                        format: "markdown".into(),
                    },
                )
                .unwrap()
                .id,
            );
        }
        let (page, documents) = read_knowledge_document_page(&store, &collection.id, 1).unwrap();
        assert_eq!(page, 1);
        assert_eq!(
            documents
                .documents
                .iter()
                .map(|doc| doc.title.as_str())
                .collect::<Vec<_>>(),
            ["Document 100"]
        );
        assert_eq!(documents.total, 101);

        oxideterm_ai::rag_remove_document(&store, &ids[100]).unwrap();
        let (page, documents) = read_knowledge_document_page(&store, &collection.id, 1).unwrap();
        assert_eq!(page, 0);
        assert_eq!(
            documents
                .documents
                .iter()
                .map(|doc| doc.title.clone())
                .collect::<Vec<_>>(),
            (0..100)
                .map(|index| format!("Document {index}"))
                .collect::<Vec<_>>()
        );
    }
}

pub struct EmbeddingJob {
    pub provider: oxideterm_ai::AiProviderView,
    pub model: String,
    pub requires_api_key: bool,
    pub missing_key_error: String,
    pub retrieval_error: String,
    pub partial_failure: String,
}

impl EmbeddingJob {
    pub fn prepare(settings: &oxideterm_settings::PersistedSettings, i18n: &oxideterm_i18n::I18n) -> Result<Self, String> {
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
            oxideterm_ai::AiEmbeddingProviderReason::NoProvider
            | oxideterm_ai::AiEmbeddingProviderReason::UnsupportedProvider => {
                return Err(i18n.t("settings_view.knowledge.error_no_embedding_support"));
            }
            oxideterm_ai::AiEmbeddingProviderReason::MissingModel => {
                return Err(i18n.t("settings_view.knowledge.error_no_embedding_model"));
            }
            oxideterm_ai::AiEmbeddingProviderReason::Ready | oxideterm_ai::AiEmbeddingProviderReason::MissingApiKey => {}
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

impl KnowledgePage {
    pub fn request_embeddings(&mut self, collection_id: String, job: Result<EmbeddingJob, String>, cx: &mut Context<Self>) {
        match job {
            Ok(job) => {
                self.start_knowledge_embeddings(
                    collection_id, job.provider, job.model, job.requires_api_key, job.missing_key_error,
                    job.retrieval_error, job.partial_failure, cx,
                );
            }
            Err(message) => {
                self.expand_knowledge_embedding_config();
                self.set_knowledge_error(message);
                cx.notify();
            }
        }
    }
}
