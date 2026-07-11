pub struct SshSessionConfig {
    config: SshConfig,
    registry: Option<SshConnectionRegistry>,
    consumer: Option<ConnectionConsumer>,
    prompt_handler: Option<Arc<dyn SshPromptHandler>>,
    managed_key_resolver: Option<ManagedKeyResolver>,
    trzsz_policy: Option<TrzszTransferPolicy>,
    runtime_handle: Option<tokio::runtime::Handle>,
    defer_pty_until_resize: bool,
    post_connect_command: Option<String>,
    remote_metadata_token: Option<String>,
}

const POST_CONNECT_COMMAND_MAX_BYTES: usize = 8192;

impl SshSessionConfig {
    pub fn new(host: impl Into<String>, port: u16, username: impl Into<String>) -> Self {
        Self {
            config: SshConfig::password(host, port, username, ""),
            registry: None,
            consumer: None,
            prompt_handler: None,
            managed_key_resolver: None,
            trzsz_policy: None,
            runtime_handle: None,
            defer_pty_until_resize: false,
            post_connect_command: None,
            remote_metadata_token: None,
        }
    }

    pub fn host(&self) -> &str {
        &self.config.host
    }

    pub fn port(&self) -> u16 {
        self.config.port
    }

    pub fn username(&self) -> &str {
        &self.config.username
    }

    pub fn with_registry(
        mut self,
        registry: SshConnectionRegistry,
        consumer: ConnectionConsumer,
    ) -> Self {
        self.registry = Some(registry);
        self.consumer = Some(consumer);
        self
    }

    pub fn with_prompt_handler(mut self, prompt_handler: Arc<dyn SshPromptHandler>) -> Self {
        self.prompt_handler = Some(prompt_handler);
        self
    }

    pub fn with_managed_key_resolver(mut self, resolver: ManagedKeyResolver) -> Self {
        self.managed_key_resolver = Some(resolver);
        self
    }

    pub fn with_trzsz_policy(mut self, policy: Option<TrzszTransferPolicy>) -> Self {
        self.trzsz_policy = policy;
        self
    }

    pub fn with_runtime_handle(mut self, handle: tokio::runtime::Handle) -> Self {
        self.runtime_handle = Some(handle);
        self
    }

    pub fn with_deferred_pty(mut self, defer_pty_until_resize: bool) -> Self {
        self.defer_pty_until_resize = defer_pty_until_resize;
        self
    }

    pub fn with_post_connect_command(mut self, command: Option<String>) -> Self {
        self.post_connect_command = command.and_then(|command| {
            let command = command.trim().to_string();
            (!command.is_empty()).then_some(command)
        });
        self
    }

    pub fn with_remote_metadata_token(mut self, token: Option<String>) -> Self {
        self.remote_metadata_token = token.and_then(|token| {
            let token = token.trim().to_string();
            (!token.is_empty()).then_some(token)
        });
        self
    }

    pub fn defer_pty_until_resize(&self) -> bool {
        self.defer_pty_until_resize
    }

    pub fn trzsz_policy(&self) -> Option<TrzszTransferPolicy> {
        self.trzsz_policy.clone()
    }

    pub fn post_connect_command(&self) -> Option<&str> {
        self.post_connect_command.as_deref()
    }

    pub fn post_connect_input(&self) -> Result<Option<Vec<u8>>, String> {
        normalize_post_connect_command(self.post_connect_command.as_deref())
    }

    pub fn remote_metadata_token(&self) -> Option<&str> {
        self.remote_metadata_token.as_deref()
    }

    pub fn remote_metadata_startup_input(&self) -> Option<String> {
        self.remote_metadata_token
            .as_deref()
            .map(build_remote_metadata_startup_input)
    }
}

impl From<oxideterm_ssh::SshConfig> for SshSessionConfig {
    fn from(config: oxideterm_ssh::SshConfig) -> Self {
        Self {
            post_connect_command: config.post_connect_command.clone(),
            config,
            registry: None,
            consumer: None,
            prompt_handler: None,
            managed_key_resolver: None,
            trzsz_policy: None,
            runtime_handle: None,
            defer_pty_until_resize: false,
            remote_metadata_token: None,
        }
    }
}

