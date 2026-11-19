use super::*;
use oxideterm_gpui_settings_view::ai::providers::{AiProviderBindings, AiProviderRequest};

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_provider_settings_bindings(
        cx: &Context<Self>,
    ) -> AiProviderBindings {
        let owner = cx.weak_entity();
        AiProviderBindings {
            controls: oxideterm_gpui_settings_view::surface::settings_controls_source(
                owner.clone(),
            ),
            status: Rc::new({
                let owner = owner.clone();
                move |id, cx| {
                    owner.upgrade().map_or((false, false), |owner| {
                        let ai = owner.read(cx).ai.state.read(cx);
                        (ai.provider_has_key(id), ai.model_is_refreshing(id))
                    })
                }
            }),
            request: Rc::new(move |request, cx| {
                let _ = owner.update(cx, |app, cx| match request {
                    AiProviderRequest::Edit(edit) => app.edit_settings(edit, cx),
                    AiProviderRequest::LoadKeyStatus(ids) => app
                        .ai
                        .state
                        .update(cx, |ai, _| ai.request_provider_key_statuses(ids)),
                    AiProviderRequest::RefreshModels(id) => {
                        if let Some((index, provider)) = app
                            .settings_store
                            .settings()
                            .ai
                            .providers
                            .iter()
                            .enumerate()
                            .filter_map(|(index, provider)| {
                                Some((index, ai_provider_view(provider)?))
                            })
                            .find(|(_, provider)| provider.id == id)
                        {
                            app.refresh_ai_provider_models(index, provider, cx);
                        }
                    }
                    AiProviderRequest::SaveKey { index, provider_id } => {
                        if app
                            .settings_store
                            .settings()
                            .ai
                            .providers
                            .get(index)
                            .and_then(ai_provider_id)
                            .as_deref()
                            == Some(provider_id.as_str())
                        {
                            app.save_ai_provider_api_key(index, cx);
                        }
                    }
                    AiProviderRequest::Confirm(intent, name) => {
                        app.open_ai_settings_confirm(intent, name, cx)
                    }
                    AiProviderRequest::ClearModelDraft(input) => {
                        app.settings_workspace.update(cx, |settings, cx| {
                            settings.clear_provider_model_draft(input, cx)
                        });
                        app.clear_ime_selection();
                    }
                });
            }),
        }
    }

    pub(in crate::workspace) fn ai_provider_has_key_cached(
        &self,
        provider_id: &str,
        cx: &App,
    ) -> bool {
        self.ai.state.read(cx).provider_has_key(provider_id)
    }

    pub(in crate::workspace) fn ensure_ai_provider_key_statuses(&mut self, cx: &mut Context<Self>) {
        let provider_views = ai_provider_views(self.settings_store.settings());
        self.ensure_ai_provider_key_statuses_for_views(&provider_views, cx);
    }

    pub(in crate::workspace) fn ensure_ai_provider_key_statuses_for_views(
        &mut self,
        provider_views: &[AiProviderView],
        cx: &mut Context<Self>,
    ) {
        // Rendering OxideSens already derives provider views, so reuse that
        // snapshot when available instead of parsing the same JSON again.
        let provider_jobs: Vec<_> = provider_views
            .iter()
            .filter(|provider| {
                ai_provider_key_display_state(&provider.provider_type, false).shows_key_control()
            })
            .map(|provider| provider.id.clone())
            .collect();

        self.ai.state.update(cx, |ai, _cx| {
            ai.request_provider_key_statuses(provider_jobs);
        });
    }

    pub(in crate::workspace) fn save_ai_provider_api_key(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(provider_id) = self
            .settings_store
            .settings()
            .ai
            .providers
            .get(index)
            .and_then(ai_provider_id)
        else {
            return;
        };
        // Match Tauri ProviderKeyInput: the visible UI draft is moved into a
        // zeroizing owner before crossing into the keychain boundary, and it is
        // never written into persisted settings.
        let input = SettingsInput::AiProviderApiKey(index);
        let Some(secret) = self
            .settings_workspace
            .update(cx, |settings, cx| settings.take_provider_key(input, cx))
        else {
            return;
        };
        self.ai.state.update(cx, |ai, cx| {
            ai.store_provider_key(index, provider_id, secret, cx);
        });
    }

    pub(in crate::workspace) fn remove_ai_provider_api_key(
        &mut self,
        provider_id: &str,
        cx: &mut Context<Self>,
    ) {
        self.ai.state.update(cx, |ai, cx| {
            ai.remove_provider_key(provider_id.to_string(), cx);
        });
        cx.notify();
    }
}
