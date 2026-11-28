use super::*;
use gpui::Div;
#[cfg(not(target_family = "wasm"))]
use oxideterm_gpui_ui::text_input::text_input_with_viewport;
use oxideterm_gpui_ui::text_input::{TextInputAnchorProbe, text_input_anchor_probe};

pub(super) mod browser_behavior {
    pub(in crate::view) use oxideterm_gpui_ui::confirm::{
        ModalFooterInputKeyAction, ModalFooterKeyAction, modal_footer_input_key_action,
        modal_footer_key_action,
    };
    pub(in crate::view) use oxideterm_gpui_ui::context_menu::{
        clamp_context_menu_position, preserve_or_move_context_selection,
    };
}

pub(super) fn selectable_text_id(scope: &str, key: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    scope.hash(&mut hasher);
    key.hash(&mut hasher);
    hasher.finish()
}

pub(super) trait SelectableTextScrollExt:
    StatefulInteractiveElement + ScrollableElement + Sized
{
    fn selectable_overflow_y_scrollbar(self, handle: &ScrollHandle) -> Self {
        self.overflow_y_scroll()
            .track_scroll(handle)
            .vertical_scrollbar(handle)
    }
}
impl<T: StatefulInteractiveElement + ScrollableElement + Sized> SelectableTextScrollExt for T {}

pub(super) fn observe_window_close(
    window: &Window,
    cx: &App,
    on_close: impl FnOnce(gpui::WindowId, &mut App) + 'static,
) -> Subscription {
    let window_id = window.window_handle().window_id();
    let mut on_close = Some(on_close);
    cx.on_window_closed(move |cx, closed_id| {
        if closed_id == window_id
            && let Some(on_close) = on_close.take()
        {
            // The window is already gone, and its session may still be updating.
            // Deliver cleanup after that update without re-entering the dead handle.
            cx.defer(move |cx| on_close(closed_id, cx));
        }
    })
}
pub(super) const FILE_MANAGER_NAVIGATION_ID: &str = "file-manager-navigation";

#[derive(Clone, Copy)]
pub(super) enum SelectableTextRole {
    PlainDocument,
    NonSelectable,
}

impl FileManagerView {
    pub(super) fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    pub(super) fn render_loading_icon(
        &self,
        id: impl Into<ElementId>,
        size: f32,
        color: Rgba,
    ) -> AnyElement {
        oxideterm_gpui_ui::motion::animated_spinner(
            &self.tokens,
            id,
            svg()
                .path(LucideIcon::LoaderCircle.path())
                .size(px(size))
                .text_color(color),
        )
    }

