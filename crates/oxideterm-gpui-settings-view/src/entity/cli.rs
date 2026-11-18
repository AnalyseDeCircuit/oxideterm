use super::*;

impl SettingsWorkspaceEntity {
    pub fn cli_companion_snapshot(&self) -> CliCompanionSnapshot {
        CliCompanionSnapshot {
            status: self.cli_companion_status.clone(),
            loading: self.cli_companion_loading,
            error: self.cli_companion_error.clone(),
        }
    }

    pub fn cli_companion_error(&self) -> Option<&str> {
        self.cli_companion_error.as_deref()
    }

    pub fn cli_companion_needs_refresh(&self) -> bool {
        self.cli_companion_status.is_none() && !self.cli_companion_loading
    }

    pub fn start_cli_companion_operation(
        &mut self,
        operation: CliCompanionOperation,
        runtime: Arc<tokio::runtime::Runtime>,
        run: fn(CliCompanionOperation) -> Result<CliCompanionStatus, String>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.cli_companion_loading {
            return false;
        }

        self.cli_companion_loading = true;
        self.cli_companion_task = Some(cx.spawn(async move |settings, cx| {
            let result = runtime
                .spawn_blocking(move || run(operation))
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result);
            let _ = settings.update(cx, |settings, cx| {
                settings.finish_cli_companion_operation(operation, result, cx);
            });
        }));
        cx.notify();
        true
    }

    pub fn finish_cli_companion_operation(
        &mut self,
        operation: CliCompanionOperation,
        result: Result<CliCompanionStatus, String>,
        cx: &mut Context<Self>,
    ) {
        self.cli_companion_task = None;
        self.cli_companion_loading = false;
        let success = match result {
            Ok(status) => {
                self.cli_companion_status = Some(status);
                self.cli_companion_error = None;
                true
            }
            Err(error) => {
                self.cli_companion_error = Some(error);
                false
            }
        };
        if operation != CliCompanionOperation::Refresh {
            cx.emit(SettingsWorkspaceEvent::CliCompanionFinished { operation, success });
        }
        cx.notify();
    }
}
