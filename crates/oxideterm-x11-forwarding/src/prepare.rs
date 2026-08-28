// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{fmt, path::Path, process::Stdio, time::Duration};

#[cfg(target_os = "macos")]
use std::path::PathBuf;

use tempfile::TempDir;
use tokio::{io::AsyncReadExt, process::Command, time::timeout};
use zeroize::Zeroizing;

use crate::{
    X11AuthCommand, X11AuthorityEnvironment, X11AuthorityFile, X11ForwardConfig, X11ForwardPlan,
    X11ForwardPolicy, X11ForwardTrust, X11ForwardingError, X11LocalEndpoint, X11Result,
    X11SshRequest, parse_xauth_list,
};

const XAUTH_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const XAUTH_OUTPUT_LIMIT_BYTES: usize = 256 * 1024;
const XAUTH_EXPIRY_GRACE_SECONDS: u64 = 60;
const MAX_XAUTH_TIMEOUT_SECONDS: u64 = u32::MAX as u64;

pub struct X11PreparedForwarding {
    pub endpoint: X11LocalEndpoint,
    pub auth: crate::X11AuthMaterial,
    pub request: X11SshRequest,
    pub acceptance_timeout: Option<Duration>,
}

impl fmt::Debug for X11PreparedForwarding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("X11PreparedForwarding")
            .field("endpoint", &"<device-local endpoint>")
            .field("auth", &"<redacted>")
            .field("request", &self.request)
            .field("acceptance_timeout", &self.acceptance_timeout)
            .finish()
    }
}

/// Resolves device-local X11 authority for one terminal without persisting cookies.
pub async fn prepare_x11_forwarding(policy: X11ForwardPolicy) -> X11Result<X11PreparedForwarding> {
    let display_value = resolve_process_display().await?;
    let environment =
        X11AuthorityEnvironment::from_values(Some(display_value), std::env::var("XAUTHORITY").ok());
    let display = environment.parse_display()?;
    let endpoint = display.local_endpoint()?;
    let config = X11ForwardConfig::new(display).with_policy(policy);
    let plan = match policy.trust {
        X11ForwardTrust::Trusted => prepare_trusted_plan(config, &environment).await?,
        X11ForwardTrust::Untrusted => prepare_untrusted_plan(config).await?,
    };
    let acceptance_timeout = policy.timeout_millis.map(Duration::from_millis);
    Ok(X11PreparedForwarding {
        endpoint,
        request: plan.ssh_request(),
        auth: plan.auth,
        acceptance_timeout,
    })
}

async fn prepare_trusted_plan(
    config: X11ForwardConfig,
    environment: &X11AuthorityEnvironment,
) -> X11Result<X11ForwardPlan> {
    let resolver = crate::X11LocalAuthorityResolver::new(
        environment.clone(),
        crate::X11AuthorityMatchContext::new(),
    );
    if let Ok(path) = resolver.authority_path()
        && let Ok(bytes) = tokio::fs::read(path).await.map(Zeroizing::new)
        && let Ok(entries) = crate::parse_xauthority_file(bytes.as_slice())
        && let Ok(plan) = X11ForwardPlan::from_binary_authority_entries(
            config.clone(),
            &entries,
            &resolver.context,
        )
    {
        return Ok(plan);
    }

    let output = run_xauth(environment.xauth_list_command()?).await?;
    let text = std::str::from_utf8(output.as_slice())
        .map_err(|error| X11ForwardingError::XauthFailed(error.to_string()))?;
    X11ForwardPlan::from_xauth_entries(config, &parse_xauth_list(text)?)
}

