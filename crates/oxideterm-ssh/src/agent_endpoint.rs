#[cfg(unix)]
use std::path::{Path, PathBuf};

const SSH_AUTH_SOCK_ENV: &str = "SSH_AUTH_SOCK";
#[cfg(windows)]
const WINDOWS_OPENSSH_AGENT_PIPE: &str = r"\\.\pipe\openssh-ssh-agent";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SshAgentEndpoint {
    #[cfg(unix)]
    UnixSocket(PathBuf),
    #[cfg(windows)]
    WindowsNamedPipe(String),
}

impl SshAgentEndpoint {
    pub(crate) fn pool_identity(&self) -> String {
        match self {
            #[cfg(unix)]
            Self::UnixSocket(path) => format!("unix:{}", path.to_string_lossy()),
            #[cfg(windows)]
            Self::WindowsNamedPipe(pipe) => format!("windows:{pipe}"),
        }
    }
}

fn configured_environment_variable(value: &str) -> Option<&str> {
    value
        .strip_prefix("${")
        .and_then(|value| value.strip_suffix('}'))
        .or_else(|| value.strip_prefix('$'))
}

#[cfg(unix)]
fn unix_socket_exists(path: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt;

    std::fs::metadata(path)
        .map(|metadata| metadata.file_type().is_socket())
        .unwrap_or(false)
}

#[cfg(windows)]
fn windows_named_pipe_available(pipe: &str) -> bool {
    use windows::{
        Win32::{
            Foundation::{ERROR_SEM_TIMEOUT, GetLastError},
            System::Pipes::WaitNamedPipeW,
        },
        core::PCWSTR,
    };

    let pipe = pipe
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // A zero-timeout wait is non-blocking. ERROR_SEM_TIMEOUT still proves
    // the pipe exists but all instances are currently busy.
    let available = unsafe { WaitNamedPipeW(PCWSTR(pipe.as_ptr()), 0).as_bool() };
    available || unsafe { GetLastError() } == ERROR_SEM_TIMEOUT
}

#[cfg(target_os = "macos")]
fn macos_launchd_agent_socket() -> Option<PathBuf> {
    let output = std::process::Command::new("/bin/launchctl")
        .args(["getenv", SSH_AUTH_SOCK_ENV])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let socket = String::from_utf8(output.stdout).ok()?;
    let socket = PathBuf::from(socket.trim());
    unix_socket_exists(&socket).then_some(socket)
}

#[cfg(target_os = "macos")]
fn macos_known_agent_socket() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let candidates = [
        home.join("Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock"),
        home.join("Library/Containers/com.bitwarden.desktop/Data/.bitwarden-ssh-agent.sock"),
        home.join(".bitwarden-ssh-agent.sock"),
        home.join("Library/Containers/com.maxgoedjen.Secretive.SecretAgent/Data/socket.ssh"),
        home.join(".orbstack/ssh-agent.sock"),
    ];
    let mut available = candidates
        .into_iter()
        .filter(|path| unix_socket_exists(path));
    let selected = available.next()?;
    // Do not silently choose between multiple third-party agents. Users can
    // disambiguate them with IdentityAgent in their OpenSSH configuration.
    available.next().is_none().then_some(selected)
}

