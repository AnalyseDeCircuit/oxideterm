use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result};
use tempfile::TempDir;

use crate::{
    EMACS_FREE_TYPE_INTEGRATION_SOURCE, LocalPtyConfig, ShellInfo,
    VIM_FREE_TYPE_INTEGRATION_SOURCE, local_shell::shell_args_for_profile,
};

const VIM_FREE_TYPE_INTEGRATION_ENV: &str = "OXIDETERM_VIM_INTEGRATION";
const EMACS_FREE_TYPE_INTEGRATION_ENV: &str = "OXIDETERM_EMACS_INTEGRATION";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalCwdIntegrationLaunchState {
    NotRequested,
    Prepared,
    Unavailable,
}

pub(crate) struct LocalShellLaunch {
    pub(crate) args: Vec<String>,
    pub(crate) env: HashMap<String, String>,
    pub(crate) integration: Option<LocalShellIntegration>,
    pub(crate) integration_state: TerminalCwdIntegrationLaunchState,
}

pub(crate) struct LocalShellIntegration {
    // The temporary startup files belong to the PTY session and remain valid
    // for shells which lazily reload their configuration after startup.
    _directories: Vec<TempDir>,
}

impl LocalShellIntegration {
    fn merge(mut self, other: Self) -> Self {
        self._directories.extend(other._directories);
        self
    }
}

pub(crate) fn prepare_local_shell_launch(
    config: &LocalPtyConfig,
    shell: &ShellInfo,
    mut env: HashMap<String, String>,
    default_args: Vec<String>,
) -> LocalShellLaunch {
    // Editor adapters are passive files. Exposing their paths for every PTY
    // lets a user toggle Free Type Mode without restarting the shell, while
    // still requiring an explicit opt-in from the editor configuration.
    let editor_integration = match prepare_editor_integration(&mut env) {
        Ok(integration) => Some(integration),
        Err(error) => {
            tracing::warn!(%error, "failed to prepare local terminal editor integration");
            None
        }
    };

    if !config.current_directory_shell_integration {
        return LocalShellLaunch {
            args: default_args,
            env,
            integration: editor_integration,
            integration_state: TerminalCwdIntegrationLaunchState::NotRequested,
        };
    }

    let prepared = prepare_known_shell(config, shell, &mut env, &default_args);
    let (args, shell_integration, integration_state) = match prepared {
        Ok(Some((args, integration))) => (
            args,
            Some(integration),
            TerminalCwdIntegrationLaunchState::Prepared,
        ),
        Ok(None) => (
            default_args,
            None,
            TerminalCwdIntegrationLaunchState::Unavailable,
        ),
        Err(error) => {
            tracing::warn!(
                shell = %shell.path.display(),
                %error,
                "failed to prepare local current-directory shell integration"
            );
            (
                default_args,
                None,
                TerminalCwdIntegrationLaunchState::Unavailable,
            )
        }
    };
    LocalShellLaunch {
        args,
        env,
        integration: match (shell_integration, editor_integration) {
            (Some(shell), Some(editor)) => Some(shell.merge(editor)),
            (Some(shell), None) => Some(shell),
            (None, Some(editor)) => Some(editor),
            (None, None) => None,
        },
        integration_state,
    }
}

fn prepare_editor_integration(env: &mut HashMap<String, String>) -> Result<LocalShellIntegration> {
    let directory = tempfile::Builder::new()
        .prefix("oxideterm-editor-")
        .tempdir()
        .context("create temporary editor integration directory")?;
    let vim_path = directory.path().join("oxideterm-free-type.vim");
    let emacs_path = directory.path().join("oxideterm-free-type.el");
    write_private_file(&vim_path, VIM_FREE_TYPE_INTEGRATION_SOURCE)?;
    write_private_file(&emacs_path, EMACS_FREE_TYPE_INTEGRATION_SOURCE)?;
    env.insert(
        VIM_FREE_TYPE_INTEGRATION_ENV.to_string(),
        vim_path.display().to_string(),
    );
    env.insert(
        EMACS_FREE_TYPE_INTEGRATION_ENV.to_string(),
        emacs_path.display().to_string(),
    );
    Ok(LocalShellIntegration {
        _directories: vec![directory],
    })
}