async fn prepare_untrusted_plan(config: X11ForwardConfig) -> X11Result<X11ForwardPlan> {
    let temp_dir = secure_xauth_temp_dir()?;
    let authority_path = temp_dir.path().join("authority");
    let expiry_seconds = xauth_expiry_seconds(config.policy.timeout_millis);
    let program = xauth_program();
    run_xauth(X11AuthCommand {
        program: program.clone(),
        args: untrusted_generate_args(&authority_path, &config.local_display, expiry_seconds),
    })
    .await?;
    #[cfg(unix)]
    tokio::fs::set_permissions(
        &authority_path,
        std::os::unix::fs::PermissionsExt::from_mode(0o600),
    )
    .await
    .map_err(|error| X11ForwardingError::AuthorityFileUnavailable(error.to_string()))?;
    let output = run_xauth(X11AuthCommand {
        program,
        args: X11AuthCommand::list(
            &config.local_display,
            X11AuthorityFile::Path(authority_path.to_string_lossy().into_owned()),
        )
        .args,
    })
    .await?;
    let text = std::str::from_utf8(output.as_slice())
        .map_err(|error| X11ForwardingError::XauthFailed(error.to_string()))?;
    // The temporary file is removed on return; the X server retains its timed authorization.
    let plan = X11ForwardPlan::from_xauth_entries(config, &parse_xauth_list(text)?)?;
    drop(temp_dir);
    Ok(plan)
}

fn secure_xauth_temp_dir() -> X11Result<TempDir> {
    tempfile::Builder::new()
        .prefix("oxideterm-x11-")
        .tempdir()
        .map_err(|error| X11ForwardingError::AuthorityFileUnavailable(error.to_string()))
}

fn xauth_expiry_seconds(timeout_millis: Option<u64>) -> Option<u64> {
    timeout_millis.map(|millis| {
        // Keep the local authorization alive slightly longer than route admission,
        // and clamp to the X SECURITY extension's 32-bit timeout field.
        millis
            .div_ceil(1_000)
            .max(1)
            .saturating_add(XAUTH_EXPIRY_GRACE_SECONDS)
            .min(MAX_XAUTH_TIMEOUT_SECONDS)
    })
}

fn untrusted_generate_args(
    authority_path: &Path,
    display: &crate::X11Display,
    timeout_seconds: Option<u64>,
) -> Vec<String> {
    let mut args = vec![
        "-f".to_string(),
        authority_path.to_string_lossy().into_owned(),
        "generate".to_string(),
        display.xauth_query_display(),
        "MIT-MAGIC-COOKIE-1".to_string(),
        "untrusted".to_string(),
    ];
    if let Some(timeout_seconds) = timeout_seconds {
        args.extend(["timeout".to_string(), timeout_seconds.to_string()]);
    }
    args
}

async fn resolve_process_display() -> X11Result<String> {
    if let Ok(display) = std::env::var("DISPLAY")
        && !display.trim().is_empty()
    {
        return Ok(display.trim().to_string());
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("/bin/launchctl");
        command.args(["getenv", "DISPLAY"]);
        let output = run_bounded_command(command).await?;
        let display = std::str::from_utf8(output.as_slice())
            .map_err(|error| X11ForwardingError::XauthFailed(error.to_string()))?
            .trim();
        if !display.is_empty() {
            return Ok(display.to_string());
        }
    }
    Err(X11ForwardingError::MissingDisplay)
}

fn xauth_program() -> String {
    #[cfg(target_os = "macos")]
    {
        let xquartz_xauth = PathBuf::from("/opt/X11/bin/xauth");
        if xquartz_xauth.is_file() {
            return xquartz_xauth.to_string_lossy().into_owned();
        }
    }
    "xauth".to_string()
}

async fn run_xauth(mut command: X11AuthCommand) -> X11Result<Zeroizing<Vec<u8>>> {
    if command.program == "xauth" {
        command.program = xauth_program();
    }
    let mut process = Command::new(&command.program);
    process.args(&command.args);
    run_bounded_command(process).await
}

