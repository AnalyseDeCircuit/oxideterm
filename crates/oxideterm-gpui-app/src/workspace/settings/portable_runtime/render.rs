use super::*;
use oxideterm_gpui_settings_view::portable_runtime::{
    PortableSettingsPage, PortableSettingsRequest,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_portable_section(
        &mut self,
        section: SettingsSectionId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ensure_portable_settings_snapshot(cx);
        let current_data_dir = self.settings_store.path();
        let current_data_dir = current_data_dir
            .parent()
            .unwrap_or(current_data_dir)
            .display()
            .to_string();
        let owner = cx.entity().downgrade();
        let dispatch = Rc::new(move |request, cx: &mut App| {
            let _ = owner.update(cx, |app, cx| match request {
                PortableSettingsRequest::ToggleAutoUnlock(enabled) => {
                    app.set_portable_auto_unlock_enabled(enabled, cx)
                }
                PortableSettingsRequest::ChangePassword => {
                    app.open_portable_password_change_dialog(cx)
                }
                PortableSettingsRequest::Export => {
                    app.open_oxide_export_portable_migration_dialog(cx)
                }
                PortableSettingsRequest::Import => {
                    app.open_oxide_import_portable_migration_dialog(cx)
                }
            });
        });
        let text = self.selectable_text_render_state_for_entity(cx.entity(), cx);
        PortableSettingsPage {
            components: self.settings_components(),
            state: self.settings_workspace.read(cx),
            current_data_dir: &current_data_dir,
            text: &|value, color| {
                text.render_display_text_with_id_in_group(
                    SelectableTextRole::NonSelectable,
                    selectable_text::selectable_document_group_id(),
                    0,
                    0,
                    value,
                    color,
                )
            },
            dispatch,
        }
        .render_section(section)
    }
}
