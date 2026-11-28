mod composer;
mod history;
mod layers;
mod provider_stream;
pub(crate) mod services;

use gpui::{prelude::*, *};
use oxideterm_ai::{AiChatRole, AiProviderView, AiStreamEvent, agent_chat_message, provider_view};
use oxideterm_gpui_ai::{
    background::*,
    bridge::{AiHostBridge, HostTask},
    knowledge::page::KnowledgePage,
    model_picker::*,
    sidebar::{editor::*, transcript::*, *},
    state::{AiModelSelectorScope, AiWorkspaceEntity, AiWorkspaceEvent},
    stream::*,
};
use oxideterm_gpui_settings_view::surface::PortableSettingsSurface;
use oxideterm_gpui_ui::text_input::TextInputAnchorId;
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, RequestMode, Response};
use zeroize::Zeroizing;

pub(crate) struct ChatSetupNotice(pub &'static str);
impl EventEmitter<ChatSetupNotice> for Assistant {}

#[derive(Clone, Copy)]
enum Turn {
    Draft,
    Retry,
    Edit,
}

struct ProviderRequest {
    provider_id: String,
    kind: String,
    endpoint: String,
    model: String,
    key: Option<Zeroizing<String>>,
}

fn selected_provider<'a>(
    providers: &'a [AiProviderView],
    id: Option<&str>,
) -> Option<&'a AiProviderView> {
    match id {
        Some(id) => providers.iter().find(|p| p.id == id && p.enabled),
        None => providers.iter().find(|p| p.enabled),
    }
}

pub struct Assistant {
    state: Entity<AiWorkspaceEntity>,
    sidebar: Entity<AiAssistantSidebarView>,
    composer: Entity<composer::Composer>,
    message: Entity<composer::Composer>,
    layers: Entity<layers::ChatLayers>,
    rename: Entity<oxideterm_gpui_editor::line_edit::LineField>,
    history: Rc<history::History>,
    settings: Entity<PortableSettingsSurface>,
    focus: FocusHandle,
    env: Rc<RefCell<(ThemeTokens, I18n, SharedString)>>,
    serial: u64,
    command_target: Rc<std::cell::Cell<bool>>,
    demo: bool,
    on_open_settings: Rc<dyn Fn(&mut Window, &mut App)>,
    on_insert_command: Rc<dyn Fn(String, &mut Window, &mut App)>,
}

