use super::*;

impl ConnectionPage<'_> {
    pub fn connection_importers_section(&self, cx: &App) -> AnyElement {
        let mut importer = self.state.read(cx).connection_import_snapshot();
        let mut rows = vec![self.connection_import_input_row(importer.source, &importer.paths, cx)];

        if let Some(preview) = importer.preview.take() {
            if importer.source == ConnectionImportSource::XTerminal && !preview.errors.is_empty() {
                rows.push(self.connection_status_row(self.i18n.t_with(
                    "settings_view.connections.importers.xterminal_skipped",
                    &[("count", &preview.errors.len().to_string())],
                )));
            }
            rows.push(self.connection_import_preview_toolbar(
                &preview,
                &importer.selected_draft_ids,
                importer.duplicate_strategy,
                cx,
            ));
            rows.push(self.connection_import_preview_list(
                preview,
                &importer.selected_draft_ids,
                cx,
            ));
        }
        if let Some(status) = importer.status {
            rows.push(self.connection_status_row(status));
        }

        self.connection_section(
            "settings_view.connections.importers.title",
            if importer.source == ConnectionImportSource::XTerminal {
                "settings_view.connections.importers.xterminal_hint"
            } else {
                "settings_view.connections.importers.description"
            },
            rows,
        )
    }

    fn connection_import_input_row(
        &self,
        source: ConnectionImportSource,
        paths: &[String],
        cx: &App,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_start()
            .gap(px(16.0))
            .child(
                div()
                    .min_w_0()
                    .max_w_full()
                    .flex_1()
                    .flex_basis(px(CONNECTION_IMPORT_SOURCE_BASIS))
                    .child(self.connection_import_source_picker(source, cx)),
            )
            .child(
                div()
                    .min_w_0()
                    .max_w_full()
                    .flex_1()
                    .flex_basis(px(CONNECTION_IMPORT_PATH_BASIS))
                    .grid()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.connections.importers.paths")),
                    )
                    .child(self.connection_import_path_toolbar(source, !paths.is_empty(), cx))
                    .child(self.connection_import_path_summary(paths)),
            )
            .into_any_element()
    }

    fn connection_import_source_picker(
        &self,
        source: ConnectionImportSource,
        cx: &App,
    ) -> AnyElement {
        let selected_label = connection_import_source_label(source, &self.i18n);
        div()
            .w_full()
            .min_w_0()
            .grid()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.connections.importers.source")),
            )
            .child(self.settings_select_control(
                SettingsSelect::ConnectionImportSource,
                selected_label,
                false,
                None,
                cx,
            ))
            .into_any_element()
    }

    fn connection_import_path_toolbar(
        &self,
        source: ConnectionImportSource,
        has_paths: bool,
        cx: &App,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(8.0))
            .when(connection_import_supports_files(source), |row| {
                row.child(self.connection_import_pick_files_button(cx))
            })
            .when(connection_import_supports_directory(source), |row| {
                row.child(self.connection_import_pick_directory_button(cx))
            })
            .child(self.connection_import_preview_button(has_paths, cx))
            .into_any_element()
    }

    fn connection_import_pick_files_button(&self, _cx: &App) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n
                .t("settings_view.connections.importers.choose_files"),
            Some(render_icon(
                "lucide/folder-input.svg",
                16.0,
                rgb(self.tokens.ui.text),
            )),
            self.connection_import_secondary_button_options(false),
            self.press(ConnectionRequest::PickImport(false)),
        )
        .into_any_element()
    }

    fn connection_import_pick_directory_button(&self, _cx: &App) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n
                .t("settings_view.connections.importers.choose_directory"),
            Some(render_icon(
                "lucide/folder-open.svg",
                16.0,
                rgb(self.tokens.ui.text),
            )),
            self.connection_import_secondary_button_options(false),
            self.press(ConnectionRequest::PickImport(true)),
        )
        .into_any_element()
    }

    fn connection_import_preview_button(&self, has_paths: bool, _cx: &App) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.connections.importers.preview"),
            Some(render_icon(
                "lucide/refresh-cw.svg",
                16.0,
                rgb(if has_paths {
                    self.tokens.ui.bg
                } else {
                    self.tokens.ui.text_muted
                }),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Default,
                    size: ButtonSize::Default,
                    radius: ButtonRadius::Md,
                    disabled: !has_paths,
                },
                ..ToolbarButtonOptions::default()
            },
            self.press(ConnectionRequest::Local(ConnectionAction::PreviewImport)),
        )
        .into_any_element()
    }

    pub(super) fn connection_import_secondary_button_options(
        &self,
        disabled: bool,
    ) -> ToolbarButtonOptions {
        ToolbarButtonOptions {
            button: ButtonOptions {
                variant: ButtonVariant::Outline,
                size: ButtonSize::Default,
                radius: ButtonRadius::Md,
                disabled,
            },
            background: Some(self.settings_panel_background(self.tokens.ui.bg_panel)),
            border: Some(rgb(self.tokens.ui.border)),
            text_color: Some(rgb(self.tokens.ui.text)),
            hover_background: Some(rgb(self.tokens.ui.bg_hover)),
            height: Some(36.0),
            padding_x: Some(12.0),
            font_size: Some(self.tokens.metrics.ui_text_sm),
            ..ToolbarButtonOptions::default()
        }
    }

    fn connection_import_path_summary(&self, paths: &[String]) -> AnyElement {
        let summary = if paths.is_empty() {
            self.i18n.t("settings_view.connections.importers.no_paths")
        } else {
            paths.join(" · ")
        };
        div()
            .w_full()
            .min_w_0()
            .truncate()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(summary)
            .into_any_element()
    }

    fn connection_import_preview_toolbar(
        &self,
        preview: &ConnectionImportPreview,
        selected_draft_ids: &HashSet<String>,
        duplicate_strategy: ConnectionImportDuplicateStrategy,
        cx: &App,
    ) -> AnyElement {
        let importable = preview
            .drafts
            .iter()
            .filter(|draft| draft.importable)
            .count();
        let all_selected = importable > 0
            && preview
                .drafts
                .iter()
                .filter(|draft| draft.importable)
                .all(|draft| selected_draft_ids.contains(&draft.id));
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .child(self.connection_import_toggle_all_button(all_selected, importable, cx))
            .child(
                div()
                    .min_w_0()
                    .max_w_full()
                    .flex_1()
                    .flex_basis(px(CONNECTION_IMPORT_PREVIEW_ACTIONS_BASIS))
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(self.connection_import_duplicate_strategy_picker(duplicate_strategy, cx))
                    .child(
                        self.settings_text_input_control(
                            SettingsInput::ConnectionImportTargetGroup,
                            String::new(),
                            self.i18n
                                .t("settings_view.connections.importers.target_group"),
                            192.0,
                            cx,
                        )
                        .into_any_element(),
                    )
                    .child(self.connection_import_apply_button(selected_draft_ids.len(), cx)),
            )
            .into_any_element()
    }

    fn connection_import_toggle_all_button(
        &self,
        all_selected: bool,
        importable_count: usize,
        _cx: &App,
    ) -> AnyElement {
        let disabled = importable_count == 0;
        let label = if all_selected {
            self.i18n
                .t("settings_view.connections.importers.deselect_all")
        } else {
            self.i18n
                .t("settings_view.connections.importers.select_all")
        };
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.accent))
            .opacity(if disabled { 0.45 } else { 1.0 })
            .cursor_pointer()
            .hover(|style| style.text_color(rgb(self.tokens.ui.accent_hover)))
            .child(label)
            .when(!disabled, |button| {
                button.on_mouse_down(
                    MouseButton::Left,
                    self.select(move |state, cx| {
                        state.toggle_all_connection_import_drafts(all_selected, cx)
                    }),
                )
            })
            .into_any_element()
    }

    fn connection_import_duplicate_strategy_picker(
        &self,
        strategy: ConnectionImportDuplicateStrategy,
        _cx: &App,
    ) -> AnyElement {
        let selected_label = connection_import_duplicate_strategy_label(strategy, &self.i18n);
        // Tauri renders duplicate strategy as a compact SelectTrigger (w-36 h-8)
        // in the import preview toolbar, not as adjacent action buttons.
        {
            let select = SettingsSelect::ConnectionImportDuplicateStrategy;
            let trigger = self
                .select_trigger(
                    selected_label,
                    false,
                    false,
                    self.open_select == Some(select),
                    self.select_origin,
                )
                .h(px(32.0))
                .text_size(px(self.tokens.metrics.ui_text_xs));
            self.select_control(
                select,
                false,
                Some(144.0),
                trigger,
                self.controls.dispatch.clone(),
            )
        }
    }

    fn connection_import_apply_button(&self, selected_count: usize, _cx: &App) -> AnyElement {
        let label = self
            .i18n
            .t("settings_view.connections.importers.import_selected")
            .replace("{{count}}", &selected_count.to_string());
        self.workspace_toolbar_action_button(
            label,
            Some(render_icon(
                "lucide/upload.svg",
                16.0,
                rgb(if selected_count == 0 {
                    self.tokens.ui.text_muted
                } else {
                    self.tokens.ui.bg
                }),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Default,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: selected_count == 0,
                },
                height: Some(32.0),
                padding_x: Some(10.0),
                font_size: Some(self.tokens.metrics.ui_text_xs),
                ..ToolbarButtonOptions::default()
            },
            self.press(ConnectionRequest::Local(ConnectionAction::ApplyImport)),
        )
        .into_any_element()
    }

    fn connection_import_preview_list(
        &self,
        preview: ConnectionImportPreview,
        selected_draft_ids: &HashSet<String>,
        cx: &App,
    ) -> AnyElement {
        if preview.drafts.is_empty() {
            return div()
                .w_full()
                .h(px(288.0))
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgb(self.tokens.ui.border))
                .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("settings_view.connections.importers.no_drafts"))
                .into_any_element();
        }

        let rows = preview
            .drafts
            .into_iter()
            .map(|draft| {
                let selected = selected_draft_ids.contains(&draft.id);
                self.connection_import_preview_row(draft, selected, cx)
            })
            .collect();
        div()
            .id("settings-connection-import-scroll")
            .w_full()
            .h(px(288.0))
            .overflow_y_scroll()
            .track_scroll(&(self.scroll)("settings-connection-import-scroll"))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
            .child(
                div()
                    .w_full()
                    .py(px(10.0))
                    .children(self.separated_settings_rows(rows)),
            )
            .into_any_element()
    }

    fn connection_import_preview_row(
        &self,
        draft: oxideterm_connections::ImportedConnectionDraft,
        selected: bool,
        _cx: &App,
    ) -> AnyElement {
        let disabled = !draft.importable;
        let detail = format!("{}@{}:{}", draft.username, draft.host, draft.port);
        let origin_detail = [
            draft.group.clone(),
            Some(connection_import_source_label(draft.source, &self.i18n)),
            Some(draft.source_path.clone()),
            if draft.source == ConnectionImportSource::XTerminal {
                draft.key_path.clone()
            } else {
                None
            },
        ]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
        let warnings = draft
            .warnings
            .iter()
            .chain(draft.unsupported_fields.iter())
            .map(|warning| {
                if draft.source == ConnectionImportSource::XTerminal {
                    self.i18n
                        .t(&format!("settings_view.connections.importers.{warning}"))
                } else {
                    warning.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" · ");
        let draft_id = draft.id.clone();
        div()
            .w_full()
            .min_w_0()
            .flex()
            .items_start()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(2.0))
            .opacity(if disabled { 0.5 } else { 1.0 })
            .child(
                div()
                    .w(px(28.0))
                    .flex_none()
                    .child(self.ssh_config_checkbox(selected)),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(draft.name),
                            )
                            .when(draft.duplicate, |row| {
                                row.child(self.connection_import_duplicate_badge())
                            }),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(detail),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(origin_detail),
                    )
                    .when(!warnings.is_empty(), |column| {
                        column.child(
                            div()
                                .truncate()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.warning))
                                .child(warnings),
                        )
                    }),
            )
            .child(
                div()
                    .w(px(CONNECTION_IMPORT_AUTH_WIDTH))
                    .flex_none()
                    .text_align(gpui::TextAlign::Right)
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(imported_auth_label(draft.auth_type, &self.i18n)),
            )
            .when(!disabled, |row| {
                row.cursor_pointer().on_mouse_down(
                    MouseButton::Left,
                    self.select(move |state, cx| {
                        state.toggle_connection_import_draft(draft_id.clone(), cx)
                    }),
                )
            })
            .into_any_element()
    }

    fn connection_import_duplicate_badge(&self) -> AnyElement {
        div()
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((self.tokens.ui.accent << 8) | 0x20))
            .text_size(px(self.tokens.metrics.ui_text_2xs))
            .text_color(rgb(self.tokens.ui.accent))
            .child(self.i18n.t("settings_view.connections.importers.duplicate"))
            .into_any_element()
    }
}