fn build_remote_metadata_startup_input(token: &str) -> String {
    let token = shell_single_quote(token);
    let bash_rc = shell_printf_argument(&remote_bash_metadata_rc());
    let zsh_rc = shell_printf_argument(&remote_zsh_metadata_rc());
    let fish_rc = shell_printf_argument(&remote_fish_metadata_rc());
    let nushell_config = shell_printf_argument(&remote_nushell_metadata_config());
    let powershell_profile = shell_printf_argument(&remote_powershell_metadata_profile());

    let script = format!(
        "__oxide_shell=${{SHELL:-/bin/sh}}; \
__oxide_base=${{__oxide_shell##*/}}; \
case \"$__oxide_base\" in \
bash) __oxide_rc=\"${{TMPDIR:-/tmp}}/.oxideterm-bashrc-$$\"; umask 077; printf '%s\\n' {bash_rc} > \"$__oxide_rc\" && OXIDETERM_REMOTE_METADATA_ID={token} OXIDETERM_BOOTSTRAP_RC=\"$__oxide_rc\" exec bash --rcfile \"$__oxide_rc\" -i; exec \"$__oxide_shell\" -i ;; \
zsh) __oxide_dir=\"${{TMPDIR:-/tmp}}/.oxideterm-zdot-$$\"; mkdir -m 700 \"$__oxide_dir\" 2>/dev/null || __oxide_dir=\"\"; if [ -n \"$__oxide_dir\" ]; then printf '%s\\n' {zsh_rc} > \"$__oxide_dir/.zshrc\" && OXIDETERM_REMOTE_METADATA_ID={token} OXIDETERM_BOOTSTRAP_ZDOT=\"$__oxide_dir\" ZDOTDIR=\"$__oxide_dir\" exec zsh -i; fi; exec \"$__oxide_shell\" -i ;; \
fish) __oxide_rc=\"${{TMPDIR:-/tmp}}/.oxideterm-fish-$$.fish\"; umask 077; printf '%s\\n' {fish_rc} > \"$__oxide_rc\" && OXIDETERM_REMOTE_METADATA_ID={token} OXIDETERM_BOOTSTRAP_FISH=\"$__oxide_rc\" exec fish --init-command \"source $__oxide_rc\" -i; exec \"$__oxide_shell\" -i ;; \
nu|nushell) __oxide_config=\"${{TMPDIR:-/tmp}}/.oxideterm-nu-config-$$.nu\"; umask 077; : > \"$__oxide_config\"; if [ -r \"$HOME/.config/nushell/config.nu\" ]; then printf '%s\\n' 'source ~/.config/nushell/config.nu' > \"$__oxide_config\"; elif [ -r \"$HOME/Library/Application Support/nushell/config.nu\" ]; then printf '%s\\n' 'source \"~/Library/Application Support/nushell/config.nu\"' > \"$__oxide_config\"; fi; printf '%s\\n' {nushell_config} >> \"$__oxide_config\" && OXIDETERM_REMOTE_METADATA_ID={token} OXIDETERM_BOOTSTRAP_NU_CONFIG=\"$__oxide_config\" exec \"$__oxide_shell\" --config \"$__oxide_config\"; exec \"$__oxide_shell\" ;; \
pwsh|powershell) __oxide_profile=\"${{TMPDIR:-/tmp}}/.oxideterm-pwsh-$$.ps1\"; umask 077; printf '%s\\n' {powershell_profile} > \"$__oxide_profile\" && OXIDETERM_REMOTE_METADATA_ID={token} OXIDETERM_BOOTSTRAP_PWSH=\"$__oxide_profile\" exec \"$__oxide_shell\" -NoLogo -NoExit -File \"$__oxide_profile\"; exec \"$__oxide_shell\" ;; \
*) exec \"$__oxide_shell\" -i ;; \
esac"
    );
    // The SSH transport requests the shell with PTY echo disabled, sends this
    // line as hidden bootstrap input, and then lets the line restore echo
    // before replacing the login shell with an integrated interactive shell.
    format!(" stty echo; /bin/sh -lc {}; exit\r", shell_single_quote(&script))
}

