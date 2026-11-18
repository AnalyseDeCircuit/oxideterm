use super::*;
use oxideterm_gpui_settings_view::navigation_editor::{
    SettingsNavigationEditorFrame,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn navigation_editor_bindings(cx: &Context<Self>) -> (Rc<dyn Fn(&App) -> Option<SettingsNavigationEditorFrame>>, Rc<dyn Fn(bool, &mut App)>) {
        let owner = cx.entity().downgrade();
        let frame = Rc::new({
            let owner = owner.clone();
            move |cx: &App| {
                let owner = owner.upgrade()?;
                let app = owner.read(cx);
                Some(SettingsNavigationEditorFrame {
                    tokens: app.tokens,
                    i18n: app.i18n.clone(),
                    panel_background: app.settings_panel_background(app.tokens.ui.bg_panel),
                    focused_action: app.standard_confirm_focus(),
                })
            }
        });
        let finish = Rc::new(move |save: bool, cx: &mut App| {
            let _ = owner.update(cx, |app, cx| {
                if save {
                    app.save_settings_navigation_editor(cx);
                } else {
                    app.close_settings_navigation_editor(cx);
                }
            });
        });
        (frame, finish)
    }

    pub(in crate::workspace) fn open_settings_navigation_editor(&mut self, cx: &mut Context<Self>) {
        let persisted_groups = &self.settings_store.settings().settings_navigation.groups;
        self.settings_workspace.update(cx, |settings, cx| {
            settings.close_settings_search(true, cx);
            settings.open_navigation_editor(persisted_groups, cx);
        });
        self.clear_ime_selection();
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn close_settings_navigation_editor(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace
            .update(cx, SettingsWorkspaceEntity::close_navigation_editor);
        self.clear_standard_confirm_focus();
        cx.notify();
    }

    fn save_settings_navigation_editor(&mut self, cx: &mut Context<Self>) {
        let Some(persisted_groups) = self.settings_workspace.update(cx, |settings, cx| {
            settings.take_navigation_editor_persisted_groups(cx)
        }) else {
            return;
        };
        self.edit_settings(
            move |settings| settings.settings_navigation.groups = persisted_groups,
            cx,
        );
        self.clear_standard_confirm_focus();
        cx.notify();
    }
}