pub fn connection_import_source_options() -> &'static [ConnectionImportSource] {
    &[
        ConnectionImportSource::SecureCrt,
        ConnectionImportSource::Xshell,
        ConnectionImportSource::Termius,
        ConnectionImportSource::MobaXterm,
        ConnectionImportSource::WindTerm,
        ConnectionImportSource::Electerm,
        ConnectionImportSource::FinalShell,
        ConnectionImportSource::XTerminal,
    ]
}

pub fn connection_import_source_label(source: ConnectionImportSource, i18n: &I18n) -> String {
    match source {
        ConnectionImportSource::SecureCrt => {
            i18n.t("settings_view.connections.importers.sources.securecrt")
        }
        ConnectionImportSource::Xshell => {
            i18n.t("settings_view.connections.importers.sources.xshell")
        }
        ConnectionImportSource::Termius => {
            i18n.t("settings_view.connections.importers.sources.termius")
        }
        ConnectionImportSource::MobaXterm => {
            i18n.t("settings_view.connections.importers.sources.mobaxterm")
        }
        ConnectionImportSource::WindTerm => {
            i18n.t("settings_view.connections.importers.sources.windterm")
        }
        ConnectionImportSource::Electerm => {
            i18n.t("settings_view.connections.importers.sources.electerm")
        }
        ConnectionImportSource::FinalShell => {
            i18n.t("settings_view.connections.importers.sources.finalshell")
        }
        ConnectionImportSource::XTerminal => {
            i18n.t("settings_view.connections.importers.sources.xterminal")
        }
    }
}

fn connection_import_supports_files(source: ConnectionImportSource) -> bool {
    source != ConnectionImportSource::FinalShell
}

fn connection_import_supports_directory(source: ConnectionImportSource) -> bool {
    matches!(
        source,
        ConnectionImportSource::SecureCrt
            | ConnectionImportSource::Xshell
            | ConnectionImportSource::FinalShell
    )
}

pub fn connection_import_duplicate_strategy_label(
    strategy: ConnectionImportDuplicateStrategy,
    i18n: &I18n,
) -> String {
    match strategy {
        ConnectionImportDuplicateStrategy::Skip => {
            i18n.t("settings_view.connections.importers.duplicate_skip")
        }
        ConnectionImportDuplicateStrategy::Rename => {
            i18n.t("settings_view.connections.importers.duplicate_rename")
        }
    }
}

pub fn imported_auth_label(auth_type: ImportedConnectionAuthType, _i18n: &I18n) -> String {
    match auth_type {
        ImportedConnectionAuthType::Password => "password",
        ImportedConnectionAuthType::Key => "key",
        ImportedConnectionAuthType::Certificate => "certificate",
        ImportedConnectionAuthType::Agent => "agent",
    }
    .to_string()
}
