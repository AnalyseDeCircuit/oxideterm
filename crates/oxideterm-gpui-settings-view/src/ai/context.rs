use std::ops::Deref;

use gpui::{
    AnyElement, App, Div, Entity, MouseButton, MouseDownEvent, Window, div, prelude::*, px, rgb,
    svg,
};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_button,
    },
    checkbox::{CheckboxOptions, CheckboxState, checkbox, checkbox_with_state},
    icons::LucideIcon,
    switch::switch,
};
use oxideterm_settings::PersistedSettings;
use oxideterm_settings_model::{
    AiToolPolicyGroup, AiToolPolicyGroupState, SettingsSectionId, ai_tool_auto_approve_total_count,
    ai_tool_auto_approved_count, ai_tool_policy_groups, set_ai_tool_policy_group_approval,
};

use super::*;
use crate::{
    SettingsInput, SettingsSelect,
    components::{SettingsControls, SettingsWindowRequest},
    entity::SettingsWorkspaceEntity,
    labels::ide_agent_label,
    options::*,
};

pub(crate) struct AiContextPage<'a> {
    pub(crate) controls: SettingsControls<'a>,
    pub(crate) state: &'a Entity<SettingsWorkspaceEntity>,
}

impl<'a> Deref for AiContextPage<'a> {
    type Target = SettingsControls<'a>;
    fn deref(&self) -> &Self::Target {
        &self.controls
    }
}

