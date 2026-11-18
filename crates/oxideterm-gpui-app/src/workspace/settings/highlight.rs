use super::*;
use oxideterm_gpui_settings_view::highlight_page::{
    HighlightEditor, HighlightPage, HighlightRequest,
};

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

impl WorkspaceApp {
    pub(super) fn blur_highlight_input(
        &mut self,
        owns: fn(SettingsInput) -> bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .settings_workspace
            .read(cx)
            .settings_entity_focused_input()
            .is_some_and(owns)
        {
            self.settings_workspace.update(cx, |settings, cx| {
                settings.blur_settings_entity_input(cx);
            });
            self.clear_ime_selection();
        }
    }

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
        self.blur_highlight_input(is_semantic_scheme_input, cx);
        self.settings_workspace.update(cx, |settings, cx| {
            settings.highlight.semantic_scheme_editor_open = true;
            cx.notify();
        });
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_semantic_scheme_editor(&mut self, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.highlight.semantic_scheme_editor_open = false;
            cx.notify();
        });
        self.blur_highlight_input(is_semantic_scheme_input, cx);
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
        self.blur_highlight_input(is_highlight_rule_input, cx);
        self.settings_workspace.update(cx, |settings, cx| {
            settings.highlight.highlight_rule_editor_index = Some(index);
            cx.notify();
        });
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn open_new_highlight_rule_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(in crate::workspace) fn close_highlight_rule_editor(&mut self, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.highlight.highlight_rule_editor_index = None;
            cx.notify();
        });
        self.blur_highlight_input(is_highlight_rule_input, cx);
        self.ime_marked_text = None;
        cx.notify();
    }

    fn with_highlight_page<R>(
        &self,
        workspace: Entity<Self>,
        cx: &App,
        render: impl FnOnce(HighlightPage<'_>, &App) -> R,
    ) -> R {
        let owner = workspace.downgrade();
        let dispatch = Rc::new(move |request, window: &mut Window, cx: &mut App| {
            let _ = owner.update(cx, |app, cx| match request {
                HighlightRequest::OpenSemantic => app.open_semantic_scheme_editor(window, cx),
                HighlightRequest::CloseSemantic => app.close_semantic_scheme_editor(cx),
                HighlightRequest::CreateSemantic => app.create_semantic_scheme(window, cx),
                HighlightRequest::ImportSemantic => app.import_semantic_scheme(cx),
                HighlightRequest::ExportSemantic => app.export_semantic_scheme(cx),
                HighlightRequest::DeleteSemantic => app.delete_active_semantic_scheme(cx),
                HighlightRequest::NewRule => app.open_new_highlight_rule_editor(window, cx),
                HighlightRequest::OpenRule(index) => {
                    app.open_highlight_rule_editor(index, window, cx)
                }
                HighlightRequest::CloseRule => app.close_highlight_rule_editor(cx),
                HighlightRequest::CreateRuleSet => app.create_highlight_rule_set(cx),
                HighlightRequest::ImportRuleSet => app.import_highlight_rule_set(cx),
                HighlightRequest::ExportRuleSet => app.export_highlight_rule_set(cx),
                HighlightRequest::DeleteRuleSet => app.delete_active_highlight_rule_set(cx),
                HighlightRequest::AddSemanticRule => app.add_semantic_scheme_rule(cx),
                HighlightRequest::ToggleSemanticRule(index) => {
                    app.toggle_semantic_scheme_rule(index, cx)
                }
                HighlightRequest::DeleteSemanticRule(index) => {
                    app.delete_semantic_scheme_rule(index, cx)
                }
                HighlightRequest::Priority(index, delta) => {
                    app.adjust_semantic_scheme_rule_priority(index, delta, cx)
                }
                HighlightRequest::Move(index, direction) => {
                    app.move_highlight_rule(index, direction, cx)
                }
                HighlightRequest::Remove(index) => app.remove_highlight_rule(index, cx),
            });
        });
        self.with_settings_controls_for(workspace, cx, |controls, cx| {
            render(
                HighlightPage {
                    controls,
                    state: &self.settings_workspace.read(cx).highlight,
                    dispatch,
                },
                cx,
            )
        })
    }

    pub(super) fn highlight_selection_card(
        &self,
        settings: &PersistedSettings,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.with_highlight_page(cx.entity(), cx, |page, cx| {
            page.highlight_selection_card(settings, cx)
        })
    }

    pub(super) fn highlight_semantic_card(
        &self,
        settings: &PersistedSettings,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.with_highlight_page(cx.entity(), cx, |page, cx| {
            page.highlight_semantic_card(settings, cx)
        })
    }

    pub(super) fn highlight_rules_card(
        &self,
        settings: &PersistedSettings,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.with_highlight_page(cx.entity(), cx, |page, cx| {
            page.highlight_rules_card(settings, cx)
        })
    }

    pub(in crate::workspace) fn mount_highlight_editor(
        settings: &Entity<SettingsWorkspaceEntity>,
        cx: &mut Context<Self>,
    ) -> Entity<HighlightEditor> {
        let owner = cx.entity().downgrade();
        let render_body = Rc::new(move |cx: &App| {
            let owner = owner.upgrade()?;
            let app = owner.read(cx);
            app.with_highlight_page(owner, cx, |page, cx| {
                let semantic = page.render_semantic_scheme_editor_modal(cx);
                let rule = page.render_highlight_rule_editor_modal(cx);
                Some(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .children(semantic)
                        .children(rule)
                        .into_any_element(),
                )
            })
        });
        cx.new(|cx| HighlightEditor::new(settings, render_body, cx))
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
}
