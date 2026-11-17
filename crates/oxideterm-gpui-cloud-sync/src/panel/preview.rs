use super::*;

pub(super) struct CloudSyncUploadSelectionRow {
    label_key: &'static str,
    action: CloudSyncUploadSelectionAction,
    meta: Option<String>,
    checked: bool,
}

impl CloudSyncPageRenderer {
    pub(super) fn render_cloud_sync_fact(
        &self,
        label_key: &str,
        value: String,
        cx: &mut App,
    ) -> AnyElement {
        let label = self.i18n.t(label_key).to_uppercase();
        cloud_sync_fact_card(
            &self.tokens,
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "cloud-sync-fact-label",
                label_key,
                label.clone(),
                self.tokens.ui.text_muted,
                cx,
            ),
            self.render_selectable_text(
                selectable_text_id("cloud-sync-fact", (&label, &value)),
                value.clone(),
                self.tokens.ui.text,
                cx,
            ),
            cloud_sync_value_prefers_mono(&value),
            Some(self.mono_font_family.clone()),
        )
    }

    pub(super) fn upload_selection_listener(
        &self,
        action: CloudSyncUploadSelectionAction,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        let cloud_sync = self.cloud_sync.clone();
        move |_event, _window, cx| {
            cloud_sync.update(cx, |cloud_sync, cx| {
                if let Some(selection) = cloud_sync.view.upload_selection.as_mut() {
                    selection.apply_action(action.clone());
                    cx.notify();
                }
            });
            cx.stop_propagation();
        }
    }

    pub(super) fn preview_selection_listener(
        &self,
        action: CloudSyncPreviewSelectionAction,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        let cloud_sync = self.cloud_sync.clone();
        move |_event, _window, cx| {
            cloud_sync.update(cx, |cloud_sync, cx| {
                let connection_names = cloud_sync
                    .view
                    .pending_preview
                    .as_ref()
                    .map(cloud_sync_preview_summary)
                    .map(|summary| summary.connection_record_names())
                    .unwrap_or_default();
                if let Some(selection) = cloud_sync.view.preview_selection.as_mut() {
                    selection.apply_action(action.clone(), connection_names);
                    cx.notify();
                }
            });
            cx.stop_propagation();
        }
    }

    pub(super) fn render_pending_preview(&self, busy: bool, cx: &mut App) -> AnyElement {
        let cloud_sync_entity = self.cloud_sync.clone();
        cloud_sync_entity.update(cx, |cloud_sync, cx| {
            let state = cloud_sync.controller.store.state();
            // Render from this borrow; reading the same entity through App would panic.
            let local_file_mode = cloud_sync.view.local_file_mode;
            if let Some(summary) = cloud_sync.view.causal_summary.as_ref() {
                return self.render_causal_preview(
                    summary,
                    &cloud_sync.view.causal_conflicts,
                    &cloud_sync.view.causal_choices,
                    cloud_sync.controller.password_change.is_some(),
                    busy,
                    cx,
                );
            }
            if let Some(preview) = cloud_sync.view.pending_preview.as_ref() {
                self.render_cloud_sync_preview(
                    preview,
                    state,
                    cloud_sync.view.preview_selection.as_ref(),
                    local_file_mode,
                    busy,
                    cx,
                )
            } else if let Some(preview) = cloud_sync.view.upload_preview.as_ref() {
                self.render_cloud_sync_upload_preview(
                    preview,
                    state,
                    cloud_sync.view.upload_selection.as_ref(),
                    local_file_mode,
                    busy,
                    cx,
                )
            } else {
                div().into_any_element()
            }
        })
    }

    pub(super) fn render_cloud_sync_preview(
        &self,
        preview: &CloudSyncPendingPreview,
        state: &CloudSyncPersistedState,
        current_selection: Option<&CloudSyncPreviewSelection>,
        local_file_mode: bool,
        busy: bool,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let model = cloud_sync_preview_card_model(preview, state, current_selection);
        let title = self.render_display_text_with_role(
            SelectableTextRole::NonSelectable,
            "cloud-sync-preview-title",
            model.copy.title_identity,
            self.i18n.t(model.copy.title_key),
            theme.text_heading,
            cx,
        );
        let fact_rows = model
            .fact_rows
            .iter()
            .map(|row| {
                cloud_sync_fact_grid(row.iter().map(|fact| {
                    self.render_cloud_sync_fact(
                        fact.label_key,
                        self.cloud_sync_preview_fact_value(&fact.value),
                        cx,
                    )
                }))
            })
            .collect::<Vec<_>>();
        let warning = model.copy.warning_key.map(|key| self.i18n.t(key));
        let mut body = Vec::new();
        let local_snapshot = self.diff.local_snapshot.as_ref().ok().map(Arc::as_ref);
        let apply_diff_items =
            cloud_sync_apply_diff_items(preview, &model.selection, local_snapshot);
        if !apply_diff_items.is_empty() {
            body.push(self.render_cloud_sync_section_diff_card(
                "cloud-sync-apply-diff",
                "plugin.cloud_sync.preflight.apply_diff_title",
                &apply_diff_items,
                cx,
            ));
        }
        let field_diff_items = cloud_sync_apply_field_diff_items(
            preview,
            &model.selection,
            &self.diff.local_field_diff,
        );
        if !field_diff_items.is_empty() {
            body.push(self.render_cloud_sync_apply_field_diff_card(
                &field_diff_items,
                &model.selection,
                cx,
            ));
        }
        if let Some(card) = self.render_cloud_sync_remote_sensitive_summary(preview, cx) {
            body.push(card);
        }
        if !model.impact_items.is_empty() {
            body.push(self.render_cloud_sync_preview_impact(&model.impact_items, cx));
        }
        for section in &model.body_sections {
            body.push(match section {
                CloudSyncPreviewBodySection::Selection => {
                    self.render_cloud_sync_preview_selection(&model.summary, &model.selection, cx)
                }
                CloudSyncPreviewBodySection::ForwardDetails(details) => {
                    self.render_cloud_sync_forward_details(&details, cx)
                }
                CloudSyncPreviewBodySection::RecordGroup { action, records } => {
                    self.render_cloud_sync_record_group(action, &records, &model.selection, cx)
                }
            });
        }
        let force_upload_available = !local_file_mode
            && (state.auto_upload_blocked_by_conflict
                || state.conflict_details.is_some()
                || state.status == CloudSyncStatus::Conflict);
        let mut actions = vec![self.render_cloud_sync_action_button(
            model.copy.apply_label_key,
            ButtonVariant::Default,
            busy || !model.can_apply,
            self.intent_listener(CloudSyncUiIntent::ApplyPreview),
        )];
        if force_upload_available && !preview.is_backup() {
            actions.push(self.render_cloud_sync_action_button(
                "plugin.cloud_sync.actions.force_upload",
                ButtonVariant::Destructive,
                busy,
                self.intent_listener(CloudSyncUiIntent::ForceUpload),
            ));
        }
        actions.push(self.render_cloud_sync_action_button(
            "plugin.cloud_sync.actions.cancel_preview",
            ButtonVariant::Outline,
            busy,
            {
                let cloud_sync = self.cloud_sync.clone();
                move |_event, _window, cx| {
                    cloud_sync.update(cx, |cloud_sync, cx| {
                        cloud_sync.view.pending_preview = None;
                        cloud_sync.view.preview_selection = None;
                        cloud_sync.clear_select_focus();
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
            },
        ));
        cloud_sync_preview_card(
            &self.tokens,
            self.cloud_sync_has_background(),
            title,
            fact_rows,
            warning,
            body,
            cloud_sync_action_grid(actions),
        )
    }

    pub(super) fn render_cloud_sync_upload_preview(
        &self,
        remote_preview: &CloudSyncPendingPreview,
        state: &CloudSyncPersistedState,
        upload_selection: Option<&CloudSyncUploadSelection>,
        local_file_mode: bool,
        busy: bool,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let title = self.render_display_text_with_role(
            SelectableTextRole::NonSelectable,
            "cloud-sync-upload-preview-title",
            "upload",
            self.i18n.t(if local_file_mode {
                "plugin.cloud_sync.sections.export_preview"
            } else {
                "plugin.cloud_sync.sections.upload_preview"
            }),
            theme.text_heading,
            cx,
        );
        let (selection_rows, raw_scope) = match upload_selection {
            Some(selection) => (
                Some(self.cloud_sync_upload_selection_rows(selection)),
                selection.raw_scope(&state.sync_scope),
            ),
            None => (None, state.sync_scope.clone()),
        };
        let mut body = Vec::new();
        if let Some(selection_rows) = selection_rows {
            body.push(self.render_cloud_sync_upload_selection(&selection_rows, cx));
        }
        if !self.diff.upload_diff_items.is_empty() {
            body.push(self.render_cloud_sync_section_diff_card(
                "cloud-sync-upload-preview-diff",
                "plugin.cloud_sync.preflight.upload_diff_title",
                &self.diff.upload_diff_items,
                cx,
            ));
        }
        let scope = normalize_sync_scope(Some(&raw_scope), &[]);
        let field_diff_items =
            cloud_sync_upload_field_diff_items(remote_preview, &self.diff.local_field_diff, &scope);
        if !field_diff_items.is_empty() {
            body.push(self.render_cloud_sync_upload_field_diff_card(
                &field_diff_items,
                upload_selection,
                cx,
            ));
        }
        let summary = cloud_sync_preview_summary(remote_preview);
        let fact_rows = if matches!(remote_preview, CloudSyncPendingPreview::Structured(_)) {
            Vec::new()
        } else {
            vec![cloud_sync_fact_grid([
                self.render_cloud_sync_fact(
                    "plugin.cloud_sync.preview.connection_count",
                    summary.connections.to_string(),
                    cx,
                ),
                self.render_cloud_sync_fact(
                    "plugin.cloud_sync.preview.quick_commands_label",
                    summary.quick_commands.to_string(),
                    cx,
                ),
            ])]
        };
        let force_upload_available = !local_file_mode
            && (state.auto_upload_blocked_by_conflict
                || state.conflict_details.is_some()
                || state.status == CloudSyncStatus::Conflict);
        let mut actions = vec![self.render_cloud_sync_action_button(
            if local_file_mode {
                "plugin.cloud_sync.actions.export_local"
            } else {
                "plugin.cloud_sync.actions.upload_now"
            },
            ButtonVariant::Default,
            busy,
            self.intent_listener(CloudSyncUiIntent::StartUpload),
        )];
        if force_upload_available {
            actions.push(self.render_cloud_sync_action_button(
                "plugin.cloud_sync.actions.force_upload",
                ButtonVariant::Destructive,
                busy,
                self.intent_listener(CloudSyncUiIntent::ForceUpload),
            ));
        }
        actions.push(self.render_cloud_sync_action_button(
            "plugin.cloud_sync.actions.cancel_preview",
            ButtonVariant::Outline,
            busy,
            {
                let cloud_sync = self.cloud_sync.clone();
                move |_event, _window, cx| {
                    cloud_sync.update(cx, |cloud_sync, cx| {
                        cloud_sync.view.upload_preview = None;
                        cloud_sync.view.upload_selection = None;
                        cloud_sync.clear_select_focus();
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
            },
        ));
        cloud_sync_preview_card(
            &self.tokens,
            self.cloud_sync_has_background(),
            title,
            fact_rows,
            None,
            body,
            cloud_sync_action_grid(actions),
        )
    }

    pub(super) fn render_cloud_sync_preview_impact(
        &self,
        items: &[CloudSyncPreviewImpactItem],
        cx: &mut App,
    ) -> AnyElement {
        let title = self.render_selectable_text_scoped(
            "cloud-sync-preview-impact-title",
            "title",
            self.i18n.t("plugin.cloud_sync.preview.apply_plan_title"),
            self.tokens.ui.text_heading,
            cx,
        );
        let rows = items.iter().map(|item| {
            self.render_cloud_sync_status_row(
                item.label_key,
                Some(self.i18n_replace(
                    "plugin.cloud_sync.preview.item_count",
                    &[("count", item.count.to_string())],
                )),
                item.status,
                cx,
            )
        });
        cloud_sync_status_list(&self.tokens, title, rows)
    }

    pub(super) fn render_cloud_sync_remote_sensitive_summary(
        &self,
        preview: &CloudSyncPendingPreview,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let (connections, portable_secrets) = match preview {
            CloudSyncPendingPreview::Structured(preview) => {
                let preview = preview.sensitive_credentials_preview.as_ref()?;
                (preview.total_connections, preview.portable_secret_count)
            }
            CloudSyncPendingPreview::Legacy { preview, .. } => (
                preview.preview.total_connections,
                preview.metadata.portable_secret_count.unwrap_or(0),
            ),
        };
        if connections == 0 && portable_secrets == 0 {
            return None;
        }
        let title = self.render_selectable_text_scoped(
            "cloud-sync-sensitive-summary-title",
            "title",
            self.i18n
                .t("plugin.cloud_sync.preview.sensitive_summary_title"),
            self.tokens.ui.text_heading,
            cx,
        );
        let summary = self.i18n_replace(
            "plugin.cloud_sync.preview.sensitive_remote_summary",
            &[
                ("connections", connections.to_string()),
                ("portableSecrets", portable_secrets.to_string()),
            ],
        );
        let mut rows = vec![cloud_sync_meta_line(self.render_selectable_text_scoped(
            "cloud-sync-sensitive-summary",
            "summary",
            summary,
            self.tokens.ui.text,
            cx,
        ))];
        let details = match preview {
            CloudSyncPendingPreview::Structured(preview) => preview
                .sensitive_credentials_preview
                .as_ref()
                .map(|p| &p.profile_credentials),
            CloudSyncPendingPreview::Legacy { preview, .. } => {
                Some(&preview.preview.profile_credentials)
            }
        };
        let mut owners = std::collections::BTreeMap::new();
        for detail in details.into_iter().flatten() {
            let counts = owners
                .entry(detail.owner.clone())
                .or_insert((0usize, 0usize));
            if detail.cleared {
                counts.1 += 1;
            } else {
                counts.0 += 1;
            }
        }
        for (index, (owner, (stored, cleared))) in owners.into_iter().enumerate() {
            use oxideterm_connections::CredentialOwner;
            let fallback = match &owner {
                CredentialOwner::Totp(id)
                | CredentialOwner::PluginConnection(id)
                | CredentialOwner::Connection(id)
                | CredentialOwner::StandaloneSftp(id)
                | CredentialOwner::Mosh(id)
                | CredentialOwner::RemoteDesktop(id)
                | CredentialOwner::Telnet(id)
                | CredentialOwner::Ftp(id) => id.clone(),
                CredentialOwner::GlobalProxy => self.i18n.t("modals.upstream_proxy.policy"),
            };
            let name = if let CloudSyncPendingPreview::Structured(preview) = preview {
                match &owner {
                    CredentialOwner::PluginConnection(id) => preview
                        .connections_snapshot
                        .as_ref()
                        .and_then(|s| s.plugin_profiles.iter().find(|p| &p.id == id))
                        .map(|p| p.name.clone()),
                    CredentialOwner::Totp(id) => preview
                        .connections_snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.totp_credentials.iter().find(|p| &p.id == id))
                        .map(|p| p.name.clone()),
                    CredentialOwner::Connection(id) => preview
                        .connections_snapshot
                        .as_ref()
                        .and_then(|s| s.records.iter().find(|p| &p.id == id))
                        .and_then(|p| p.payload.as_ref())
                        .map(|p| p.name.clone()),
                    CredentialOwner::StandaloneSftp(id) => preview
                        .standalone_sftp_profiles_snapshot
                        .as_ref()
                        .and_then(|s| s.records.iter().find(|p| &p.id == id))
                        .map(|p| p.name.clone()),
                    CredentialOwner::Mosh(id) => preview
                        .mosh_profiles_snapshot
                        .as_ref()
                        .and_then(|s| s.records.iter().find(|p| &p.id == id))
                        .map(|p| p.name.clone()),
                    CredentialOwner::RemoteDesktop(id) => preview
                        .remote_desktop_profiles_snapshot
                        .as_ref()
                        .and_then(|s| s.records.iter().find(|p| &p.id == id))
                        .map(|p| p.name.clone()),
                    CredentialOwner::Telnet(id) => preview
                        .telnet_profiles_snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.records.iter().find(|p| &p.id == id))
                        .map(|p| p.name.clone()),
                    CredentialOwner::GlobalProxy => None,
                    CredentialOwner::Ftp(id) => preview
                        .standalone_sftp_profiles_snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.ftp.as_ref())
                        .and_then(|snapshot| {
                            snapshot.records.iter().find(|profile| &profile.id == id)
                        })
                        .map(|profile| profile.name.clone()),
                }
                .unwrap_or(fallback)
            } else {
                fallback
            };
            let description = self.i18n_replace(
                "plugin.cloud_sync.preview.profile_credential_actions",
                &[
                    ("name", name),
                    ("stored", stored.to_string()),
                    ("cleared", cleared.to_string()),
                ],
            );
            rows.push(cloud_sync_meta_line(self.render_selectable_text_scoped(
                "cloud-sync-sensitive-owner",
                index,
                description,
                self.tokens.ui.text,
                cx,
            )));
        }
        Some(cloud_sync_status_list(&self.tokens, title, rows))
    }

    pub(super) fn render_cloud_sync_upload_selection(
        &self,
        rows: &[CloudSyncUploadSelectionRow],
        cx: &mut App,
    ) -> AnyElement {
        let title = self.render_selectable_text_scoped(
            "cloud-sync-upload-selection-title",
            "title",
            self.i18n.t("plugin.cloud_sync.preview.upload_scope_title"),
            self.tokens.ui.text_heading,
            cx,
        );
        let mut block = div().flex().flex_col().gap(px(8.0));
        for row in rows {
            let action = row.action.clone();
            block = block.child(self.render_cloud_sync_check_row(
                self.i18n.t(row.label_key),
                row.meta.clone(),
                row.checked,
                false,
                self.upload_selection_listener(action),
                cx,
            ));
        }
        cloud_sync_status_list(&self.tokens, title, [block.into_any_element()])
    }

    fn cloud_sync_upload_selection_rows(
        &self,
        selection: &CloudSyncUploadSelection,
    ) -> Vec<CloudSyncUploadSelectionRow> {
        let row_specs = [
            (
                "plugin.cloud_sync.settings.sync_connections",
                CloudSyncUploadSelectionAction::ToggleConnections,
            ),
            (
                "plugin.cloud_sync.settings.sync_forwards",
                CloudSyncUploadSelectionAction::ToggleForwards,
            ),
            (
                "plugin.cloud_sync.settings.sync_quick_commands",
                CloudSyncUploadSelectionAction::ToggleQuickCommands,
            ),
            (
                "plugin.cloud_sync.settings.sync_serial_profiles",
                CloudSyncUploadSelectionAction::ToggleSerialProfiles,
            ),
            (
                "plugin.cloud_sync.settings.sync_ble_profiles",
                CloudSyncUploadSelectionAction::ToggleBleProfiles,
            ),
            (
                "plugin.cloud_sync.settings.sync_telnet_profiles",
                CloudSyncUploadSelectionAction::ToggleTelnetProfiles,
            ),
            (
                "plugin.cloud_sync.settings.sync_mosh_profiles",
                CloudSyncUploadSelectionAction::ToggleMoshProfiles,
            ),
            (
                "plugin.cloud_sync.settings.sync_remote_desktop_profiles",
                CloudSyncUploadSelectionAction::ToggleRemoteDesktopProfiles,
            ),
            (
                "plugin.cloud_sync.settings.sync_sensitive_credentials",
                CloudSyncUploadSelectionAction::ToggleSensitiveCredentials,
            ),
            (
                "plugin.cloud_sync.settings.sync_app_settings",
                CloudSyncUploadSelectionAction::ToggleAppSettings,
            ),
            (
                "plugin.cloud_sync.settings.sync_plugin_settings",
                CloudSyncUploadSelectionAction::TogglePluginSettings,
            ),
        ];
        row_specs
            .into_iter()
            .filter_map(|(label_key, action)| {
                self.cloud_sync_upload_section_visible(selection, &action)
                    .then(|| CloudSyncUploadSelectionRow {
                        label_key,
                        meta: self.cloud_sync_upload_selection_meta(selection, &action),
                        checked: selection.is_item_checked(&action),
                        action,
                    })
            })
            .collect()
    }

    pub(super) fn cloud_sync_upload_section_visible(
        &self,
        selection: &CloudSyncUploadSelection,
        action: &CloudSyncUploadSelectionAction,
    ) -> bool {
        match action {
            CloudSyncUploadSelectionAction::ToggleConnections => {
                selection.sync_connections || !selection.connection_item_ids.is_empty()
            }
            CloudSyncUploadSelectionAction::ToggleForwards => {
                selection.sync_forwards || !selection.forward_item_ids.is_empty()
            }
            CloudSyncUploadSelectionAction::ToggleQuickCommands => {
                selection.sync_quick_commands || !selection.quick_command_item_ids.is_empty()
            }
            CloudSyncUploadSelectionAction::ToggleSerialProfiles => {
                selection.sync_serial_profiles || !selection.serial_profile_item_ids.is_empty()
            }
            CloudSyncUploadSelectionAction::ToggleBleProfiles => {
                selection.sync_ble_profiles || !selection.ble_profile_item_ids.is_empty()
            }
            CloudSyncUploadSelectionAction::ToggleTelnetProfiles => {
                selection.sync_telnet_profiles || !selection.telnet_profile_item_ids.is_empty()
            }
            CloudSyncUploadSelectionAction::ToggleMoshProfiles => {
                selection.sync_mosh_profiles || !selection.mosh_profile_item_ids.is_empty()
            }
            CloudSyncUploadSelectionAction::ToggleRemoteDesktopProfiles => {
                selection.sync_remote_desktop_profiles
                    || !selection.remote_desktop_profile_item_ids.is_empty()
            }
            CloudSyncUploadSelectionAction::ToggleSensitiveCredentials => {
                selection.sync_sensitive_credentials
            }
            CloudSyncUploadSelectionAction::ToggleAppSettings => selection.sync_app_settings,
            CloudSyncUploadSelectionAction::TogglePluginSettings => selection.sync_plugin_settings,
            _ => true,
        }
    }

    pub(super) fn cloud_sync_upload_selection_meta(
        &self,
        selection: &CloudSyncUploadSelection,
        action: &CloudSyncUploadSelectionAction,
    ) -> Option<String> {
        let count = match action {
            CloudSyncUploadSelectionAction::ToggleConnections => {
                selection.connection_item_ids.len()
            }
            CloudSyncUploadSelectionAction::ToggleForwards => selection.forward_item_ids.len(),
            CloudSyncUploadSelectionAction::ToggleQuickCommands => {
                selection.quick_command_item_ids.len()
            }
            CloudSyncUploadSelectionAction::ToggleSerialProfiles => {
                selection.serial_profile_item_ids.len()
            }
            CloudSyncUploadSelectionAction::ToggleBleProfiles => {
                selection.ble_profile_item_ids.len()
            }
            CloudSyncUploadSelectionAction::ToggleTelnetProfiles => {
                selection.telnet_profile_item_ids.len()
            }
            CloudSyncUploadSelectionAction::ToggleMoshProfiles => {
                selection.mosh_profile_item_ids.len()
            }
            CloudSyncUploadSelectionAction::ToggleRemoteDesktopProfiles => {
                selection.remote_desktop_profile_item_ids.len()
            }
            CloudSyncUploadSelectionAction::ToggleSensitiveCredentials => {
                return self.diff.upload_sensitive_summary.clone();
            }
            CloudSyncUploadSelectionAction::ToggleAppSettings => {
                selection.selected_app_settings_sections.len()
            }
            _ => 0,
        };
        (count > 0).then(|| {
            self.i18n_replace(
                "plugin.cloud_sync.preview.item_count",
                &[("count", count.to_string())],
            )
        })
    }
}

impl CloudSyncPageRenderer {
    pub(super) fn render_cloud_sync_preview_selection(
        &self,
        summary: &CloudSyncPreviewSummary,
        selection: &CloudSyncPreviewSelection,
        cx: &mut App,
    ) -> AnyElement {
        let mut block = div().flex().flex_col().gap(px(8.0));
        for row in selection.preview_rows(summary) {
            let action = row.action.clone();
            block = block.child(
                self.render_cloud_sync_check_row(
                    self.cloud_sync_preview_selection_label(row.label),
                    row.meta
                        .map(|label| self.cloud_sync_preview_selection_label(label)),
                    row.checked,
                    row.disabled,
                    self.preview_selection_listener(action),
                    cx,
                ),
            );
        }
        block.into_any_element()
    }

    pub(super) fn cloud_sync_preview_selection_label(
        &self,
        label: CloudSyncPreviewSelectionLabel,
    ) -> String {
        match label {
            CloudSyncPreviewSelectionLabel::I18nCount {
                key,
                count_name,
                count,
            } => self.i18n_replace(key, &[(count_name, count.to_string())]),
            CloudSyncPreviewSelectionLabel::AppSettings => {
                self.i18n.t("plugin.cloud_sync.preview.toggle_app_settings")
            }
            CloudSyncPreviewSelectionLabel::AppSettingsSection { section_id } => {
                cloud_sync_app_settings_section_label_key(&section_id)
                    .map(|key| self.i18n.t(key))
                    .unwrap_or(section_id)
            }
            CloudSyncPreviewSelectionLabel::PluginId(plugin_id) => plugin_id,
        }
    }

    pub(super) fn cloud_sync_preview_fact_value(
        &self,
        value: &CloudSyncPreviewFactValue,
    ) -> String {
        match value {
            CloudSyncPreviewFactValue::Count(count) => count.to_string(),
            CloudSyncPreviewFactValue::YesNo(true) => self.i18n.t("plugin.cloud_sync.common.yes"),
            CloudSyncPreviewFactValue::YesNo(false) => self.i18n.t("plugin.cloud_sync.common.no"),
        }
    }

    pub(super) fn render_cloud_sync_check_row(
        &self,
        label: String,
        meta: Option<String>,
        checked: bool,
        disabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label_key = label.clone();
        // Preview rows toggle on the row, matching Tauri checkbox row select-none labels.
        cloud_sync_check_row(
            &self.tokens,
            checked,
            disabled,
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "cloud-sync-preview-check-label",
                label_key,
                label,
                if disabled {
                    theme.text_muted
                } else {
                    theme.text
                },
                cx,
            ),
            meta.map(|meta| {
                let meta_key = meta.clone();
                self.render_display_text_with_role(
                    SelectableTextRole::NonSelectable,
                    "cloud-sync-preview-check-meta",
                    meta_key,
                    meta,
                    theme.text_muted,
                    cx,
                )
            }),
            listener,
        )
    }

    pub(super) fn render_cloud_sync_forward_details(
        &self,
        details: &[CloudSyncForwardDetail],
        cx: &mut App,
    ) -> AnyElement {
        let mut block = self.render_cloud_sync_preview_block(
            self.i18n
                .t("plugin.cloud_sync.preview.forward_details_title"),
            cx,
        );
        let model = cloud_sync_forward_detail_rows(details);
        for row in model.rows {
            block = block.child(self.render_cloud_sync_list_item(row.title, Some(row.meta), cx));
        }
        if model.overflow_count > 0 {
            block = block.child(self.render_cloud_sync_list_more(model.overflow_count));
        }
        block.into_any_element()
    }

    pub(super) fn render_cloud_sync_record_group(
        &self,
        action: &'static str,
        records: &[CloudSyncPreviewRecord],
        selection: &CloudSyncPreviewSelection,
        cx: &mut App,
    ) -> AnyElement {
        let model = cloud_sync_preview_record_group_model(action, records, selection);
        let mut block = self.render_cloud_sync_preview_block(self.i18n.t(model.title_key), cx);
        for row in model.rows {
            match row {
                CloudSyncPreviewRecordRow::Connection {
                    record,
                    checked,
                    disabled,
                } => {
                    let name = record.name.clone();
                    let meta = Some(self.format_cloud_sync_preview_record(&record));
                    block = block.child(self.render_cloud_sync_check_row(
                        record.name.clone(),
                        meta,
                        checked,
                        disabled,
                        {
                            let cloud_sync = self.cloud_sync.clone();
                            move |_event, _window, cx| {
                                cloud_sync.update(cx, |cloud_sync, cx| {
                                    if let Some(selection) =
                                        cloud_sync.view.preview_selection.as_mut()
                                    {
                                        if !selection.selected_connection_names.remove(&name) {
                                            selection
                                                .selected_connection_names
                                                .insert(name.clone());
                                        }
                                        cx.notify();
                                    }
                                });
                                cx.stop_propagation();
                            }
                        },
                        cx,
                    ));
                }
                CloudSyncPreviewRecordRow::Item { record } => {
                    let meta = Some(self.format_cloud_sync_preview_record(&record));
                    block = block.child(self.render_cloud_sync_list_item(record.name, meta, cx));
                }
            }
        }
        if model.overflow_count > 0 {
            block = block.child(self.render_cloud_sync_list_more(model.overflow_count));
        }
        block.into_any_element()
    }

    pub(super) fn render_cloud_sync_preview_block(&self, title: String, cx: &mut App) -> gpui::Div {
        let theme = self.tokens.ui;
        cloud_sync_preview_block(
            &self.tokens,
            self.render_selectable_text_scoped(
                "cloud-sync-preview-block-title",
                title.clone(),
                title,
                theme.text_heading,
                cx,
            ),
        )
    }

    pub(super) fn render_cloud_sync_list_item(
        &self,
        title: String,
        meta: Option<String>,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mono = cloud_sync_value_prefers_mono(&title);
        cloud_sync_list_item(
            &self.tokens,
            self.render_selectable_text_scoped(
                "cloud-sync-list-title",
                title.clone(),
                title,
                theme.text,
                cx,
            ),
            meta.map(|meta| {
                self.render_selectable_text_scoped(
                    "cloud-sync-list-meta",
                    meta.clone(),
                    meta,
                    theme.text_muted,
                    cx,
                )
            }),
            mono,
            Some(self.mono_font_family.clone()),
        )
    }

    pub(super) fn render_cloud_sync_list_more(&self, count: usize) -> AnyElement {
        cloud_sync_list_more(
            &self.tokens,
            self.i18n_replace(
                "plugin.cloud_sync.preview.more_items",
                &[("count", count.to_string())],
            ),
        )
    }

    pub(super) fn format_cloud_sync_preview_record(
        &self,
        record: &CloudSyncPreviewRecord,
    ) -> String {
        let Some(key) = cloud_sync_preview_record_label_key(record.action.as_str()) else {
            return record.reason_code.clone();
        };
        self.i18n_replace(
            key,
            &[
                ("name", record.name.clone()),
                (
                    "target",
                    record
                        .target_name
                        .clone()
                        .unwrap_or_else(|| "—".to_string()),
                ),
            ],
        )
    }
}
