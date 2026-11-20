use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use tempfile::TempDir;

use crate::integration_scripts::normalize_integration_script;
use crate::shell_completion::ShellHistoryFormat;

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
    history_report: Option<(ShellHistoryFormat, PathBuf)>,
}

impl LocalShellIntegration {
    fn merge(mut self, other: Self) -> Self {
        self._directories.extend(other._directories);
        self.history_report = self.history_report.or(other.history_report);
        self
    }

    pub(crate) fn take_history_file(&mut self) -> Option<crate::LocalShellHistoryFile> {
        let (format, path) = self.history_report.as_ref()?;
        // Only our local startup script can supply this path; PTY output is not a file-read API.
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = options.open(path).ok()?;
        let metadata = file.metadata().ok()?;
        if !metadata.is_file() || metadata.len() == 0 {
            return None;
        }
        let mut data = Vec::new();
        file.take(128 * 1024).read_to_end(&mut data).ok()?;
        let bytes = data.strip_suffix(&[0])?;
        let format = *format;
        self.history_report = None;
        #[cfg(unix)]
        let path = {
            use std::os::unix::ffi::OsStrExt;
            PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
        };
        #[cfg(not(unix))]
        let path = PathBuf::from(std::str::from_utf8(bytes).ok()?);
        path.is_absolute()
            .then_some(crate::LocalShellHistoryFile { format, path })
    }
}

pub(crate) fn prepare_local_shell_launch(
    config: &LocalPtyConfig,
    shell: &ShellInfo,
    mut env: HashMap<String, String>,
    default_args: Vec<String>,
) -> LocalShellLaunch {
    let default_args = if shell.id.starts_with("wsl") {
        let mut args = default_args;
        if let Some(cwd) = config.cwd.as_ref() {
            if let Some(index) = args.iter().position(|arg| arg == "--cd") {
                if index + 1 < args.len() {
                    args.remove(index + 1);
                }
                args.remove(index);
            }
            args.extend(["--cd".to_owned(), cwd.to_string_lossy().into_owned()]);
        }
        args
    } else {
        default_args
    };
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

    let format = match shell.id.as_str() {
        "bash" | "git-bash" => Some(ShellHistoryFormat::Bash),
        "zsh" => Some(ShellHistoryFormat::Zsh),
        "fish" => Some(ShellHistoryFormat::Fish),
        "pwsh" | "powershell" => Some(ShellHistoryFormat::PowerShell),
        _ => None,
    };
    if !config.current_directory_shell_integration && format.is_none() {
        return LocalShellLaunch {
            args: default_args,
            env,
            integration: editor_integration,
            integration_state: TerminalCwdIntegrationLaunchState::NotRequested,
        };
    }

    let prepared = prepare_known_shell(config, shell, &mut env, &default_args, format);
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
        integration_state: if config.current_directory_shell_integration {
            integration_state
        } else {
            TerminalCwdIntegrationLaunchState::NotRequested
        },
    }
}

fn prepare_editor_integration(env: &mut HashMap<String, String>) -> Result<LocalShellIntegration> {
    let directory = tempfile::Builder::new()
        .prefix("oxideterm-editor-")
        .tempdir()
        .context("create temporary editor integration directory")?;
    let vim_path = directory.path().join("oxideterm-free-type.vim");
    let emacs_path = directory.path().join("oxideterm-free-type.el");
    write_private_file(
        &vim_path,
        &normalize_integration_script(VIM_FREE_TYPE_INTEGRATION_SOURCE),
    )?;
    write_private_file(
        &emacs_path,
        &normalize_integration_script(EMACS_FREE_TYPE_INTEGRATION_SOURCE),
    )?;
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
        history_report: None,
    })
}

