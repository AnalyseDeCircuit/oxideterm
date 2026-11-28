use super::*;
use oxideterm_gpui_editor::line_edit::{FieldEvent, LineField};
use oxideterm_gpui_ui::{
    button::{ButtonOptions, ButtonVariant, button_with},
    modal,
};

#[derive(Clone, Copy)]
pub(super) enum Field {
    Sessions,
    Connections,
    TabTitle,
    Marketplace,
}

impl Field {
    fn placeholder(self) -> &'static str {
        match self {
            Self::Sessions => "sidebar.search.placeholder",
            Self::Connections => "sessionManager.toolbar.search_placeholder",
            Self::TabTitle => "tabbar.rename_tab_placeholder",
            Self::Marketplace => "plugin.search_placeholder",
        }
    }
}

pub(super) struct Fields {
    sessions: Entity<LineField>,
    connections: Entity<LineField>,
    title: Entity<LineField>,
    marketplace: Entity<LineField>,
    renaming: Option<TabId>,
}

impl Fields {
    pub(super) fn new(
        tokens: ThemeTokens,
        i: &I18n,
        w: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Self {
        let mut mount = |field: Field| {
            let view = cx.new(|cx| {
                LineField::new(
                    tokens,
                    i.t(field.placeholder()),
                    !matches!(field, Field::TabTitle),
                    cx,
                )
            });
            cx.subscribe_in(&view, w, move |h, _, e, w, cx| {
                h.field_event(field, *e, w, cx)
            })
            .detach();
            view
        };
        Self {
            sessions: mount(Field::Sessions),
            connections: mount(Field::Connections),
            title: mount(Field::TabTitle),
            marketplace: mount(Field::Marketplace),
            renaming: None,
        }
    }
    pub(super) fn view(&self, field: Field) -> &Entity<LineField> {
        match field {
            Field::Sessions => &self.sessions,
            Field::Connections => &self.connections,
            Field::TabTitle => &self.title,
            Field::Marketplace => &self.marketplace,
        }
    }
    pub(super) fn environment(&self, t: ThemeTokens, i: &I18n, cx: &mut App) {
        for field in [
            Field::Sessions,
            Field::Connections,
            Field::TabTitle,
            Field::Marketplace,
        ] {
            self.view(field)
                .update(cx, |v, cx| v.environment(t, i.t(field.placeholder()), cx));
        }
    }
    pub(super) fn owns_keys(&self, w: &Window, cx: &App) -> bool {
        self.renaming.is_some()
            || [&self.connections, &self.sessions, &self.marketplace]
                .into_iter()
                .any(|v| v.read(cx).focus_handle(cx).is_focused(w))
    }
}

impl Workspace {
    fn field_event(
        &mut self,
        field: Field,
        event: FieldEvent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match (field, event) {
            (Field::Marketplace, FieldEvent::Changed) => {
                self.pages.catalog.page = 0;
                cx.notify();
            }
            (Field::Marketplace, FieldEvent::Cancel) => {
                self.fields
                    .marketplace
                    .update(cx, |v, cx| v.reset("", false, cx));
                self.pages.catalog.page = 0;
                w.focus(&self.focus, cx);
                cx.notify();
            }
            (_, FieldEvent::Changed) => cx.notify(),
            (Field::TabTitle, FieldEvent::Submit) => self.end_tab_rename(true, w, cx),
            (Field::TabTitle, FieldEvent::Cancel) => self.end_tab_rename(false, w, cx),
            (Field::Sessions | Field::Connections, FieldEvent::Cancel) => {
                self.fields
                    .view(field)
                    .update(cx, |v, cx| v.reset("", false, cx));
                let key = match field {
                    Field::Sessions => {
                        self.session_search_open = false;
                        "sessions-search"
                    }
                    Field::Connections => {
                        self.connections_search_open = false;
                        "connections-search"
                    }
                    _ => unreachable!(),
                };
                self.begin_disclosure(key.into(), false, cx);
                w.focus(&self.focus, cx);
                self.changed(cx);
            }
            (_, FieldEvent::Submit) => w.focus(&self.focus, cx),
        }
    }
    pub(super) fn begin_tab_rename(&mut self, id: TabId, w: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.layout.tab_by_id(id) else {
            return;
        };
        if !oxideterm_gpui_shell::tabs::tab_is_renamable(&tab.kind) {
            return;
        }
        self.fields
            .title
            .update(cx, |v, cx| v.reset(&tab.title, true, cx));
        self.fields.renaming = Some(id);
        w.focus(&self.fields.title.read(cx).focus_handle(cx), cx);
        cx.notify();
    }
    fn end_tab_rename(&mut self, commit: bool, w: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.fields.renaming else {
            return;
        };
        if commit {
            let title = self.fields.title.read(cx).text();
            if title.trim().is_empty() {
                return;
            }
            if self.layout.rename_tab(id, title)
                && let Some(tab) = self.layout.tab_by_id(id)
                && tab.kind != TabKind::Workspace
                && let Some(sid) = tab
                    .root_pane
                    .as_ref()
                    .and_then(|r| r.session_id_for_pane(tab.active_pane_id?))
                && let Some(session) = self.sessions.get_mut(&sid)
            {
                session.title = title.trim().to_owned();
            }
        }
        self.fields.renaming = None;
        if let Some(s) = self.active_session().and_then(|id| self.sessions.get(&id)) {
            w.focus(&s.view.read(cx).focus_handle(cx), cx);
        } else {
            w.focus(&self.focus, cx);
        }
        self.changed(cx);
    }
    pub(super) fn rename_dialog(&self, cx: &Context<Self>) -> Option<AnyElement> {
        self.fields.renaming?;
        let t = &self.tokens;
        let disabled = self.fields.title.read(cx).text().trim().is_empty();
        let content = modal::dialog_content(t)
            .w(px(420.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(modal::modal_header(
                t,
                self.i18n.t("tabbar.rename_tab"),
                self.i18n.t("tabbar.rename_tab_description"),
            ))
            .child(modal::modal_body(t).child(self.fields.title.clone()))
            .child(
                modal::dialog_footer(t)
                    .child(
                        button_with(t, self.i18n.t("common.cancel"), ButtonOptions::default())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|h, _, w, cx| h.end_tab_rename(false, w, cx)),
                            ),
                    )
                    .child(
                        button_with(
                            t,
                            self.i18n.t("tabbar.rename_tab_action"),
                            ButtonOptions {
                                variant: ButtonVariant::Default,
                                disabled,
                                ..Default::default()
                            },
                        )
                        .when(!disabled, |b| {
                            b.on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|h, _, w, cx| h.end_tab_rename(true, w, cx)),
                            )
                        }),
                    ),
            );
        Some(modal::staged_dialog_overlay(
            t,
            "web-tab-rename",
            modal::TauriBackdropRole::Dialog,
            modal::staged_overlay_root().on_mouse_down(
                MouseButton::Left,
                cx.listener(|h, _, w, cx| h.end_tab_rename(false, w, cx)),
            ),
            content,
            8.,
            MotionDuration::Control,
            true,
        ))
    }
}
