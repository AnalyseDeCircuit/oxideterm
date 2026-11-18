pub fn local_file_audit_context(
    source: oxideterm_audit::AuditSource,
) -> Option<oxideterm_audit::AuditContext> {
    let request = oxideterm_audit::AuditContext::current_request();
    let is_scoped = request.is_some();
    let mut context = request.or_else(oxideterm_audit::AuditContext::current)?;
    if !is_scoped {
        context.source = source;
    }
    context.target = Some(oxideterm_audit::redact("local"));
    context.protocol = Some("local".into());
    context.session_id = None;
    context.consumer_id = None;
    context.transport_id = None;
    context.node_id = None;
    context.connection_id = None;
    Some(context)
}

#[cfg(windows)]
const FILE_MANAGER_EXTERNAL_BRIDGE_CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn open_path_external(path: &str) -> Result<(), String> {
    let audit = oxideterm_audit::AuditOperation::in_context(
        local_file_audit_context(oxideterm_audit::AuditSource::User).as_ref(),
        oxideterm_audit::AuditCategory::File,
        "file_external_open",
        Some(path),
    );
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("explorer.exe");
        configure_file_manager_external_bridge(&mut command);
        command.arg(std::path::Path::new(path));
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        command
    };

    let result = match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("external app exited with status {status}")),
        Err(error) => Err(format!("failed to launch external app: {error}")),
    };
    audit.finish(
        if result.is_ok() {
            oxideterm_audit::AuditOutcome::Sent
        } else {
            oxideterm_audit::AuditOutcome::Failed
        },
        oxideterm_audit::AuditEvidence::Dispatch,
        None,
        None,
    );
    result
}

pub fn reveal_path_external(path: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.args(["-R", path]);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("explorer");
        configure_file_manager_external_bridge(&mut command);
        command.arg(format!("/select,{path}"));
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let parent = std::path::Path::new(path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new(path));
        let mut command = std::process::Command::new("xdg-open");
        command.arg(parent);
        command
    };

    let status = command
        .status()
        .map_err(|error| format!("failed to reveal file: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("reveal exited with status {status}"))
    }
}

#[cfg(target_os = "windows")]
fn configure_file_manager_external_bridge(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;

    // Keep Windows launcher bridges hidden while they hand off to Explorer or
    // the user's associated application.
    command.creation_flags(FILE_MANAGER_EXTERNAL_BRIDGE_CREATE_NO_WINDOW);
}

pub fn local_file_operation(action: &str, detail: &str) -> oxideterm_audit::AuditOperation {
    oxideterm_audit::AuditOperation::in_context(
        local_file_audit_context(oxideterm_audit::AuditSource::User).as_ref(),
        oxideterm_audit::AuditCategory::File,
        action,
        Some(detail),
    )
}