fn prepare_known_shell(
    config: &LocalPtyConfig,
    shell: &ShellInfo,
    env: &mut HashMap<String, String>,
    default_args: &[String],
    format: Option<ShellHistoryFormat>,
) -> Result<Option<(Vec<String>, LocalShellIntegration)>> {
    if shell.id.starts_with("wsl") {
        if default_args
            .iter()
            .any(|arg| matches!(arg.as_str(), "--exec" | "-e" | "--"))
        {
            return Ok(None);
        }
        return prepare_wsl(config, env, default_args).map(Some);
    }
    if shell.id == "cmd" {
        return Ok(None);
    }

    let mut directory = tempfile::Builder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        directory.permissions(fs::Permissions::from_mode(0o700));
    }
    let directory = directory
        .prefix("oxideterm-shell-")
        .tempdir()
        .context("create temporary shell integration directory")?;
    let report = directory.path().join("history-source");
    let hook = if let Some(format) = format {
        write_private_file(&report, "")?;
        history_report_hook(format, &report)
    } else {
        String::new()
    };
    let args = match shell.id.as_str() {
        "bash" | "git-bash"
            if !config.current_directory_shell_integration && config.load_profile =>
        {
            // Preserve native login-shell startup; profiles may extend the inherited prompt command.
            let previous = env
                .get("PROMPT_COMMAND")
                .cloned()
                .or_else(|| std::env::var("PROMPT_COMMAND").ok())
                .unwrap_or_default();
            env.insert("PROMPT_COMMAND".into(), format!("{previous}\nif [[ -z \"${{__oxideterm_history_reported:-}}\" ]]; then {hook}; __oxideterm_history_reported=1; fi"));
            default_args.to_vec()
        }
        "bash" | "git-bash" => prepare_bash(config, directory.path(), &hook)?,
        "zsh" => prepare_zsh(config, directory.path(), env, &hook)?,
        "fish" => prepare_fish(config, directory.path(), &hook)?,
        "nu" | "nu.exe" => prepare_nushell(config, directory.path())?,
        "pwsh" | "powershell" => prepare_powershell(
            directory.path(),
            default_args,
            &hook,
            config.current_directory_shell_integration,
        )?,
        _ => return Ok(None),
    };

    Ok(Some((
        args,
        LocalShellIntegration {
            _directories: vec![directory],
            history_report: format.map(|format| (format, report)),
        },
    )))
}

fn history_report_hook(format: ShellHistoryFormat, report: &Path) -> String {
    let destination = report.to_string_lossy();
    #[cfg(windows)]
    let destination = destination.replace('\\', "/");
    let quoted = posix_quote(&destination);
    match format {
        ShellHistoryFormat::Bash | ShellHistoryFormat::Zsh => {
            let write_path = if cfg!(windows) {
                r#"builtin printf '%s\0' "$(cygpath -aw -- "$HISTFILE")""#
            } else {
                r#"case "$HISTFILE" in /*) builtin printf '%s\0' "$HISTFILE";; *) builtin printf '%s/%s\0' "$PWD" "$HISTFILE";; esac"#
            };
            format!(
                r#"if [[ -n "${{HISTFILE:-}}" ]]; then {write_path}; else builtin printf '\0'; fi >| {quoted}"#
            )
        }
        ShellHistoryFormat::Fish => format!(
            r#"if test -n "$fish_private_mode"; or begin; set -q fish_history; and test -z "$fish_history"; end
    builtin printf '\0'
else
    set -l name fish
    if set -q fish_history; set name "$fish_history"; end
    builtin printf '%s/%s_history\0' "$__fish_user_data_dir" "$name"
end > {quoted}"#
        ),
        ShellHistoryFormat::PowerShell => format!(
            r#"& {{ try {{
    $option = Get-PSReadLineOption -ErrorAction Stop
    $path = if ($option.HistorySaveStyle -ne 'SaveNothing') {{ $option.HistorySavePath }}
    [IO.File]::WriteAllText({0}, [string]$path + [char]0)
}} catch {{ [IO.File]::WriteAllText({0}, [string][char]0) }} }}"#,
            nu_quote(&destination)
        ),
    }
}

fn prepare_bash(config: &LocalPtyConfig, directory: &Path, history: &str) -> Result<Vec<String>> {
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
        &format!(
            "{profile_loader}\n{history}\n{}",
            integration_hooks(
                config.current_directory_shell_integration,
                posix_prompt_hook,
                include_str!("shell_integration/bash.sh")
            ),
        ),
    )?;
    Ok(vec![
        "--noprofile".to_string(),
        "--rcfile".to_string(),
        init_path.to_string_lossy().replace('\\', "/"),
        "-i".to_string(),
    ])
}

