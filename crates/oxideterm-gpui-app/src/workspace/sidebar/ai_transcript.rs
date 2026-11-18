use std::rc::Rc;

use gpui::{App, AppContext, Context, Entity, IntoElement};
use oxideterm_ai::{active_model_selection, active_provider_view, model_reasoning_capability};
use oxideterm_gpui_ai::{
    sidebar::{
        editor::AiEditorView,
        transcript::{
            AiAssistantSidebarView, AiComposerFrame, AiOverlayFrame, AiRenameFrame,
            AiSidebarChildren, AiSidebarLayer, AiSidebarLayerKind, AiTranscriptBindings,
            AiTranscriptFrame, AiTranscriptRequest, SelectableTextRole as TranscriptTextRole,
        },
    },
    state::AiWorkspaceEntity,
};
use oxideterm_gpui_markdown::render::MarkdownCodeBlockActions;
use oxideterm_gpui_ui::tauri_ui_font_family;
use oxideterm_settings::AiActiveBackend;

use super::super::{
    SelectableTextRole, TerminalNoticeVariant, WorkspaceApp, settings_mono_font_family,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_sidebar_layer(
        &self,
        layer: AiSidebarLayerKind,
    ) -> gpui::AnyElement {
        AiSidebarLayer::new(&self.ai.transcript, layer).into_any_element()
    }

    pub(in crate::workspace) fn mount_ai_transcript(
        state: &Entity<AiWorkspaceEntity>,
        message_editor: Entity<AiEditorView>,
        views: AiSidebarChildren,
        tokens: oxideterm_theme::ThemeTokens,
        i18n: oxideterm_i18n::I18n,
        runtime: std::sync::Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) -> Entity<AiAssistantSidebarView> {
        let owner = cx.weak_entity();
        let action_owner = owner.clone();
        let acp_owner = owner.clone();
        let text_owner = owner.clone();
        let rename_target = super::super::ime::WorkspaceImeTarget::AiConversationRename;
        let anchor_owner = owner.clone();
        let mermaid_owner = owner.clone();
        let bindings = AiTranscriptBindings {
            history_path: super::super::default_ai_conversations_path(),
            runtime,
            frame: Rc::new(move |cx: &App| {
                let entity = owner.upgrade()?;
                let app = entity.read(cx);
                let settings = app.settings_store.settings();
                let text = app.selectable_text_render_state_for_entity(entity.clone(), cx);
                let agent_models = oxideterm_ai::provider_views(&settings.ai.providers)
                    .into_iter()
                    .filter(|provider| provider.enabled)
                    .flat_map(|provider| {
                        provider.models.into_iter().map(move |model| {
                            oxideterm_ai::agent::AgentModel {
                                provider_id: provider.id.clone(),
                                model,
                            }
                        })
                    })
                    .collect();
                let ai = &settings.ai;
                let model_selected = if ai.active_backend == AiActiveBackend::Acp {
                    ai.active_acp_agent_id.is_some()
                } else {
                    ai.active_provider_id.is_some()
                        && active_model_selection(ai.active_model.as_deref()).is_some()
                };
                let images_allowed = app.ai.state.read(cx).chat_ui().images.is_empty()
                    || app.resolve_ai_stream_config(cx).is_ok_and(|config| {
                        config.execution_backend == oxideterm_ai::AiExecutionBackend::Acp
                            || oxideterm_ai::model_image_input(
                                &config.provider_type,
                                &config.model,
                                config.image_input,
                            ) != Some(false)
                    });
                let reasoning = if ai.active_backend == AiActiveBackend::Acp {
                    None
                } else {
                    let providers = oxideterm_ai::provider_views(&ai.providers);
                    active_provider_view(&providers, ai.active_provider_id.as_deref()).and_then(
                        |provider| {
                            let model = active_model_selection(ai.active_model.as_deref())?;
                            (!model_reasoning_capability(&provider.provider_type, &model)
                                .levels
                                .is_empty())
                            .then(|| {
                                (
                                    provider.clone(),
                                    model.clone(),
                                    app.active_ai_reasoning_level(provider, &model, cx),
                                )
                            })
                        },
                    )
                };
                let composer = AiComposerFrame {
                    enabled: ai.enabled,
                    model_selected,
                    images_allowed,
                    backend: ai.active_backend,
                    context_counts: ai
                        .enabled
                        .then(|| app.ai_context_message_usage_counts(cx))
                        .flatten(),
                    terminal_context: app.ai_active_terminal_context_available(cx),
                    split_context: app.ai_active_tab_has_split_panes(cx),
                    ide_context: app.ai_has_ide_context(cx),
                    sftp_context: app.ai_has_sftp_context(cx),
                    tools: app.ai_tool_indicator_info(cx),
                    reasoning: reasoning.as_ref().map(|(_, _, level)| *level),
                    auth_methods: app
                        .ai
                        .state
                        .read(cx)
                        .conversation_state()
                        .active_conversation_id
                        .as_deref()
                        .and_then(|id| app.ai.acp.read(cx).authentication_methods(id)),
                };
                Some(AiTranscriptFrame {
                    composer,
                    overlay: AiOverlayFrame {
                        rename: AiRenameFrame {
                            anchor: rename_target.anchor_id(),
                            viewport: app.text_input_anchors.viewport(rename_target.anchor_id()),
                            marked: app
                                .marked_text_for_target(rename_target, cx)
                                .map(str::to_owned),
                            selected_range: app.ime_selected_range_for_target(rename_target, cx),
                            active_offset: app.ime_active_offset_for_target(rename_target, cx),
                            caret_visible: app.input_caret.visible(),
                        },
                        reasoning,
                        breakdown: app
                            .ai
                            .state
                            .read(cx)
                            .chat_ui()
                            .context_popover_open
                            .then(|| app.ai_context_token_breakdown(cx)),
                        confirm_focus: app.standard_confirm_focus(),
                    },

                    tokens: app.tokens,
                    i18n: app.i18n.clone(),
                    font: tauri_ui_font_family(&settings.appearance.ui_font_family),
                    mono_font: settings_mono_font_family(settings),
                    panel_background: app
                        .context_sidebar_content_background(app.tokens.ui.shell_bg()),
                    divider: app.workspace_chrome_divider(),
                    window_background: app.window_background_preferences().is_some(),
                    thinking_style: settings.ai.thinking_style,
                    thinking_default_expanded: settings.ai.thinking_default_expanded,
                    agent_models,
                    markdown: app.localized_markdown_options(),
                    selectable_text: Rc::new(move |fragment| {
                        let role = match fragment.role {
                            TranscriptTextRole::PlainDocument => SelectableTextRole::PlainDocument,
                            TranscriptTextRole::RowSafe => SelectableTextRole::RowSafe,
                            TranscriptTextRole::NonSelectable => SelectableTextRole::NonSelectable,
                        };
                        text.render_styled_text_in_group(
                            role,
                            fragment.group,
                            fragment.id,
                            fragment.order,
                            fragment.text,
                            fragment.runs,
                            fragment.links,
                            fragment.join_previous,
                        )
                    }),
                })
            }),
            request: Rc::new(move |request, window, cx| {
                let _ = action_owner.update(cx, |app, cx| match request {
                    AiTranscriptRequest::ClipboardReadFailed => app.push_ai_settings_toast(
                        app.i18n.t("ai.chat.load_failed_generic"),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                    AiTranscriptRequest::Regenerate => app.regenerate_ai_last_response(cx),
                    AiTranscriptRequest::CancelEdit => app.cancel_edit_ai_message(cx),
                    AiTranscriptRequest::SaveEdit => app.save_ai_message_edit(cx),
                    AiTranscriptRequest::FollowUp(a0) => app.send_ai_follow_up_suggestion(a0, cx),
                    AiTranscriptRequest::ApproveTool(a0, a1, a2) => {
                        app.resolve_ai_tool_approval(a0, a1, a2, cx)
                    }
                    AiTranscriptRequest::AcpPermission(a0, a1, a2) => {
                        app.resolve_ai_acp_permission(a0, a1, a2, cx)
                    }
                    AiTranscriptRequest::SelectToolCandidate(a0, a1, a2) => {
                        app.resolve_ai_tool_candidate_selection(a0, a1, a2, cx)
                    }
                    AiTranscriptRequest::CursorResponse(a0, a1, a2) => {
                        app.resolve_ai_cursor_request(a0, &a1, a2, cx)
                    }
                    AiTranscriptRequest::InsertCommand(a0) => {
                        app.insert_ai_code_block_command(a0, cx)
                    }
                    AiTranscriptRequest::ImagePreviewFailed => app.push_ai_settings_toast(
                        app.i18n.t("ai.images.missing"),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                    AiTranscriptRequest::RetryHistory => app.retry_ai_chat_initialization(cx),
                    AiTranscriptRequest::BeginRename(id, title) => {
                        if let Some(window) = window {
                            app.begin_ai_conversation_rename(id, title, window, cx);
                        }
                    }
                    AiTranscriptRequest::RenamePointer(event) => {
                        if let Some(window) = window {
                            app.ime_marked_text = None;
                            window.focus(&app.focus_handle, cx);
                            app.begin_ime_selection_from_mouse_down(
                                rename_target,
                                &event,
                                window,
                                cx,
                            );
                        }
                    }
                    AiTranscriptRequest::RenameDrag(event) => {
                        if let Some(window) = window {
                            app.update_ime_selection_drag_from_mouse_move(&event, window, cx);
                        }
                    }
                    AiTranscriptRequest::SaveRename => app.save_ai_conversation_rename(cx),
                    AiTranscriptRequest::DeleteConversation(id) => {
                        app.delete_ai_conversation(&id, cx)
                    }
                    AiTranscriptRequest::SafetyDefault => app.set_ai_safety_mode_default(cx),
                    AiTranscriptRequest::SafetyReadOnly => app.set_ai_safety_mode_read_only(cx),
                    AiTranscriptRequest::SafetyConfirm => app.open_ai_safety_confirm(cx),
                    AiTranscriptRequest::ConfirmSafety(confirmed) => {
                        if app.begin_ai_safety_confirm_exit(cx) && confirmed {
                            app.confirm_ai_safety_bypass(cx);
                        }
                    }
                    AiTranscriptRequest::ConfirmSummary(confirmed) => {
                        if app.begin_ai_summarize_confirm_exit(cx) && confirmed {
                            app.start_ai_summarize_conversation(cx);
                        }
                    }
                    AiTranscriptRequest::ConfirmClearAll(confirmed) => {
                        app.begin_ai_clear_all_confirm_exit(confirmed, cx);
                    }
                    AiTranscriptRequest::ConfirmDeleteMessage(confirmed) => {
                        app.begin_ai_delete_message_confirm_exit(confirmed, cx);
                    }
                    AiTranscriptRequest::Autocomplete(candidate) => {
                        app.apply_ai_chat_autocomplete_candidate(&candidate, cx)
                    }
                    AiTranscriptRequest::SelectReasoning(provider, kind, model, level) => {
                        app.select_ai_reasoning_level(provider, kind, model, level, cx);
                    }
                    AiTranscriptRequest::OpenSettings => {
                        if let Some(window) = window {
                            app.open_ai_settings(window, cx);
                        }
                    }
                    AiTranscriptRequest::ClosePopovers => {
                        app.close_ai_sidebar_popovers(cx);
                        cx.notify();
                    }
                    AiTranscriptRequest::NewConversation => {
                        app.create_ai_sidebar_conversation(None, cx);
                    }
                    AiTranscriptRequest::Stop => app.cancel_ai_chat_stream(cx),
                    AiTranscriptRequest::Steer => app.steer_ai_chat_draft(cx),
                    AiTranscriptRequest::InterruptAndSend => {
                        app.interrupt_and_send_ai_chat_draft(cx)
                    }
                    AiTranscriptRequest::Send => app.send_ai_chat_draft(cx),
                    AiTranscriptRequest::Compact => app.start_ai_compact_conversation(cx),
                    AiTranscriptRequest::Summarize => app.open_ai_summarize_confirm(cx),
                    AiTranscriptRequest::SelectImages => app.select_ai_images(cx),
                    AiTranscriptRequest::ContinueTextOnly(id) => {
                        app.continue_ai_image_text_only(&id, cx)
                    }
                    AiTranscriptRequest::ImportImages(paths) => {
                        app.import_ai_image_paths(paths, cx)
                    }
                    AiTranscriptRequest::Authenticate(id, method) => {
                        app.ai.acp.update(cx, |acp, _| {
                            acp.authenticate(&id, method);
                        });
                    }
                    AiTranscriptRequest::QueueAction(conversation, id, action) => {
                        app.apply_ai_queue_action(&conversation, &id, action, cx)
                    }
                    AiTranscriptRequest::AgentModelLocked => app.push_ai_settings_toast(
                        app.i18n.t("ai.agents.model_locked"),
                        TerminalNoticeVariant::Warning,
                        cx,
                    ),
                    AiTranscriptRequest::EditMessage(id) => {
                        app.start_edit_ai_message(id, cx);
                        if let Some(window) = window {
                            window.focus(&app.focus_handle, cx);
                        }
                    }
                    AiTranscriptRequest::FocusComposer => {
                        app.ime_marked_text = None;
                        if let Some(window) = window {
                            window.focus(&app.focus_handle, cx);
                        }
                        gpui::App::notify(cx, app.ai.transcript.entity_id());
                    }
                });
            }),
            file_preview: Rc::new(move |id, cx| {
                let owner = acp_owner.upgrade()?;
                owner.read(cx).ai.acp.read(cx).file_write_preview(id)
            }),
            text_anchor: Rc::new(move |anchor, window, cx| {
                let owner = text_owner.clone();
                window.defer(cx, move |_, cx| {
                    let _ = owner.update(cx, |app, cx| app.update_text_input_anchor(anchor, cx));
                });
            }),
            select_anchor: Rc::new(move |anchor, window, cx| {
                let owner = anchor_owner.clone();
                window.defer(cx, move |_, cx| {
                    let _ = owner.update(cx, |app, cx| app.update_select_anchor(anchor, cx));
                });
            }),
            markdown_actions: MarkdownCodeBlockActions {
                on_run: None,
                on_mermaid_zoom: Some(std::sync::Arc::new(
                    move |source, image, width, height, window, cx| {
                        if let Some(owner) = mermaid_owner.upgrade() {
                            Self::mermaid_zoom_handler_for_workspace(owner)(
                                source, image, width, height, window, cx,
                            );
                        }
                    },
                )),
            },
        };
        cx.new(|cx| {
            AiAssistantSidebarView::new(
                state.clone(),
                message_editor,
                views,
                bindings,
                tokens,
                i18n,
                cx,
            )
        })
    }
}
