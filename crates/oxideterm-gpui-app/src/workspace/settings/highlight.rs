use super::*;
use oxideterm_terminal_semantic::{
    CompiledSemanticScheme, SemanticLineRole, classify_line_with_compiled_scheme,
    semantic_line_emphasis,
};

const SEMANTIC_SCHEME_MODAL_WIDTH: f32 = 820.0;
const SEMANTIC_SCHEME_MODAL_MAX_HEIGHT: f32 = 760.0;
const HIGHLIGHT_RULE_MODAL_WIDTH: f32 = 680.0;
const HIGHLIGHT_RULE_MODAL_MAX_HEIGHT: f32 = 720.0;

pub(in crate::workspace) fn is_semantic_scheme_input(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::SemanticSchemeName
            | SettingsInput::SemanticSchemeRulePattern(_)
            | SettingsInput::SemanticSchemeRuleCapture(_)
            | SettingsInput::SemanticSchemeColor(_)
    )
}

pub(in crate::workspace) fn is_highlight_rule_input(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::HighlightLabel(_)
            | SettingsInput::HighlightPattern(_)
            | SettingsInput::HighlightForeground(_)
            | SettingsInput::HighlightBackground(_)
    )
}

pub(in crate::workspace) fn semantic_class_label(class: SemanticClass, i18n: &I18n) -> String {
    let key = match class {
        SemanticClass::Command => "command",
        SemanticClass::Keyword => "keyword",
        SemanticClass::Option => "option",
        SemanticClass::Operator => "operator",
        SemanticClass::String => "string",
        SemanticClass::Variable => "variable",
        SemanticClass::Comment => "comment",
        SemanticClass::Link => "link",
        SemanticClass::Path => "path",
        SemanticClass::Address => "address",
        SemanticClass::Weekday => "weekday",
        SemanticClass::Month => "month",
        SemanticClass::Timestamp => "timestamp",
        SemanticClass::PermissionRead => "permission_read",
        SemanticClass::PermissionWrite => "permission_write",
        SemanticClass::PermissionExecute => "permission_execute",
        SemanticClass::PermissionSpecial => "permission_special",
        SemanticClass::Number => "number",
        SemanticClass::Error => "error",
        SemanticClass::Warning => "warning",
        SemanticClass::Success => "success",
        SemanticClass::Info => "info",
    };
    i18n.t(&format!(
        "settings_view.terminal.highlight_rules.semantic_class_{key}"
    ))
}

pub(in crate::workspace) fn semantic_context_label(
    context: SemanticRuleContext,
    i18n: &I18n,
) -> String {
    let key = match context {
        SemanticRuleContext::Any => "any",
        SemanticRuleContext::Command => "command",
        SemanticRuleContext::Output => "output",
    };
    i18n.t(&format!(
        "settings_view.terminal.highlight_rules.semantic_context_{key}"
    ))
}

impl WorkspaceApp {
    const HIGHLIGHT_PREVIEW_WRAP_CHARS: usize = 32;