fn prepare_zsh(
    config: &LocalPtyConfig,
    directory: &Path,
    env: &mut HashMap<String, String>,
    history: &str,
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
            zsh_source_user_file(".zshenv", false),
        )
    } else {
        format!(
            "unsetopt GLOBAL_RCS\nsetopt RCS\nexport ZDOTDIR={}",
            posix_quote(&directory.display().to_string())
        )
    };
    write_private_file(&directory.join(".zshenv"), &zshenv)?;
    let user_rc = if config.load_profile {
        zsh_source_user_file(".zshrc", true)
    } else {
        let restore = original
            .map(|p| posix_quote(&p.display().to_string()))
            .unwrap_or_else(|| "\"$HOME\"".to_string());
        format!("export ZDOTDIR={restore}\nunsetopt RCS")
    };
    write_private_file(
        &directory.join(".zshrc"),
        &format!(
            "{user_rc}\n{history}\n{}\n{}",
            zsh_import_history(config.load_profile),
            integration_hooks(
                config.current_directory_shell_integration,
                zsh_prompt_hook,
                include_str!("shell_integration/zsh.zsh")
            ),
        ),
    )?;
    env.insert("ZDOTDIR".to_string(), directory.display().to_string());
    // The PTY provides interactive mode; the temporary ZDOTDIR controls whether
    // user startup files are loaded without turning the session into a login shell.
    Ok(Vec::new())
}

fn prepare_fish(config: &LocalPtyConfig, directory: &Path, history: &str) -> Result<Vec<String>> {
    let init_path = directory.join("fish.fish");
    write_private_file(
        &init_path,
        &format!(
            "{history}\n{}",
            integration_hooks(
                config.current_directory_shell_integration,
                fish_prompt_hook,
                include_str!("shell_integration/fish.fish")
            )
        ),
    )?;
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

fn prepare_powershell(
    directory: &Path,
    default_args: &[String],
    history: &str,
    cwd: bool,
) -> Result<Vec<String>> {
    let init_path = directory.join("profile.ps1");
    write_private_file(
        &init_path,
        &format!(
            "{history}\n{}",
            integration_hooks(
                cwd,
                powershell_prompt_hook,
                include_str!("shell_integration/powershell.ps1")
            )
        ),
    )?;
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

fn prepare_wsl(
    config: &LocalPtyConfig,
    env: &mut HashMap<String, String>,
    default_args: &[String],
) -> Result<(Vec<String>, LocalShellIntegration)> {
    let directory = tempfile::Builder::new()
        .prefix("oxideterm-wsl-")
        .tempdir()
        .context("create temporary WSL integration directory")?;
    let path = directory.path();
    prepare_bash(config, path, "")?;
    prepare_fish(config, path, "")?;
    prepare_powershell(path, &[], "", true)?;
    write_private_file(
        &path.join("nushell.nu"),
        &normalize_integration_script(include_str!("shell_integration/nushell.nu")),
    )?;
    write_private_file(
        &path.join("oxideterm-free-type.vim"),
        &normalize_integration_script(VIM_FREE_TYPE_INTEGRATION_SOURCE),
    )?;
    write_private_file(
        &path.join("oxideterm-free-type.el"),
        &normalize_integration_script(EMACS_FREE_TYPE_INTEGRATION_SOURCE),
    )?;
    let zshenv = if config.load_profile {
        format!(
            "typeset -g __oxideterm_user_zdotdir=\"$OXIDETERM_USER_ZDOTDIR\"\n\
             typeset -g __oxideterm_integration_zdotdir=\"$ZDOTDIR\"\n\
             unset OXIDETERM_USER_ZDOTDIR\n{}\nsetopt RCS",
            zsh_source_user_file(".zshenv", false),
        )
    } else {
        "unsetopt GLOBAL_RCS\nsetopt RCS".to_string()
    };
    write_private_file(&path.join(".zshenv"), &zshenv)?;
    write_private_file(
        &path.join(".zshrc"),
        &format!(
            "{}\n{}\n{}\n{}",
            config
                .load_profile
                .then(|| zsh_source_user_file(".zshrc", true))
                .unwrap_or_default(),
            zsh_import_history(config.load_profile),
            zsh_prompt_hook(),
            normalize_integration_script(include_str!("shell_integration/zsh.zsh")),
        ),
    )?;
    env.insert(
        "OXIDETERM_SHELL_DIR".to_string(),
        path.display().to_string(),
    );
    let wslenv = env.entry("WSLENV".to_string()).or_default();
    if !wslenv.is_empty() {
        wslenv.push(':');
    }
    wslenv.push_str("OXIDETERM_SHELL_DIR/p");
    let mut args = default_args.to_vec();
    args.extend([
        "--exec".to_string(),
        "/bin/sh".to_string(),
        "-c".to_string(),
        normalize_integration_script(include_str!("shell_integration/wsl.sh")).into_owned(),
        "oxideterm".to_string(),
        if config.load_profile { "1" } else { "0" }.to_string(),
    ]);
    Ok((
        args,
        LocalShellIntegration {
            _directories: vec![directory],
            history_report: None,
        },
    ))
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

fn zsh_source_user_file(name: &str, final_user_file: bool) -> String {
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
        format!("{source}\nexport ZDOTDIR=\"$__oxideterm_integration_zdotdir\"")
    }
}

fn integration_hooks(enabled: bool, prompt: fn() -> String, script: &'static str) -> String {
    if enabled {
        format!("{}\n{}", prompt(), normalize_integration_script(script))
    } else {
        String::new()
    }
}

fn posix_prompt_hook() -> String {
    r#"if [[ ${__oxideterm_cwd_hooks:-0} != 1 ]]; then
__oxideterm_pct_path() {
    command printf '%s' "$1" | command od -An -tx1 -v | command tr -d ' \n' | command sed 's/../%&/g; s|%2f|/|g'
}
__oxideterm_emit_cwd() {
    __oxideterm_cwd=$(pwd -P 2>/dev/null || pwd 2>/dev/null) || return
    command printf '\033]7;file://%s\007' "$(__oxideterm_pct_path "$__oxideterm_cwd")"
}
if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == 'declare -a '* ]]; then
    PROMPT_COMMAND=(__oxideterm_emit_cwd "${PROMPT_COMMAND[@]}")
else
    PROMPT_COMMAND="__oxideterm_emit_cwd${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
fi
__oxideterm_cwd_hooks=1
__oxideterm_emit_cwd
fi"#.to_string()
}