    pub(super) fn sidebar_search_row(&self, background: Rgba) -> Div {
        div()
            .w_full()
            .min_w_0()
            .flex_none()
            .h(px(36.0))
            .px_3()
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.one))
            .border_b_1()
            .border_color(self.chrome_divider)
            .bg(background)
    }

    pub(super) fn localized_markdown_options(&self) -> MarkdownOptions {
        self.markdown.clone()
    }
    pub(super) fn markdown_mermaid_actions(
        &self,
        _: &mut Context<Self>,
    ) -> oxideterm_gpui_markdown::MarkdownCodeBlockActions {
        oxideterm_gpui_markdown::MarkdownCodeBlockActions {
            on_run: None,
            on_mermaid_zoom: self.bindings.mermaid_zoom.clone(),
        }
    }

    pub(super) fn render_path_completion_overlay(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        #[cfg(target_family = "wasm")]
        let bounds = self.fields.path_anchor.get()?.bounds;
        #[cfg(not(target_family = "wasm"))]
        let bounds = self.ime[FileManagerInput::Path.index()].anchor?.bounds;
        let state = &self.file_manager.read(cx).path_completion;
        if !state.is_visible() {
            return None;
        }
        let view = cx.weak_entity();
        Some(crate::render_completion_popup(
            "file-manager-path-completion",
            &self.tokens,
            bounds,
            state.selected_index(),
            state.scroll_handle.clone(),
            state.suggestions(),
            move |index, cx| {
                let _ = view.update(cx, |view, cx| {
                    view.accept_file_manager_path_completion(index, cx)
                });
            },
        ))
    }

    pub(super) fn ime_selected_range_for_target(
        &self,
        input: FileManagerInput,
        _: &App,
    ) -> Option<std::ops::Range<usize>> {
        self.ime[input.index()].selection.clone()
    }
    pub(super) fn marked_text_for_target(&self, input: FileManagerInput, _: &App) -> Option<&str> {
        self.ime[input.index()].marked.as_deref()
    }

    pub(super) fn workspace_text_input(
        &self,
        input: FileManagerInput,
        view: TextInputView<'_>,
        _cx: &mut App,
    ) -> Div {
        #[cfg(target_family = "wasm")]
        {
            let field = &self.fields.views[&input];
            field.update(_cx, |f, cx| {
                f.sync_value(view.value, cx);
                f.environment(self.tokens, view.placeholder, cx);
            });
            div().w_full().h_full().min_w_0().child(field.clone())
        }
        #[cfg(not(target_family = "wasm"))]
        {
            let ime = &self.ime[input.index()];
            text_input_with_viewport(&self.tokens, view, &ime.viewport, ime.active_offset)
        }
    }

    pub(super) fn text_input_with_workspace_ime(
        &self,
        input: FileManagerInput,
        control: Div,
        prepare: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &Context<Self>,
    ) -> TextInputAnchorProbe {
        #[cfg(target_family = "wasm")]
        {
            let owner = cx.weak_entity();
            let anchor = self.fields.path_anchor.clone();
            return text_input_anchor_probe(
                input.anchor_id(),
                control.capture_any_mouse_down(move |event, _, cx| {
                    if event.button == MouseButton::Left {
                        let _ = owner.update(cx, |this, cx| prepare(this, cx));
                    }
                }),
                move |data, _, _| {
                    if input == FileManagerInput::Path {
                        anchor.set(Some(data));
                    }
                },
            );
        }
        #[cfg(not(target_family = "wasm"))]
        {
            let anchor = self.bindings.anchor.clone();
            text_input_anchor_probe(
                input.anchor_id(),
                control
                    .cursor(gpui::CursorStyle::IBeam)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            prepare(this, cx);
                            this.pointer(
                                FileManagerPointer::TextDown(input, event.clone()),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                        this.pointer(FileManagerPointer::TextMove(event.clone()), window, cx);
                    })),
                move |data, _, _| anchor(data),
            )
        }
    }

    pub(super) fn render_display_text_with_role(
        &self,
        role: SelectableTextRole,
        scope: &str,
        key: impl Hash,
        text: impl Into<String>,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match role {
            SelectableTextRole::PlainDocument => {
                self.render_selectable_display_text(scope, key, text, color, cx)
            }
            SelectableTextRole::NonSelectable => div()
                .min_w_0()
                .font_family(self.ui_font_family.clone())
                .text_color(rgb(color))
                .child(text.into())
                .into_any_element(),
        }
    }

    pub(super) fn render_selectable_text_scoped(
        &self,
        scope: &str,
        key: impl Hash,
        text: impl Into<SharedString>,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = text.into();
        (self.bindings.selectable)(
            FileManagerSelectableText {
                id: selectable_text_id(scope, key),
                text,
                color,
                row: None,
            },
            cx,
        )
    }
    pub(super) fn render_selectable_display_text(
        &self,
        scope: &str,
        key: impl Hash,
        text: impl Into<String>,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = text.into();
        self.render_selectable_text_scoped(scope, (key, text.as_str()), text.clone(), color, cx)
    }
    pub(super) fn render_row_safe_selectable_display_text_in_group(
        &self,
        group: u64,
        scope: &str,
        key: impl Hash,
        order: usize,
        text: impl Into<String>,
        color: u32,
        font: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        (self.bindings.selectable)(
            FileManagerSelectableText {
                id: selectable_text_id(scope, key),
                text: text.into().into(),
                color,
                row: Some((group, order, font)),
            },
            cx,
        )
    }

    pub(super) fn queue_workspace_tooltip(
        &mut self,
        id: impl Into<String>,
        label: String,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.request(
            FileManagerRequest::Tooltip {
                id: id.into(),
                label,
                x,
                y,
            },
            cx,
        );
    }
    pub(super) fn clear_workspace_tooltip(&self, id: &str, cx: &mut Context<Self>) {
        self.request(FileManagerRequest::ClearTooltip(id.to_owned()), cx);
    }

    pub(super) fn render_vertical_selection_surface(
        &self,
        id: &'static str,
        index: usize,
        surface: Div,
    ) -> AnyElement {
        self.segmented_control_user_motion
            .render_vertical(id, index, &self.tokens, surface)
    }

    pub(super) fn begin_user_segmented_control_transition_with_vertical_offset(
        &mut self,
        id: &'static str,
        index: usize,
        offset: Option<f32>,
        cx: &mut Context<Self>,
    ) {
        let Some(motion) = oxideterm_gpui_ui::segmented_control_motion(&self.tokens) else {
            self.segmented_control_user_motion.clear(id);
            return;
        };
        let generation = self
            .segmented_control_user_motion
            .begin_with_vertical_offset(id, index, offset);
        cx.spawn(async move |weak, cx| {
            cx.background_executor().timer(motion.duration).await;
            let _ = weak.update(cx, |this, cx| {
                if this.segmented_control_user_motion.finish(id, generation) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(in crate::view) fn workspace_tooltip_icon_button(
        &self,
        icon: LucideIcon,
        icon_size: f32,
        icon_color: Rgba,
        options: oxideterm_gpui_ui::button::IconButtonOptions,
        tooltip: String,
        element_id_prefix: &'static str,
        flex_none: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        workspace: gpui::Entity<Self>,
    ) -> AnyElement {
        let actionable = !(options.disabled || options.loading);
        let tooltip_for_move = tooltip.clone();
        let tooltip_element_id = tooltip.clone();
        let tooltip_request_id = tooltip.clone();
        let tooltip_workspace = workspace.clone();
        let clear_workspace = workspace;

        // FileManager, SFTP, and launcher toolbar buttons all map to Tauri
        // icon buttons with hover tooltips. Keep tooltip ownership and the
        // disabled/loading click guard in one helper so feature surfaces only
        // supply button metrics and the action body.
        oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, icon_size, icon_color),
            options,
        )
        .id((gpui::ElementId::from(element_id_prefix), tooltip_element_id))
        .on_mouse_move(move |event: &MouseMoveEvent, _window, cx| {
            let _ = tooltip_workspace.update(cx, |this, cx| {
                this.queue_workspace_tooltip(
                    tooltip_request_id.clone(),
                    tooltip_for_move.clone(),
                    f32::from(event.position.x) + 12.0,
                    f32::from(event.position.y) + 16.0,
                    cx,
                );
            });
        })
        .on_hover(move |hovered: &bool, _window, cx| {
            if !*hovered {
                let _ = clear_workspace.update(cx, |this, cx| {
                    this.clear_workspace_tooltip(&tooltip, cx);
                });
            }
        })
        .when(actionable, |button| {
            button.on_mouse_down(MouseButton::Left, listener)
        })
        .when(!actionable, |button| {
            // Disabled browser buttons do not activate their parent row. Stop the
            // click at the shared tooltip button so file/SFTP/sidebar toolbars do
            // not each need a local disabled-event patch.
            button.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
        })
        .when(flex_none, |button| button.flex_none())
        .into_any_element()
    }

    pub(in crate::view) fn workspace_icon_action_button(
        &self,
        icon: LucideIcon,
        icon_size: f32,
        icon_color: Rgba,
        options: oxideterm_gpui_ui::button::IconButtonOptions,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let actionable = !(options.disabled || options.loading);

        // Row-level icon buttons do not always have tooltips, but they still
        // share the browser button contract: disabled/loading buttons keep
        // their visual state and never dispatch pointer activation.
        oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, icon_size, icon_color),
            options,
        )
        .when(actionable, |button| {
            button.on_mouse_down(MouseButton::Left, cx.listener(listener))
        })
        .when(!actionable, |button| {
            // Match the DOM disabled button contract for inline row actions:
            // inert buttons consume the pointer instead of selecting/opening the row.
            button.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
        })
    }

    pub(in crate::view) fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: oxideterm_gpui_ui::button::ToolbarButtonOptions,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        oxideterm_gpui_ui::button::toolbar_action_button(
            &self.tokens,
            label,
            icon,
            options,
            listener,
        )
    }

    pub(in crate::view) fn workspace_confirm_footer_action_button(
        &self,
        label: String,
        variant: oxideterm_gpui_ui::button::ButtonVariant,
        action: ConfirmDialogAction,
        disabled: bool,
        focused_action: Option<ConfirmDialogAction>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.workspace_modal_footer_action_button(
            label,
            variant,
            action,
            disabled,
            focused_action,
            None,
            listener,
            cx,
        )
    }

    fn workspace_modal_footer_action_button<T>(
        &self,
        label: String,
        variant: oxideterm_gpui_ui::button::ButtonVariant,
        action: T,
        disabled: bool,
        focused_action: Option<T>,
        min_width: Option<f32>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div
    where
        T: std::marker::Copy + Eq,
    {
        // DialogFooter buttons across settings, AI, FileManager, and import/export
        // use the same shadcn Button contract: disabled buttons are inert, and the
        // focus ring only follows explicit keyboard-owned footer focus.
        self.workspace_toolbar_action_button(
            label,
            None,
            oxideterm_gpui_ui::button::ToolbarButtonOptions {
                button: oxideterm_gpui_ui::button::ButtonOptions {
                    variant,
                    size: oxideterm_gpui_ui::button::ButtonSize::Sm,
                    radius: oxideterm_gpui_ui::button::ButtonRadius::Md,
                    disabled,
                },
                min_width,
                focus_visible: focused_action == Some(action),
                ..oxideterm_gpui_ui::button::ToolbarButtonOptions::default()
            },
            cx.listener(listener),
        )
    }

    pub(in crate::view) fn workspace_context_menu_backdrop(
        &self,
        menu: impl gpui::IntoElement,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Radix context menus close for primary and secondary outside clicks.
        // Keep all native context-menu backdrops on the same dismissal path so
        // focus restoration and overlay arbitration do not diverge by feature.
        oxideterm_gpui_ui::context_menu::context_menu_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.dismiss_file_manager_context_menu(cx);
                    this.pointer(FileManagerPointer::DismissTransient, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _event, window, cx| {
                    this.dismiss_file_manager_context_menu(cx);
                    this.pointer(FileManagerPointer::DismissTransient, window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(menu)
    }

    pub(in crate::view) fn workspace_context_menu_styled_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        style: oxideterm_gpui_ui::context_menu::ContextMenuActionableStyle,
        close_menu: impl Fn(&mut Self) + 'static,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Closing Radix menu items share the same row hover/disabled/loading
        // state as persistent checkbox-style items, but run a close callback
        // before the action. Keep that two-step behavior centralized so file,
        // SFTP, session, topology, and terminal menus do not compose guards manually.
        let item = oxideterm_gpui_ui::context_menu::context_menu_actionable_row(
            item, disabled, loading, style,
        );
        oxideterm_gpui_ui::context_menu::context_menu_action(
            item,
            disabled,
            loading,
            cx.listener(move |this, event, window, cx| {
                close_menu(this);
                listener(this, event, window, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        )
    }
}
