mod history;
mod services;

use gpui::{prelude::*, *};
use oxideterm_ai::{AiChatRole, AiStreamEvent, agent_chat_message};
use oxideterm_gpui_ai::{
    background::*,
    bridge::{AiHostBridge, HostTask},
    knowledge::page::KnowledgePage,
    model_picker::*,
    sidebar::{editor::*, transcript::*, *},
    state::{AiWorkspaceEntity, AiWorkspaceEvent},
    stream::*,
};
use oxideterm_gpui_ui::text_input::TextInputAnchorId;
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

pub struct Assistant {
    state: Entity<AiWorkspaceEntity>,
    sidebar: Entity<AiAssistantSidebarView>,
    focus: FocusHandle,
    env: Rc<RefCell<(ThemeTokens, I18n)>>,
    serial: u64,
}
impl Assistant {
    pub fn new(t: ThemeTokens, i: I18n, cx: &mut Context<Self>) -> Self {
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
        let model = AiModelPickerViews::mount(
            &state,
            AiModelPickerBindings {
                frame: Rc::new(move |_| {
                    let (t, i) = &*style.borrow();
                    Some(AiModelPickerFrame {
                        tokens: *t,
                        i18n: i.clone(),
                        providers: vec![],
                        status: Default::default(),
                        active_provider: None,
                        active_model: Some("OxideTerm Demo".into()),
                        acp_active: false,
                        active_acp_agent_id: None,
                        marked: None,
                        search_anchor: TextInputAnchorId(3),
                        caret_visible: true,
                    })
                }),
                request: Rc::new(|_, _, _| {}),
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
                    if let Some((ticket, d)) = s.take_chat_window_request(cx) {
                        if let AiChatWindowDelivery::Host(d) = d
                            && let PortableStreamEvent::Stream(event) = d.event
                        {
                            s.apply_stream_event_state(
                                d.generation,
                                &d.conversation_id,
                                &d.assistant_id,
                                event,
                                None,
                            );
                            s.complete_chat_stream(d.generation);
                            s.set_conversation_loading(&d.conversation_id, false);
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
            focus,
            env,
            serial: 0,
        }
    }
    pub fn environment(&mut self, t: ThemeTokens, i: I18n, cx: &mut Context<Self>) {
        self.env.replace((t, i));
        App::notify(cx, self.sidebar.entity_id());
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
    fn send(&mut self, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        if !state.history_ready() || state.chat_ui().draft.trim().is_empty() {
            return;
        }
        let text = state.chat_ui().draft.clone();
        self.serial += 1;
        let (generation, conversation_id, assistant_id, sender) = self.state.update(cx, |s, cx| {
            let id = s.ensure_conversation(
                format!("chat-{}", self.serial),
                None,
                js_sys::Date::now() as i64,
                None,
            );
            s.add_message(&id, agent_chat_message(AiChatRole::User, text));
            let mut reply = agent_chat_message(AiChatRole::Assistant, String::new());
            reply.is_streaming = true;
            let aid = reply.id.clone();
            s.add_message(&id, reply);
            s.set_chat_draft(String::new());
            let (gen_id, tx) = s.begin_chat_stream(id.clone(), aid.clone());
            cx.notify();
            (gen_id, id, aid, tx)
        });
        let intro = self.env.borrow().1.t("menu.terminal");
        let task = cx.spawn(async move |_, cx| {
            for event in [
                AiStreamEvent::Content(format!("{intro}: ")),
                AiStreamEvent::Content("`pwd`, `ls`, `cat README.md`.\n\n".into()),
                AiStreamEvent::Content("```sh\ncd /srv/oxideterm\nls\ncat README.md\n```".into()),
                AiStreamEvent::Done,
            ] {
                cx.background_executor()
                    .timer(Duration::from_millis(170))
                    .await;
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
        });
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
impl Render for Assistant {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .child(self.sidebar.clone())
    }
}
