//! Shell detection and scanning
//!
//! Automatically detects available shells on the system and provides
//! preferences management for default shell selection.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[cfg(unix)]
use std::fs;

/// Information about a detected shell
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShellInfo {
    /// Unique identifier (e.g., "zsh", "bash", "powershell")
    pub id: String,
    /// Human-readable label (e.g., "Zsh", "Bash", "PowerShell")
    pub label: String,
    /// Full path to the shell executable
    pub path: PathBuf,
    /// Default arguments (e.g., ["--login"] for login shell)
    pub args: Vec<String>,
}

impl ShellInfo {
    pub fn new(id: impl Into<String>, label: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            path: path.into(),
            args: vec![],
        }
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }
}

// Platform-specific default shells
#[cfg(target_os = "macos")]
const DEFAULT_SHELL_PATH: &str = "/bin/zsh";
#[cfg(target_os = "macos")]
const DEFAULT_SHELL_ID: &str = "zsh";
#[cfg(target_os = "macos")]
const DEFAULT_SHELL_LABEL: &str = "Zsh";

#[cfg(target_os = "linux")]
const DEFAULT_SHELL_PATH: &str = "/bin/bash";
#[cfg(target_os = "linux")]
const DEFAULT_SHELL_ID: &str = "bash";
#[cfg(target_os = "linux")]
const DEFAULT_SHELL_LABEL: &str = "Bash";

#[cfg(target_os = "windows")]
const DEFAULT_SHELL_PATH: &str = "cmd.exe";
#[cfg(target_os = "windows")]
const DEFAULT_SHELL_ID: &str = "cmd";
#[cfg(target_os = "windows")]
const DEFAULT_SHELL_LABEL: &str = "Command Prompt";

/// Scan the system for available shells
pub fn scan_shells() -> Vec<ShellInfo> {
    let mut shells = Vec::new();

    #[cfg(unix)]
    {
        shells.extend(scan_unix_shells());
    }

    #[cfg(target_os = "windows")]
    {
        shells.extend(scan_windows_shells());
    }

    // Deduplicate by path
    shells.sort_by(|a, b| a.path.cmp(&b.path));
    shells.dedup_by(|a, b| a.path == b.path);

    // Sort by label for consistent display
    shells.sort_by(|a, b| a.label.cmp(&b.label));

    shells
}

/// Get the default shell for the current platform
pub fn default_shell() -> ShellInfo {
    // First, try to use $SHELL environment variable (Unix)
    #[cfg(unix)]
    if let Ok(shell_path) = std::env::var("SHELL") {
        let path = PathBuf::from(&shell_path);
        if path.exists() {
            let id = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("shell")
                .to_string();
            let label = capitalize_first(&id);
            return ShellInfo::new(id, label, path).with_args(vec!["--login".to_string()]);
        }
    }

    // Fallback to platform default
    ShellInfo::new(DEFAULT_SHELL_ID, DEFAULT_SHELL_LABEL, DEFAULT_SHELL_PATH)
        .with_args(default_args_for_shell(DEFAULT_SHELL_ID))
}

/// Get default arguments for a given shell
fn default_args_for_shell(shell_id: &str) -> Vec<String> {
    match shell_id {
        "zsh" | "bash" | "fish" | "sh" => vec!["--login".to_string()],
        // PowerShell: -NoExit keeps interactive mode, -ExecutionPolicy Bypass allows profile scripts
        "pwsh" | "powershell" => vec![
            "-NoLogo".to_string(),
            "-NoExit".to_string(),
            "-ExecutionPolicy".to_string(),
            "Bypass".to_string(),
        ],
        _ => vec![],
    }
}

/// Get shell arguments with optional profile loading control
/// 
/// This function generates appropriate shell arguments based on:
/// - The shell type (bash, zsh, powershell, etc.)
/// - Whether to load the user's profile/startup files
/// 
/// # Arguments
/// * `shell_id` - The shell identifier (e.g., "zsh", "pwsh", "git-bash")
/// * `load_profile` - Whether to load shell startup files (.bashrc, .zshrc, Profile.ps1)
/// 
/// # Returns
/// A vector of command-line arguments for the shell
pub fn get_shell_args(shell_id: &str, load_profile: bool) -> Vec<String> {
    match shell_id {
        // Unix shells: --login loads profile, --norc/--noprofile skips
        "zsh" => {
            if load_profile {
                vec!["--login".to_string()]
            } else {
                vec!["--no-rcs".to_string()]
            }
        }
        "bash" => {
            if load_profile {
                vec!["--login".to_string()]
            } else {
                vec!["--noprofile".to_string(), "--norc".to_string()]
            }
        }
        "fish" => {
            if load_profile {
                vec!["--login".to_string()]
            } else {
                vec!["--no-config".to_string()]
            }
        }
        "sh" | "dash" => {
            if load_profile {
                vec!["--login".to_string()]
            } else {
                vec![]
            }
        }
        // PowerShell: -NoProfile skips profile scripts
        "pwsh" | "powershell" => {
            let mut args = vec![
                "-NoLogo".to_string(),
                "-NoExit".to_string(),
                "-ExecutionPolicy".to_string(),
                "Bypass".to_string(),
            ];
            if !load_profile {
                args.push("-NoProfile".to_string());
            }
            args
        }
        // Git Bash (MSYS2 bash)
        "git-bash" => {
            if load_profile {
                vec!["--login".to_string()]
            } else {
                vec!["--noprofile".to_string(), "--norc".to_string()]
            }
        }
        // WSL: profile handling is done inside the distribution
        id if id.starts_with("wsl") => {
            // WSL passes args to wsl.exe, not to the shell inside
            // Profile loading is controlled by the shell config in WSL
            vec![]
        }
        // cmd.exe: no profile concept
        "cmd" => vec![],
        _ => vec![],
    }
}

