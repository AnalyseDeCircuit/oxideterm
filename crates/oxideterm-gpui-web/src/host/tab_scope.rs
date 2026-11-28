use super::*;
use crate::palette::PaletteAction;
use oxideterm_gpui_shell::{
    palette::{
        PaletteContent,
        model::{PaletteItem, PaletteSection, RankedItem, rank_palette_item},
    },
    tabs::{
        TabContent, TabScope,
        scope::{ScopeButton, ScopeTrigger},
    },
};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum Affinity {
    Global,
    Local,
    Node(String),
    Transfer { profile: String, endpoint: String },
}

pub(crate) type Scope = TabScope<Affinity>;
pub(crate) type BrowserTabs = TabLayout<Affinity>;

impl Workspace {
    fn tab_affinities(&self, cx: &App) -> HashMap<TabId, Vec<Affinity>> {
        self.layout
            .unfiltered_main_tabs()
            .map(|(_, tab)| {
                let mut keys = Vec::new();
                let mut add = |key| {
                    if !keys.contains(&key) {
                        keys.push(key);
                    }
                };
                for content in self.layout.tab_contents(tab.id) {
                    match content {
                        TabContent::Terminal(id) => {
                            if let Some(s) = self.sessions.get(&id) {
                                add(if s.node == "local" && self.demo {
                                    Affinity::Local
                                } else {
                                    Affinity::Node(s.node.clone())
                                });
                            }
                        }
                        TabContent::Page(id) => {
                            if let Some(sftp) =
                                self.files.as_ref().and_then(|f| f.sftp_pages.get(&id))
                            {
                                let target = &sftp.read(cx).target;
                                if let Some(p) = &target.profile {
                                    for endpoint in
                                        std::iter::once(&p.primary).chain(p.secondary.as_ref())
                                    {
                                        add(Affinity::Transfer {
                                            profile: target.id.clone(),
                                            endpoint: endpoint.label(),
                                        });
                                    }
                                } else {
                                    add(Affinity::Node(target.id.clone()));
                                }
                            } else if let Some(node) = self
                                .projects
                                .project_node(id)
                                .or_else(|| self.tools.forwards.node(id))
                            {
                                add(Affinity::Node(node.into()));
                            } else {
                                add(Affinity::Global);
                            }
                        }
                    }
                }
                (tab.id, keys)
            })
            .collect()
    }

    // Recompute only after tab membership or explicit selection changes, never on terminal output.
    pub(crate) fn sync_active_scope(&mut self, cx: &App) {
        let Scope::Affinity(current) = self.layout.scope() else {
            return;
        };
        let keys = self.tab_affinities(cx);
        let active = self.layout.active_tab_id();
        let reveal = active
            .and_then(|id| keys.get(&id))
            .filter(|values| !values.contains(current))
            .map(|values| values.first().cloned().map_or(Scope::All, Scope::Affinity));
        self.layout.update_affinities(keys);
        if let Some(scope) = reveal {
            self.layout.prepare_scope(scope);
            self.layout.observe_active_tab(active);
        }
        self.layout.reconcile_scope_selection();
    }

    pub(super) fn reconcile_scope_after_close(&mut self, cx: &App) {
        if *self.layout.scope() != Scope::All {
            self.layout.update_affinities(self.tab_affinities(cx));
            self.layout.reconcile_scope_selection();
        }
    }

    fn scope_label(&self, scope: &Scope) -> (String, Option<String>) {
        match scope {
            Scope::All => (self.i18n.t("sidebar.tab_scope.all"), None),
            Scope::Affinity(Affinity::Global) => (self.i18n.t("sidebar.tab_scope.global"), None),
            Scope::Affinity(Affinity::Local) => (self.i18n.t("sidebar.tab_scope.local"), None),
            Scope::Affinity(Affinity::Node(id)) => {
                if let Some(row) = self.catalog_data.items.iter().find(|r| r.id() == id) {
                    (row.name.clone(), Some(row.subtitle.clone()))
                } else if id == "local" && !self.demo {
                    (
                        self.gateway_host
                            .clone()
                            .unwrap_or_else(|| self.i18n.t("menu.terminal")),
                        None,
                    )
                } else {
                    (id.clone(), None)
                }
            }
            Scope::Affinity(Affinity::Transfer { profile, endpoint }) => (
                endpoint.clone(),
                self.catalog_data
                    .items
                    .iter()
                    .find(|r| r.id() == profile)
                    .map(|r| r.name.clone()),
            ),
        }
    }