fn prepare_known_shell(
    config: &LocalPtyConfig,
    shell: &ShellInfo,
    env: &mut HashMap<String, String>,
    default_args: &[String],
) -> Result<Option<(Vec<String>, LocalShellIntegration)>> {
    if shell.id.starts_with("wsl") || matches!(shell.id.as_str(), "git-bash" | "cmd") {
        return Ok(None);
    }

    let directory = tempfile::Builder::new()
        .prefix("oxideterm-shell-")
        .tempdir()
        .context("create temporary shell integration directory")?;
    let args = match shell.id.as_str() {
        "bash" => prepare_bash(config, directory.path())?,
        "zsh" => prepare_zsh(config, directory.path(), env)?,
        "fish" => prepare_fish(config, directory.path())?,
        "nu" | "nu.exe" => prepare_nushell(config, directory.path())?,
        "pwsh" | "powershell" => prepare_powershell(directory.path(), default_args)?,
        _ => return Ok(None),
    };

    Ok(Some((
        args,
        LocalShellIntegration {
            _directories: vec![directory],
        },
    )))
}

fn prepare_bash(config: &LocalPtyConfig, directory: &Path) -> Result<Vec<String>> {
    let init_path = directory.join("bashrc");
    let profile_loader = if config.load_profile {
        r#"if [ -r /etc/profile ]; then . /etc/profile; fi
if [ -r "$HOME/.bash_profile" ]; then . "$HOME/.bash_profile"
elif [ -r "$HOME/.bash_login" ]; then . "$HOME/.bash_login"
elif [ -r "$HOME/.profile" ]; then . "$HOME/.profile"
elif [ -r "$HOME/.bashrc" ]; then . "$HOME/.bashrc"
fi"#
    } else {
        ""
    };
    write_private_file(
        &init_path,
        &format!("{profile_loader}\n{}", posix_prompt_hook()),
    )?;
    Ok(vec![
        "--noprofile".to_string(),
        "--rcfile".to_string(),
        init_path.display().to_string(),
        "-i".to_string(),
    ])
}

fn prepare_zsh(
    config: &LocalPtyConfig,
    directory: &Path,
    env: &mut HashMap<String, String>,
) -> Result<Vec<String>> {
    let original_zdotdir = env
        .get("ZDOTDIR")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("ZDOTDIR").map(std::path::PathBuf::from))
        .or_else(|| env.get("HOME").map(std::path::PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(std::path::PathBuf::from));
    let original = original_zdotdir.as_deref();
    let zshenv = if config.load_profile {
        let user_zdotdir = original
            .map(|path| posix_quote(&path.display().to_string()))
            .unwrap_or_else(|| "\"$HOME\"".to_string());
        format!(
            "typeset -g __oxideterm_user_zdotdir={}\ntypeset -g __oxideterm_integration_zdotdir={}\n{}\nsetopt RCS",
            user_zdotdir,
            posix_quote(&directory.display().to_string()),
            zsh_source_user_file(".zshenv", directory, false),
        )
    } else {
        format!(
            "unsetopt GLOBAL_RCS\nsetopt RCS\nexport ZDOTDIR={}",
            posix_quote(&directory.display().to_string())
        )
    };
    write_private_file(&directory.join(".zshenv"), &zshenv)?;
    write_private_file(
        &directory.join(".zshrc"),
        &format!(
            "{}\n{}",
            config
                .load_profile
                .then(|| zsh_source_user_file(".zshrc", directory, true))
                .unwrap_or_default(),
            zsh_prompt_hook(config.load_profile)
        ),
    )?;
    env.insert("ZDOTDIR".to_string(), directory.display().to_string());
    // The PTY provides interactive mode; the temporary ZDOTDIR controls whether
    // user startup files are loaded without turning the session into a login shell.
    Ok(Vec::new())
}

fn prepare_fish(config: &LocalPtyConfig, directory: &Path) -> Result<Vec<String>> {
    let init_path = directory.join("fish.fish");
    write_private_file(&init_path, &fish_prompt_hook())?;
    let mut args =
        shell_args_for_profile(&ShellInfo::new("fish", "Fish", "fish"), config.load_profile);
    args.push("--init-command".to_string());
    args.push(format!(
        "source {}",
        fish_quote(&init_path.display().to_string())
    ));
    Ok(args)
}

