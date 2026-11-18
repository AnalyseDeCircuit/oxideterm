use super::*;
use oxideterm_gpui_settings_view::cli_companion::run_cli_companion_operation;
pub(in crate::workspace) use oxideterm_gpui_settings_view::cli_companion::{
    CLI_COMPANION_COMMAND_NAME, LEGACY_CLI_COMPANION_COMMAND_NAME,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn refresh_cli_companion_status(&mut self, cx: &mut Context<Self>) {
        self.start_cli_companion_operation(CliCompanionOperation::Refresh, cx);
    }

    pub(in crate::workspace) fn install_cli_companion(&mut self, cx: &mut Context<Self>) {
        self.start_cli_companion_operation(CliCompanionOperation::Install, cx);
    }

    pub(in crate::workspace) fn uninstall_legacy_cli_companion(&mut self, cx: &mut Context<Self>) {
        self.start_cli_companion_operation(CliCompanionOperation::UninstallLegacy, cx);
    }

    pub(in crate::workspace) fn migrate_cli_companion(&mut self, cx: &mut Context<Self>) {
        self.start_cli_companion_operation(CliCompanionOperation::Migrate, cx);
    }

    pub(super) fn start_cli_companion_operation(
        &mut self,
        operation: CliCompanionOperation,
        cx: &mut Context<Self>,
    ) {
        let runtime = self.forwarding_runtime.clone();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_cli_companion_operation(
                operation,
                runtime,
                run_cli_companion_operation,
                cx,
            );
        });
    }
}
