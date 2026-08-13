use oxideterm_ssh::{RemoteEnvInfo, SftpError, SftpSession};

use crate::{EMACS_FREE_TYPE_INTEGRATION_SOURCE, VIM_FREE_TYPE_INTEGRATION_SOURCE};

pub const REMOTE_SHELL_INTEGRATION_VERSION: u32 = 4;
pub const REMOTE_SHELL_INTEGRATION_RELATIVE_DIR: &str = ".oxideterm/shell-integration";

const MANAGED_BLOCK_START: &str = ">>> OxideTerm remote shell integration >>>";
const MANAGED_BLOCK_END: &str = "<<< OxideTerm remote shell integration <<<";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteShellKind {
    Bash,
    Zsh,
    Fish,
    Nushell,
    PowerShell,
}

impl RemoteShellKind {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Bash => "Bash",
            Self::Zsh => "Zsh",
            Self::Fish => "Fish",
            Self::Nushell => "Nushell",
            Self::PowerShell => "PowerShell",
        }
    }

    fn integration_file_name(self) -> &'static str {
        match self {
            Self::Bash => "bash.sh",
            Self::Zsh => "zsh.zsh",
            Self::Fish => "fish.fish",
            Self::Nushell => "nushell.nu",
            Self::PowerShell => "powershell.ps1",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteShellIntegrationState {
    NotInstalled,
    FilesOnly,
    Installed,
    NeedsUpdate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteShellIntegrationStatus {
    pub shell: RemoteShellKind,
    pub state: RemoteShellIntegrationState,
    pub integration_directory: String,
    pub integration_file: String,
    pub startup_file: String,
}

#[derive(Clone, Debug)]
struct RemoteShellIntegrationLayout {
    shell: RemoteShellKind,
    home: String,
    integration_directory: String,
    integration_file: String,
    startup_file: String,
}

/// Inspects the integration without changing any remote files.
pub async fn inspect_remote_shell_integration(
    sftp: &SftpSession,
    remote_env: Option<&RemoteEnvInfo>,
) -> Result<RemoteShellIntegrationStatus, String> {
    let layout = integration_layout(sftp, remote_env)?;
    let startup_content = read_optional_text(sftp, &layout.startup_file).await?;
    let expected_reference = startup_reference(layout.shell);
    let reference_matches = startup_content.as_deref().is_some_and(|content| {
        complete_managed_blocks(content)
            .iter()
            .any(|span| content[span.start..span.end].trim_end() == expected_reference)
    });
    let has_reference = startup_content
        .as_deref()
        .is_some_and(|content| !complete_managed_blocks(content).is_empty());
    let mut has_package_file = false;
    let mut package_matches = true;
    // The status covers the complete owned package, including editor adapters,
    // so a partial or legacy installation can be repaired deterministically.
    for (name, expected_content) in integration_files() {
        let path = join_remote(&layout.integration_directory, name);
        let content = read_optional_text(sftp, &path).await?;
        has_package_file |= content.is_some();
        package_matches &= content
            .as_deref()
            .is_some_and(|content| content == expected_content);
    }
    let state = match (
        has_reference,
        reference_matches,
        has_package_file,
        package_matches,
    ) {
        (true, true, true, true) => RemoteShellIntegrationState::Installed,
        (true, _, _, _) => RemoteShellIntegrationState::NeedsUpdate,
        (false, _, true, _) => RemoteShellIntegrationState::FilesOnly,
        (false, _, false, _) => RemoteShellIntegrationState::NotInstalled,
    };
    Ok(status_from_layout(layout, state))
}

/// Writes inspectable shell files and adds one clearly marked startup reference.
pub async fn install_remote_shell_integration(
    sftp: &SftpSession,
    remote_env: Option<&RemoteEnvInfo>,
) -> Result<RemoteShellIntegrationStatus, String> {
    let layout = integration_layout(sftp, remote_env)?;
    ensure_remote_directory(sftp, &join_remote(&layout.home, ".oxideterm")).await?;
    ensure_remote_directory(sftp, &layout.integration_directory).await?;

    for (name, content) in integration_files() {
        let path = join_remote(&layout.integration_directory, name);
        sftp.write_content(&path, content.as_bytes())
            .await
            .map_err(|error| format!("failed to write {path}: {error}"))?;
    }

    if let Some(parent) = remote_parent(&layout.startup_file) {
        ensure_remote_directory(sftp, &parent).await?;
    }
    let current = read_optional_text(sftp, &layout.startup_file)
        .await?
        .unwrap_or_default();
    let updated = install_managed_block(&current, &startup_reference(layout.shell));
    sftp.replace_config_content(&layout.startup_file, updated.as_bytes())
        .await
        .map_err(|error| {
            format!(
                "failed to update startup file {}: {error}",
                layout.startup_file
            )
        })?;

    Ok(status_from_layout(
        layout,
        RemoteShellIntegrationState::Installed,
    ))
}

/// Removes only OxideTerm's marked startup block and optionally its owned files.
pub async fn remove_remote_shell_integration(
    sftp: &SftpSession,
    remote_env: Option<&RemoteEnvInfo>,
    delete_owned_files: bool,
) -> Result<RemoteShellIntegrationStatus, String> {
    let layout = integration_layout(sftp, remote_env)?;
    if let Some(current) = read_optional_text(sftp, &layout.startup_file).await? {
        let updated = remove_managed_block(&current);
        if updated != current {
            sftp.replace_config_content(&layout.startup_file, updated.as_bytes())
                .await
                .map_err(|error| {
                    format!(
                        "failed to update startup file {}: {error}",
                        layout.startup_file
                    )
                })?;
        }
    }
    if delete_owned_files {
        match sftp.delete_recursive(&layout.integration_directory).await {
            Ok(_) | Err(SftpError::FileNotFound(_) | SftpError::DirectoryNotFound(_)) => {}
            Err(error) => {
                return Err(format!(
                    "failed to delete {}: {error}",
                    layout.integration_directory
                ));
            }
        }
    }
    let integration_file_exists = !delete_owned_files
        && read_optional_text(sftp, &layout.integration_file)
            .await?
            .is_some();
    let state = if delete_owned_files || !integration_file_exists {
        RemoteShellIntegrationState::NotInstalled
    } else {
        RemoteShellIntegrationState::FilesOnly
    };
    Ok(status_from_layout(layout, state))
}

fn integration_layout(
    sftp: &SftpSession,
    remote_env: Option<&RemoteEnvInfo>,
) -> Result<RemoteShellIntegrationLayout, String> {
    let remote_env = remote_env.ok_or_else(|| {
        "remote shell detection is still unavailable; reconnect and try again".to_string()
    })?;
    let home = remote_env
        .home
        .as_deref()
        .unwrap_or_else(|| sftp.home())
        .trim_end_matches(['/', '\\'])
        .to_string();
    if home.is_empty() {
        return Err("remote home directory is unavailable".to_string());
    }
    let shell = detect_remote_shell(remote_env.shell.as_deref()).ok_or_else(|| {
        let detected = remote_env.shell.as_deref().unwrap_or("unknown");
        format!("unsupported remote shell: {detected}")
    })?;
    let integration_directory = join_remote(&home, REMOTE_SHELL_INTEGRATION_RELATIVE_DIR);
    let integration_file = join_remote(&integration_directory, shell.integration_file_name());
    let startup_file = startup_file_path(shell, remote_env, &home);
    Ok(RemoteShellIntegrationLayout {
        shell,
        home,
        integration_directory,
        integration_file,
        startup_file,
    })
}

fn status_from_layout(
    layout: RemoteShellIntegrationLayout,
    state: RemoteShellIntegrationState,
) -> RemoteShellIntegrationStatus {
    RemoteShellIntegrationStatus {
        shell: layout.shell,
        state,
        integration_directory: layout.integration_directory,
        integration_file: layout.integration_file,
        startup_file: layout.startup_file,
    }
}

fn detect_remote_shell(shell: Option<&str>) -> Option<RemoteShellKind> {
    let shell = shell?.trim().to_ascii_lowercase().replace('\\', "/");
    let executable = shell.rsplit('/').next().unwrap_or(&shell);
    if executable.starts_with("powershell") || executable.starts_with("pwsh") {
        Some(RemoteShellKind::PowerShell)
    } else if executable == "bash" || executable.starts_with("bash ") {
        Some(RemoteShellKind::Bash)
    } else if executable == "zsh" || executable.starts_with("zsh ") {
        Some(RemoteShellKind::Zsh)
    } else if executable == "fish" || executable.starts_with("fish ") {
        Some(RemoteShellKind::Fish)
    } else if executable == "nu" || executable == "nushell" || executable.starts_with("nushell ") {
        Some(RemoteShellKind::Nushell)
    } else {
        None
    }
}

fn startup_file_path(shell: RemoteShellKind, remote_env: &RemoteEnvInfo, home: &str) -> String {
    match shell {
        RemoteShellKind::Bash => join_remote(home, ".bashrc"),
        RemoteShellKind::Zsh => {
            join_remote(remote_env.zdotdir.as_deref().unwrap_or(home), ".zshrc")
        }
        RemoteShellKind::Fish => remote_env.xdg_config_home.as_deref().map_or_else(
            || join_remote(home, ".config/fish/config.fish"),
            |config_home| join_remote(config_home, "fish/config.fish"),
        ),
        RemoteShellKind::Nushell if remote_env.os_type.eq_ignore_ascii_case("macos") => {
            join_remote(home, "Library/Application Support/nushell/config.nu")
        }
        RemoteShellKind::Nushell => join_remote(home, ".config/nushell/config.nu"),
        RemoteShellKind::PowerShell
            if remote_env.os_type.to_ascii_lowercase().contains("windows") =>
        {
            let profile_directory = if remote_env.shell.as_deref().is_some_and(|shell| {
                let executable = shell.trim().to_ascii_lowercase().replace('\\', "/");
                let executable = executable.rsplit('/').next().unwrap_or(&executable);
                executable.starts_with("powershell")
            }) {
                "Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1"
            } else {
                "Documents/PowerShell/Microsoft.PowerShell_profile.ps1"
            };
            join_remote(home, profile_directory)
        }
        RemoteShellKind::PowerShell => {
            join_remote(home, ".config/powershell/Microsoft.PowerShell_profile.ps1")
        }
    }
}

fn integration_files() -> [(&'static str, &'static str); 8] {
    [
        ("README.txt", REMOTE_INTEGRATION_README),
        ("bash.sh", BASH_INTEGRATION),
        ("zsh.zsh", ZSH_INTEGRATION),
        ("fish.fish", FISH_INTEGRATION),
        ("nushell.nu", NUSHELL_INTEGRATION),
        ("powershell.ps1", POWERSHELL_INTEGRATION),
        ("oxideterm-free-type.vim", VIM_FREE_TYPE_INTEGRATION_SOURCE),
        ("oxideterm-free-type.el", EMACS_FREE_TYPE_INTEGRATION_SOURCE),
    ]
}

fn startup_reference(shell: RemoteShellKind) -> String {
    let reference = match shell {
        RemoteShellKind::Bash => {
            r#"[ -r "$HOME/.oxideterm/shell-integration/bash.sh" ] && . "$HOME/.oxideterm/shell-integration/bash.sh""#
        }
        RemoteShellKind::Zsh => {
            r#"[ -r "$HOME/.oxideterm/shell-integration/zsh.zsh" ] && source "$HOME/.oxideterm/shell-integration/zsh.zsh""#
        }
        RemoteShellKind::Fish => {
            r#"test -r "$HOME/.oxideterm/shell-integration/fish.fish"; and source "$HOME/.oxideterm/shell-integration/fish.fish""#
        }
        RemoteShellKind::Nushell => {
            r#"source ($nu.home-path | path join '.oxideterm' 'shell-integration' 'nushell.nu')"#
        }
        RemoteShellKind::PowerShell => {
            r#". (Join-Path $HOME '.oxideterm/shell-integration/powershell.ps1')"#
        }
    };
    format!(
        "# {MANAGED_BLOCK_START}\n# oxideterm-shell-integration-version: {REMOTE_SHELL_INTEGRATION_VERSION}\n{reference}\n# {MANAGED_BLOCK_END}"
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ManagedBlockSpan {
    start: usize,
    end: usize,
}

fn install_managed_block(content: &str, block: &str) -> String {
    let spans = complete_managed_blocks(content);
    if spans.is_empty() {
        return append_complete_block(content, block);
    };
    let first = spans[0];
    let mut updated = String::with_capacity(content.len());
    updated.push_str(&content[..first.start]);
    updated.push_str(block);
    updated.push('\n');
    let mut cursor = first.end;
    for span in spans.iter().skip(1) {
        updated.push_str(&content[cursor..span.start]);
        cursor = span.end;
    }
    updated.push_str(&content[cursor..]);
    updated
}

fn append_complete_block(content: &str, block: &str) -> String {
    let trimmed = content.trim_end_matches(['\r', '\n']);
    if trimmed.is_empty() {
        format!("{block}\n")
    } else {
        format!("{trimmed}\n\n{block}\n")
    }
}

fn remove_managed_block(content: &str) -> String {
    let spans = complete_managed_blocks(content);
    if spans.is_empty() {
        return content.to_string();
    }
    let mut updated = String::with_capacity(content.len());
    let mut cursor = 0;
    for span in spans {
        let start = if content[cursor..span.start].ends_with("\n\n") {
            span.start.saturating_sub(1)
        } else {
            span.start
        };
        updated.push_str(&content[cursor..start]);
        cursor = span.end;
    }
    updated.push_str(&content[cursor..]);
    updated
}

fn complete_managed_blocks(content: &str) -> Vec<ManagedBlockSpan> {
    let start_marker = format!("# {MANAGED_BLOCK_START}");
    let end_marker = format!("# {MANAGED_BLOCK_END}");
    let mut spans = Vec::new();
    let mut pending_start = None;
    let mut offset = 0;
    for line in content.split_inclusive('\n') {
        let normalized = line.trim_end_matches(['\r', '\n']).trim();
        if normalized == start_marker {
            // A later start marker supersedes an incomplete earlier marker so
            // repair never consumes unrelated text between malformed blocks.
            pending_start = Some(offset);
        } else if normalized == end_marker
            && let Some(start) = pending_start.take()
        {
            spans.push(ManagedBlockSpan {
                start,
                end: offset + line.len(),
            });
        }
        offset += line.len();
    }
    spans
}

async fn read_optional_text(sftp: &SftpSession, path: &str) -> Result<Option<String>, String> {
    match sftp.read_file_bytes(path).await {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|error| format!("remote file {path} is not UTF-8: {error}")),
        Err(SftpError::FileNotFound(_) | SftpError::DirectoryNotFound(_)) => Ok(None),
        Err(error) => Err(format!("failed to read {path}: {error}")),
    }
}

async fn ensure_remote_directory(sftp: &SftpSession, path: &str) -> Result<(), String> {
    match sftp.stat(path).await {
        Ok(info) if info.file_type == oxideterm_ssh::FileType::Directory => return Ok(()),
        Ok(_) => return Err(format!("remote path is not a directory: {path}")),
        Err(SftpError::FileNotFound(_) | SftpError::DirectoryNotFound(_)) => {}
        Err(error) => return Err(format!("failed to inspect {path}: {error}")),
    }
    if let Some(parent) = remote_parent(path) {
        Box::pin(ensure_remote_directory(sftp, &parent)).await?;
    }
    match sftp.mkdir(path).await {
        Ok(()) => Ok(()),
        Err(error) => match sftp.stat(path).await {
            Ok(info) if info.file_type == oxideterm_ssh::FileType::Directory => Ok(()),
            _ => Err(format!("failed to create {path}: {error}")),
        },
    }
}

fn join_remote(base: &str, relative: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches(['/', '\\']),
        relative.trim_start_matches(['/', '\\']).replace('\\', "/")
    )
}

fn remote_parent(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let (parent, _) = normalized.rsplit_once('/')?;
    (!parent.is_empty() && !parent.ends_with(':')).then(|| parent.to_string())
}

const REMOTE_INTEGRATION_README: &str = r#"OxideTerm Remote Shell Integration
=====================================

Version: 4
Directory protocol: OSC 7
Private editor protocol: OSC 7719 v3

These readable shell hooks report only the current working directory and host
name through standard OSC 7. They do not run commands, read command text, or
contain credentials. An application running in the shell can emit the same
control sequence, so this metadata is a terminal integration signal rather
than an authentication boundary.

The active shell startup file contains a clearly marked OxideTerm reference.
Use OxideTerm Settings > Terminal > Awareness & Integration to inspect, repair,
or remove the reference and these files.

The same directory contains optional Free Type Mode adapters for Vim, Neovim,
and Emacs. Their paths are exported only when the SSH server accepts OxideTerm's
per-channel capability marker. The adapters suppress private OSC inside tmux,
GNU screen, and Zellij because one shared pane cannot isolate output by attached
terminal client. Load the matching adapter explicitly from your editor
configuration to enable full-screen editor integration.
"#;

const BASH_INTEGRATION: &str = r#"# OxideTerm remote shell integration v4.
# Reports cwd and host through standard OSC 7 and gates private editor adapters.
if [ -n "${LC_OXIDETERM_SESSION:-}" ] || [ "${OXIDETERM_PRIVATE_OSC:-}" = 1 ]; then
  export OXIDETERM_PRIVATE_OSC=1
  export OXIDETERM_VIM_INTEGRATION="$HOME/.oxideterm/shell-integration/oxideterm-free-type.vim"
  export OXIDETERM_EMACS_INTEGRATION="$HOME/.oxideterm/shell-integration/oxideterm-free-type.el"
fi
unset LC_OXIDETERM_SESSION
__oxideterm_pct() {
  printf '%s' "$1" | od -An -tx1 -v | tr -d ' \n' | sed 's/../%&/g'
}
__oxideterm_pct_path() {
  __oxideterm_pct "$1" | sed 's|%2f|/|g'
}
__oxideterm_emit_remote_metadata() {
  __oxideterm_cwd=$(pwd -P 2>/dev/null || pwd 2>/dev/null) || return
  __oxideterm_host=${HOSTNAME:-$(hostname 2>/dev/null || printf '')}
  __oxideterm_host=$(printf '%s' "$__oxideterm_host" | tr -cd 'A-Za-z0-9._-')
  [ -n "$__oxideterm_host" ] || __oxideterm_host=localhost
  printf '\033]7;file://%s%s\007' "$__oxideterm_host" "$(__oxideterm_pct_path "$__oxideterm_cwd")"
}
__oxideterm_prompt_hook() {
  declare -F __oxideterm_emit_remote_metadata >/dev/null 2>&1 && __oxideterm_emit_remote_metadata
}
__oxideterm_hook_name=__oxideterm_prompt_hook
if declare -p PROMPT_COMMAND 2>/dev/null | grep -Eq '^declare -[A-Za-z]*a'; then
  __oxideterm_prompt_commands=()
  __oxideterm_hook_found=0
  for __oxideterm_prompt_command in "${PROMPT_COMMAND[@]}"; do
    case "$__oxideterm_prompt_command" in
      __oxideterm_emit_remote_metadata|__oxideterm_prompt_hook) ;;
      *)
        __oxideterm_prompt_commands+=("$__oxideterm_prompt_command")
        [ "$__oxideterm_prompt_command" = "$__oxideterm_hook_name" ] && __oxideterm_hook_found=1
        ;;
    esac
  done
  [ "$__oxideterm_hook_found" -eq 1 ] || __oxideterm_prompt_commands+=("$__oxideterm_hook_name")
  PROMPT_COMMAND=("${__oxideterm_prompt_commands[@]}")
  unset __oxideterm_prompt_commands __oxideterm_prompt_command __oxideterm_hook_found
