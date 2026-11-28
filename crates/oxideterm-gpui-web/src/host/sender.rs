mod directory;
use directory::{Directory, Picker};
use super::*;
use oxideterm_gpui_command_sender::{
    ExpandedSender, SenderAction, SenderPanelFrame, TERMINAL_SENDER_RESIZE_HOTZONE_HEIGHT,
    TerminalCommandSenderEntity as Model, TerminalCommandSenderId, TerminalCommandSenderStatus,
    TerminalCommandSenderTarget as Target, TerminalCommandSenderTargetScope as Scope,
};
use oxideterm_gpui_editor::{
    EditorContextMenuLabels,
    line_edit::{FieldEvent, LineField},
};
use oxideterm_gpui_ui::button::{ButtonRadius, IconButtonOptions, icon_button_with_tooltip};
use oxideterm_settings::{TerminalBroadcastTargetKind, TerminalSettings};

pub(super) struct WebSender {
    host: WeakEntity<Workspace>,
    model: Entity<Model>,
    compact: Entity<LineField>,
    directory: Option<Directory>,
    picker: Option<Picker>,
    _changes: Subscription,
    placement: Placement,
}

enum Placement {
    FollowSettings,
    RequestedOn(Option<TabId>),
}

impl WebSender {
    pub(super) fn new(
        host: WeakEntity<Workspace>,
        tokens: ThemeTokens,
        i18n: &I18n,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = i18n.t("terminal.command_bar.command_placeholder");
        let compact = cx.new(|cx| LineField::new(tokens, placeholder.clone(), true, cx));
        let model = cx.new(|cx| {
            Model::new(
                tokens,
                placeholder,
                i18n.t("terminal.sender.placeholder"),
                EditorContextMenuLabels {
                    copy: i18n.t("menu.copy"),
                    cut: i18n.t("fileManager.cut"),
                    paste: i18n.t("menu.paste"),
                    select_all: i18n.t("fileManager.selectAll"),
                },
                cx,
            )
        });
        let changes = cx.observe(&model, |_, _, cx| cx.notify());
        cx.subscribe_in(&compact, w, |sender, field, e, w, cx| match e {
            FieldEvent::Changed => {
                let text = field.read(cx).text().to_owned();
                sender
                    .model
                    .update(cx, |m, cx| m.replace_active_compact_text(text, cx));
            }
            FieldEvent::Submit => sender.run(sender.model.read(cx).active_document_id(), cx),
            FieldEvent::Cancel => {
                sender.placement = Placement::FollowSettings;
                let _ = sender.host.update(cx, |h, cx| {
                    let focus = h
                        .active_session()
                        .and_then(|id| h.sessions.get(&id))
                        .map(|s| s.view.focus_handle(cx))
                        .unwrap_or_else(|| h.focus.clone());
                    w.focus(&focus, cx);
                    h.changed(cx);
                });
            }
        })
        .detach();
        Self {
            host,
            model,
            compact,
            directory: None,
            picker: None,
            _changes: changes,
            placement: Placement::FollowSettings,
        }
    }

    pub(super) fn owns_keys(&self, w: &Window, cx: &App) -> bool {
        self.picker.as_ref().is_some_and(|p| p.search.focus_handle(cx).is_focused(w))
            || self.compact.focus_handle(cx).is_focused(w)
            || self
                .model
                .read(cx)
                .active_document_snapshot()
                .is_some_and(|doc| doc.editor.focus_handle(cx).contains_focused(w, cx))
    }

    fn focus_draft(&self, w: &mut Window, cx: &mut App) {
        let m = self.model.read(cx);
        if m.is_expanded() {
            if let Some(doc) = m.active_document_snapshot() {
                w.focus(&doc.editor.focus_handle(cx), cx);
            }
        } else {
            w.focus(&self.compact.focus_handle(cx), cx);
        }
    }

