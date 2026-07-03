use std::path::PathBuf;

pub(crate) fn default_ssh_dir() -> PathBuf {
    if let Ok(Some(ssh_dir)) = oxideterm_portable_runtime::portable_ssh_dir() {
        return ssh_dir;
    }

    local_home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ssh")
}

pub(crate) fn expand_home_path(value: &str) -> String {
    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = local_home_dir()
    {
        return home.join(rest).display().to_string();
    }
    value.to_string()
}

fn local_home_dir() -> Option<PathBuf> {
    // Prefer the platform profile directory so Windows is not steered by a
    // shell-provided HOME value from MSYS, Git Bash, or the packaging host.
    dirs::home_dir().or_else(|| std::env::var_os("HOME").map(PathBuf::from))
}