async fn run_bounded_command(mut command: Command) -> X11Result<Zeroizing<Vec<u8>>> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|error| X11ForwardingError::XauthUnavailable(error.to_string()))?;
    let stdout = child.stdout.take().ok_or_else(|| {
        X11ForwardingError::XauthFailed("xauth stdout was not captured".to_string())
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        X11ForwardingError::XauthFailed("xauth stderr was not captured".to_string())
    })?;
    let stdout_task = tokio::spawn(read_bounded_output(stdout));
    let stderr_task = tokio::spawn(read_bounded_output(stderr));
    let status = match timeout(XAUTH_COMMAND_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => {
            stdout_task.abort();
            stderr_task.abort();
            return Err(X11ForwardingError::XauthFailed(error.to_string()));
        }
        Err(_) => {
            // The child is kill-on-drop; abort its bounded pipe readers as one task group.
            stdout_task.abort();
            stderr_task.abort();
            return Err(X11ForwardingError::XauthTimedOut);
        }
    };
    let output = stdout_task
        .await
        .map_err(|error| X11ForwardingError::XauthFailed(error.to_string()))??;
    let _error_output = stderr_task
        .await
        .map_err(|error| X11ForwardingError::XauthFailed(error.to_string()))??;
    if !status.success() {
        // Treat command output as sensitive because xauth may include authority records.
        return Err(X11ForwardingError::XauthFailed(format!(
            "process exited with {status}"
        )));
    }
    Ok(output)
}

async fn read_bounded_output<R>(reader: R) -> X11Result<Zeroizing<Vec<u8>>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut output = Zeroizing::new(Vec::new());
    reader
        .take((XAUTH_OUTPUT_LIMIT_BYTES + 1) as u64)
        .read_to_end(&mut output)
        .await
        .map_err(|error| X11ForwardingError::XauthFailed(error.to_string()))?;
    if output.len() > XAUTH_OUTPUT_LIMIT_BYTES {
        return Err(X11ForwardingError::XauthOutputTooLarge(
            XAUTH_OUTPUT_LIMIT_BYTES,
        ));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_generation_uses_temporary_authority_and_grace_timeout() {
        let display = crate::X11Display::parse(":0").unwrap();
        let args = untrusted_generate_args(Path::new("/private/tmp/auth"), &display, Some(1_260));

        assert_eq!(
            args,
            [
                "-f",
                "/private/tmp/auth",
                "generate",
                ":0",
                "MIT-MAGIC-COOKIE-1",
                "untrusted",
                "timeout",
                "1260",
            ]
        );
    }

    #[test]
    fn untrusted_generation_can_follow_connection_lifetime() {
        let display = crate::X11Display::parse(":0").unwrap();
        let args = untrusted_generate_args(Path::new("/private/tmp/auth"), &display, None);

        assert!(!args.iter().any(|argument| argument == "timeout"));
    }

    #[test]
    fn untrusted_expiry_adds_grace_without_overflowing_x_security_timeout() {
        assert_eq!(xauth_expiry_seconds(Some(1_200_000)), Some(1_260));
        assert_eq!(
            xauth_expiry_seconds(Some(u64::MAX)),
            Some(MAX_XAUTH_TIMEOUT_SECONDS)
        );
        assert_eq!(xauth_expiry_seconds(None), None);
    }

    #[test]
    fn prepared_forwarding_debug_redacts_both_cookies() {
        let auth = crate::X11AuthMaterial::with_fake_cookie(
            crate::X11AuthCookie::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
            crate::X11AuthCookie::from_hex("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap(),
        );
        let config = X11ForwardConfig::new(crate::X11Display::parse(":0").unwrap());
        let prepared = X11PreparedForwarding {
            endpoint: crate::X11LocalEndpoint::unix_socket_for_display(0),
            request: config.ssh_request(&auth),
            auth,
            acceptance_timeout: Some(Duration::from_secs(60)),
        };

        let debug = format!("{prepared:?}");
        assert!(!debug.contains("aaaaaaaa"));
        assert!(!debug.contains("bbbbbbbb"));
        assert!(!debug.contains("X11-unix"));
    }
}