impl Assistant {
    pub fn new(
        t: ThemeTokens,
        ui_font: SharedString,
        i: I18n,
        settings: Entity<PortableSettingsSurface>,
        demo: bool,
        on_open_settings: Rc<dyn Fn(&mut Window, &mut App)>,
        on_insert_command: Rc<dyn Fn(String, &mut Window, &mut App)>,
        cx: &mut Context<Self>,
    ) -> Self {
        let env = Rc::new(RefCell::new((t, i.clone(), ui_font)));
        let command_target = Rc::new(std::cell::Cell::new(false));
        let can_insert_command = command_target.clone();
        let local = Rc::new(services::LocalOnly);
        let history = Rc::new(history::History::default());
        let knowledge =
            cx.new(|_| KnowledgePage::with_bridge(Arc::new(services::LocalOnly), local.clone()));
        let state = cx.new(|cx| {
            AiWorkspaceEntity::with_bridge(
                AiHostBridge {
                    provider: local.clone(),
                    acp_metadata: local.clone(),
                    mcp: local.clone(),
                },
                history.clone(),
                knowledge,
                local,
                cx,
            )
        });
        let focus = cx.focus_handle();
        let anchors = layers::Anchors::default();
        let place = layers::anchor_writer(&anchors, &state);
        let header_anchor = place.clone();
        let rename = Self::rename_field(&state, t, cx);
        let send = cx.weak_entity();
        let composer = cx.new(|cx| {
            composer::Composer::new(
                AiEditorKind::Composer,
                state.clone(),
                env.clone(),
                send.clone(),
                cx,
            )
        });
        let message = cx.new(|cx| {
            composer::Composer::new(
                AiEditorKind::MessageEdit,
                state.clone(),
                env.clone(),
                send,
                cx,
            )
        });
        let owner = cx.weak_entity();
        let style = env.clone();
        let header = cx.new(|_| {
            AiChatHeaderView::new(
                state.clone(),
                Rc::new(move |_| {
                    let (t, i, ui_font) = &*style.borrow();
                    Some(AiChatHeaderFrame {
                        tokens: *t,
                        i18n: i.clone(),
                        background: rgb(t.ui.bg_panel),
                        font: ui_font.clone(),
                    })
                }),
                Rc::new(move |r, _, cx| {
                    let _ = owner.update(cx, |a, cx| match r {
                        AiChatHeaderRequest::NewConversation => a.new_chat(cx),
                        AiChatHeaderRequest::OpenPopover(p, open) => a.state.update(cx, |s, cx| {
                            s.set_chat_popover_open(p, open);
                            cx.notify()
                        }),
                    });
                }),
                header_anchor,
            )
        });
        let jobs =
            cx.new(|cx| AiBackgroundTaskEntity::with_bridge(|_| Box::new(services::LocalOnly), cx));
        let style = env.clone();
        let background = cx.new(|cx| {
            AiBackgroundTaskView::new(
                state.clone(),
                jobs,
                Rc::new(move |_| {
                    let (t, i, _) = &*style.borrow();
                    Some(AiBackgroundTaskFrame {
                        tokens: *t,
                        i18n: i.clone(),
                        background: rgb(t.ui.bg_panel),
                    })
                }),
                AiBackgroundTaskFrame {
                    tokens: t,
                    i18n: i.clone(),
                    background: rgb(t.ui.bg_panel),
                },
                cx,
            )
        });

        let style = env.clone();
        let picker_settings = settings.clone();
        let picker_owner = cx.weak_entity();
        let model = AiModelPickerViews::mount(
            &state,
            AiModelPickerBindings {
                frame: Rc::new(move |cx| {
                    let (t, i, _) = &*style.borrow();
                    let ai = &picker_settings.read(cx).settings(cx).ai;
                    let providers: Vec<AiProviderView> =
                        ai.providers.iter().filter_map(provider_view).collect();
                    let selected = selected_provider(&providers, ai.active_provider_id.as_deref());
                    let active_provider = selected.map(|p| p.id.clone());
                    let active_model = selected.and_then(|p| {
                        ai.active_model
                            .clone()
                            .filter(|m| !m.trim().is_empty())
                            .or_else(|| p.models.first().cloned())
                    });
                    let status = providers
                        .iter()
                        .map(|p| {
                            let has_key =
                                !oxideterm_ai::provider_chat_requires_key(&p.provider_type)
                                    || p.provider_type == "lmstudio"
                                    || picker_settings.read(cx).has_provider_api_key(&p.id, cx);
                            (
                                p.id.clone(),
                                AiModelPickerStatus {
                                    has_key,
                                    ..Default::default()
                                },
                            )
                        })
                        .collect();
                    Some(AiModelPickerFrame {
                        tokens: *t,
                        i18n: i.clone(),
                        providers,
                        status,
                        active_provider,
                        active_model,
                        acp_active: false,
                        active_acp_agent_id: None,
                        marked: None,
                        search_anchor: TextInputAnchorId(3),
                        caret_visible: true,
                    })
                }),
                request: Rc::new(move |req, w, cx| {
                    let _ = picker_owner.update(cx, |a, cx| match req {
                        AiModelPickerRequest::Toggle(scope) => {
                            a.state.update(cx, |s, cx| {
                                let open = !s.model_selector_is_open(scope);
                                s.set_model_selector_open(scope, open);
                                cx.notify();
                            });
                            cx.notify();
                        }
                        AiModelPickerRequest::Select(provider_id, model_id) => {
                            a.settings.update(cx, |surface, cx| {
                                surface.edit_preferences(cx, |prefs| {
                                    prefs.ai.active_provider_id = Some(provider_id.clone());
                                    prefs.ai.active_model = Some(model_id.clone());
                                });
                            });
                            a.state.update(cx, |s, cx| {
                                s.set_model_selector_open(AiModelSelectorScope::Sidebar, false);
                                cx.notify();
                            });
                            cx.notify();
                        }
                        AiModelPickerRequest::Settings { close } => {
                            if close {
                                a.state.update(cx, |s, cx| {
                                    s.set_model_selector_open(AiModelSelectorScope::Sidebar, false);
                                    cx.notify();
                                });
                            }
                            (a.on_open_settings)(w, cx);
                        }
                        _ => {}
                    });
                }),
                anchor: place.clone(),
                text_anchor: Rc::new(|_, _, _| {}),
            },
            cx,
        );

        let owner = cx.weak_entity();
        let style = env.clone();
        let composer_settings = settings.clone();
        let composer_state = state.clone();
        let rename_view = rename.clone();
        let initial_history = history.clone();
        let sidebar = cx.new(|cx| {
            AiAssistantSidebarView::with_bindings(
                state.clone(),
                message.clone(),
                AiSidebarChildren {
                    header,
                    background,
                    model_trigger: model.sidebar.clone(),
                    composer: composer.clone().into(),
                },
                PortableTranscriptBindings {
                    initialize: Rc::new(move |s, _, cx| {
                        s.initialize_hosted_history(initial_history.as_ref(), cx)
                    }),
                    image_bytes: Rc::new(|_, _, _| Box::pin(async { None })),
                    frame: Rc::new(move |cx| {
                        let (t, i, ui_font) = &*style.borrow();
                        Some(AiTranscriptFrame {
                            can_insert_command: can_insert_command.get(),
                            tokens: *t,
                            i18n: i.clone(),
                            font: ui_font.clone(),
                            mono_font: "Lilex".into(),
                            panel_background: rgb(t.ui.bg_panel),
                            divider: rgb(t.ui.border),
                            window_background: false,
                            thinking_style: Default::default(),
                            thinking_default_expanded: false,
                            agent_models: vec![],
                            markdown: Default::default(),
                            selectable_text: Rc::new(|v| div().child(v.text).into_any_element()),
                            overlay: AiOverlayFrame {
                                rename: AiRenameFrame {
                                    field: Some(rename_view.clone().into()),
                                    anchor: TextInputAnchorId(4),
                                    viewport: Default::default(),
                                    marked: None,
                                    selected_range: None,
                                    active_offset: None,
                                    caret_visible: true,
                                },
                                reasoning: None,
                                breakdown: None,
                                confirm_focus: None,
                            },
                            composer: AiComposerFrame {
                                enabled: composer_settings.read(cx).settings(cx).ai.enabled,
                                model_selected: true,
                                images_allowed: composer_state.read(cx).chat_ui().images.is_empty(),
                                backend: Default::default(),
                                context_counts: None,
                                terminal_context: false,
                                split_context: false,
                                ide_context: false,
                                sftp_context: false,
                                tools: (false, 0),
                                reasoning: None,
                                auth_methods: None,
                            },
                        })
                    }),
                    request: Rc::new(move |r, w, cx| {
                        let _ = owner.update(cx, |a, cx| match r {
                            AiTranscriptRequest::EditMessage(id) => a
                                .state
                                .update(cx, |s, cx| s.load_message_edit(id, |_| {}, cx)),
                            AiTranscriptRequest::CancelEdit => {
                                a.state.update(cx, |s, cx| s.cancel_message_edit(cx))
                            }
                            AiTranscriptRequest::Regenerate => a.start_turn(Turn::Retry, cx),
                            AiTranscriptRequest::SaveEdit => a.start_turn(Turn::Edit, cx),
                            AiTranscriptRequest::Send => a.send(cx),
                            AiTranscriptRequest::Stop => a.stop(cx),
                            AiTranscriptRequest::NewConversation => a.new_chat(cx),
                            AiTranscriptRequest::ConfirmClearAll(yes)
                            | AiTranscriptRequest::ConfirmDeleteMessage(yes) => {
                                a.confirm_chat(yes, cx)
                            }
                            AiTranscriptRequest::ClosePopovers => a.state.update(cx, |s, cx| {
                                s.close_chat_popovers();
                                s.dismiss_model_selector();
                                cx.notify();
                            }),
                            AiTranscriptRequest::OpenSettings => {
                                if let Some(w) = w {
                                    (a.on_open_settings)(w, cx);
                                }
                            }
                            AiTranscriptRequest::BeginRename(id, title) => {
                                a.state.update(cx, |s, cx| {
                                    s.begin_conversation_rename(id, title.clone());
                                    cx.notify();
                                });
                                a.rename.update(cx, |f, cx| f.reset(&title, true, cx));
                                if let Some(w) = w {
                                    w.focus(&a.rename.focus_handle(cx), cx);
                                }
                            }
                            AiTranscriptRequest::SaveRename => {
                                a.state.update(cx, |s, cx| s.save_conversation_rename(cx));
                            }
                            AiTranscriptRequest::DeleteConversation(id) => {
                                a.state.update(cx, |s, cx| {
                                    s.cancel_chat_stream_for(&id);
                                    s.delete_conversation(&id);
                                    s.persist_chat_state();
                                    cx.notify();
                                });
                            }
                            AiTranscriptRequest::InsertCommand(cmd) => {
                                if let Some(w) = w {
                                    (a.on_insert_command)(cmd, w, cx);
                                }
                            }
                            AiTranscriptRequest::FocusComposer => {
                                if let Some(w) = w {
                                    w.focus(&a.composer.focus_handle(cx), cx)
                                }
                                a.state.update(cx, |s, cx| {
                                    s.focus_chat_input();
                                    cx.notify();
                                });
                            }
                            AiTranscriptRequest::FollowUp(text) => {
                                a.state.update(cx, |s, _| s.set_chat_draft(text));
                                a.send(cx);
                            }
                            _ => {}
                        });
                    }),
                    file_preview: Rc::new(|_, _| None),
                    markdown_actions: Default::default(),
                    select_anchor: place,
                    text_anchor: Rc::new(|_, _, _| {}),
                },
                t,
                i,
                cx,
            )
        });

        cx.subscribe(&state, |_, state, event, cx| {
            if matches!(event, AiWorkspaceEvent::ChatStreamDeliveryReady) {
                state.update(cx, |s, cx| {
                    while let Some((ticket, d)) = s.take_chat_window_request(cx) {
                        if let AiChatWindowDelivery::Host(d) = d
                            && let PortableStreamEvent::Stream(event) = d.event
                        {
                            let finished =
                                matches!(event, AiStreamEvent::Done | AiStreamEvent::Error(_));
                            // Browser failures contain only localized messages and HTTP status, never response bodies.
                            let safe_error = match &event {
                                AiStreamEvent::Error(reason) => Some(reason.clone()),
                                _ => None,
                            };
                            s.apply_stream_event_state(
                                d.generation,
                                &d.conversation_id,
                                &d.assistant_id,
                                event,
                                safe_error,
                            );
                            if finished {
                                s.complete_chat_stream(d.generation);
                                s.set_conversation_loading(&d.conversation_id, false);
                            }
                        }
                        s.acknowledge_chat_window_request(ticket, cx);
                        cx.notify();
                    }
                });
            }
        })
        .detach();

        let layers = cx.new(|cx| {
            layers::ChatLayers::new(
                state.clone(),
                sidebar.clone(),
                model,
                anchors,
                env.clone(),
                cx,
            )
        });
        Self {
            state,
            sidebar,
            composer,
            message,
            layers,
            rename,
            history,
            settings,
            focus,
            env,
            serial: 0,
            command_target,
            demo,
            on_open_settings,
            on_insert_command,
        }
    }

