
// Tauri ReconnectTab uses `max-w-2xl` for the switch row, select grids, and hint card.
const SETTINGS_RECONNECT_MAX_WIDTH: f32 = 672.0;
const KNOWLEDGE_MAX_IMPORT_FILE_SIZE: u64 = 5 * 1024 * 1024;
const KNOWLEDGE_EMBEDDING_BATCH_SIZE: usize = 32;
const KNOWLEDGE_DIALOG_WIDTH: f32 = 520.0;
const KNOWLEDGE_ACTION_BUTTON_HEIGHT: f32 = 28.0; // Tauri size="sm" outline action buttons.
const KNOWLEDGE_ICON_BUTTON_SIZE: f32 = 28.0; // Tauri h-7 w-7 document row buttons.
const KNOWLEDGE_INLINE_ICON_SIZE: f32 = 14.0; // Tauri h-3.5 w-3.5 action icons.
const KNOWLEDGE_ROW_ICON_SIZE: f32 = 16.0; // Tauri h-4 w-4 row icons.
const KNOWLEDGE_EMBEDDING_ICON_BOX: f32 = 32.0; // Tauri h-8 w-8 semantic search icon box.
const KNOWLEDGE_EMBEDDING_CONFIG_BUTTON_HEIGHT: f32 = 32.0; // Tauri h-8 configure button.
const KNOWLEDGE_SECTION_BORDER_ALPHA: u32 = 0x80; // Tauri border-theme-border/50.
const KNOWLEDGE_SECTION_BG_ALPHA: u32 = 0xcc; // Tauri bg-card/80.
const KNOWLEDGE_SECTION_DIVIDER_ALPHA: u32 = 0x66; // Tauri border-theme-border/40.
const KNOWLEDGE_STATUS_BORDER_ALPHA: u32 = 0x33; // Tauri border-current/20.
const KNOWLEDGE_STATUS_BG_ALPHA: u32 = 0x1a; // Tauri bg-current/10.
const KNOWLEDGE_ICON_BUTTON_HOVER_ALPHA: u32 = 0x0d; // Tauri hover:bg-theme-bg-hover/5.

