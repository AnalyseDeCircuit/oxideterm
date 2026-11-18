use super::*;
use crate::workspace::ime::WorkspaceImeTarget;
use oxideterm_gpui_settings_view::{
    components::SettingsWindowDispatch,
    portable_runtime::dialog::{PortablePasswordDialog, PortablePasswordFrame},
};

impl WorkspaceApp {
    pub(in crate::workspace) fn mount_portable_password_dialog(
        settings: &Entity<SettingsWorkspaceEntity>,
        cx: &mut Context<Self>,
    ) -> Entity<PortablePasswordDialog> {
        let owner = cx.entity().downgrade();
        let frame = Rc::new({
            let owner = owner.clone();
            move |cx: &App| {
                let owner = owner.upgrade()?;
                let app = owner.read(cx);
                Some(PortablePasswordFrame {
                    tokens: app.tokens,
                    i18n: app.i18n.clone(),
                    focused_action: app.standard_confirm_focus(),
                })
            }
        });
        let field = Rc::new({
            let owner = owner.clone();
            let dispatch = Self::settings_window_dispatch_for(owner.clone());
            move |input, cx: &App| {
                let Some(app) = owner.upgrade() else {
                    return div().into_any_element();
                };
                app.read(cx)
                    .portable_entity_password_input(input, dispatch.clone(), cx)
            }
        });
        let dispatch = Rc::new(move |submit, cx: &mut App| {
            let _ = owner.update(cx, |app, cx| {
                app.clear_standard_confirm_focus();
                if submit {
                    app.submit_portable_password_change(cx);
                } else {
                    app.close_portable_password_change_dialog(cx);
                }
            });
        });
        cx.new(|cx| PortablePasswordDialog::new(settings.clone(), frame, field, dispatch, cx))
    }

    fn portable_entity_password_input(
        &self,
        input: SettingsInput,
        dispatch: SettingsWindowDispatch,
        cx: &App,
    ) -> AnyElement {
        let settings = self.settings_workspace.read(cx);
        let display_value = settings
            .settings_entity_input_value(input)
            .expect("portable inputs are owned by the Settings Entity");
        let focused = settings.settings_entity_focused_input() == Some(input);
        let frame = self.settings_ime_frame(input, display_value, focused, cx);
        self.settings_components().text_field(
            frame,
            String::new(),
            TextInputContentAlign::Start,
            true,
            |input| input.w_full(),
            dispatch,
        )
    }

    pub(in crate::workspace) fn portable_password_field(
        &self,
        label_key: &str,
        input: SettingsInput,
        value: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.portable_password_input(input, value, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn portable_password_input(
        &self,
        input: SettingsInput,
        value: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.portable_password_input_with_placeholder(input, value, String::new(), cx)
    }

    pub(in crate::workspace) fn portable_password_input_with_placeholder(
        &self,
        input: SettingsInput,
        value: &str,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.as_str()
        } else {
            value
        };
        let target = WorkspaceImeTarget::Settings(input);
        self.text_input_with_workspace_ime(
            target,
            self.workspace_text_input(
                target,
                TextInputView {
                    value: display_value,
                    placeholder,
                    focused,
                    caret_visible: self.input_caret.visible(),
                    secret: true,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
            )
            .w_full(),
            move |this, cx| {
                // App Lock moves the secret into the root IME adapter instead
                // of creating a second focused draft.
                this.focus_settings_input(input, String::new(), cx);
            },
            cx,
        )
        .into_any_element()
    }
}
