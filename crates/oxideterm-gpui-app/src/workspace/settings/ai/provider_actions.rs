impl WorkspaceApp {
    pub(in crate::workspace) fn refresh_ai_provider_models(
        &mut self,
        index: usize,
        provider: AiProviderView,
        cx: &mut Context<Self>,
    ) {
        if self.ai_model_refreshing.contains(&provider.id) {
            cx.notify();
            return;
        }

        let api_key = match ai_provider_refresh_key_policy(&provider.provider_type) {
            AiProviderRefreshKeyPolicy::NoKey => None,
            AiProviderRefreshKeyPolicy::OptionalStoredKey => {
                self.ai_key_store.get_provider_key(&provider.id).ok().flatten()
            }
            AiProviderRefreshKeyPolicy::RequiredStoredKey => {
                match self.ai_key_store.get_provider_key(&provider.id) {
                    Ok(Some(key)) => Some(key),
                    Ok(None) => {
                        self.ai_provider_key_status.insert(provider.id.clone(), false);
                        self.push_ai_settings_toast(
                            self.i18n.t("settings_view.ai.api_key_missing"),
                            TerminalNoticeVariant::Warning,
                        );
                        cx.notify();
                        return;
                    }
                    Err(error) => {
                        self.push_ai_settings_toast(
                            self.ai_i18n_error(
                                "settings_view.ai.refresh_failed",
                                &error.to_string(),
                            ),
                            TerminalNoticeVariant::Error,
                        );
                        cx.notify();
                        return;
                    }
                }
            }
        };

        self.next_ai_model_refresh_generation =
            self.next_ai_model_refresh_generation.saturating_add(1);
        let generation = self.next_ai_model_refresh_generation;
        self.ai_model_refresh_generations
            .insert(provider.id.clone(), generation);
        self.ai_model_refreshing.insert(provider.id.clone());
        cx.notify();

        let provider_id = provider.id.clone();
        if self.ai_model_refresh_tx.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            self.ai_model_refresh_tx = Some(tx);
            self.ai_model_refresh_rx = Some(rx);
        }
        let Some(ui_tx) = self.ai_model_refresh_tx.as_ref().cloned() else {
            return;
        };
        self.ai_model_refresh_pending = self.ai_model_refresh_pending.saturating_add(1);
        self.forwarding_runtime.spawn(async move {
            let result = fetch_provider_models(provider, api_key).await;
            let result = result.map_err(|error| error.to_string());
            let _ = ui_tx.send(AiModelRefreshDelivery {
                index,
                provider_id,
                generation,
                result,
            });
        });
        self.schedule_ai_model_refresh_poll(cx);
    }

    pub(super) fn poll_ai_model_refresh_results(&mut self, cx: &mut Context<Self>) {
        let Some(rx) = self.ai_model_refresh_rx.take() else {
            return;
        };
        let mut keep_rx = true;
        loop {
            match rx.try_recv() {
                Ok(delivery) => {
                    self.ai_model_refresh_pending =
                        self.ai_model_refresh_pending.saturating_sub(1);
                    if self
                        .ai_model_refresh_generations
                        .get(&delivery.provider_id)
                        != Some(&delivery.generation)
                    {
                        continue;
                    }
                    self.ai_model_refreshing.remove(&delivery.provider_id);
                    match delivery.result {
                        Ok(refresh) => {
                            self.edit_settings(
                                |settings| {
                                    ai_apply_provider_model_refresh(
                                        &mut settings.ai.providers,
                                        &mut settings.ai.model_context_windows,
                                        delivery.index,
                                        &delivery.provider_id,
                                        refresh,
                                    );
                                },
                                cx,
                            );
                        }
                        Err(error) => {
                            self.push_ai_settings_toast(
                                self.ai_i18n_error("settings_view.ai.refresh_failed", &error),
                                TerminalNoticeVariant::Error,
                            );
                            cx.notify();
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    keep_rx = false;
                    self.ai_model_refresh_tx = None;
                    self.ai_model_refresh_pending = 0;
                    break;
                }
            }
        }
        if keep_rx && self.ai_model_refresh_pending > 0 {
            self.ai_model_refresh_rx = Some(rx);
        } else if self.ai_model_refresh_pending == 0 {
            self.ai_model_refresh_tx = None;
        }
    }

    fn schedule_ai_model_refresh_poll(&mut self, cx: &mut Context<Self>) {
        if self.ai_model_refresh_polling {
            return;
        }
        self.ai_model_refresh_polling = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(50)).await;
            let _ = weak.update(cx, |this, cx| {
                this.ai_model_refresh_polling = false;
                this.poll_ai_model_refresh_results(cx);
                if this.ai_model_refresh_pending > 0 {
                    this.schedule_ai_model_refresh_poll(cx);
                }
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn push_ai_settings_toast(
        &mut self,
        title: String,
        variant: TerminalNoticeVariant,
    ) {
        self.workspace_toasts.push(WorkspaceToast {
            notice: TerminalNotice {
                title,
                description: None,
                status_text: None,
                progress: None,
                variant,
            },
            expires_at: Instant::now() + Duration::from_secs(4),
        });
    }

}

pub(super) struct AiModelRefreshDelivery {
    pub(super) index: usize,
    pub(super) provider_id: String,
    pub(super) generation: u64,
    pub(super) result: Result<oxideterm_ai::ProviderModelRefresh, String>,
}
