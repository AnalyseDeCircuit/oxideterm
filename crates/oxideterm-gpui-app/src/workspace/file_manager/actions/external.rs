use super::*;

pub(in crate::workspace::file_manager) fn open_path_external(path: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", "", path]);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        command
    };

    let status = command
        .status()
        .map_err(|error| format!("failed to launch external app: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("external app exited with status {status}"))
    }
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
