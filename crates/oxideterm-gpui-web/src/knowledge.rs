//! Mounts OxideTerm's real desktop `KnowledgeWorkspaceView` + `KnowledgeDocumentEditor`
//! in the browser using an in-memory `AiKnowledgeHost` implementation.

use crate::host::{Action, Workspace};
use gpui::{prelude::*, *};
use oxideterm_ai::{
    DocScope, RagAddDocumentRequest, RagCollectionResponse, RagCreateBlankDocumentRequest,
    RagCreateCollectionRequest, RagDocScopeRequest as DocScopeRequest, RagDocumentContentResponse,
    RagDocumentResponse, RagDocumentSaveOutcome, RagKeywordIndexState, RagPaginatedDocuments,
    RagSearchRequest, RagSearchResultResponse, RagSemanticIndexState, RagStatsResponse,
};
use oxideterm_gpui_ai::{
    bridge::{
        HostFuture,
        knowledge::{AiKnowledgeHost, DocumentMetadata, KnowledgeSaveError},
    },
    knowledge::{
        editor::{KnowledgePreviewFrame, KnowledgePreviewSource},
        page::{EmbeddingJob, KnowledgeJobs, KnowledgePage},
        workspace::{
            KnowledgeBindings, KnowledgeEnvironment, KnowledgeInput, KnowledgeInputFrame,
            KnowledgeInputRequest, KnowledgeRequest, KnowledgeViews, KnowledgeWorkspaceEntity,
            KnowledgeWorkspaceView,
        },
    },
};
use oxideterm_gpui_markdown::MarkdownOptions;
use oxideterm_gpui_ui::text_input::{TextInputAnchorId, TextInputViewport};
use std::{
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

struct StoredDoc {
    meta: RagDocumentResponse,
    content: String,
}

struct StoreInner {
    collections: Vec<RagCollectionResponse>,
    documents: Vec<StoredDoc>,
    next_id: u64,
    revision: u64,
}

pub struct WebKnowledgeHost {
    inner: Mutex<StoreInner>,
    revision: AtomicU64,
}

impl WebKnowledgeHost {
    pub fn seeded() -> Self {
        let now = 1_764_000_000_000i64;
        let collection_id = "col-workspace".to_string();
        let collections = vec![RagCollectionResponse {
            id: collection_id.clone(),
            name: "Workspace Notes".into(),
            scope: DocScope::Global,
            created_at: now,
            updated_at: now,
        }];
        let documents = vec![
            StoredDoc {
                meta: RagDocumentResponse {
                    id: "doc-1".into(),
                    collection_id: collection_id.clone(),
                    title: "Operations Runbook".into(),
                    source_path: None,
                    format: "markdown".into(),
                    chunk_count: 3,
                    indexed_at: now,
                    version: 1,
                },
                content: "# Operations Runbook\n\nUse **OxideTerm** to inspect live services, inspect logs, and manage SSH / BLE sessions.\n\n## Quick Checks\n\n- Inspect systemd units in **Host Tools → Services**\n- Inspect listening sockets in **Host Tools → Ports**\n- Run ad-hoc diagnostics with **OxideSens**\n\n```bash\nsystemctl status nginx --no-pager\nss -tulnp\n```\n"
                    .into(),
            },
            StoredDoc {
                meta: RagDocumentResponse {
                    id: "doc-2".into(),
                    collection_id,
                    title: "BLE Hardware Notes".into(),
                    source_path: None,
                    format: "markdown".into(),
                    chunk_count: 2,
                    indexed_at: now,
                    version: 1,
                },
                content: "# BLE Hardware Notes\n\nOxideTerm Web connects directly to GATT peripherals via Chrome/Edge **Web Bluetooth**.\n\n## Supported UART Profiles\n\n- **Nordic UART Service (NUS)** — `6e400001-b5a3-f393-e0a9-e50e24dcca9e`\n- **HM-10 / CC2541** — `0000ffe0-0000-1000-8000-00805f9b34fb`\n- **Microchip Transparent UART** — `49535343-fe7d-4ae5-8fa9-9fafd205e455`\n"
                    .into(),
            },
        ];
        Self {
            inner: Mutex::new(StoreInner {
                collections,
                documents,
                next_id: 3,
                revision: 1,
            }),
            revision: AtomicU64::new(1),
        }
    }

    fn bump_revision(&self, inner: &mut StoreInner) {
        inner.revision += 1;
        self.revision.store(inner.revision, Ordering::Relaxed);
    }
}

impl AiKnowledgeHost for WebKnowledgeHost {
    fn list_collections(
        &self,
        _scope_filter: Option<&str>,
    ) -> Result<Vec<RagCollectionResponse>, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "store poisoned".to_string())?;
        Ok(guard.collections.clone())
    }

    fn list_documents(
        &self,
        collection_id: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<RagPaginatedDocuments, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "store poisoned".to_string())?;
        let all: Vec<RagDocumentResponse> = guard
            .documents
            .iter()
            .filter(|d| d.meta.collection_id == collection_id)
            .map(|d| d.meta.clone())
            .collect();
        let total = all.len();
        let start = offset.unwrap_or(0).min(total);
        let end = limit.map_or(total, |l| start.saturating_add(l).min(total));
        Ok(RagPaginatedDocuments {
            documents: all[start..end].to_vec(),
            total,
        })
    }

    fn get_document_content(&self, doc_id: &str) -> Result<String, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "store poisoned".to_string())?;
        guard
            .documents
            .iter()
            .find(|d| d.meta.id == doc_id)
            .map(|d| d.content.clone())
            .ok_or_else(|| format!("Document {doc_id} not found"))
    }

    fn create_collection(
        &self,
        request: RagCreateCollectionRequest,
    ) -> Result<RagCollectionResponse, String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "store poisoned".to_string())?;
        let id = format!("col-{}", guard.next_id);
        guard.next_id += 1;
        let now = 1_764_000_000_000i64 + guard.next_id as i64;
        let scope = match request.scope {
            DocScopeRequest::Global => DocScope::Global,
            DocScopeRequest::Connection { connection_id } => DocScope::Connection(connection_id),
        };
        let col = RagCollectionResponse {
            id,
            name: request.name,
            scope,
            created_at: now,
            updated_at: now,
        };
        guard.collections.push(col.clone());
        self.bump_revision(&mut guard);
        Ok(col)
    }

    fn create_blank_document(
        &self,
        request: RagCreateBlankDocumentRequest,
    ) -> Result<RagDocumentResponse, String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "store poisoned".to_string())?;
        let id = format!("doc-{}", guard.next_id);
        guard.next_id += 1;
        let now = 1_764_000_000_000i64 + guard.next_id as i64;
        let meta = RagDocumentResponse {
            id,
            collection_id: request.collection_id,
            title: request.title.clone(),
            source_path: None,
            format: if request.format.trim().is_empty() {
                "markdown".into()
            } else {
                request.format
            },
            chunk_count: 1,
            indexed_at: now,
            version: 1,
        };
        let content = format!("# {}\n\n", request.title);
        guard.documents.push(StoredDoc {
            meta: meta.clone(),
            content,
        });
        self.bump_revision(&mut guard);
        Ok(meta)
    }

    fn delete_collection(&self, collection_id: &str) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "store poisoned".to_string())?;
        guard.collections.retain(|c| c.id != collection_id);
        guard
            .documents
            .retain(|d| d.meta.collection_id != collection_id);
        self.bump_revision(&mut guard);
        Ok(())
    }

    fn remove_document(&self, doc_id: &str) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "store poisoned".to_string())?;
        guard.documents.retain(|d| d.meta.id != doc_id);
        self.bump_revision(&mut guard);
        Ok(())
    }

    fn add_document(&self, request: RagAddDocumentRequest) -> Result<RagDocumentResponse, String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "store poisoned".to_string())?;
        let id = format!("doc-{}", guard.next_id);
        guard.next_id += 1;
        let now = 1_764_000_000_000i64 + guard.next_id as i64;
        let meta = RagDocumentResponse {
            id,
            collection_id: request.collection_id,
            title: request.title,
            source_path: request.source_path,
            format: request.format,
            chunk_count: 1,
            indexed_at: now,
            version: 1,
        };
        guard.documents.push(StoredDoc {
            meta: meta.clone(),
            content: request.content,
        });
        self.bump_revision(&mut guard);
        Ok(meta)
    }

    fn search(&self, request: RagSearchRequest) -> Result<Vec<RagSearchResultResponse>, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "store poisoned".to_string())?;
        let query = request.query.trim().to_lowercase();
        let top_k = request.top_k.unwrap_or(10);
        let mut results = Vec::new();
        for doc in &guard.documents {
            if !request.collection_ids.is_empty()
                && !request.collection_ids.contains(&doc.meta.collection_id)
            {
                continue;
            }
            if query.is_empty()
                || doc.meta.title.to_lowercase().contains(&query)
                || doc.content.to_lowercase().contains(&query)
            {
                results.push(RagSearchResultResponse {
                    chunk_id: format!("{}-0", doc.meta.id),
                    doc_id: doc.meta.id.clone(),
                    doc_title: doc.meta.title.clone(),
                    section_path: None,
                    content: doc.content.clone(),
                    score: 1.0,
                    source: "keyword".into(),
                });
                if results.len() >= top_k {
                    break;
                }
            }
        }
        Ok(results)
    }

    fn get_collection_stats(&self, collection_id: &str) -> Result<RagStatsResponse, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "store poisoned".to_string())?;
        let docs: Vec<_> = guard
            .documents
            .iter()
            .filter(|d| d.meta.collection_id == collection_id)
            .collect();
        let chunk_count = docs.iter().map(|d| d.meta.chunk_count).sum();
        Ok(RagStatsResponse {
            doc_count: docs.len(),
            chunk_count,
            embedded_chunk_count: chunk_count,
            last_updated: 1_764_000_000_000,
        })
    }

    fn keyword_index_state(&self) -> RagKeywordIndexState {
        RagKeywordIndexState::Ready
    }

    fn document_semantic_index_state(
        &self,
        _doc_id: &str,
    ) -> Result<RagSemanticIndexState, String> {
        Ok(RagSemanticIndexState::Ready)
    }

    fn save_document(
        &self,
        doc_id: &str,
        content: String,
        expected_version: Option<u64>,
    ) -> Result<RagDocumentSaveOutcome, KnowledgeSaveError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| KnowledgeSaveError::Failed("store poisoned".into()))?;
        let Some(doc) = guard.documents.iter_mut().find(|d| d.meta.id == doc_id) else {
            return Err(KnowledgeSaveError::Failed(format!(
                "Document {doc_id} not found"
            )));
        };
        if let Some(expected) = expected_version
            && doc.meta.version != expected
        {
            return Err(KnowledgeSaveError::Conflict);
        }
        doc.content = content;
        doc.meta.version += 1;
        let saved_meta = doc.meta.clone();
        self.bump_revision(&mut guard);
        Ok(RagDocumentSaveOutcome {
            document: saved_meta,
            keyword_index: RagKeywordIndexState::Ready,
            semantic_index: RagSemanticIndexState::Ready,
        })
    }

    fn get_document(&self, doc_id: &str) -> Result<RagDocumentContentResponse, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "store poisoned".to_string())?;
        let doc = guard
            .documents
            .iter()
            .find(|d| d.meta.id == doc_id)
            .ok_or_else(|| format!("Document {doc_id} not found"))?;
        Ok(RagDocumentContentResponse {
            document: doc.meta.clone(),
            content: doc.content.clone(),
            semantic_index: RagSemanticIndexState::Ready,
        })
    }

    fn stats_revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }

    fn rename_collection(&self, id: &str, name: &str) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "store poisoned".to_string())?;
        if let Some(col) = guard.collections.iter_mut().find(|c| c.id == id) {
            col.name = name.to_owned();
            self.bump_revision(&mut guard);
        }
        Ok(())
    }

    fn edit_metadata(
        &self,
        id: &str,
        name: Option<&str>,
        collection: Option<&str>,
        _version: u64,
    ) -> Result<DocumentMetadata, String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "store poisoned".to_string())?;
        let doc = guard
            .documents
            .iter_mut()
            .find(|d| d.meta.id == id)
            .ok_or_else(|| format!("Document {id} not found"))?;
        if let Some(title) = name {
            doc.meta.title = title.to_owned();
        }
        if let Some(col) = collection {
            doc.meta.collection_id = col.to_owned();
        }
        doc.meta.version += 1;
        let out = DocumentMetadata {
            id: doc.meta.id.clone(),
            title: doc.meta.title.clone(),
            collection_id: doc.meta.collection_id.clone(),
            version: doc.meta.version,
        };
        self.bump_revision(&mut guard);
        Ok(out)
    }

    fn transfer(
        &self,
        id: &str,
        destination: String,
        cut: bool,
        copy_title: String,
    ) -> Result<(String, u64, bool), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "store poisoned".to_string())?;
        let Some(pos) = guard.documents.iter().position(|d| d.meta.id == id) else {
            return Err(format!("Document {id} not found"));
        };
        if cut {
            let changed = guard.documents[pos].meta.collection_id != destination;
            guard.documents[pos].meta.collection_id = destination;
            guard.documents[pos].meta.version += 1;
            let version = guard.documents[pos].meta.version;
            self.bump_revision(&mut guard);
            Ok((id.to_owned(), version, changed))
        } else {
            let mut meta = guard.documents[pos].meta.clone();
            let content = guard.documents[pos].content.clone();
            let orig_title = meta.title.clone();
            let new_id = format!("doc-{}", guard.next_id);
            guard.next_id += 1;
            meta.id = new_id.clone();
            meta.collection_id = destination;
            meta.title = copy_title.replace("{{title}}", &orig_title);
            meta.version = 1;
            guard.documents.push(StoredDoc { meta, content });
            self.bump_revision(&mut guard);
            Ok((new_id, 1, true))
        }
    }
}

