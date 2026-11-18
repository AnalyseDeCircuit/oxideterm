use super::*;
use oxideterm_settings::BackgroundStyle;

impl WorkspaceApp {
    pub(super) fn appearance_edit_scheme(&self, cx: &App) -> Option<bool> {
        self.settings_workspace.read(cx).editing_scheme()
    }
    pub(in crate::workspace) fn edit_background_style(
        &mut self,
        edit: impl FnOnce(&mut BackgroundStyle),
        cx: &mut Context<Self>,
    ) {
        if self.settings_workspace.read(cx).theme_editor().is_some() {
            self.settings_workspace.update(cx, |settings, cx| {
                settings.edit_theme_editor_background(edit, cx);
            });
        } else {
            let dark = self.appearance_edit_scheme(cx);
            self.edit_settings(
                |settings| {
                    let mut style = settings.terminal.background_for_scheme(dark);
                    edit(&mut style);
                    settings.terminal.set_background_for_scheme(dark, style);
                },
                cx,
            );
        }
    }
}
