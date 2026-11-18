use super::*;
use gpui::{AppContext, TestAppContext};
use std::time::Instant;

fn new_page(path: PathBuf, cx: &mut TestAppContext) -> gpui::Entity<KnowledgePage> {
    let store = Arc::new(oxideterm_ai::RagStore::new(&path).unwrap());
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap(),
    );
    cx.new(|_| {
        KnowledgePage::new(
            Rc::new(move || store.clone()),
            runtime,
            oxideterm_ai::AiProviderKeyStore::new(),
        )
    })
}

#[gpui::test]
fn knowledge_document_creation_failure_and_window_transfer_preserve_the_dialog_owner(
    cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let entity = new_page(directory.path().to_path_buf(), cx);
    entity.update(cx, |entity, cx| {
        let store = entity.rag_store();
        assert!(entity.focus_settings_input(SettingsInput::KnowledgeCollectionName, cx));
        assert!(entity.replace_settings_input(
            SettingsInput::KnowledgeCollectionName,
            None,
            "部署运维手册",
            cx,
        ));
        assert!(entity.create_knowledge_collection("failed".to_string()));
        let first = oxideterm_ai::rag_list_collections(&store, None)
            .unwrap()
            .remove(0);
        assert_eq!(first.name, "部署运维手册");
        let second = oxideterm_ai::rag_create_collection(
            &store,
            oxideterm_ai::RagCreateCollectionRequest {
                name: "second".to_string(),
                scope: oxideterm_ai::RagDocScopeRequest::Global,
            },
        )
        .unwrap();
        let owner_window_id: gpui::WindowId = 41_u64.into();
        entity.open_knowledge_document_dialog(first.id.clone(), owner_window_id, true);
        entity.select_knowledge_collection(second.id.clone());
        assert!(entity.focus_settings_input(SettingsInput::KnowledgeDocumentTitle, cx));
        assert!(entity.replace_settings_input(
            SettingsInput::KnowledgeDocumentTitle,
            None,
            "Runbook",
            cx,
        ));

        let (document, open_in_workspace) = entity
            .create_blank_knowledge_document("failed".to_string())
            .unwrap();

        assert_eq!(document.collection_id, first.id);
        assert!(open_in_workspace);
        assert!(entity.knowledge_document_dialog_owned_by(owner_window_id));
        assert_eq!(
            oxideterm_ai::rag_list_documents(&store, &second.id, None, None)
                .unwrap()
                .total,
            0
        );

        oxideterm_ai::rag_delete_collection(&store, &first.id).unwrap();
        assert!(entity.replace_settings_input(
            SettingsInput::KnowledgeDocumentTitle,
            None,
            "Runbook",
            cx,
        ));

        assert!(
            entity
                .create_blank_knowledge_document("failed".to_string())
                .is_none()
        );
        assert!(entity.knowledge_document_dialog_open());
        assert_eq!(entity.knowledge_new_document_title(), "Runbook");
        assert_eq!(entity.knowledge_new_document_error(), Some("failed"));
        assert_eq!(entity.knowledge_error(), None);

        let second_window_id: gpui::WindowId = 44_u64.into();
        assert!(entity.replace_settings_input(
            SettingsInput::KnowledgeDocumentTitle,
            Some(0..7),
            "Draft title",
            cx,
        ));

        entity.open_knowledge_document_dialog(second.id, second_window_id, false);
        assert!(entity.knowledge_document_dialog_owned_by(owner_window_id));
        assert!(!entity.knowledge_document_dialog_owned_by(second_window_id));
        assert_eq!(entity.knowledge_new_document_title(), "Draft title");

        assert!(entity.transfer_knowledge_document_dialog_owner(
            owner_window_id,
            second_window_id,
            cx,
        ));
        assert!(entity.knowledge_document_dialog_owned_by(second_window_id));
        assert!(entity.dismiss_knowledge_document_dialog_for_window(second_window_id, cx));
        assert!(!entity.knowledge_document_dialog_open());
        assert_eq!(entity.knowledge_new_document_title(), "");
        assert_eq!(entity.focused_settings_input(), None);
    });
}