    pub(in crate::workspace) fn open_semantic_scheme_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .settings_store
            .settings()
            .terminal
            .active_custom_semantic_scheme()
            .is_none()
        {
            return;
        }
        self.prepare_modal_interaction_boundary(cx);
        if let Some(input) = self
            .focused_settings_input
            .filter(|input| is_semantic_scheme_input(*input))
        {
            self.focused_settings_input = None;
            self.clear_settings_input_draft(input);
            self.clear_ime_selection();
        }
        self.semantic_scheme_editor_open = true;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_semantic_scheme_editor(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.semantic_scheme_editor_open = false;
        if let Some(input) = self
            .focused_settings_input
            .filter(|input| is_semantic_scheme_input(*input))
        {
            self.focused_settings_input = None;
            self.clear_settings_input_draft(input);
            self.clear_ime_selection();
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(in crate::workspace) fn open_highlight_rule_editor(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index
            >= self
                .settings_store
                .settings()
                .terminal
                .effective_highlight_rules()
                .len()
        {
            return;
        }
        self.prepare_modal_interaction_boundary(cx);
        if let Some(input) = self
            .focused_settings_input
            .filter(|input| is_highlight_rule_input(*input))
        {
            self.focused_settings_input = None;
            self.clear_settings_input_draft(input);
            self.clear_ime_selection();
        }
        self.highlight_rule_editor_index = Some(index);
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn open_new_highlight_rule_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let before_len = self
            .settings_store
            .settings()
            .terminal
            .effective_highlight_rules()
            .len();
        if before_len >= MAX_HIGHLIGHT_RULES {
            return;
        }
        self.add_highlight_rule(cx);
        let after_len = self
            .settings_store
            .settings()
            .terminal
            .effective_highlight_rules()
            .len();
        if after_len > before_len {
            self.open_highlight_rule_editor(after_len - 1, window, cx);
        }
    }

    pub(in crate::workspace) fn close_highlight_rule_editor(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.highlight_rule_editor_index = None;
        if let Some(input) = self
            .focused_settings_input
            .filter(|input| is_highlight_rule_input(*input))
        {
            self.focused_settings_input = None;
            self.clear_settings_input_draft(input);
            self.clear_ime_selection();
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(in crate::workspace) fn render_semantic_scheme_editor_modal(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.semantic_scheme_editor_open {
            return None;
        }
        let settings = self.settings_store.settings();
        let scheme = settings.terminal.active_custom_semantic_scheme()?;
        let semantic_scheme =
            resolved_terminal_semantic_scheme(settings.terminal.semantic_scheme, Some(scheme));
        let dialog = oxideterm_gpui_ui::modal_container(&self.tokens)
            .w(px(SEMANTIC_SCHEME_MODAL_WIDTH))
            .max_w_full()
            .max_h(px(SEMANTIC_SCHEME_MODAL_MAX_HEIGHT))
            .shadow(oxideterm_gpui_ui::theme_overlay_shadow(&self.tokens))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                dialog_header(&self.tokens)
                    .child(dialog_title(&self.tokens, scheme.name.clone()))
                    .child(dialog_description(
                        &self.tokens,
                        self.i18n
                            .t("settings_view.terminal.highlight_rules.semantic_coloring_hint"),
                    )),
            )
            .child(
                oxideterm_gpui_ui::modal::modal_body(&self.tokens)
                    .id("semantic-scheme-editor-modal-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .selectable_overflow_y_scrollbar(
                        &self.selectable_text_scroll_handle("semantic-scheme-editor-modal-scroll"),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(16.0))
                            .child(self.semantic_scheme_editor(scheme, cx))
                            .child(self.card_separator())
                            .child(self.semantic_scheme_preview(&semantic_scheme)),
                    ),
            )
            .child(
                dialog_footer(&self.tokens).justify_end().child(
                    self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.terminal.command_specs_close"),
                        None,
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Default,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                            has_background: true,
                            height: Some(30.0),
                            padding_x: Some(14.0),
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(|this, _event, _window, cx| {
                            this.close_semantic_scheme_editor(cx);
                            cx.stop_propagation();
                        }),
                    ),
                ),
            );

        Some(
            dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_semantic_scheme_editor(cx);
                        cx.stop_propagation();
                    }),
                )
                .child(overlay_content_boundary(dialog))
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_highlight_rule_editor_modal(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let index = self.highlight_rule_editor_index?;
        let rules = self
            .settings_store
            .settings()
            .terminal
            .effective_highlight_rules();
        let rule = rules.get(index)?;
        let title = if rule.label.trim().is_empty() {
            self.i18n
                .t("settings_view.terminal.highlight_rules.untitled_rule")
        } else {
            rule.label.clone()
        };
        let dialog = oxideterm_gpui_ui::modal_container(&self.tokens)
            .w(px(HIGHLIGHT_RULE_MODAL_WIDTH))
            .max_w_full()
            .max_h(px(HIGHLIGHT_RULE_MODAL_MAX_HEIGHT))
            .shadow(oxideterm_gpui_ui::theme_overlay_shadow(&self.tokens))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                dialog_header(&self.tokens)
                    .child(dialog_title(&self.tokens, title))
                    .child(dialog_description(
                        &self.tokens,
                        self.i18n
                            .t("settings_view.terminal.highlight_rules.description"),
                    )),
            )
            .child(
                oxideterm_gpui_ui::modal::modal_body(&self.tokens)
                    .id("highlight-rule-editor-modal-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .selectable_overflow_y_scrollbar(
                        &self.selectable_text_scroll_handle("highlight-rule-editor-modal-scroll"),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(16.0))
                            .child(self.highlight_rule_form(index, rule, cx))
                            .child(self.card_separator())
                            .child(self.highlight_preview(std::slice::from_ref(rule))),
                    ),
            )
            .child(
                dialog_footer(&self.tokens).justify_end().child(
                    self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.terminal.command_specs_close"),
                        None,
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Default,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                            has_background: true,
                            height: Some(30.0),
                            padding_x: Some(14.0),
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(|this, _event, _window, cx| {
                            this.close_highlight_rule_editor(cx);
                            cx.stop_propagation();
                        }),
                    ),
                ),
            );

        Some(
            dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_highlight_rule_editor(cx);
                        cx.stop_propagation();
                    }),
                )
                .child(overlay_content_boundary(dialog))
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn highlight_selection_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_card(
            "terminal.highlight_override.selection_matches",
            "",
            vec![self.bool_row(
                "terminal.highlight_override.selection_matches",
                "terminal.highlight_override.selection_matches_default_hint",
                settings.terminal.selection_highlighting,
                |settings, enabled| settings.terminal.selection_highlighting = enabled,
                cx,
            )],
        )
    }

    pub(in crate::workspace) fn highlight_semantic_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_custom_scheme = settings.terminal.active_custom_semantic_scheme();
        let semantic_scheme_label = active_custom_scheme.map_or_else(
            || terminal_semantic_scheme_label(settings.terminal.semantic_scheme, &self.i18n),
            |scheme| scheme.name.clone(),
        );
        let semantic_scheme = resolved_terminal_semantic_scheme(
            settings.terminal.semantic_scheme,
            active_custom_scheme,
        );

        let semantic_toggle = div()
            .flex_none()
            .child(
                switch(
                    &self.tokens,
                    "semantic-coloring",
                    String::new(),
                    settings.terminal.semantic_coloring,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.edit_settings(
                            |settings| {
                                settings.terminal.semantic_coloring =
                                    !settings.terminal.semantic_coloring;
                            },
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ),
            )
            .into_any_element();

        let semantic_rows = vec![
            self.card_separator(),
            self.select_setting_row(
                "settings_view.terminal.highlight_rules.semantic_scheme",
                "settings_view.terminal.highlight_rules.semantic_scheme_hint",
                SettingsSelect::TerminalSemanticScheme,
                semantic_scheme_label,
                self.tokens.metrics.settings_select_width,
                cx,
            ),
            div()
                .w_full()
                .flex()
                .flex_row()
                .flex_wrap()
                .justify_end()
                .gap(px(8.0))
                .child(self.semantic_scheme_action_button(
                    LucideIcon::Pencil,
                    self.i18n.t("settings_view.terminal.triggers.edit_rule"),
                    active_custom_scheme.is_none(),
                    cx.listener(|this, _event, window, cx| {
                        this.open_semantic_scheme_editor(window, cx);
                        cx.stop_propagation();
                    }),
                ))
                .child(self.semantic_scheme_action_button(
                    LucideIcon::Plus,
                    self.i18n
                        .t("settings_view.terminal.highlight_rules.semantic_scheme_create"),
                    false,
                    cx.listener(|this, _event, window, cx| {
                        this.create_semantic_scheme(window, cx);
                        cx.stop_propagation();
                    }),
                ))
                .child(self.semantic_scheme_action_button(
                    LucideIcon::Download,
                    self.i18n
                        .t("settings_view.terminal.highlight_rules.semantic_scheme_import"),
                    false,
                    cx.listener(|this, _event, _window, cx| {
                        this.import_semantic_scheme(cx);
                        cx.stop_propagation();
                    }),
                ))
                .child(self.semantic_scheme_action_button(
                    LucideIcon::Upload,
                    self.i18n
                        .t("settings_view.terminal.highlight_rules.semantic_scheme_export"),
                    active_custom_scheme.is_none(),
                    cx.listener(|this, _event, _window, cx| {
                        this.export_semantic_scheme(cx);
                        cx.stop_propagation();
                    }),
                ))
                .child(self.semantic_scheme_action_button(
                    LucideIcon::Trash2,
                    self.i18n
                        .t("settings_view.terminal.highlight_rules.semantic_scheme_delete"),
                    active_custom_scheme.is_none(),
                    cx.listener(|this, _event, _window, cx| {
                        this.delete_active_semantic_scheme(cx);
                        cx.stop_propagation();
                    }),
                ))
                .into_any_element(),
            self.card_separator(),
            self.semantic_scheme_preview(&semantic_scheme),
        ];

        self.settings_card_with_header(
            "settings_view.terminal.highlight_rules.semantic_coloring",
            "settings_view.terminal.highlight_rules.semantic_coloring_hint",
            None,
            Some(semantic_toggle),
            semantic_rows,
        )
    }

    pub(in crate::workspace) fn highlight_rules_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rules = settings.terminal.effective_highlight_rules();
        let limit_text = self
            .i18n
            .t("settings_view.terminal.highlight_rules.limit")
            .replace("{{count}}", &MAX_HIGHLIGHT_RULES.to_string());
        let add_disabled = rules.len() >= MAX_HIGHLIGHT_RULES;
        let active_highlight_rule_set = settings
            .terminal
            .default_highlight_rule_set
            .as_deref()
            .and_then(|id| settings.terminal.highlight_rule_set(id));
        let highlight_rule_set_label = active_highlight_rule_set.map_or_else(
            || {
                self.i18n
                    .t("settings_view.terminal.highlight_rules.rule_set_global_base")
            },
            |rule_set| rule_set.name.clone(),
        );

        let rules_header_actions = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(8.0))
            .child(self.settings_select_control(
                SettingsSelect::HighlightPreset,
                self.i18n
                    .t("settings_view.terminal.highlight_rules.add_preset"),
                add_disabled,
                Some(168.0),
                cx,
            ))
            .child(
                // Keep the max-rule guard at dispatch time as well as in the button
                // state so stale UI cannot add a rule past the configured limit.
                self.workspace_toolbar_action_button(
                    self.i18n
                        .t("settings_view.terminal.highlight_rules.add_rule"),
                    Some(Self::render_lucide_icon(
                        LucideIcon::Plus,
                        14.0,
                        rgb(self.tokens.ui.accent_text),
                    )),
                    ToolbarButtonOptions {
                        button: ButtonOptions {
                            variant: ButtonVariant::Default,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: add_disabled,
                        },
                        ..ToolbarButtonOptions::default()
                    },
                    cx.listener(move |this, _event, window, cx| {
                        this.open_new_highlight_rule_editor(window, cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .into_any_element();

        let mut rules_rows = vec![
            self.card_separator(),
            self.select_setting_row(
                "settings_view.terminal.highlight_rules.rule_set",
                "settings_view.terminal.highlight_rules.rule_set_hint",
                SettingsSelect::HighlightRuleSet,
                highlight_rule_set_label,
                self.tokens.metrics.settings_select_width,
                cx,
            ),
            div()
                .w_full()
                .flex()
                .flex_row()
                .flex_wrap()
                .justify_end()
                .gap(px(8.0))
                .child(self.semantic_scheme_action_button(
                    LucideIcon::Plus,
                    self.i18n
                        .t("settings_view.terminal.highlight_rules.rule_set_create"),
                    settings.terminal.highlight_rule_sets.len() >= MAX_HIGHLIGHT_RULE_SETS,
                    cx.listener(|this, _event, _window, cx| {
                        this.create_highlight_rule_set(cx);
                        cx.stop_propagation();
                    }),
                ))
                .child(self.semantic_scheme_action_button(
                    LucideIcon::Download,
                    self.i18n
                        .t("settings_view.terminal.highlight_rules.rule_set_import"),
                    settings.terminal.highlight_rule_sets.len() >= MAX_HIGHLIGHT_RULE_SETS,
                    cx.listener(|this, _event, _window, cx| {
                        this.import_highlight_rule_set(cx);
                        cx.stop_propagation();
                    }),
                ))
                .child(self.semantic_scheme_action_button(
                    LucideIcon::Upload,
                    self.i18n
                        .t("settings_view.terminal.highlight_rules.rule_set_export"),
                    active_highlight_rule_set.is_none(),
                    cx.listener(|this, _event, _window, cx| {
                        this.export_highlight_rule_set(cx);
                        cx.stop_propagation();
                    }),
                ))
                .child(self.semantic_scheme_action_button(
                    LucideIcon::Trash2,
                    self.i18n
                        .t("settings_view.terminal.highlight_rules.rule_set_delete"),
                    active_highlight_rule_set.is_none(),
                    cx.listener(|this, _event, _window, cx| {
                        this.delete_active_highlight_rule_set(cx);
                        cx.stop_propagation();
                    }),
                ))
                .into_any_element(),
        ];
        if let Some(rule_set) = active_highlight_rule_set {
            rules_rows.push(self.highlight_input_block(
                self.i18n
                    .t("settings_view.terminal.highlight_rules.rule_set_name"),
                SettingsInput::HighlightRuleSetName,
                rule_set.name.clone(),
                self.i18n
                    .t("settings_view.terminal.highlight_rules.rule_set_default_name"),
                320.0,
                cx,
            ));
        }
        rules_rows.push(self.card_separator());
        rules_rows.push(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .gap(px(12.0))
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(limit_text)
                .child(
                    self.i18n
                        .t("settings_view.terminal.highlight_rules.priority_hint"),
                )
                .into_any_element(),
        );
        rules_rows.push(self.card_separator());

        if rules.is_empty() {
            rules_rows.push(
                div()
                    .w_full()
                    .px(px(16.0))
                    .py(px(28.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.terminal.highlight_rules.empty"))
                    .into_any_element(),
            );
        } else {
            let summary_rows = rules
                .iter()
                .enumerate()
                .map(|(index, rule)| self.highlight_rule_summary_row(index, rule, rules.len(), cx))
                .collect::<Vec<_>>();
            rules_rows.push(
                div()
                    .w_full()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
                    .px(px(12.0))
                    .py(px(10.0))
                    .flex()
                    .flex_col()
                    .children(self.separated_settings_rows(summary_rows))
                    .into_any_element(),
            );
        }
        rules_rows.push(self.card_separator());
        rules_rows.push(self.highlight_preview(rules));

        self.settings_card_with_header(
            "settings_view.terminal.highlight_rules.title",
            "settings_view.terminal.highlight_rules.description",
            None,
            Some(rules_header_actions),
            rules_rows,
        )
    }

    fn semantic_scheme_action_button(
        &self,
        icon: LucideIcon,
        label: String,
        disabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(
                icon,
                12.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
    }

    fn create_highlight_rule_set(&mut self, cx: &mut Context<Self>) {
        self.close_highlight_rule_editor(cx);
        let name = self
            .i18n
            .t("settings_view.terminal.highlight_rules.rule_set_default_name");
        self.edit_settings(
            |settings| {
                if settings.terminal.highlight_rule_sets.len() >= MAX_HIGHLIGHT_RULE_SETS {
                    return;
                }
                let rule_set = oxideterm_settings::create_highlight_rule_set(name, Vec::new());
                settings.terminal.default_highlight_rule_set = Some(rule_set.id.clone());
                settings.terminal.highlight_rule_sets.push(rule_set);
            },
            cx,
        );
    }

    fn delete_active_highlight_rule_set(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings_store.settings();
        let Some(id) = settings.terminal.default_highlight_rule_set.clone() else {
            return;
        };
        let owner_name = self
            .connection_store
            .connections()
            .iter()
            .find(|connection| {
                connection.options.terminal.highlight_rule_set.as_deref() == Some(&id)
            })
            .map(|connection| connection.name.clone())
            .or_else(|| {
                self.connection_store
                    .telnet_profiles()
                    .iter()
                    .find(|profile| profile.terminal.highlight_rule_set.as_deref() == Some(&id))
                    .map(|profile| profile.name.clone())
            });
        if let Some(owner_name) = owner_name {
            let message = self
                .i18n
                .t("settings_view.terminal.highlight_rules.rule_set_delete_in_use")
                .replace("{{name}}", &owner_name);
            self.send_settings_notice(message, TerminalNoticeVariant::Error, cx);
            return;
        }
        self.close_highlight_rule_editor(cx);
        self.edit_settings(
            |settings| {
                settings
                    .terminal
                    .highlight_rule_sets
                    .retain(|rule_set| rule_set.id != id);
                settings.terminal.default_highlight_rule_set = None;
            },
            cx,
        );
    }

    fn import_highlight_rule_set(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n
                    .t("settings_view.terminal.highlight_rules.rule_set_import"),
            )),
        });
        let runtime = self.forwarding_runtime.handle().clone();
        let workspace = cx.entity().downgrade();
        let success = self
            .i18n
            .t("settings_view.terminal.highlight_rules.rule_set_import_success");
        cx.spawn(async move |_workspace, cx| {
            let path = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let Some(path) = path else {
                return;
            };
            let result = runtime
                .spawn_blocking(move || {
                    let json = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
                    serde_json::from_str::<HighlightRuleSet>(&json)
                        .map_err(|error| error.to_string())
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result);
            let _ = workspace.update(cx, |this, cx| match result {
                Ok(imported) => {
                    let mut imported = sanitize_highlight_rule_sets(vec![imported])
                        .into_iter()
                        .next()
                        .expect("one imported highlight rule set");
                    let imported_name = imported.name.clone();
                    let mut imported_id = None;
                    this.edit_settings(
                        |settings| {
                            if settings.terminal.highlight_rule_sets.len()
                                >= MAX_HIGHLIGHT_RULE_SETS
                            {
                                return;
                            }
                            let rule_set = oxideterm_settings::create_highlight_rule_set(
                                imported_name,
                                std::mem::take(&mut imported.rules),
                            );
                            imported_id = Some(rule_set.id.clone());
                            settings.terminal.highlight_rule_sets.push(rule_set);
                            settings.terminal.default_highlight_rule_set = imported_id.clone();
                        },
                        cx,
                    );
                    if imported_id.is_some() {
                        this.close_highlight_rule_editor(cx);
                        this.send_settings_notice(success, TerminalNoticeVariant::Success, cx);
                    }
                }
                Err(error) => this.send_settings_notice(error, TerminalNoticeVariant::Error, cx),
            });
        })
        .detach();
    }

    fn export_highlight_rule_set(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings_store.settings();
        let Some(id) = settings.terminal.default_highlight_rule_set.as_deref() else {
            return;
        };
        let Some(rule_set) = settings.terminal.highlight_rule_set(id) else {
            return;
        };
        let Ok(json) = serde_json::to_string_pretty(rule_set) else {
            return;
        };
        let file_stem = rule_set
            .name
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '-' {
                    character
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let file_stem = if file_stem.trim_matches('-').is_empty() {
            "highlight-rule-set".to_string()
        } else {
            file_stem
        };
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n
                    .t("settings_view.terminal.highlight_rules.rule_set_export"),
            )),
        });
        let runtime = self.forwarding_runtime.handle().clone();
        let success = self
            .i18n
            .t("settings_view.terminal.highlight_rules.rule_set_export_success");
        cx.spawn(async move |workspace, cx| {
            let directory = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let Some(directory) = directory else {
                return;
            };
            let result = runtime
                .spawn_blocking(move || {
                    let path = directory.join(format!("{file_stem}.oxideterm-highlights.json"));
                    std::fs::write(path, json).map_err(|error| error.to_string())
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result);
            let _ = workspace.update(cx, |this, cx| match result {
                Ok(()) => this.send_settings_notice(success, TerminalNoticeVariant::Success, cx),
                Err(error) => this.send_settings_notice(error, TerminalNoticeVariant::Error, cx),
            });
        })
        .detach();
    }

    fn create_semantic_scheme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .i18n
            .t("settings_view.terminal.highlight_rules.semantic_scheme_default_name");
        let mut result = Ok(String::new());
        self.edit_settings(
            |settings| {
                result = create_custom_semantic_scheme(
                    settings,
                    name,
                    oxideterm_settings::TerminalSemanticScheme::Balanced,
                );
            },
            cx,
        );
        match result {
            Ok(_) => self.open_semantic_scheme_editor(window, cx),
            Err(error) => self.send_settings_notice(error, TerminalNoticeVariant::Error, cx),
        }
    }

    fn delete_active_semantic_scheme(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self
            .settings_store
            .settings()
            .terminal
            .semantic_custom_scheme
            .clone()
        else {
            return;
        };
        let referenced_by = self
            .connection_store
            .connections()
            .iter()
            .find(|connection| {
                connection.options.terminal.semantic_scheme.as_deref() == Some(id.as_str())
            })
            .map(|connection| connection.name.as_str())
            .or_else(|| {
                self.connection_store
                    .telnet_profiles()
                    .iter()
                    .find(|profile| {
                        profile.terminal.semantic_scheme.as_deref() == Some(id.as_str())
                    })
                    .map(|profile| profile.name.as_str())
            });
        if let Some(name) = referenced_by {
            let message = self
                .i18n
                .t("settings_view.terminal.highlight_rules.semantic_scheme_delete_in_use")
                .replace("{{name}}", name);
            self.send_settings_notice(message, TerminalNoticeVariant::Error, cx);
            return;
        }
        self.close_semantic_scheme_editor(cx);
        self.edit_settings(
            |settings| {
                delete_custom_semantic_scheme(settings, &id);
            },
            cx,
        );
    }

    fn import_semantic_scheme(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n
                    .t("settings_view.terminal.highlight_rules.semantic_scheme_import"),
            )),
        });
        let runtime = self.forwarding_runtime.handle().clone();
        let success = self
            .i18n
            .t("settings_view.terminal.highlight_rules.semantic_scheme_import_success");
        cx.spawn(async move |workspace, cx| {
            let path = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let Some(path) = path else {
                return;
            };
            let file_stem = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("Imported Scheme");
            let fallback_name = if file_stem.eq_ignore_ascii_case("scheme") {
                path.parent()
                    .and_then(|parent| parent.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or(file_stem)
                    .to_string()
            } else {
                file_stem.to_string()
            };
            let result = runtime
                .spawn_blocking(move || {
                    std::fs::read_to_string(path).map_err(|error| error.to_string())
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result);
            let _ = workspace.update(cx, |this, cx| match result {
                Ok(json) => {
                    let mut import_result = Ok(String::new());
                    this.edit_settings(
                        |settings| {
                            import_result = import_custom_semantic_scheme_named(
                                settings,
                                &json,
                                &fallback_name,
                            );
                        },
                        cx,
                    );
                    match import_result {
                        Ok(_) => {
                            this.send_settings_notice(success, TerminalNoticeVariant::Success, cx)
                        }
                        Err(error) => {
                            this.send_settings_notice(error, TerminalNoticeVariant::Error, cx)
                        }
                    }
                }
                Err(error) => this.send_settings_notice(error, TerminalNoticeVariant::Error, cx),
            });
        })
        .detach();
    }

    fn export_semantic_scheme(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings_store.settings();
        let Some(id) = settings.terminal.semantic_custom_scheme.as_deref() else {
            return;
        };
        let Ok(json) = export_custom_semantic_scheme(settings, id) else {
            return;
        };
        let file_stem = id
            .trim_start_matches(CUSTOM_SEMANTIC_SCHEME_PREFIX)
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '-' {
                    character
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n
                    .t("settings_view.terminal.highlight_rules.semantic_scheme_export"),
            )),
        });
        let runtime = self.forwarding_runtime.handle().clone();
        let success = self
            .i18n
            .t("settings_view.terminal.highlight_rules.semantic_scheme_export_success");
        cx.spawn(async move |workspace, cx| {
            let directory = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let Some(directory) = directory else {
                return;
            };
            let result = runtime
                .spawn_blocking(move || {
                    let path = directory.join(format!("{file_stem}.oxideterm-scheme.json"));
                    std::fs::write(path, json).map_err(|error| error.to_string())
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result);
            let _ = workspace.update(cx, |this, cx| match result {
                Ok(()) => this.send_settings_notice(success, TerminalNoticeVariant::Success, cx),
                Err(error) => this.send_settings_notice(error, TerminalNoticeVariant::Error, cx),
            });
        })
        .detach();
    }

    fn semantic_scheme_editor(
        &self,
        scheme: &SemanticSchemeDocument,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut editor = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_end()
                    .justify_between()
                    .gap(px(12.0))
                    .child(self.highlight_input_block(
                        self.i18n.t(
                            "settings_view.terminal.highlight_rules.semantic_scheme_name",
                        ),
                        SettingsInput::SemanticSchemeName,
                        scheme.name.clone(),
                        self.i18n.t(
                            "settings_view.terminal.highlight_rules.semantic_scheme_default_name",
                        ),
                        260.0,
                        cx,
                    ))
                    .child(self.semantic_scheme_action_button(
                        LucideIcon::Plus,
                        self.i18n
                            .t("settings_view.terminal.highlight_rules.semantic_rule_add"),
                        scheme.rules.len() >= MAX_SEMANTIC_RULES,
                        cx.listener(|this, _event, _window, cx| {
                            this.add_semantic_scheme_rule(cx);
                            cx.stop_propagation();
                        }),
                    )),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        self.i18n
                            .t("settings_view.terminal.highlight_rules.semantic_colors"),
                    ),
            );

        let mut colors = div().w_full().flex().flex_row().flex_wrap().gap(px(12.0));
        for (index, &class) in SEMANTIC_CLASSES.iter().enumerate() {
            colors = colors.child(
                self.highlight_input_block(
                    semantic_class_label(class, &self.i18n),
                    SettingsInput::SemanticSchemeColor(index),
                    scheme.colors.get(&class).cloned().unwrap_or_default(),
                    self.i18n
                        .t("settings_view.terminal.highlight_rules.semantic_color_theme"),
                    145.0,
                    cx,
                ),
            );
        }
        editor = editor.child(colors).child(
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(
                    self.i18n
                        .t("settings_view.terminal.highlight_rules.semantic_rules"),
                ),
        );
        for (index, rule) in scheme.rules.iter().enumerate() {
            editor = editor.child(self.semantic_scheme_rule_row(index, rule, cx));
        }
        editor.into_any_element()
    }

    fn semantic_scheme_rule_row(
        &self,
        index: usize,
        rule: &SemanticRuleDefinition,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                switch(
                                    &self.tokens,
                                    ("semantic-rule-enabled", index),
                                    String::new(),
                                    rule.enabled,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        this.toggle_semantic_scheme_rule(index, cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .font_family(settings_mono_font_family(
                                        self.settings_store.settings(),
                                    ))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(rule.id.clone()),
                            ),
                    )
                    .child(self.highlight_small_button(
                        self.i18n.t("settings_view.terminal.highlight_rules.delete"),
                        true,
                        move |this, cx| this.delete_semantic_scheme_rule(index, cx),
                        cx,
                    )),
            )
            .child(
                self.highlight_input_block(
                    self.i18n
                        .t("settings_view.terminal.highlight_rules.pattern"),
                    SettingsInput::SemanticSchemeRulePattern(index),
                    rule.pattern.clone(),
                    self.i18n
                        .t("settings_view.terminal.highlight_rules.pattern_placeholder"),
                    520.0,
                    cx,
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_end()
                    .gap(px(12.0))
                    .child(self.settings_select_control(
                        SettingsSelect::SemanticSchemeRuleClass(index),
                        semantic_class_label(rule.class, &self.i18n),
                        false,
                        Some(150.0),
                        cx,
                    ))
                    .child(self.settings_select_control(
                        SettingsSelect::SemanticSchemeRuleContext(index),
                        semantic_context_label(rule.context, &self.i18n),
                        false,
                        Some(150.0),
                        cx,
                    ))
                    .child(
                        self.highlight_input_block(
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.semantic_rule_capture"),
                            SettingsInput::SemanticSchemeRuleCapture(index),
                            rule.capture.to_string(),
                            "0".to_string(),
                            112.0,
                            cx,
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(self.highlight_small_button(
                                "−".to_string(),
                                rule.priority > 0,
                                move |this, cx| {
                                    this.adjust_semantic_scheme_rule_priority(index, -1, cx)
                                },
                                cx,
                            ))
                            .child(
                                div()
                                    .min_w(px(28.0))
                                    .text_align(gpui::TextAlign::Center)
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(rule.priority.to_string()),
                            )
                            .child(self.highlight_small_button(
                                "+".to_string(),
                                rule.priority < u8::MAX,
                                move |this, cx| {
                                    this.adjust_semantic_scheme_rule_priority(index, 1, cx)
                                },
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn add_semantic_scheme_rule(&mut self, cx: &mut Context<Self>) {
        self.edit_settings(
            |settings| {
                let _ = add_custom_semantic_rule(settings);
            },
            cx,
        );
    }

    fn delete_semantic_scheme_rule(&mut self, index: usize, cx: &mut Context<Self>) {
        self.edit_settings(
            |settings| {
                delete_custom_semantic_rule(settings, index);
            },
            cx,
        );
    }

    fn toggle_semantic_scheme_rule(&mut self, index: usize, cx: &mut Context<Self>) {
        self.edit_settings(
            |settings| {
                let _ = edit_custom_semantic_scheme(settings, |scheme| {
                    if let Some(rule) = scheme.rules.get_mut(index) {
                        rule.enabled = !rule.enabled;
                    }
                });
            },
            cx,
        );
    }

    fn adjust_semantic_scheme_rule_priority(
        &mut self,
        index: usize,
        delta: i16,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            |settings| {
                let _ = edit_custom_semantic_scheme(settings, |scheme| {
                    if let Some(rule) = scheme.rules.get_mut(index) {
                        rule.priority = (i16::from(rule.priority) + delta).clamp(0, 255) as u8;
                    }
                });
            },
            cx,
        );
    }

    fn highlight_color_swatch(&self, hex: Option<&str>) -> Option<AnyElement> {
        let color = hex.and_then(parse_color_hex)?;
        Some(
            div()
                .w(px(10.0))
                .h(px(10.0))
                .rounded_full()
                .border_1()
                .border_color(rgb(self.tokens.ui.border))
                .bg(rgb(color))
                .into_any_element(),
        )
    }

    fn highlight_rule_summary_row(
        &self,
        index: usize,
        rule: &HighlightRule,
        total: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = if rule.label.trim().is_empty() {
            self.i18n
                .t("settings_view.terminal.highlight_rules.untitled_rule")
        } else {
            rule.label.clone()
        };
        let mode_label = highlight_render_mode_label(rule.render_mode, &self.i18n);
        let validation_error = highlight_rule_validation_error(rule);
        let fg_swatch = self.highlight_color_swatch(rule.foreground.as_deref());
        let bg_swatch = if rule.render_mode == HighlightRuleRenderMode::Background
            && !rule.preserve_background
        {
            self.highlight_color_swatch(rule.background.as_deref())
        } else {
            None
        };

        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .child(
                switch(
                    &self.tokens,
                    ("highlight-summary-enabled", index),
                    String::new(),
                    rule.enabled,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        let next = !this
                            .settings_store
                            .settings()
                            .terminal
                            .effective_highlight_rules()
                            .get(index)
                            .is_some_and(|rule| rule.enabled);
                        this.edit_settings(
                            move |settings| {
                                if let Some(rule) = settings
                                    .terminal
                                    .effective_highlight_rules_mut()
                                    .get_mut(index)
                                {
                                    rule.enabled = next;
                                }
                            },
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .cursor_pointer()
                    .rounded(px(self.tokens.radii.sm))
                    .px(px(6.0))
                    .py(px(4.0))
                    .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.open_highlight_rule_editor(index, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.0))
                            .when_some(fg_swatch, |row, swatch| row.child(swatch))
                            .when_some(bg_swatch, |row, swatch| row.child(swatch))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(title),
                            )
                            .child(self.text_badge(mode_label, self.tokens.ui.text_muted))
                            .when(rule.is_regex, |row| {
                                row.child(self.text_badge(
                                    self.i18n.t("settings_view.terminal.highlight_rules.regex"),
                                    self.tokens.ui.text_muted,
                                ))
                            })
                            .when_some(validation_error, |row, _| {
                                row.child(self.text_badge(
                                    self.i18n
                                        .t("settings_view.terminal.highlight_rules.invalid_rule"),
                                    self.tokens.ui.warning,
                                ))
                            }),
                    )
                    .child(
                        div()
                            .truncate()
                            .font_family(settings_mono_font_family(self.settings_store.settings()))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(summarize_highlight_pattern(&rule.pattern)),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.0))
                    .child(self.highlight_small_button(
                        "↑".to_string(),
                        index > 0,
                        move |this, cx| this.move_highlight_rule(index, -1, cx),
                        cx,
                    ))
                    .child(self.highlight_small_button(
                        "↓".to_string(),
                        index + 1 < total,
                        move |this, cx| this.move_highlight_rule(index, 1, cx),
                        cx,
                    ))
                    .child(
                        self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.terminal.triggers.edit_rule"),
                            None,
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Outline,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: false,
                                },
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(move |this, _event, window, cx| {
                                this.open_highlight_rule_editor(index, window, cx);
                                cx.stop_propagation();
                            }),
                        ),
                    )
                    .child(self.highlight_small_button(
                        self.i18n.t("settings_view.terminal.highlight_rules.delete"),
                        true,
                        move |this, cx| this.remove_highlight_rule(index, cx),
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn highlight_rule_form(
        &self,
        index: usize,
        rule: &HighlightRule,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mode_label = highlight_render_mode_label(rule.render_mode, &self.i18n);
        let scope_label = highlight_match_scope_label(rule.match_scope, &self.i18n);
        let validation_error = highlight_rule_validation_error(rule);

        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_end()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(220.0))
                            .child(self.highlight_fill_input_block(
                                self.i18n.t("settings_view.terminal.highlight_rules.label"),
                                SettingsInput::HighlightLabel(index),
                                rule.label.clone(),
                                self.i18n
                                    .t("settings_view.terminal.highlight_rules.label_placeholder"),
                                cx,
                            )),
                    )
                    .child(self.highlight_render_mode_control(index, mode_label, cx))
                    .child(self.highlight_match_scope_control(index, scope_label, cx)),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(self.highlight_fill_input_block(
                        self.i18n
                            .t("settings_view.terminal.highlight_rules.pattern"),
                        SettingsInput::HighlightPattern(index),
                        rule.pattern.clone(),
                        self.i18n
                            .t("settings_view.terminal.highlight_rules.pattern_placeholder"),
                        cx,
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(if validation_error.is_some() {
                                rgb(self.tokens.ui.warning)
                            } else {
                                rgb(self.tokens.ui.text_muted)
                            })
                            .child(
                                validation_error
                                    .map(|reason| {
                                        self.i18n.t(&format!(
                                            "settings_view.terminal.highlight_rules.validation.{reason}"
                                        ))
                                    })
                                    .unwrap_or_else(|| {
                                        self.i18n.t(if rule.is_regex {
                                            "settings_view.terminal.highlight_rules.mode_hint.regex"
                                        } else {
                                            "settings_view.terminal.highlight_rules.mode_hint.literal"
                                        })
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_end()
                    .gap(px(12.0))
                    .child(
                        self.highlight_input_block(
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.foreground"),
                            SettingsInput::HighlightForeground(index),
                            rule.foreground.clone().unwrap_or_default(),
                            "#f8fafc".to_string(),
                            150.0,
                            cx,
                        ),
                    )
                    .when(
                        rule.render_mode != HighlightRuleRenderMode::Background
                            || !rule.preserve_background,
                        |row| {
                            row.child(
                                self.highlight_input_block(
                                    self.i18n
                                        .t("settings_view.terminal.highlight_rules.background"),
                                    SettingsInput::HighlightBackground(index),
                                    rule.background.clone().unwrap_or_default(),
                                    "#991b1b".to_string(),
                                    150.0,
                                    cx,
                                ),
                            )
                        },
                    )
                    .when(
                        rule.render_mode == HighlightRuleRenderMode::Background,
                        |row| {
                            row.child(
                                div().pb(px(5.0)).child(self.highlight_checkbox(
                                    ("highlight-preserve-background", index),
                                    self.i18n.t(
                                        "settings_view.terminal.highlight_rules.preserve_background",
                                    ),
                                    rule.preserve_background,
                                    move |settings, value| {
                                        if let Some(rule) = settings
                                            .terminal
                                            .effective_highlight_rules_mut()
                                            .get_mut(index)
                                        {
                                            rule.preserve_background = value;
                                        }
                                    },
                                    cx,
                                )),
                            )
                        },
                    ),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap(px(16.0))
                    .child(
                        self.highlight_checkbox(
                            ("highlight-enabled", index),
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.enabled"),
                            rule.enabled,
                            move |settings, value| {
                                if let Some(rule) = settings
                                    .terminal
                                    .effective_highlight_rules_mut()
                                    .get_mut(index)
                                {
                                    rule.enabled = value;
                                }
                            },
                            cx,
                        ),
                    )
                    .child(self.highlight_checkbox(
                        ("highlight-regex", index),
                        self.i18n.t("settings_view.terminal.highlight_rules.regex"),
                        rule.is_regex,
                        move |settings, value| {
                            if let Some(rule) = settings
                                .terminal
                                .effective_highlight_rules_mut()
                                .get_mut(index)
                            {
                                rule.is_regex = value;
                            }
                        },
                        cx,
                    ))
                    .child(
                        self.highlight_checkbox(
                            ("highlight-case-sensitive", index),
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.case_sensitive"),
                            rule.case_sensitive,
                            move |settings, value| {
                                if let Some(rule) = settings
                                    .terminal
                                    .effective_highlight_rules_mut()
                                    .get_mut(index)
                                {
                                    rule.case_sensitive = value;
                                }
                            },
                            cx,
                        ),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn highlight_small_button(
        &self,
        label: String,
        enabled: bool,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Rule-row ghost Buttons mirror TerminalHighlightRulesSection.tsx and
        // share the same disabled action guard as other settings Buttons.
        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: !enabled,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, _window, cx| {
                action(this, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn highlight_input_block(
        &self,
        label: String,
        input: SettingsInput,
        value: String,
        placeholder: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(label),
            )
            .child(self.settings_text_input_control(input, value, placeholder, width, cx))
            .into_any_element()
    }

    fn highlight_fill_input_block(
        &self,
        label: String,
        input: SettingsInput,
        value: String,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(label),
            )
            .child(self.settings_text_input_control_fill(input, value, placeholder, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn highlight_render_mode_control(
        &self,
        index: usize,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let select_id = SettingsSelect::HighlightRenderMode(index);
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(
                        self.i18n
                            .t("settings_view.terminal.highlight_rules.render_mode"),
                    ),
            )
            .child(self.settings_select_control(select_id, value, false, Some(148.0), cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn highlight_match_scope_control(
        &self,
        index: usize,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let select_id = SettingsSelect::HighlightMatchScope(index);
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(
                        self.i18n
                            .t("settings_view.terminal.highlight_rules.match_scope"),
                    ),
            )
            .child(self.settings_select_control(select_id, value, false, Some(148.0), cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn highlight_checkbox(
        &self,
        id: impl Into<gpui::ElementId>,
        label: String,
        checked: bool,
        setter: impl Fn(&mut PersistedSettings, bool) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        checkbox(&self.tokens, id, label, checked)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(
                        |settings| {
                            setter(settings, !checked);
                            let rules = settings.terminal.effective_highlight_rules_mut();
                            *rules = reindex_highlight_rules(rules.clone());
                        },
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn semantic_scheme_preview(&self, scheme: &CompiledSemanticScheme) -> AnyElement {
        // Preview rendering uses the runtime classifier and theme mapping so edits have immediate fidelity.
        let lines = [
            self.i18n
                .t("settings_view.terminal.highlight_rules.preview_line_error"),
            self.i18n
                .t("settings_view.terminal.highlight_rules.preview_line_warning"),
            self.i18n
                .t("settings_view.terminal.highlight_rules.preview_line_ok"),
            self.i18n
                .t("settings_view.terminal.highlight_rules.preview_line_trace"),
            self.i18n
                .t("settings_view.terminal.highlight_rules.preview_line_audit"),
        ];
        let mut sample = div()
            .w_full()
            .min_w(px(0.0))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.terminal.background))
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .line_height(px(24.0))
            .text_color(rgb(self.tokens.terminal.foreground));
        let theme = TerminalUiTheme::from_tokens(self.tokens);
        for line in lines {
            sample = sample.child(self.semantic_scheme_preview_line(&line, scheme, &theme));
        }

        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(
                                self.i18n
                                    .t("settings_view.terminal.highlight_rules.semantic_preview"),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_align(gpui::TextAlign::Right)
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(
                                self.i18n.t(
                                    "settings_view.terminal.highlight_rules.semantic_preview_hint",
                                ),
                            ),
                    ),
            )
            .child(sample)
            .into_any_element()
    }

    fn semantic_scheme_preview_line(
        &self,
        line: &str,
        scheme: &CompiledSemanticScheme,
        theme: &TerminalUiTheme,
    ) -> AnyElement {
        let role = SemanticLineRole::Output;
        let spans = classify_line_with_compiled_scheme(line, role, scheme);
        let band = semantic_line_emphasis(line, role)
            .filter(|class| spans.iter().any(|span| span.class == *class))
            .and_then(|class| terminal_semantic_line_band(theme, class));
        let mut row = div()
            .w_full()
            .min_w(px(0.0))
            .px(px(6.0))
            .rounded(px(self.tokens.radii.xs))
            .flex()
            .flex_row()
            .flex_wrap()
            .overflow_hidden()
            .when_some(band, |row, (color, opacity)| {
                row.bg(rgba((color << 8) | alpha_byte(opacity)))
            });
        let mut cursor = 0;
        for span in spans {
            if span.range.start > cursor {
                row = self.highlight_preview_plain_chunks(row, &line[cursor..span.range.start]);
            }
            let color =
                terminal_semantic_variant_color(theme, span.class, span.style_variant, scheme);
            for chunk in Self::highlight_preview_wrapping_chunks(&line[span.range.clone()]) {
                row = row.child(div().text_color(rgb(color)).child(chunk));
            }
            cursor = span.range.end;
        }
        if cursor < line.len() {
            row = self.highlight_preview_plain_chunks(row, &line[cursor..]);
        }
        row.into_any_element()
    }

    pub(in crate::workspace) fn highlight_preview(&self, rules: &[HighlightRule]) -> AnyElement {
        let lines = [
            self.i18n
                .t("settings_view.terminal.highlight_rules.preview_line_error"),
            self.i18n
                .t("settings_view.terminal.highlight_rules.preview_line_warning"),
            self.i18n
                .t("settings_view.terminal.highlight_rules.preview_line_ok"),
            self.i18n
                .t("settings_view.terminal.highlight_rules.preview_line_trace"),
            self.i18n
                .t("settings_view.terminal.highlight_rules.preview_line_audit"),
        ];
        let mut preview = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(
                                self.i18n
                                    .t("settings_view.terminal.highlight_rules.preview"),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_align(gpui::TextAlign::Right)
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(
                                self.i18n
                                    .t("settings_view.terminal.highlight_rules.preview_hint"),
                            ),
                    ),
            );
        let mut sample = div()
            .w_full()
            .min_w(px(0.0))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.terminal.background))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .line_height(px(24.0))
            .text_color(rgb(self.tokens.terminal.foreground));
        for line in lines {
            sample = sample.child(self.highlight_preview_line(&line, rules));
        }
        preview = preview.child(sample);
        preview.into_any_element()
    }

    pub(in crate::workspace) fn highlight_preview_line(
        &self,
        line: &str,
        rules: &[HighlightRule],
    ) -> AnyElement {
        let matches = accepted_highlight_preview_matches(line, rules);
        let mut row = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .flex_wrap()
            .overflow_hidden();
        let mut cursor = 0;
        for matched in matches {
            if matched.start > cursor {
                row = self.highlight_preview_plain_chunks(row, &line[cursor..matched.start]);
            }
            for chunk in Self::highlight_preview_wrapping_chunks(&line[matched.start..matched.end])
            {
                row = row.child(highlight_preview_segment(
                    &self.tokens,
                    &chunk,
                    matched.rule,
                ));
            }
            cursor = matched.end;
        }
        if cursor < line.len() {
            row = self.highlight_preview_plain_chunks(row, &line[cursor..]);
        }
        row.into_any_element()
    }

    pub(in crate::workspace) fn highlight_preview_plain_chunks(
        &self,
        mut row: Div,
        text: &str,
    ) -> Div {
        for chunk in Self::highlight_preview_wrapping_chunks(text) {
            row = row.child(chunk);
        }
        row
    }

    pub(in crate::workspace) fn highlight_preview_wrapping_chunks(text: &str) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut current_chars = 0usize;

        for ch in text.chars() {
            current.push(ch);
            current_chars += 1;
            if ch.is_whitespace() || current_chars >= Self::HIGHLIGHT_PREVIEW_WRAP_CHARS {
                chunks.push(std::mem::take(&mut current));
                current_chars = 0;
            }
        }

        if !current.is_empty() {
            chunks.push(current);
        }

        chunks
    }
}