fn prepare_nushell(config: &LocalPtyConfig, directory: &Path) -> Result<Vec<String>> {
    let config_path = directory.join("config.nu");
    let user_config = config
        .load_profile
        .then(nushell_user_config_path)
        .flatten()
        .filter(|path| path != &config_path && path.is_file());
    let source = user_config
        .as_deref()
        .map(|path| format!("source {}\n", nu_quote(&path.display().to_string())))
        .unwrap_or_default();
    write_private_file(&config_path, &format!("{source}{}", nushell_prompt_hook()))?;
    Ok(vec![
        "--config".to_string(),
        config_path.display().to_string(),
    ])
}

fn prepare_powershell(directory: &Path, default_args: &[String]) -> Result<Vec<String>> {
    let init_path = directory.join("profile.ps1");
    write_private_file(&init_path, &powershell_prompt_hook())?;
    let mut args = default_args.to_vec();
    let source = format!(
        ". '{}';",
        init_path.display().to_string().replace('\'', "''")
    );
    if let Some(command_index) = args.iter().position(|arg| arg == "-Command") {
        if let Some(command) = args.get_mut(command_index + 1) {
            command.push(';');
            command.push_str(&source);
        } else {
            args.push(source);
        }
    } else {
        args.push("-Command".to_string());
        args.push(source);
    }
    Ok(args)
}

fn write_private_file(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).with_context(|| format!("write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("restrict permissions for {}", path.display()))?;
    }
    Ok(())
}

fn zsh_source_user_file(name: &str, integration_directory: &Path, final_user_file: bool) -> String {
    // Global startup files may derive HISTFILE from the temporary ZDOTDIR before user files run.
    let source = format!(
        "export ZDOTDIR=\"$__oxideterm_user_zdotdir\"\nif [[ -n \"${{HISTFILE:-}}\" && \"$HISTFILE\" == \"$__oxideterm_integration_zdotdir/\"* ]]; then\n    HISTFILE=\"$ZDOTDIR/${{HISTFILE#\"$__oxideterm_integration_zdotdir\"/}}\"\nfi\nif [[ -r \"$ZDOTDIR/{name}\" ]]; then source \"$ZDOTDIR/{name}\"; fi\n__oxideterm_user_zdotdir=\"${{ZDOTDIR:-$HOME}}\""
    );
    if final_user_file {
        // Restore the user's effective ZDOTDIR for the interactive session.
        format!(
            "{source}\nexport ZDOTDIR=\"$__oxideterm_user_zdotdir\"\nunset __oxideterm_user_zdotdir __oxideterm_integration_zdotdir"
        )
    } else {
        // Route the next startup stage back through the temporary integration directory.
        format!(
            "{source}\nexport ZDOTDIR={}",
            posix_quote(&integration_directory.display().to_string())
        )
    }
}

fn posix_prompt_hook() -> String {
    format!(
        r#"__oxideterm_pct_path() {{
    command printf '%s' "$1" | command od -An -tx1 -v | command tr -d ' \n' | command sed 's/../%&/g; s|%2f|/|g'
}}
__oxideterm_emit_cwd() {{
    __oxideterm_cwd=$(pwd -P 2>/dev/null || pwd 2>/dev/null) || return
    command printf '\033]7;file://%s\007' "$(__oxideterm_pct_path "$__oxideterm_cwd")"
}}
PROMPT_COMMAND="__oxideterm_emit_cwd${{PROMPT_COMMAND:+;$PROMPT_COMMAND}}"
__oxideterm_emit_cwd"#
    )
}

