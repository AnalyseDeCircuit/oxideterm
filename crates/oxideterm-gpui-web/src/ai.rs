mod history;
mod services;

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

pub struct Assistant {
    state: Entity<AiWorkspaceEntity>,
    sidebar: Entity<AiAssistantSidebarView>,
    model_menu: Entity<AiModelPickerView>,
    settings: Entity<PortableSettingsSurface>,
    focus: FocusHandle,
    env: Rc<RefCell<(ThemeTokens, I18n)>>,
    serial: u64,
    on_open_settings: Rc<dyn Fn(&mut Window, &mut App)>,
    on_insert_command: Rc<dyn Fn(String, &mut Window, &mut App)>,
}

impl Assistant {
    pub fn new(
        t: ThemeTokens,
        i: I18n,
        settings: Entity<PortableSettingsSurface>,
        on_open_settings: Rc<dyn Fn(&mut Window, &mut App)>,
        on_insert_command: Rc<dyn Fn(String, &mut Window, &mut App)>,
        cx: &mut Context<Self>,
    ) -> Self {
        let env = Rc::new(RefCell::new((t, i.clone())));
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
        let f = focus.clone();
        let style = env.clone();
        let editor_state = state.clone();
        let bindings = AiEditorBindings {
            frame: Rc::new(move |kind, _| {
                let (t, i) = &*style.borrow();
                Some(AiEditorFrame {
                    tokens: *t,
                    i18n: i.clone(),
                    font: "IBM Plex Sans".into(),
                    width: 320.,
                    anchor: TextInputAnchorId(if kind == AiEditorKind::Composer { 1 } else { 2 }),
                    model_selected: true,
                    marked_range: None,
                    selected_range: None,
                    active_offset: None,
                    composed_text: None,
                    caret_visible: true,
                })
            }),
            anchor: Rc::new(|_, _, _| {}),
            input: Rc::new(move |kind, _, w, cx| {
                w.focus(&f, cx);
                editor_state.update(cx, |s, cx| {
                    if kind == AiEditorKind::Composer {
                        s.focus_chat_input()
                    } else {
                        s.focus_message_edit()
                    }
                    cx.notify();
                });
            }),
        };
        let composer =
            cx.new(|_| AiEditorView::new(AiEditorKind::Composer, state.clone(), bindings.clone()));
        let message =
            cx.new(|_| AiEditorView::new(AiEditorKind::MessageEdit, state.clone(), bindings));
        let owner = cx.weak_entity();
        let style = env.clone();
        let header = cx.new(|_| {
            AiChatHeaderView::new(
                state.clone(),
                Rc::new(move |_| {
                    let (t, i) = &*style.borrow();
                    Some(AiChatHeaderFrame {
                        tokens: *t,
                        i18n: i.clone(),
                        background: rgb(t.ui.bg_panel),
                        font: "IBM Plex Sans".into(),
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
                Rc::new(|_, _, _| {}),
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
                    let (t, i) = &*style.borrow();
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
                    let (t, i) = &*style.borrow();
                    let ai = &picker_settings.read(cx).settings(cx).ai;
                    let providers: Vec<AiProviderView> =
                        ai.providers.iter().filter_map(provider_view).collect();
                    let active_provider = ai.active_provider_id.clone();
                    let active_model = ai
                        .active_model
                        .clone()
                        .filter(|m| !m.trim().is_empty())
                        .or_else(|| {
                            providers
                                .iter()
                                .find(|p| Some(&p.id) == active_provider.as_ref())
                                .and_then(|p| p.models.first().cloned())
                        })
                        .or_else(|| Some("OxideSens Demo".into()));
                    Some(AiModelPickerFrame {
                        tokens: *t,
                        i18n: i.clone(),
                        providers,
                        status: Default::default(),
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
                                    prefs.ai.enabled = true;
                                    prefs.ai.enabled_confirmed = true;
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
                anchor: Rc::new(|_, _, _| {}),
                text_anchor: Rc::new(|_, _, _| {}),
            },
            cx,
        );

        let owner = cx.weak_entity();
        let style = env.clone();
        let sidebar = cx.new(|cx| {
            AiAssistantSidebarView::with_bindings(
                state.clone(),
                message,
                AiSidebarChildren {
                    header,
                    background,
                    model_trigger: model.sidebar,
                    composer,
                },
                PortableTranscriptBindings {
                    initialize: Rc::new(move |s, _, cx| {
                        s.initialize_hosted_history(history.as_ref(), cx)
                    }),
                    image_bytes: Rc::new(|_, _, _| Box::pin(async { None })),
                    frame: Rc::new(move |_| {
                        let (t, i) = &*style.borrow();
                        Some(AiTranscriptFrame {
                            tokens: *t,
                            i18n: i.clone(),
                            font: "IBM Plex Sans".into(),
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
                                enabled: true,
                                model_selected: true,
                                images_allowed: false,
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
                            AiTranscriptRequest::Send => a.send(cx),
                            AiTranscriptRequest::Stop => a.stop(cx),
                            AiTranscriptRequest::NewConversation => a.new_chat(cx),
                            AiTranscriptRequest::DeleteConversation(id) => {
                                a.state.update(cx, |s, cx| {
                                    s.delete_conversation(&id);
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
                                    w.focus(&a.focus, cx)
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
                    select_anchor: Rc::new(|_, _, _| {}),
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
                            s.apply_stream_event_state(
                                d.generation,
                                &d.conversation_id,
                                &d.assistant_id,
                                event,
                                None,
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

        Self {
            state,
            sidebar,
            model_menu: model.menu,
            settings,
            focus,
            env,
            serial: 0,
            on_open_settings,
            on_insert_command,
        }
    }

    pub fn environment(&mut self, t: ThemeTokens, i: I18n, cx: &mut Context<Self>) {
        self.env.replace((t, i));
        App::notify(cx, self.sidebar.entity_id());
        App::notify(cx, self.model_menu.entity_id());
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

    fn active_provider_request(
        &self,
        cx: &App,
    ) -> Option<(String, String, String, Option<Zeroizing<String>>)> {
        let ai = &self.settings.read(cx).settings(cx).ai;
        let providers: Vec<AiProviderView> =
            ai.providers.iter().filter_map(provider_view).collect();
        let active = ai
            .active_provider_id
            .as_deref()
            .and_then(|id| providers.iter().find(|p| p.id == id))
            .or_else(|| providers.iter().find(|p| p.enabled))?;
        let model = ai
            .active_model
            .clone()
            .filter(|m| !m.trim().is_empty())
            .or_else(|| active.models.first().cloned())?;
        let key = self.settings.read(cx).provider_api_key(&active.id, cx);
        let keyless_ok = active.provider_type == "ollama" || active.provider_type == "lmstudio";
        if key.is_none() && !keyless_ok {
            return None;
        }
        Some((
            active.provider_type.clone(),
            active.base_url.clone(),
            model,
            key,
        ))
    }

    fn send(&mut self, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        if !state.history_ready() || state.chat_ui().draft.trim().is_empty() {
            return;
        }
        let user_prompt = state.chat_ui().draft.clone();
        let provider_req = self.active_provider_request(cx);
        self.serial += 1;
        let (generation, conversation_id, assistant_id, sender, history_msgs) =
            self.state.update(cx, |s, cx| {
                let id = s.ensure_conversation(
                    format!("chat-{}", self.serial),
                    None,
                    js_sys::Date::now() as i64,
                    None,
                );
                s.add_message(
                    &id,
                    agent_chat_message(AiChatRole::User, user_prompt.clone()),
                );
                let history_msgs: Vec<(String, String)> = s
                    .conversation_state()
                    .active_conversation()
                    .map(|c| {
                        c.messages
                            .iter()
                            .filter(|m| !m.content.trim().is_empty())
                            .map(|m| {
                                let role = match m.role {
                                    AiChatRole::User => "user",
                                    AiChatRole::Assistant => "assistant",
                                    AiChatRole::System => "system",
                                    AiChatRole::Tool => "tool",
                                };
                                (role.to_string(), m.content.clone())
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let mut reply = agent_chat_message(AiChatRole::Assistant, String::new());
                reply.is_streaming = true;
                let aid = reply.id.clone();
                s.add_message(&id, reply);
                s.set_chat_draft(String::new());
                let (gen_id, tx) = s.begin_chat_stream(id.clone(), aid.clone());
                cx.notify();
                (gen_id, id, aid, tx, history_msgs)
            });

        let task = if let Some((provider_type, base_url, model, api_key)) = provider_req {
            cx.spawn(async move |_, _cx| {
                let outcome =
                    fetch_llm_completion(&provider_type, &base_url, &model, api_key, &history_msgs)
                        .await;
                let events = match outcome {
                    Ok(content) => vec![AiStreamEvent::Content(content), AiStreamEvent::Done],
                    Err(err) => vec![AiStreamEvent::Error(err)],
                };
                for event in events {
                    if sender
                        .send(PortableStreamDelivery {
                            generation,
                            conversation_id: conversation_id.clone(),
                            assistant_id: assistant_id.clone(),
                            event: PortableStreamEvent::Stream(event),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
        } else {
            let reply_text = demo_assistant_reply(&user_prompt);
            cx.spawn(async move |_, cx| {
                let chunks: Vec<&str> = reply_text.split_inclusive('\n').collect();
                for chunk in chunks {
                    cx.background_executor()
                        .timer(Duration::from_millis(45))
                        .await;
                    if sender
                        .send(PortableStreamDelivery {
                            generation,
                            conversation_id: conversation_id.clone(),
                            assistant_id: assistant_id.clone(),
                            event: PortableStreamEvent::Stream(AiStreamEvent::Content(
                                chunk.to_string(),
                            )),
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                let _ = sender.send(PortableStreamDelivery {
                    generation,
                    conversation_id,
                    assistant_id,
                    event: PortableStreamEvent::Stream(AiStreamEvent::Done),
                });
            })
        };
        self.state.update(cx, |s, _| {
            s.set_host_stream_task(generation, HostTask::new(move || drop(task)))
        });
    }

    fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if !self.state.read(cx).chat_ui().input_focused {
            return;
        }
        if e.keystroke.key == "enter" && !e.keystroke.modifiers.shift {
            self.send(cx);
        } else if e.keystroke.key == "backspace" {
            self.state.update(cx, |s, cx| {
                s.pop_chat_draft();
                cx.notify()
            });
        } else if let Some(text) = &e.keystroke.key_char
            && !e.keystroke.modifiers.control
            && !e.keystroke.modifiers.platform
        {
            self.state.update(cx, |s, cx| {
                s.replace_chat_input(None, text);
                cx.notify()
            });
        } else {
            return;
        }
        w.prevent_default();
        cx.stop_propagation();
    }
}

async fn fetch_llm_completion(
    provider_type: &str,
    base_url: &str,
    model: &str,
    api_key: Option<Zeroizing<String>>,
    messages: &[(String, String)],
) -> Result<String, String> {
    let window = web_sys::window().ok_or_else(|| "Browser window unavailable".to_string())?;
    let trimmed_base = base_url.trim().trim_end_matches('/');
    let is_anthropic = provider_type == "anthropic";
    let url = if is_anthropic {
        format!("{trimmed_base}/messages")
    } else if trimmed_base.ends_with("/chat/completions") {
        trimmed_base.to_string()
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
            "stream": false,
        })
    };

    let headers = Headers::new().map_err(|_| "Failed to create HTTP headers".to_string())?;
    headers
        .set("Content-Type", "application/json")
        .map_err(|_| "Failed to set Content-Type".to_string())?;
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
            .map_err(|_| "Failed to set auth header".to_string())?;
        if is_anthropic {
            let _ = headers.set("anthropic-version", "2023-06-01");
        }
    }

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    opts.set_headers(&headers);
    let body_str = Zeroizing::new(body.to_string());
    opts.set_body(&wasm_bindgen::JsValue::from_str(&body_str));

    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|_| "Invalid provider API endpoint URL".to_string())?;
    let resp_val = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| "Network or CORS error contacting provider API".to_string())?;
    let resp: Response = resp_val
        .dyn_into()
        .map_err(|_| "Unexpected response object".to_string())?;
    let status = resp.status();
    let text_promise = resp
        .text()
        .map_err(|_| "Failed to read response body".to_string())?;
    let text = JsFuture::from(text_promise)
        .await
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    if !resp.ok() {
        return Err(format!("Provider returned HTTP {status}"));
    }

    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| "Invalid JSON from provider".to_string())?;
    if is_anthropic {
        json.get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| {
                let parts: Vec<&str> = arr
                    .iter()
                    .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                    .collect();
                (!parts.is_empty()).then(|| parts.join(""))
            })
            .ok_or_else(|| "Empty Anthropic response".to_string())
    } else {
        json.get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|t| t.as_str())
            .map(str::to_owned)
            .ok_or_else(|| "Empty completion response".to_string())
    }
}

fn demo_assistant_reply(prompt: &str) -> String {
    let lower = prompt.to_lowercase();
    if lower.contains("port") || lower.contains("端口") || lower.contains("listen") {
        "Active listening sockets on `oxide.demo`:\n\n```sh\nss -tulnp\n```\n\n- `:22` (`sshd`)\n- `:80` / `:443` (`nginx`)\n- `:6379` (`redis-server`)\n- `:8443` (`oxideterm-gateway`)\n\nYou can also inspect them live in the **Ports** Host Tool on the right rail, or add an API key in **Settings → AI → Providers** to use a real LLM.".to_string()
    } else if lower.contains("docker") || lower.contains("容器") || lower.contains("container") {
        "Running containers on `oxide.demo`:\n\n```sh\ndocker ps --format \"table {{.Names}}\\t{{.Image}}\\t{{.Status}}\\t{{.Ports}}\"\n```\n\n- `oxide-web` (`nginx:alpine`, Up 4 days, `8080->80/tcp`)\n- `oxide-redis` (`redis:7-alpine`, Up 4 days, `6379/tcp`)\n\nAdd a real provider & API key in **Settings → AI → Providers** for live model completions.".to_string()
    } else if lower.contains("disk")
        || lower.contains("磁盘")
        || lower.contains("df")
        || lower.contains("空间")
    {
        "Filesystem usage on `oxide.demo`:\n\n```sh\ndf -h / /srv/oxideterm\n```\n\n- `/` — 29 GiB / 80 GiB (36%)\n- `/srv/oxideterm` — 12 GiB / 50 GiB (24%)\n\nOpen **Filesystems** in the right rail for mount and inode details, or configure a live model in **Settings → AI → Providers**.".to_string()
    } else {
        format!(
            "OxideSens is running in **Browser Demo** mode (no live LLM provider configured yet).\n\nReceived: `{}`\n\nTo use a real model directly from the browser:\n1. Open **Settings → AI → Providers** (or click the model selector below)\n2. Add an OpenAI / Anthropic / DeepSeek / Ollama / OpenAI-compatible provider and save an API key (kept in memory only)\n3. Select the model and send your next prompt.\n\nQuick demo shell commands:\n```sh\ncd /srv/oxideterm && ls -la\n```",
            prompt.trim().replace('`', "'")
        )
    }
}

impl Render for Assistant {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .child(self.sidebar.clone())
            .child(self.model_menu.clone())
    }
}
