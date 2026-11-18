use super::*;
fn navigator_document(
    title: &str,
    format: &str,
    source_path: Option<&str>,
) -> oxideterm_ai::RagDocumentResponse {
    oxideterm_ai::RagDocumentResponse {
        id: "doc".to_string(),
        collection_id: "collection".to_string(),
        title: title.to_string(),
        source_path: source_path.map(str::to_string),
        format: format.to_string(),
        chunk_count: 1,
        indexed_at: 0,
        version: 0,
    }
}

#[test]
fn navigator_search_matches_title_and_body() {
    let terms = vec!["production".to_owned(), "数据库".to_owned()];
    assert!(knowledge_document_matches(
        "Production runbook",
        "数据库恢复步骤",
        &terms
    ));
    assert!(!knowledge_document_matches(
        "Production runbook",
        "Network recovery",
        &terms
    ));
    assert!(!knowledge_document_matches(
        "Local runbook",
        "数据库恢复步骤",
        &terms
    ));
    assert!(knowledge_document_matches("Untitled", "", &[]));
}

#[test]
fn autosave_completion_does_not_accept_pending_switch_without_user_confirmation() {
    let mut workspace = KnowledgeWorkspaceEntity::default();
    workspace.request_document_switch("next".to_string());

    assert_eq!(workspace.take_pending_document_after_save(), None);
    assert_eq!(workspace.pending_document_id.as_deref(), Some("next"));

    workspace.confirm_save_before_leaving();
    assert_eq!(
        workspace.take_pending_document_after_save().as_deref(),
        Some("next")
    );
}

#[test]
fn autosave_completion_does_not_quit_without_user_confirmation() {
    let mut workspace = KnowledgeWorkspaceEntity::default();
    workspace.request_app_quit();

    assert!(!workspace.take_pending_app_quit_after_save());
    assert!(workspace.pending_app_quit);

    workspace.confirm_save_before_leaving();
    assert!(workspace.take_pending_app_quit_after_save());
    assert!(!workspace.pending_app_quit);
}

#[test]
fn only_closing_knowledge_tab_invalidates_pending_document_load() {
    let mut workspace = KnowledgeWorkspaceEntity::default();
    workspace.register_tab(TabId(7));
    let generation = workspace.begin_document_load("doc".to_string());
    workspace.close_tab(TabId(8));
    assert_eq!(workspace.tab_id(), Some(TabId(7)));
    assert_eq!(workspace.load_generation, generation);
    workspace.close_tab(TabId(7));
    assert_ne!(workspace.load_generation, generation);
    assert_eq!(workspace.tab_id(), None);
    assert_eq!(workspace.selected_document_id, None);
}

#[test]
fn created_document_is_deduplicated_and_survives_stale_navigator_refresh() {
    let mut existing = navigator_document("Existing", "markdown", None);
    existing.id = "existing".to_string();
    let mut created = navigator_document("Created", "markdown", None);
    created.id = "created".to_string();
    let mut workspace = KnowledgeWorkspaceEntity::default();
    workspace.navigator_snapshot.selected_collection_id = Some("collection".to_string());
    workspace.navigator_snapshot.documents = Arc::new(vec![existing]);
    let stale_generation = workspace.begin_navigator_refresh(true).unwrap();

    workspace.insert_created_document(created.clone());
    workspace.insert_created_document(created);
    assert_eq!(
        workspace
            .navigator_snapshot
            .documents
            .iter()
            .map(|document| document.id.as_str())
            .collect::<Vec<_>>(),
        vec!["existing", "created"]
    );
    let current_generation = workspace.begin_navigator_refresh(true).unwrap();

    assert!(
        !workspace
            .install_navigator_snapshot(stale_generation, KnowledgeNavigatorSnapshot::default(),)
    );
    assert_eq!(
        workspace
            .navigator_snapshot
            .documents
            .iter()
            .map(|document| document.id.as_str())
            .collect::<Vec<_>>(),
        vec!["existing", "created"]
    );
    assert_ne!(stale_generation, current_generation);
}