fn zsh_prompt_hook(import_history: bool) -> String {
    let history_hook = if import_history {
        r#"# The real .zshrc is nested, so import history after it has selected HISTFILE.
if (( ${#history[@]} == 0 )) && [[ -n "$HISTFILE" && -r "$HISTFILE" ]]; then
    builtin fc -R "$HISTFILE"
fi"#
    } else {
        ""
    };
    format!(
        r#"{history_hook}
function __oxideterm_pct_path() {{
    command printf '%s' "$1" | command od -An -tx1 -v | command tr -d ' \n' | command sed 's/../%&/g; s|%2f|/|g'
}}
function __oxideterm_emit_cwd() {{
    local __oxideterm_cwd="${{PWD:A}}"
    command printf '\033]7;file://%s\007' "$(__oxideterm_pct_path "$__oxideterm_cwd")"
}}
autoload -Uz add-zsh-hook
autoload +X add-zsh-hook
add-zsh-hook precmd __oxideterm_emit_cwd
__oxideterm_emit_cwd"#
    )
}

fn fish_prompt_hook() -> String {
    r#"function __oxideterm_pct_path
    command printf '%s' "$argv[1]" | command od -An -tx1 -v | command tr -d ' \n' | command sed 's/../%&/g; s|%2f|/|g'
end
function __oxideterm_emit_cwd --on-event fish_prompt
    set -l __oxideterm_cwd (pwd -P 2>/dev/null; or pwd 2>/dev/null)
    command printf '\033]7;file://%s\007' (__oxideterm_pct_path "$__oxideterm_cwd")
end
__oxideterm_emit_cwd"#
        .to_string()
}

fn nushell_prompt_hook() -> String {
    r#"def __oxideterm_pct_path [value: string] {
    ^printf '%s' $value | ^od -An -tx1 -v | ^tr -d ' \n' | ^sed 's/../%&/g; s|%2f|/|g'
}
def __oxideterm_emit_cwd [] {
    print --no-newline $"\u{1b}]7;file://(__oxideterm_pct_path (pwd))\u{07}"
}
$env.config = ($env.config | upsert hooks.pre_prompt (($env.config.hooks.pre_prompt? | default []) | append {|| __oxideterm_emit_cwd }))
__oxideterm_emit_cwd"#
        .to_string()
}

fn powershell_prompt_hook() -> String {
    r#"$script:__oxideterm_original_prompt = $null
if (Test-Path Function:\prompt) {
    $script:__oxideterm_original_prompt = (Get-Command prompt).ScriptBlock
}
function global:__oxideterm_pct_path {
    param([string]$Value)
    [Uri]::EscapeDataString($Value)
}
function global:__oxideterm_emit_cwd {
    $location = Get-Location
    $cwd = if ($location.ProviderPath) { $location.ProviderPath } else { $location.Path }
    [Console]::Out.Write("$([char]27)]7;$(__oxideterm_pct_path $cwd)$([char]7)")
}
function global:prompt {
    __oxideterm_emit_cwd
    if ($script:__oxideterm_original_prompt) {
        & $script:__oxideterm_original_prompt
    } else {
        "PS $($executionContext.SessionState.Path.CurrentLocation)$('>' * ($nestedPromptLevel + 1)) "
    }
}
__oxideterm_emit_cwd"#
        .to_string()
}

fn nushell_user_config_path() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    if let Some(app_data) = std::env::var_os("APPDATA") {
        return Some(std::path::PathBuf::from(app_data).join("nushell/config.nu"));
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return Some(
            std::path::PathBuf::from(home).join("Library/Application Support/nushell/config.nu"),
        );
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
        })
        .map(|base| base.join("nushell/config.nu"))
}

fn posix_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn fish_quote(value: &str) -> String {
    posix_quote(value)
}