    fn run(&self, id: TerminalCommandSenderId, cx: &mut App) {
        let group = self
            .model
            .read(cx)
            .active_document_snapshot()
            .filter(|doc| doc.id == id && doc.target_scope == Scope::Group)
            .and_then(|doc| doc.selected_group_id);
        let Ok((current, entries, members)) = self.host.read_with(cx, |h, cx| {
            let entries = h.sender_entries();
            let members = group.map(|g| h.sender_group(g, &entries, cx));
            (
                h.layout.active_tab().and_then(|t| t.active_pane_id),
                entries,
                members,
            )
        }) else {
            return;
        };
        let candidates: Vec<_> = entries
            .iter()
            .map(|&(pane_id, session)| {
                let host = self.host.clone();
                Target {
                    pane_id,
                    write: Rc::new(move |frame, cx| {
                        host.update(cx, |h, cx| h.write_sender_frame(session, frame, cx))
                            .ok()
                            .flatten()
                    }),
                }
            })
            .collect();
        self.model.update(cx, |m, cx| {
            if let (Some(g), Some(panes)) = (group, members) {
                m.set_target_group(id, g, &panes, cx);
            }
            m.retain_live_targets(&entries.iter().map(|e| e.0).collect(), cx);
            if !m.start(id, current, candidates, cx) {
                m.set_expanded(true, cx);
            }
        });
    }

    fn action(&mut self, action: SenderAction, w: &mut Window, cx: &mut Context<Self>) {
        match action {
            SenderAction::Start(id) => self.run(id, cx),
            SenderAction::Group(id, group) => {
                if let Ok(panes) = self
                    .host
                    .read_with(cx, |h, cx| h.sender_group(group, &h.sender_entries(), cx))
                {
                    self.model
                        .update(cx, |m, cx| m.set_target_group(id, group, &panes, cx));
                }
            }
            SenderAction::FocusEditor(_) | SenderAction::FocusCompact => self.focus_draft(w, cx),
            SenderAction::BlurHostInput => {}
        }
    }

    pub(super) fn apply_appearance(
        &self,
        tokens: ThemeTokens,
        i: &I18n,
        terminal: &TerminalSettings,
        cx: &mut App,
    ) {
        self.compact.update(cx, |f, cx| {
            f.environment(tokens, i.t("terminal.command_bar.command_placeholder"), cx)
        });
        let family = terminal
            .font_family
            .terminal_family_name(&terminal.custom_font_family);
        self.model.update(cx, |m, cx| {
            m.sync_editor_appearance(
                tokens,
                family,
                terminal.font_size as f32,
                terminal.line_height as f32,
                false,
                cx,
            )
        });
    }
}

