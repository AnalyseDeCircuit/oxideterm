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
        self.settings_components().text_field(
            self.settings_ime_frame(input, display_value, focused, cx),
            value.clone(),
            TextInputContentAlign::Center,
            false,
            |input| input.w(px(width)),
            Self::settings_window_dispatch(cx),
        )
    }

    pub(in crate::workspace) fn settings_text_input_control_fill(
        &self,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
        self.settings_controls_for(cx.entity(), cx).field(
            input,
            value.as_ref(),
            placeholder,
            width,
            align,
            secret,
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
        render(self.settings_controls_for(owner, cx), cx)
    }

    pub(super) fn settings_document_text(
        &self,
        owner: Entity<Self>,
        cx: &App,
    ) -> Rc<
        dyn Fn(
            oxideterm_gpui_settings_view::keybinding_page::KeybindingTextRole,
            u64,
            String,
            u32,
        ) -> AnyElement,
    > {
        use oxideterm_gpui_settings_view::keybinding_page::KeybindingTextRole;
        let text = self.selectable_text_render_state_for_entity(owner, cx);
        Rc::new(move |role, id, value, color| {
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
        })
    }

    pub(super) fn settings_controls_for(
        &self,
        owner: Entity<Self>,
        cx: &App,
    ) -> oxideterm_gpui_settings_view::components::SettingsControls<'_> {
        use oxideterm_gpui_settings_view::components::SettingsControls;
        let dispatch = Self::settings_window_dispatch_for(owner.downgrade());
        let text = self.selectable_text_render_state_for_entity(owner, cx);
        SettingsControls {
            components: self.settings_components(),
            open_select: self.open_settings_select,
            select_origin: self.settings_select_focus_origin,
            dispatch,
            ime: self,
            label: Rc::new(move |scope, key, value, color, _| {
                let id = selectable_text::selectable_text_id(scope, (key, value.as_str()));
                text.render_display_text_with_id_in_group(
                    SelectableTextRole::PlainDocument,
                    selectable_text::selectable_document_group_id(),
                    id,
                    0,
                    value,
                    color,
                )
            }),
        }
    }
}

impl oxideterm_gpui_settings_view::components::SettingsImeSource for WorkspaceApp {
    fn frame<'a>(
        &'a self,
        input: SettingsInput,
        value: &'a str,
        cx: &'a App,
    ) -> oxideterm_gpui_settings_view::components::SettingsImeFrame<'a> {
        let workspace = self.settings_workspace.read(cx);
        let ai = self.knowledge_page(cx).read(cx);
        let (display, focused) = if let Some(draft) = workspace.settings_entity_input_value(input) {
            (
                draft,
                workspace.settings_entity_focused_input() == Some(input),
            )
        } else if oxideterm_gpui_ai::knowledge::page::KnowledgePage::owns_settings_input(input) {
            (
                ai.settings_input_value(input).unwrap_or(value),
                ai.focused_settings_input() == Some(input),
            )
        } else if self.focused_settings_input == Some(input) {
            (self.settings_input_draft.as_str(), true)
        } else {
            (value, false)
        };
        self.settings_ime_frame(input, display, focused, cx)
    }
}