fn remote_bash_metadata_rc() -> String {
    format!(
        r#"[ -r "$HOME/.bashrc" ] && . "$HOME/.bashrc"
{}
if [ -n "${{OXIDETERM_BOOTSTRAP_RC:-}}" ]; then rm -f "$OXIDETERM_BOOTSTRAP_RC"; unset OXIDETERM_BOOTSTRAP_RC; fi
__oxideterm_emit_remote_metadata
PROMPT_COMMAND="__oxideterm_emit_remote_metadata${{PROMPT_COMMAND:+;$PROMPT_COMMAND}}""#,
        remote_metadata_shell_functions()
    )
}

fn remote_zsh_metadata_rc() -> String {
    format!(
        r#"[ -r "$HOME/.zshrc" ] && . "$HOME/.zshrc"
{}
if [ -n "${{OXIDETERM_BOOTSTRAP_ZDOT:-}}" ]; then rm -f "$OXIDETERM_BOOTSTRAP_ZDOT/.zshrc"; rmdir "$OXIDETERM_BOOTSTRAP_ZDOT" 2>/dev/null; unset OXIDETERM_BOOTSTRAP_ZDOT; fi
__oxideterm_emit_remote_metadata
precmd_functions=(${{precmd_functions[@]}} __oxideterm_emit_remote_metadata)"#,
        remote_metadata_shell_functions()
    )
}

fn remote_fish_metadata_rc() -> String {
    r#"function __oxideterm_pct
    command printf '%s' "$argv[1]" | command od -An -tx1 -v | command tr -d ' \n' | command sed 's/../%&/g'
end
function __oxideterm_emit_remote_metadata --on-event fish_prompt
    test -n "$OXIDETERM_REMOTE_METADATA_ID"; or return
    set -l __oxideterm_cwd (pwd -P 2>/dev/null; or pwd 2>/dev/null)
    set -l __oxideterm_host "$HOSTNAME"
    test -n "$__oxideterm_host"; or set __oxideterm_host (hostname 2>/dev/null; or command printf '')
    command printf '\033]7719;v=1;id=%s;cwd=%s;host=%s\007' "$OXIDETERM_REMOTE_METADATA_ID" (__oxideterm_pct "$__oxideterm_cwd") (__oxideterm_pct "$__oxideterm_host")
end
if test -n "$OXIDETERM_BOOTSTRAP_FISH"
    command rm -f -- "$OXIDETERM_BOOTSTRAP_FISH"
    set -e OXIDETERM_BOOTSTRAP_FISH
end
__oxideterm_emit_remote_metadata"#
        .to_string()
}

fn remote_nushell_metadata_config() -> String {
    r#"def __oxideterm_pct [value: string] {
    ^printf '%s' $value | ^od -An -tx1 -v | ^tr -d ' \n' | ^sed 's/../%&/g'
}
def __oxideterm_emit_remote_metadata [] {
    if (($env.OXIDETERM_REMOTE_METADATA_ID? | default '') == '') { return }
    let __oxideterm_host = ($env.HOSTNAME? | default (^hostname | str trim))
    print --no-newline $"\u{1b}]7719;v=1;id=($env.OXIDETERM_REMOTE_METADATA_ID);cwd=(__oxideterm_pct (pwd));host=(__oxideterm_pct $__oxideterm_host)\u{07}"
}
$env.config = ($env.config | upsert hooks.pre_prompt (($env.config.hooks.pre_prompt? | default []) | append {|| __oxideterm_emit_remote_metadata }))
if (($env.OXIDETERM_BOOTSTRAP_NU_CONFIG? | default '') != '') {
    rm --force $env.OXIDETERM_BOOTSTRAP_NU_CONFIG
    hide-env OXIDETERM_BOOTSTRAP_NU_CONFIG
}
__oxideterm_emit_remote_metadata"#
        .to_string()
}