else
  case ";${PROMPT_COMMAND-};" in
    *";__oxideterm_prompt_hook;"*) ;;
    *) PROMPT_COMMAND="__oxideterm_prompt_hook${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;;
  esac
fi
unset __oxideterm_hook_name
"#;

const ZSH_INTEGRATION: &str = concat!(
    "# OxideTerm remote shell integration v4.\n",
    "# Reports cwd and host through standard OSC 7 and gates private editor adapters.\n",
    "if [ -n \"${LC_OXIDETERM_SESSION:-}\" ] || [ \"${OXIDETERM_PRIVATE_OSC:-}\" = 1 ]; then\n  export OXIDETERM_PRIVATE_OSC=1\n  export OXIDETERM_VIM_INTEGRATION=\"$HOME/.oxideterm/shell-integration/oxideterm-free-type.vim\"\n  export OXIDETERM_EMACS_INTEGRATION=\"$HOME/.oxideterm/shell-integration/oxideterm-free-type.el\"\nfi\n",
    "unset LC_OXIDETERM_SESSION\n",
    "__oxideterm_pct() {\n  printf '%s' \"$1\" | od -An -tx1 -v | tr -d ' \\n' | sed 's/../%&/g'\n}\n",
    "__oxideterm_pct_path() {\n  __oxideterm_pct \"$1\" | sed 's|%2f|/|g'\n}\n",
    "__oxideterm_emit_remote_metadata() {\n  __oxideterm_cwd=$(pwd -P 2>/dev/null || pwd 2>/dev/null) || return\n  __oxideterm_host=${HOSTNAME:-$(hostname 2>/dev/null || printf '')}\n  __oxideterm_host=$(printf '%s' \"$__oxideterm_host\" | tr -cd 'A-Za-z0-9._-')\n  [ -n \"$__oxideterm_host\" ] || __oxideterm_host=localhost\n  printf '\\033]7;file://%s%s\\007' \"$__oxideterm_host\" \"$(__oxideterm_pct_path \"$__oxideterm_cwd\")\"\n}\n",
    "autoload -Uz add-zsh-hook\nadd-zsh-hook -d precmd __oxideterm_emit_remote_metadata 2>/dev/null\nadd-zsh-hook precmd __oxideterm_emit_remote_metadata\n"
);

