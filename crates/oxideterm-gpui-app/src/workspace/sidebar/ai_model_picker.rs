use std::rc::Rc;

use gpui::{App, AppContext, Context, Entity};
use oxideterm_gpui_ai::{
    model_picker::{
        AiModelPickerBindings, AiModelPickerFrame, AiModelPickerPart, AiModelPickerRequest,
        AiModelPickerView, AiModelPickerViews, active_model_picker_provider,
    },
    state::{AiModelSelectorScope, AiWorkspaceEntity},
};
use oxideterm_gpui_ui::select::SelectAnchorId;

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
                let ai = state_for_frame.read(cx);
                let providers = ai.picker_providers(&settings.ai, &app.i18n);
                let status = providers.iter().map(|provider| {
                    (provider.id.clone(), ai.picker_status(provider, &settings.ai, &app.i18n,
                        |id| app.ai_picker_plugin_available(id, cx)))
                }).collect();
                Some(AiModelPickerFrame {
                    tokens: app.tokens,
                    i18n: app.i18n.clone(),
                    providers,
                    status,
                    active_provider: active_model_picker_provider(&settings.ai),
                    active_model: settings.ai.active_model.clone(),
                    acp_active: settings.ai.active_backend
                        == oxideterm_settings::AiActiveBackend::Acp,
                    active_acp_agent_id: settings.ai.active_acp_agent_id.clone(),
                    marked: app.marked_text_for_target(search, cx).map(str::to_owned),
                    search_anchor: search.anchor_id(),
                    caret_visible: app.input_caret.visible(),
                })
            }),
            request: Rc::new(move |request, window, cx| {
                let _ = request_workspace.update(cx, |app, cx| match request {
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
                    AiModelPickerRequest::Refresh(provider) => {
                        app.refresh_ai_provider_from_selector(provider, cx)
                    }
                    AiModelPickerRequest::Discover(id) => {
                        app.schedule_ai_acp_model_discovery(id, cx)
                    }
                    AiModelPickerRequest::Select(provider, model) => {
                        app.select_ai_model_from_selector(provider, model, cx)
                    }
                    AiModelPickerRequest::SelectAcpModel(agent, config, value) => {
                        app.select_ai_acp_model_from_selector(agent, config, value, cx)
                    }
                    AiModelPickerRequest::SelectAcpConfig(agent, config, value) => {
                        app.select_ai_acp_config_from_selector(agent, config, value, cx)
                    }
                    AiModelPickerRequest::SelectAcpMode(agent, mode) => {
                        app.select_ai_acp_mode_from_selector(agent, mode, cx)
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
        let mut mount =
            |part| cx.new(|_| AiModelPickerView::new(state.clone(), bindings.clone(), part));
        AiModelPickerViews {
            sidebar: mount(AiModelPickerPart::Trigger(
                AiModelSelectorScope::Sidebar,
                SelectAnchorId::AiModelSelector,
            )),
            inline: mount(AiModelPickerPart::Trigger(
                AiModelSelectorScope::TerminalInline,
                SelectAnchorId::AiInlineModelSelector,
            )),
            menu: mount(AiModelPickerPart::Menu),
        }
    }
}
