use crate::options::set_ai_skills_enabled;
use gpui::{
    AnyElement, App, Div, Entity, KeyDownEvent, MouseButton, MouseDownEvent, SharedString, Window,
    div, prelude::*, px, rgb, svg,
};
use oxideterm_gpui_ui::{
    button::{ToolbarButtonOptions, toolbar_button},
    checkbox::checkbox,
    entity_row::{EntityListRowOptions, entity_list_row},
    icons::LucideIcon,
};
use oxideterm_settings::PersistedSettings;
use std::{ops::Deref, path::Path, rc::Rc};

use super::{AiSkillAction, AiSkillData};
use crate::{SettingsInput, components::SettingsControls, entity::SettingsWorkspaceEntity};
use oxideterm_skills::{
    SkillDiagnostic, SkillDiagnosticKind, SkillOrigin, SkillRecord, SkillScope,
};

fn visible_skills<'a>(
    records: &'a [SkillRecord],
    query: &str,
    scope: Option<SkillScope>,
) -> Vec<&'a SkillRecord> {
    let query = query.trim().to_lowercase();
    records
        .iter()
        .filter(|record| {
            scope.is_none_or(|scope| record.scope == scope)
                && (query.is_empty()
                    || record.name.to_lowercase().contains(&query)
                    || record.description.to_lowercase().contains(&query))
        })
        .collect()
}

fn skill_scope_key(scope: SkillScope) -> &'static str {
    match scope {
        SkillScope::Workspace => "settings_view.ai.skills_scope_workspace",
        SkillScope::User => "settings_view.ai.skills_scope_user",
        SkillScope::Plugin => "settings_view.ai.skills_scope_plugin",
    }
}

fn set_skill_enabled(settings: &mut PersistedSettings, path: &Path, enabled: bool) {
    let disabled = &mut settings.ai.skills.disabled_paths;
    // Discovery accepts both a SKILL.md path and its directory as a disabled entry.
    // Enabling removes either reference; the skill files themselves are never changed.
    disabled.retain(|entry| {
        let entry = Path::new(entry);
        entry != path && Some(entry) != path.parent()
    });
    if !enabled {
        disabled.push(path.to_string_lossy().into_owned());
    }
}

pub(crate) struct AiSkillsPage<'a> {
    pub(crate) controls: SettingsControls<'a>,
    pub(crate) state: &'a Entity<SettingsWorkspaceEntity>,
    pub(crate) data: AiSkillData,
    pub(crate) dispatch: Rc<dyn Fn(AiSkillAction, &mut App)>,
}

impl<'a> Deref for AiSkillsPage<'a> {
    type Target = SettingsControls<'a>;

    fn deref(&self) -> &Self::Target {
        &self.controls
    }
}

