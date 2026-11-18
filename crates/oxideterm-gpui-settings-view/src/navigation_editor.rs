use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, CursorStyle, Div, ElementId, Entity, MouseButton, MouseDownEvent,
    Pixels, Point, Render, Rgba, Window, div, prelude::*, px, relative, rgb, rgba,
};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions, icon_button_with_tooltip, toolbar_button,
    },
    confirm::ConfirmDialogAction,
    modal::{
        dialog_content, dialog_description, dialog_footer, dialog_header, dialog_title,
        dismissible_dialog_backdrop, overlay_content_boundary,
    },
};
use oxideterm_i18n::I18n;
use oxideterm_settings_model::SettingsNavigationLayout;
use oxideterm_theme::ThemeTokens;

use crate::{
    SettingsNavigationDraftAction, SettingsTab,
    entity::SettingsWorkspaceEntity,
    surface::nav::{render_icon, tab_icon},
};

const SETTINGS_NAVIGATION_EDITOR_WIDTH: f32 = 660.0;
const SETTINGS_NAVIGATION_EDITOR_HEIGHT: f32 = 720.0;
const SETTINGS_NAVIGATION_ROW_HEIGHT: f32 = 46.0;

#[derive(Clone, Copy, Debug)]
enum SettingsNavigationDragKind {
    Page(SettingsTab),
    Group(usize),
}

#[derive(Clone)]
struct SettingsNavigationDrag {
    kind: SettingsNavigationDragKind,
    label: String,
    position: Point<Pixels>,
    background: Rgba,
    border: Rgba,
    text: Rgba,
}

impl SettingsNavigationDrag {
    fn with_position(&self, position: Point<Pixels>) -> Self {
        let mut preview = self.clone();
        preview.position = position;
        preview
    }
}

impl Render for SettingsNavigationDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Keep the preview compact so the destination group remains visible.
        div().pl(self.position.x).pt(self.position.y).child(
            div()
                .max_w(px(300.0))
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(self.border)
                .bg(self.background)
                .text_color(self.text)
                .text_size(px(13.0))
                .shadow_lg()
                .child(self.label.clone()),
        )
    }
}

pub struct SettingsNavigationEditorFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub panel_background: Rgba,
    pub focused_action: Option<ConfirmDialogAction>,
}

pub struct SettingsNavigationEditor {
    settings_workspace: Entity<SettingsWorkspaceEntity>,
    frame: Rc<dyn Fn(&App) -> Option<SettingsNavigationEditorFrame>>,
    finish: Rc<dyn Fn(bool, &mut App)>,
}

impl SettingsNavigationEditor {
    pub fn new(
        settings_workspace: Entity<SettingsWorkspaceEntity>,
        frame: Rc<dyn Fn(&App) -> Option<SettingsNavigationEditorFrame>>,
        finish: Rc<dyn Fn(bool, &mut App)>,
    ) -> Self {
        Self {
            settings_workspace,
            frame,
            finish,
        }
    }
}

impl SettingsNavigationEditor {
    fn apply_draft_action(
        &mut self,
        action: SettingsNavigationDraftAction,
        cx: &mut Context<Self>,
    ) {
        if self.settings_workspace.update(cx, |settings, cx| {
            settings.apply_navigation_draft_action(action, cx)
        }) {
            cx.notify();
        }
    }
}

struct NavigationEditorBody<'a> {
    settings_workspace: &'a Entity<SettingsWorkspaceEntity>,
    frame: SettingsNavigationEditorFrame,
}

impl std::ops::Deref for NavigationEditorBody<'_> {
    type Target = SettingsNavigationEditorFrame;

    fn deref(&self) -> &Self::Target {
        &self.frame
    }
}

