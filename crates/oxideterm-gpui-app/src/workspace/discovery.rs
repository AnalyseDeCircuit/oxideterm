mod open;
pub(super) use open::{DiscoveryOpenKey, PendingDiscoveryOpen};

use super::*;
use oxideterm_discovery::{DiscoveryFailure, DiscoverySource, ProviderAvailability};
use oxideterm_gpui_settings_view::connections_page::DiscoveryProviderChoice;

fn discovery_secret_account(
    plugin: &str,
    binding: &oxideterm_settings::DiscoverySecretBinding,
) -> anyhow::Result<String> {
    use oxideterm_settings::DiscoverySecretBinding;
    match binding {
        DiscoverySecretBinding::Plugin(key) => {
            oxideterm_plugin_host_api::secrets::plugin_secret_account_id(plugin, key)
                .map_err(anyhow::Error::msg)
        }
        DiscoverySecretBinding::SshPassphrase(account) => Ok(account.clone()),
    }
}

fn failure_message(i18n: &I18n, error: &DiscoveryFailure) -> String {
    match error {
        DiscoveryFailure::Provider(error) => error.message.clone(),
        DiscoveryFailure::Unavailable => i18n.t("settings_view.discovery.unavailable"),
        DiscoveryFailure::PermissionDenied => i18n.t("settings_view.discovery.permission_denied"),
        DiscoveryFailure::StaleTarget => i18n.t("settings_view.discovery.stale_target"),
        DiscoveryFailure::InvalidResponse => i18n.t("settings_view.discovery.invalid_response"),
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn node_allows_session_tools(&self, node_id: &NodeId) -> bool {
        self.node_router
            .runtime_store()
            .discovery_origin(node_id)
            .is_none_or(|origin| origin.allows_asset_channels())
    }

    pub(in crate::workspace) fn node_allows_drill_down(&self, node_id: &NodeId) -> bool {
        self.node_router
            .runtime_store()
            .discovery_origin(node_id)
            .is_none()
    }

    pub(in crate::workspace) fn discovery_node_allows_target(&self, node_id: &NodeId) -> bool {
        self.node_router
            .runtime_store()
            .discovery_origin(node_id)
            .is_none_or(|origin| origin.allows_target_operations())
    }

    pub(in crate::workspace) fn discovery_pane_allows_target(
        &self,
        pane_id: PaneId,
        cx: &App,
    ) -> bool {
        if let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id)
            && self
                .plugin_entity
                .read(cx)
                .discovery_terminal(&pane.read(cx).shared_session())
        {
            return false;
        }
        self.session_id_for_pane(pane_id, cx)
            .and_then(|session| {
                self.workspace_runtime
                    .read(cx)
                    .ssh_terminal_node_id(session)
            })
            .is_none_or(|id| self.discovery_node_allows_target(&id))
    }

    fn load_discovery_settings(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let saved = SettingsStore::load_from_path(self.settings_store.path())?;
        self.apply_discovery_settings(&saved, cx);
        Ok(())
    }

    fn apply_discovery_settings(&mut self, saved: &SettingsStore, cx: &mut Context<Self>) {
        let sources = &saved.settings().discovery_sources;
        let previous = &self.settings_store.settings().discovery_sources;
        if previous != sources {
            self.settings_workspace.update(cx, |state, cx| {
                for old in previous {
                    if !sources.iter().any(|new| new == old) {
                        state.clear_discovery_feedback(&old.id, cx);
                    }
                }
            });
            self.settings_store.settings_mut().discovery_sources = sources.clone();
        }
    }

    pub(in crate::workspace) fn reconcile_discovery_sources(&mut self, cx: &mut Context<Self>) {
        let Ok(saved) = SettingsStore::load_from_path(self.settings_store.path()) else {
            self.report_discovery_failure(DiscoveryFailure::Unavailable, cx);
            return;
        };
        let plugin = self.plugin_entity.read(cx);
        let mut providers = Vec::new();
        for (plugin_id, def) in plugin.registry().discovery_providers() {
            let mut def = def.clone();
            let locale = plugin_ui::native_plugin_locale_tag(self.i18n.locale());
            let translate = |title: &str| {
                title
                    .strip_prefix('@')
                    .and_then(|key| {
                        def.form
                            .translations
                            .get(locale)
                            .and_then(|m| m.get(key))
                            .or_else(|| def.form.translations.get("en").and_then(|m| m.get(key)))
                    })
                    .cloned()
                    .unwrap_or_else(|| title.to_owned())
            };
            let title = translate(&def.title);
            for action in &mut def.source_actions {
                action.title = translate(&action.title);
            }
            plugin_ui::localize_native_plugin_schema(&mut def.form, self.i18n.locale());
            providers.push(DiscoveryProviderChoice::new(plugin_id.into(), title, def));
        }
        self.discovery.configure_saved(&saved, |s| {
            match s
                .plugin_id
                .as_ref()
                .filter(|_| providers.iter().any(|choice| choice.matches(s)))
            {
                Some(id) => plugin
                    .discovery_process(id, &s.provider_id)
                    .map(|p| ProviderAvailability::Active(Arc::new(p)))
                    .unwrap_or(ProviderAvailability::AwaitingActivation),
                None => ProviderAvailability::Unavailable,
            }
        });
        self.apply_discovery_settings(&saved, cx);
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_discovery_providers(providers, cx)
        });
        self.publish_discovery_catalog(cx);
    }

    pub(in crate::workspace) fn publish_discovery_catalog(&mut self, cx: &mut Context<Self>) {
        if !self
            .discovery
            .matches_sources(&self.settings_store.settings().discovery_sources)
            && self.load_discovery_settings(cx).is_err()
        {
            self.report_discovery_failure(DiscoveryFailure::Unavailable, cx);
            return;
        }
        let withdrawn = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .and_then(|f| f.discovery_origin.as_ref())
            .is_some_and(|o| {
                !o.usable()
                    || self.discovery.epoch(&o.source_id) != Some(o.epoch)
                    || self.discovery.target_revision(&o.source_id, &o.target_id)
                        != Some(o.target_revision.clone())
            });
        if withdrawn {
            self.update_connection_form_state(cx, |state| state.form = None);
        }
        let entries = self.discovery.visible_entries();
        let palette_rows_changed = self.command_palette.read(cx).is_open()
            && self.connections.session_manager.read(cx).discovery_entries != entries;
        let selected = if palette_rows_changed {
            self.selected_palette_item_id(cx)
        } else {
            None
        };
        let mut groups = Vec::new();
        for source in self.settings_store.settings().discovery_sources.iter() {
            let Some(snapshot) = self.discovery.snapshot(&source.id) else {
                continue;
            };
            let status = if let Some(error) = &snapshot.failure {
                Some(self.i18n.t_with(
                    "settings_view.discovery.refresh_failed",
                    &[("reason", &failure_message(&self.i18n, error))],
                ))
            } else {
                snapshot.refreshed_at.map(|at| {
                    let local = chrono::DateTime::<chrono::Local>::from(at);
                    self.i18n.t_with(
                        "settings_view.discovery.refreshed",
                        &[
                            ("time", &local.format("%H:%M:%S").to_string()),
                            ("count", &snapshot.targets.len().to_string()),
                        ],
                    )
                })
            };
            if source.enabled && self.discovery.provider_available(&source.id) {
                let hint = snapshot
                    .failure
                    .as_ref()
                    .map(|error| failure_message(&self.i18n, error))
                    .or_else(|| {
                        if !snapshot.complete {
                            Some(self.i18n.t("settings_view.known_hosts.loading"))
                        } else if snapshot.targets.is_empty() {
                            Some(self.i18n.t("settings_view.discovery.no_targets"))
                        } else {
                            None
                        }
                    });
                groups.push(
                    oxideterm_gpui_connections::session_manager::DiscoveryCatalogGroup {
                        id: source.id.clone(),
                        name: source.name.clone(),
                        failed: snapshot.failure.is_some(),
                        hint,
                    },
                );
            }
            self.settings_workspace.update(cx, |s, cx| {
                s.set_discovery_catalog(source.id.clone(), snapshot.refreshing, status, cx)
            });
        }
        let sources = &self.settings_store.settings().discovery_sources;
        self.connections.session_manager.update(cx, |s, cx| {
            s.expanded_discovery
                .retain(|key| sources.iter().any(|source| key.first() == Some(&source.id)));
            s.update_discovery(entries, groups, cx);
        });
        if let Some(id) = selected {
            self.restore_palette_item(&id, cx);
        }
        if palette_rows_changed {
            App::notify(cx, self.palette_surface.entity_id());
        }
    }

    pub(in crate::workspace) fn report_discovery_failure(
        &mut self,
        error: DiscoveryFailure,
        cx: &mut Context<Self>,
    ) {
        self.report_discovery_status(failure_message(&self.i18n, &error), cx);
    }

    pub(in crate::workspace) fn report_discovery_open_failure(
        &self,
        error: DiscoveryFailure,
        cx: &mut Context<Self>,
    ) {
        self.report_discovery_open_error(failure_message(&self.i18n, &error), cx);
    }

    fn report_discovery_status(&mut self, message: String, cx: &mut Context<Self>) {
        self.connections
            .session_manager
            .update(cx, |s, cx| s.set_status(Some(message), cx));
    }

    pub(in crate::workspace) fn run_discovery_source_action(
        &mut self,
        id: String,
        action_id: String,
        cx: &mut Context<Self>,
    ) {
        let request = self
            .settings_workspace
            .update(cx, |s, cx| s.begin_discovery_action(&id, cx));
        let Some((cancel, task)) = self.discovery.source_action(&id, &action_id) else {
            let message = failure_message(&self.i18n, &DiscoveryFailure::Unavailable);
            self.settings_workspace.update(cx, |s, cx| {
                s.finish_discovery_action(&id, request, Some(message), cx)
            });
            return;
        };
        cx.spawn(async move |workspace, cx| {
            let result = task.await.unwrap_or(Err(DiscoveryFailure::Unavailable));
            let _ = workspace.update(cx, |this, cx| {
                let message = result
                    .err()
                    .filter(|_| !cancel.is_cancelled())
                    .map(|error| failure_message(&this.i18n, &error));
                this.settings_workspace.update(cx, |s, cx| {
                    s.finish_discovery_action(&id, request, message, cx)
                });
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn save_discovery_source(
        &mut self,
        source: DiscoverySource,
        secrets: Vec<(String, zeroize::Zeroizing<String>)>,
        cx: &mut Context<Self>,
    ) -> bool {
        if source.id.is_empty() || source.name.trim().is_empty() || !source.config.is_object() {
            self.report_discovery_failure(DiscoveryFailure::InvalidResponse, cx);
            return false;
        }
        let previous_source = self.discovery.source(&source.id);
        if previous_source
            .as_ref()
            .is_some_and(|old| old.removal_pending)
        {
            self.report_discovery_status(
                self.i18n.t("settings_view.discovery.removal_pending"),
                cx,
            );
            return false;
        }
        let result = (|| -> anyhow::Result<()> {
            let mut settings = SettingsStore::load_from_path(self.settings_store.path())?;
            let plugin = source
                .plugin_id
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Missing provider"))?;
            let store = self.ai.state.read(cx).key_store().clone();
            let outcome = settings.save_discovery_source(
                source,
                previous_source.is_none(),
                secrets,
                |key, value| {
                    store.store_provider_key(&discovery_secret_account(&plugin, key)?, value)
                },
                |key| store.delete_provider_key(&discovery_secret_account(&plugin, key)?),
            )?;
            self.apply_discovery_settings(&settings, cx);
            if outcome == oxideterm_settings::DiscoverySaveOutcome::CleanupPending {
                self.push_command_palette_toast(
                    self.i18n.t("settings_view.discovery.title"),
                    Some(self.i18n.t("settings_view.discovery.cleanup_pending")),
                    TerminalNoticeVariant::Warning,
                    cx,
                );
            }
            Ok(())
        })();
        if result.is_err() {
            self.report_discovery_failure(DiscoveryFailure::Unavailable, cx);
            return false;
        }
        self.reconcile_discovery_sources(cx);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn delete_discovery_source(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        let begin =
            SettingsStore::load_from_path(self.settings_store.path()).and_then(|mut settings| {
                let drained = self.discovery.begin_remove(id, &mut settings)?;
                self.apply_discovery_settings(&settings, cx);
                Ok(drained)
            });
        let mut drained = match begin {
            Ok(Some(drained)) => drained,
            Ok(None) => {
                self.push_command_palette_toast(
                    self.i18n.t("settings_view.discovery.title"),
                    Some(self.i18n.t("settings_view.discovery.source_in_use")),
                    TerminalNoticeVariant::Warning,
                    cx,
                );
                return;
            }
            Err(_) => {
                self.push_command_palette_toast(
                    self.i18n.t("settings_view.discovery.title"),
                    Some(self.i18n.t("settings_view.discovery.unavailable")),
                    TerminalNoticeVariant::Error,
                    cx,
                );
                return;
            }
        };
        let id = id.to_owned();
        cx.spawn(async move |workspace, cx| {
            while drained.changed().await.is_ok() {}
            let _ = workspace.update(cx, |this, cx| {
                this.finish_deleting_discovery_source(&id, cx)
            });
        })
        .detach();
    }

    fn finish_deleting_discovery_source(&mut self, id: &str, cx: &mut Context<Self>) {
        let result = (|| -> anyhow::Result<()> {
            let mut settings = SettingsStore::load_from_path(self.settings_store.path())?;
            if let Some(source) = settings
                .settings()
                .discovery_sources
                .iter()
                .find(|s| s.id == id && s.removal_pending)
                && let Some(plugin) = &source.plugin_id
            {
                let store = self.ai.state.read(cx).key_store().clone();
                for key in source.deletion_bindings() {
                    let account = discovery_secret_account(plugin, &key)?;
                    store.delete_provider_key(&account)?;
                }
            }
            self.discovery.finish_remove(id, &mut settings)?;
            self.apply_discovery_settings(&settings, cx);
            Ok(())
        })();
        if result.is_err() {
            self.report_discovery_status(
                self.i18n.t("settings_view.discovery.removal_pending"),
                cx,
            );
        } else {
            self.reconcile_discovery_sources(cx);
        }
        cx.notify();
    }
}