    pub fn panel(&self, width: f32, writable: bool, cx: &mut App) {
        self.composer.update(cx, |c, cx| c.set_width(width, cx));
        self.message.update(cx, |c, cx| c.set_width(width, cx));
        if self.command_target.replace(writable) != writable {
            App::notify(cx, self.sidebar.entity_id());
        }
    }

    pub fn environment(
        &mut self,
        t: ThemeTokens,
        i: I18n,
        ui_font: SharedString,
        cx: &mut Context<Self>,
    ) {
        self.env.replace((t, i, ui_font));
        self.composer.update(cx, |c, cx| c.appearance(cx));
        self.message.update(cx, |c, cx| c.appearance(cx));
        App::notify(cx, self.sidebar.entity_id());
        App::notify(cx, self.layers.entity_id());
        cx.notify();
    }

    fn new_chat(&mut self, cx: &mut Context<Self>) {
        self.serial += 1;
        self.state.update(cx, |s, cx| {
            s.create_conversation(
                format!("chat-{}", self.serial),
                None,
                js_sys::Date::now() as i64,
                None,
            );
            s.reset_chat_for_new_conversation();
            cx.notify();
        });
    }

    fn stop(&self, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.cancel_chat_stream();
            cx.notify();
        });
    }

    fn active_provider_request(&self, cx: &App) -> Result<Option<ProviderRequest>, &'static str> {
        let surface = self.settings.read(cx);
        let ai = &surface.settings(cx).ai;
        if ai.providers.is_empty() && ai.active_provider_id.is_none() {
            return if self.demo {
                Ok(None)
            } else {
                Err("ai.model_selector.no_provider")
            };
        }
        let providers = oxideterm_ai::provider_views(&ai.providers);
        let active = selected_provider(&providers, ai.active_provider_id.as_deref())
            .ok_or("ai.model_selector.no_model_selected")?;
        let model = ai
            .active_model
            .clone()
            .filter(|m| !m.trim().is_empty())
            .or_else(|| active.models.first().cloned())
            .ok_or("ai.model_selector.no_model_selected")?;
        if active.api_protocol == oxideterm_ai::AiApiProtocol::Responses
            || active.provider_type == "gemini"
        {
            return Err("settings_view.ai.browser_protocol");
        }
        let key = surface.provider_api_key(&active.id, cx);
        if key.is_none()
            && active.provider_type != "lmstudio"
            && oxideterm_ai::provider_chat_requires_key(&active.provider_type)
        {
            return Err("ai.model_selector.no_key_warning");
        }
        let url =
            url::Url::parse(&active.base_url).map_err(|_| "settings_view.ai.chat_endpoint")?;
        if !matches!(url.scheme(), "https" | "http") || !url.has_host() {
            return Err("settings_view.ai.chat_endpoint");
        }
        Ok(Some(ProviderRequest {
            provider_id: active.id.clone(),
            kind: active.provider_type.clone(),
            endpoint: active.base_url.clone(),
            model,
            key,
        }))
    }

    fn send(&mut self, cx: &mut Context<Self>) {
        self.start_turn(Turn::Draft, cx);
    }

    fn start_turn(&mut self, turn: Turn, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        if !self.settings.read(cx).settings(cx).ai.enabled
            || !state.history_ready()
            || (matches!(turn, Turn::Draft)
                && (!state.chat_ui().images.is_empty() || state.chat_ui().draft.trim().is_empty()))
            || (!matches!(turn, Turn::Draft) && state.chat_is_loading())
        {
            return;
        }
        let provider_req = match self.active_provider_request(cx) {
            Ok(req) => req,
            Err(key) => {
                cx.emit(ChatSetupNotice(key));
                return;
            }
        };
        let i18n = self.env.borrow().1.clone();
        let now = js_sys::Date::now() as i64;
        let Some((generation, conversation_id, assistant_id, sender, committed)) =
            self.state.update(cx, |s, cx| {
                let id = match turn {
                    Turn::Draft => {
                        self.serial += 1;
                        let id =
                            s.ensure_conversation(format!("chat-{}", self.serial), None, now, None);
                        let user = agent_chat_message(AiChatRole::User, s.chat_ui().draft.clone());
                        s.add_message(&id, user);
                        s.set_chat_draft(String::new());
                        id
                    }
                    Turn::Retry => {
                        let id = s.conversation_state().active_conversation_id.clone()?;
                        s.truncate_conversation_after_last_user(&id, now)?
                    }
                    Turn::Edit => {
                        let (message, content) = s.editable_message_draft()?;
                        let next = s.next_chat_id(now);
                        let (backend_id, model) = provider_req
                            .as_ref()
                            .map(|r| (r.provider_id.clone(), r.model.clone()))
                            .unwrap_or_else(|| ("demo".into(), String::new()));
                        let provenance = oxideterm_ai::AiMessageBackendProvenance {
                            kind: oxideterm_ai::AiMessageBackendKind::Provider,
                            backend_id,
                            model: model.clone(),
                        };
                        let id = s.replace_active_user_message(
                            &message, next, content, model, provenance, now,
                        )?;
                        s.clear_message_edit();
                        id
                    }
                };
                let mut reply = agent_chat_message(AiChatRole::Assistant, String::new());
                reply.model = provider_req.as_ref().map(|p| p.model.clone());
                reply.is_streaming = true;
                let aid = reply.id.clone();
                s.add_message(&id, reply);
                let (gen_id, tx) = s.begin_chat_stream(id.clone(), aid.clone());
                cx.notify();
                Some((gen_id, id, aid, tx, s.committed_history()))
            })
        else {
            return;
        };

        let history = self.history.clone();
        let task = cx.spawn(async move |_, cx| {
            let emit = |event| {
                sender.send(PortableStreamDelivery {
                    generation,
                    conversation_id: conversation_id.clone(),
                    assistant_id: assistant_id.clone(),
                    event: PortableStreamEvent::Stream(event),
                })
            };
            // The visible transcript may contain only previews or an older page.
            let messages = if committed.await == Ok(true) {
                history
                    .request_messages(&conversation_id, &assistant_id)
                    .ok()
            } else {
                None
            };
            let Some(messages) = messages else {
                let _ = emit(AiStreamEvent::Error(i18n.t("ai.chat.load_failed_generic")));
                return;
            };
            if let Some(ProviderRequest {
                kind: provider_type,
                endpoint: base_url,
                model,
                key: api_key,
                ..
            }) = provider_req
            {
                let result = fetch_llm_completion(
                    &provider_type,
                    &base_url,
                    &model,
                    api_key,
                    &i18n,
                    &messages,
                    |delta| {
                        let _ = emit(AiStreamEvent::Content(delta));
                    },
                )
                .await;
                let _ = emit(match result {
                    Ok(()) => AiStreamEvent::Done,
                    Err(reason) => AiStreamEvent::Error(reason),
                });
            } else if let Some((_, prompt)) = messages.iter().rev().find(|(role, _)| role == "user")
            {
                let text = demo_assistant_reply(prompt, &i18n);
                for chunk in text.split_inclusive('\n') {
                    cx.background_executor()
                        .timer(Duration::from_millis(45))
                        .await;
                    if emit(AiStreamEvent::Content(chunk.into())).is_err() {
                        return;
                    }
                }
                let _ = emit(AiStreamEvent::Done);
            } else {
                let _ = emit(AiStreamEvent::Error(i18n.t("ai.chat.load_failed_generic")));
            }
        });
        self.state.update(cx, |s, _| {
            s.set_host_stream_task(generation, HostTask::new(move || drop(task)))
        });
    }

    pub fn composer_focused(&self, w: &Window, cx: &App) -> bool {
        self.composer.focus_handle(cx).is_focused(w) || self.message.focus_handle(cx).is_focused(w)
    }
}

