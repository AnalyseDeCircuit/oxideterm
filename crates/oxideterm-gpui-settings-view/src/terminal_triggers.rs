use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, Div, Entity, MouseButton, MouseDownEvent, Render, Subscription,
    Window, div, prelude::*, px, rgb,
};
use oxideterm_connections::ConnectionStore;
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_button,
    },
    modal::{
        dialog_description, dialog_footer, dialog_header, dialog_title,
        dismissible_dialog_backdrop, overlay_content_boundary,
    },
    scroll::ScrollableElement,
    switch::switch,
};
use oxideterm_terminal_triggers::TerminalTrigger;
use oxideterm_workspace::PaneId;

use crate::{
    SettingsInput, SettingsSelect, components::SettingsControls, entity::SettingsWorkspaceEntity,
};

mod render;
mod state;
pub use state::{
    TerminalTriggersSettingsState, editable_trigger_action, editable_trigger_action_mut,
    is_terminal_trigger_input,
};

/// The editor redraws its own overlay while the window observes only open/close.
pub struct TerminalTriggerEditor {
    render_body: Rc<dyn Fn(&App) -> Option<AnyElement>>,
    _settings_changed: Subscription,
}

impl TerminalTriggerEditor {
    pub fn new(
        settings: &Entity<SettingsWorkspaceEntity>,
        render_body: Rc<dyn Fn(&App) -> Option<AnyElement>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings_changed = cx.observe(settings, |_, settings, cx| {
            if settings.read(cx).terminal_triggers.editor_open {
                cx.notify();
            }
        });
        Self {
            render_body,
            _settings_changed: settings_changed,
        }
    }
}

impl Render for TerminalTriggerEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        (self.render_body)(cx).unwrap_or_else(|| div().into_any_element())
    }
}

pub enum TerminalTriggerRequest {
    OpenNew,
    OpenEdit(String),
    OpenDuplicate(String, String),
    CloseEditor,
    SaveEditor,
    CloseScopePicker,
    ShowGlobal,
    ToggleShellTrust(bool),
    CancelShellTrust,
    ConfirmShellTrust,
    ToggleRule(String),
    DeleteRule(String),
    ToggleForPane(PaneId, String),
    EditDraft {
        clear_input: bool,
        edit: Box<dyn FnOnce(&mut TerminalTriggersSettingsState)>,
    },
}

pub struct TerminalTriggerPage<'a> {
    pub controls: SettingsControls<'a>,
    pub terminal_triggers: &'a TerminalTriggersSettingsState,
    pub connection_store: &'a ConnectionStore,
    pub terminal_trigger_shell_confirmation_pending: bool,
    pub modal_scroll: gpui::ScrollHandle,
    pub pane_exists: &'a dyn Fn(PaneId, &App) -> bool,
    pub applies_to_pane: &'a dyn Fn(PaneId, &TerminalTrigger, &App) -> bool,
    pub enabled_for_pane: &'a dyn Fn(PaneId, &str, &App) -> bool,
    pub quick_command_label: &'a dyn Fn(&str, &App) -> Option<String>,
    pub dispatch_trigger: Rc<dyn Fn(TerminalTriggerRequest, &mut Window, &mut App)>,
}

impl<'a> std::ops::Deref for TerminalTriggerPage<'a> {
    type Target = SettingsControls<'a>;
    fn deref(&self) -> &Self::Target {
        &self.controls
    }
}

fn render_lucide_icon(icon: &'static str, size: f32, color: gpui::Rgba) -> AnyElement {
    gpui::svg()
        .path(icon)
        .size(px(size))
        .text_color(color)
        .into_any_element()
}

impl TerminalTriggerPage<'_> {
    fn trigger_listener(
        &self,
        request: impl Fn() -> TerminalTriggerRequest + 'static,
        stop_propagation: bool,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        let dispatch = self.dispatch_trigger.clone();
        move |_, window, cx| {
            dispatch(request(), window, cx);
            if stop_propagation {
                cx.stop_propagation();
            }
        }
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let actionable = !(options.button.disabled || options.loading);
        toolbar_button(self.tokens, label, icon, options)
            .when(actionable, |button| {
                button.on_mouse_down(MouseButton::Left, listener)
            })
            .when(!actionable, |button| {
                button.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            })
    }
}