fn remote_powershell_metadata_profile() -> String {
    r#"$script:__oxideterm_original_prompt = $null
if (Test-Path Function:\prompt) {
    $script:__oxideterm_original_prompt = (Get-Command prompt).ScriptBlock
}
function global:__oxideterm_pct {
    param([string]$Value)
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($Value)
    $parts = foreach ($byte in $bytes) { '%' + $byte.ToString('x2') }
    -join $parts
}
function global:__oxideterm_emit_remote_metadata {
    $id = $env:OXIDETERM_REMOTE_METADATA_ID
    if ([string]::IsNullOrEmpty($id)) { return }
    $location = Get-Location
    $cwd = if ($location.ProviderPath) { $location.ProviderPath } else { $location.Path }
    $hostName = if ($env:HOSTNAME) { $env:HOSTNAME } elseif ($env:COMPUTERNAME) { $env:COMPUTERNAME } else { [System.Net.Dns]::GetHostName() }
    [Console]::Out.Write("`e]7719;v=1;id=$id;cwd=$(__oxideterm_pct $cwd);host=$(__oxideterm_pct $hostName)`a")
}
function global:prompt {
    __oxideterm_emit_remote_metadata
    if ($script:__oxideterm_original_prompt) {
        & $script:__oxideterm_original_prompt
    } else {
        "PS $($executionContext.SessionState.Path.CurrentLocation)$('>' * ($nestedPromptLevel + 1)) "
    }
}
if ($env:OXIDETERM_BOOTSTRAP_PWSH) {
    Remove-Item -Force $env:OXIDETERM_BOOTSTRAP_PWSH -ErrorAction SilentlyContinue
    Remove-Item Env:OXIDETERM_BOOTSTRAP_PWSH -ErrorAction SilentlyContinue
}
__oxideterm_emit_remote_metadata"#
        .to_string()
}

fn remote_metadata_shell_functions() -> &'static str {
    r#"__oxideterm_pct() {
  printf '%s' "$1" | od -An -tx1 -v | tr -d ' \n' | sed 's/../%&/g'
}
__oxideterm_emit_remote_metadata() {
  [ -n "${OXIDETERM_REMOTE_METADATA_ID:-}" ] || return
  __oxideterm_cwd=$(pwd -P 2>/dev/null || pwd 2>/dev/null) || return
  __oxideterm_host=${HOSTNAME:-$(hostname 2>/dev/null || printf '')}
  printf '\033]7719;v=1;id=%s;cwd=%s;host=%s\007' "$OXIDETERM_REMOTE_METADATA_ID" "$(__oxideterm_pct "$__oxideterm_cwd")" "$(__oxideterm_pct "$__oxideterm_host")"
}"#
}