async fn fetch_llm_completion(
    provider_type: &str,
    base_url: &str,
    model: &str,
    api_key: Option<Zeroizing<String>>,
    i18n: &I18n,
    messages: &[(String, String)],
    mut emit: impl FnMut(String),
) -> Result<(), String> {
    let window = web_sys::window().ok_or_else(|| i18n.t("settings_view.ai.chat_network"))?;
    let trimmed_base = base_url.trim().trim_end_matches('/');
    let is_anthropic = provider_type == "anthropic";
    let url = if is_anthropic {
        if trimmed_base.ends_with("/messages") {
            trimmed_base.to_owned()
        } else if trimmed_base.ends_with("/v1") {
            format!("{trimmed_base}/messages")
        } else {
            format!("{trimmed_base}/v1/messages")
        }
    } else if trimmed_base.ends_with("/chat/completions") {
        trimmed_base.to_string()
    } else if provider_type == "ollama" && !trimmed_base.ends_with("/v1") {
        format!("{trimmed_base}/v1/chat/completions")
    } else {
        format!("{trimmed_base}/chat/completions")
    };

    let body = if is_anthropic {
        let msgs: Vec<serde_json::Value> = messages
            .iter()
            .filter(|(role, _)| role != "system")
            .map(|(role, content)| serde_json::json!({ "role": role, "content": content }))
            .collect();
        serde_json::json!({
            "model": model,
            "max_tokens": 2048,
            "stream": true,
            "messages": msgs,
        })
    } else {
        let msgs: Vec<serde_json::Value> = messages
            .iter()
            .map(|(role, content)| serde_json::json!({ "role": role, "content": content }))
            .collect();
        serde_json::json!({
            "model": model,
            "messages": msgs,
            "stream": true,
        })
    };

    let headers = Headers::new().map_err(|_| i18n.t("settings_view.ai.chat_endpoint"))?;
    headers
        .set("Content-Type", "application/json")
        .map_err(|_| i18n.t("settings_view.ai.chat_endpoint"))?;
    if let Some(key) = api_key {
        let auth_value = if is_anthropic {
            Zeroizing::new(key.trim().to_string())
        } else {
            Zeroizing::new(format!("Bearer {}", key.trim()))
        };
        let header_name = if is_anthropic {
            "x-api-key"
        } else {
            "Authorization"
        };
        headers
            .set(header_name, &auth_value)
            .map_err(|_| i18n.t("settings_view.ai.chat_endpoint"))?;
        if is_anthropic {
            headers
                .set("anthropic-version", "2023-06-01")
                .and_then(|_| headers.set("anthropic-dangerous-direct-browser-access", "true"))
                .map_err(|_| i18n.t("settings_view.ai.chat_endpoint"))?;
        }
    }

    // The conversation owns this future: stopping it must cancel fetch and its body reader.
    struct FetchGuard(web_sys::AbortController);
    impl Drop for FetchGuard {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let guard = FetchGuard(
        web_sys::AbortController::new().map_err(|_| i18n.t("settings_view.ai.chat_network"))?,
    );
    let opts = RequestInit::new();
    opts.set_signal(Some(&guard.0.signal()));
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    opts.set_headers(&headers);
    let body_str = Zeroizing::new(body.to_string());
    opts.set_body(&wasm_bindgen::JsValue::from_str(&body_str));

    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|_| i18n.t("settings_view.ai.chat_endpoint"))?;
    let resp_val = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| i18n.t("settings_view.ai.browser_network"))?;
    let resp: Response = resp_val
        .dyn_into()
        .map_err(|_| i18n.t("settings_view.ai.chat_invalid_stream"))?;
    if !resp.ok() {
        return Err(format!(
            "{} (HTTP {})",
            i18n.t("settings_view.ai.chat_rejected"),
            resp.status()
        ));
    }
    let content_type = resp
        .headers()
        .get("content-type")
        .ok()
        .flatten()
        .unwrap_or_default();
    if !content_type
        .split(';')
        .next()
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
    {
        use futures::StreamExt;
        let body = resp
            .body()
            .ok_or_else(|| i18n.t("settings_view.ai.stream_interrupted"))?;
        let chunks = wasm_streams::ReadableStream::from_raw(body)
            .into_stream()
            .map(|part| part.map(|js| js_sys::Uint8Array::new(&js).to_vec()));
        return provider_stream::read_text_events(chunks, is_anthropic, emit)
            .await
            .map_err(|key| i18n.t(key));
    }
    let text_promise = resp
        .text()
        .map_err(|_| i18n.t("settings_view.ai.stream_interrupted"))?;
    let text = JsFuture::from(text_promise)
        .await
        .map_err(|_| i18n.t("settings_view.ai.stream_interrupted"))?
        .as_string()
        .ok_or_else(|| i18n.t("settings_view.ai.chat_invalid_stream"))?;

    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| i18n.t("settings_view.ai.chat_invalid_stream"))?;
    let content = if is_anthropic {
        json.get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| {
                let parts: Vec<&str> = arr
                    .iter()
                    .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                    .collect();
                (!parts.is_empty()).then(|| parts.join(""))
            })
            .ok_or_else(|| i18n.t("settings_view.ai.chat_invalid_stream"))
    } else {
        json.get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|t| t.as_str())
            .map(str::to_owned)
            .ok_or_else(|| i18n.t("settings_view.ai.chat_invalid_stream"))
    }?;
    emit(content);
    Ok(())
}

fn demo_assistant_reply(prompt: &str, i18n: &I18n) -> String {
    let lower = prompt.to_lowercase();
    let example = if ["port", "端口", "listen"].iter().any(|s| lower.contains(s)) {
        "ss -tulnp"
    } else if ["docker", "容器", "container"]
        .iter()
        .any(|s| lower.contains(s))
    {
        "docker ps"
    } else if ["disk", "磁盘", "df", "空间"]
        .iter()
        .any(|s| lower.contains(s))
    {
        "df -h"
    } else {
        "pwd"
    };
    format!(
        "{}\n\n```sh\n{example}\n```",
        i18n.t("settings_view.ai.browser_demo_reply")
    )
}

impl Render for Assistant {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .track_focus(&self.focus)
            .child(self.sidebar.clone())
    }
}
