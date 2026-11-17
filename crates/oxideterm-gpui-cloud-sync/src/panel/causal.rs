use super::*;

impl CloudSyncPageRenderer {
    pub(super) fn render_causal_preview(
        &self,
        summary: &oxideterm_cloud_sync::operation::SyncPlanSummary,
        previews: &[oxideterm_cloud_sync::operation::SyncConflictPreview],
        choices: &std::collections::BTreeMap<usize, usize>,
        changing_password: bool,
        busy: bool,
        cx: &mut App,
    ) -> AnyElement {
        let mut body = Vec::new();
        let mut directions = div().flex().flex_wrap().gap(px(8.0));
        for (direction, key) in [
            (SyncDirection::Download, "plugin.cloud_sync.causal.download"),
            (SyncDirection::Merge, "plugin.cloud_sync.causal.merge"),
            (SyncDirection::Upload, "plugin.cloud_sync.causal.upload"),
        ] {
            directions = directions.child(self.render_cloud_sync_action_button(
                key,
                if summary.direction == direction {
                    ButtonVariant::Default
                } else {
                    ButtonVariant::Outline
                },
                busy || (direction != SyncDirection::Upload && !summary.has_remote),
                self.intent_listener(CloudSyncUiIntent::ChooseDirection(direction)),
            ));
        }
        if !changing_password {
            body.push(directions.into_any_element());
        }
        let hint_key = match summary.direction {
            SyncDirection::Download => "plugin.cloud_sync.causal.download_hint",
            SyncDirection::Merge => "plugin.cloud_sync.causal.merge_hint",
            SyncDirection::Upload => "plugin.cloud_sync.causal.upload_hint",
        };
        body.push(
            div()
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t(if changing_password {
                    "plugin.cloud_sync.causal.change_password_description"
                } else {
                    hint_key
                }))
                .into_any_element(),
        );
        for (index, conflict) in summary.conflicts.iter().enumerate() {
            let preview = previews.get(index);
            let title = div()
                .child(
                    preview
                        .and_then(|preview| preview.name.clone())
                        .unwrap_or_else(|| self.i18n.t("plugin.cloud_sync.causal.configuration")),
                )
                .into_any_element();
            let mut buttons = div().flex().flex_wrap().gap(px(8.0));
            for (candidate, _) in conflict.candidates.iter().enumerate() {
                let label = format!(
                    "{} {}{}",
                    self.i18n.t("plugin.cloud_sync.causal.candidate"),
                    candidate + 1,
                    if choices.get(&index) == Some(&candidate) {
                        " ✓"
                    } else {
                        ""
                    }
                );
                let value = preview
                    .and_then(|preview| preview.candidates.get(candidate))
                    .and_then(|value| value.as_ref())
                    .map(|value| {
                        if conflict.field.is_presence() {
                            self.i18n.t(if value.as_str() == "true" {
                                "plugin.cloud_sync.causal.keep"
                            } else {
                                "common.delete"
                            })
                        } else {
                            value.to_string()
                        }
                    })
                    .unwrap_or_else(|| self.i18n.t("plugin.cloud_sync.causal.protected"));
                let label = if preview.is_some_and(|preview| preview.current == Some(candidate)) {
                    format!(
                        "{label} · {}",
                        self.i18n.t("plugin.cloud_sync.causal.current")
                    )
                } else {
                    label
                };
                buttons = buttons.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(div().max_w(px(360.0)).child(value))
                        .child(
                            self.workspace_toolbar_action_button(
                                label,
                                None,
                                cloud_sync_button_options(ButtonVariant::Outline, busy),
                                self.intent_listener(CloudSyncUiIntent::ChooseCausal {
                                    conflict: index,
                                    candidate,
                                }),
                            )
                            .into_any_element(),
                        ),
                );
            }
            body.push(cloud_sync_list_item(
                &self.tokens,
                title,
                Some(buttons.into_any_element()),
                false,
                None,
            ));
        }
        let actions = div()
            .flex()
            .gap(px(8.0))
            .child(
                self.render_cloud_sync_action_button(
                    match summary.direction {
                        SyncDirection::Download => "plugin.cloud_sync.causal.apply_download",
                        SyncDirection::Merge => "plugin.cloud_sync.causal.apply",
                        SyncDirection::Upload => "plugin.cloud_sync.causal.apply_upload",
                    },
                    ButtonVariant::Default,
                    busy || (summary.direction == SyncDirection::Download
                        && summary
                            .conflicts
                            .iter()
                            .enumerate()
                            .any(|(index, _)| !choices.contains_key(&index))),
                    self.intent_listener(CloudSyncUiIntent::ApplyCausal),
                ),
            )
            .child(self.render_cloud_sync_action_button(
                "plugin.cloud_sync.actions.cancel_preview",
                ButtonVariant::Outline,
                busy,
                self.intent_listener(CloudSyncUiIntent::CancelCausal),
            ))
            .into_any_element();
        cloud_sync_preview_card(
            &self.tokens,
            self.has_background,
            div()
                .child(self.i18n.t("plugin.cloud_sync.causal.preview"))
                .into_any_element(),
            [self.render_cloud_sync_fact(
                "plugin.cloud_sync.causal.changes",
                summary.changed_fields.to_string(),
                cx,
            )],
            if !summary.has_remote {
                Some(self.i18n.t("plugin.cloud_sync.causal.remote_empty"))
            } else if summary.first_sync && !changing_password {
                Some(self.i18n.t("plugin.cloud_sync.causal.first_hint"))
            } else if summary.upgrading {
                Some(self.i18n.t("plugin.cloud_sync.causal.upgrade"))
            } else if !summary.conflicts.is_empty() {
                Some(self.i18n.t("plugin.cloud_sync.causal.conflict_hint"))
            } else {
                None
            },
            body,
            actions,
        )
    }
}