struct WebKnowledgeJobs;

impl KnowledgeJobs for WebKnowledgeJobs {
    fn reindex(
        &self,
        _collection: String,
        _cancel: Arc<AtomicBool>,
    ) -> HostFuture<Result<(), String>> {
        Box::pin(async { Ok(()) })
    }

    fn embed(&self, _collection: String, _job: EmbeddingJob) -> HostFuture<Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
}

pub struct WebKnowledge {
    pub views: KnowledgeViews,
    pub page: Entity<KnowledgePage>,
    pub active_input: Option<KnowledgeInput>,
}

impl WebKnowledge {
    pub fn new(focus: &FocusHandle, cx: &mut Context<Workspace>) -> Self {
        let host: Arc<dyn AiKnowledgeHost> = Arc::new(WebKnowledgeHost::seeded());
        let jobs: Rc<dyn KnowledgeJobs> = Rc::new(WebKnowledgeJobs);
        let page = cx.new(|_| KnowledgePage::with_bridge(host, jobs));
        let state = cx.new(|_| KnowledgeWorkspaceEntity::default());
        let owner = cx.weak_entity();

        let env_owner = owner.clone();
        let preview_owner = owner.clone();
        let frame_owner = owner.clone();
        let input_owner = owner.clone();
        let req_owner = owner.clone();

        let preview: KnowledgePreviewSource = Rc::new(move |_cx| {
            let _owner = preview_owner.upgrade()?;
            Some(KnowledgePreviewFrame {
                options: MarkdownOptions::default(),
                text: None,
                actions: None,
            })
        });

        let bindings = KnowledgeBindings {
            page: page.clone(),
            environment: Rc::new(move |cx| {
                let owner = env_owner.upgrade()?;
                let h = owner.read(cx);
                let prefs = h
                    .settings
                    .read(cx)
                    .settings(cx)
                    .window_ui
                    .knowledge_editor
                    .clone();
                Some(KnowledgeEnvironment {
                    tokens: h.tokens,
                    i18n: h.i18n.clone(),
                    background: false,
                    preferences: prefs,
                })
            }),
            preview,
            frame: Rc::new(move |input, cx| {
                let owner = frame_owner.upgrade()?;
                let h = owner.read(cx);
                let focused = h.knowledge.active_input == Some(input);
                let anchor_id = TextInputAnchorId(match input {
                    KnowledgeInput::Search => 81_001,
                    KnowledgeInput::Rename => 81_002,
                });
                Some(KnowledgeInputFrame {
                    focused,
                    caret_visible: focused,
                    selected_range: None,
                    active_offset: None,
                    marked_text: None,
                    anchor_id,
                    viewport: TextInputViewport::default(),
                    update_anchor: Rc::new(|_| {}),
                })
            }),
            input: Rc::new(move |request, window, cx| {
                let input_owner = input_owner.clone();
                window.defer(cx, move |window, cx| {
                    let _ = input_owner.update(cx, |h, cx| match request {
                        KnowledgeInputRequest::Focus(input)
                        | KnowledgeInputRequest::PointerDown(input, _) => {
                            h.knowledge.active_input = Some(input);
                            h.sidebar_search_focused = false;
                            window.focus(&h.focus, cx);
                            cx.notify();
                        }
                        KnowledgeInputRequest::Clear
                        | KnowledgeInputRequest::DismissSearchOutside(_) => {
                            h.knowledge.active_input = None;
                            cx.notify();
                        }
                        KnowledgeInputRequest::Drag(_) => {}
                    });
                });
            }),
            request: Rc::new(move |request, window, cx| {
                let _ = req_owner.update(cx, |h, cx| match request {
                    KnowledgeRequest::Settings => {
                        h.settings.update(cx, |s, cx| {
                            s.open_section(oxideterm_gpui_settings_view::SettingsTab::Knowledge, cx)
                        });
                        if let Some(w) = window {
                            h.action(Action::Settings, w, cx);
                        }
                    }
                    KnowledgeRequest::SaveEditorPreferences(prefs) => {
                        h.settings.update(cx, |s, cx| {
                            s.edit_preferences(cx, |settings| {
                                settings.window_ui.knowledge_editor = prefs;
                            });
                        });
                    }
                    KnowledgeRequest::Close(tab_id, _) => {
                        h.close(tab_id, cx);
                    }
                    KnowledgeRequest::PresentCreationDialog => {
                        // Create a new document directly inside the selected collection.
                        let col_id = h
                            .knowledge
                            .views
                            .state
                            .read(cx)
                            .input_value(KnowledgeInput::Search)
                            .map(|_| ());
                        let _ = col_id;
                        let host = h.knowledge.page.read(cx).knowledge_host();
                        if let Ok(cols) = host.list_collections(None)
                            && let Some(col) = cols.first()
                        {
                            let count = host
                                .list_documents(&col.id, None, None)
                                .map(|p| p.total)
                                .unwrap_or(0);
                            if host
                                .create_blank_document(RagCreateBlankDocumentRequest {
                                    collection_id: col.id.clone(),
                                    title: format!("Untitled Note {}", count + 1),
                                    format: "markdown".into(),
                                })
                                .is_ok()
                            {
                                h.knowledge.views.refresh(cx);
                            }
                        }
                    }
                    _ => {}
                });
            }),
            embedding_job: Rc::new(|_| None),
        };

        let view =
            cx.new(|cx| KnowledgeWorkspaceView::new(state.clone(), bindings, focus.clone(), cx));
        let views = KnowledgeViews { view, state };
        views.refresh(cx);

        Self {
            views,
            page,
            active_input: None,
        }
    }
}