fn shell_single_quote(value: &str) -> String {
    let mut quoted = String::from("'");
    for ch in value.chars() {
        if ch == '\'' {
            quoted.push_str("'\"'\"'");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

fn shell_printf_argument(value: &str) -> String {
    debug_assert!(
        !value.ends_with(['\n', '\r']),
        "shell profile payloads must not rely on trailing newlines"
    );
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            // Decode line breaks only after the interactive shell has parsed
            // the complete one-line bootstrap command with echo still off.
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    format!("\"$(printf '%b' {})\"", shell_single_quote(&escaped))
}

fn normalize_post_connect_command(command: Option<&str>) -> Result<Option<Vec<u8>>, String> {
    let Some(command) = command.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    // Tauri sends each logical line as an Enter key. Normalize all newline
    // variants to carriage returns before the SSH PTY receives the payload.
    let mut normalized = command.replace("\r\n", "\n").replace('\r', "\n");
    normalized = normalized.replace('\n', "\r");
    if !normalized.ends_with('\r') {
        normalized.push('\r');
    }

    let bytes = normalized.into_bytes();
    if bytes.len() > POST_CONNECT_COMMAND_MAX_BYTES {
        return Err(format!(
            "Post-connect command is too long (max {} bytes)",
            POST_CONNECT_COMMAND_MAX_BYTES
        ));
    }

    Ok(Some(bytes))
}

#[cfg(test)]
mod ssh_config_tests {
    use super::{
        SshSessionConfig, normalize_post_connect_command, remote_fish_metadata_rc,
        remote_nushell_metadata_config, remote_powershell_metadata_profile, shell_printf_argument,
    };
    use oxideterm_ssh::SshConfig;

    #[test]
    fn post_connect_command_trims_and_adds_enter_like_tauri() {
        assert_eq!(
            normalize_post_connect_command(Some("  cd /srv/app  ")).unwrap(),
            Some(b"cd /srv/app\r".to_vec())
        );
    }

    #[test]
    fn post_connect_command_converts_multiline_to_enter_keys_like_tauri() {
        assert_eq!(
            normalize_post_connect_command(Some("cd /srv/app\nls")).unwrap(),
            Some(b"cd /srv/app\rls\r".to_vec())
        );
    }

    #[test]
    fn post_connect_command_ignores_blank_values_like_tauri() {
        assert_eq!(normalize_post_connect_command(Some("   ")).unwrap(), None);
        assert_eq!(normalize_post_connect_command(None).unwrap(), None);
    }

    #[test]
    fn post_connect_override_can_clear_saved_node_command() {
        let config = SshConfig {
            post_connect_command: Some("cd /srv/app".to_string()),
            ..SshConfig::default()
        };

        let session_config = SshSessionConfig::from(config).with_post_connect_command(None);

        assert_eq!(session_config.post_connect_command(), None);
    }

    #[test]
    fn runtime_handle_is_optional_and_injectable() {
        let runtime = tokio::runtime::Runtime::new().unwrap();

        assert!(SshSessionConfig::new("example.com", 22, "alice")
            .runtime_handle
            .is_none());
        assert!(SshSessionConfig::new("example.com", 22, "alice")
            .with_runtime_handle(runtime.handle().clone())
            .runtime_handle
            .is_some());
    }

    #[test]
    fn remote_metadata_startup_input_includes_supported_shell_hooks() {
        let input = SshSessionConfig::new("example.com", 22, "alice")
            .with_remote_metadata_token(Some("token-1".to_string()))
            .remote_metadata_startup_input()
            .unwrap();

        assert!(input.starts_with(" stty echo; /bin/sh -lc "));
        assert!(input.contains("fish)"));
        assert!(input.contains("fish --init-command"));
        assert!(!input.contains('\n'));
        assert!(remote_fish_metadata_rc()
            .contains("function __oxideterm_emit_remote_metadata --on-event fish_prompt"));
        assert!(input.contains("nu|nushell)"));
        assert!(remote_nushell_metadata_config().contains("hooks.pre_prompt"));
        assert!(input.contains("pwsh|powershell)"));
        assert!(remote_powershell_metadata_profile().contains("function global:prompt"));
        assert!(input.contains("OXIDETERM_BOOTSTRAP_PWSH"));
        assert!(!input.contains("2>/dev/null; exec /bin/sh"));

        // Validate the exact nested quoting sent to the remote login shell.
        let syntax = std::process::Command::new("/bin/sh")
            .args(["-n", "-c", input.trim_end_matches('\r')])
            .output()
            .expect("local POSIX shell should validate startup input");
        assert!(
            syntax.status.success(),
            "startup input must be valid POSIX shell: {}",
            String::from_utf8_lossy(&syntax.stderr)
        );
    }

    #[test]
    fn shell_printf_argument_round_trips_multiline_profile_without_physical_newlines() {
        let profile = "first 'quoted' line\nsecond \\ path";
        let argument = shell_printf_argument(profile);
        assert!(!argument.contains('\n'));

        let output = std::process::Command::new("/bin/sh")
            .args(["-c", &format!("printf '%s' {argument}")])
            .output()
            .expect("local POSIX shell should decode profile argument");
        assert!(output.status.success());
        assert_eq!(output.stdout, profile.as_bytes());
    }
}

impl std::fmt::Debug for SshSessionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshSessionConfig")
            .field("config", &self.config)
            .field("registry", &self.registry)
            .field("consumer", &self.consumer)
            .field("prompt_handler", &self.prompt_handler.is_some())
            .field("managed_key_resolver", &self.managed_key_resolver.is_some())
            .field("trzsz_policy", &self.trzsz_policy)
            .field("runtime_handle", &self.runtime_handle.is_some())
            .field("defer_pty_until_resize", &self.defer_pty_until_resize)
            .field("post_connect_command", &self.post_connect_command.is_some())
            .field("remote_metadata_token", &self.remote_metadata_token.is_some())
            .finish()
    }
}