const FISH_INTEGRATION: &str = r#"# OxideTerm remote shell integration v4.
# Reports cwd and host through standard OSC 7 and gates private editor adapters.
set -l __oxideterm_private_session 0
if set -q LC_OXIDETERM_SESSION; and test -n "$LC_OXIDETERM_SESSION"
    set __oxideterm_private_session 1
else if set -q OXIDETERM_PRIVATE_OSC; and test "$OXIDETERM_PRIVATE_OSC" = 1
    set __oxideterm_private_session 1
end
if test "$__oxideterm_private_session" = 1
    set -gx OXIDETERM_PRIVATE_OSC 1
    set -gx OXIDETERM_VIM_INTEGRATION "$HOME/.oxideterm/shell-integration/oxideterm-free-type.vim"
    set -gx OXIDETERM_EMACS_INTEGRATION "$HOME/.oxideterm/shell-integration/oxideterm-free-type.el"
end
set -e LC_OXIDETERM_SESSION
set -e __oxideterm_private_session
function __oxideterm_pct
    command printf '%s' "$argv[1]" | command od -An -tx1 -v | command tr -d ' \n' | command sed 's/../%&/g'
end
function __oxideterm_pct_path
    __oxideterm_pct "$argv[1]" | string replace -a '%2f' '/'
