use super::*;
use gpui::{Div, Stateful};

pub struct KnowledgeSelectInputs<'a> {
    pub connections: &'a [oxideterm_connections::SavedConnection],
    pub collection_scope: Option<&'a str>,
    pub document_format: &'a str,
}

impl SettingsSelectMenu<'_> {
    pub(super) fn knowledge_popup(
        &self,
        select: SettingsSelect,
        width: f32,
    ) -> Option<Stateful<Div>> {
        match select {
            SettingsSelect::AiEmbeddingProvider => {
                let current = self
                    .settings
                    .ai
                    .embedding_config
                    .as_ref()
                    .and_then(|config| config.get("providerId"))
                    .and_then(serde_json::Value::as_str);
                let mut popup = select_panel_overlay_popup_with_max_height(
                    self.tokens,
                    width.max(224.0),
                    320.0,
                );
                let providers = std::iter::once((
                    None,
                    self.i18n
                        .t("settings_view.knowledge.auto_embedding_provider"),
                ))
                .chain(
                    oxideterm_settings_model::ai_provider_views(self.settings)
                        .into_iter()
                        .map(|provider| (Some(provider.id), provider.name)),
                );
                for (provider, label) in providers {
                    popup = popup.child(select_option_action(
                        select_option(self.tokens, label, current == provider.as_deref()),
                        false,
                        false,
                        self.action(move |actions, _, window, cx| {
                            actions.send(SelectMenuRequest::Close, window, cx);
                            let provider = provider.clone();
                            actions.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    let model = settings
                                        .ai
                                        .embedding_config
                                        .as_ref()
                                        .and_then(|config| config.get("model"))
                                        .and_then(serde_json::Value::as_str)
                                        .unwrap_or_default();
                                    settings.ai.embedding_config = Some(serde_json::json!({
                                        "providerId": provider,
                                        "model": model,
                                    }));
                                })),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            SettingsSelect::KnowledgeCollectionScope => {
                let mut popup = select_panel_overlay_popup_with_max_height(
                    self.tokens,
                    width.max(220.0),
                    320.0,
                );
                let scopes =
                    std::iter::once((None, self.i18n.t("settings_view.knowledge.scope_global")))
                        .chain(self.knowledge.connections.iter().map(|connection| {
                            (Some(connection.id.clone()), connection.name.clone())
                        }));
                for (scope, label) in scopes {
                    popup = popup.child(select_option_action(
                        select_option(
                            self.tokens,
                            label,
                            self.knowledge.collection_scope == scope.as_deref(),
                        ),
                        false,
                        false,
                        self.action(move |actions, _, window, cx| {
                            actions.send(SelectMenuRequest::Close, window, cx);
                            actions.send(
                                SelectMenuRequest::KnowledgeCollectionScope(scope.clone()),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            SettingsSelect::KnowledgeDocumentFormat => {
                let mut popup = select_overlay_popup(self.tokens, width.max(220.0));
                for (format, label) in [
                    ("markdown", "settings_view.knowledge.format_markdown"),
                    ("plaintext", "settings_view.knowledge.format_plain_text"),
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            self.tokens,
                            self.i18n.t(label),
                            self.knowledge.document_format == format,
                        ),
                        false,
                        false,
                        self.action(move |actions, _, window, cx| {
                            actions.send(SelectMenuRequest::Close, window, cx);
                            actions.send(
                                SelectMenuRequest::KnowledgeDocumentFormat(format),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            _ => None,
        }
    }
}
