use super::navigation::TAB_DRAG_THRESHOLD_PX;
use super::*;

use oxideterm_gpui_ui::button::ButtonVariant;
use oxideterm_gpui_ui::context_menu::{
    ContextMenuItemKind, context_menu_content, context_menu_event_boundary, context_menu_item,
    context_menu_separator,
};
use oxideterm_gpui_ui::modal::overlay_content_boundary;

const TAB_CONTEXT_MENU_WIDTH: f32 = 228.0;
const TAB_CONTEXT_MENU_HEIGHT: f32 = 136.0;
const TAB_CONTEXT_MENU_RENAME_HEIGHT: f32 = 168.0;
const TAB_CONTEXT_MENU_MARGIN: f32 = 8.0;
const TAB_RENAME_DIALOG_WIDTH: f32 = 420.0;
const TAB_HANDOFF_PREVIEW_WIDTH_EXTRA: f32 = 96.0;
const TAB_HANDOFF_PREVIEW_MIN_WIDTH: f32 = 220.0;
const TAB_HANDOFF_PREVIEW_MAX_WIDTH: f32 = 360.0;
const TAB_HANDOFF_PREVIEW_HEIGHT: f32 = 48.0;
const TAB_HANDOFF_VIEWPORT_MARGIN: f32 = 8.0;
const TAB_HANDOFF_POINTER_OFFSET_Y: f32 = 14.0;
const TAB_HANDOFF_CORNER_RADIUS: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct TabWindowHandoffRect {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

fn tab_handoff_preview_width(tab_width: f32) -> f32 {
    (tab_width + TAB_HANDOFF_PREVIEW_WIDTH_EXTRA)
        .clamp(TAB_HANDOFF_PREVIEW_MIN_WIDTH, TAB_HANDOFF_PREVIEW_MAX_WIDTH)
}

fn tab_window_handoff_rect(
    pointer_x: f32,
    pointer_y: f32,
    viewport_width: f32,
    viewport_height: Option<f32>,
    minimum_top: f32,
    tab_width: f32,
) -> TabWindowHandoffRect {
    let width = tab_handoff_preview_width(tab_width);
    let left = (pointer_x - width * 0.5).clamp(
        TAB_HANDOFF_VIEWPORT_MARGIN,
        (viewport_width - width - TAB_HANDOFF_VIEWPORT_MARGIN).max(TAB_HANDOFF_VIEWPORT_MARGIN),
    );
    let unclamped_top = (pointer_y + TAB_HANDOFF_POINTER_OFFSET_Y).max(minimum_top);
    let top = viewport_height.map_or(unclamped_top, |height| {
        unclamped_top.clamp(
            TAB_HANDOFF_VIEWPORT_MARGIN,
            (height - TAB_HANDOFF_PREVIEW_HEIGHT - TAB_HANDOFF_VIEWPORT_MARGIN)
                .max(TAB_HANDOFF_VIEWPORT_MARGIN),
        )
    });
    TabWindowHandoffRect {
        left,
        top,
        width,
        height: TAB_HANDOFF_PREVIEW_HEIGHT,
    }
}

fn interpolate_tab_window_handoff_rect(
    origin: TabWindowHandoffRect,
    target: TabWindowHandoffRect,
    progress: f32,
) -> TabWindowHandoffRect {
    TabWindowHandoffRect {
        left: oxideterm_gpui_ui::motion::lerp(origin.left, target.left, progress),
        top: oxideterm_gpui_ui::motion::lerp(origin.top, target.top, progress),
        width: oxideterm_gpui_ui::motion::lerp(origin.width, target.width, progress),
        height: oxideterm_gpui_ui::motion::lerp(origin.height, target.height, progress),
    }
}

fn tab_return_visible_insertion_index(pointer_x: f32, tab_widths: &[f32]) -> usize {
    let mut tab_left = 0.0;
    for (visible_index, width) in tab_widths.iter().copied().enumerate() {
        if pointer_x < tab_left + width * 0.5 {
            return visible_index;
        }
        tab_left += width;
    }
    tab_widths.len()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DetachedTabSurfaceRoute {
    Settings,
    Ide(TabId),
    Sftp(TabId),
    Forwards(TabId),
    Other,
}

fn detached_tab_surface_route(tab_id: TabId, kind: &TabKind) -> DetachedTabSurfaceRoute {
    // Stateful shared surfaces must resolve the detached tab explicitly instead
    // of consulting the main window's active-tab slot.
    match kind {
        TabKind::Settings => DetachedTabSurfaceRoute::Settings,
        TabKind::Ide => DetachedTabSurfaceRoute::Ide(tab_id),
        TabKind::Sftp => DetachedTabSurfaceRoute::Sftp(tab_id),
        TabKind::Forwards => DetachedTabSurfaceRoute::Forwards(tab_id),
        _ => DetachedTabSurfaceRoute::Other,
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn update_main_window_tabbar_drop_bounds(
        &mut self,
        window: &Window,
        titlebar_visible: bool,
        zen_mode: bool,
        cx: &App,
    ) {
        if zen_mode {
            self.main_window_tabbar_drop_bounds = None;
            return;
        }

        let window_bounds = window.bounds();
        let titlebar_height = if titlebar_visible {
            self.tokens.metrics.titlebar_height
        } else {
            0.0
        };
        let left_offset = if self.sidebar_collapsed {
            self.tokens.metrics.activity_bar_width
        } else {
            self.sidebar_width
        };
        let right_offset = if self.context_sidebar_visible() {
            self.ai_entity.read(cx).chat_ui().sidebar_width
        } else {
            0.0
        };
        let width = (f32::from(window_bounds.size.width) - left_offset - right_offset).max(0.0);
        self.main_window_tabbar_drop_bounds = Some(Bounds::new(
            gpui::point(
                window_bounds.origin.x + px(left_offset),
                window_bounds.origin.y + px(titlebar_height),
            ),
            gpui::size(px(width), px(self.tokens.metrics.tabbar_height)),
        ));
    }

    pub(in crate::workspace) fn open_tab_context_menu(
        &mut self,
        tab_id: TabId,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        self.main_window_tabs.context_menu = Some(TabContextMenu {
            tab_id,
            x: f32::from(event.position.x),
            y: f32::from(event.position.y),
        });
        cx.notify();
    }

    pub(in crate::workspace) fn close_tab_context_menu(&mut self) -> bool {
        self.main_window_tabs.context_menu.take().is_some()
    }

    pub(in crate::workspace) fn begin_tab_rename(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(title) = self.tab_by_id(tab_id, cx).and_then(|tab| {
            matches!(
                tab.kind,
                TabKind::LocalTerminal | TabKind::SshTerminal | TabKind::MoshTerminal
            )
            .then(|| tab.title.clone())
        }) else {
            return false;
        };
        let title_len = title.encode_utf16().count();
        // Keep the draft in window UI state; the canonical tab changes only on submit.
        self.tab_rename_dialog = Some(TabRenameDialog {
            tab_id,
            draft: title,
        });
        self.close_tab_context_menu();
        self.ime_marked_text = None;
        self.set_ime_selection_from_anchor(WorkspaceImeTarget::TabRename, 0, title_len);
        // Move key dispatch off the terminal pane before the platform text
        // owner receives printable input for the rename field.
        window.focus(&self.focus_handle, cx);
        self.show_active_input_caret(cx);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn cancel_tab_rename(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let closed = self.tab_rename_dialog.take().is_some();
        if closed {
            self.ime_marked_text = None;
            self.clear_ime_selection();
            // Restore the pane as the keyboard owner only after the blocking
            // rename input has released its platform text handler.
            self.focus_active_pane(window, cx);
            cx.notify();
        }
        closed
    }

    pub(in crate::workspace) fn submit_tab_rename(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(dialog) = self.tab_rename_dialog.as_ref() else {
            return false;
        };
        if dialog.draft.trim().is_empty() {
            return false;
        }
        let tab_id = dialog.tab_id;
        let draft = dialog.draft.clone();
        let renamed = self.tab_host.update(cx, |tab_host, _cx| {
            tab_host.rename_terminal_tab(tab_id, &draft)
        });
        // Renaming changes display metadata only; existing mounts and sessions stay live.
        // If an asynchronous tab close won the race, dismiss the stale dialog as well.
        self.tab_rename_dialog = None;
        self.ime_marked_text = None;
        self.clear_ime_selection();
        self.focus_active_pane(window, cx);
        cx.notify();
        renamed
    }

    pub(in crate::workspace) fn handle_tab_rename_dialog_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.tab_rename_dialog.is_none() || event.keystroke.modifiers.platform {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.cancel_tab_rename(window, cx),
            "enter" => {
                self.submit_tab_rename(window, cx);
                true
            }
            _ => false,
        }
    }

    pub(in crate::workspace) fn render_tab_rename_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.tab_rename_dialog.as_ref()?;
        let target = WorkspaceImeTarget::TabRename;
        let submit_disabled = dialog.draft.trim().is_empty();
        let input = text_input(
            &self.tokens,
            TextInputView {
                value: dialog.draft.as_str(),
                placeholder: self.i18n.t("tabbar.rename_tab_placeholder"),
                focused: true,
                caret_visible: self.input_caret.visible(),
                secret: false,
                selected_all: false,
                selected_range: self.ime_selected_range_for_target(target, cx),
                marked_text: self.marked_text_for_target(target, cx),
            },
        )
        .h(px(34.0));
        let input = self.text_input_with_workspace_ime(
            target,
            input,
            |this, cx| {
                this.show_active_input_caret(cx);
            },
            cx,
        );
        let cancel_action = self.workspace_confirm_footer_action_button(
            self.i18n.t("common.cancel"),
            ButtonVariant::Secondary,
            ConfirmDialogAction::Cancel,
            false,
            None,
            |this, _event, window, cx| {
                this.cancel_tab_rename(window, cx);
            },
            cx,
        );
        let rename_action = self.workspace_confirm_footer_action_button(
            self.i18n.t("tabbar.rename_tab_action"),
            ButtonVariant::Default,
            ConfirmDialogAction::Confirm,
            submit_disabled,
            None,
            |this, _event, window, cx| {
                this.submit_tab_rename(window, cx);
            },
            cx,
        );
        let theme = self.tokens.ui;
        let content = oxideterm_gpui_ui::modal::dialog_content(&self.tokens)
            .w(px(TAB_RENAME_DIALOG_WIDTH))
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("tabbar.rename_tab")),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("tabbar.rename_tab_description")),
                    ),
            )
            .child(div().px_4().py_4().child(input))
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .child(cancel_action)
                    .child(rename_action),
            );

        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        this.cancel_tab_rename(window, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(content))
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn detach_tab_to_window(
        &mut self,
        tab_id: TabId,
        entry_handoff_origin: Option<TabWindowHandoffOrigin>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_index) = self.tab_index_by_id(tab_id, cx) else {
            return;
        };
        let Some(tabs::TabDetachTransition {
            mount_id,
            selection,
        }) = self
            .tab_host
            .update(cx, |tab_host, _cx| tab_host.begin_detach_from_main(tab_id))
        else {
            return;
        };
        self.apply_main_window_active_tab_change(selection.previous, selection.current, cx);
        let window_registration =
            self.reserve_workspace_window(window_registry::WindowRole::Detached { tab_id });
        // Capture the source tab before it leaves the live strip. The snapshot
        // is committed only after native window creation succeeds.
        let exiting_visual = self.tab_exit_visual(tab_index, cx);

        self.sync_active_tab_surface(cx);
        self.focus_active_pane(window, cx);

        let session = cx.entity();
        let bounds = window.bounds();
        let entry_handoff_duration = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        let entry_handoff_origin = entry_handoff_origin.filter(|_| self.tokens.motion.enabled);
        // GPUI constructs and draws the detached window synchronously while
        // this Workspace update is active, so bootstrap it from scalar values.
        let background_cache_byte_limit = self.render_policy.image_cache_bytes;
        let open_result = cx.open_window(
            oxideterm_gpui_platform::window_options(bounds),
            move |detached_window, cx| {
                cx.new(|cx| {
                    super::detached_tab_window::DetachedTabWindow::new(
                        session,
                        tab_id,
                        mount_id,
                        window_registration,
                        entry_handoff_origin,
                        entry_handoff_duration,
                        background_cache_byte_limit,
                        detached_window,
                        cx,
                    )
                })
            },
        );

        match open_result {
            Ok(handle) => {
                let detached_window_handle = handle.into();
                let window_registered =
                    self.commit_workspace_window(window_registration, detached_window_handle, cx);
                if !window_registered {
                    let _ = detached_window_handle
                        .update(cx, |_root, window, _cx| window.remove_window());
                    if let Some(selection) = self.tab_host.update(cx, |tab_host, _cx| {
                        tab_host.rollback_detach_to_main(tab_id, mount_id)
                    }) {
                        self.apply_main_window_active_tab_change(
                            selection.previous,
                            selection.current,
                            cx,
                        );
                    }
                    self.sync_active_tab_surface(cx);
                    cx.notify();
                    return;
                }
                let committed = self.tab_host.update(cx, |tab_host, _cx| {
                    tab_host.commit_detach(tab_id, mount_id, detached_window_handle)
                });
                if !committed {
                    // A synchronous tab cleanup can invalidate the reservation
                    // while the native window is being constructed.
                    let _ = detached_window_handle
                        .update(cx, |_root, window, _cx| window.remove_window());
                    self.release_workspace_window(
                        window_registration,
                        detached_window_handle.window_id(),
                        cx,
                    );
                    self.sync_active_tab_surface(cx);
                    cx.notify();
                    return;
                }
                self.sync_ide_surface_mount(tab_id, cx);
                self.sync_host_tools_lifecycle(false, cx);
                self.bind_remote_desktop_window(tab_id, detached_window_handle, cx);
                self.resume_remote_desktop_frame_delivery(tab_id, cx);
                if let Some(exiting_visual) = exiting_visual {
                    self.begin_tab_visual_exit(exiting_visual, cx);
                }
            }
            Err(_) => {
                self.rollback_workspace_window(window_registration);
                if let Some(selection) = self.tab_host.update(cx, |tab_host, _cx| {
                    tab_host.rollback_detach_to_main(tab_id, mount_id)
                }) {
                    self.apply_main_window_active_tab_change(
                        selection.previous,
                        selection.current,
                        cx,
                    );
                }
            }
        }
        self.sync_active_tab_surface(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn tab_detach_handoff_origin(
        &self,
        drag: &TabDragState,
        window: &Window,
    ) -> Option<TabWindowHandoffOrigin> {
        if !drag.active || drag.mode != TabDragMode::Detach || !self.tokens.motion.enabled {
            return None;
        }
        let geometry = tab_window_handoff_rect(
            drag.current_x,
            drag.current_y,
            f32::from(window.viewport_size().width),
            None,
            self.window_titlebar_height(window)
                + self.tokens.metrics.tabbar_height
                + TAB_HANDOFF_VIEWPORT_MARGIN,
            drag.tab_widths
                .get(drag.from_index)
                .copied()
                .unwrap_or(self.tokens.metrics.tab_max_width),
        );
        let window_origin = window.bounds().origin;
        Some(TabWindowHandoffOrigin {
            screen_left: f32::from(window_origin.x) + geometry.left,
            screen_top: f32::from(window_origin.y) + geometry.top,
            width: geometry.width,
            height: geometry.height,
        })
    }

    pub(in crate::workspace) fn return_detached_tab_to_main(
        &mut self,
        tab_id: TabId,
        current_window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let transition = self.tab_host.update(cx, |tab_host, _cx| {
            tab_host.return_to_main_and_select(tab_id, tabs::TabMountCloseReason::ReturnToMain)
        });
        if let Some(transition) = transition {
            // Close the source shell directly; re-entering its handle from its own event fails.
            self.apply_tab_mount_cleanup(transition.cleanup, Some(current_window), cx);
            self.apply_main_window_active_tab_change(
                transition.selection.previous,
                transition.selection.current,
                cx,
            );
            self.detached_tab_return_drag = None;
            self.sync_active_tab_surface(cx);
            cx.notify();
        }
    }

    pub(in crate::workspace) fn release_detached_tab_window(
        &mut self,
        tab_id: TabId,
        mount_id: tabs::TabMountId,
        window_registration: window_registry::WindowRegistration,
        current_window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let window_id = current_window.window_handle().window_id();
        self.release_workspace_window(window_registration, window_id, cx);
        let transition = self.tab_host.update(cx, |tab_host, _cx| {
            tab_host.remove_tab_for_detached_window_release(tab_id, mount_id, window_id)
        });
        if let Some(transition) = transition {
            self.detached_tab_return_drag = None;
            self.finish_tab_removal(transition, None, current_window, cx);
        }
    }

    pub(in crate::workspace) fn focus_detached_tab_window(
        &self,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(handle) = self.tab_host.read(cx).detached_window_handle(tab_id) else {
            return false;
        };
        handle
            .update(cx, |_root, window, _cx| window.activate_window())
            .is_ok()
    }

    pub(in crate::workspace) fn apply_tab_mount_cleanup(
        &self,
        cleanup: tabs::TabMountCleanupPlan,
        current_window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if let Some(handle) = cleanup.detached_window {
            if let Some(window) = current_window
                && window.window_handle().window_id() == handle.window_id()
            {
                window.remove_window();
                return;
            }
            // Native window teardown is a shell concern. The mount was already
            // removed, so its release callback is stale by construction.
            let _ = handle.update(cx, |_root, window: &mut Window, _cx| window.remove_window());
        }
    }

    fn detached_window_screen_point(window: &Window, window_point: Point<Pixels>) -> Point<Pixels> {
        let window_bounds = window.bounds();
        gpui::point(
            window_bounds.origin.x + window_point.x,
            window_bounds.origin.y + window_point.y,
        )
    }

    fn detached_tab_return_handoff_origin(
        &self,
        screen_point: Point<Pixels>,
        window: &Window,
    ) -> TabWindowHandoffOrigin {
        let window_bounds = window.bounds();
        let geometry = tab_window_handoff_rect(
            f32::from(screen_point.x - window_bounds.origin.x),
            f32::from(screen_point.y - window_bounds.origin.y),
            f32::from(window.viewport_size().width),
            Some(f32::from(window.viewport_size().height)),
            TAB_HANDOFF_VIEWPORT_MARGIN,
            self.tokens.metrics.tab_max_width,
        );
        TabWindowHandoffOrigin {
            screen_left: f32::from(window_bounds.origin.x) + geometry.left,
            screen_top: f32::from(window_bounds.origin.y) + geometry.top,
            width: geometry.width,
            height: geometry.height,
        }
    }

    fn begin_detached_tab_return_handoff(
        &mut self,
        tab_id: TabId,
        origin: TabWindowHandoffOrigin,
        cx: &mut Context<Self>,
    ) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        if delay.is_zero() {
            self.detached_tab_return_handoff = None;
            return;
        }
        self.next_tab_window_handoff_generation =
            self.next_tab_window_handoff_generation.wrapping_add(1);
        let generation = self.next_tab_window_handoff_generation;
        self.detached_tab_return_handoff = Some(DetachedTabReturnHandoff {
            tab_id,
            origin,
            generation,
        });
        // The workspace owns at most one return relay. A generation check keeps
        // a stale cleanup task from removing a newer user-initiated handoff.
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |workspace, cx| {
                if workspace
                    .detached_tab_return_handoff
                    .is_some_and(|handoff| handoff.generation == generation)
                {
                    workspace.detached_tab_return_handoff = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn detached_tab_return_visible_index(&self, screen_x: f32, cx: &App) -> Option<usize> {
        let drop_bounds = self.main_window_tabbar_drop_bounds.as_ref()?;
        let scroll_x = f32::from(-self.main_window_tabs.scroll_handle.offset().x).max(0.0);
        let pointer_x = screen_x - f32::from(drop_bounds.origin.x) + scroll_x
            - self.tokens.metrics.tabbar_leading_offset;
        let outside_main_tabs = self.tab_host.read(cx).outside_main_tab_ids();
        let visible_widths = self
            .tabs(cx)
            .iter()
            .filter(|tab| !outside_main_tabs.contains(&tab.id))
            .map(|tab| self.tab_visual_width(tab))
            .collect::<Vec<_>>();
        Some(tab_return_visible_insertion_index(
            pointer_x,
            &visible_widths,
        ))
    }

    pub(in crate::workspace) fn start_detached_tab_return_drag(
        &mut self,
        tab_id: TabId,
        event: &MouseDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let screen_point = Self::detached_window_screen_point(window, event.position);
        self.detached_tab_return_drag = Some(DetachedTabReturnDrag {
            tab_id,
            start_screen_x: f32::from(screen_point.x),
            start_screen_y: f32::from(screen_point.y),
            current_screen_x: f32::from(screen_point.x),
            current_screen_y: f32::from(screen_point.y),
            active: false,
        });
        cx.notify();
    }

    pub(in crate::workspace) fn update_detached_tab_return_drag(
        &mut self,
        tab_id: TabId,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let screen_point = Self::detached_window_screen_point(window, event.position);
        let Some(mut drag) = self.detached_tab_return_drag else {
            return;
        };
        if drag.tab_id != tab_id {
            return;
        }

        let was_active = drag.active;
        let previous_placeholder = self.detached_tab_return_placeholder(cx);
        drag.current_screen_x = f32::from(screen_point.x);
        drag.current_screen_y = f32::from(screen_point.y);
        let delta_x = drag.current_screen_x - drag.start_screen_x;
        let delta_y = drag.current_screen_y - drag.start_screen_y;
        // Treat this as a tab-return gesture only after a real window drag,
        // so ordinary titlebar clicks do not accidentally dock the tab.
        drag.active = delta_x.hypot(delta_y) > TAB_DRAG_THRESHOLD_PX;
        self.detached_tab_return_drag = Some(drag);
        let next_placeholder = self.detached_tab_return_placeholder(cx);
        if drag.active != was_active || previous_placeholder != next_placeholder {
            // Repaint only when the pointer crosses an insertion midpoint or
            // enters/leaves the drop strip, not for every native window move.
            cx.notify();
        }
    }

    pub(in crate::workspace) fn finish_detached_tab_return_drag(
        &mut self,
        tab_id: TabId,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let screen_point = Self::detached_window_screen_point(window, event.position);
        let Some(drag) = self.detached_tab_return_drag.take() else {
            return false;
        };
        if drag.tab_id != tab_id || !drag.active {
            cx.notify();
            return false;
        }
        let should_return = self
            .main_window_tabbar_drop_bounds
            .as_ref()
            .is_some_and(|bounds| bounds.contains(&screen_point));
        if should_return {
            let handoff_origin = self.detached_tab_return_handoff_origin(screen_point, window);
            if let Some(visible_index) =
                self.detached_tab_return_visible_index(f32::from(screen_point.x), cx)
            {
                self.move_tab_to_visible_index(tab_id, visible_index, cx);
            }
            self.begin_detached_tab_return_handoff(tab_id, handoff_origin, cx);
            self.return_detached_tab_to_main(tab_id, window, cx);
            true
        } else {
            cx.notify();
            false
        }
    }

    fn detached_tab_return_drag_screen_point(&self) -> Option<Point<Pixels>> {
        let drag = self.detached_tab_return_drag?;
        drag.active
            .then(|| gpui::point(px(drag.current_screen_x), px(drag.current_screen_y)))
    }

    pub(super) fn detached_tab_return_placeholder(
        &self,
        cx: &App,
    ) -> Option<DetachedTabReturnPlaceholder> {
        let drag = self.detached_tab_return_drag?;
        let screen_point = self.detached_tab_return_drag_screen_point()?;
        let drop_bounds = self.main_window_tabbar_drop_bounds.as_ref()?;
        if !drop_bounds.contains(&screen_point) {
            return None;
        }
        Some(DetachedTabReturnPlaceholder {
            tab_id: drag.tab_id,
            visible_index: self.detached_tab_return_visible_index(f32::from(screen_point.x), cx)?,
        })
    }

    fn render_tab_window_handoff_surface(
        &self,
        animation_id: impl Into<gpui::ElementId>,
        tab_id: TabId,
        title: String,
        icon: LucideIcon,
        origin: TabWindowHandoffRect,
        target: TabWindowHandoffRect,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let accent = theme.accent;
        let spatial = self.tokens.motion.spatial_enabled;
        let surface = div()
            .id(("tab-window-handoff-surface", tab_id.0))
            .absolute()
            .left(px(origin.left))
            .top(px(origin.top))
            .w(px(origin.width))
            .h(px(origin.height))
            .overflow_hidden()
            .rounded(px(TAB_HANDOFF_CORNER_RADIUS))
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(14.0))
            .bg(rgb(theme.bg_panel))
            .border_1()
            .border_color(rgba((accent << 8) | 0xaa))
            .shadow_lg()
            .child(Self::render_lucide_icon(icon, 16.0, rgb(accent)))
            .child(
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(self.tokens.metrics.tab_font_size))
                    .text_color(rgb(theme.text))
                    .child(title),
            );

        surface
            .with_animation(
                animation_id,
                Animation::new(oxideterm_gpui_ui::motion::duration(
                    &self.tokens,
                    oxideterm_gpui_ui::motion::MotionDuration::Overlay,
                ))
                .with_easing(oxideterm_gpui_ui::motion::ease_in_out_cubic),
                move |surface, progress| {
                    let surface = surface.opacity(1.0 - progress);
                    if !spatial {
                        return surface;
                    }
                    let rect = interpolate_tab_window_handoff_rect(origin, target, progress);
                    surface
                        .left(px(rect.left))
                        .top(px(rect.top))
                        .w(px(rect.width))
                        .h(px(rect.height))
                        .rounded(px(oxideterm_gpui_ui::motion::lerp(
                            TAB_HANDOFF_CORNER_RADIUS,
                            0.0,
                            progress,
                        )))
                },
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_detached_tab_return_handoff(
        &self,
        window: &Window,
        cx: &App,
    ) -> Option<AnyElement> {
        let handoff = self.detached_tab_return_handoff?;
        let tab = self.tab_by_id(handoff.tab_id, cx)?;
        let tab_index = self.tab_index_by_id(handoff.tab_id, cx)?;
        let window_bounds = window.bounds();
        let origin = TabWindowHandoffRect {
            left: handoff.origin.screen_left - f32::from(window_bounds.origin.x),
            top: handoff.origin.screen_top - f32::from(window_bounds.origin.y),
            width: handoff.origin.width,
            height: handoff.origin.height,
        };
        let outside_main_tabs = self.tab_host.read(cx).outside_main_tab_ids();
        let preceding_width = self
            .tabs(cx)
            .iter()
            .take(tab_index)
            .filter(|candidate| !outside_main_tabs.contains(&candidate.id))
            .map(|candidate| self.tab_visual_width(candidate))
            .sum::<f32>();
        let target = TabWindowHandoffRect {
            left: self.tabbar_left_x() + self.tokens.metrics.tabbar_leading_offset
                - self.tabbar_effective_scroll_x(window, cx)
                + preceding_width,
            top: self
                .main_window_tabbar_drop_bounds
                .map(|bounds| f32::from(bounds.origin.y - window_bounds.origin.y))
                .unwrap_or_else(|| self.window_titlebar_height(window)),
            width: self.tab_visual_width(tab),
            height: self.tokens.metrics.tabbar_height,
        };
        Some(self.render_tab_window_handoff_surface(
            (
                gpui::ElementId::from(("detached-tab-return-handoff", handoff.tab_id.0)),
                format!("generation-{}", handoff.generation),
            ),
            handoff.tab_id,
            self.tab_display_title(tab),
            LucideIcon::PanelLeft,
            origin,
            target,
        ))
    }

    pub(in crate::workspace) fn render_tab_detach_drag_preview(
        &self,
        window: &Window,
        cx: &App,
    ) -> Option<AnyElement> {
        let drag = self.main_window_tabs.drag.as_ref()?;
        if !drag.active || drag.mode != TabDragMode::Detach {
            return None;
        }

        let tab_title = self
            .tab_by_id(drag.tab_id, cx)
            .map(|tab| self.tab_display_title(tab))
            .unwrap_or_else(|| "OxideTerm".to_string());
        let theme = self.tokens.ui;
        let accent = theme.accent;
        let geometry = tab_window_handoff_rect(
            drag.current_x,
            drag.current_y,
            f32::from(window.viewport_size().width),
            None,
            self.window_titlebar_height(window)
                + self.tokens.metrics.tabbar_height
                + TAB_HANDOFF_VIEWPORT_MARGIN,
            drag.tab_widths
                .get(drag.from_index)
                .copied()
                .unwrap_or(self.tokens.metrics.tab_max_width),
        );

        // The preview appears only after the drag is classified as a detach,
        // leaving ordinary horizontal tab reordering visually unchanged.
        let preview = div()
            .absolute()
            .left(px(geometry.left))
            .top(px(geometry.top))
            .w(px(geometry.width))
            .min_h(px(geometry.height))
            .px(px(14.0))
            .py(px(10.0))
            .rounded(px(TAB_HANDOFF_CORNER_RADIUS))
            .flex()
            .items_center()
            .gap(px(10.0))
            .bg(rgb(theme.bg_panel))
            .border_1()
            .border_color(rgba((accent << 8) | 0xaa))
            .shadow_lg()
            .child(Self::render_lucide_icon(
                LucideIcon::ExternalLink,
                16.0,
                rgb(accent),
            ))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.tab_font_size))
                            .line_height(px(18.0))
                            .text_color(rgb(theme.text))
                            .child(tab_title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px((self.tokens.metrics.tab_font_size - 1.0).max(11.0)))
                            .line_height(px(16.0))
                            .text_color(rgb(accent))
                            .child(self.i18n.t("tabbar.detach_to_window")),
                    ),
            );
        let preview = if self.tokens.motion.enabled {
            preview
                .with_animation(
                    ("tab-detach-drag-preview", drag.tab_id.0),
                    Animation::new(oxideterm_gpui_ui::motion::scaled_duration(
                        &self.tokens,
                        760,
                    ))
                    .repeat(),
                    |preview, delta| {
                        let pulse = if delta < 0.5 {
                            delta * 2.0
                        } else {
                            (1.0 - delta) * 2.0
                        };
                        preview.opacity(
                            0.82 + oxideterm_gpui_ui::motion::ease_in_out_cubic(pulse) * 0.16,
                        )
                    },
                )
                .into_any_element()
        } else {
            preview.opacity(0.96).into_any_element()
        };

        Some(preview)
    }

    fn render_detached_tab_return_drag_preview(
        &self,
        tab_id: TabId,
        window: &Window,
        cx: &App,
    ) -> Option<AnyElement> {
        let drag = self.detached_tab_return_drag?;
        if drag.tab_id != tab_id || !drag.active {
            return None;
        }

        let tab_title = self
            .tab_by_id(drag.tab_id, cx)
            .map(|tab| self.tab_display_title(tab))
            .unwrap_or_else(|| "OxideTerm".to_string());
        let theme = self.tokens.ui;
        let accent = theme.accent;
        let viewport = window.viewport_size();
        let window_bounds = window.bounds();
        let local_x = drag.current_screen_x - f32::from(window_bounds.origin.x);
        let local_y = drag.current_screen_y - f32::from(window_bounds.origin.y);
        let geometry = tab_window_handoff_rect(
            local_x,
            local_y,
            f32::from(viewport.width),
            Some(f32::from(viewport.height)),
            TAB_HANDOFF_VIEWPORT_MARGIN,
            self.tokens.metrics.tab_max_width,
        );

        // Return drags originate in the detached window, so this preview is
        // rendered there while the main window separately renders the drop zone.
        let preview = div()
            .absolute()
            .left(px(geometry.left))
            .top(px(geometry.top))
            .w(px(geometry.width))
            .min_h(px(geometry.height))
            .px(px(14.0))
            .py(px(10.0))
            .rounded(px(TAB_HANDOFF_CORNER_RADIUS))
            .flex()
            .items_center()
            .gap(px(10.0))
            .bg(rgb(theme.bg_panel))
            .border_1()
            .border_color(rgba((accent << 8) | 0xaa))
            .shadow_lg()
            .child(Self::render_lucide_icon(
                LucideIcon::PanelLeft,
                16.0,
                rgb(accent),
            ))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.tab_font_size))
                            .line_height(px(18.0))
                            .text_color(rgb(theme.text))
                            .child(tab_title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px((self.tokens.metrics.tab_font_size - 1.0).max(11.0)))
                            .line_height(px(16.0))
                            .text_color(rgb(accent))
                            .child(self.i18n.t("tabbar.return_to_main_window")),
                    ),
            );
        let preview = if self.tokens.motion.enabled {
            preview
                .with_animation(
                    ("detached-tab-return-drag-preview", drag.tab_id.0),
                    Animation::new(oxideterm_gpui_ui::motion::scaled_duration(
                        &self.tokens,
                        760,
                    ))
                    .repeat(),
                    |preview, delta| {
                        let pulse = if delta < 0.5 {
                            delta * 2.0
                        } else {
                            (1.0 - delta) * 2.0
                        };
                        preview.opacity(
                            0.82 + oxideterm_gpui_ui::motion::ease_in_out_cubic(pulse) * 0.16,
                        )
                    },
                )
                .into_any_element()
        } else {
            preview.opacity(0.96).into_any_element()
        };

        Some(preview)
    }

    pub(in crate::workspace) fn render_tab_context_menu(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.main_window_tabs.context_menu?;
        let renamable = self.tab_by_id(menu.tab_id, cx).is_some_and(|tab| {
            matches!(
                tab.kind,
                TabKind::LocalTerminal | TabKind::SshTerminal | TabKind::MoshTerminal
            )
        });
        self.tab_by_id(menu.tab_id, cx)?;
        let viewport = window.viewport_size();
        let menu_height = if renamable {
            TAB_CONTEXT_MENU_RENAME_HEIGHT
        } else {
            TAB_CONTEXT_MENU_HEIGHT
        };
        let placement = browser_behavior::clamp_context_menu_position(
            menu.x,
            menu.y,
            f32::from(viewport.width),
            f32::from(viewport.height),
            TAB_CONTEXT_MENU_WIDTH,
            menu_height,
            TAB_CONTEXT_MENU_MARGIN,
        );
        let detached = self.tab_host.read(cx).is_detached(menu.tab_id);
        let menu_body = context_menu_event_boundary(
            context_menu_content(&self.tokens)
                .w(px(TAB_CONTEXT_MENU_WIDTH))
                .when(renamable, |content| {
                    content.child(
                        context_menu_item(
                            &self.tokens,
                            self.i18n.t("tabbar.rename_tab"),
                            ContextMenuItemKind::Plain,
                            false,
                            false,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, window, cx| {
                                this.begin_tab_rename(menu.tab_id, window, cx);
                                cx.stop_propagation();
                            }),
                        ),
                    )
                })
                .child(
                    context_menu_item(
                        &self.tokens,
                        if detached {
                            self.i18n.t("tabbar.return_to_main_window")
                        } else {
                            self.i18n.t("tabbar.detach_to_window")
                        },
                        ContextMenuItemKind::Plain,
                        false,
                        false,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.close_tab_context_menu();
                            if detached {
                                this.return_detached_tab_to_main(menu.tab_id, window, cx);
                            } else {
                                this.detach_tab_to_window(menu.tab_id, None, window, cx);
                            }
                            cx.stop_propagation();
                        }),
                    ),
                )
                .child(context_menu_separator(&self.tokens))
                .child(
                    context_menu_item(
                        &self.tokens,
                        self.i18n.t("tabbar.close_tab"),
                        ContextMenuItemKind::Plain,
                        false,
                        false,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.close_tab_context_menu();
                            this.close_tab_by_id(menu.tab_id, window, cx);
                            cx.stop_propagation();
                        }),
                    ),
                ),
        );
        let menu_body = overlay_content_boundary(menu_body);

        Some(
            self.workspace_context_menu_backdrop(
                deferred(
                    anchored()
                        .anchor(Corner::TopLeft)
                        .position(gpui::point(px(placement.x), px(placement.y)))
                        .position_mode(AnchoredPositionMode::Window)
                        .child(menu_body),
                )
                .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY),
                cx,
            )
            .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_detached_tab_window(
        &mut self,
        tab_id: TabId,
        entry_handoff_origin: Option<TabWindowHandoffOrigin>,
        window_background: &Entity<window_shell::WorkspaceWindowBackgroundEntity>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.app_lock.locked {
            window.set_window_title(&SharedString::from(
                self.i18n.t("settings_view.general.app_lock_window_title"),
            ));
            return self.render_detached_tab_message(
                "OxideTerm",
                "settings_view.general.app_lock_detached_description",
                cx,
            );
        }
        // Release TabHost's read guard before rendering without cloning the
        // complete Tab; the pane tree is bounded to four panes.
        let Some((title, tab_kind, root_pane)) = self.tab_by_id(tab_id, cx).map(|tab| {
            (
                self.tab_display_title(tab),
                tab.kind.clone(),
                tab.root_pane.clone(),
            )
        }) else {
            return self.render_detached_tab_message("OxideTerm", "tabbar.detached_tab_closed", cx);
        };
        window.set_window_title(&SharedString::from(title.clone()));

        let content =
            self.render_detached_tab_content(tab_id, &tab_kind, root_pane.as_ref(), window, cx);
        let content = self.wrap_content_background(
            window_background,
            content,
            Some(tab_background_key(&tab_kind)),
            window,
            cx,
        );
        let titlebar_visible = self.window_titlebar_visible(window);

        let window_content = div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(self.tokens.ui.bg))
            .when(titlebar_visible, |root| {
                root.child(self.render_detached_tab_title_bar(tab_id, title.clone(), window, cx))
            })
            .child(div().flex_1().min_h(px(0.0)).child(content))
            .when_some(
                self.render_detached_tab_return_drag_preview(tab_id, window, cx),
                |root, preview| root.child(preview),
            );
        let window_content = oxideterm_gpui_ui::motion::fade_in(
            &self.tokens,
            ("detached-tab-window-enter", tab_id.0),
            window_content,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );

        let entry_handoff = entry_handoff_origin.map(|origin| {
            let window_bounds = window.bounds();
            let viewport = window.viewport_size();
            let origin = TabWindowHandoffRect {
                left: origin.screen_left - f32::from(window_bounds.origin.x),
                top: origin.screen_top - f32::from(window_bounds.origin.y),
                width: origin.width,
                height: origin.height,
            };
            let target = TabWindowHandoffRect {
                left: 0.0,
                top: 0.0,
                width: f32::from(viewport.width),
                height: f32::from(viewport.height),
            };
            self.render_tab_window_handoff_surface(
                ("detached-tab-entry-handoff", tab_id.0),
                tab_id,
                title,
                LucideIcon::ExternalLink,
                origin,
                target,
            )
        });
        let tab_window_modals = self.render_tab_window_modals(tab_id, &tab_kind, window, cx);

        // Keep the native window base opaque while its workspace content fades in.
        div()
            .size_full()
            .relative()
            .bg(rgb(self.tokens.ui.bg))
            .child(window_content)
            .when_some(entry_handoff, |root, handoff| root.child(handoff))
            // Detached tabs use their own native window root as the modal portal.
            .children(tab_window_modals)
            .into_any_element()
    }

    fn render_detached_tab_content(
        &mut self,
        tab_id: TabId,
        kind: &TabKind,
        root_pane: Option<&PaneNode>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match detached_tab_surface_route(tab_id, kind) {
            DetachedTabSurfaceRoute::Settings => return self.render_settings_surface(cx),
            DetachedTabSurfaceRoute::Ide(tab_id) => {
                return self.render_ide_surface_for_tab(tab_id, cx);
            }
            DetachedTabSurfaceRoute::Sftp(tab_id) => {
                return self.render_sftp_surface_for_tab(tab_id, window, cx);
            }
            DetachedTabSurfaceRoute::Forwards(tab_id) => {
                return self.render_forwards_surface_for_tab(tab_id, window, cx);
            }
            DetachedTabSurfaceRoute::Other => {}
        }
        match (kind, root_pane) {
            (TabKind::FileManager, _) => self.render_file_manager_surface(window, cx),
            (TabKind::Graphics, _) => self.render_graphics_surface(window, cx),
            (TabKind::Runtime, _) => self.render_connection_runtime_surface(cx),
            (TabKind::ConnectionPool, _) => {
                // Detached windows can outlive the UI route that created them.
                // Preserve compatibility by rendering the runtime overview.
                self.host_tools.update(cx, |host_tools, _cx| {
                    host_tools.reset_runtime_section();
                });
                self.render_connection_runtime_surface(cx)
            }
            (TabKind::Topology, _) => self.render_topology_surface(cx),
            (TabKind::NotificationCenter, _) => self.render_notification_center_surface(cx),
            (TabKind::SessionManager, _) => self.render_session_manager_surface(window, cx),
            (TabKind::PluginManager, _) => self.render_plugin_manager_surface(cx),
            (TabKind::Plugin { plugin_id, tab_id }, _) => {
                self.render_native_plugin_tab_surface(plugin_id, tab_id, cx)
            }
            (TabKind::CloudSync, _) => self.render_cloud_sync_surface(cx),
            (TabKind::RemoteDesktop, _) => self.render_remote_desktop_surface(tab_id, window, cx),
            (_, Some(root_pane)) => self.render_detached_terminal_surface(tab_id, root_pane, cx),
            _ => self.render_empty_workspace(f32::from(window.viewport_size().width), cx),
        }
    }

    fn render_detached_tab_title_bar(
        &self,
        tab_id: TabId,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let button_layout = sidebar::client_titlebar_button_layout(cx);
        let supported_controls = window.window_controls();
        div()
            .h(px(self.tokens.metrics.titlebar_height))
            .w_full()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg))
            // Linux controls must begin at the configured edge; keep the
            // existing traffic-light/title inset on the other desktop shells.
            .when(!cfg!(target_os = "linux"), |bar| bar.pl(px(72.0)))
            .text_size(px(self.tokens.metrics.titlebar_label_font_size))
            .text_color(rgb(theme.text))
            .when(cfg!(target_os = "linux"), |bar| {
                bar.child(self.render_client_titlebar_controls(
                    button_layout.left,
                    supported_controls,
                    theme.bg,
                    theme.text_muted,
                    window.is_maximized(),
                    cx,
                ))
            })
            .child(
                div()
                    .id(("detached-tab-title-drag", tab_id.0))
                    .h_full()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .occlude()
                    // Windows moves client-decorated windows through native
                    // HTCAPTION handling; consuming mouse-down in GPUI blocks it.
                    .when(cfg!(target_os = "windows"), |region| {
                        region.window_control_area(gpui::WindowControlArea::Drag)
                    })
                    .when(!cfg!(target_os = "windows"), |region| {
                        region
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    if sidebar::handle_window_drag_mouse_down(event, window) {
                                        cx.stop_propagation();
                                        return;
                                    }
                                    this.start_detached_tab_return_drag(tab_id, event, window, cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .on_mouse_move(cx.listener(
                                move |this, event: &MouseMoveEvent, window, cx| {
                                    this.update_detached_tab_return_drag(tab_id, event, window, cx);
                                },
                            ))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                                    this.finish_detached_tab_return_drag(tab_id, event, window, cx);
                                    cx.stop_propagation();
                                }),
                            )
                    })
                    .child(div().min_w(px(0.0)).truncate().child(title)),
            )
            .child(
                div()
                    .h_full()
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(6.0))
                    .cursor_pointer()
                    .text_color(rgb(theme.text_muted))
                    .hover(move |button| button.bg(rgb(theme.bg_hover)))
                    .child(Self::render_lucide_icon(
                        LucideIcon::PanelLeft,
                        15.0,
                        rgb(theme.text_muted),
                    ))
                    .child(self.i18n.t("tabbar.return_to_main_window"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.return_detached_tab_to_main(tab_id, window, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .when(
                cfg!(any(target_os = "windows", target_os = "linux")),
                |bar| {
                    bar.child(self.render_detached_client_titlebar_controls(
                        button_layout.right,
                        supported_controls,
                        window,
                        cx,
                    ))
                },
            )
            .when(
                cfg!(target_os = "linux") && supported_controls.window_menu,
                |bar| {
                    bar.on_mouse_down(MouseButton::Right, |event, window, cx| {
                        window.show_window_menu(event.position);
                        cx.stop_propagation();
                    })
                },
            )
            .into_any_element()
    }

    fn render_detached_client_titlebar_controls(
        &self,
        buttons: [Option<gpui::WindowButton>; gpui::MAX_BUTTONS_PER_SIDE],
        supported_controls: gpui::WindowControls,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        self.render_client_titlebar_controls(
            buttons,
            supported_controls,
            theme.bg,
            theme.text_muted,
            window.is_maximized(),
            cx,
        )
    }

    fn render_detached_tab_message(
        &self,
        title: &'static str,
        message_key: &'static str,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(self.tokens.ui.bg))
            .child(
                div()
                    .h(px(self.tokens.metrics.titlebar_height))
                    .flex()
                    .items_center()
                    .px(px(16.0))
                    .border_b_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .child(title),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(message_key)),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_insertion_index_follows_the_pointer_between_tab_midpoints() {
        let widths = [100.0, 160.0, 120.0];

        assert_eq!(tab_return_visible_insertion_index(0.0, &widths), 0);
        assert_eq!(tab_return_visible_insertion_index(80.0, &widths), 1);
        assert_eq!(tab_return_visible_insertion_index(200.0, &widths), 2);
        assert_eq!(tab_return_visible_insertion_index(500.0, &widths), 3);
    }

    #[test]
    fn detached_shared_surfaces_route_without_main_active_tab_state() {
        let tab_id = TabId(42);

        assert_eq!(
            detached_tab_surface_route(tab_id, &TabKind::Settings),
            DetachedTabSurfaceRoute::Settings
        );
        assert_eq!(
            detached_tab_surface_route(tab_id, &TabKind::Ide),
            DetachedTabSurfaceRoute::Ide(tab_id)
        );
        assert_eq!(
            detached_tab_surface_route(tab_id, &TabKind::Sftp),
            DetachedTabSurfaceRoute::Sftp(tab_id)
        );
        assert_eq!(
            detached_tab_surface_route(tab_id, &TabKind::Forwards),
            DetachedTabSurfaceRoute::Forwards(tab_id)
        );
    }
}
