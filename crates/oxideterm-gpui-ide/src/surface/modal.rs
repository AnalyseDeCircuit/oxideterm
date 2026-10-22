// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum IdeModal {
    DirtyClose,
    Conflict,
    FolderSwitch,
    FolderPicker,
    TreeName,
    Delete,
    AgentRemove,
}

/// Window-level presentation of the surface's modal state, without a second state owner.
pub struct IdeModalLayer {
    surface: Entity<IdeSurface>,
    active: Option<IdeModal>,
    previous_focus: Option<FocusHandle>,
    window_id: Option<gpui::WindowId>,
    _observation: gpui::Subscription,
}

impl IdeModalLayer {
    pub fn new(surface: Entity<IdeSurface>, cx: &mut Context<Self>) -> Self {
        let observation = cx.observe(&surface, |_, _, cx| cx.notify());
        Self {
            surface,
            active: None,
            previous_focus: None,
            window_id: None,
            _observation: observation,
        }
    }
}

impl Render for IdeModalLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let window_id = window.window_handle().window_id();
        if self.window_id != Some(window_id) {
            self.active = None;
            self.previous_focus = None;
            self.window_id = Some(window_id);
        }
        let active = self.surface.read(cx).active_modal();
        let focus = self.surface.read(cx).modal_focus_handle.clone();
        if active != self.active {
            if active.is_some() {
                if self.active.is_none() {
                    self.previous_focus = window.focused(cx);
                }
                window.focus(&focus, cx);
            } else if let Some(previous) = self.previous_focus.take()
                && focus.contains_focused(window, cx)
            {
                window.focus(&previous, cx);
            }
            self.active = active;
        }
        let Some(active) = active else {
            return div().into_any_element();
        };
        let dialog = self.surface.update(cx, |surface, cx| match active {
            IdeModal::DirtyClose => surface.render_dirty_close_dialog(cx),
            IdeModal::Conflict => surface.render_conflict_dialog(cx),
            IdeModal::FolderSwitch => surface.render_folder_switch_confirm_dialog(cx),
            IdeModal::FolderPicker => surface.render_folder_picker_dialog(cx),
            IdeModal::TreeName => surface.render_tree_name_input_dialog(cx),
            IdeModal::Delete => surface.render_delete_confirm_dialog(cx),
            IdeModal::AgentRemove => surface.render_agent_remove_confirm_dialog(cx),
        });
        let tokens = self.surface.read(cx).tokens;
        div()
            .font_family(tauri_ui_font_family(tokens.metrics.font_family))
            .text_size(px(tokens.metrics.ui_text_sm))
            .text_color(rgb(tokens.ui.text))
            .absolute()
            .inset_0()
            .track_focus(&focus)
            .key_context(oxideterm_gpui_ui::modal::MODAL_KEY_CONTEXT)
            .debug_selector(|| "ide-window-modal".into())
            .capture_key_down(cx.listener(|this, event, window, cx| {
                let consumed = this.surface.update(cx, |surface, cx| {
                    surface.handle_modal_key(event, window, cx)
                });
                if consumed {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .on_key_up(|_, _, cx| cx.stop_propagation())
            .on_modifiers_changed(|_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.surface.update(cx, |surface, cx| {
                    surface.update_folder_picker_path_selection_drag(event, window, cx);
                });
                cx.stop_propagation();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.surface.update(cx, |surface, _| {
                        surface.finish_folder_picker_path_selection_drag()
                    });
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
            .child(dialog)
            .into_any_element()
    }
}

impl IdeSurface {
    pub(super) fn active_modal(&self) -> Option<IdeModal> {
        if self.agent_remove_confirm_open {
            Some(IdeModal::AgentRemove)
        } else if self.delete_confirm.is_some() {
            Some(IdeModal::Delete)
        } else if self.tree_name_input.is_some() {
            Some(IdeModal::TreeName)
        } else if self.folder_picker.open {
            Some(IdeModal::FolderPicker)
        } else if self.folder_switch_confirm_open {
            Some(IdeModal::FolderSwitch)
        } else if self.conflict_state.is_some() {
            Some(IdeModal::Conflict)
        } else {
            self.workspace.pending_close().map(|_| IdeModal::DirtyClose)
        }
    }

    pub fn has_modal(&self) -> bool {
        self.active_modal().is_some()
    }

    /// Returns false only when the top modal needs the platform text-input pipeline.
    pub fn handle_modal_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(active) = self.active_modal() else {
            return false;
        };
        let busy = match active {
            IdeModal::TreeName => self
                .tree_name_input
                .as_ref()
                .is_some_and(|input| input.submitting),
            IdeModal::Delete => self
                .delete_confirm
                .as_ref()
                .is_some_and(|confirm| confirm.deleting),
            IdeModal::Conflict => self
                .conflict_state
                .as_ref()
                .is_some_and(|conflict| self.saving_tabs.contains(&conflict.tab_id)),
            _ => false,
        };
        if busy {
            return true;
        }
        match active {
            IdeModal::TreeName | IdeModal::FolderPicker => {
                let text_enabled = if active == IdeModal::TreeName {
                    self.tree_name_input
                        .as_ref()
                        .is_some_and(|input| !input.submitting)
                } else {
                    self.folder_picker.path_input_focused
                };
                let composing = if active == IdeModal::TreeName {
                    self.tree_name_input
                        .as_ref()
                        .is_some_and(|input| input.marked_text.is_some())
                } else {
                    self.folder_picker.path_marked_text.is_some()
                };
                if text_enabled
                    && (key_uses_platform_text_input(event)
                        || (composing
                            && matches!(event.keystroke.key.as_str(), "enter" | "space" | " ")))
                {
                    return false;
                }
                if active == IdeModal::TreeName {
                    self.handle_tree_name_input_key(event, cx);
                } else {
                    self.handle_folder_picker_key(event, window, cx);
                }
            }
            _ if event.keystroke.key == "escape" => {
                match active {
                    IdeModal::DirtyClose => {
                        self.resolve_dirty_close(DirtyCloseDecision::Cancel, cx)
                    }
                    IdeModal::Conflict => self.conflict_state = None,
                    IdeModal::FolderSwitch => self.folder_switch_confirm_open = false,
                    IdeModal::Delete => self.cancel_delete_tree_item(cx),
                    IdeModal::AgentRemove => self.agent_remove_confirm_open = false,
                    _ => unreachable!(),
                }
                cx.notify();
            }
            _ => {}
        }
        true
    }
}