impl WorkspaceApp {
    fn settings_local(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        let mut shell_rows = vec![self.local_shell_select_row(settings, cx)];
        if let Some(path_hint) = self.local_shell_path_hint(settings) {
            shell_rows.push(path_hint);
        }
        shell_rows.push(self.card_separator());
        shell_rows.push(
            self.setting_row(
                "settings_view.local_terminal.git_bash_path",
                "settings_view.local_terminal.git_bash_path_hint",
                self.settings_text_input_control(
                    SettingsInput::LocalGitBashPath,
                    settings
                        .local_terminal
                        .git_bash_path
                        .clone()
                        .unwrap_or_default(),
                    self.i18n
                        .t("settings_view.local_terminal.git_bash_path_placeholder"),
                    300.0,
                    cx,
                ),
            ),
        );
        shell_rows.push(self.card_separator());
        shell_rows.push(
            self.setting_row(
                "settings_view.local_terminal.default_cwd",
                "settings_view.local_terminal.default_cwd_hint",
                self.settings_text_input_control(
                    SettingsInput::LocalDefaultCwd,
                    settings
                        .local_terminal
                        .default_cwd
                        .clone()
                        .unwrap_or_default(),
                    "~".to_string(),
                    self.tokens.metrics.settings_select_width,
                    cx,
                ),
            ),
        );

        let mut oh_my_posh_rows = vec![self.checkbox_row(
            "settings_view.local_terminal.oh_my_posh_enable",
            "settings_view.local_terminal.oh_my_posh_enable_hint",
            settings.local_terminal.oh_my_posh_enabled,
            set_oh_my_posh,
            cx,
        )];
        if settings.local_terminal.oh_my_posh_enabled {
            oh_my_posh_rows.push(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(rgba((self.tokens.ui.info << 8) | 0x33))
                    .bg(rgba((self.tokens.ui.info << 8) | 0x1a))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.info))
                            .child(format!(
                                "💡 {}",
                                self.i18n.t("settings_view.local_terminal.oh_my_posh_note")
                            )),
                    )
                    .into_any_element(),
            );
            oh_my_posh_rows.push(self.card_separator());
            oh_my_posh_rows.push(
                self.setting_row(
                    "settings_view.local_terminal.oh_my_posh_theme",
                    "settings_view.local_terminal.oh_my_posh_theme_hint",
                    self.settings_text_input_control(
                        SettingsInput::LocalOhMyPoshTheme,
                        settings
                            .local_terminal
                            .oh_my_posh_theme
                            .clone()
                            .unwrap_or_default(),
                        self.i18n
                            .t("settings_view.local_terminal.oh_my_posh_theme_placeholder"),
                        300.0,
                        cx,
                    ),
                ),
            );
        }

        let shortcut_default = if cfg!(target_os = "macos") {
            "⌘T"
        } else {
            "Ctrl+T"
        };
        let shortcut_launcher = if cfg!(target_os = "macos") {
            "⌘⇧T"
        } else {
            "Ctrl+Shift+T"
        };

        let effective_shells = self.effective_local_shells_for_settings(settings);
        let shell_list = if effective_shells.is_empty() {
            vec![
                div()
                    .text_align(gpui::TextAlign::Center)
                    .py(px(32.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.local_terminal.loading_shells"))
                    .into_any_element(),
            ]
        } else {
            effective_shells
                .iter()
                .map(|shell| {
                    self.available_shell_row(
                        shell,
                        settings.local_terminal.default_shell_id.as_deref(),
                    )
                })
                .collect()
        };

        vec![
            self.settings_card(
                "settings_view.local_terminal.shell",
                "settings_view.local_terminal.default_shell_hint",
                shell_rows,
            ),
            self.settings_card(
                "settings_view.local_terminal.shell_profile",
                "settings_view.local_terminal.load_shell_profile_hint",
                vec![self.checkbox_row(
                    "settings_view.local_terminal.load_shell_profile",
                    "settings_view.local_terminal.load_shell_profile_hint",
                    settings.local_terminal.load_shell_profile,
                    set_load_shell_profile,
                    cx,
                )],
            ),
            self.settings_card(
                "settings_view.local_terminal.oh_my_posh",
                "settings_view.local_terminal.oh_my_posh_note",
                oh_my_posh_rows,
            ),
            self.settings_card(
                "settings_view.local_terminal.shortcuts",
                "settings_view.local_terminal.custom_env_hint",
                vec![
                    self.local_shortcut_row(
                        "settings_view.local_terminal.new_default_shell",
                        shortcut_default,
                    ),
                    self.card_separator(),
                    self.local_shortcut_row(
                        "settings_view.local_terminal.new_shell_launcher",
                        shortcut_launcher,
                    ),
                ],
            ),
            self.settings_card(
                "settings_view.local_terminal.available_shells",
                "settings_view.local_terminal.select_shell",
                shell_list,
            ),
        ]
    }

    fn settings_reconnect(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        let reconnect_enabled = settings.reconnect.enabled;
        vec![
            self.reconnect_enabled_row(reconnect_enabled, cx),
            separator(&self.tokens, SeparatorOrientation::Horizontal).into_any_element(),
            div()
                .flex()
                .flex_col()
                .gap(px(24.0))
                .opacity(if reconnect_enabled { 1.0 } else { 0.4 })
                .child(
                    div()
                        .text_size(px(18.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(self.tokens.ui.text_heading))
                        .child(self.i18n.t("settings_view.reconnect.strategy")),
                )
                .child(
                    div()
                        .w_full()
                        .max_w(px(SETTINGS_RECONNECT_MAX_WIDTH))
                        .grid()
                        .grid_cols(2)
                        .gap(px(32.0))
                        .child(self.reconnect_select_field(
                            "settings_view.reconnect.max_attempts",
                            "settings_view.reconnect.max_attempts_hint",
                            SettingsSelect::ReconnectMaxAttempts,
                            reconnect_attempt_label(settings.reconnect.max_attempts),
                            reconnect_enabled,
                            cx,
                        ))
                        .child(self.reconnect_select_field(
                            "settings_view.reconnect.base_delay",
                            "settings_view.reconnect.base_delay_hint",
                            SettingsSelect::ReconnectBaseDelay,
                            reconnect_delay_label(settings.reconnect.base_delay_ms),
                            reconnect_enabled,
                            cx,
                        )),
                )
                .child(
                    div()
                        .w_full()
                        .max_w(px(SETTINGS_RECONNECT_MAX_WIDTH))
                        .grid()
                        .grid_cols(2)
                        .gap(px(32.0))
                        .child(self.reconnect_select_field(
                            "settings_view.reconnect.max_delay",
                            "settings_view.reconnect.max_delay_hint",
                            SettingsSelect::ReconnectMaxDelay,
                            reconnect_delay_label(settings.reconnect.max_delay_ms),
                            reconnect_enabled,
                            cx,
                        )),
                )
                .child(
                    div()
                        .max_w(px(SETTINGS_RECONNECT_MAX_WIDTH))
                        .p(px(16.0))
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
                        .bg(self.settings_panel_background(self.tokens.ui.bg_card))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.reconnect.formula_hint")),
                )
                .into_any_element(),
        ]
    }

    fn reconnect_enabled_row(&self, checked: bool, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .max_w(px(SETTINGS_RECONNECT_MAX_WIDTH))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .grid()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.reconnect.enabled")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.reconnect.enabled_hint")),
                    ),
            )
            .child(
                checkbox(&self.tokens, String::new(), checked)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.edit_settings(
                                |settings| set_reconnect_enabled(settings, !checked),
                                cx,
                            );
                        }),
                    )
                    .into_any_element(),
            )
            .into_any_element()
    }

    fn reconnect_select_field(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        value: String,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_id = select_id.anchor_id();
        let workspace = cx.entity();
        let trigger = select_trigger(&self.tokens, value, false, !enabled);
        let trigger = if enabled {
            trigger.cursor_pointer().on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.focused_settings_input = None;
                    this.open_settings_select = if this.open_settings_select == Some(select_id) {
                        None
                    } else {
                        Some(select_id)
                    };
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
        } else {
            trigger
        };

        div()
            .w_full()
            .min_w(px(0.0))
            .grid()
            .gap(px(8.0))
            .child(
                div()
                    .grid()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(label_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(hint_key)),
                    ),
            )
            .child(select_anchor_probe(
                anchor_id,
                trigger,
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ))
            .when(!enabled, |field| {
                field.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
            })
            .into_any_element()
    }

    fn settings_ai(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        vec![self.ai_settings_surface(cx)]
    }

    fn settings_knowledge(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let collections = oxideterm_ai::rag_list_collections(&self.ai_rag_store, None)
            .unwrap_or_default();
        let selected_id = self
            .knowledge_selected_collection_id
            .as_deref()
            .filter(|id| collections.iter().any(|collection| collection.id == *id))
            .map(str::to_string)
            .or_else(|| collections.first().map(|collection| collection.id.clone()));
        let selected_collection = selected_id
            .as_deref()
            .and_then(|id| collections.iter().find(|collection| collection.id == id));
        let selected_documents = selected_id
            .as_deref()
            .and_then(|id| {
                oxideterm_ai::rag_list_documents(&self.ai_rag_store, id, None, Some(100)).ok()
            });
        let selected_stats = selected_id
            .as_deref()
            .and_then(|id| oxideterm_ai::rag_get_collection_stats(&self.ai_rag_store, id).ok());

        let mut rows = vec![self.knowledge_collections_card(&collections, selected_id.as_deref(), cx)];
        if let Some(error) = self.knowledge_error.as_ref() {
            rows.insert(0, self.knowledge_error_row(error));
        }
        if let Some(collection) = selected_collection {
            rows.push(self.knowledge_documents_card(
                collection,
                selected_documents,
                selected_stats,
                cx,
            ));
        }
        rows
    }

    fn knowledge_error_row(&self, error: &str) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba((self.tokens.ui.error << 8) | 0x4d))
            .bg(rgba((self.tokens.ui.error << 8) | 0x1a))
            .p(px(12.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.error))
            .child(error.to_string())
            .into_any_element()
    }

    fn knowledge_collections_card(
        &self,
        collections: &[oxideterm_ai::RagCollectionResponse],
        selected_id: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut rows = vec![
            div()
                .flex()
                .items_center()
                .justify_end()
                .child(
                    button_with(
                        &self.tokens,
                        self.i18n.t("settings_view.knowledge.create_collection"),
                        ButtonOptions {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: false,
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.knowledge_create_dialog_open = true;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .into_any_element(),
                )
                .into_any_element(),
        ];
        if collections.is_empty() {
            rows.push(self.knowledge_empty_row(
                LucideIcon::BookOpen,
                self.i18n.t("settings_view.knowledge.no_collections"),
            ));
        } else {
            for collection in collections {
                rows.push(self.knowledge_collection_row(collection, selected_id, cx));
            }
        }
        self.settings_card(
            "settings_view.knowledge.collections",
            "settings_view.knowledge.create_description",
            rows,
        )
    }

    fn knowledge_documents_card(
        &self,
        collection: &oxideterm_ai::RagCollectionResponse,
        documents: Option<oxideterm_ai::RagPaginatedDocuments>,
        stats: Option<oxideterm_ai::RagStatsResponse>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let reindex_collection_id = collection.id.clone();
        let import_collection_id = collection.id.clone();
        let embedding_collection_id = collection.id.clone();
        let documents = documents.map(|page| page.documents).unwrap_or_default();
        let import_label = self
            .knowledge_import_progress
            .map(|(current, total)| format!("{current}/{total}"))
            .unwrap_or_else(|| self.i18n.t("settings_view.knowledge.import_files"));
        let embedding_label = self
            .knowledge_embedding_progress
            .map(|(current, total)| format!("{current}/{total}"))
            .unwrap_or_else(|| self.i18n.t("settings_view.knowledge.generate_embeddings"));
        let reindex_label = self
            .knowledge_reindex_progress
            .map(|(current, total)| {
                if total == 0 {
                    self.i18n.t("settings_view.knowledge.reindex")
                } else {
                    format!("{current}/{total}")
                }
            })
            .unwrap_or_else(|| self.i18n.t("settings_view.knowledge.reindex"));
        let mut rows = vec![
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(self.tokens.ui.text))
                                .child(collection.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.knowledge_scope_label(&collection.scope)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            self.knowledge_text_icon_button(
                                LucideIcon::FolderOpen,
                                import_label,
                                self.knowledge_import_progress.is_some(),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, window, cx| {
                                    this.knowledge_import_files(
                                        import_collection_id.clone(),
                                        window,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            ),
                        )
                        .child(
                            self.knowledge_text_icon_button(
                                LucideIcon::FilePlus,
                                self.i18n.t("settings_view.knowledge.new_document"),
                                false,
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.knowledge_new_document_dialog_open = true;
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                        )
                        .child(
                            self.knowledge_text_icon_button(
                                LucideIcon::Sparkles,
                                embedding_label,
                                self.knowledge_embedding_progress.is_some(),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.knowledge_generate_embeddings(
                                        embedding_collection_id.clone(),
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            ),
                        )
                        .child(
                            self.knowledge_text_icon_button(
                                if self.knowledge_reindex_progress.is_some() {
                                    LucideIcon::X
                                } else {
                                    LucideIcon::RefreshCw
                                },
                                reindex_label,
                                matches!(self.knowledge_reindex_progress, Some((_current, 0))),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    if this.knowledge_reindex_progress.is_some() {
                                        this.knowledge_cancel_reindex(cx);
                                    } else {
                                        this.knowledge_reindex(reindex_collection_id.clone(), cx);
                                    }
                                    cx.stop_propagation();
                                }),
                            ),
                        ),
                )
                .into_any_element(),
        ];
        rows.push(self.knowledge_embedding_config_section(cx));
        if let Some(stats) = stats {
            rows.push(self.knowledge_stats_row(stats));
        }
        rows.push(self.card_separator());
        if documents.is_empty() {
            rows.push(self.knowledge_empty_row(
                LucideIcon::FileText,
                self.i18n.t("settings_view.knowledge.no_documents"),
            ));
        } else {
            for document in documents {
                rows.push(self.knowledge_document_row(document, cx));
            }
        }
        self.settings_card(
            "settings_view.knowledge.title",
            "settings_view.knowledge.description",
            rows,
        )
    }

    fn knowledge_collection_row(
        &self,
        collection: &oxideterm_ai::RagCollectionResponse,
        selected_id: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = selected_id == Some(collection.id.as_str());
        let collection_id = collection.id.clone();
        let delete_id = collection.id.clone();
        let delete_name = collection.name.clone();
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if selected {
                rgba((self.tokens.ui.accent << 8) | 0x4d)
            } else {
                rgba(0x00000000)
            })
            .bg(if selected {
                rgba((self.tokens.ui.accent << 8) | 0x1a)
            } else {
                rgba(0x00000000)
            })
            .px(px(12.0))
            .py(px(8.0))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.knowledge_selected_collection_id = Some(collection_id.clone());
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(div().flex_none().child(Self::render_lucide_icon(
                        LucideIcon::BookOpen,
                        KNOWLEDGE_ROW_ICON_SIZE,
                        rgb(self.tokens.ui.text_muted),
                    )))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .truncate()
                                    .child(collection.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .truncate()
                                    .child(format!(
                                        "{} · {}",
                                        self.knowledge_scope_label(&collection.scope),
                                        self.knowledge_format_date(collection.updated_at)
                                    )),
                            ),
                    ),
            )
            .child(
                div().flex_none().child(
                    self.knowledge_icon_button(
                        LucideIcon::Trash2,
                        rgb(self.tokens.ui.text_muted),
                        Some(rgb(self.tokens.ui.error)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.knowledge_delete_confirm = Some(KnowledgeDeleteConfirm {
                                target: KnowledgeDeleteTarget::Collection,
                                id: delete_id.clone(),
                                name: delete_name.clone(),
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
                ),
            )
            .into_any_element()
    }

    fn knowledge_text_icon_button(&self, icon: LucideIcon, label: String, disabled: bool) -> Div {
        div()
            .h(px(KNOWLEDGE_ACTION_BUTTON_HEIGHT))
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg))
            .px(px(10.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .opacity(if disabled { 0.5 } else { 1.0 })
            .cursor_pointer()
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .child(Self::render_lucide_icon(
                icon,
                KNOWLEDGE_INLINE_ICON_SIZE,
                rgb(self.tokens.ui.text),
            ))
            .child(div().truncate().child(label))
    }

    fn knowledge_icon_button(
        &self,
        icon: LucideIcon,
        color: gpui::Rgba,
        hover_color: Option<gpui::Rgba>,
    ) -> Div {
        div()
            .size(px(KNOWLEDGE_ICON_BUTTON_SIZE))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.sm))
            .text_color(color)
            .cursor_pointer()
            .hover(move |style| {
                let style = style.bg(rgba((0xffffff << 8) | KNOWLEDGE_ICON_BUTTON_HOVER_ALPHA));
                if let Some(hover_color) = hover_color {
                    style.text_color(hover_color)
                } else {
                    style
                }
            })
            .child(Self::render_lucide_icon(
                icon,
                KNOWLEDGE_INLINE_ICON_SIZE,
                color,
            ))
    }

    fn knowledge_document_row(
        &self,
        document: oxideterm_ai::RagDocumentResponse,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let delete_id = document.id.clone();
        let delete_name = document.title.clone();
        let edit_id = document.id.clone();
        let editing_this = self
            .knowledge_external_edit
            .as_ref()
            .is_some_and(|edit| edit.doc_id == document.id);
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(12.0))
            .py(px(8.0))
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(div().flex_none().child(Self::render_lucide_icon(
                        LucideIcon::FileText,
                        KNOWLEDGE_ROW_ICON_SIZE,
                        rgb(self.tokens.ui.text_muted),
                    )))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .truncate()
                                    .child(document.title),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .truncate()
                                    .child(format!(
                                        "{} · {} {} · {}",
                                        document.format,
                                        document.chunk_count,
                                        self.i18n.t("settings_view.knowledge.chunks"),
                                        self.knowledge_format_date(document.indexed_at)
                                    )),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(if editing_this {
                        self.knowledge_icon_button(
                            LucideIcon::RefreshCw,
                            rgb(self.tokens.ui.accent),
                            Some(rgb(self.tokens.ui.accent)),
                        )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.knowledge_sync_external_edit(true, cx);
                                    cx.stop_propagation();
                                }),
                            )
                    } else {
                        self.knowledge_icon_button(
                            LucideIcon::Pencil,
                            rgb(self.tokens.ui.text_muted),
                            Some(rgb(self.tokens.ui.text)),
                        )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.knowledge_open_external(edit_id.clone(), cx);
                                    cx.stop_propagation();
                                }),
                            )
                    })
                    .child(
                        self.knowledge_icon_button(
                            LucideIcon::Trash2,
                            rgb(self.tokens.ui.text_muted),
                            Some(rgb(self.tokens.ui.error)),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.knowledge_delete_confirm = Some(KnowledgeDeleteConfirm {
                                    target: KnowledgeDeleteTarget::Document,
                                    id: delete_id.clone(),
                                    name: delete_name.clone(),
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .into_any_element()
    }

    fn knowledge_embedding_config_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.settings_store.settings();
        let preliminary = oxideterm_ai::resolve_ai_embedding_provider(
            &settings.ai.providers,
            settings.ai.active_provider_id.as_deref(),
            settings.ai.embedding_config.as_ref(),
            None,
        );
        let has_api_key = preliminary.provider.as_ref().and_then(|provider| {
            oxideterm_ai::ai_embedding_requires_api_key(provider)
                .then(|| self.ai_key_store.has_provider_key(&provider.id))
        });
        let resolved = oxideterm_ai::resolve_ai_embedding_provider(
            &settings.ai.providers,
            settings.ai.active_provider_id.as_deref(),
            settings.ai.embedding_config.as_ref(),
            has_api_key,
        );
        let provider_label = settings
            .ai
            .embedding_config
            .as_ref()
            .and_then(|config| config.get("providerId"))
            .and_then(serde_json::Value::as_str)
            .and_then(|provider_id| {
                ai_provider_views(settings)
                    .into_iter()
                    .find(|provider| provider.id == provider_id)
                    .map(|provider| provider.name)
            })
            .unwrap_or_else(|| self.i18n.t("settings_view.knowledge.auto_embedding_provider"));
        let model_value = self.current_settings_input_value(SettingsInput::AiEmbeddingModel);
        let status = match resolved.reason {
            oxideterm_ai::AiEmbeddingProviderReason::Ready => resolved
                .provider
                .as_ref()
                .map(|provider| {
                    self.i18n
                        .t("settings_view.knowledge.semantic_search_using")
                        .replace("{{provider}}", &provider.name)
                        .replace("{{model}}", &resolved.model)
                })
                .unwrap_or_else(|| self.i18n.t("settings_view.knowledge.semantic_search_not_configured")),
            oxideterm_ai::AiEmbeddingProviderReason::MissingModel => {
                self.i18n.t("settings_view.knowledge.semantic_search_missing_model")
            }
            oxideterm_ai::AiEmbeddingProviderReason::MissingApiKey => {
                self.i18n.t("settings_view.knowledge.embedding_api_key_missing")
            }
            oxideterm_ai::AiEmbeddingProviderReason::UnsupportedProvider => {
                self.i18n.t("settings_view.knowledge.embedding_provider_unsupported")
            }
            oxideterm_ai::AiEmbeddingProviderReason::NoProvider => {
                self.i18n.t("settings_view.knowledge.semantic_search_not_configured")
            }
        };
        let status_color = if resolved.reason == oxideterm_ai::AiEmbeddingProviderReason::Ready {
            self.tokens.ui.success
        } else {
            self.tokens.ui.text_muted
        };
        let chevron = if self.knowledge_embedding_config_expanded {
            LucideIcon::ChevronDown
        } else {
            LucideIcon::ChevronRight
        };

        div()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba(
                (self.tokens.ui.border << 8) | KNOWLEDGE_SECTION_BORDER_ALPHA,
            ))
            .bg(rgba((self.tokens.ui.bg << 8) | KNOWLEDGE_SECTION_BG_ALPHA))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .p(px(12.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .items_start()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .size(px(KNOWLEDGE_EMBEDDING_ICON_BOX))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(self.tokens.radii.md))
                                    .bg(rgba(
                                        (self.tokens.ui.accent << 8) | KNOWLEDGE_STATUS_BG_ALPHA,
                                    ))
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::Sparkles,
                                        KNOWLEDGE_ROW_ICON_SIZE,
                                        rgb(self.tokens.ui.accent),
                                    )),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_sm))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(self.tokens.ui.text))
                                            .child(self.i18n.t("settings_view.knowledge.semantic_search")),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .flex_wrap()
                                            .child(
                                                div()
                                                    .rounded_full()
                                                    .border_1()
                                                    .border_color(rgba(
                                                        (status_color << 8)
                                                            | KNOWLEDGE_STATUS_BORDER_ALPHA,
                                                    ))
                                                    .bg(rgba(
                                                        (status_color << 8)
                                                            | KNOWLEDGE_STATUS_BG_ALPHA,
                                                    ))
                                                    .px(px(8.0))
                                                    .py(px(2.0))
                                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                                    .text_color(rgb(status_color))
                                                    .child(status),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                                    .text_color(rgb(self.tokens.ui.text_muted))
                                                    .child(self.i18n.t("settings_view.knowledge.keyword_search_ready")),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .h(px(KNOWLEDGE_EMBEDDING_CONFIG_BUTTON_HEIGHT))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgb(self.tokens.ui.border))
                            .bg(rgb(self.tokens.ui.bg))
                            .px(px(10.0))
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Wrench,
                                KNOWLEDGE_INLINE_ICON_SIZE,
                                rgb(self.tokens.ui.text),
                            ))
                            .child(self.i18n.t("settings_view.knowledge.configure_embeddings"))
                            .child(Self::render_lucide_icon(
                                chevron,
                                KNOWLEDGE_INLINE_ICON_SIZE,
                                rgb(self.tokens.ui.text_muted),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.knowledge_embedding_config_expanded =
                                        !this.knowledge_embedding_config_expanded;
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    ),
            )
            .when(self.knowledge_embedding_config_expanded, |section| {
                section.child(
                    div()
                        .border_t_1()
                        .border_color(rgba(
                            (self.tokens.ui.border << 8) | KNOWLEDGE_SECTION_DIVIDER_ALPHA,
                        ))
                        .p(px(12.0))
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.i18n.t("settings_view.knowledge.semantic_search_description")),
                        )
                        .child(
                            div()
                                .grid()
                                .grid_cols(2)
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(6.0))
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(self.tokens.ui.text_muted))
                                                .child(self.i18n.t("settings_view.ai.embedding_provider")),
                                        )
                                        .child(self.ai_settings_select_control(
                                            SettingsSelect::AiEmbeddingProvider,
                                            provider_label,
                                            224.0,
                                            cx,
                                        )),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(6.0))
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(self.tokens.ui.text_muted))
                                                .child(self.i18n.t("settings_view.ai.embedding_model")),
                                        )
                                        .child(self.settings_text_input_control(
                                            SettingsInput::AiEmbeddingModel,
                                            model_value,
                                            self.i18n.t("settings_view.ai.embedding_model"),
                                            224.0,
                                            cx,
                                        )),
                                ),
                        ),
                )
            })
            .into_any_element()
    }

    fn knowledge_stats_row(&self, stats: oxideterm_ai::RagStatsResponse) -> AnyElement {
        let embedded_pct = if stats.chunk_count > 0 {
            ((stats.embedded_chunk_count as f64 / stats.chunk_count as f64) * 100.0).round() as i64
        } else {
            0
        };
        let mut row = div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap(px(24.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.knowledge_stat_item(
                stats.doc_count.to_string(),
                self.i18n.t("settings_view.knowledge.stat_docs"),
            ))
            .child(self.knowledge_stat_item(
                stats.chunk_count.to_string(),
                self.i18n.t("settings_view.knowledge.stat_chunks"),
            ))
            .child(self.knowledge_stat_item(
                format!("{embedded_pct}%"),
                self.i18n.t("settings_view.knowledge.stat_embedded"),
            ));
        if stats.last_updated > 0 {
            row = row.child(self.knowledge_stat_item(
                self.knowledge_format_date(stats.last_updated),
                self.i18n.t("settings_view.knowledge.stat_updated"),
            ));
        }
        row.into_any_element()
    }

    fn knowledge_stat_item(&self, value: String, label: String) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(value),
            )
            .child(label)
            .into_any_element()
    }

    fn knowledge_format_date(&self, timestamp_millis: i64) -> String {
        let Some(datetime) = chrono::DateTime::from_timestamp_millis(timestamp_millis) else {
            return "-".to_string();
        };
        let datetime = datetime.with_timezone(&chrono::Local);
        match self.i18n.locale() {
            Locale::ZhCn | Locale::ZhTw => datetime.format("%Y年%-m月%-d日").to_string(),
            _ => datetime.format("%b %-d, %Y").to_string(),
        }
    }

    fn knowledge_empty_row(&self, icon: LucideIcon, label: String) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .py(px(32.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(Self::render_lucide_icon(
                icon,
                32.0,
                rgba((self.tokens.ui.text_muted << 8) | 0x66),
            ))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .child(label),
            )
            .into_any_element()
    }

    fn knowledge_scope_label(&self, scope: &oxideterm_ai::DocScope) -> String {
        match scope {
            oxideterm_ai::DocScope::Global => self.i18n.t("settings_view.knowledge.scope_global"),
            oxideterm_ai::DocScope::Connection(_) => {
                self.i18n.t("settings_view.knowledge.scope_connection")
            }
        }
    }

    fn knowledge_document_format_label(&self) -> String {
        match self.knowledge_new_document_format.as_str() {
            "plaintext" => "Plain Text".to_string(),
            _ => "Markdown".to_string(),
        }
    }

    fn knowledge_create_collection(&mut self, cx: &mut Context<Self>) {
        let name = self.knowledge_new_collection_name.trim().to_string();
        if name.is_empty() {
            cx.notify();
            return;
        }
        match oxideterm_ai::rag_create_collection(
            &self.ai_rag_store,
            oxideterm_ai::RagCreateCollectionRequest {
                name,
                scope: oxideterm_ai::RagDocScopeRequest::Global,
            },
        ) {
            Ok(collection) => {
                self.knowledge_selected_collection_id = Some(collection.id);
                self.knowledge_new_collection_name.clear();
                self.settings_input_draft.clear();
                self.knowledge_error = None;
            }
            Err(error) => {
                self.knowledge_error = Some(error);
            }
        }
        cx.notify();
    }

    fn knowledge_create_blank_document(&mut self, cx: &mut Context<Self>) {
        let Some(collection_id) = self.knowledge_selected_collection_id.clone().or_else(|| {
            oxideterm_ai::rag_list_collections(&self.ai_rag_store, None)
                .ok()
                .and_then(|collections| collections.first().map(|collection| collection.id.clone()))
        }) else {
            cx.notify();
            return;
        };
        let title = self.knowledge_new_document_title.trim().to_string();
        if title.is_empty() {
            cx.notify();
            return;
        }
        match oxideterm_ai::rag_create_blank_document(
            &self.ai_rag_store,
            oxideterm_ai::RagCreateBlankDocumentRequest {
                collection_id,
                title,
                format: self.knowledge_new_document_format.clone(),
            },
        ) {
            Ok(document) => {
                self.knowledge_new_document_title.clear();
                self.settings_input_draft.clear();
                self.knowledge_error = None;
                self.knowledge_open_external(document.id, cx);
            }
            Err(error) => {
                self.knowledge_error = Some(error);
            }
        }
        cx.notify();
    }

    fn knowledge_delete_collection(&mut self, collection_id: String, cx: &mut Context<Self>) {
        match oxideterm_ai::rag_delete_collection(&self.ai_rag_store, &collection_id) {
            Ok(()) => {
                if self.knowledge_selected_collection_id.as_deref() == Some(collection_id.as_str())
                {
                    self.knowledge_selected_collection_id = None;
                }
                self.knowledge_external_edit = None;
                self.knowledge_error = None;
            }
            Err(error) => {
                self.knowledge_error = Some(error);
            }
        }
        cx.notify();
    }

    fn knowledge_delete_document(&mut self, document_id: String, cx: &mut Context<Self>) {
        match oxideterm_ai::rag_remove_document(&self.ai_rag_store, &document_id) {
            Ok(()) => {
                if self
                    .knowledge_external_edit
                    .as_ref()
                    .is_some_and(|edit| edit.doc_id == document_id)
                {
                    self.knowledge_external_edit = None;
                }
                self.knowledge_error = None;
            }
            Err(error) => {
                self.knowledge_error = Some(error);
            }
        }
        cx.notify();
    }

    fn knowledge_reindex(&mut self, collection_id: String, cx: &mut Context<Self>) {
        if self.knowledge_reindex_progress.is_some() {
            cx.notify();
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_task = cancel.clone();
        let store = self.ai_rag_store.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.knowledge_reindex_progress = Some((0, 0));
        self.knowledge_reindex_cancel = Some(cancel);
        self.knowledge_reindex_rx = Some(rx);
        self.knowledge_error = None;
        self.schedule_knowledge_reindex_poll(cx);
        self.forwarding_runtime.spawn(async move {
            let mut last_emitted = 0usize;
            let mut on_progress = |current: usize, total: usize| {
                if current == total || current.saturating_sub(last_emitted) >= 10 {
                    let _ = tx.send(KnowledgeReindexDelivery::Progress { current, total });
                    last_emitted = current;
                }
            };
            let result = oxideterm_ai::rag_reindex_collection_with_progress(
                &store,
                &collection_id,
                Some(cancel_for_task.as_ref()),
                Some(&mut on_progress),
            );
            let _ = tx.send(KnowledgeReindexDelivery::Finished(result));
        });
        cx.notify();
    }

    fn knowledge_cancel_reindex(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.knowledge_reindex_cancel.as_ref() {
            cancel.store(true, Ordering::Relaxed);
        }
        cx.notify();
    }

    fn poll_knowledge_reindex_results(&mut self, cx: &mut Context<Self>) {
        let Some(rx) = self.knowledge_reindex_rx.take() else {
            return;
        };
        let mut keep_rx = true;
        while let Ok(delivery) = rx.try_recv() {
            match delivery {
                KnowledgeReindexDelivery::Progress { current, total } => {
                    self.knowledge_reindex_progress = Some((current, total));
                }
                KnowledgeReindexDelivery::Finished(result) => {
                    keep_rx = false;
                    self.knowledge_reindex_progress = None;
                    self.knowledge_reindex_cancel = None;
                    if let Err(error) = result {
                        self.knowledge_error = Some(error);
                    } else {
                        self.knowledge_error = None;
                    }
                }
            }
        }
        if keep_rx {
            self.knowledge_reindex_rx = Some(rx);
        }
        cx.notify();
    }

    fn schedule_knowledge_reindex_poll(&mut self, cx: &mut Context<Self>) {
        if self.knowledge_reindex_polling {
            return;
        }
        self.knowledge_reindex_polling = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(33)).await;
            let _ = weak.update(cx, |this, cx| {
                this.knowledge_reindex_polling = false;
                if this.knowledge_reindex_rx.is_some() {
                    this.poll_knowledge_reindex_results(cx);
                    this.schedule_knowledge_reindex_poll(cx);
                }
            });
        })
        .detach();
    }

    fn knowledge_import_files(
        &mut self,
        collection_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.knowledge_import_progress.is_some() {
            return;
        }
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.knowledge.import_files"),
            )),
        });
        let store = self.ai_rag_store.clone();
        let error_title = self.i18n.t("settings_view.knowledge.error_import");
        cx.spawn(async move |weak, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let total = paths.len();
            if total == 0 {
                return;
            }
            let _ = weak.update(cx, |this, cx| {
                this.knowledge_import_progress = Some((0, total));
                this.knowledge_error = None;
                cx.notify();
            });
            let mut result = Ok(());
            for (index, path) in paths.iter().enumerate() {
                result = import_knowledge_file(&store, &collection_id, path).map(|_| ());
                let current = index + 1;
                let failed = result.is_err();
                let _ = weak.update(cx, |this, cx| {
                    this.knowledge_import_progress = Some((current, total));
                    if failed {
                        this.knowledge_error = result
                            .as_ref()
                            .err()
                            .map(|error| format!("{error_title}: {error}"));
                    }
                    cx.notify();
                });
                if failed {
                    break;
                }
            }
            let _ = weak.update(cx, |this, cx| {
                this.knowledge_import_progress = None;
                if let Err(error) = result {
                    this.knowledge_error = Some(format!("{error_title}: {error}"));
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn knowledge_generate_embeddings(&mut self, collection_id: String, cx: &mut Context<Self>) {
        if self.knowledge_embedding_progress.is_some() {
            return;
        }
        let settings = self.settings_store.settings().clone();
        let resolved = oxideterm_ai::resolve_ai_embedding_provider(
            &settings.ai.providers,
            settings.ai.active_provider_id.as_deref(),
            settings.ai.embedding_config.as_ref(),
            None,
        );
        let Some(provider) = resolved.provider.clone() else {
            self.knowledge_error = Some(self.i18n.t("settings_view.knowledge.error_no_embedding_support"));
            cx.notify();
            return;
        };
        if resolved.reason == oxideterm_ai::AiEmbeddingProviderReason::UnsupportedProvider
            || resolved.reason == oxideterm_ai::AiEmbeddingProviderReason::NoProvider
        {
            self.knowledge_error = Some(self.i18n.t("settings_view.knowledge.error_no_embedding_support"));
            cx.notify();
            return;
        }
        if resolved.reason == oxideterm_ai::AiEmbeddingProviderReason::MissingModel {
            self.knowledge_error = Some(self.i18n.t("settings_view.knowledge.error_no_embedding_model"));
            cx.notify();
            return;
        }
        let api_key = if oxideterm_ai::ai_embedding_requires_api_key(&provider) {
            match self.ai_key_store.get_provider_key(&provider.id).ok().flatten() {
                Some(key) if !key.trim().is_empty() => Some(key),
                _ => {
                    self.knowledge_error =
                        Some(self.i18n.t("settings_view.knowledge.error_no_embedding_api_key"));
                    cx.notify();
                    return;
                }
            }
        } else {
            self.ai_key_store.get_provider_key(&provider.id).ok().flatten()
        };
        let store = self.ai_rag_store.clone();
        let error_title = self
            .i18n
            .t("settings_view.knowledge.error_generate_embeddings");
        let partial_template = self
            .i18n
            .t("settings_view.knowledge.embedding_partial_failure");
        let model = resolved.model.clone();
        cx.spawn(async move |weak, cx| {
            let pending =
                match oxideterm_ai::rag_get_pending_embeddings(&store, &collection_id, Some(500))
                {
                    Ok(pending) => pending,
                    Err(error) => {
                        let _ = weak.update(cx, |this, cx| {
                            this.knowledge_error = Some(format!("{error_title}: {error}"));
                            cx.notify();
                        });
                        return;
                    }
                };
            if pending.is_empty() {
                return;
            }
            let total = pending.len();
            let _ = weak.update(cx, |this, cx| {
                this.knowledge_embedding_progress = Some((0, total));
                this.knowledge_error = None;
                cx.notify();
            });
            let mut processed = 0usize;
            let mut failed_count = 0usize;
            for batch in pending.chunks(KNOWLEDGE_EMBEDDING_BATCH_SIZE) {
                let texts = batch
                    .iter()
                    .map(|pending| pending.content.clone())
                    .collect::<Vec<_>>();
                match oxideterm_ai::embed_texts(&provider, api_key.clone(), &model, texts).await {
                    Ok(vectors) => {
                        let embeddings = batch
                            .iter()
                            .zip(vectors.into_iter())
                            .map(|(pending, vector)| oxideterm_ai::RagEmbeddingInputRequest {
                                chunk_id: pending.chunk_id.clone(),
                                vector,
                            })
                            .collect::<Vec<_>>();
                        if oxideterm_ai::rag_store_embeddings(
                            &store,
                            oxideterm_ai::RagStoreEmbeddingsRequest {
                                embeddings,
                                model_name: model.clone(),
                            },
                        )
                        .is_err()
                        {
                            failed_count += batch.len();
                        }
                    }
                    Err(_) => {
                        failed_count += batch.len();
                    }
                }
                processed += batch.len();
                let _ = weak.update(cx, |this, cx| {
                    this.knowledge_embedding_progress = Some((processed, total));
                    cx.notify();
                });
            }
            let _ = weak.update(cx, |this, cx| {
                this.knowledge_embedding_progress = None;
                if failed_count > 0 {
                    this.knowledge_error = Some(
                        partial_template
                            .replace("{{failed}}", &failed_count.to_string())
                            .replace("{{total}}", &total.to_string()),
                    );
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn knowledge_open_external(&mut self, document_id: String, cx: &mut Context<Self>) {
        let docs = oxideterm_ai::rag_list_collections(&self.ai_rag_store, None)
            .ok()
            .into_iter()
            .flatten()
            .find_map(|collection| {
                oxideterm_ai::rag_list_documents(
                    &self.ai_rag_store,
                    &collection.id,
                    None,
                    Some(500),
                )
                .ok()
                .and_then(|page| {
                    page.documents
                        .into_iter()
                        .find(|document| document.id == document_id)
                })
            });
        let Some(document) = docs else {
            self.knowledge_error = Some(self.i18n.t("settings_view.knowledge.error_open_external"));
            cx.notify();
            return;
        };
        let content = match oxideterm_ai::rag_get_document_content(&self.ai_rag_store, &document_id)
        {
            Ok(content) => content,
            Err(error) => {
                self.knowledge_error = Some(format!(
                    "{}: {error}",
                    self.i18n.t("settings_view.knowledge.error_open_external")
                ));
                cx.notify();
                return;
            }
        };
        let edit_dir = self
            .settings_store
            .path()
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("rag-edit");
        if let Err(error) = fs::create_dir_all(&edit_dir) {
            self.knowledge_error = Some(format!(
                "{}: {error}",
                self.i18n.t("settings_view.knowledge.error_open_external")
            ));
            cx.notify();
            return;
        }
        let extension = if document.format == "plaintext" {
            "txt"
        } else {
            "md"
        };
        let path = edit_dir.join(format!("{}.{}", document.id, extension));
        if let Err(error) = fs::write(&path, content) {
            self.knowledge_error = Some(format!(
                "{}: {error}",
                self.i18n.t("settings_view.knowledge.error_open_external")
            ));
            cx.notify();
            return;
        }
        let opened = open_path_external(&path).map_err(|error| error.to_string());
        match opened {
            Ok(()) => {
                self.knowledge_external_edit = Some(KnowledgeExternalEdit {
                    doc_id: document.id,
                    path,
                    version: document.version,
                });
                self.knowledge_error = None;
            }
            Err(error) => {
                self.knowledge_error = Some(format!(
                    "{}: {error}",
                    self.i18n.t("settings_view.knowledge.error_open_external")
                ));
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_sync_external_edit(
        &mut self,
        notify_no_changes: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self.knowledge_external_edit.clone() else {
            return;
        };
        let content = match fs::read_to_string(&edit.path) {
            Ok(content) => content,
            Err(error) => {
                self.knowledge_error = Some(format!(
                    "{}: {error}",
                    self.i18n.t("settings_view.knowledge.error_sync")
                ));
                cx.notify();
                return;
            }
        };
        match oxideterm_ai::rag_get_document_content(&self.ai_rag_store, &edit.doc_id) {
            Ok(current) if current == content => {
                if notify_no_changes {
                    self.push_ai_settings_toast(
                        self.i18n.t("settings_view.knowledge.doc_no_changes"),
                        TerminalNoticeVariant::Success,
                    );
                    cx.notify();
                }
                return;
            }
            Ok(_) => {}
            Err(error) => {
                self.knowledge_error = Some(format!(
                    "{}: {error}",
                    self.i18n.t("settings_view.knowledge.error_sync")
                ));
                cx.notify();
                return;
            }
        }
        match oxideterm_ai::rag_update_document(
            &self.ai_rag_store,
            &edit.doc_id,
            content,
            Some(edit.version),
        ) {
            Ok(document) => {
                self.knowledge_external_edit = Some(KnowledgeExternalEdit {
                    doc_id: document.id,
                    path: edit.path,
                    version: document.version,
                });
                self.knowledge_error = None;
                self.push_ai_settings_toast(
                    self.i18n.t("settings_view.knowledge.doc_updated"),
                    TerminalNoticeVariant::Success,
                );
            }
            Err(error) => {
                self.knowledge_error = Some(format!(
                    "{}: {error}",
                    self.i18n.t("settings_view.knowledge.error_sync")
                ));
            }
        }
        cx.notify();
    }

    fn knowledge_confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.knowledge_delete_confirm.take() else {
            cx.notify();
            return;
        };
        match confirm.target {
            KnowledgeDeleteTarget::Collection => {
                self.knowledge_delete_collection(confirm.id, cx);
            }
            KnowledgeDeleteTarget::Document => {
                self.knowledge_delete_document(confirm.id, cx);
            }
        }
    }

    fn render_knowledge_create_collection_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.knowledge_create_dialog_open {
            return None;
        }
        let can_create = !self.knowledge_new_collection_name.trim().is_empty();
        Some(
            dialog_backdrop()
                .child(
                    dialog_content(&self.tokens)
                        .w(px(KNOWLEDGE_DIALOG_WIDTH))
                        .max_w(relative(0.92))
                        .shadow_lg()
                        .child(
                            dialog_header(&self.tokens)
                                .child(dialog_title(
                                    &self.tokens,
                                    self.i18n.t("settings_view.knowledge.create_collection"),
                                ))
                                .child(dialog_description(
                                    &self.tokens,
                                    self.i18n.t("settings_view.knowledge.create_description"),
                                )),
                        )
                        .child(
                            div()
                                .px(px(24.0))
                                .py(px(18.0))
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(self.tokens.ui.text))
                                                .child(self.i18n.t("settings_view.knowledge.collection_name")),
                                        )
                                        .child(self.settings_text_input_control(
                                            SettingsInput::KnowledgeCollectionName,
                                            self.knowledge_new_collection_name.clone(),
                                            self.i18n
                                                .t("settings_view.knowledge.collection_name_placeholder"),
                                            420.0,
                                            cx,
                                        )),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(self.tokens.ui.text))
                                                .child(self.i18n.t("settings_view.knowledge.scope")),
                                        )
                                        .child(self.ai_settings_select_control(
                                            SettingsSelect::KnowledgeCollectionScope,
                                            self.i18n.t("settings_view.knowledge.scope_global"),
                                            420.0,
                                            cx,
                                        )),
                                ),
                        )
                        .child(
                            dialog_footer(&self.tokens)
                                .child(
                                    button_with(
                                        &self.tokens,
                                        self.i18n.t("common.actions.cancel"),
                                        ButtonOptions {
                                            variant: ButtonVariant::Outline,
                                            size: ButtonSize::Sm,
                                            radius: ButtonRadius::Md,
                                            disabled: false,
                                        },
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.knowledge_create_dialog_open = false;
                                            this.knowledge_new_collection_name.clear();
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                                )
                                .child(
                                    button_with(
                                        &self.tokens,
                                        self.i18n.t("settings_view.knowledge.create_collection"),
                                        ButtonOptions {
                                            variant: ButtonVariant::Default,
                                            size: ButtonSize::Sm,
                                            radius: ButtonRadius::Md,
                                            disabled: !can_create,
                                        },
                                    )
                                    .when(can_create, |button| {
                                        button.on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.knowledge_create_collection(cx);
                                                this.knowledge_create_dialog_open = false;
                                                cx.stop_propagation();
                                            }),
                                        )
                                    }),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_knowledge_new_document_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.knowledge_new_document_dialog_open {
            return None;
        }
        let can_create = !self.knowledge_new_document_title.trim().is_empty();
        Some(
            dialog_backdrop()
                .child(
                    dialog_content(&self.tokens)
                        .w(px(KNOWLEDGE_DIALOG_WIDTH))
                        .max_w(relative(0.92))
                        .shadow_lg()
                        .child(
                            dialog_header(&self.tokens)
                                .child(dialog_title(
                                    &self.tokens,
                                    self.i18n.t("settings_view.knowledge.new_document"),
                                ))
                                .child(dialog_description(
                                    &self.tokens,
                                    self.i18n.t("settings_view.knowledge.new_document_description"),
                                )),
                        )
                        .child(
                            div()
                                .px(px(24.0))
                                .py(px(18.0))
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(self.tokens.ui.text))
                                                .child(self.i18n.t("settings_view.knowledge.new_document_title")),
                                        )
                                        .child(self.settings_text_input_control(
                                            SettingsInput::KnowledgeDocumentTitle,
                                            self.knowledge_new_document_title.clone(),
                                            self.i18n
                                                .t("settings_view.knowledge.new_document_title_placeholder"),
                                            420.0,
                                            cx,
                                        )),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(self.tokens.ui.text))
                                                .child(self.i18n.t("settings_view.knowledge.format")),
                                        )
                                        .child(self.ai_settings_select_control(
                                            SettingsSelect::KnowledgeDocumentFormat,
                                            self.knowledge_document_format_label(),
                                            420.0,
                                            cx,
                                        )),
                                ),
                        )
                        .child(
                            dialog_footer(&self.tokens)
                                .child(
                                    button_with(
                                        &self.tokens,
                                        self.i18n.t("common.actions.cancel"),
                                        ButtonOptions {
                                            variant: ButtonVariant::Outline,
                                            size: ButtonSize::Sm,
                                            radius: ButtonRadius::Md,
                                            disabled: false,
                                        },
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.knowledge_new_document_dialog_open = false;
                                            this.knowledge_new_document_title.clear();
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                                )
                                .child(
                                    button_with(
                                        &self.tokens,
                                        self.i18n.t("settings_view.knowledge.new_document"),
                                        ButtonOptions {
                                            variant: ButtonVariant::Default,
                                            size: ButtonSize::Sm,
                                            radius: ButtonRadius::Md,
                                            disabled: !can_create,
                                        },
                                    )
                                    .when(can_create, |button| {
                                        button.on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.knowledge_create_blank_document(cx);
                                                this.knowledge_new_document_dialog_open = false;
                                                cx.stop_propagation();
                                            }),
                                        )
                                    }),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_knowledge_delete_confirm_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let confirm = self.knowledge_delete_confirm.as_ref()?;
        let message_key = match confirm.target {
            KnowledgeDeleteTarget::Collection => "settings_view.knowledge.delete_collection_confirm",
            KnowledgeDeleteTarget::Document => "settings_view.knowledge.delete_document_confirm",
        };
        let message = self
            .i18n
            .t(message_key)
            .replace("{{name}}", &confirm.name);
        Some(
            dialog_backdrop()
                .child(
                    dialog_content(&self.tokens)
                        .w(px(KNOWLEDGE_DIALOG_WIDTH))
                        .max_w(relative(0.92))
                        .shadow_lg()
                        .child(
                            dialog_header(&self.tokens)
                                .child(dialog_title(
                                    &self.tokens,
                                    self.i18n.t("settings_view.knowledge.delete_confirm_title"),
                                ))
                                .child(dialog_description(&self.tokens, message)),
                        )
                        .child(
                            dialog_footer(&self.tokens)
                                .child(
                                    button_with(
                                        &self.tokens,
                                        self.i18n.t("common.actions.cancel"),
                                        ButtonOptions {
                                            variant: ButtonVariant::Outline,
                                            size: ButtonSize::Sm,
                                            radius: ButtonRadius::Md,
                                            disabled: false,
                                        },
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.knowledge_delete_confirm = None;
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                                )
                                .child(
                                    button_with(
                                        &self.tokens,
                                        self.i18n.t("common.delete"),
                                        ButtonOptions {
                                            variant: ButtonVariant::Destructive,
                                            size: ButtonSize::Sm,
                                            radius: ButtonRadius::Md,
                                            disabled: false,
                                        },
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.knowledge_confirm_delete(cx);
                                            cx.stop_propagation();
                                        }),
                                    ),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    fn settings_keybindings(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        let side = crate::keybindings::KeybindingSide::current();
        let modified = crate::keybindings::modified_count(&settings.keybindings.overrides);
        let query = self.keybinding_search_query.trim().to_lowercase();

        let mut rows = vec![
            self.keybinding_toolbar(modified, cx),
        ];

        let mut visible_scope_count = 0;
        for scope in [
            crate::keybindings::ActionScope::Global,
            crate::keybindings::ActionScope::Terminal,
            crate::keybindings::ActionScope::Split,
            crate::keybindings::ActionScope::Palette,
        ] {
            let definitions = crate::keybindings::ACTION_DEFINITIONS
                .iter()
                .filter(|definition| definition.scope == scope)
                .filter(|definition| self.keybinding_scope_filter.matches(definition.scope))
                .filter(|definition| {
                    if query.is_empty() {
                        return true;
                    }
                    let label = self.i18n.t(&definition.label_key()).to_lowercase();
                    label.contains(&query) || definition.id.to_lowercase().contains(&query)
                })
                .collect::<Vec<_>>();
            if !definitions.is_empty() {
                visible_scope_count += 1;
                rows.push(self.keybinding_scope_table(scope, &definitions, side, cx));
            }
        }

        if visible_scope_count == 0 {
            rows.push(self.keybinding_no_results());
        }

        rows.push(self.keybinding_reference_tables());

        rows
    }

    fn keybinding_toolbar(&self, modified: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(18.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("settings_view.keybindings.intl_keyboard_note")),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap(px(12.0))
                    .child(self.keybinding_search_input(cx))
                    .child(self.keybinding_scope_filter(cx))
                    .child(div().flex_1().min_w(px(0.0)))
                    .child(self.keybinding_toolbar_button(
                        LucideIcon::Upload,
                        "settings_view.keybindings.import",
                        false,
                        cx,
                    ))
                    .child(self.keybinding_toolbar_button(
                        LucideIcon::Download,
                        "settings_view.keybindings.export",
                        false,
                        cx,
                    ))
                    .when(modified > 0, |toolbar| toolbar.child(
                        button_with(
                            &self.tokens,
                            self.i18n.t("settings_view.keybindings.reset_all"),
                            ButtonOptions {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: modified == 0,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.keybinding_reset_all_confirm_open = true;
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                    )),
            )
            .into_any_element()
    }

    fn keybinding_search_input(&self, cx: &mut Context<Self>) -> AnyElement {
        let focused = self.focused_settings_input == Some(SettingsInput::KeybindingSearch);
        let value = if focused {
            self.settings_input_draft.as_str()
        } else {
            self.keybinding_search_query.as_str()
        };
        let target = WorkspaceImeTarget::Settings(SettingsInput::KeybindingSearch);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value,
                    placeholder: self.i18n.t("settings_view.keybindings.search_placeholder"),
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w(px(280.0))
            .h(px(36.0))
            .pl(px(34.0))
            .cursor(CursorStyle::IBeam)
            .child(
                div()
                    .absolute()
                    .left(px(12.0))
                    .top(px(10.0))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Search,
                        15.0,
                        rgb(self.tokens.ui.text_muted),
                    )),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.focus_settings_input(
                        SettingsInput::KeybindingSearch,
                        this.keybinding_search_query.clone(),
                        cx,
                    );
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                }),
            ),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn keybinding_scope_filter(&self, cx: &mut Context<Self>) -> AnyElement {
        let filters = [
            KeybindingScopeFilter::All,
            KeybindingScopeFilter::Scope(crate::keybindings::ActionScope::Global),
            KeybindingScopeFilter::Scope(crate::keybindings::ActionScope::Terminal),
            KeybindingScopeFilter::Scope(crate::keybindings::ActionScope::Split),
            KeybindingScopeFilter::Scope(crate::keybindings::ActionScope::Palette),
        ];
        let mut row = div().flex().items_center().gap(px(4.0));
        for filter in filters {
            let active = self.keybinding_scope_filter == filter;
            row = row.child(
                button_with(
                    &self.tokens,
                    self.i18n.t(filter.label_key()),
                    ButtonOptions {
                        variant: if active {
                            ButtonVariant::Secondary
                        } else {
                            ButtonVariant::Ghost
                        },
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.keybinding_scope_filter = filter;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
            );
        }
        row.into_any_element()
    }

    fn keybinding_toolbar_button(
        &self,
        icon: LucideIcon,
        label_key: &'static str,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let action = label_key;
        button_with(
            &self.tokens,
            self.i18n.t(label_key),
            ButtonOptions {
                variant: ButtonVariant::Ghost,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled,
            },
        )
        .child(Self::render_lucide_icon(
            icon,
            14.0,
            rgb(self.tokens.ui.text_muted),
        ))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, window, cx| {
                match action {
                    "settings_view.keybindings.import" => this.import_keybindings(window, cx),
                    "settings_view.keybindings.export" => this.export_keybindings(cx),
                    _ => {}
                }
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn keybinding_no_results(&self) -> AnyElement {
        div()
            .w_full()
            .py(px(44.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t("settings_view.keybindings.no_results"))
            .into_any_element()
    }

    fn keybinding_scope_table(
        &self,
        scope: crate::keybindings::ActionScope,
        definitions: &[&crate::keybindings::ActionDefinition],
        side: crate::keybindings::KeybindingSide,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut table = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(theme.border))
            .overflow_hidden()
            .child(
                div()
                    .h(px(40.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(rgb(theme.bg_panel))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t(scope.label_key()).to_uppercase()),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("settings_view.keybindings.column_shortcut")),
                    ),
            );

        for definition in definitions {
            table = table.child(self.keybinding_action_row(definition, side, cx));
        }

        table.into_any_element()
    }

    fn keybinding_action_row(
        &self,
        definition: &crate::keybindings::ActionDefinition,
        side: crate::keybindings::KeybindingSide,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let settings = self.settings_store.settings();
        let current =
            crate::keybindings::effective_combo(definition, &settings.keybindings.overrides, side);
        let default = definition.default_combo(side);
        let modified = current != *default;
        let recording = self
            .keybinding_recording_action_id
            .as_deref()
            .is_some_and(|id| id == definition.id);
        let action_id = definition.id.to_string();
        let record_action_id = action_id.clone();
        let reset_action_id = action_id.clone();
        let conflicts = if recording {
            self.keybinding_conflict_action_ids.as_slice()
        } else {
            &[]
        };

        div()
            .w_full()
            .min_w(px(0.0))
            .px(px(14.0))
            .py(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .border_b_1()
            .border_color(rgb(theme.border))
            .when(recording, |row| row.bg(rgba((theme.accent << 8) | 0x0d)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t(&definition.label_key())),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t(definition.scope.label_key()))
                            .when(modified, |meta| {
                                meta.child(
                                    self.keybinding_modified_badge(),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when(recording, |controls| {
                        controls.child(self.keybinding_recording_cell(conflicts, side, cx))
                    })
                    .when(!recording, |controls| {
                        controls
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .rounded(px(self.tokens.radii.sm))
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                                    .child(self.keybinding_kbd_badge(
                                        &crate::keybindings::format_combo(&current),
                                        false,
                                    ))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            this.keybinding_recording_action_id =
                                                Some(record_action_id.clone());
                                            this.keybinding_recording_combo = None;
                                            this.keybinding_conflict_action_ids.clear();
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .when(modified, |controls| {
                                controls.child(
                                    self.keybinding_icon_button(LucideIcon::RotateCcw)
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |this, _event, window, cx| {
                                                this.reset_keybinding(&reset_action_id, window, cx);
                                                cx.stop_propagation();
                                            }),
                                        ),
                                )
                            })
                    }),
            )
            .into_any_element()
    }

    fn keybinding_recording_cell(
        &self,
        conflicts: &[String],
        side: crate::keybindings::KeybindingSide,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let combo = self.keybinding_recording_combo.as_ref();
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(4.0))
                    .child(match combo {
                        Some(combo) => {
                            self.keybinding_kbd_badge(&crate::keybindings::format_combo(combo), true)
                        }
                        None => div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .italic()
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("settings_view.keybindings.record_prompt"))
                            .into_any_element(),
                    })
                    .when(combo.is_some() && !conflicts.is_empty(), |cell| {
                        cell.child(
                            div()
                                .max_w(px(240.0))
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(rgb(theme.warning))
                                .child(self.keybinding_conflict_text(conflicts, side)),
                        )
                    }),
            )
            .when(combo.is_some(), |cell| {
                let label_key = if conflicts.is_empty() {
                    "✓"
                } else {
                    "settings_view.keybindings.override_anyway"
                };
                cell.child(
                    button_with(
                        &self.tokens,
                        if conflicts.is_empty() {
                            label_key.to_string()
                        } else {
                            self.i18n.t(label_key)
                        },
                        ButtonOptions {
                            variant: ButtonVariant::Ghost,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: false,
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            this.confirm_keybinding_recording(window, cx);
                            cx.stop_propagation();
                        }),
                    ),
                )
            })
            .child(
                self.keybinding_icon_button(LucideIcon::X).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.cancel_keybinding_recording(cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .into_any_element()
    }

    fn keybinding_modified_badge(&self) -> AnyElement {
        div()
            .flex_none()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((self.tokens.ui.accent << 8) | 0x33))
            .px(px(6.0))
            .py(px(1.0))
            .text_size(px(10.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.accent))
            .child(self.i18n.t("settings_view.keybindings.modified"))
            .into_any_element()
    }

    fn keybinding_kbd_badge(&self, value: &str, accent: bool) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(if accent {
                rgba((self.tokens.ui.accent << 8) | 0x4d)
            } else {
                rgba((self.tokens.ui.border << 8) | 0x80)
            })
            .bg(if accent {
                rgba((self.tokens.ui.accent << 8) | 0x33)
            } else {
                rgb(self.tokens.ui.bg)
            })
            .px(px(8.0))
            .py(px(2.0))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(if accent {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text
            }))
            .child(value.to_string())
            .into_any_element()
    }

    fn keybinding_icon_button(&self, icon: LucideIcon) -> Div {
        div()
            .size(px(28.0))
            .rounded(px(self.tokens.radii.sm))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(rgb(self.tokens.ui.text_muted))
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .child(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text_muted),
            ))
    }

    fn keybinding_conflict_text(
        &self,
        conflicts: &[String],
        side: crate::keybindings::KeybindingSide,
    ) -> String {
        let Some(conflict) = conflicts
            .iter()
            .filter_map(|id| crate::keybindings::action_definition(id))
            .next()
        else {
            return String::new();
        };
        self.i18n
            .t("settings_view.keybindings.conflict_warning")
            .replace("{{scope}}", &self.i18n.t(conflict.scope.label_key()))
            .replace("{{action}}", &self.i18n.t(&conflict.label_key()))
            .replace(
                "{{shortcut}}",
                &crate::keybindings::format_combo(conflict.default_combo(side)),
            )
    }

    fn render_keybinding_reset_all_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        confirm_dialog(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div()
                    .child(self.i18n.t("settings_view.keybindings.reset_all_confirm"))
                    .into_any_element(),
                description: None,
                cancel_label: div()
                    .child(self.i18n.t("common.actions.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t("settings_view.keybindings.reset_all"))
                    .into_any_element(),
            },
            cx.listener(|this, _event, _window, cx| {
                this.keybinding_reset_all_confirm_open = false;
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, window, cx| {
                this.keybinding_reset_all_confirm_open = false;
                this.reset_all_keybindings(window, cx);
                cx.stop_propagation();
            }),
        )
    }

    fn keybinding_reference_tables(&self) -> AnyElement {
        let categories = [
            (
                "settings_view.help.category_file_manager",
                vec![
                    ("settings_view.help.shortcut_select_all", "⌘A", "Ctrl+A"),
                    ("settings_view.help.shortcut_copy", "⌘C", "Ctrl+C"),
                    ("settings_view.help.shortcut_cut", "⌘X", "Ctrl+X"),
                    ("settings_view.help.shortcut_paste", "⌘V", "Ctrl+V"),
                    ("settings_view.help.shortcut_rename", "F2", "F2"),
                    ("settings_view.help.shortcut_delete", "Delete", "Delete"),
                    ("settings_view.help.shortcut_quick_look", "Space", "Space"),
                    ("settings_view.help.shortcut_open", "Enter", "Enter"),
                ],
            ),
            (
                "settings_view.help.category_sftp",
                vec![
                    ("settings_view.help.shortcut_select_all", "⌘A", "Ctrl+A"),
                    ("settings_view.help.shortcut_quick_look", "Space", "Space"),
                    ("settings_view.help.shortcut_sftp_enter_dir", "Enter", "Enter"),
                    ("settings_view.help.shortcut_sftp_upload", "→", "→"),
                    ("settings_view.help.shortcut_sftp_download", "←", "←"),
                    ("settings_view.help.shortcut_rename", "F2", "F2"),
                    ("settings_view.help.shortcut_delete", "Delete", "Delete"),
                ],
            ),
            (
                "settings_view.help.category_editor",
                vec![
                    ("settings_view.help.shortcut_save", "⌘S", "Ctrl+S"),
                    ("settings_view.help.shortcut_find", "⌘F", "Ctrl+F"),
                    ("settings_view.help.shortcut_copy", "⌘C", "Ctrl+C"),
                    ("settings_view.help.shortcut_paste", "⌘V", "Ctrl+V"),
                    ("settings_view.help.shortcut_close", "Esc", "Esc"),
                ],
            ),
        ];

        let theme = self.tokens.ui;
        let mut section = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .pt(px(6.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(self.i18n.t("settings_view.help.shortcuts")),
            );

        for (title_key, rows) in categories {
            let mut table = div()
                .w_full()
                .min_w(px(0.0))
                .rounded(px(self.tokens.radii.lg))
                .border_1()
                .border_color(rgb(theme.border))
                .overflow_hidden()
                .child(
                    div()
                        .h(px(36.0))
                        .px(px(14.0))
                        .flex()
                        .items_center()
                        .bg(rgb(theme.bg_panel))
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t(title_key).to_uppercase()),
                );
            let row_count = rows.len();
            for (index, (label_key, mac, other)) in rows.into_iter().enumerate() {
                table = table.child(self.keybinding_reference_row(
                    label_key,
                    mac,
                    other,
                    index + 1 < row_count,
                ));
            }
            section = section.child(table);
        }

        section.into_any_element()
    }

    fn keybinding_reference_row(
        &self,
        label_key: &str,
        mac: &str,
        other: &str,
        show_separator: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .min_w(px(0.0))
            .px(px(14.0))
            .py(px(9.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .when(show_separator, |row| {
                row.border_b_1().border_color(rgb(theme.border))
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.keybinding_badge(mac))
            .child(self.keybinding_badge(other))
            .into_any_element()
    }

    fn keybinding_badge(&self, value: &str) -> AnyElement {
        div()
            .flex_none()
            .min_w(px(72.0))
            .h(px(28.0))
            .px(px(10.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg))
            .flex()
            .items_center()
            .justify_center()
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text))
            .child(value.to_string())
            .into_any_element()
    }

    fn settings_help(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![
            self.settings_card(
                "settings_view.help.version_info",
                "settings_view.help.description",
                vec![
                    self.value_row(
                        "settings_view.help.app_name",
                        "settings_view.help.version_info",
                        "OxideTerm Native".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.version",
                        "settings_view.help.version_info",
                        env!("CARGO_PKG_VERSION").to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.license",
                        "settings_view.help.resources",
                        "GPL-3.0-only".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.portable_mode",
                        "settings_view.help.portable_mode_hint",
                        self.i18n.t("settings_view.help.updates_manual_only"),
                    ),
                    self.cycle_row(
                        "settings_view.help.update_channel",
                        "settings_view.help.update_channel_hint",
                        update_channel_label(settings.general.update_channel, &self.i18n),
                        cycle_update_channel,
                        cx,
                    ),
                ],
            ),
            self.settings_card(
                "settings_view.help.shortcuts",
                "settings_view.help.resources",
                vec![
                    self.value_row(
                        "settings_view.help.shortcut_new_tab",
                        "settings_view.help.category_app",
                        "Cmd+T".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_close_tab",
                        "settings_view.help.category_app",
                        "Cmd+W".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_find",
                        "settings_view.help.category_terminal",
                        "Cmd+F".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_split_h",
                        "settings_view.help.category_split",
                        "Cmd+Shift+E".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_split_v",
                        "settings_view.help.category_split",
                        "Cmd+Shift+D".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_settings",
                        "settings_view.help.category_app",
                        "Cmd+,".to_string(),
                    ),
                ],
            ),
            self.settings_card(
                "settings_view.help.diagnostics",
                "settings_view.help.open_logs_hint",
                vec![
                    self.value_row(
                        "settings_view.help.open_logs",
                        "settings_view.help.open_logs_hint",
                        self.i18n.t("common.disabled"),
                    ),
                    self.value_row(
                        "settings_view.help.memory_diagnostics_title",
                        "settings_view.help.memory_diagnostics_hint",
                        self.i18n.t("common.disabled"),
                    ),
                    self.value_row(
                        "settings_view.help.check_update",
                        "settings_view.help.updates_manual_only_hint",
                        self.i18n.t("settings_view.help.updates_manual_only"),
                    ),
                ],
            ),
        ]
    }

    fn cycle_row(
        &self,
        label_key: &str,
        hint_key: &str,
        value: String,
        cycle: fn(&mut PersistedSettings),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = button(&self.tokens, value, oxideterm_gpui_ui::ButtonTone::Secondary)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(cycle, cx);
                }),
            )
            .into_any_element();
        self.setting_row(label_key, hint_key, control)
    }

    fn language_label(&self, language: Language) -> String {
        match language {
            Language::De => "Deutsch",
            Language::En => "English",
            Language::EsEs => "Español (España)",
            Language::FrFr => "Français (France)",
            Language::It => "Italiano",
            Language::Ko => "한국어",
            Language::PtBr => "Português (Brasil)",
            Language::Vi => "Tiếng Việt",
            Language::Ja => "日本語",
            Language::ZhCn => "简体中文",
            Language::ZhTw => "繁體中文",
        }
        .to_string()
    }
}

fn import_knowledge_file(
    store: &oxideterm_ai::RagStore,
    collection_id: &str,
    path: &std::path::Path,
) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > KNOWLEDGE_MAX_IMPORT_FILE_SIZE {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("document");
        return Err(format!(
            "File \"{file_name}\" exceeds 5 MB limit ({} MB)",
            (metadata.len() as f64 / 1024.0 / 1024.0).round() as u64
        ));
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document")
        .to_string();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let format = match extension.as_str() {
        "md" | "markdown" => "markdown",
        "txt" => "plaintext",
        _ => return Err(format!("Unsupported document type: {file_name}")),
    };
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    oxideterm_ai::rag_add_document(
        store,
        oxideterm_ai::RagAddDocumentRequest {
            collection_id: collection_id.to_string(),
            title: file_name,
            content,
            format: format.to_string(),
            source_path: Some(path.to_string_lossy().to_string()),
        },
    )
    .map(|_| ())
}

fn open_path_external(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?.wait()?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn()?
            .wait()?;
        return Ok(());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?.wait()?;
        Ok(())
    }
}

fn reconnect_max_attempt_options() -> [i64; 8] {
    [1, 2, 3, 5, 8, 10, 15, 20]
}

fn reconnect_base_delay_options() -> [(i64, &'static str); 6] {
    [
        (500, "0.5s"),
        (1_000, "1s"),
        (2_000, "2s"),
        (3_000, "3s"),
        (5_000, "5s"),
        (10_000, "10s"),
    ]
}

fn reconnect_max_delay_options() -> [(i64, &'static str); 5] {
    [
        (5_000, "5s"),
        (10_000, "10s"),
        (15_000, "15s"),
        (30_000, "30s"),
        (60_000, "60s"),
    ]
}

fn reconnect_attempt_label(value: i64) -> String {
    value.to_string()
}

fn reconnect_delay_label(value: i64) -> String {
    if value % 1_000 == 0 {
        format!("{}s", value / 1_000)
    } else {
        format!("{:.1}s", value as f64 / 1_000.0)
    }
}
