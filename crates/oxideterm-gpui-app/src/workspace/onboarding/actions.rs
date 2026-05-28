use super::*;
use oxideterm_connections::{
    list_ssh_config_hosts, resolve_ssh_config_alias, saved_connection_from_ssh_host,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn open_onboarding_from_palette(&mut self, cx: &mut Context<Self>) {
        self.edit_settings(|settings| settings.onboarding_completed = false, cx);
        self.onboarding
            .reset_for_open(self.settings_store.settings());
        cx.notify();
    }

    pub(in crate::workspace) fn complete_onboarding(&mut self, cx: &mut Context<Self>) {
        let ai_opt_in = self.onboarding.ai_opt_in;
        let tool_use_opt_in = self.onboarding.tool_use_opt_in;
        self.edit_settings(
            move |settings| {
                settings.onboarding_completed = true;
                if ai_opt_in {
                    settings.ai.enabled = true;
                    settings.ai.enabled_confirmed = true;
                }
                if ai_opt_in && tool_use_opt_in {
                    settings.ai.tool_use.enabled = true;
                }
            },
            cx,
        );
        self.onboarding.open = false;
        cx.notify();
    }

    pub(in crate::workspace) fn onboarding_go_to_step(
        &mut self,
        step: usize,
        cx: &mut Context<Self>,
    ) {
        if step >= ONBOARDING_TOTAL_STEPS || (!self.onboarding.disclaimer_accepted && step > 1) {
            return;
        }
        self.onboarding.step = step;
        self.onboarding.scroll_handle = ScrollHandle::new();
        if OnboardingStep::from_index(step) == OnboardingStep::CliCompanion
            && self.settings_page.cli_companion_status.is_none()
            && !self.settings_page.cli_companion_loading
        {
            self.refresh_cli_companion_status(cx);
        }
        if OnboardingStep::from_index(step) == OnboardingStep::QuickStart
            && self.onboarding.host_count.is_none()
        {
            self.refresh_onboarding_ssh_host_count(cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn onboarding_next(&mut self, cx: &mut Context<Self>) {
        if self.onboarding.step == 1 && !self.onboarding.disclaimer_accepted {
            return;
        }
        if self.onboarding.step + 1 < ONBOARDING_TOTAL_STEPS {
            self.onboarding_go_to_step(self.onboarding.step + 1, cx);
        } else {
            self.complete_onboarding(cx);
        }
    }

    pub(in crate::workspace) fn onboarding_back(&mut self, cx: &mut Context<Self>) {
        if self.onboarding.step > 0 {
            self.onboarding_go_to_step(self.onboarding.step - 1, cx);
        }
    }

    pub(in crate::workspace) fn onboarding_skip_to_quick_start(&mut self, cx: &mut Context<Self>) {
        if self.onboarding.disclaimer_accepted {
            self.onboarding_go_to_step(ONBOARDING_TOTAL_STEPS - 1, cx);
        }
    }

    pub(in crate::workspace) fn close_onboarding_if_allowed(&mut self, cx: &mut Context<Self>) {
        if self.onboarding.disclaimer_accepted {
            self.complete_onboarding(cx);
        }
    }

    pub(in crate::workspace) fn handle_onboarding_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.onboarding.open {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.close_onboarding_if_allowed(cx),
            "enter" => self.onboarding_next(cx),
            "arrowleft" => self.onboarding_back(cx),
            "arrowright" => self.onboarding_next(cx),
            _ => return false,
        }
        true
    }

    pub(in crate::workspace) fn refresh_onboarding_ssh_host_count(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let runtime = self.forwarding_runtime.clone();
        cx.spawn(async move |weak, cx| {
            let count = runtime
                .spawn_blocking(|| {
                    list_ssh_config_hosts(&HashSet::new())
                        .map(|hosts| hosts.into_iter().filter(|host| host.alias != "*").count())
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()))
                .unwrap_or(0);
            let _ = weak.update(cx, |this, cx| {
                this.onboarding.host_count = Some(count);
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn import_onboarding_ssh_hosts(&mut self, cx: &mut Context<Self>) {
        if self.onboarding.import_state != OnboardingImportState::Idle {
            return;
        }
        self.onboarding.import_state = OnboardingImportState::Loading;
        let existing_names = self
            .connection_store
            .connections()
            .iter()
            .map(|connection| connection.name.clone())
            .collect::<HashSet<_>>();
        let aliases = list_ssh_config_hosts(&existing_names)
            .map(|hosts| {
                hosts
                    .into_iter()
                    .filter(|host| host.alias != "*")
                    .map(|host| host.alias)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut imported = 0usize;
        for alias in aliases {
            if self
                .connection_store
                .connections()
                .iter()
                .any(|connection| connection.name == alias)
            {
                continue;
            }
            let Ok(Some(host)) = resolve_ssh_config_alias(&alias) else {
                continue;
            };
            let Ok(connection) = saved_connection_from_ssh_host(host) else {
                continue;
            };
            if self
                .connection_store
                .import_ssh_connection(connection)
                .is_ok()
            {
                imported += 1;
            }
        }
        let _ = self.connection_store.save();
        self.onboarding.imported_count = imported;
        self.onboarding.import_state = OnboardingImportState::Done;
        self.onboarding.host_count = Some(imported);
        cx.notify();
    }

    pub(in crate::workspace) fn onboarding_open_terminal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.complete_onboarding(cx);
        let _ = self.create_local_terminal_tab(window, cx);
    }

    pub(in crate::workspace) fn onboarding_open_new_connection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.complete_onboarding(cx);
        self.open_new_connection_form(window, cx);
    }

    pub(in crate::workspace) fn onboarding_open_cli_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.complete_onboarding(cx);
        self.settings_page.set_active_tab(SettingsTab::General);
        self.open_settings(window, cx);
    }
}
