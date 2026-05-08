use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(unix)]
use std::fs;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellInfo {
    pub id: String,
    pub label: String,
    pub path: PathBuf,
    pub args: Vec<String>,
}

impl ShellInfo {
    pub fn new(id: impl Into<String>, label: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            path: path.into(),
            args: Vec::new(),
        }
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }
}

#[derive(Clone, Debug)]
pub struct LocalPtyConfig {
    pub shell: Option<ShellInfo>,
    pub cwd: Option<PathBuf>,
    pub env: HashMap<String, String>,
    pub load_profile: bool,
    pub oh_my_posh_enabled: bool,
    pub oh_my_posh_theme: Option<String>,
}

impl Default for LocalPtyConfig {
    fn default() -> Self {
        Self {
            shell: None,
            cwd: None,
            env: HashMap::new(),
            load_profile: true,
            oh_my_posh_enabled: false,
            oh_my_posh_theme: None,
        }
    }
}

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

pub fn scan_shells() -> Vec<ShellInfo> {
    let mut shells = Vec::new();

    #[cfg(unix)]
    shells.extend(scan_unix_shells());

    #[cfg(target_os = "windows")]
    shells.extend(scan_windows_shells());

    shells.sort_by(|a, b| a.path.cmp(&b.path));
    shells.dedup_by(|a, b| a.path == b.path);
    shells.sort_by(|a, b| a.label.cmp(&b.label));

    if shells.is_empty() {
        shells.push(default_shell());
    }

    shells
}

pub fn default_shell() -> ShellInfo {
    #[cfg(unix)]
    if let Ok(shell_path) = env::var("SHELL") {
        let path = PathBuf::from(&shell_path);
        if path.exists() {
            let id = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("shell")
                .to_string();
            return ShellInfo::new(id.clone(), capitalize_first(&id), path)
                .with_args(default_args_for_shell(&id));
        }
    }

    ShellInfo::new(DEFAULT_SHELL_ID, DEFAULT_SHELL_LABEL, DEFAULT_SHELL_PATH)
        .with_args(default_args_for_shell(DEFAULT_SHELL_ID))
}

pub fn shell_args_for_profile(shell: &ShellInfo, load_profile: bool) -> Vec<String> {
    if shell.id.starts_with("wsl") {
        return shell.args.clone();
    }
    match shell.id.as_str() {
        "zsh" => {
            if load_profile {
                vec!["--login".to_string()]
            } else {
                vec!["--no-rcs".to_string()]
            }
        }
        "bash" | "git-bash" => {
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
                Vec::new()
            }
        }
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
        "cmd" => vec!["/K".to_string(), "chcp 65001 >nul".to_string()],
        _ => shell.args.clone(),
    }
}

fn default_args_for_shell(shell_id: &str) -> Vec<String> {
    match shell_id {
        "zsh" | "bash" | "git-bash" | "fish" | "sh" | "dash" => vec!["--login".to_string()],
        "pwsh" | "powershell" => vec![
            "-NoLogo".to_string(),
            "-NoExit".to_string(),
            "-ExecutionPolicy".to_string(),
            "Bypass".to_string(),
        ],
        "cmd" => vec!["/K".to_string(), "chcp 65001 >nul".to_string()],
        _ => Vec::new(),
    }
}

#[cfg(unix)]
fn scan_unix_shells() -> Vec<ShellInfo> {
    let mut shells = Vec::new();

    if let Ok(content) = fs::read_to_string("/etc/shells") {
        for line in content.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let path = PathBuf::from(line);
            if path.exists()
                && let Some(shell) = shell_info_from_path(&path)
            {
                shells.push(shell);
            }
        }
    }

    const COMMON_SHELLS: &[&str] = &["zsh", "bash", "fish", "sh", "dash", "ksh", "tcsh"];
    for shell_name in COMMON_SHELLS {
        if let Some(path) = command_path(shell_name)
            && !shells.iter().any(|shell| shell.path == path)
            && let Some(shell) = shell_info_from_path(&path)
        {
            shells.push(shell);
        }
    }

    for dir in [
        "/bin",
        "/usr/bin",
        "/usr/local/bin",
        "/opt/homebrew/bin",
        "/opt/local/bin",
    ] {
        for shell_name in COMMON_SHELLS {
            let path = PathBuf::from(dir).join(shell_name);
            if path.exists()
                && !shells.iter().any(|shell| shell.path == path)
                && let Some(shell) = shell_info_from_path(&path)
            {
                shells.push(shell);
            }
        }
    }

    shells
}

