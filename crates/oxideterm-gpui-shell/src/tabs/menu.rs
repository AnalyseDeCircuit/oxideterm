use super::tab_is_renamable;
use gpui::{prelude::*, *};
use oxideterm_gpui_ui::{context_menu::*, modal::TAURI_POPOVER_LAYER_PRIORITY};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::{SplitDirection, TabId, TabKind, TerminalSessionId};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    Rename,
    Edit,
    Detach,
    Return,
    Terminal(TerminalSessionId),
    NewTerminal,
    Sftp,
    Ide,
    Forwards,
    DrillIn,
    Disconnect,
    CloseTerminals,
    Sidebar,
    Merge(TabId, SplitDirection),
    Close,
    Others,
    Right,
    All,
}

pub struct Options {
    pub kind: TabKind,
    pub actions: Vec<Command>,
    pub terminals: Vec<(TerminalSessionId, bool)>,
    pub active_merge: Option<TabId>,
    pub merge_targets: Vec<(TabId, String)>,
    pub visible: Vec<TabId>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Choice {
    Run(Command),
    Submenu(SplitDirection),
}
struct Row {
    label: String,
    icon: &'static str,
    choice: Choice,
    separator: bool,
    active: bool,
}

impl Options {
    fn rows(&self, tab: TabId, i: &I18n) -> Vec<Row> {
        use Command::*;
        let mut rows = Vec::new();
        let mut row = |label: String, icon, choice, separator, active| {
            rows.push(Row {
                label,
                icon,
                choice,
                separator,
                active,
            })
        };
        if tab_is_renamable(&self.kind) {
            row(
                i.t("tabbar.rename_tab"),
                "lucide/pencil.svg",
                Choice::Run(Rename),
                false,
                false,
            );
        }
        if self.actions.contains(&Edit) {
            row(
                i.t("tabbar.edit_connection"),
                "lucide/settings.svg",
                Choice::Run(Edit),
                false,
                false,
            );
        }
        for (dir, icon, immediate, choose) in [
            (
                SplitDirection::Horizontal,
                "lucide/square-split-horizontal.svg",
                "tabbar.split_into_active_horizontal",
                "tabbar.merge_side_by_side_into",
            ),
            (
                SplitDirection::Vertical,
                "lucide/square-split-vertical.svg",
                "tabbar.split_into_active_vertical",
                "tabbar.merge_stacked_into",
            ),
        ] {
            let (choice, key) = if let Some(target) = self.active_merge {
                (Choice::Run(Merge(target, dir)), immediate)
            } else if !self.merge_targets.is_empty() {
                (Choice::Submenu(dir), choose)
            } else {
                continue;
            };
            row(
                i.t(key),
                icon,
                choice,
                dir == SplitDirection::Horizontal,
                false,
            );
        }
        for (cmd, key, icon) in [
            (
                Detach,
                "tabbar.detach_to_window",
                "lucide/external-link.svg",
            ),
            (
                Return,
                "tabbar.return_to_main_window",
                "lucide/arrow-left-right.svg",
            ),
        ] {
            if self.actions.contains(&cmd) {
                row(i.t(key), icon, Choice::Run(cmd), false, false);
            }
        }
        for (index, (session, active)) in self.terminals.iter().enumerate() {
            row(
                i.t("sessions.tree.terminal")
                    .replace("{{number}}", &(index + 1).to_string()),
                "lucide/terminal.svg",
                Choice::Run(Terminal(*session)),
                index == 0,
                *active,
            );
        }
        let mut first_tool = true;
        for (cmd, key, icon) in [
            (
                NewTerminal,
                "sessions.tree.actions.new_terminal",
                "lucide/plus.svg",
            ),
            (Sftp, "sessions.tree.actions.sftp", "lucide/folder-open.svg"),
            (Ide, "settings_view.tabs.ide", "lucide/code-2.svg"),
            (
                Forwards,
                "sessions.tree.actions.port_forwarding",
                "lucide/arrow-left-right.svg",
            ),
            (
                DrillIn,
                "sessions.tree.actions.drill_in",
                "lucide/arrow-down-right.svg",
            ),
            (
                Disconnect,
                "sessions.tree.actions.disconnect",
                "lucide/wifi-off.svg",
            ),
            (
                CloseTerminals,
                "tabbar.close_terminals",
                "lucide/terminal.svg",
            ),
            (Sidebar, "tabbar.switch_to_sidebar", "lucide/panel-left.svg"),
        ] {
            if self.actions.contains(&cmd) {
                row(
                    i.t(key),
                    icon,
                    Choice::Run(cmd),
                    first_tool || matches!(cmd, Disconnect | CloseTerminals),
                    false,
                );
                first_tool = false;
            }
        }
        row(
            i.t("tabbar.close_tab"),
            "lucide/x.svg",
            Choice::Run(Close),
            true,
            false,
        );
        if self.visible.contains(&tab) && self.visible.len() > 1 {
            row(
                i.t("tabbar.close_other_tabs"),
                "lucide/x.svg",
                Choice::Run(Others),
                false,
                false,
            );
            if self.visible.last() != Some(&tab) {
                row(
                    i.t("tabbar.close_tabs_to_right"),
                    "lucide/arrow-right.svg",
                    Choice::Run(Right),
                    false,
                    false,
                );
            }
            row(
                i.t("tabbar.close_all_tabs"),
                "lucide/x.svg",
                Choice::Run(All),
                false,
                false,
            );
        }
        if let Some(first) = rows.first_mut() {
            first.separator = false;
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::{BrowserOverlayPlacement, TAB_CONTEXT_SUBMENU_OVERLAP, place_tab_context_submenu};

    #[test]
    fn tab_context_merge_submenu_placement_flips_left_and_clamps_bottom() {
        let normal = place_tab_context_submenu(
            BrowserOverlayPlacement { x: 120.0, y: 80.0 },
            228.0,
            64.0,
            200.0,
            120.0,
            1200.0,
            800.0,
            8.0,
        );
        assert_eq!(normal.x, 120.0 + 228.0 - TAB_CONTEXT_SUBMENU_OVERLAP);
        assert_eq!(normal.y, 144.0);

        // Near the right and bottom viewport edges, flip to the left of the
        // parent menu and clamp vertically within the viewport margin.
        let flipped = place_tab_context_submenu(
            BrowserOverlayPlacement { x: 940.0, y: 680.0 },
            228.0,
            96.0,
            200.0,
            160.0,
            1200.0,
            800.0,
            8.0,
        );
        assert_eq!(flipped.x, 940.0 - 200.0 + TAB_CONTEXT_SUBMENU_OVERLAP);
        assert_eq!(flipped.y, 800.0 - 160.0 - 8.0);
    }
}

type Dispatch = Rc<dyn Fn(Option<Command>, Modifiers, &mut Window, &mut App)>;

pub struct TabMenu {
    tab: TabId,
    position: Point<Pixels>,
    focus: FocusHandle,
    previous: Option<FocusHandle>,
    selected: usize,
    sub: Option<(SplitDirection, usize)>,
    scroll: ScrollHandle,
    sub_scroll: ScrollHandle,
    dispatch: Dispatch,
    frame: Rc<dyn Fn(&App) -> Option<(ThemeTokens, I18n, Options)>>,
}

impl TabMenu {
    pub fn open(
        tab: TabId,
        position: Point<Pixels>,
        w: &mut Window,
        cx: &mut App,
        frame: impl Fn(&App) -> Option<(ThemeTokens, I18n, Options)> + 'static,
        dispatch: impl Fn(Option<Command>, Modifiers, &mut Window, &mut App) + 'static,
    ) -> Entity<Self> {
        let previous = w.focused(cx);
        let focus = cx.focus_handle();
        w.focus(&focus, cx);
        cx.new(|_| Self {
            tab,
            position,
            focus,
            previous,
            selected: 0,
            sub: None,
            scroll: ScrollHandle::new(),
            sub_scroll: ScrollHandle::new(),
            dispatch: Rc::new(dispatch),
            frame: Rc::new(frame),
        })
    }

    fn finish(&self, cmd: Option<Command>, mods: Modifiers, w: &mut Window, cx: &mut App) {
        if let Some(previous) = &self.previous {
            w.focus(previous, cx);
        }
        let cmd = cmd.filter(|cmd| (self.frame)(cx).is_some_and(|(_,i,opts)| {
            opts.rows(self.tab,&i).iter().any(|r|r.choice==Choice::Run(*cmd))
                || matches!(cmd,Command::Merge(target,_) if opts.active_merge.is_none() && opts.merge_targets.iter().any(|(id,_)|id==target))
        }));
        (self.dispatch)(cmd, mods, w, cx);
    }

    fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        w.prevent_default();
        let Some((_, i, opts)) = (self.frame)(cx) else {
            self.finish(None, e.keystroke.modifiers, w, cx);
            return;
        };
        let rows = opts.rows(self.tab, &i);
        let key = e.keystroke.key.as_str();
        if let Some((dir, index)) = self.sub {
            let n = opts.merge_targets.len();
            let index = index.min(n.saturating_sub(1));
            match key {
                "left" | "arrowleft" | "escape" => self.sub = None,
                "enter" | "space" | " " => {
                    if let Some((target, _)) = opts.merge_targets.get(index) {
                        self.finish(
                            Some(Command::Merge(*target, dir)),
                            e.keystroke.modifiers,
                            w,
                            cx,
                        );
                        return;
                    }
                }
                "tab" => {
                    self.finish(None, e.keystroke.modifiers, w, cx);
                    return;
                }
                _ => {
                    if let Some(next) = step(key, index, n) {
                        self.sub = Some((dir, next));
                        self.sub_scroll.scroll_to_item(next);
                    }
                }
            }
        } else {
            self.selected = self.selected.min(rows.len().saturating_sub(1));
            match key {
                "escape" | "tab" => {
                    self.finish(None, e.keystroke.modifiers, w, cx);
                    return;
                }
                "right" | "arrowright" | "enter" | "space" | " " => {
                    if let Some(r) = rows.get(self.selected) {
                        match r.choice {
                            Choice::Run(cmd) if !matches!(key, "right" | "arrowright") => {
                                self.finish(Some(cmd), e.keystroke.modifiers, w, cx);
                                return;
                            }
                            Choice::Submenu(dir) => {
                                self.sub = Some((dir, 0));
                                self.sub_scroll.scroll_to_item(0);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {
                    if let Some(next) = step(key, self.selected, rows.len()) {
                        self.selected = next;
                        self.scroll.scroll_to_item(next);
                    }
                }
            }
        }
        cx.notify();
    }
}

const TAB_CONTEXT_SUBMENU_OVERLAP: f32 = 2.0;

/// Positions a cascading tab context submenu next to its trigger row,
/// preferring the right side with a slight overlap to eliminate pointer dead
/// zones and flipping to the left when it would overflow the viewport.
fn place_tab_context_submenu(
    parent_placement: BrowserOverlayPlacement,
    parent_width: f32,
    trigger_offset_y: f32,
    sub_width: f32,
    sub_height: f32,
    viewport_width: f32,
    viewport_height: f32,
    margin: f32,
) -> BrowserOverlayPlacement {
    let right_x = parent_placement.x + parent_width - TAB_CONTEXT_SUBMENU_OVERLAP;
    let x = if right_x + sub_width + margin <= viewport_width {
        right_x
    } else {
        (parent_placement.x - sub_width + TAB_CONTEXT_SUBMENU_OVERLAP).max(margin)
    };
    let max_y = (viewport_height - sub_height - margin).max(margin);
    let y = (parent_placement.y + trigger_offset_y).clamp(margin, max_y);
    BrowserOverlayPlacement { x, y }
}

fn step(key: &str, index: usize, n: usize) -> Option<usize> {
    if n == 0 {
        return None;
    }
    Some(match key {
        "up" | "arrowup" => (index + n - 1) % n,
        "down" | "arrowdown" => (index + 1) % n,
        "home" => 0,
        "end" => n - 1,
        _ => return None,
    })
}

impl Render for TabMenu {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some((t, i, opts)) = (self.frame)(cx) else {
            return div().into_any_element();
        };
        let rows = opts.rows(self.tab, &i);
        let viewport = w.viewport_size();
        let vw = f32::from(viewport.width);
        let vh = f32::from(viewport.height);
        let margin = 8.;
        let max_h = (vh - 2. * margin).max(1.);
        let width = oxideterm_gpui_ui::estimate_menu_width(
            &t,
            if opts.active_merge.is_some() {
                320.
            } else {
                228.
            },
            380.,
            true,
            rows.iter().map(|r| r.label.as_str()),
        )
        .min((vw - 2. * margin).max(1.));
        let rh = context_menu_item_height_estimate(&t);
        let sep = context_menu_separator_height_estimate(&t);
        let height = (rows.len() as f32 * rh
            + rows.iter().filter(|r| r.separator).count() as f32 * sep
            + 2. * t.metrics.ui_menu_padding)
            .min(max_h);
        let pos = clamp_context_menu_position(
            f32::from(self.position.x),
            f32::from(self.position.y),
            vw,
            vh,
            width,
            height,
            margin,
        );
        let mut menu = context_menu_content(&t)
            .w(px(width))
            .id("tab-context-menu-content")
            .max_h(px(max_h))
            .overflow_y_scroll()
            .track_scroll(&self.scroll);
        let mut y = t.metrics.ui_menu_padding + f32::from(self.scroll.offset().y);
        let mut submenu = None;
        for (index, r) in rows.into_iter().enumerate() {
            let choice = r.choice;
            if r.separator {
                y += sep;
            }
            if let Choice::Submenu(dir) = choice
                && self.sub.is_some_and(|(s, _)| s == dir)
            {
                submenu = Some((dir, y));
            }
            let owner = cx.weak_entity();
            let row = context_menu_item_with_icon(
                &t,
                r.icon,
                r.label.clone(),
                matches!(
                    choice,
                    Choice::Run(
                        Command::Disconnect
                            | Command::CloseTerminals
                            | Command::Close
                            | Command::All
                    )
                ),
                false,
            )
            .id(("tab-context-row", index))
            .role(Role::MenuItem)
            .aria_label(r.label)
            .when_some(
                match choice {
                    Choice::Run(Command::Rename) => Some("tab-context-menu-rename"),
                    Choice::Run(Command::Edit) => Some("tab-context-menu-edit-connection"),
                    Choice::Run(Command::Detach | Command::Return) => {
                        Some("tab-context-menu-detach")
                    }
                    Choice::Submenu(SplitDirection::Horizontal) => {
                        Some("tab-context-menu-merge-horizontal")
                    }
                    Choice::Submenu(SplitDirection::Vertical) => {
                        Some("tab-context-menu-merge-vertical")
                    }
                    _ => None,
                },
                |row, id| row.debug_selector(move || id.into()),
            )
            .on_a11y_action(AccessibleAction::Click, move |_, w, cx| {
                let _ = owner.update(cx, |m, cx| match choice {
                    Choice::Run(cmd) => m.finish(Some(cmd), Modifiers::default(), w, cx),
                    Choice::Submenu(d) => {
                        m.selected = index;
                        m.sub = Some((d, 0));
                        cx.notify();
                    }
                });
            })
            .when(index == self.selected, |row| row.bg(rgb(t.ui.bg_hover)))
            .when(r.active, |row| {
                row.text_color(rgb(t.ui.accent))
                    .font_weight(FontWeight::MEDIUM)
            })
            .when(matches!(choice, Choice::Submenu(_)), |row| {
                row.child(
                    svg()
                        .path("lucide/chevron-right.svg")
                        .size(px(t.metrics.ui_menu_indicator_size)),
                )
            })
            .on_hover(cx.listener(move |m, inside: &bool, _, cx| {
                if *inside {
                    let sub = match choice {
                        Choice::Submenu(d) => Some((d, 0)),
                        _ => None,
                    };
                    if m.selected != index || m.sub.map(|s| s.0) != sub.map(|s| s.0) {
                        m.selected = index;
                        m.sub = sub;
                        m.sub_scroll.scroll_to_item(0);
                        cx.notify();
                    }
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |m, e: &MouseDownEvent, w, cx| {
                    match choice {
                        Choice::Run(cmd) => m.finish(Some(cmd), e.modifiers, w, cx),
                        Choice::Submenu(d) => {
                            m.selected = index;
                            m.sub = Some((d, 0));
                            cx.notify();
                        }
                    }
                    cx.stop_propagation();
                }),
            );
            menu = menu.child(
                div()
                    .when(r.separator, |d| d.child(context_menu_separator(&t)))
                    .child(row),
            );
            y += rh;
        }
        let main = oxideterm_gpui_ui::modal::overlay_content_boundary(context_menu_event_boundary(
            div().child(menu),
        ));
        let mut layer = div().child(
            anchored()
                .anchor(Corner::TopLeft)
                .position_mode(AnchoredPositionMode::Window)
                .position(point(px(pos.x), px(pos.y)))
                .child(oxideterm_gpui_ui::motion::popover_zoom_enter(
                    &t,
                    ("tab-context-menu-motion", self.tab.0),
                    div().child(main),
                    -4.,
                    point(0., 0.),
                )),
        );
        if let Some((dir, offset)) = submenu {
            let width2 = oxideterm_gpui_ui::estimate_menu_width(
                &t,
                168.,
                300.,
                false,
                opts.merge_targets.iter().map(|(_, s)| s.as_str()),
            )
            .min((vw - 2. * margin).max(1.));
            let height2 =
                (opts.merge_targets.len() as f32 * rh + 2. * t.metrics.ui_menu_padding).min(max_h);
            let p = place_tab_context_submenu(pos, width, offset, width2, height2, vw, vh, margin);
            let sub = context_menu_sub_content(&t)
                .w(px(width2))
                .id("tab-context-merge-submenu-content")
                .max_h(px(max_h))
                .overflow_y_scroll()
                .track_scroll(&self.sub_scroll)
                .children(opts.merge_targets.into_iter().enumerate().map(
                    |(index, (target, title))| {
                        let owner = cx.weak_entity();
                        context_menu_item(
                            &t,
                            title.clone(),
                            ContextMenuItemKind::Plain,
                            false,
                            false,
                        )
                        .id(("tab-merge-target", target.0))
                        .role(Role::MenuItem)
                        .aria_label(title)
                        .when(self.sub == Some((dir, index)), |r| r.bg(rgb(t.ui.bg_hover)))
                        .on_a11y_action(AccessibleAction::Click, move |_, w, cx| {
                            let _ = owner.update(cx, |m, cx| {
                                m.finish(
                                    Some(Command::Merge(target, dir)),
                                    Modifiers::default(),
                                    w,
                                    cx,
                                )
                            });
                        })
                        .on_hover(cx.listener(move |m, over: &bool, _, cx| {
                            if *over && m.sub != Some((dir, index)) {
                                m.sub = Some((dir, index));
                                cx.notify();
                            }
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |m, e: &MouseDownEvent, w, cx| {
                                m.finish(Some(Command::Merge(target, dir)), e.modifiers, w, cx);
                                cx.stop_propagation();
                            }),
                        )
                    },
                ));
            layer = layer.child(
                anchored()
                    .anchor(Corner::TopLeft)
                    .position_mode(AnchoredPositionMode::Window)
                    .position(point(px(p.x), px(p.y)))
                    .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                        context_menu_event_boundary(div().child(sub)),
                    )),
            );
        }
        context_menu_backdrop()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|m, e: &MouseDownEvent, w, cx| {
                    m.finish(None, e.modifiers, w, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|m, e: &MouseDownEvent, w, cx| {
                    m.finish(None, e.modifiers, w, cx);
                    cx.stop_propagation();
                }),
            )
            .child(deferred(layer).with_priority(TAURI_POPOVER_LAYER_PRIORITY))
            .into_any_element()
    }
}