end
function __oxideterm_emit_remote_metadata --on-event fish_prompt
    set -l __oxideterm_cwd (pwd -P 2>/dev/null; or pwd 2>/dev/null)
    set -l __oxideterm_host "$HOSTNAME"
    test -n "$__oxideterm_host"; or set __oxideterm_host (hostname 2>/dev/null; or command printf '')
    set __oxideterm_host (command printf '%s' "$__oxideterm_host" | command tr -cd 'A-Za-z0-9._-')
    test -n "$__oxideterm_host"; or set __oxideterm_host localhost
    command printf '\033]7;file://%s%s\007' "$__oxideterm_host" (__oxideterm_pct_path "$__oxideterm_cwd")
end
"#;

const NUSHELL_INTEGRATION: &str = r#"# OxideTerm remote shell integration v4.
# Reports cwd and host through standard OSC 7 and gates private editor adapters.
let __oxideterm_private_session = (($env.LC_OXIDETERM_SESSION? | default '') != '') or (($env.OXIDETERM_PRIVATE_OSC? | default '') == '1')
if $__oxideterm_private_session {
    $env.OXIDETERM_PRIVATE_OSC = '1'
    $env.OXIDETERM_VIM_INTEGRATION = ($nu.home-path | path join '.oxideterm' 'shell-integration' 'oxideterm-free-type.vim')
    $env.OXIDETERM_EMACS_INTEGRATION = ($nu.home-path | path join '.oxideterm' 'shell-integration' 'oxideterm-free-type.el')
}
hide-env --ignore-errors LC_OXIDETERM_SESSION
def __oxideterm_pct [value: string] {
    $value | url encode --all
}
def __oxideterm_pct_path [value: string] {
    let __oxideterm_path = ($value | str replace --all '\' '/')
    let __oxideterm_uri_path = if ($__oxideterm_path | str contains --regex '^[A-Za-z]:/') {
        '/' + $__oxideterm_path
    } else {
        $__oxideterm_path
    }
    __oxideterm_pct $__oxideterm_uri_path | str replace --all '%2F' '/'
}
def __oxideterm_emit_remote_metadata [] {
    let __oxideterm_host = ($env.HOSTNAME? | default ($env.COMPUTERNAME? | default 'localhost') | default --empty 'localhost')
    print --no-newline $"\u{1b}]7;file://(__oxideterm_pct $__oxideterm_host)(__oxideterm_pct_path (pwd | into string))\u{07}"
}
if (($env.OXIDETERM_SHELL_INTEGRATION_VERSION? | default 0) != 4) {
    $env.OXIDETERM_SHELL_INTEGRATION_VERSION = 4
    $env.config = ($env.config | upsert hooks.pre_prompt (($env.config.hooks.pre_prompt? | default []) | append {|| __oxideterm_emit_remote_metadata }))
}
"#;

const POWERSHELL_INTEGRATION: &str = r#"# OxideTerm remote shell integration v4.
# Reports cwd and host through standard OSC 7 and gates private editor adapters.
if ($env:LC_OXIDETERM_SESSION -or $env:OXIDETERM_PRIVATE_OSC -eq '1') {
    $env:OXIDETERM_PRIVATE_OSC = '1'
    $env:OXIDETERM_VIM_INTEGRATION = Join-Path $HOME '.oxideterm/shell-integration/oxideterm-free-type.vim'
    $env:OXIDETERM_EMACS_INTEGRATION = Join-Path $HOME '.oxideterm/shell-integration/oxideterm-free-type.el'
}
Remove-Item Env:LC_OXIDETERM_SESSION -ErrorAction SilentlyContinue
if (-not $global:__oxideterm_shell_integration_v4) {
    $global:__oxideterm_shell_integration_v4 = $true
    $legacyPrompt = Get-Variable -Name __oxideterm_original_prompt -Scope Script -ErrorAction SilentlyContinue
    # Reuse the pre-v3 prompt when a repaired profile is reloaded in place;
    # chaining the v3 OxideTerm wrapper would recurse through the same variable.
    $script:__oxideterm_v4_original_prompt = if ($global:__oxideterm_shell_integration_v3 -and $legacyPrompt) { $legacyPrompt.Value } elseif (Test-Path Function:\prompt) { (Get-Command prompt).ScriptBlock } else { $null }
    Remove-Variable legacyPrompt -ErrorAction SilentlyContinue
    function global:__oxideterm_pct {
        param([string]$Value)
        -join ([System.Text.Encoding]::UTF8.GetBytes($Value) | ForEach-Object { '%' + $_.ToString('x2') })
    }
    function global:__oxideterm_pct_path {
        param([string]$Value)
        (__oxideterm_pct ($Value -replace '\\', '/')) -replace '%2f', '/'
    }
    function global:__oxideterm_emit_remote_metadata {
        $location = Get-Location
        $cwd = if ($location.ProviderPath) { $location.ProviderPath } else { $location.Path }
        $hostName = if ($env:HOSTNAME) { $env:HOSTNAME } elseif ($env:COMPUTERNAME) { $env:COMPUTERNAME } else { [System.Net.Dns]::GetHostName() }
        $hostName = [Regex]::Replace($hostName, '[^A-Za-z0-9._-]', '')
        if (-not $hostName) { $hostName = 'localhost' }
        $uriPath = $cwd -replace '\\', '/'
        if ($uriPath -match '^[A-Za-z]:/') { $uriPath = '/' + $uriPath }
        [Console]::Out.Write("$([char]27)]7;file://$hostName$(__oxideterm_pct_path $uriPath)$([char]7)")
    }
    function global:prompt {
        __oxideterm_emit_remote_metadata
        if ($script:__oxideterm_v4_original_prompt) { & $script:__oxideterm_v4_original_prompt } else { "PS $($executionContext.SessionState.Path.CurrentLocation)$('>' * ($nestedPromptLevel + 1)) " }
    }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_detection_accepts_paths_and_windows_version_labels() {
        assert_eq!(
            detect_remote_shell(Some("/bin/bash")),
            Some(RemoteShellKind::Bash)
        );
        assert_eq!(
            detect_remote_shell(Some("/usr/bin/fish")),
            Some(RemoteShellKind::Fish)
        );
        assert_eq!(
            detect_remote_shell(Some("PowerShell 7.5.2")),
            Some(RemoteShellKind::PowerShell)
        );
        assert_eq!(detect_remote_shell(Some("/bin/tcsh")), None);
    }

    #[test]
    fn managed_startup_block_is_idempotent_and_removable() {
        let original = "export EDITOR=vim\n";
        let block = startup_reference(RemoteShellKind::Bash);
        let installed = install_managed_block(original, &block);
        let reinstalled = install_managed_block(&installed, &block);
        assert_eq!(installed, reinstalled);
        assert_eq!(remove_managed_block(&installed), original);
    }

    #[test]
    fn managed_block_parser_ignores_marker_text_and_preserves_incomplete_blocks() {
        let block = startup_reference(RemoteShellKind::Zsh);
        let original = format!(
            "echo '# {MANAGED_BLOCK_START}'\n# {MANAGED_BLOCK_START}\nlegacy without end\n"
        );
        let installed = install_managed_block(&original, &block);
        assert!(installed.starts_with(&original));
        assert_eq!(complete_managed_blocks(&installed).len(), 1);
        assert!(remove_managed_block(&installed).starts_with(&original));
    }

    #[test]
    fn reinstall_replaces_first_complete_block_and_removes_duplicates() {
        let desired = startup_reference(RemoteShellKind::Fish);
        let old = format!("# {MANAGED_BLOCK_START}\nold\n# {MANAGED_BLOCK_END}\n");
        let duplicate = format!("head\n{old}middle\n{old}tail\n");
        let installed = install_managed_block(&duplicate, &desired);
        assert_eq!(complete_managed_blocks(&installed).len(), 1);
        assert!(installed.contains("head\n"));
        assert!(installed.contains("middle\n"));
        assert!(installed.contains("tail\n"));
    }

    #[test]
    fn version_three_managed_block_upgrades_in_place() {
        let old_block = format!(
            "# {MANAGED_BLOCK_START}\n# oxideterm-shell-integration-version: 3\nlegacy source\n# {MANAGED_BLOCK_END}"
        );
        let original = format!("before\n{old_block}\nafter\n");
        let upgraded = install_managed_block(&original, &startup_reference(RemoteShellKind::Bash));

        assert!(upgraded.starts_with("before\n"));
        assert!(upgraded.ends_with("after\n"));
        assert!(upgraded.contains("oxideterm-shell-integration-version: 4"));
        assert!(!upgraded.contains("legacy source"));
        assert_eq!(complete_managed_blocks(&upgraded).len(), 1);
    }

    #[test]
    fn remote_package_contains_exact_editor_adapter_sources() {
        let files = integration_files();
        assert!(files.contains(&("oxideterm-free-type.vim", VIM_FREE_TYPE_INTEGRATION_SOURCE)));
        assert!(files.contains(&("oxideterm-free-type.el", EMACS_FREE_TYPE_INTEGRATION_SOURCE)));
        assert!(REMOTE_INTEGRATION_README.contains("optional Free Type Mode adapters"));
    }

    #[test]
    fn shell_config_paths_honor_zdotdir_and_xdg_config_home() {
        let mut env = RemoteEnvInfo::unknown();
        env.os_type = "Linux".to_string();
        env.zdotdir = Some("/home/alice/.config/zsh".to_string());
        env.xdg_config_home = Some("/home/alice/.config-custom".to_string());
        assert_eq!(
            startup_file_path(RemoteShellKind::Zsh, &env, "/home/alice"),
            "/home/alice/.config/zsh/.zshrc"
        );
        assert_eq!(
            startup_file_path(RemoteShellKind::Fish, &env, "/home/alice"),
            "/home/alice/.config-custom/fish/config.fish"
        );
    }

    #[test]
    fn windows_powershell_profiles_follow_the_detected_shell_family() {
        let mut env = RemoteEnvInfo::unknown();
        env.os_type = "Windows".to_string();
        env.shell = Some("PowerShell 5.1".to_string());
        assert_eq!(
            startup_file_path(RemoteShellKind::PowerShell, &env, "C:/Users/alice"),
            "C:/Users/alice/Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1"
        );
        env.shell = Some("pwsh 7.5".to_string());
        assert_eq!(
            startup_file_path(RemoteShellKind::PowerShell, &env, "C:/Users/alice"),
            "C:/Users/alice/Documents/PowerShell/Microsoft.PowerShell_profile.ps1"
        );
    }

    #[test]
    fn powershell_v4_reload_bypasses_the_v3_prompt_wrapper() {
        let script = format!(
            r#"
$script:__oxideterm_original_prompt = {{ 'base-prompt' }}
$script:legacyCalls = 0
$global:__oxideterm_shell_integration_v3 = $true
function global:prompt {{
    $script:legacyCalls += 1
    if ($script:legacyCalls -gt 2) {{ throw 'recursive prompt wrapper' }}
    & $script:__oxideterm_original_prompt
}}
{POWERSHELL_INTEGRATION}
prompt
"#
        );
        let output = match std::process::Command::new("pwsh")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &script,
            ])
            .output()
        {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => panic!("failed to execute PowerShell integration test: {error}"),
        };

        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("base-prompt"));
        assert_eq!(
            output
                .stdout
                .windows(b"]7;file://".len())
                .filter(|window| *window == b"]7;file://")
                .count(),
            1
        );
    }

    #[cfg(unix)]
    #[test]
    fn bash_source_exposes_private_adapters_only_for_marked_sessions() {
        let unmarked_script = format!(
            "unset LC_OXIDETERM_SESSION OXIDETERM_PRIVATE_OSC OXIDETERM_VIM_INTEGRATION OXIDETERM_EMACS_INTEGRATION\n{BASH_INTEGRATION}\nprintf '%s|%s|%s' \"${{OXIDETERM_PRIVATE_OSC-}}\" \"${{OXIDETERM_VIM_INTEGRATION-}}\" \"${{LC_OXIDETERM_SESSION-}}\""
        );
        let unmarked = std::process::Command::new("bash")
            .args(["--noprofile", "--norc", "-c", &unmarked_script])
            .output()
            .expect("Bash should be available for shell integration tests");
        assert!(unmarked.status.success());
        assert_eq!(String::from_utf8_lossy(&unmarked.stdout), "||");

        let marked_script = format!(
            "HOME=/home/alice\nLC_OXIDETERM_SESSION=1\n{BASH_INTEGRATION}\nprintf '%s|%s|%s' \"${{OXIDETERM_PRIVATE_OSC-}}\" \"${{OXIDETERM_VIM_INTEGRATION-}}\" \"${{LC_OXIDETERM_SESSION-}}\""
        );
        let marked = std::process::Command::new("bash")
            .args(["--noprofile", "--norc", "-c", &marked_script])
            .output()
            .expect("Bash should be available for shell integration tests");
        assert!(marked.status.success());
        assert_eq!(
            String::from_utf8_lossy(&marked.stdout),
            "1|/home/alice/.oxideterm/shell-integration/oxideterm-free-type.vim|"
        );
    }

    #[cfg(unix)]
    #[test]
    fn bash_source_emits_standard_osc7_with_encoded_path() {
        let script = format!(
            "HOSTNAME='build host'\n{BASH_INTEGRATION}\ncd /tmp\n__oxideterm_emit_remote_metadata"
        );
        let output = std::process::Command::new("bash")
            .args(["--noprofile", "--norc", "-c", &script])
            .output()
            .expect("Bash should be available for shell integration tests");

        assert!(output.status.success());
        assert!(output.stdout.starts_with(b"\x1b]7;file://buildhost/"));
        assert!(output.stdout.ends_with(b"\x07"));
        assert!(output.stdout.contains(&b'%'));
        assert!(!output.stdout.windows(5).any(|window| window == b"7719;"));
    }

    #[cfg(unix)]
    #[test]
    fn bash_source_keeps_existing_prompt_commands_when_executed() {
        let scalar_script = format!(
            "PROMPT_COMMAND='existing-command'\n{BASH_INTEGRATION}\nprintf '%s' \"$PROMPT_COMMAND\""
        );
        let scalar = std::process::Command::new("bash")
            .args(["--noprofile", "--norc", "-c", &scalar_script])
            .output()
            .expect("Bash should be available for Shell integration tests");
        assert!(scalar.status.success());
        assert_eq!(
            String::from_utf8_lossy(&scalar.stdout),
            "__oxideterm_prompt_hook;existing-command"
        );

        let array_script = format!(
            "PROMPT_COMMAND=(first-command second-command)\n{BASH_INTEGRATION}\nprintf '%s\\n' \"${{PROMPT_COMMAND[@]}}\""
        );
        let array = std::process::Command::new("bash")
            .args(["--noprofile", "--norc", "-c", &array_script])
            .output()
            .expect("Bash should be available for Shell integration tests");
        assert!(array.status.success());
        assert_eq!(
            String::from_utf8_lossy(&array.stdout),
            "first-command\nsecond-command\n__oxideterm_prompt_hook\n"
        );
    }
}