/// Capitalize the first letter of a string
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

// ============================================================================
// Unix shell scanning
// ============================================================================

#[cfg(unix)]
fn scan_unix_shells() -> Vec<ShellInfo> {
    let mut shells = Vec::new();

    // 1. Read /etc/shells
    if let Ok(content) = fs::read_to_string("/etc/shells") {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let path = PathBuf::from(line);
            if path.exists() {
                if let Some(shell) = shell_info_from_path(&path) {
                    shells.push(shell);
                }
            }
        }
    }

    // 2. Check common shell locations via `which`
    let common_shells = ["zsh", "bash", "fish", "sh", "dash", "ksh", "tcsh"];
    for shell_name in common_shells {
        if let Ok(output) = std::process::Command::new("which")
            .arg(shell_name)
            .output()
        {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let path = PathBuf::from(&path_str);
                if path.exists() && !shells.iter().any(|s| s.path == path) {
                    if let Some(shell) = shell_info_from_path(&path) {
                        shells.push(shell);
                    }
                }
            }
        }
    }

    shells
}

#[cfg(unix)]
fn shell_info_from_path(path: &PathBuf) -> Option<ShellInfo> {
    let file_name = path.file_name()?.to_str()?;
    let id = file_name.to_string();
    let label = match file_name {
        "zsh" => "Zsh",
        "bash" => "Bash",
        "fish" => "Fish",
        "sh" => "Bourne Shell",
        "dash" => "Dash",
        "ksh" => "Korn Shell",
        "tcsh" => "TENEX C Shell",
        _ => return None, // Skip unknown shells
    };

    Some(
        ShellInfo::new(&id, label, path.clone())
            .with_args(default_args_for_shell(&id)),
    )
}

// ============================================================================
// Windows shell scanning
// ============================================================================

#[cfg(target_os = "windows")]
fn scan_windows_shells() -> Vec<ShellInfo> {
    let mut shells = Vec::new();

    // 1. Command Prompt (always available)
    shells.push(ShellInfo::new("cmd", "Command Prompt", "cmd.exe"));

    // 2. PowerShell (Windows PowerShell, always available on modern Windows)
    let powershell_path = PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
    if powershell_path.exists() {
        shells.push(
            ShellInfo::new("powershell", "Windows PowerShell", powershell_path)
                .with_args(vec![
                    "-NoLogo".to_string(),
                    "-NoExit".to_string(),
                    "-ExecutionPolicy".to_string(),
                    "Bypass".to_string(),
                ]),
        );
    }

    // 3. PowerShell Core (pwsh.exe, if installed)
    // Check common installation paths
    let pwsh_paths = [
        r"C:\Program Files\PowerShell\7\pwsh.exe",
        r"C:\Program Files (x86)\PowerShell\7\pwsh.exe",
    ];
    for path_str in pwsh_paths {
        let path = PathBuf::from(path_str);
        if path.exists() {
            shells.push(
                ShellInfo::new("pwsh", "PowerShell Core", path)
                    .with_args(vec![
                        "-NoLogo".to_string(),
                        "-NoExit".to_string(),
                        "-ExecutionPolicy".to_string(),
                        "Bypass".to_string(),
                    ]),
            );
            break;
        }
    }

    // Also check if pwsh is in PATH
    if let Ok(output) = std::process::Command::new("where").arg("pwsh.exe").output() {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            let path = PathBuf::from(&path_str);
            if path.exists() && !shells.iter().any(|s| s.id == "pwsh") {
                shells.push(
                    ShellInfo::new("pwsh", "PowerShell Core", path)
                        .with_args(vec![
                            "-NoLogo".to_string(),
                            "-NoExit".to_string(),
                            "-ExecutionPolicy".to_string(),
                            "Bypass".to_string(),
                        ]),
                );
            }
        }
    }

    // 4. Git Bash (if installed)
    let git_bash_paths = [
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\Program Files (x86)\Git\bin\bash.exe",
    ];
    for path_str in git_bash_paths {
        let path = PathBuf::from(path_str);
        if path.exists() {
            shells.push(
                ShellInfo::new("git-bash", "Git Bash", path)
                    .with_args(vec!["--login".to_string()]),
            );
            break;
        }
    }

    // 5. WSL2 - Scan for installed distributions
    scan_wsl_distributions(&mut shells);

    shells
}

