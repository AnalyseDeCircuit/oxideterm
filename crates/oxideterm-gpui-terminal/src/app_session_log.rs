impl TerminalPane {
    pub fn session_log_available(&self) -> bool {
        self.preferences.session_log_options.is_some()
    }

    pub fn session_log_status(&self) -> crate::session_log::TerminalSessionLogStatus {
        self.session_log
            .as_ref()
            .map(TerminalSessionLog::status)
            .unwrap_or_else(|| crate::session_log::TerminalSessionLogStatus {
                path: self.last_session_log_path.clone(),
                ..Default::default()
            })
    }

    pub fn start_session_log(&mut self, cx: &mut Context<Self>) -> std::result::Result<(), String> {
        let before = self.session_log_status().state;
        let context = self.terminal.lock().audit_context();
        let audit = oxideterm_audit::AuditOperation::in_request(
            context.as_ref(),
            oxideterm_audit::AuditCategory::Automation,
            "session_log_start",
            None,
        );
        let result = (|| {
            if let Some(log) = self.session_log.as_ref() {
                if !log.status().failed {
                    return Ok(());
                }
                // A completed writer error may be replaced without touching the terminal session.
                self.last_session_log_path = log.status().path;
                self.session_log.take();
                self.sync_terminal_output_events_enabled();
            }
            let options = self
                .preferences
                .session_log_options
                .clone()
                .ok_or_else(|| "terminal session log directory is unavailable".to_string())?;
            self.session_log = Some(TerminalSessionLog::start(options).map_err(|_| {
                // File-system details stay out of notices because paths may identify sensitive hosts.
                "could not start terminal session log".to_string()
            })?);
            self.sync_terminal_output_events_enabled();
            cx.emit(TerminalPaneEvent::SessionLogStatusChanged);
            cx.notify();
            Ok(())
        })();
        audit.finish(
            match &result {
                Ok(()) if self.session_log_status().state == before => {
                    oxideterm_audit::AuditOutcome::Unchanged
                }
                Ok(()) => oxideterm_audit::AuditOutcome::Succeeded,
                Err(_) => oxideterm_audit::AuditOutcome::Failed,
            },
            oxideterm_audit::AuditEvidence::Lifecycle,
            None,
            None,
        );
        result
    }

    pub fn pause_session_log(&mut self, cx: &mut Context<Self>) -> std::result::Result<(), String> {
        let before = self.session_log_status().state;
        let context = self.terminal.lock().audit_context();
        let audit = oxideterm_audit::AuditOperation::in_request(
            context.as_ref(),
            oxideterm_audit::AuditCategory::Automation,
            "session_log_pause",
            None,
        );
        let result = (|| {
            let Some(log) = self.session_log.as_mut() else {
                return Ok(());
            };
            log.pause()
                .map_err(|_| "could not pause terminal session log".to_string())?;
            self.sync_terminal_output_events_enabled();
            cx.emit(TerminalPaneEvent::SessionLogStatusChanged);
            cx.notify();
            Ok(())
        })();
        audit.finish(
            match &result {
                Ok(()) if self.session_log_status().state == before => {
                    oxideterm_audit::AuditOutcome::Unchanged
                }
                Ok(()) => oxideterm_audit::AuditOutcome::Succeeded,
                Err(_) => oxideterm_audit::AuditOutcome::Failed,
            },
            oxideterm_audit::AuditEvidence::Lifecycle,
            None,
            None,
        );
        result
    }

    pub fn resume_session_log(&mut self, cx: &mut Context<Self>) {
        if let Some(log) = self.session_log.as_mut() {
            let before = log.status().state;
            log.resume();
            let changed = log.status().state != before;
            let context = self.terminal.lock().audit_context();
            oxideterm_audit::AuditOperation::in_request(
                context.as_ref(),
                oxideterm_audit::AuditCategory::Automation,
                "session_log_resume",
                None,
            )
            .finish(
                if changed {
                    oxideterm_audit::AuditOutcome::Succeeded
                } else {
                    oxideterm_audit::AuditOutcome::Unchanged
                },
                oxideterm_audit::AuditEvidence::Lifecycle,
                None,
                None,
            );
            self.sync_terminal_output_events_enabled();
            cx.emit(TerminalPaneEvent::SessionLogStatusChanged);
            cx.notify();
        }
    }

    pub fn flush_session_log(&self) -> std::result::Result<(), String> {
        let Some(log) = self.session_log.as_ref() else {
            return Ok(());
        };
        log.flush()
            .map_err(|_| "could not flush terminal session log".to_string())
    }

    pub fn stop_session_log(
        &mut self,
        cx: &mut Context<Self>,
    ) -> std::result::Result<Option<std::path::PathBuf>, String> {
        let context = self.terminal.lock().audit_context();
        let audit = oxideterm_audit::AuditOperation::in_request(
            context.as_ref(),
            oxideterm_audit::AuditCategory::Automation,
            "session_log_stop",
            None,
        );
        let result = (|| {
            let Some(log) = self.session_log.take() else {
                return Ok(None);
            };
            self.last_session_log_path = log.status().path;
            self.sync_terminal_output_events_enabled();
            cx.emit(TerminalPaneEvent::SessionLogStatusChanged);
            cx.notify();
            let path = log
                .finish()
                .map_err(|_| "could not finish terminal session log".to_string())?;
            self.last_session_log_path = Some(path.clone());
            Ok(Some(path))
        })();
        audit.finish(
            match &result {
                Ok(Some(_)) => oxideterm_audit::AuditOutcome::Succeeded,
                Ok(None) => oxideterm_audit::AuditOutcome::Unchanged,
                Err(_) => oxideterm_audit::AuditOutcome::Failed,
            },
            oxideterm_audit::AuditEvidence::Lifecycle,
            None,
            None,
        );
        result
    }
}