impl Render for WebSender {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Ok((frame, i18n, quick)) = self.host.read_with(cx, |h, cx| {
            let prefs = h.settings.read(cx).settings(cx);
            let current = h
                .active_session()
                .and_then(|id| h.sessions.get(&id))
                .map(|s| s.title.clone())
                .unwrap_or_default();
            (
                SenderPanelFrame {
                    tokens: h.tokens,
                    mono_font: oxideterm_gpui_settings_view::components::settings_mono_font_family(
                        prefs,
                    ),
                    divider: h.chrome_divider(),
                    has_background: h.window_wallpaper().is_some(),
                    current,
                    targets: h
                        .sender_entries()
                        .into_iter()
                        .map(|(pane, id)| {
                            (pane, format!("{} · #{}", h.sessions[&id].title, pane.0))
                        })
                        .collect(),
                    groups: prefs
                        .terminal
                        .broadcast_groups
                        .iter()
                        .map(|g| (g.id, g.name.clone()))
                        .collect(),
                },
                h.i18n.clone(),
                h.demo && prefs.terminal.command_bar.quick_commands_enabled,
            )
        }) else {
            return div().into_any_element();
        };
        let m = self.model.read(cx);
        let expanded = m.is_expanded();
        let height = m.panel_height();
        let tokens = frame.tokens;
        if expanded {
            let owner = cx.weak_entity();
            return div()
                .relative()
                .flex_none()
                .flex()
                .flex_col()
                .h(px(height))
                .overflow_hidden()
                .child(ExpandedSender::new(
                    self.model.clone(),
                    frame,
                    i18n,
                    move |a, w, cx| {
                        let _ = owner.update(cx, |s, cx| s.action(a, w, cx));
                    },
                ))
                .child(
                    div()
                        .id("sender-resize")
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(px(TERMINAL_SENDER_RESIZE_HOTZONE_HEIGHT))
                        .cursor(CursorStyle::ResizeRow)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|s, e: &MouseDownEvent, w, cx| {
                                s.model.update(cx, |m, cx| {
                                    if e.click_count >= 2 {
                                        m.reset_height(cx)
                                    } else {
                                        m.start_resize(e.position.y, cx)
                                    }
                                });
                                w.prevent_default();
                                cx.stop_propagation();
                            }),
                        ),
                )
                .into_any_element();
        }
        let id = m.active_document_id();
        let running = m
            .active_document_snapshot()
            .is_some_and(|doc| doc.status == TerminalCommandSenderStatus::Running);
        let draft =
            zeroize::Zeroizing::new(m.active_compact_draft().unwrap_or_default().to_owned());
        self.compact.update(cx, |f, cx| f.sync_value(&draft, cx));
        let button =
            |icon: LucideIcon,
             key: &'static str,
             id: &'static str,
             click: Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App)>| {
                icon_button_with_tooltip(
                    &tokens,
                    icon.path(),
                    14.,
                    rgb(tokens.ui.text_muted),
                    IconButtonOptions::opaque_toolbar(24., ButtonRadius::Md),
                    i18n.t(key),
                    id,
                    true,
                    click,
                )
            };
        div()
            .flex_none()
            .w_full()
            .h(px(oxideterm_gpui_ui::WORKSPACE_STATUS_BAR_HEIGHT))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(6.))
            .border_t_1()
            .border_color(frame.divider)
            .bg(if frame.has_background {oxideterm_gpui_ui::tab_base_background(&tokens)} else {rgb(tokens.ui.bg)})
            .relative()
            .children(self.directory_chip(&tokens, cx))
            .child(button(
                LucideIcon::ListChecks,
                "terminal.sender.expand",
                "sender-expand",
                Box::new(cx.listener(|s, _, w, cx| {
                    s.picker = None;
                    s.model.update(cx, |m, cx| m.set_expanded(true, cx));
                    s.focus_draft(w, cx);
                })),
            ))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .h(px(24.))
                    .font_family(frame.mono_font)
                    .child(self.compact.clone()),
            )
            .when(quick, |bar| {
                bar.child(button(
                    LucideIcon::Zap,
                    "terminal.quick_commands.title",
                    "sender-quick",
                    Box::new(cx.listener(|s, _, w, cx| {
                        let _ = s.host.update(cx, |h, cx| h.action(Action::Quick, w, cx));
                    })),
                ))
            })
            .child(button(
                if running {
                    LucideIcon::Square
                } else {
                    LucideIcon::Play
                },
                if running {
                    "terminal.sender.stop"
                } else {
                    "terminal.sender.start"
                },
                "sender-run",
                Box::new(cx.listener(move |s, _, _, cx| {
                    if running {
                        s.model.update(cx, |m, cx| m.stop(id, cx));
                    } else {
                        s.run(id, cx);
                    }
                })),
            ))
            .children(self.directory_panel(&tokens, &i18n, cx))
            .into_any_element()
    }
}

impl Workspace {
    fn sender_entries(&self) -> Vec<(PaneId, TerminalSessionId)> {
        let mut entries = Vec::new();
        for (_, tab) in self.layout.unfiltered_main_tabs() {
            if let Some(root) = &tab.root_pane {
                let mut ids = Vec::new();
                root.collect_pane_ids(&mut ids);
                for pane in ids {
                    if let Some(session) = root.session_id_for_pane(pane)
                        && self
                            .sessions
                            .get(&session)
                            .is_some_and(|s| s.writable(self.ready))
                    {
                        entries.push((pane, session));
                    }
                }
            }
        }
        entries
    }