#[cfg(unix)]
fn shell_info_from_path(path: &Path) -> Option<ShellInfo> {
    let file_name = path.file_name()?.to_str()?;
    let label = match file_name {
        "zsh" => "Zsh",
        "bash" => "Bash",
        "fish" => "Fish",
        "sh" => "Bourne Shell",
        "dash" => "Dash",
        "ksh" => "Korn Shell",
        "tcsh" => "TENEX C Shell",
        _ => return None,
    };

    Some(
        ShellInfo::new(file_name, label, path.to_path_buf())
            .with_args(default_args_for_shell(file_name)),
    )
}

#[cfg(unix)]
fn command_path(command: &str) -> Option<PathBuf> {
    let output = Command::new("which").arg(command).output().ok()?;
    output.status.success().then(|| {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        PathBuf::from(path)
    })
}

#[cfg(target_os = "windows")]
fn scan_windows_shells() -> Vec<ShellInfo> {
    let mut shells = Vec::new();

    shells.push(ShellInfo::new("cmd", "Command Prompt", "cmd.exe"));

    let system_root = env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let powershell_path =
        PathBuf::from(&system_root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    if powershell_path.exists() {
        shells.push(
            ShellInfo::new("powershell", "Windows PowerShell", powershell_path)
                .with_args(default_args_for_shell("powershell")),
        );
    }

    let program_files =
        env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".to_string());
    let program_files_x86 =
        env::var("ProgramFiles(x86)").unwrap_or_else(|_| r"C:\Program Files (x86)".to_string());

    for path in [
        PathBuf::from(&program_files).join(r"PowerShell\7\pwsh.exe"),
        PathBuf::from(&program_files_x86).join(r"PowerShell\7\pwsh.exe"),
    ] {
        if path.exists() {
            shells.push(
                ShellInfo::new("pwsh", "PowerShell Core", path)
                    .with_args(default_args_for_shell("pwsh")),
            );
            break;
        }
    }

    if !shells.iter().any(|shell| shell.id == "pwsh")
        && let Some(path) = windows_command_path("pwsh.exe")
    {
        shells.push(
            ShellInfo::new("pwsh", "PowerShell Core", path)
                .with_args(default_args_for_shell("pwsh")),
        );
    }

    for path in [
        PathBuf::from(&program_files).join(r"Git\bin\bash.exe"),
        PathBuf::from(&program_files_x86).join(r"Git\bin\bash.exe"),
    ] {
        if path.exists() {
            shells.push(
                ShellInfo::new("git-bash", "Git Bash", path)
                    .with_args(default_args_for_shell("git-bash")),
            );
            break;
        }
    }

    if !shells.iter().any(|shell| shell.id == "git-bash")
        && let Some(path) = windows_command_path("bash.exe")
    {
        shells.push(
            ShellInfo::new("git-bash", "Git Bash", path)
                .with_args(default_args_for_shell("git-bash")),
        );
    }

    scan_wsl_distributions(&mut shells);
    shells
}

#[cfg(target_os = "windows")]
fn windows_command_path(command: &str) -> Option<PathBuf> {
    let output = Command::new("where")
        .arg(command)
        .creation_flags(0x08000000)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    let path = PathBuf::from(path);
    path.exists().then_some(path)
}

#[cfg(target_os = "windows")]
fn scan_wsl_distributions(shells: &mut Vec<ShellInfo>) {
    let wsl_path =
        PathBuf::from(env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string()))
            .join(r"System32\wsl.exe");
    if !wsl_path.exists() {
        return;
    }

    let output = match Command::new(&wsl_path)
        .args(["--list", "--quiet"])
        .creation_flags(0x08000000)
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => {
            shells.push(
                ShellInfo::new("wsl", "WSL (Default)", wsl_path)
                    .with_args(vec!["--cd".to_string(), "~".to_string()]),
            );
            return;
        }
    };

    let stdout = String::from_utf16_lossy(
        &output
            .stdout
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>(),
    );

    let mut found = false;
    for (index, distro) in stdout
        .lines()
        .map(|line| line.replace('\0', "").trim().to_string())
        .filter(|line| !line.is_empty())
        .enumerate()
    {
        found = true;
        let id = format!("wsl-{}", distro.to_lowercase().replace(' ', "-"));
        let label = if index == 0 {
            format!("WSL: {} (Default)", distro)
        } else {
            format!("WSL: {}", distro)
        };
        shells.push(ShellInfo::new(id, label, wsl_path.clone()).with_args(vec![
            "-d".to_string(),
            distro,
            "--cd".to_string(),
            "~".to_string(),
        ]));
    }

    if !found {
        shells.push(
            ShellInfo::new("wsl", "WSL (Default)", wsl_path)
                .with_args(vec!["--cd".to_string(), "~".to_string()]),
        );
    }
}

fn capitalize_first(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
