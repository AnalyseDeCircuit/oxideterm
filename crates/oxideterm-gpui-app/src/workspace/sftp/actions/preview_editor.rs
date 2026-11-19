use super::external::open_path_in_external_app;
use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_sftp_editor_search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(editor) = self.sftp_view().read(cx).preview_editor.clone() else {
            return false;
        };
        if event.keystroke.key == "escape" && editor.read(cx).search_is_open() {
            return editor.update(cx, |editor, cx| editor.close_search(window, cx));
        }
        if crate::keybindings::keystroke_matches_action(
            &event.keystroke,
            "editor.find",
            &self.settings_store.settings().keybindings.overrides,
        ) {
            return editor.update(cx, |editor, cx| editor.open_search(cx));
        }
        false
    }

    pub(in crate::workspace) fn open_or_preview_sftp_file(
        &mut self,
        pane: SftpPane,
        file: &SftpFileEntry,
        cx: &mut Context<Self>,
    ) {
        if file.file_type == SftpFileType::Directory {
            let base = match pane {
                SftpPane::Local => self.sftp_view().read(cx).local_path.clone(),
                SftpPane::Remote => self.sftp_view().read(cx).remote_path.clone(),
            };
            self.set_sftp_path(pane, join_sftp_path(&base, &file.name), cx);
        } else if pane == SftpPane::Remote {
            // The backend may recognize an executable by its header even without an extension.
            let plugin_preview = {
                let plugins = self.plugin_entity.clone();
                let runtime = self.forwarding_runtime.clone();
                let tokens = self.tokens;
                let i18n = self.i18n.clone();
                Some(cx.new(|cx| {
                    super::super::super::plugin_preview::PluginFilePreview::new(
                        plugins, runtime, tokens, i18n, cx,
                    )
                }))
            };
            let plugin_preview = plugin_preview.map(|view| {
                SftpDocumentPreview::new(
                    view,
                    crate::workspace::plugin_preview::PluginFilePreview::set_source,
                )
            });
            let generation = self.sftp_view().update(cx, |sftp, cx| {
                sftp.begin_remote_preview(pane, file, plugin_preview, cx)
            });
            self.spawn_remote_sftp_preview(file.path.clone(), generation, cx);
        }
    }

    pub(in crate::workspace::sftp) fn download_sftp_preview(
        &self,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            return;
        };
        let protocol = self.transfer_protocol_for_remote(&remote_id);
        if let Some(launch) = self.sftp_view().update(cx, |state, cx| {
            state.prepare_preview_download(remote_id, name, protocol, cx)
        }) {
            self.spawn_sftp_transfer_launch(launch, cx);
        }
    }

    pub(in crate::workspace::sftp) fn open_sftp_preview_external(
        &mut self,
        path: &str,
        cx: &mut Context<Self>,
    ) {
        let audit = oxideterm_audit::AuditContext::current().map(|mut context| {
            context.source = oxideterm_audit::AuditSource::User;
            context.target = Some(oxideterm_audit::redact(path));
            context.operation(
                oxideterm_audit::AuditCategory::File,
                "file_external_open",
                Some("local_application_launch"),
            )
        });
        let result = open_path_in_external_app(path);
        if let Some(audit) = audit {
            audit.finish(
                if result.is_ok() {
                    oxideterm_audit::AuditOutcome::Sent
                } else {
                    oxideterm_audit::AuditOutcome::Failed
                },
                oxideterm_audit::AuditEvidence::Dispatch,
                None,
                None,
            );
        }
        if let Err(error) = result {
            let error = format!(
                "{}: {}",
                self.i18n.t("sftp.toast.open_external_failed"),
                error
            );
            self.sftp_view().update(cx, |sftp, cx| {
                sftp.preview_error = Some(error);
                cx.notify();
            });
        }
    }

    fn spawn_remote_sftp_preview(&self, path: String, generation: u64, cx: &mut Context<Self>) {
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            return;
        };
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            return;
        };
        let tx = self.sftp_view().read(cx).worker_sender();
        let runtime = self.forwarding_runtime.clone();
        let audit_request = oxideterm_audit::AuditContext::current_request().or_else(|| {
            oxideterm_audit::AuditContext::current().map(|mut context| {
                context.source = oxideterm_audit::AuditSource::User;
                context
            })
        });
        let task = runtime.spawn(async move {
            let task = load_remote_sftp_preview(backend, &path);
            let result = if let Some(context) = audit_request {
                context.scope(task).await
            } else {
                task.await
            };
            let _ = tx.send(SftpWorkerResult::PreviewLoaded {
                asset_owner: result
                    .as_ref()
                    .ok()
                    .and_then(PreviewAssetOwner::from_asset_content_owned_temp),
                generation,
                path,
                result,
            });
        });
        self.sftp_view().update(cx, |sftp, _| {
            sftp.preview_load_task = Some(task.abort_handle())
        });
    }

    pub(in crate::workspace::sftp) fn spawn_remote_sftp_preview_hex(
        &self,
        path: String,
        offset: u64,
        generation: u64,
        cx: &App,
    ) {
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            return;
        };
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            return;
        };
        let tx = self.sftp_view().read(cx).worker_sender();
        let error_prefix = self.i18n.t("sftp.toast.load_more_failed");
        let runtime = self.forwarding_runtime.clone();
        let audit_request = oxideterm_audit::AuditContext::current_request().or_else(|| {
            oxideterm_audit::AuditContext::current().map(|mut context| {
                context.source = oxideterm_audit::AuditSource::User;
                context
            })
        });
        runtime.spawn(async move {
            let task = load_remote_sftp_preview_hex(backend, &path, offset);
            let result = if let Some(context) = audit_request {
                context.scope(task).await
            } else {
                task.await
            };
            let _ = tx.send(SftpWorkerResult::PreviewHexLoaded {
                generation,
                path,
                error_prefix,
                result,
            });
        });
    }

    pub(in crate::workspace) fn spawn_remote_sftp_preview_save(
        &self,
        expected_version: Option<String>,
        path: String,
        content: Arc<str>,
        encoding: Arc<str>,
        line_ending: TextLineEnding,
        generation: u64,
        tx: delivery::ActiveDeliverySender<SftpWorkerResult>,
        cx: &App,
    ) -> bool {
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            return false;
        };
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            return false;
        };
        let network_error_message = self.i18n.t("sftp.preview.network_error");
        let conflict_message = self
            .i18n
            .t("ide.conflict_desc")
            .replace("{{fileName}}", &oxideterm_sftp::display_remote_path(&path));
        let runtime = self.forwarding_runtime.clone();
        let audit_request = oxideterm_audit::AuditContext::current_request().or_else(|| {
            oxideterm_audit::AuditContext::current().map(|mut context| {
                context.source = oxideterm_audit::AuditSource::User;
                context
            })
        });
        runtime.spawn(async move {
            let task = save_remote_sftp_preview(
                backend,
                &path,
                expected_version.as_deref(),
                content.as_ref(),
                encoding.as_ref(),
                line_ending,
            );
            let result = if let Some(context) = audit_request {
                context.scope(task).await
            } else {
                task.await
            }
            .map_err(|error| match error {
                oxideterm_sftp::SftpError::Conflict(_) => conflict_message,
                error => error.to_string(),
            });
            let _ = tx.send(SftpWorkerResult::PreviewSaved {
                generation,
                path,
                content,
                network_error_message,
                result,
            });
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use oxideterm_gpui_editor::TextEditorView;

    #[gpui::test]
    fn only_visible_text_previews_receive_editor_keys(cx: &mut TestAppContext) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        let editor =
            cx.new(|cx| TextEditorView::new("# title", &oxideterm_theme::default_tokens(), cx));
        entity.update(cx, |sftp, _| {
            sftp.preview_editor = Some(editor);
            sftp.preview_content = Some(Arc::new(PreviewContent::Text {
                version: None,
                data: "# title".into(),
                mime_type: Some("text/markdown".into()),
                language: Some("markdown".into()),
                encoding: "utf-8".into(),
                confidence: 1.0,
                has_bom: false,
            }));
            sftp.set_dialog(SftpDialog::Preview {
                name: "readme.md".into(),
            });
            assert!(!sftp.preview_text_editor_visible());
            sftp.preview_markdown_source_mode = true;
            assert!(sftp.preview_text_editor_visible());
            sftp.set_dialog(SftpDialog::Editor {
                name: "readme.md".into(),
            });
            assert!(sftp.preview_text_editor_visible());
            sftp.set_dialog(SftpDialog::EditorCloseConfirm {
                name: "readme.md".into(),
            });
            assert!(!sftp.preview_text_editor_visible());
        });
    }

    #[gpui::test]
    fn preview_save_shares_the_captured_version_across_later_edits(cx: &mut TestAppContext) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        let original: Arc<str> = "saved\n中文".into();
        let editor = cx.new(|cx| {
            TextEditorView::new(original.clone(), &oxideterm_theme::default_tokens(), cx)
        });
        entity.update(cx, |sftp, _| {
            sftp.preview_editor_initial_content = original.clone();
            sftp.preview_editor_observed_content = original.clone();
            sftp.preview_path = Some("/tmp/shared.txt".into());
            sftp.preview_content = Some(Arc::new(PreviewContent::Text {
                data: original.to_string(),
                version: Some("sftp-sha256:opened".into()),
                mime_type: None,
                language: None,
                encoding: "UTF-8".into(),
                confidence: 1.0,
                has_bom: false,
            }));
        });
        let captured = Arc::new(std::sync::Mutex::new(None));
        let observed = captured.clone();
        let _subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |_, _, event: &SftpWorkspaceEvent, _| {
                if let SftpWorkspaceEvent::PreviewSaveRequested {
                    path,
                    content,
                    expected_version,
                    ..
                } = event
                {
                    assert_eq!(path, "/tmp/shared.txt");
                    assert_eq!(expected_version.as_deref(), Some("sftp-sha256:opened"));
                    *observed.lock().unwrap() = Some(content.clone());
                }
            })
        });
        editor.update(cx, |editor, cx| editor.insert_text("new\n", cx));
        let version = cx.read(|cx| editor.read(cx).buffer().text_snapshot());
        entity.update(cx, |sftp, cx| {
            sftp.sync_preview_editor_state(&editor, cx);
            assert!(Arc::ptr_eq(&version, &sftp.preview_editor_observed_content));
            sftp.save_preview_editor_content(version.clone(), cx);
        });
        let saved = captured
            .lock()
            .unwrap()
            .take()
            .expect("save request delivered");
        assert!(Arc::ptr_eq(&saved, &version));
        editor.update(cx, |editor, cx| editor.insert_text("later", cx));
        entity.update(cx, |sftp, cx| sftp.sync_preview_editor_state(&editor, cx));
        assert_eq!(saved.as_ref(), "new\nsaved\n中文");
        cx.read(|cx| {
            assert_eq!(
                entity.read(cx).preview_editor_observed_content.as_ref(),
                "new\nlatersaved\n中文"
            );
            assert_eq!(
                entity.read(cx).preview_editor_initial_content.as_ref(),
                "saved\n中文"
            );
        });
    }
}