impl NavigationEditorBody<'_> {
    fn restore_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let disabled = options.button.disabled;
        toolbar_button(&self.tokens, label, icon, options).on_mouse_down(
            MouseButton::Left,
            move |event, window, cx| {
                if disabled {
                    cx.stop_propagation();
                } else {
                    listener(event, window, cx);
                }
            },
        )
    }

    fn footer_button(
        &self,
        label: String,
        variant: ButtonVariant,
        action: ConfirmDialogAction,
        cx: &mut Context<SettingsNavigationEditor>,
    ) -> Div {
        toolbar_button(
            &self.tokens,
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                focus_visible: self.focused_action == Some(action),
                ..ToolbarButtonOptions::default()
            },
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                (this.finish)(action == ConfirmDialogAction::Confirm, cx);
                cx.stop_propagation();
            }),
        )
    }

    fn settings_navigation_editor_group_label(&self, group_number: usize) -> String {
        self.i18n
            .t("settings_view.navigation_editor.group")
            .replace("{{number}}", &group_number.to_string())
    }

    fn settings_navigation_drag(
        &self,
        kind: SettingsNavigationDragKind,
        label: String,
    ) -> SettingsNavigationDrag {
        SettingsNavigationDrag {
            kind,
            label,
            position: Point::default(),
            background: rgb(self.tokens.ui.bg_panel),
            border: rgb(self.tokens.ui.accent),
            text: rgb(self.tokens.ui.text),
        }
    }

    fn settings_navigation_drop_action(
        drag: &SettingsNavigationDrag,
        group_index: usize,
        at_start: bool,
    ) -> SettingsNavigationDraftAction {
        match drag.kind {
            SettingsNavigationDragKind::Page(tab) if at_start => {
                SettingsNavigationDraftAction::MoveTabToGroupStart { tab, group_index }
            }
            SettingsNavigationDragKind::Page(tab) => {
                SettingsNavigationDraftAction::MoveTabToGroupEnd { tab, group_index }
            }
            SettingsNavigationDragKind::Group(source_index) => {
                SettingsNavigationDraftAction::MoveGroupToPosition {
                    source_index,
                    target_index: group_index,
                }
            }
        }
    }

    fn render_settings_navigation_drag_handle(
        &self,
        drag: SettingsNavigationDrag,
        element_id: impl Into<ElementId>,
    ) -> AnyElement {
        let grip_color = rgba((self.tokens.ui.text_muted << 8) | 0xb8);
        div()
            .id(element_id)
            .size(px(30.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .cursor(CursorStyle::OpenHand)
            .hover(|handle| handle.bg(rgba(0xffffff14)))
            .child(
                // A six-dot grip communicates direct manipulation more clearly
                // than a menu icon while remaining independent of icon assets.
                div().w(px(6.0)).flex().flex_wrap().gap(px(2.0)).children(
                    (0..6).map(move |_| div().size(px(2.0)).rounded(px(1.0)).bg(grip_color)),
                ),
            )
            .on_drag(drag, |drag, position, _window, cx| {
                let preview = drag.with_position(position);
                cx.new(|_| preview)
            })
            .into_any_element()
    }

    fn render_settings_navigation_page_row(
        &self,
        tab: SettingsTab,
        shows_bottom_divider: bool,
        cx: &mut Context<SettingsNavigationEditor>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let page_label = self.i18n.t(tab.label_key());
        let drag = self
            .settings_navigation_drag(SettingsNavigationDragKind::Page(tab), page_label.clone());

        div()
            .id(format!("settings-navigation-page-row-{}", tab.id()))
            .h(px(SETTINGS_NAVIGATION_ROW_HEIGHT))
            .flex_none()
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(9.0))
            .when(shows_bottom_divider, |row| {
                row.border_b_1().border_color(rgb(theme.border))
            })
            .hover(move |row| row.bg(rgba((theme.bg_hover << 8) | 0x80)))
            .drag_over::<SettingsNavigationDrag>(move |row, drag, _window, _cx| {
                if matches!(drag.kind, SettingsNavigationDragKind::Page(_)) {
                    row.bg(rgba((theme.accent << 8) | 0x12))
                } else {
                    row
                }
            })
            .can_drop(|drag, _window, _cx| {
                drag.downcast_ref::<SettingsNavigationDrag>()
                    .is_some_and(|drag| matches!(drag.kind, SettingsNavigationDragKind::Page(_)))
            })
            .on_drop(
                cx.listener(move |this, drag: &SettingsNavigationDrag, _window, cx| {
                    if let SettingsNavigationDragKind::Page(source_tab) = drag.kind {
                        this.apply_draft_action(
                            SettingsNavigationDraftAction::MoveTabBefore {
                                tab: source_tab,
                                target: tab,
                            },
                            cx,
                        );
                    }
                    cx.stop_propagation();
                }),
            )
            .child(self.render_settings_navigation_drag_handle(
                drag,
                format!("settings-navigation-page-drag-{}", tab.id()),
            ))
            .child(
                div()
                    .flex_none()
                    .child(render_icon(tab_icon(tab), 17.0, rgb(theme.accent))),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(page_label),
            )
            .into_any_element()
    }

    fn render_settings_navigation_group(
        &self,
        layout: &SettingsNavigationLayout,
        group_index: usize,
        cx: &mut Context<SettingsNavigationEditor>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let group = &layout.groups()[group_index];
        let group_label = self.settings_navigation_editor_group_label(group_index + 1);
        let group_drag = self.settings_navigation_drag(
            SettingsNavigationDragKind::Group(group_index),
            group_label.clone(),
        );
        let group_is_empty = group.is_empty();
        let group_header_radius = self.tokens.radii.sm;

        let mut page_rows = div()
            .id(("settings-navigation-group-pages", group_index))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(self.panel_background)
            .drag_over::<SettingsNavigationDrag>(move |body, drag, _window, _cx| {
                if matches!(drag.kind, SettingsNavigationDragKind::Page(_)) {
                    body.bg(rgba((theme.accent << 8) | 0x0d))
                } else {
                    body
                }
            })
            .can_drop(|drag, _window, _cx| {
                drag.downcast_ref::<SettingsNavigationDrag>()
                    .is_some_and(|drag| matches!(drag.kind, SettingsNavigationDragKind::Page(_)))
            })
            .on_drop(
                cx.listener(move |this, drag: &SettingsNavigationDrag, _window, cx| {
                    let action = Self::settings_navigation_drop_action(drag, group_index, false);
                    this.apply_draft_action(action, cx);
                    cx.stop_propagation();
                }),
            );

        if group_is_empty {
            page_rows = page_rows.child(
                div()
                    .h(px(54.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("settings_view.navigation_editor.empty_group")),
            );
        } else {
            for (page_index, tab) in group.iter().enumerate() {
                let shows_bottom_divider = page_index + 1 < group.len();
                page_rows = page_rows.child(self.render_settings_navigation_page_row(
                    *tab,
                    shows_bottom_divider,
                    cx,
                ));
            }
        }

        let mut header_actions = div();
        if group_is_empty && layout.group_count() > 1 {
            let remove_tooltip = format!(
                "{} — {}",
                self.i18n.t("settings_view.navigation_editor.remove_group"),
                group_label
            );
            header_actions = header_actions.child(icon_button_with_tooltip(
                &self.tokens,
                "lucide/trash-2.svg",
                14.0,
                rgb(theme.text_muted),
                IconButtonOptions {
                    hover_background: Some(rgb(theme.bg_hover)),
                    ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
                },
                remove_tooltip,
                "settings-navigation-remove-group",
                true,
                cx.listener(move |this, _event, _window, cx| {
                    this.apply_draft_action(
                        SettingsNavigationDraftAction::RemoveEmptyGroup { group_index },
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ));
        }

        // The group header stays flat so the shared page list is the only
        // visual surface at this hierarchy level.
        div()
            .id(("settings-navigation-group", group_index))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .id(("settings-navigation-group-header", group_index))
                    .h(px(36.0))
                    .px(px(4.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .drag_over::<SettingsNavigationDrag>(move |header, _drag, _window, _cx| {
                        header
                            .rounded(px(group_header_radius))
                            .bg(rgba((theme.accent << 8) | 0x16))
                    })
                    .can_drop(|drag, _window, _cx| drag.is::<SettingsNavigationDrag>())
                    .on_drop(cx.listener(
                        move |this, drag: &SettingsNavigationDrag, _window, cx| {
                            let action =
                                Self::settings_navigation_drop_action(drag, group_index, true);
                            this.apply_draft_action(action, cx);
                            cx.stop_propagation();
                        },
                    ))
                    .child(self.render_settings_navigation_drag_handle(
                        group_drag,
                        ("settings-navigation-group-drag", group_index),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_heading))
                            .child(group_label),
                    )
                    .child(header_actions),
            )
            .child(page_rows)
            .into_any_element()
    }

    fn render_dialog(&self, cx: &mut Context<SettingsNavigationEditor>) -> Option<AnyElement> {
        let layout = self
            .settings_workspace
            .read(cx)
            .navigation_draft_snapshot()?;
        let backdrop = dismissible_dialog_backdrop().on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                (this.finish)(false, cx);
                cx.stop_propagation();
            }),
        );

        let mut groups = div().flex().flex_col().gap(px(12.0));
        for group_index in 0..layout.group_count() {
            groups = groups.child(self.render_settings_navigation_group(&layout, group_index, cx));
        }
        let theme = self.tokens.ui;
        groups = groups.child(
            div()
                .id("settings-navigation-add-group")
                .h(px(44.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .rounded(px(self.tokens.radii.lg))
                .border_1()
                .border_dashed()
                .border_color(rgb(theme.border))
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(rgb(theme.text_muted))
                .cursor_pointer()
                .hover(move |button| {
                    button
                        .border_color(rgb(theme.accent))
                        .bg(rgba((theme.accent << 8) | 0x0d))
                })
                .drag_over::<SettingsNavigationDrag>(move |button, _drag, _window, _cx| {
                    button
                        .border_color(rgb(theme.accent))
                        .bg(rgba((theme.accent << 8) | 0x16))
                })
                .can_drop(|drag, _window, _cx| drag.is::<SettingsNavigationDrag>())
                .on_drop(
                    cx.listener(|this, drag: &SettingsNavigationDrag, _window, cx| {
                        let action = match drag.kind {
                            SettingsNavigationDragKind::Page(tab) => {
                                SettingsNavigationDraftAction::AddGroupWithTab(tab)
                            }
                            SettingsNavigationDragKind::Group(source_index) => {
                                SettingsNavigationDraftAction::MoveGroupToEnd { source_index }
                            }
                        };
                        this.apply_draft_action(action, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.apply_draft_action(SettingsNavigationDraftAction::AddGroup, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(render_icon("lucide/plus.svg", 15.0, rgb(theme.text_muted)))
                .child(self.i18n.t("settings_view.navigation_editor.add_group")),
        );

        let form = overlay_content_boundary(
            dialog_content(&self.tokens)
                .w(px(SETTINGS_NAVIGATION_EDITOR_WIDTH))
                .max_w(relative(0.94))
                .h(px(SETTINGS_NAVIGATION_EDITOR_HEIGHT))
                .max_h(relative(0.90))
                .flex()
                .flex_col()
                .shadow_lg()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(
                    dialog_header(&self.tokens)
                        .child(dialog_title(
                            &self.tokens,
                            self.i18n.t("settings_view.navigation_editor.title"),
                        ))
                        .child(dialog_description(
                            &self.tokens,
                            self.i18n.t("settings_view.navigation_editor.description"),
                        )),
                )
                .child(
                    div()
                        .id("settings-navigation-editor-scroll")
                        .flex_1()
                        .min_h(px(0.0))
                        .overflow_y_scroll()
                        .px(px(20.0))
                        .py(px(16.0))
                        .child(groups),
                )
                .child(
                    dialog_footer(&self.tokens)
                        .child(
                            self.restore_button(
                                self.i18n
                                    .t("settings_view.navigation_editor.restore_default"),
                                Some(render_icon(
                                    "lucide/rotate-ccw.svg",
                                    14.0,
                                    rgb(self.tokens.ui.text_muted),
                                )),
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Secondary,
                                        size: ButtonSize::Default,
                                        radius: ButtonRadius::Md,
                                        disabled: layout.is_default(),
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(|this, _event, _window, cx| {
                                    this.apply_draft_action(
                                        SettingsNavigationDraftAction::RestoreDefault,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            ),
                        )
                        .child(div().flex_1())
                        .child(self.footer_button(
                            self.i18n.t("common.actions.cancel"),
                            ButtonVariant::Outline,
                            ConfirmDialogAction::Cancel,
                            cx,
                        ))
                        .child(self.footer_button(
                            self.i18n.t("settings_view.navigation_editor.save"),
                            ButtonVariant::Default,
                            ConfirmDialogAction::Confirm,
                            cx,
                        )),
                ),
        );

        Some(backdrop.child(form).into_any_element())
    }
}

impl Render for SettingsNavigationEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        (self.frame)(cx)
            .and_then(|frame| {
                NavigationEditorBody {
                    settings_workspace: &self.settings_workspace,
                    frame,
                }
                .render_dialog(cx)
            })
            .unwrap_or_else(|| div().into_any_element())
    }
}