/// Scan for installed WSL distributions and add them as shell options
#[cfg(target_os = "windows")]
fn scan_wsl_distributions(shells: &mut Vec<ShellInfo>) {
    let wsl_path = PathBuf::from(r"C:\Windows\System32\wsl.exe");
    if !wsl_path.exists() {
        return;
    }

    // Try to get list of installed distributions
    let output = match std::process::Command::new(&wsl_path)
        .args(["--list", "--quiet"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => {
            // Fallback: add generic WSL entry if we can't enumerate
            shells.push(
                ShellInfo::new("wsl", "WSL (Default)", wsl_path.clone())
                    .with_args(vec!["--cd".to_string(), "~".to_string()]),
            );
            return;
        }
    };

    // Parse distribution list (UTF-16 LE output on Windows)
    let stdout = String::from_utf16_lossy(
        &output
            .stdout
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>(),
    );

    let distros: Vec<&str> = stdout
        .lines()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if distros.is_empty() {
        // No distributions installed, add generic WSL entry
        shells.push(
            ShellInfo::new("wsl", "WSL (Default)", wsl_path)
                .with_args(vec!["--cd".to_string(), "~".to_string()]),
        );
        return;
    }

    // Add each distribution as a separate shell option
    for (i, distro) in distros.iter().enumerate() {
        // Clean up distro name (remove any null bytes or special chars)
        let distro_clean = distro.replace('\0', "").trim().to_string();
        if distro_clean.is_empty() {
            continue;
        }

        let id = format!("wsl-{}", distro_clean.to_lowercase().replace(' ', "-"));
        let label = if i == 0 {
            format!("WSL: {} (Default)", distro_clean)
        } else {
            format!("WSL: {}", distro_clean)
        };

        shells.push(
            ShellInfo::new(id, label, wsl_path.clone())
                .with_args(vec![
                    "-d".to_string(),
                    distro_clean,
                    "--cd".to_string(),
                    "~".to_string(),
                ]),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_shell_exists() {
        let shell = default_shell();
        // On CI, the default shell might not exist, so just check the structure
        assert!(!shell.id.is_empty());
        assert!(!shell.label.is_empty());
    }

    #[test]
    fn test_scan_shells_returns_results() {
        let shells = scan_shells();
        // Should find at least one shell on any system
        assert!(!shells.is_empty(), "No shells found on system");
    }

    #[test]
    fn test_capitalize_first() {
        assert_eq!(capitalize_first("zsh"), "Zsh");
        assert_eq!(capitalize_first("bash"), "Bash");
        assert_eq!(capitalize_first(""), "");
    }

    #[test]
    fn test_get_shell_args_zsh() {
        let args_with_profile = get_shell_args("zsh", true);
        assert!(args_with_profile.contains(&"--login".to_string()));

        let args_without_profile = get_shell_args("zsh", false);
        assert!(args_without_profile.contains(&"--no-rcs".to_string()));
        assert!(!args_without_profile.contains(&"--login".to_string()));
    }

    #[test]
    fn test_get_shell_args_bash() {
        let args_with_profile = get_shell_args("bash", true);
        assert!(args_with_profile.contains(&"--login".to_string()));

        let args_without_profile = get_shell_args("bash", false);
        assert!(args_without_profile.contains(&"--noprofile".to_string()));
        assert!(args_without_profile.contains(&"--norc".to_string()));
    }

    #[test]
    fn test_get_shell_args_powershell() {
        let args_with_profile = get_shell_args("pwsh", true);
        assert!(args_with_profile.contains(&"-NoLogo".to_string()));
        assert!(args_with_profile.contains(&"-NoExit".to_string()));
        assert!(!args_with_profile.contains(&"-NoProfile".to_string()));

        let args_without_profile = get_shell_args("pwsh", false);
        assert!(args_without_profile.contains(&"-NoProfile".to_string()));
    }

    #[test]
    fn test_get_shell_args_fish() {
        let args_with_profile = get_shell_args("fish", true);
        assert!(args_with_profile.contains(&"--login".to_string()));

        let args_without_profile = get_shell_args("fish", false);
        assert!(args_without_profile.contains(&"--no-config".to_string()));
    }
}