#[gpui::test]
#[ignore = "manual knowledge statistics redraw cost comparison"]
fn knowledge_statistics_redraw_cost(cx: &mut TestAppContext) {
    use serde_json::json;
    let directory = tempfile::tempdir().unwrap();
    let entity = new_page(directory.path().to_path_buf(), cx);
    let store = entity.read_with(cx, |entity, _| entity.rag_store());
    store.create_collection(&serde_json::from_value(json!({ "id": "stats", "name": "Statistics", "scope": "Global", "created_at": 1, "updated_at": 1 })).unwrap()).unwrap();
    let chunks = (0..1024).map(|i| serde_json::from_value(json!({
            "id": format!("chunk-{i:04}"), "doc_id": "document", "section_path": null,
            "content": format!("Synthetic chunk {i}"), "tokens_estimate": 4, "offset": i * 32, "length": 20,
        })).unwrap()).collect::<Vec<_>>();
    store.add_document(&serde_json::from_value(json!({
            "id": "document", "collection_id": "stats", "title": "Statistics", "source_path": null,
            "format": "PlainText", "content_hash": "fixture", "indexed_at": 2, "chunk_count": chunks.len(), "version": 1,
        })).unwrap(), &chunks, None).unwrap();
    store.store_embeddings_batch(&(0..768).map(|i| serde_json::from_value(json!({
            "chunk_id": format!("chunk-{i:04}"), "vector": vec![0.5; 1536], "model_name": "fixture", "dimensions": 1536,
        })).unwrap()).collect::<Vec<_>>()).unwrap();
    let start = Instant::now();
    for _ in 0..120 {
        let stats = store.get_collection_stats("stats").unwrap();
        assert_eq!(
            (
                stats.doc_count,
                stats.chunk_count,
                stats.embedded_chunk_count
            ),
            (1, 1024, 768)
        );
    }
    eprintln!(
        "KNOWLEDGE_STATS direct_120_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.0
    );
    let start = Instant::now();
    entity.update(cx, |entity, cx| {
        assert!(
            entity
                .request_knowledge_collection_stats("stats", cx)
                .is_none()
        );
    });
    cx.run_until_parked();
    eprintln!(
        "KNOWLEDGE_STATS initial_async_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.0
    );
    let start = Instant::now();
    for _ in 0..120 {
        let stats = entity.update(cx, |entity, cx| {
            entity
                .request_knowledge_collection_stats("stats", cx)
                .unwrap()
        });
        assert_eq!(
            (
                stats.doc_count,
                stats.chunk_count,
                stats.embedded_chunk_count
            ),
            (1, 1024, 768)
        );
    }
    eprintln!(
        "KNOWLEDGE_STATS cached_120_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.0
    );
}

#[gpui::test]
fn knowledge_statistics_discard_stale_reads_and_refresh_after_writes(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let entity = new_page(directory.path().to_path_buf(), cx);
    let store = entity.read_with(cx, |entity, _| entity.rag_store());
    let collection = |name: &str| {
        oxideterm_ai::rag_create_collection(
            &store,
            oxideterm_ai::RagCreateCollectionRequest {
                name: name.into(),
                scope: oxideterm_ai::RagDocScopeRequest::Global,
            },
        )
        .unwrap()
        .id
    };
    let first = collection("First");
    let second = collection("Second");
    // Both requests arrive before background delivery; the first result must not publish.
    entity.update(cx, |entity, cx| {
        assert!(
            entity
                .request_knowledge_collection_stats(&first, cx)
                .is_none()
        );
        assert!(
            entity
                .request_knowledge_collection_stats(&second, cx)
                .is_none()
        );
    });
    cx.run_until_parked();
    entity.update(cx, |entity, cx| {
        assert!(entity.knowledge_stats_cache_key().is_none());
        assert!(
            entity
                .request_knowledge_collection_stats(&second, cx)
                .is_none()
        );
    });
    // A committed write between request and delivery must invalidate that read as well.
    let document = oxideterm_ai::rag_create_blank_document(
        &store,
        oxideterm_ai::RagCreateBlankDocumentRequest {
            collection_id: second.clone(),
            title: "Note".into(),
            format: "markdown".into(),
        },
    )
    .unwrap();
    cx.run_until_parked();
    assert!(entity.read_with(cx, |entity, _| entity.knowledge_stats_cache_key().is_none()));
    let refresh = |cx: &mut TestAppContext, expected: (usize, usize, usize)| {
        entity.update(cx, |entity, cx| {
            assert!(
                entity
                    .request_knowledge_collection_stats(&second, cx)
                    .is_none()
            );
        });
        cx.run_until_parked();
        for _ in 0..2 {
            let stats = entity.update(cx, |entity, cx| {
                entity
                    .request_knowledge_collection_stats(&second, cx)
                    .unwrap()
            });
            assert_eq!(
                (
                    stats.doc_count,
                    stats.chunk_count,
                    stats.embedded_chunk_count
                ),
                expected
            );
        }
    };
    refresh(cx, (1, 0, 0));
    oxideterm_ai::rag_save_document(&store, &document.id, "A single paragraph.".into(), None)
        .unwrap();
    refresh(cx, (1, 1, 0));
    let chunks = store.get_chunks_for_doc(&document.id).unwrap();
    store.as_ref().clone().store_embedding(&serde_json::from_value(serde_json::json!({
            "chunk_id": chunks[0].id, "vector": [1.0, 0.0], "dimensions": 2, "model_name": "fixture",
        })).unwrap()).unwrap();
    refresh(cx, (1, 1, 1));
    oxideterm_ai::rag_remove_document(&store, &document.id).unwrap();
    refresh(cx, (0, 0, 0));
    // Releasing the owner while another collection is pending must not retain the entity.
    entity.update(cx, |entity, cx| {
        entity.request_knowledge_collection_stats(&first, cx);
    });
    let weak = entity.downgrade();
    drop(entity);
    cx.run_until_parked();
    assert!(weak.upgrade().is_none());
}

