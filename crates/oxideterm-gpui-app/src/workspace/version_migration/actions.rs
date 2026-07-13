use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn open_version_migration_from_palette(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.version_migration.reset_for_open();
        cx.notify();
    }

    pub(in crate::workspace) fn version_migration_go_to_step(
        &mut self,
        step: usize,
        cx: &mut Context<Self>,
    ) {
        if step >= VERSION_MIGRATION_TOTAL_STEPS {
            return;
        }
        self.version_migration.step = step;
        self.version_migration.scroll_handle = ScrollHandle::new();
        if step == 1
            && self.settings_page.cli_companion_status.is_none()
            && !self.settings_page.cli_companion_loading
        {
            self.refresh_cli_companion_status(cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn version_migration_next(&mut self, cx: &mut Context<Self>) {
        if self.version_migration.step + 1 < VERSION_MIGRATION_TOTAL_STEPS {
            self.version_migration_go_to_step(self.version_migration.step + 1, cx);
        } else {
            self.complete_version_migration_notice(cx);
        }
    }

    pub(in crate::workspace) fn version_migration_back(&mut self, cx: &mut Context<Self>) {
        if self.version_migration.step > 0 {
            self.version_migration_go_to_step(self.version_migration.step - 1, cx);
        }
    }

    pub(in crate::workspace) fn complete_version_migration_notice(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        match crate::migration_snapshot::acknowledge_pre_2_0_migration_notice(
            self.settings_store.path(),
        ) {
            Ok(()) => {
                self.version_migration.open = false;
                self.version_migration.error = None;
            }
            Err(error) => {
                // Keep the page open when acknowledgement persistence fails so
                // the notice is not silently lost on the next application start.
                self.version_migration.error = Some(error.to_string());
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn handle_version_migration_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.version_migration.open {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.complete_version_migration_notice(cx),
            "enter" | "arrowright" => self.version_migration_next(cx),
            "arrowleft" => self.version_migration_back(cx),
            _ => return false,
        }
        true
    }
}
