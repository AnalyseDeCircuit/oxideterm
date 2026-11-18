use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn number_input(
        &self,
        input: SettingsInput,
        value: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.as_str()
        } else {
            value.as_str()
        };
        let frame = self.settings_ime_frame(input, display_value, focused, cx);
        self.settings_components().text_field(
            frame,
            value.to_owned(),
            TextInputContentAlign::Center,
            false,
            |input| input.w(px(width)),
            Self::settings_window_dispatch(cx),
        )
    }

    pub(in crate::workspace) fn settings_text_input_control(
        &self,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_text_input_control_inner(
            input,
            value,
            placeholder,
            Some(width),
            TextInputContentAlign::Start,
            false,
            cx,
        )
    }

    pub(in crate::workspace) fn settings_text_input_control_fill(
        &self,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Responsive form fields own their width at the parent flex layer.
        // Filling that slot avoids fixed-width inputs inside growing columns.
        self.settings_text_input_control_inner(
            input,
            value,
            placeholder,
            None,
            TextInputContentAlign::Start,
            false,
            cx,
        )
    }

    pub(in crate::workspace) fn settings_text_input_control_inner(
        &self,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        width: Option<f32>,
        align: TextInputContentAlign,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_settings_field(
            input,
            value.as_ref(),
            placeholder,
            width,
            align,
            secret,
            Self::settings_window_dispatch(cx),
            cx,
        )
    }

    pub(super) fn with_settings_controls<R>(
        &self,
        cx: &Context<Self>,
        render: impl FnOnce(oxideterm_gpui_settings_view::components::SettingsControls<'_>, &App) -> R,
    ) -> R {
        self.with_settings_controls_for(cx.entity(), cx, render)
    }

    pub(super) fn with_settings_controls_for<R>(
        &self,
        owner: Entity<Self>,
        cx: &App,
        render: impl FnOnce(oxideterm_gpui_settings_view::components::SettingsControls<'_>, &App) -> R,
    ) -> R {
        use oxideterm_gpui_settings_view::components::SettingsControls;
        let dispatch = Self::settings_window_dispatch_for(owner.downgrade());
        let field = |input, value: &str, placeholder, width, align, secret, cx: &App| {
            self.render_settings_field(
                input,
                value,
                placeholder,
                width,
                align,
                secret,
                dispatch.clone(),
                cx,
            )
        };
        let text = self.selectable_text_render_state_for_entity(owner, cx);
        let label = |scope: &str, key: &str, value: String, color, _: &App| {
            let id = selectable_text::selectable_text_id(scope, (key, value.as_str()));
            text.render_display_text_with_id_in_group(
                SelectableTextRole::PlainDocument,
                selectable_text::selectable_document_group_id(),
                id,
                0,
                value,
                color,
            )
        };
        render(
            SettingsControls {
                components: self.settings_components(),
                open_select: self.open_settings_select,
                select_origin: self.settings_select_focus_origin,
                dispatch: dispatch.clone(),
                field: &field,
                label: &label,
            },
            cx,
        )
    }

    fn render_settings_field(
        &self,
        input: SettingsInput,
        value: &str,
        placeholder: String,
        width: Option<f32>,
        align: TextInputContentAlign,
        secret: bool,
        dispatch: oxideterm_gpui_settings_view::components::SettingsWindowDispatch,
        cx: &App,
    ) -> AnyElement {
        let settings_workspace = self.settings_workspace.read(cx);
        let entity_value = settings_workspace.settings_entity_input_value(input);
        let settings_entity_owned = entity_value.is_some();
        let ai_entity_owned = ai_state::AiWorkspaceEntity::owns_settings_input(input);
        let ai_entity = self.ai_entity.read(cx);
        let ai_entity_value = ai_entity_owned
            .then(|| ai_entity.settings_input_value(input))
            .flatten();
        let focused = if settings_entity_owned {
            settings_workspace.settings_entity_focused_input() == Some(input)
        } else if ai_entity_owned {
            ai_entity.focused_settings_input() == Some(input)
        } else {
            self.focused_settings_input == Some(input)
        };
        let display_value = if let Some(entity_value) = entity_value {
            // Borrow Entity-owned drafts for this frame instead of cloning them
            // into root rendering snapshots, especially for secret inputs.
            entity_value
        } else if let Some(ai_entity_value) = ai_entity_value {
            // Knowledge and AI dialogs keep their drafts in AiWorkspaceEntity.
            // Render from that same owner so IME replacements are visible immediately.
            ai_entity_value
        } else if focused {
            self.settings_input_draft.as_str()
        } else {
            value
        };
        let frame = self.settings_ime_frame(input, display_value, focused, cx);
        self.settings_components().text_field(
            frame,
            placeholder,
            align,
            secret,
            |input| {
                input
                    .when_some(width, |input, width| input.w(px(width)).max_w_full())
                    .when(width.is_none(), |input| input.w_full())
                    .min_w(px(0.0))
            },
            dispatch,
        )
    }
}