#[gpui::test]
fn hidden_knowledge_page_still_receives_import_completion(cx: &mut TestAppContext) {
    let data_dir = std::env::temp_dir().join(format!(
        "oxideterm-knowledge-entity-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&data_dir).expect("create Knowledge test directory");
    let document_path = data_dir.join("guide.md");
    std::fs::write(&document_path, "# Guide\nEntity-owned import")
        .expect("write Knowledge test document");
    let entity = new_page(data_dir.clone(), cx);
    let store = entity.read_with(cx, |entity, _cx| entity.rag_store());
    let collection = oxideterm_ai::rag_create_collection(
        &store,
        oxideterm_ai::RagCreateCollectionRequest {
            name: "Hidden page".to_string(),
            scope: oxideterm_ai::RagDocScopeRequest::Global,
        },
    )
    .expect("create Knowledge test collection");

    entity.update(cx, |entity, cx| {
        assert!(entity.start_knowledge_import(
            std::future::ready(Some(vec![document_path])),
            collection.id.clone(),
            "safe import failure".to_string(),
            cx,
        ));
        assert!(entity.knowledge_import_task.is_some());
    });
    // No settings page or workspace callback is present while the task
    // finishes; the entity must still apply the completion reliably.
    cx.run_until_parked();

    entity.read_with(cx, |entity, _cx| {
        assert!(entity.knowledge_import_task.is_none());
        assert_eq!(entity.knowledge_import_progress(), None);
        assert_eq!(entity.knowledge_error(), None);
    });
    let documents = oxideterm_ai::rag_list_documents(&store, &collection.id, None, Some(10))
        .expect("list imported Knowledge documents");
    assert_eq!(documents.documents.len(), 1);
    assert_eq!(
        oxideterm_ai::rag_get_document_content(&store, &documents.documents[0].id).unwrap(),
        "# Guide\nEntity-owned import"
    );
    drop(entity);
    drop(store);
    std::fs::remove_dir_all(&data_dir).expect("remove Knowledge test directory");
}

#[gpui::test]
fn releasing_the_page_cancels_the_import_receiver_and_reindex_token(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let entity = new_page(directory.path().to_path_buf(), cx);
    let (paths, rx) = tokio::sync::oneshot::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    entity.update(cx, |page, cx| {
        page.reindex_cancel = Some(cancel.clone());
        assert!(page.start_knowledge_import(
            async move { rx.await.ok().flatten() },
            "notes".into(),
            "Import failed".into(),
            cx,
        ));
    });
    drop(entity);
    cx.update(|_| {});
    cx.run_until_parked();
    assert!(cancel.load(Ordering::Acquire));
    assert!(paths.send(Some(Vec::new())).is_err());
}