    fn sender_group(
        &self,
        id: uuid::Uuid,
        entries: &[(PaneId, TerminalSessionId)],
        cx: &App,
    ) -> Vec<PaneId> {
        let Some(group) = self
            .settings
            .read(cx)
            .settings(cx)
            .terminal
            .broadcast_groups
            .iter()
            .find(|g| g.id == id)
        else {
            return vec![];
        };
        entries
            .iter()
            .filter_map(|(pane, id)| {
                let s = self.sessions.get(id)?;
                let kind = if s.is_ble() {
                    TerminalBroadcastTargetKind::Ble
                } else if s.node != "local" {
                    TerminalBroadcastTargetKind::Ssh
                } else {
                    return None;
                };
                group
                    .members
                    .iter()
                    .any(|r| r.kind == kind && r.saved_connection_id == s.node)
                    .then_some(*pane)
            })
            .collect()
    }

    pub(crate) fn insert_sender_draft(
        &mut self,
        text: String,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let placement = if self.active_session().is_some()
            && self
                .settings
                .read(cx)
                .settings(cx)
                .terminal
                .command_bar
                .enabled
        {
            Placement::FollowSettings
        } else {
            Placement::RequestedOn(self.layout.active_tab_id())
        };
        self.sender.update(cx, |s, cx| {
            s.placement = placement;
            s.model.update(cx, |m, cx| m.replace_active_text(text, cx));
            s.focus_draft(w, cx);
            cx.notify();
        });
        self.changed(cx);
    }

    pub(super) fn with_sender(&self, content: AnyElement, cx: &App) -> AnyElement {
        let sender = self.sender.read(cx);
        let requested = matches!(sender.placement, Placement::RequestedOn(tab) if tab == self.layout.active_tab_id());
        if !requested
            && (self.active_session().is_none()
                || !self
                    .settings
                    .read(cx)
                    .settings(cx)
                    .terminal
                    .command_bar
                    .enabled)
        {
            return content;
        }
        let available = sender.model.read(cx).available_height.clone();
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .child(div().min_h_0().flex_1().overflow_hidden().child(content))
            .child(self.sender.clone())
            .on_children_prepainted(move |bounds, w, _| {
                let height = bounds.iter().map(|b| f32::from(b.size.height)).sum::<f32>();
                if (available.replace(height) - height).abs() >= 1. {
                    w.refresh();
                }
            })
            .into_any_element()
    }

    pub(crate) fn quick_target_sessions(&self, cx: &App) -> Vec<TerminalSessionId> {
        let entries = self.sender_entries();
        let current = self.layout.active_tab().and_then(|t| t.active_pane_id);
        let Some(doc) = self
            .sender
            .read(cx)
            .model
            .read(cx)
            .active_document_snapshot()
        else {
            return vec![];
        };
        let selected = if doc.target_scope == Scope::Group {
            doc.selected_group_id
                .map(|g| self.sender_group(g, &entries, cx).into_iter().collect())
                .unwrap_or_default()
        } else {
            doc.selected_targets
        };
        entries
            .into_iter()
            .filter_map(|(p, s)| {
                doc.target_scope
                    .includes(p, current, &selected)
                    .then_some(s)
            })
            .collect()
    }

    pub(super) fn resize_sender(&self, event: Option<&MouseMoveEvent>, cx: &mut App) {
        let model = &self.sender.read(cx).model;
        if model.read(cx).is_resizing() {
            model.clone().update(cx, |m, cx| match event {
                Some(e) => m.update_resize(e.position.y, e.dragging(), cx),
                None => m.finish_resize(cx),
            });
        }
    }
}
