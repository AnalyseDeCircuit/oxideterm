use super::*;
use oxideterm_gpui_settings_view::keybinding_page::{
    KeybindingPage, KeybindingRequest, KeybindingTextRole,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn keybinding_definitions(
        &self,
        cx: &App,
    ) -> Vec<crate::keybindings::ActionDefinition> {
        let mut definitions = crate::keybindings::ACTION_DEFINITIONS.to_vec();
        for entry in &self
            .plugin_entity
            .read(cx)
            .registry()
            .contributions()
            .runtime_keybindings
        {
            if let Some(definition) = crate::keybindings::plugin_action_definition(entry)
                && !definitions
                    .iter()
                    .any(|existing| existing.id == definition.id)
            {
                definitions.push(definition);
            }
        }
        definitions
    }

    pub(in crate::workspace) fn keybinding_definition(
        &self,
        id: &str,
        cx: &App,
    ) -> Option<crate::keybindings::ActionDefinition> {
        self.keybinding_definitions(cx)
            .into_iter()
            .find(|definition| definition.id == id)
    }

    pub(in crate::workspace) fn keybinding_label(
        &self,
        definition: &crate::keybindings::ActionDefinition,
    ) -> String {
        definition
            .label
            .clone()
            .unwrap_or_else(|| self.i18n.t(&definition.label_key()))
    }

    fn with_keybinding_page<R>(
        &self,
        cx: &Context<Self>,
        render: impl FnOnce(KeybindingPage<'_>, &App) -> R,
    ) -> R {
        let catalog = self.keybinding_definitions(cx);
        let owner = cx.entity().downgrade();
        let dispatch = Rc::new(move |request, cx: &mut App| {
            let _ = owner.update(cx, |app, cx| match request {
                KeybindingRequest::Import => app.import_keybindings(cx),
                KeybindingRequest::Export => app.export_keybindings(cx),
                KeybindingRequest::Reset(id) => app.reset_keybinding(&id, cx),
                KeybindingRequest::Unbind(id) => app.unbind_keybinding(&id, cx),
                KeybindingRequest::CommitRecording => app.confirm_keybinding_recording(cx),
                KeybindingRequest::CloseReset { apply } => {
                    if app.begin_keybinding_reset_all_confirm_exit(cx) && apply {
                        app.reset_all_keybindings(cx);
                    }
                }
            });
        });
        let input_dispatch = Self::settings_window_dispatch(cx);
        let search = |cx: &App| {
            let state = self.settings_workspace.read(cx);
            let input = SettingsInput::KeybindingSearch;
            self.settings_components().keybinding_search_field(
                self.settings_ime_frame(
                    input,
                    state.keybinding_search_query(),
                    state.settings_entity_focused_input() == Some(input),
                    cx,
                ),
                input_dispatch.clone(),
            )
        };
        let text = self.selectable_text_render_state_for_entity(cx.entity(), cx);
        let label = |role, id, value, color| {
            let role = match role {
                KeybindingTextRole::PlainDocument => SelectableTextRole::PlainDocument,
                KeybindingTextRole::NonSelectable => SelectableTextRole::NonSelectable,
            };
            text.render_display_text_with_id_in_group(
                role,
                selectable_text::selectable_document_group_id(),
                id,
                0,
                value,
                color,
            )
        };
        render(
            KeybindingPage {
                components: self.settings_components(),
                state: &self.settings_workspace,
                catalog: &catalog,
                search: &search,
                label: &label,
                dispatch,
            },
            cx,
        )
    }

    pub(in crate::workspace) fn settings_keybindings_section(
        &self,
        section: SettingsSectionId,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.with_keybinding_page(cx, |page, cx| {
            page.settings_keybindings_section(section, cx)
        })
    }

    pub(in crate::workspace) fn render_keybinding_reset_all_confirm_dialog(
        &self,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.with_keybinding_page(cx, |page, cx| {
            page.render_keybinding_reset_all_confirm_dialog(cx)
        })
    }
}