#[cfg(unix)]
fn default_unix_agent_socket() -> Option<PathBuf> {
    if let Some(socket) = std::env::var_os(SSH_AUTH_SOCK_ENV).map(PathBuf::from)
        && unix_socket_exists(&socket)
    {
        return Some(socket);
    }

    #[cfg(target_os = "macos")]
    {
        macos_launchd_agent_socket().or_else(macos_known_agent_socket)
    }

    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn resolve_ssh_agent_endpoint_with_policy(
    configured: Option<&str>,
    none_disables_agent: bool,
) -> Result<SshAgentEndpoint, String> {
    if none_disables_agent && configured.is_some_and(|value| value.eq_ignore_ascii_case("none")) {
        return Err("SSH agent is disabled by IdentityAgent".to_string());
    }

    #[cfg(unix)]
    {
        let socket = match configured {
            None => default_unix_agent_socket(),
            Some(value) if value == SSH_AUTH_SOCK_ENV => {
                std::env::var_os(SSH_AUTH_SOCK_ENV).map(PathBuf::from)
            }
            Some(value) => configured_environment_variable(value)
                .map(|variable| std::env::var_os(variable).map(PathBuf::from))
                .unwrap_or_else(|| Some(PathBuf::from(value))),
        }
        .ok_or_else(|| "SSH agent socket is not configured".to_string())?;
        if !unix_socket_exists(&socket) {
            return Err("SSH agent socket is unavailable".to_string());
        }
        return Ok(SshAgentEndpoint::UnixSocket(socket));
    }

    #[cfg(windows)]
    {
        let pipe = match configured {
            None => Some(WINDOWS_OPENSSH_AGENT_PIPE.to_string()),
            Some(value) if value == SSH_AUTH_SOCK_ENV => std::env::var(SSH_AUTH_SOCK_ENV).ok(),
            Some(value) => configured_environment_variable(value)
                .map(|variable| std::env::var(variable).ok())
                .unwrap_or_else(|| Some(value.to_string())),
        }
        .filter(|pipe| !pipe.trim().is_empty())
        .ok_or_else(|| "SSH agent named pipe is not configured".to_string())?;
        if !windows_named_pipe_available(&pipe) {
            return Err("SSH agent named pipe is unavailable".to_string());
        }
        return Ok(SshAgentEndpoint::WindowsNamedPipe(pipe));
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = configured;
        Err("SSH Agent is not supported on this platform".to_string())
    }
}

pub(crate) fn resolve_ssh_agent_endpoint(
    configured: Option<&str>,
) -> Result<SshAgentEndpoint, String> {
    // IdentityAgent uses the OpenSSH-specific "none" value to disable agent
    // authentication and forwarding through that identity endpoint.
    resolve_ssh_agent_endpoint_with_policy(configured, true)
}

pub(crate) fn resolve_ssh_agent_forwarding_endpoint(
    configured: Option<&str>,
) -> Result<SshAgentEndpoint, String> {
    // ForwardAgent accepts a socket path, so the literal path "none" must not
    // inherit IdentityAgent's special disable semantics.
    resolve_ssh_agent_endpoint_with_policy(configured, false)
}

pub(crate) fn ssh_agent_endpoint_pool_identity(configured: Option<&str>) -> String {
    resolve_ssh_agent_endpoint(configured)
        .map(|endpoint| endpoint.pool_identity())
        .unwrap_or_else(|_| format!("unavailable:{}", configured.unwrap_or("default")))
}

pub(crate) fn ssh_agent_forwarding_endpoint_pool_identity(configured: Option<&str>) -> String {
    resolve_ssh_agent_forwarding_endpoint(configured)
        .map(|endpoint| endpoint.pool_identity())
        .unwrap_or_else(|_| format!("unavailable:{}", configured.unwrap_or("default")))
}

/// Returns an inexpensive availability hint for the connection form.
pub fn ssh_agent_available(configured: Option<&str>) -> Option<bool> {
    #[cfg(unix)]
    {
        return Some(resolve_ssh_agent_endpoint(configured).is_ok());
    }

    #[cfg(windows)]
    {
        return Some(resolve_ssh_agent_endpoint(configured).is_ok());
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = configured;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{
        configured_environment_variable, resolve_ssh_agent_endpoint,
        resolve_ssh_agent_forwarding_endpoint,
    };

    #[test]
    fn identity_agent_environment_selectors_are_parsed_without_a_shell() {
        assert_eq!(
            configured_environment_variable("$CUSTOM_AGENT"),
            Some("CUSTOM_AGENT")
        );
        assert_eq!(
            configured_environment_variable("${CUSTOM_AGENT}"),
            Some("CUSTOM_AGENT")
        );
        assert_eq!(configured_environment_variable("/tmp/agent.sock"), None);
    }

    #[cfg(unix)]
    #[test]
    fn missing_explicit_socket_does_not_fall_back_to_default_agent() {
        let missing =
            std::env::temp_dir().join(format!("oxideterm-missing-agent-{}", std::process::id()));

        let error =
            resolve_ssh_agent_endpoint(missing.to_str()).expect_err("missing socket must fail");

        assert!(error.contains("unavailable"));
    }

    #[test]
    fn none_only_disables_identity_agent() {
        let identity_error =
            resolve_ssh_agent_endpoint(Some("none")).expect_err("IdentityAgent none must disable");
        let forwarding_error = resolve_ssh_agent_forwarding_endpoint(Some("none"))
            .expect_err("the literal forwarding socket should be unavailable in this test");

        assert!(identity_error.contains("disabled"));
        assert!(!forwarding_error.contains("disabled"));
    }
}
