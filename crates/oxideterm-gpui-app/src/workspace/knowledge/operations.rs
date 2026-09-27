use super::*;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct NoteClipboard {
    document_id: String,
    cut: bool,
}

impl WorkspaceApp {
    pub(super) fn copy_knowledge_note(&mut self, id: String, cut: bool, cx: &mut Context<Self>) {
        if let Some(editor) = self.knowledge_workspace.read(cx).editor.clone() {
            editor.update(cx, |editor, cx| {
                if editor.document_id == id && editor.is_dirty() {
                    editor.save_current_draft(cx);
                }
            });
        }
        self.knowledge_workspace.update(cx, |state, _| {
            state.clipboard = Some(NoteClipboard {
                document_id: id,
                cut,
            });
        });
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_note_is_cut(&self, id: &str, cx: &App) -> bool {
        self.knowledge_workspace
            .read(cx)
            .clipboard
            .as_ref()
            .is_some_and(|clipboard| clipboard.cut && clipboard.document_id == id)
    }

    pub(super) fn paste_knowledge_note(&mut self, cx: &mut Context<Self>) {
        let state = self.knowledge_workspace.read(cx);
        if state.metadata_task.is_some()
            || state
                .editor
                .as_ref()
                .is_some_and(|editor| editor.read(cx).is_dirty())
        {
            return;
        }
        let Some(clipboard) = state.clipboard.clone() else {
            return;
        };
        let Some(target) = state.navigator_snapshot.selected_collection_id.clone() else {
            return;
        };
        let load_generation = state.load_generation;
        let copy_title = self.i18n.t("settings_view.knowledge.note_copy_title");
        let store = self.ai_entity.read(cx).rag_store();
        let operation = clipboard.clone();
        let target_for_result = target.clone();
        let audit_context = oxideterm_audit::AuditContext::current_request()
            .or_else(oxideterm_audit::AuditContext::current)
            .map(|mut context| {
                if context.source == oxideterm_audit::AuditSource::Application {
                    context.source = oxideterm_audit::AuditSource::User;
                }
                context.target = Some(oxideterm_audit::redact(&clipboard.document_id));
                context
            });
        let audit = oxideterm_audit::AuditOperation::in_context(
            audit_context.as_ref(),
            oxideterm_audit::AuditCategory::File,
            if clipboard.cut {
                "knowledge_document_move"
            } else {
                "knowledge_document_copy"
            },
            Some(&format!("destination_collection={target}")),
        );
        let task = cx.spawn(async move |workspace, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let source =
                        store
                            .get_doc_metadata(&operation.document_id)?
                            .ok_or_else(|| {
                                oxideterm_ai::RagError::DocumentNotFound(
                                    operation.document_id.clone(),
                                )
                            })?;
                    if operation.cut {
                        let changed = source.collection_id != target;
                        let moved = if changed {
                            store.edit_document_metadata(
                                &source.id,
                                None,
                                Some(&target),
                                source.version,
                            )?
                        } else {
                            source
                        };
                        Ok((moved.id, moved.version, changed))
                    } else {
                        oxideterm_ai::rag_copy_document(
                            &store,
                            &source.id,
                            target,
                            copy_title.replace("{{title}}", &source.title),
                        )
                        .map(|doc| (doc.id, doc.version, true))
                    }
                })
                .await;
            audit.finish(
                match &result {
                    Ok((_, _, true)) => oxideterm_audit::AuditOutcome::Succeeded,
                    Ok((_, _, false)) => oxideterm_audit::AuditOutcome::Unchanged,
                    Err(_) => oxideterm_audit::AuditOutcome::Failed,
                },
                oxideterm_audit::AuditEvidence::Protocol,
                None,
                None,
            );
            let _ = workspace.update(cx, |workspace, cx| {
                let selected = workspace
                    .knowledge_workspace
                    .read(cx)
                    .navigator_snapshot
                    .selected_collection_id
                    .clone();
                workspace.knowledge_workspace.update(cx, |state, cx| {
                    state.metadata_task = None;
                    match &result {
                        Ok((id, version, _)) => {
                            if clipboard.cut && state.clipboard.as_ref() == Some(&clipboard) {
                                state.clipboard = None;
                            }
                            if clipboard.cut
                                && let Some(editor) = state.editor.as_ref()
                            {
                                editor.update(cx, |editor, cx| {
                                    if editor.document_id == *id {
                                        editor.collection_id = target_for_result.clone();
                                        editor.version = *version;
                                        cx.notify();
                                    }
                                });
                            }
                        }
                        Err(_) => {
                            state.metadata_error =
                                Some(workspace.i18n.t("settings_view.knowledge.metadata_failed"))
                        }
                    }
                });
                if let Ok((id, _, _)) = result {
                    workspace.refresh_knowledge_navigator(true, cx);
                    let state = workspace.knowledge_workspace.read(cx);
                    if selected.as_ref() == Some(&target_for_result)
                        && state.load_generation == load_generation
                        && !state
                            .editor
                            .as_ref()
                            .is_some_and(|editor| editor.read(cx).is_dirty())
                    {
                        // Reload moved metadata so subsequent saves use the advanced version.
                        workspace.knowledge_workspace.update(cx, |state, _| {
                            state.editor = None;
                            state._editor_subscription = None;
                        });
                        workspace.select_knowledge_document(id, cx);
                    }
                }
                cx.notify();
            });
        });
        self.knowledge_workspace.update(cx, |state, _| {
            state.metadata_error = None;
            state.metadata_task = Some(task);
        });
        cx.notify();
    }
}