    pub(crate) fn scope_choices(&self, query: &str, cx: &App) -> Vec<RankedItem<PaletteAction>> {
        let affinities = self.tab_affinities(cx);
        let mut counts = HashMap::new();
        for key in affinities.values().flatten() {
            *counts.entry(key.clone()).or_insert(0usize) += 1;
        }
        let mut choices = vec![Scope::All, Scope::Affinity(Affinity::Global)];
        if self.demo {
            choices.push(Scope::Affinity(Affinity::Local));
        }
        let mut connections = counts
            .keys()
            .filter(|k| matches!(k, Affinity::Node(_) | Affinity::Transfer { .. }))
            .cloned()
            .map(Scope::Affinity)
            .collect::<Vec<_>>();
        connections.sort_by_cached_key(|s| {
            (
                self.scope_label(s).0.to_lowercase(),
                match s {
                    Scope::Affinity(k) => Some(k.clone()),
                    Scope::All => None,
                },
            )
        });
        choices.extend(connections);
        choices
            .into_iter()
            .filter_map(|scope| {
                let (label, detail) = self.scope_label(&scope);
                let value = format!("{label} {}", detail.as_deref().unwrap_or_default());
                let (id, icon, section) = match &scope {
                    Scope::All => (
                        "scope:all".into(),
                        LucideIcon::Layers,
                        PaletteSection::Commands,
                    ),
                    Scope::Affinity(Affinity::Global) => (
                        "scope:global".into(),
                        LucideIcon::AppWindow,
                        PaletteSection::Commands,
                    ),
                    Scope::Affinity(Affinity::Local) => (
                        "scope:local".into(),
                        LucideIcon::Terminal,
                        PaletteSection::Commands,
                    ),
                    Scope::Affinity(Affinity::Node(id)) => (
                        format!("scope:node:{id}"),
                        if crate::ble::is_ble_node(id) {
                            LucideIcon::Bluetooth
                        } else {
                            LucideIcon::Server
                        },
                        PaletteSection::Connections,
                    ),
                    Scope::Affinity(Affinity::Transfer { profile, endpoint }) => (
                        format!("scope:sftp:{profile}:{endpoint}"),
                        LucideIcon::FolderSync,
                        PaletteSection::Connections,
                    ),
                };
                let count = match &scope {
                    Scope::All => affinities.len(),
                    Scope::Affinity(k) => counts.get(k).copied().unwrap_or(0),
                };
                rank_palette_item(
                    PaletteItem {
                        id,
                        label,
                        detail,
                        icon,
                        section,
                        value,
                        count: Some(count.to_string()),
                        active: &scope == self.layout.scope(),
                        shortcut: None,
                        disabled: false,
                        action: PaletteAction::SelectScope(scope),
                    },
                    query,
                )
            })
            .collect()
    }

    pub(crate) fn select_scope(&mut self, scope: Scope, w: &mut Window, cx: &mut Context<Self>) {
        if self.onboarding_locked() {
            return;
        }
        self.layout.update_affinities(self.tab_affinities(cx));
        self.layout.switch_scope(scope);
        self.tab_drop.preview = None;
        self.strip.update(cx, |strip, cx| {
            strip.state.drag = None;
            strip.state.exiting_tabs.clear();
            strip.state.scroll_handle.set_offset(point(px(0.), px(0.)));
            cx.notify();
        });
        if let Some(s) = self.active_session().and_then(|s| self.sessions.get(&s)) {
            w.focus(&s.view.read(cx).focus_handle(cx), cx);
        } else {
            w.focus(&self.focus, cx);
        }
        self.changed(cx);
    }

    fn scope_button(&mut self, action: ScopeButton, w: &mut Window, cx: &mut Context<Self>) {
        match action {
            ScopeButton::Clear => self.select_scope(Scope::All, w, cx),
            ScopeButton::Pick => {
                let choices = self.scope_choices("", cx);
                let selected = choices.iter().position(|i| i.item.active).unwrap_or(0);
                self.palette.model.update(cx, |m, cx| {
                    m.open_content(PaletteContent::TabScopes, cx);
                    m.set_selected_index(selected, cx);
                    m.reveal_selection(&choices);
                    m.focus.enter(&self.focus, w, cx);
                });
                self.changed(cx);
            }
        }
    }

    pub(super) fn scope_trigger(&self, width: f32, cx: &Context<Self>) -> Option<AnyElement> {
        let tokens = self.tokens;
        ScopeTrigger {
            label: self.scope_label(self.layout.scope()).0,
            filtered: *self.layout.scope() != Scope::All,
            visible_count: self.layout.main_tabs().count(),
            total_count: self.layout.unfiltered_main_tabs().count(),
            available_width: width,
            text_color: tokens.ui.text_muted,
            hover_bg: tokens.ui.bg_hover,
        }
        .render(&tokens, &self.i18n, |button, action, label| {
            let owner = cx.weak_entity();
            button
                .role(Role::Button)
                .aria_label(label.clone())
                .tooltip(move |_, cx| {
                    oxideterm_gpui_ui::tooltip::tooltip_view(tokens, label.clone(), None, cx)
                })
                .on_a11y_action(AccessibleAction::Click, move |_, w, cx| {
                    let _ = owner.update(cx, |h, cx| h.scope_button(action, w, cx));
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |h, _, w, cx| {
                        h.scope_button(action, w, cx);
                        cx.stop_propagation();
                    }),
                )
        })
    }

    pub(super) fn empty_scope(&self, cx: &Context<Self>) -> AnyElement {
        use oxideterm_gpui_ui::button::*;
        oxideterm_gpui_ui::empty_state(
            &self.tokens,
            super::chrome::glyph(LucideIcon::LayoutList, 24., self.tokens.ui.accent),
            self.i18n.t("sidebar.tab_scope.empty"),
            None,
            Some(
                button_with(
                    &self.tokens,
                    self.i18n.t("sidebar.tab_scope.show_all"),
                    ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Default,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|h, _, w, cx| h.select_scope(Scope::All, w, cx)),
                )
                .into_any_element(),
            ),
        )
        .size_full()
        .into_any_element()
    }
}