impl AiContextPage<'_> {
    pub(crate) fn render(&self, section: SettingsSectionId, cx: &App) -> AnyElement {
        let settings = self.controls.settings;
        let card = match section {
            SettingsSectionId::AiContextControls => self.ai_context_controls_section(settings, cx),
            SettingsSectionId::AiContextSystemPrompt => self.ai_system_prompt_section(settings, cx),
            SettingsSectionId::AiContextMemory => self.ai_memory_section(settings, cx),
            SettingsSectionId::AiToolsToolUse => self.ai_tool_use_section(settings, cx),
            _ => unreachable!("not an AI context or policy section"),
        };
        available(card, settings.ai.enabled)
    }

    fn press<F: Fn() -> SettingsWindowRequest + 'static>(
        &self,
        request: F,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<F> {
        let dispatch = self.controls.dispatch.clone();
        move |_, window, cx| {
            dispatch(request(), window, cx);
            cx.stop_propagation();
        }
    }

    fn edit<F: Fn(&mut PersistedSettings) + Clone + 'static>(
        &self,
        edit: F,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<F> {
        self.press(move || SettingsWindowRequest::Edit(Box::new(edit.clone())))
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        toolbar_button(self.tokens, label, icon, options).on_mouse_down(MouseButton::Left, press)
    }

    fn render_lucide_icon(icon: LucideIcon, size: f32, color: gpui::Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }
    fn ai_context_controls_section(&self, settings: &PersistedSettings, cx: &App) -> AnyElement {
        self.settings_card(
            "settings_view.ai.context_controls",
            "settings_view.ai.context_automatic_hint",
            vec![
                self.ai_context_source_row(
                    "settings_view.ai.context_source_ide",
                    "settings_view.ai.context_source_ide_hint",
                    settings.ai.context_sources.ide,
                    set_ai_context_source_ide,
                    cx,
                ),
                self.ai_context_source_row(
                    "settings_view.ai.context_source_sftp",
                    "settings_view.ai.context_source_sftp_hint",
                    settings.ai.context_sources.sftp,
                    set_ai_context_source_sftp,
                    cx,
                ),
            ],
        )
    }

    fn ai_context_source_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        _cx: &App,
    ) -> AnyElement {
        settings_ai_context_source_row(
            self.tokens,
            self.i18n.t(label_key),
            self.i18n.t(hint_key),
            switch(self.tokens, label_key.to_owned(), String::new(), checked).into_any_element(),
        )
        .on_mouse_down(
            MouseButton::Left,
            self.edit(move |settings| setter(settings, !checked)),
        )
        .into_any_element()
    }

    fn ai_system_prompt_section(&self, settings: &PersistedSettings, cx: &App) -> AnyElement {
        let has_custom_prompt = !settings.ai.custom_system_prompt.trim().is_empty();
        self.settings_card(
            "settings_view.ai.system_prompt_title",
            "settings_view.ai.system_prompt_hint",
            vec![self.setting_row(
                if has_custom_prompt {
                    "settings_view.ai.system_prompt_status_custom"
                } else {
                    "settings_view.ai.system_prompt_status_default"
                },
                "",
                self.ai_text_editor_action_button(
                    text_editor::AiTextEditorDialog::SystemPrompt,
                    cx,
                ),
                cx,
            )],
        )
    }

    fn ai_memory_section(&self, settings: &PersistedSettings, cx: &App) -> AnyElement {
        let has_saved_memory = !settings.ai.memory.content.trim().is_empty()
            || settings
                .ai
                .memory
                .entries
                .iter()
                .any(|entry| !entry.content.trim().is_empty());
        self.settings_card(
            "settings_view.ai.memory_title",
            "settings_view.ai.memory_hint",
            vec![
                self.setting_row(
                    if has_saved_memory {
                        "settings_view.ai.memory_status_saved"
                    } else {
                        "settings_view.ai.memory_status_empty"
                    },
                    "",
                    self.ai_text_editor_action_button(text_editor::AiTextEditorDialog::Memory, cx),
                    cx,
                ),
                self.bool_row(
                    "settings_view.ai.memory_enabled",
                    "settings_view.ai.memory_enabled_hint",
                    settings.ai.memory.enabled,
                    set_ai_memory_enabled,
                    cx,
                ),
            ],
        )
    }

    fn ai_text_editor_action_button(
        &self,
        dialog: text_editor::AiTextEditorDialog,
        _cx: &App,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t(match dialog {
                text_editor::AiTextEditorDialog::SystemPrompt => {
                    "settings_view.ai.system_prompt_edit"
                }
                text_editor::AiTextEditorDialog::Memory => "settings_view.ai.memory_edit",
            }),
            Some(Self::render_lucide_icon(
                LucideIcon::Pencil,
                12.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            self.press(move || SettingsWindowRequest::OpenAiTextEditor(dialog)),
        )
        .into_any_element()
    }

    fn ai_tool_use_section(&self, settings: &PersistedSettings, cx: &App) -> AnyElement {
        let approved_count = ai_tool_auto_approved_count(settings);
        let total_count = ai_tool_auto_approve_total_count(settings);
        let policy_groups = settings_ai_tool_policy_grid(
            ai_tool_policy_groups(settings)
                .into_iter()
                .map(|group| self.ai_tool_policy_group(group, cx))
                .collect(),
        );

        let tool_use_expanded = self.state.read(cx).ai_tool_policy_expanded;
        let collapsed_summary = (!tool_use_expanded).then(|| {
            settings_ai_tool_collapsed_summary(
                self.tokens,
                format!(
                    "{} · {}",
                    self.i18n.t("settings_view.ai.tool_use_policy_summary"),
                    self.i18n
                        .t("settings_view.ai.tool_use_collapsed_summary")
                        .replace("{{approved}}", &approved_count.to_string())
                        .replace("{{total}}", &total_count.to_string())
                ),
            )
        });
        let expanded_body = tool_use_expanded.then(|| {
            settings_ai_tool_expanded_body(
                self.tokens,
                settings.ai.tool_use.enabled,
                vec![
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.ai.tool_use_approve_hint"))
                        .into_any_element(),
                    policy_groups,
                    settings_ai_policy_warning(
                        self.tokens,
                        self.i18n.t("settings_view.ai.tool_policy_warning"),
                    ),
                ],
            )
        });

        let controls = self.settings_card_with_header(
            "settings_view.ai.tool_use",
            "",
            Some(Self::render_lucide_icon(
                LucideIcon::Wrench,
                16.0,
                rgb(self.tokens.ui.text_heading),
            )),
            None,
            vec![
                self.select_setting_row(
                    "settings_view.ide.agent_mode_label",
                    "settings_view.ide.agent_mode_hint",
                    SettingsSelect::IdeAgentMode,
                    ide_agent_label(settings.ide.agent_mode, self.i18n),
                    self.tokens.metrics.settings_select_width,
                    cx,
                ),
                self.bool_row(
                    "settings_view.ai.tool_use_enabled",
                    "settings_view.ai.tool_use_enabled_hint",
                    settings.ai.tool_use.enabled,
                    set_ai_tool_use_enabled,
                    cx,
                ),
                self.number_row(
                    "settings_view.ai.tool_use_max_rounds",
                    "settings_view.ai.tool_use_max_rounds_hint",
                    SettingsInput::AiToolUseMaxRounds,
                    settings
                        .ai
                        .tool_use
                        .max_rounds
                        .unwrap_or(oxideterm_settings::DEFAULT_AI_TOOL_MAX_ROUNDS),
                    cx,
                ),
                self.ai_disabled_tools_notice(settings, cx),
            ],
        );
        let policy = self.settings_card_with_header(
            "settings_view.ai.tool_auto_approval_title",
            "",
            None,
            Some(self.ai_tool_expand_button(cx)),
            collapsed_summary.into_iter().chain(expanded_body).collect(),
        );
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_page_gap))
            .child(controls)
            .child(policy)
            .into_any_element()
    }

    fn ai_tool_expand_button(&self, cx: &App) -> AnyElement {
        let expanded = self.state.read(cx).ai_tool_policy_expanded;
        // Tool-policy expand/collapse is an outline small Button in Tauri.
        // Route it through the same shared primitive as other settings
        // command buttons.
        self.workspace_toolbar_action_button(
            if expanded {
                self.i18n.t("settings_view.ai.tool_use_collapse")
            } else {
                self.i18n.t("settings_view.ai.tool_use_expand")
            },
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
            {
                let state = self.state.downgrade();
                move |_, _, cx| {
                    let _ = state.update(cx, |state, cx| {
                        state.ai_tool_policy_expanded = !state.ai_tool_policy_expanded;
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
            },
        )
        .into_any_element()
    }

    fn ai_tool_policy_group(&self, group: AiToolPolicyGroup, _cx: &App) -> AnyElement {
        let group_state = group.state();
        let group_for_toggle = group.clone();
        let next_bulk_value = group.next_bulk_value();
        let group_checkbox_state = match group_state {
            AiToolPolicyGroupState::Locked | AiToolPolicyGroupState::AllApproved => {
                CheckboxState::Checked
            }
            AiToolPolicyGroupState::PartiallyApproved => CheckboxState::Indeterminate,
            AiToolPolicyGroupState::NoneApproved => CheckboxState::Unchecked,
        };
        let mut group_control = checkbox_with_state(
            self.tokens,
            group.title_key,
            String::new(),
            group_checkbox_state,
            CheckboxOptions {
                disabled: group_state == AiToolPolicyGroupState::Locked,
                ..CheckboxOptions::default()
            },
        );
        if let Some(approved) = next_bulk_value {
            group_control = group_control.on_mouse_down(
                MouseButton::Left,
                self.edit(move |settings| {
                    set_ai_tool_policy_group_approval(settings, &group_for_toggle, approved)
                }),
            );
        }

        let mut items = Vec::new();
        for item in group.items {
            let tool_key = item.key.map(str::to_string);
            let checked = item.checked;
            let locked = item.locked;
            let control = checkbox(self.tokens, item.label_key, String::new(), checked)
                .opacity(if locked { 0.5 } else { 1.0 })
                .when(!locked, |checkbox| {
                    checkbox.on_mouse_down(
                        MouseButton::Left,
                        self.edit(move |settings| {
                            if let Some(tool_key) = tool_key.clone() {
                                settings
                                    .ai
                                    .tool_use
                                    .auto_approve_tools
                                    .insert(tool_key, serde_json::json!(!checked));
                            }
                        }),
                    )
                })
                .into_any_element();
            let disabled = item.key.is_some_and(|key| {
                self.settings
                    .ai
                    .tool_use
                    .disabled_tools
                    .iter()
                    .any(|name| name == key)
            });
            let control = if disabled {
                self.text_badge(self.i18n.t("common.disabled"), self.tokens.ui.warning)
            } else if locked {
                self.text_badge(
                    self.i18n.t("settings_view.ai.tool_no_confirmation"),
                    self.tokens.ui.text_muted,
                )
            } else {
                control
            };
            items.push(settings_ai_tool_policy_item(
                self.tokens,
                self.i18n.t(item.label_key),
                control,
            ));
        }

        settings_ai_tool_policy_group(
            self.tokens,
            self.i18n.t(group.title_key),
            self.i18n.t(group.description_key),
            if group_state == AiToolPolicyGroupState::Locked {
                self.text_badge(
                    self.i18n.t("settings_view.ai.tool_fixed_policy"),
                    self.tokens.ui.text_muted,
                )
            } else {
                group_control.into_any_element()
            },
            items,
        )
    }

    fn ai_disabled_tools_notice(&self, settings: &PersistedSettings, _cx: &App) -> AnyElement {
        let count = settings.ai.tool_use.disabled_tools.len();
        if count == 0 {
            return div().into_any_element();
        }
        let labels = ai_tool_policy_groups(settings)
            .into_iter()
            .flat_map(|group| group.items)
            .filter_map(|item| item.key.map(|key| (key, item.label_key)))
            .collect::<std::collections::BTreeMap<_, _>>();
        let tools = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .children(settings.ai.tool_use.disabled_tools.iter().map(|name| {
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        labels
                            .get(name.as_str())
                            .map(|key| self.i18n.t(key))
                            .unwrap_or_else(|| name.clone()),
                    )
            }))
            .into_any_element();
        settings_ai_disabled_tools_notice(
            self.tokens,
            self.i18n
                .t("settings_view.ai.tool_use_disabled_tools_title")
                .replace("{{count}}", &count.to_string()),
            self.i18n.t("settings_view.ai.tool_use_disabled_tools_hint"),
            tools,
            self.workspace_toolbar_action_button(
                self.i18n
                    .t("settings_view.ai.tool_use_restore_disabled_tools"),
                None,
                ToolbarButtonOptions {
                    button: ButtonOptions {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                    ..ToolbarButtonOptions::default()
                },
                self.edit(|settings| settings.ai.tool_use.disabled_tools.clear()),
            )
            .into_any_element(),
        )
    }
}