impl Workspace {
    pub(crate) fn knowledge_input_key(
        &mut self,
        e: &KeyDownEvent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let on_knowledge = self
            .layout
            .active_tab()
            .is_some_and(|tab| tab.kind == oxideterm_workspace::TabKind::Knowledge);
        if !on_knowledge {
            self.knowledge.active_input = None;
            return false;
        }
        if let Some(input) = self.knowledge.active_input {
            if e.keystroke.modifiers.platform || e.keystroke.modifiers.control {
                return false;
            }
            let current = self
                .knowledge
                .views
                .state
                .read(cx)
                .input_value(input)
                .unwrap_or("")
                .to_owned();
            match e.keystroke.key.as_str() {
                "escape" => {
                    if input == KnowledgeInput::Search {
                        self.knowledge.views.view.update(cx, |view, cx| {
                            view.set_input_value(input, String::new(), cx);
                        });
                    }
                    self.knowledge.active_input = None;
                    cx.notify();
                    return true;
                }
                "enter" => {
                    self.knowledge.active_input = None;
                    cx.notify();
                    return true;
                }
                "backspace" => {
                    let mut next = current;
                    next.pop();
                    self.knowledge.views.view.update(cx, |view, cx| {
                        view.set_input_value(input, next, cx);
                    });
                    cx.notify();
                    return true;
                }
                _ => {
                    if let Some(ch) = &e.keystroke.key_char
                        && !ch.is_empty()
                        && !ch.chars().any(|c| c.is_control())
                    {
                        let mut next = current;
                        next.push_str(ch);
                        self.knowledge.views.view.update(cx, |view, cx| {
                            view.set_input_value(input, next, cx);
                        });
                        cx.notify();
                        return true;
                    }
                }
            }
            return false;
        }
        self.knowledge
            .views
            .handle_key(true, self.knowledge.active_input, e, w, cx)
    }
}