impl AiSkillsPage<'_> {
    fn action<
        F: Fn(&mut SettingsWorkspaceEntity, &mut gpui::Context<SettingsWorkspaceEntity>) + 'static,
    >(
        &self,
        action: F,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<F> {
        let state = self.state.downgrade();
        move |_, _, cx| {
            let _ = state.update(cx, |state, cx| action(state, cx));
            cx.stop_propagation();
        }
    }

    fn key_action<
        F: Fn(&mut SettingsWorkspaceEntity, &mut gpui::Context<SettingsWorkspaceEntity>) + 'static,
    >(
        &self,
        action: F,
    ) -> impl Fn(&KeyDownEvent, &mut Window, &mut App) + use<F> {
        let state = self.state.downgrade();
        move |event, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                let _ = state.update(cx, |state, cx| action(state, cx));
                cx.stop_propagation();
            }
        }
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        action: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        toolbar_button(self.tokens, label, icon, options).on_mouse_down(MouseButton::Left, action)
    }

    fn icon(icon: LucideIcon, size: f32, color: gpui::Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    fn submit<F: Fn() -> AiSkillAction + 'static>(
        &self,
        request: F,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<F> {
        let dispatch = self.dispatch.clone();
        move |_, _, cx| {
            dispatch(request(), cx);
            cx.stop_propagation();
        }
    }

    pub(crate) fn render(&self, cx: &App) -> AnyElement {
        let records = &self.data.records;
        let diagnostics = &self.data.diagnostics;
        let state = self.state.read(cx);
        let visible = visible_skills(records, &state.skill_search_query, state.skill_scope_filter);
        let enabled_count = records.iter().filter(|record| record.enabled).count();
        let diagnostics_expanded = state.skill_diagnostics_expanded;
        let refresh = self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.ai.skills_refresh"),
            Some(Self::icon(
                LucideIcon::RefreshCw,
                14.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions::default(),
            self.submit(|| AiSkillAction::Refresh),
        );
        let mut section = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .child(self.bool_row(
                "settings_view.ai.skills_enabled",
                "settings_view.ai.skills_enabled_hint",
                self.settings.ai.skills.enabled,
                set_ai_skills_enabled,
                cx,
            ))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(
                                self.i18n
                                    .t("settings_view.ai.skills_summary")
                                    .replace("{{enabled}}", &enabled_count.to_string())
                                    .replace("{{total}}", &records.len().to_string()),
                            ),
                    )
                    .when(!diagnostics.is_empty(), |row| {
                        row.child(
                            self.workspace_toolbar_action_button(
                                self.i18n
                                    .t("settings_view.ai.skills_issues")
                                    .replace("{{count}}", &diagnostics.len().to_string()),
                                Some(Self::icon(
                                    LucideIcon::AlertTriangle,
                                    14.0,
                                    rgb(self.tokens.ui.warning),
                                )),
                                ToolbarButtonOptions::default(),
                                self.action(|state, cx| {
                                    state.skill_diagnostics_expanded =
                                        !state.skill_diagnostics_expanded;
                                    cx.notify();
                                }),
                            ),
                        )
                    }),
            );
        if diagnostics_expanded && !diagnostics.is_empty() {
            section = section.child(self.ai_skill_diagnostics(diagnostics, cx));
        }
        section = section.child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(12.0))
                .child(div().min_w_0().flex_1().flex_basis(px(240.0)).child(
                    self.settings_text_input_control_fill(
                        SettingsInput::SkillSearch,
                        "",
                        self.i18n.t("settings_view.ai.skills_search"),
                        cx,
                    ),
                ))
                .child(self.ai_skill_scope_filter(cx)),
        );
        if visible.is_empty() {
            section = section.child(
                div()
                    .py(px(20.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(if records.is_empty() {
                        "settings_view.ai.skills_empty"
                    } else {
                        "settings_view.ai.skills_no_matches"
                    })),
            );
        } else {
            let mut list = div().w_full().min_w_0().flex().flex_col();
            for (index, record) in visible.into_iter().enumerate() {
                if index > 0 {
                    list = list.child(self.card_separator());
                }
                list = list.child(self.ai_skill_row(record, cx));
            }
            section = section.child(list);
        }
        self.settings_card_with_header(
            "settings_view.ai.skills_title",
            "",
            None,
            Some(refresh.into_any_element()),
            vec![section.into_any_element()],
        )
    }

    fn ai_skill_scope_filter(&self, cx: &App) -> AnyElement {
        let selected = self.state.read(cx).skill_scope_filter;
        let mut filters = div().flex().flex_wrap().gap(px(4.0));
        for scope in [
            None,
            Some(SkillScope::Workspace),
            Some(SkillScope::User),
            Some(SkillScope::Plugin),
        ] {
            let label = scope
                .map(skill_scope_key)
                .unwrap_or("settings_view.ai.skills_all");
            filters = filters.child(
                oxideterm_gpui_ui::segmented_control_item(
                    &self.tokens,
                    self.i18n.t(label),
                    selected == scope,
                )
                .id(label)
                .role(gpui::Role::Tab)
                .aria_label(self.i18n.t(label))
                .aria_selected(selected == scope)
                .focusable()
                .tab_stop(true)
                .focus_visible(|style| style.border_1().border_color(rgb(self.tokens.ui.accent)))
                .on_mouse_down(
                    MouseButton::Left,
                    self.action(move |state, cx| {
                        state.skill_scope_filter = scope;
                        cx.notify();
                    }),
                )
                .on_key_down(self.key_action(move |state, cx| {
                    state.skill_scope_filter = scope;
                    cx.notify();
                })),
            );
        }
        filters.into_any_element()
    }

    fn toggle_details(
        state: &mut SettingsWorkspaceEntity,
        path: &Path,
        cx: &mut gpui::Context<SettingsWorkspaceEntity>,
    ) {
        if !state.expanded_skill_paths.remove(path) {
            state.expanded_skill_paths.insert(path.to_path_buf());
        }
        cx.notify();
    }

    fn ai_skill_row(&self, record: &SkillRecord, cx: &App) -> AnyElement {
        let expanded = self
            .state
            .read(cx)
            .expanded_skill_paths
            .contains(&record.skill_path);
        let path = record.skill_path.clone();
        let toggle_path = path.clone();
        let keyboard_path = path.clone();
        let checked = record.enabled;
        let origin = match record.origin {
            SkillOrigin::AgentStandard => ".agents",
            SkillOrigin::OxideTerm => "OxideTerm",
            SkillOrigin::ClaudeCompatible => ".claude",
            SkillOrigin::CopilotCompatible => ".github",
            SkillOrigin::OpenCodeCompatible => ".opencode",
            SkillOrigin::Plugin => "skills/",
        };
        let title = div()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(record.name.clone()),
            )
            .child(self.text_badge(
                format!(
                    "{} · {}",
                    self.i18n.t(skill_scope_key(record.scope)),
                    origin
                ),
                self.tokens.ui.text_muted,
            ));
        let status_key = if checked {
            "common.enabled"
        } else {
            "common.disabled"
        };
        let toggle = checkbox(
            &self.tokens,
            format!("skill-enabled-{}", path.display()),
            String::new(),
            checked,
        )
        .id(SharedString::from(format!(
            "skill-enabled-{}",
            path.display()
        )))
        .role(gpui::Role::CheckBox)
        .aria_toggled(if checked {
            gpui::Toggled::True
        } else {
            gpui::Toggled::False
        })
        .focusable()
        .tab_stop(true)
        .focus_visible(|style| style.border_1().border_color(rgb(self.tokens.ui.accent)))
        .aria_label(format!("{} · {}", record.name, self.i18n.t(status_key)))
        .on_mouse_down(
            MouseButton::Left,
            self.submit(move || {
                let path = toggle_path.clone();
                AiSkillAction::EditAndRefresh(Box::new(move |settings| {
                    set_skill_enabled(settings, &path, !checked)
                }))
            }),
        )
        .on_key_down({
            let dispatch = self.dispatch.clone();
            move |event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    let path = keyboard_path.clone();
                    dispatch(
                        AiSkillAction::EditAndRefresh(Box::new(move |settings| {
                            set_skill_enabled(settings, &path, !checked)
                        })),
                        cx,
                    );
                    cx.stop_propagation();
                }
            }
        });
        let keyboard_path = path.clone();
        let row = entity_list_row(
            &self.tokens,
            EntityListRowOptions::new().has_background_image(self.has_background),
            Some(oxideterm_gpui_ui::motion::animated_chevron(
                self.tokens,
                (
                    SharedString::from(format!("skill-chevron-{}", path.display())),
                    expanded as usize,
                ),
                svg()
                    .path(LucideIcon::ChevronRight.path())
                    .size(px(14.0))
                    .text_color(rgb(self.tokens.ui.text_muted)),
                expanded,
            )),
            title.into_any_element(),
            Some(
                div()
                    .min_w_0()
                    .when(!expanded, |description| description.line_clamp(2))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(18.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(record.description.clone())
                    .into_any_element(),
            ),
            vec![],
            vec![toggle.into_any_element()],
        )
        .id(SharedString::from(format!("skill-row-{}", path.display())))
        .role(gpui::Role::Button)
        .aria_label(record.name.clone())
        .aria_expanded(expanded)
        .focusable()
        .tab_stop(true)
        .focus_visible(|style| style.border_1().border_color(rgb(self.tokens.ui.accent)))
        .cursor_pointer()
        .on_mouse_down(
            MouseButton::Left,
            self.action(move |state, cx| Self::toggle_details(state, &path, cx)),
        )
        .on_key_down(
            self.key_action(move |state, cx| Self::toggle_details(state, &keyboard_path, cx)),
        );
        let mut content = div().w_full().min_w_0().child(row);
        if expanded {
            let path = record.skill_path.clone();
            let details = div()
                .w_full()
                .min_w_0()
                .px(px(12.0))
                .pb(px(12.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(div().child((self.label)(
                    "skill-file-path",
                    &record.id,
                    record.skill_path.display().to_string(),
                    self.tokens.ui.text_muted,
                    cx,
                )))
                .when_some(record.compatibility.as_ref(), |details, compatibility| {
                    details.child(
                        div().child(
                            self.i18n
                                .t("settings_view.ai.skills_compatibility")
                                .replace("{{value}}", compatibility),
                        ),
                    )
                })
                .child(self.workspace_toolbar_action_button(
                    self.i18n.t("settings_view.ai.skills_open_folder"),
                    Some(Self::icon(
                        LucideIcon::FolderOpen,
                        14.0,
                        rgb(self.tokens.ui.text_muted),
                    )),
                    ToolbarButtonOptions::default(),
                    move |_, _, cx| {
                        cx.reveal_path(&path);
                        cx.stop_propagation();
                    },
                ));
            content = content.child(details);
        }
        content.into_any_element()
    }

    fn ai_skill_diagnostics(&self, diagnostics: &[SkillDiagnostic], cx: &App) -> AnyElement {
        let mut list = div().w_full().min_w_0().flex().flex_col().gap(px(12.0));
        for diagnostic in diagnostics {
            let key = match diagnostic.kind {
                SkillDiagnosticKind::Invalid => "settings_view.ai.skills_invalid",
                SkillDiagnosticKind::Unreadable => "settings_view.ai.skills_unreadable",
                SkillDiagnosticKind::Shadowed => "settings_view.ai.skills_shadowed",
            };
            list = list.child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .child(
                        div()
                            .text_color(rgb(self.tokens.ui.warning))
                            .child(self.i18n.t(key)),
                    )
                    .child((self.label)(
                        "skill-diagnostic-path",
                        &diagnostic.path.to_string_lossy(),
                        diagnostic.path.display().to_string(),
                        self.tokens.ui.text_muted,
                        cx,
                    ))
                    .child(
                        div()
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(diagnostic.message.clone()),
                    ),
            );
        }
        list.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn record(name: &str, description: &str, scope: SkillScope, enabled: bool) -> SkillRecord {
        SkillRecord {
            id: name.into(),
            name: name.into(),
            description: description.into(),
            compatibility: None,
            scope,
            origin: SkillOrigin::AgentStandard,
            skill_path: PathBuf::from(name).join("SKILL.md"),
            root: PathBuf::from(name),
            content_hash: String::new(),
            enabled,
            priority: 1,
        }
    }

    #[test]
    fn skill_manager_search_combines_scope_with_name_or_description() {
        let records = [
            record("review", "Review Rust changes", SkillScope::Workspace, true),
            record("deploy", "Review deployment steps", SkillScope::User, false),
            record("notes", "整理会议记录", SkillScope::Plugin, true),
        ];
        for (query, scope, expected) in [
            (" REVIEW ", None, vec!["review", "deploy"]),
            ("review", Some(SkillScope::User), vec!["deploy"]),
            ("记录", Some(SkillScope::Plugin), vec!["notes"]),
            ("记录", Some(SkillScope::Workspace), vec![]),
            ("", None, vec!["review", "deploy", "notes"]),
        ] {
            let names = visible_skills(&records, query, scope)
                .into_iter()
                .map(|record| record.name.as_str())
                .collect::<Vec<_>>();
            assert_eq!(names, expected, "query={query:?}, scope={scope:?}");
        }
    }

    #[test]
    fn skill_manager_toggle_changes_only_disabled_references() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("SKILL.md");
        let source = "---\nname: review\ndescription: Review changes\n---\nKeep the original instructions.\n";
        std::fs::write(&path, source).unwrap();
        let mut settings = PersistedSettings::default();
        settings.ai.skills.disabled_paths = vec!["another-skill/SKILL.md".into()];
        set_skill_enabled(&mut settings, &path, false);
        set_skill_enabled(&mut settings, &path, false);
        assert_eq!(
            settings.ai.skills.disabled_paths,
            vec![
                "another-skill/SKILL.md".to_string(),
                path.to_string_lossy().into_owned(),
            ]
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
        settings
            .ai
            .skills
            .disabled_paths
            .push(directory.path().to_string_lossy().into_owned());
        set_skill_enabled(&mut settings, &path, true);
        assert_eq!(
            settings.ai.skills.disabled_paths,
            vec!["another-skill/SKILL.md"]
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    }
}
