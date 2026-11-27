use super::*;

impl ConnectionPage<'_> {
    pub(super) fn discovery_editor_actions(
        &self,
        draft: Option<DiscoverySource>,
    ) -> Div {
        let dispatch = self.dispatch.clone();
        let save = self.workspace_toolbar_action_button(
            self.i18n.t("plugin.connections.save"),
            None,
            self.connection_import_secondary_button_options(draft.is_none()),
            self.select(move |state, cx| {
                if let Some(request) = state
                    .discovery
                    .editor
                    .take()
                    .and_then(SourceEditor::save_request)
                {
                    state.blur_settings_entity_input(cx);
                    cx.notify();
                    let dispatch = dispatch.clone();
                    cx.defer(move |cx| dispatch(request, cx));
                }
            }),
        );
        div().flex().flex_wrap().gap(px(8.)).child(save)
    }
}