fn nu_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_for(shell_id: &str) -> (LocalPtyConfig, ShellInfo) {
        let config = LocalPtyConfig {
            current_directory_shell_integration: true,
            ..LocalPtyConfig::default()
        };
        let shell = ShellInfo::new(shell_id, shell_id, shell_id);
        (config, shell)
    }

    #[test]
    fn known_shells_prepare_owned_startup_integration() {
        for shell_id in ["bash", "zsh", "fish", "nu", "pwsh"] {
            let (config, shell) = config_for(shell_id);
            let args = shell_args_for_profile(&shell, config.load_profile);
            let launch = prepare_local_shell_launch(&config, &shell, HashMap::new(), args);
            assert_eq!(
                launch.integration_state,
                TerminalCwdIntegrationLaunchState::Prepared,
                "{shell_id}"
            );
            assert!(launch.integration.is_some(), "{shell_id}");
            assert!(
                launch
                    .args
                    .iter()
                    .any(|arg| arg.contains("oxideterm-shell-"))
                    || matches!(shell_id, "zsh"),
                "{shell_id}"
            );
        }
    }

    #[test]
    fn unsupported_shell_uses_normal_arguments_and_fallback_state() {
        let (config, shell) = config_for("xonsh");
        let launch =
            prepare_local_shell_launch(&config, &shell, HashMap::new(), shell.args.clone());

        assert_eq!(
            launch.integration_state,
            TerminalCwdIntegrationLaunchState::Unavailable
        );
        assert!(launch.integration.is_some());
        assert_eq!(launch.args, shell.args);
    }

    #[test]
    fn disabled_cwd_integration_still_exposes_owned_editor_adapters() {
        let shell = ShellInfo::new("bash", "Bash", "bash");
        let launch = prepare_local_shell_launch(
            &LocalPtyConfig::default(),
            &shell,
            HashMap::new(),
            shell_args_for_profile(&shell, true),
        );

        assert_eq!(
            launch.integration_state,
            TerminalCwdIntegrationLaunchState::NotRequested
        );
        assert!(launch.integration.is_some());
        let vim_path = std::path::PathBuf::from(
            launch
                .env
                .get(VIM_FREE_TYPE_INTEGRATION_ENV)
                .expect("Vim adapter path"),
        );
        let emacs_path = std::path::PathBuf::from(
            launch
                .env
                .get(EMACS_FREE_TYPE_INTEGRATION_ENV)
                .expect("Emacs adapter path"),
        );
        assert_eq!(
            fs::read_to_string(&vim_path).expect("read Vim adapter"),
            VIM_FREE_TYPE_INTEGRATION_SOURCE
        );
        assert_eq!(
            fs::read_to_string(&emacs_path).expect("read Emacs adapter"),
            EMACS_FREE_TYPE_INTEGRATION_SOURCE
        );

        drop(launch);

        assert!(!vim_path.exists());
        assert!(!emacs_path.exists());
    }

    #[test]
    fn temporary_startup_files_are_removed_with_launch_owner() {
        let (config, shell) = config_for("bash");
        let args = shell_args_for_profile(&shell, config.load_profile);
        let launch = prepare_local_shell_launch(&config, &shell, HashMap::new(), args);
        let startup_file = launch
            .args
            .iter()
            .find(|argument| std::path::Path::new(argument).is_file())
            .map(std::path::PathBuf::from)
            .expect("temporary Bash startup file");
        assert!(startup_file.is_file());

        drop(launch);

        assert!(!startup_file.exists());
    }

    #[test]
    fn generated_hooks_emit_osc7_and_preserve_slashes() {
        for hook in [
            posix_prompt_hook(),
            zsh_prompt_hook(true),
            fish_prompt_hook(),
            nushell_prompt_hook(),
            powershell_prompt_hook(),
        ] {
            assert!(hook.contains("]7;"));
        }
        assert!(posix_prompt_hook().contains("]7;file://"));
        assert!(posix_prompt_hook().contains("s|%2f|/|g"));
    }

    #[test]
    fn powershell_hook_uses_windows_powershell_compatible_control_bytes() {
        let hook = powershell_prompt_hook();

        // Windows PowerShell 5.1 renders the PowerShell 7-only `e escape literally.
        assert!(hook.contains("$([char]27)]7;"));
        assert!(hook.contains("$([char]7)"));
        assert!(!hook.contains("`e]"));
    }

    #[cfg(unix)]
    #[test]
    fn zsh_user_config_resolves_history_from_original_zdotdir() {
        if !std::process::Command::new("zsh")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
        {
            return;
        }

        let user_config = tempfile::tempdir().expect("temporary user Zsh directory");
        let integration = tempfile::tempdir().expect("temporary integration directory");
        fs::write(
            user_config.path().join(".zshrc"),
            "HISTFILE=\"$ZDOTDIR/.zsh_history\"\nHISTSIZE=100\nSAVEHIST=100\nsetopt share_history\n",
        )
        .expect("write user Zsh config");
        fs::write(
            user_config.path().join(".zsh_history"),
            "oxideterm-history-probe\n",
        )
        .expect("write user Zsh history");

        let config = LocalPtyConfig {
            load_profile: true,
            ..LocalPtyConfig::default()
        };
        let mut launch_env = HashMap::from([(
            "ZDOTDIR".to_string(),
            user_config.path().display().to_string(),
        )]);
        let args = prepare_zsh(&config, integration.path(), &mut launch_env)
            .expect("prepare Zsh integration");

        let output = std::process::Command::new("zsh")
            .args(args)
            .args(["-i", "-c", "fc -l -1"])
            .env("ZDOTDIR", integration.path())
            .output()
            .expect("run integrated Zsh");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(
            output.status.success() && stdout.contains("oxideterm-history-probe"),
            "integrated Zsh did not load original history: stdout={stdout:?}, stderr={:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
