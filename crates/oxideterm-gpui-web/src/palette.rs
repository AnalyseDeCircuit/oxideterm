use crate::host::{Action, Workspace};
use gpui::{prelude::*, *};
use oxideterm_gpui_shell::palette::{model::*, *};
use oxideterm_gpui_ui::{icons::LucideIcon, text_input::TextInputViewport};
use std::rc::Rc;

pub struct Palette {
    pub model: Entity<PaletteModel<()>>,
    pub view: Entity<ShellPalette<Action>>,
    viewport: TextInputViewport,
}
impl Palette {
    pub fn new(cx: &mut Context<Workspace>) -> Self {
        let model = cx.new(|_| PaletteModel::new(()));
        let host = cx.weak_entity();
        let dispatch = host.clone();
        let view = cx.new(|cx| {
            ShellPalette::new(
                &model,
                move |_, cx| {
                    host.read_with(cx, |h, cx| {
                        let m = h.palette.model.read(cx);
                        let palette = m.view();
                        PaletteFrame {
                            tokens: h.tokens,
                            i18n: h.i18n.clone(),
                            mono_font: "Lilex".into(),
                            selected: m.selected_index(),
                            preview_selected: false,
                            titlebar_height: 0.,
                            scope_counts: None,
                            input: query_input(
                                &h.tokens,
                                &h.i18n,
                                &palette,
                                &h.palette.viewport,
                                true,
                                None,
                                None,
                                None,
                            )
                            .into_any_element(),
                            ranked_items: h.ranked(cx),
                            palette,
                        }
                    })
                    .ok()
                },
                Rc::new(move |r, w, cx| {
                    let _ = dispatch.update(cx, |h, cx| h.palette_request(r, w, cx));
                }),
                cx,
            )
        });
        Self {
            view,
            model,
            viewport: Default::default(),
        }
    }
    pub fn open(&self, focus: &FocusHandle, w: &mut Window, cx: &mut Context<Workspace>) {
        self.model.update(cx, |m, cx| {
            m.open_content(PaletteContent::Commands, cx);
            m.focus.enter(focus, w, cx);
        });
    }
}
impl Workspace {
    fn ranked(&self, cx: &App) -> Vec<RankedItem<Action>> {
        let items = Action::ALL
            .iter()
            .copied()
            .filter(|a| {
                self.demo
                    || matches!(
                        a,
                        Action::Terminal
                            | Action::Close
                            | Action::Split
                            | Action::Settings
                            | Action::Notifications
                            | Action::Welcome
                    )
            })
            .map(|a| PaletteItem {
                id: a.key().into(),
                label: self.i18n.t(a.key()),
                section: PaletteSection::Commands,
                icon: LucideIcon::Terminal,
                detail: None,
                shortcut: None,
                count: None,
                active: false,
                value: String::new(),
                action: a,
                disabled: false,
            })
            .collect();
        rank_palette_section(items, self.palette.model.read(cx).query())
    }
    pub fn palette_key(&mut self, k: &Keystroke, w: &mut Window, cx: &mut Context<Self>) {
        let m = self.palette.model.read(cx);
        if let Some(r) = key_request(k, m.query(), m.selected_index(), || self.ranked(cx)) {
            self.palette_request(r, w, cx);
        }
    }
    fn palette_request(&mut self, r: ShellPaletteRequest, w: &mut Window, cx: &mut Context<Self>) {
        match r {
            ShellPaletteRequest::Execute(id) => {
                if let Some(a) = self
                    .ranked(cx)
                    .into_iter()
                    .find(|i| i.item.id == id)
                    .map(|i| i.item.action)
                {
                    self.palette_request(ShellPaletteRequest::Close, w, cx);
                    self.action(a, w, cx);
                }
            }
            ShellPaletteRequest::Close => {
                self.palette.model.update(cx, |m, cx| {
                    m.close(cx);
                    m.focus.restore(&self.focus, w, cx);
                });
                self.changed(cx);
            }
            ShellPaletteRequest::Select(i) => {
                let items = self.ranked(cx);
                self.palette.model.update(cx, |m, cx| {
                    m.set_selected_index(i, cx);
                    m.reveal_selection(&items);
                });
            }
            ShellPaletteRequest::QueryChanged(q) => self.palette.model.update(cx, |m, cx| {
                let n = m.query().encode_utf16().count();
                m.replace_query_utf16(Some(0..n), &q, cx);
            }),
        }
    }
}