fn zsh_import_history(import_history: bool) -> &'static str {
    if import_history {
        r#"# The real .zshrc is nested, so import history after it has selected HISTFILE.
if (( ${#history[@]} == 0 )) && [[ -n "$HISTFILE" && -r "$HISTFILE" ]]; then
    builtin fc -R "$HISTFILE"
fi"#
    } else {
        ""
    }
}

fn zsh_prompt_hook() -> String {
    format!(
        r#"function __oxideterm_pct_path() {{
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
    normalize_integration_script(include_str!("shell_integration/nushell.nu")).into_owned()
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
    $lastSucceeded = $?
    __oxideterm_emit_cwd
    if ($script:__oxideterm_original_prompt) {
        if (-not $lastSucceeded) { Write-Error 'Command failed.' -ErrorAction Ignore }
        $script:__oxideterm_original_prompt.Invoke()
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
    fn wsl_project_directory_replaces_default_without_shell_interpolation() {
        let (mut config, mut shell) = config_for("wsl-ubuntu");
        config.cwd = Some("/home/user/project with spaces;echo ignored".into());
        shell.args = vec!["-d".into(), "Ubuntu".into(), "--cd".into(), "~".into()];
        let launch =
            prepare_local_shell_launch(&config, &shell, HashMap::new(), shell.args.clone());
        assert_eq!(
            launch.args[..4],
            [
                "-d",
                "Ubuntu",
                "--cd",
                "/home/user/project with spaces;echo ignored"
            ]
        );
    }

    #[test]
    fn local_integration_scripts_use_lf() {
        for (shell_id, startup_name) in [
            ("bash", "bashrc"),
            ("zsh", ".zshrc"),
            ("fish", "fish.fish"),
            ("nu", "config.nu"),
            ("pwsh", "profile.ps1"),
            ("wsl-ubuntu", "bashrc"),
        ] {
            let (mut config, shell) = config_for(shell_id);
            config.load_profile = false;
            let launch = prepare_local_shell_launch(&config, &shell, HashMap::new(), Vec::new());
            let integration = launch
                .integration
                .as_ref()
                .expect("owned integration files");
            let mut names = Vec::new();
            for directory in &integration._directories {
                for entry in fs::read_dir(directory.path()).unwrap() {
                    let path = entry.unwrap().path();
                    names.push(path.file_name().unwrap().to_string_lossy().into_owned());
                    assert!(
                        !fs::read(&path).unwrap().contains(&b'\r'),
                        "{shell_id}: {} contains a carriage return",
                        path.file_name().unwrap().display()
                    );
                }
            }
            assert!(
                names.iter().any(|name| name == startup_name),
                "{shell_id} startup file was not generated"
            );
            if shell_id.starts_with("wsl") {
                let command_index = launch.args.iter().position(|arg| arg == "-c").unwrap();
                assert!(
                    !launch.args[command_index + 1].contains('\r'),
                    "WSL launch command contains a carriage return"
                );
            }
        }
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
            normalize_integration_script(VIM_FREE_TYPE_INTEGRATION_SOURCE)
        );
        assert_eq!(
            fs::read_to_string(&emacs_path).expect("read Emacs adapter"),
            normalize_integration_script(EMACS_FREE_TYPE_INTEGRATION_SOURCE)
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
}
