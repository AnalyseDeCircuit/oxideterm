use super::*;

#[cfg(windows)]
const FILE_MANAGER_EXTERNAL_BRIDGE_CREATE_NO_WINDOW: u32 = 0x08000000;

pub(in crate::workspace::file_manager) fn open_path_external(path: &str) -> Result<(), String> {
    let audit = oxideterm_audit::AuditOperation::in_context(
        super::super::local_file_audit_context(oxideterm_audit::AuditSource::User).as_ref(),
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
        let mut command = std::process::Command::new("cmd");
        configure_file_manager_external_bridge(&mut command);
        command.args(["/C", "start", "", path]);
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

pub(in crate::workspace::file_manager) fn reveal_path_external(path: &str) -> Result<(), String> {
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
