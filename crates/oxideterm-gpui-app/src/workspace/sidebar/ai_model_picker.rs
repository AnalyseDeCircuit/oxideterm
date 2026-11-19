use oxideterm_gpui_ai::model_picker::host::AiModelPickerHost as _;
use std::rc::Rc;

use gpui::{App, Context, Entity};
use oxideterm_gpui_ai::{
    model_picker::{
        AiModelPickerBindings, AiModelPickerFrame, AiModelPickerRequest, AiModelPickerViews,
    },
    state::AiWorkspaceEntity,
};

use super::super::{WorkspaceApp, ime::WorkspaceImeTarget};

impl WorkspaceApp {
    pub(in crate::workspace) fn mount_ai_model_picker(
        state: &Entity<AiWorkspaceEntity>,
        cx: &mut Context<Self>,
    ) -> AiModelPickerViews {
        let workspace = cx.weak_entity();
        let request_workspace = workspace.clone();
        let select_workspace = workspace.clone();
        let input_workspace = workspace.clone();
        let search = WorkspaceImeTarget::AiModelSelectorSearch;
        let state_for_frame = state.clone();
        let bindings = AiModelPickerBindings {
            frame: Rc::new(move |cx: &App| {
                let workspace = workspace.upgrade()?;
                let app = workspace.read(cx);
                let settings = app.settings_store.settings();
                Some(AiModelPickerFrame::from_state(
                    state_for_frame.read(cx),
                    &settings.ai,
                    app.tokens,
                    app.i18n.clone(),
                    app.marked_text_for_target(search, cx).map(str::to_owned),
                    search.anchor_id(),
                    app.input_caret.visible(),
                    |id| app.ai_picker_plugin_available(id, cx),
                ))
            }),
            request: Rc::new(move |request, window, cx| {
                let _ = request_workspace.update(cx, |app, cx| {
                    let Some(request) = app.handle_picker_action(request, cx) else {
                        return;
                    };
                    match request {
                        AiModelPickerRequest::Toggle(scope) => {
                            app.toggle_ai_model_selector(scope, window, cx)
                        }
                        AiModelPickerRequest::Settings { close } => {
                            if close {
                                app.close_ai_model_selector(cx);
                            }
                            app.open_ai_settings(window, cx);
                        }
                        AiModelPickerRequest::ManageAcp(custom) => {
                            app.close_ai_model_selector(cx);
                            app.open_acp_plugin_manager(custom, window, cx);
                        }
                        AiModelPickerRequest::SearchPointer(event) => {
                            app.ime_marked_text = None;
                            window.focus(&app.focus_handle, cx);
                            app.begin_ime_selection_from_mouse_down(search, &event, window, cx);
                        }
                        AiModelPickerRequest::SearchDrag(event) => {
                            app.update_ime_selection_drag_from_mouse_move(&event, window, cx);
                        }
                        AiModelPickerRequest::SelectAcpConfig(agent, config, value) => {
                            app.select_ai_acp_config_from_selector(agent, config, value, cx)
                        }
                        AiModelPickerRequest::SelectAcpMode(agent, mode) => {
                            app.select_ai_acp_mode_from_selector(agent, mode, cx)
                        }
                        _ => unreachable!("picker-local action already dispatched"),
                    }
                });
            }),
            anchor: Rc::new(move |anchor, window, cx| {
                let workspace = select_workspace.clone();
                window.defer(cx, move |_, cx| {
                    let _ = workspace.update(cx, |app, cx| app.update_select_anchor(anchor, cx));
                });
            }),
            text_anchor: Rc::new(move |anchor, window, cx| {
                let workspace = input_workspace.clone();
                window.defer(cx, move |_, cx| {
                    let _ =
                        workspace.update(cx, |app, cx| app.update_text_input_anchor(anchor, cx));
                });
            }),
        };
        AiModelPickerViews::mount(state, bindings, cx)
    }
}
