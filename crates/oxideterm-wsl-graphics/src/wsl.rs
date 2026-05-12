// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[cfg(target_os = "windows")]
use std::time::Duration;
#[cfg(target_os = "windows")]
use tokio::io::AsyncReadExt;
#[cfg(target_os = "windows")]
use tokio::net::TcpStream;
use tokio::process::Child;
#[cfg(target_os = "windows")]
use tokio::time::{sleep, timeout};

use crate::{PrerequisiteResult, WslDistro, WslGraphicsError};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn list_distros() -> Result<Vec<WslDistro>, WslGraphicsError> {
    list_distros_impl()
}

pub fn launch_distro(distro: &str) -> Result<(), WslGraphicsError> {
    launch_distro_impl(distro)
}

pub fn check_prerequisites(distro: &str) -> Result<PrerequisiteResult, WslGraphicsError> {
    check_prerequisites_impl(distro)
}

pub async fn check_prerequisites_async(
    distro: &str,
) -> Result<PrerequisiteResult, WslGraphicsError> {
    check_prerequisites_async_impl(distro).await
}

pub async fn check_vnc_available(distro: &str) -> Result<(), WslGraphicsError> {
    check_vnc_available_impl(distro).await
}

pub async fn start_session(
    distro: &str,
    desktop_cmd: &str,
    dbus_cmd: &str,
    extra_env: &str,
) -> Result<(u16, Child, Option<Child>), WslGraphicsError> {
    start_session_impl(distro, desktop_cmd, dbus_cmd, extra_env).await
}

pub async fn start_app_session(
    distro: &str,
    argv: &[String],
    geometry: Option<&str>,
) -> Result<(u16, String, Child, Child), WslGraphicsError> {
    start_app_session_impl(distro, argv, geometry).await
}

pub async fn cleanup_wsl_session(distro: &str) {
    cleanup_wsl_session_impl(distro).await;
}

pub fn decode_wsl_output(raw: &[u8]) -> String {
    if raw.len() >= 2 && raw[0] == 0xff && raw[1] == 0xfe {
        return decode_utf16le(&raw[2..]);
    }
    if raw.len() >= 4 && raw[1] == 0x00 && raw[3] == 0x00 {
        return decode_utf16le(raw);
    }
    String::from_utf8_lossy(raw).to_string()
}

pub fn parse_wsl_distro_list(stdout: &str) -> Vec<WslDistro> {
    let mut distros = Vec::new();
    for line in stdout.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let is_default = line.starts_with('*');
        let line = line.trim_start_matches('*').trim();
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() >= 2 {
            distros.push(WslDistro {
                name: parts[0].to_string(),
                is_default,
                is_running: parts
                    .get(1)
                    .is_some_and(|state| state.eq_ignore_ascii_case("Running")),
            });
        }
    }
    distros
}

fn decode_utf16le(data: &[u8]) -> String {
    let u16_iter = data
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]));
    char::decode_utf16(u16_iter)
        .filter_map(Result::ok)
        .filter(|ch| *ch != '\0')
        .collect()
}

#[cfg(target_os = "windows")]
fn wsl_command() -> std::process::Command {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    let mut command = Command::new("wsl.exe");
    // Mirrors Tauri's graphics WSL subprocess policy: avoid flashing console windows.
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(target_os = "windows")]
fn wsl_command_async() -> tokio::process::Command {
    use tokio::process::Command;

    let mut command = Command::new("wsl.exe");
    // Mirrors Tauri's graphics WSL subprocess policy: avoid flashing console windows.
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(target_os = "windows")]
fn list_distros_impl() -> Result<Vec<WslDistro>, WslGraphicsError> {
    let output = wsl_command()
        .args(["--list", "--verbose"])
        .output()
        .map_err(|_| WslGraphicsError::WslNotAvailable)?;
    if !output.status.success() {
        return Err(WslGraphicsError::WslNotAvailable);
    }

    let distros = parse_wsl_distro_list(&decode_wsl_output(&output.stdout));
    if distros.is_empty() {
        return Err(WslGraphicsError::WslNotAvailable);
    }
    Ok(distros)
}

#[cfg(not(target_os = "windows"))]
fn list_distros_impl() -> Result<Vec<WslDistro>, WslGraphicsError> {
    Err(WslGraphicsError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
fn launch_distro_impl(distro: &str) -> Result<(), WslGraphicsError> {
    use std::process::Command;

    Command::new("wsl")
        .args(["-d", distro])
        .spawn()
        .map_err(WslGraphicsError::Io)?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn launch_distro_impl(_distro: &str) -> Result<(), WslGraphicsError> {
    Err(WslGraphicsError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
fn check_prerequisites_impl(distro: &str) -> Result<PrerequisiteResult, WslGraphicsError> {
    if !wsl_has_command(distro, "Xtigervnc")? {
        return Err(WslGraphicsError::NoVncServer(distro.to_string()));
    }
    let desktop = crate::desktop_candidates()
        .iter()
        .copied()
        .find(|candidate| wsl_has_command(distro, candidate.probe_cmd).unwrap_or(false))
        .ok_or_else(|| WslGraphicsError::NoDesktop(distro.to_string()))?;
    let dbus_cmd = if wsl_has_command(distro, "dbus-run-session")? {
        "dbus-run-session"
    } else if wsl_has_command(distro, "dbus-launch")? {
        "dbus-launch"
    } else {
        return Err(WslGraphicsError::NoDbus(distro.to_string()));
    };
    Ok(PrerequisiteResult {
        vnc_cmd: "Xtigervnc".to_string(),
        desktop,
        dbus_cmd: dbus_cmd.to_string(),
    })
}

#[cfg(not(target_os = "windows"))]
fn check_prerequisites_impl(_distro: &str) -> Result<PrerequisiteResult, WslGraphicsError> {
    Err(WslGraphicsError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
fn wsl_has_command(distro: &str, command_name: &str) -> Result<bool, WslGraphicsError> {
    let output = wsl_command()
        .args(["-d", distro, "--", "which", command_name])
        .status()
        .map_err(WslGraphicsError::Io)?;
    Ok(output.success())
}

#[cfg(target_os = "windows")]
async fn check_prerequisites_async_impl(
    distro: &str,
) -> Result<PrerequisiteResult, WslGraphicsError> {
    if !wsl_has_command_async(distro, "Xtigervnc").await {
        return Err(WslGraphicsError::NoVncServer(distro.to_string()));
    }

    let desktop = detect_desktop(distro)
        .await
        .ok_or_else(|| WslGraphicsError::NoDesktop(distro.to_string()))?;

    let dbus_cmd = if wsl_has_command_async(distro, "dbus-run-session").await {
        "dbus-run-session"
    } else if wsl_has_command_async(distro, "dbus-launch").await {
        "dbus-launch"
    } else {
        return Err(WslGraphicsError::NoDbus(distro.to_string()));
    };

    tracing::info!(
        "WSL Graphics prerequisites OK: desktop='{}' ({}), dbus='{}', extra_env={}",
        desktop.launch_cmd,
        desktop.display_name,
        dbus_cmd,
        if desktop.extra_env.is_empty() {
            "(none)"
        } else {
            "yes"
        }
    );

    Ok(PrerequisiteResult {
        vnc_cmd: "Xtigervnc".to_string(),
        desktop,
        dbus_cmd: dbus_cmd.to_string(),
    })
}

#[cfg(not(target_os = "windows"))]
async fn check_prerequisites_async_impl(
    _distro: &str,
) -> Result<PrerequisiteResult, WslGraphicsError> {
    Err(WslGraphicsError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
async fn check_vnc_available_impl(distro: &str) -> Result<(), WslGraphicsError> {
    if !wsl_has_command_async(distro, "Xtigervnc").await {
        return Err(WslGraphicsError::NoVncServer(distro.to_string()));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
async fn check_vnc_available_impl(_distro: &str) -> Result<(), WslGraphicsError> {
    Err(WslGraphicsError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
async fn start_session_impl(
    distro: &str,
    desktop_cmd: &str,
    dbus_cmd: &str,
    extra_env: &str,
) -> Result<(u16, Child, Option<Child>), WslGraphicsError> {
    let port = find_free_port().await?;
    let x_display = find_free_display(distro).await;

    let vnc_child = wsl_command_async()
        .args([
            "-d",
            distro,
            "--",
            "Xtigervnc",
            &x_display,
            "-rfbport",
            &port.to_string(),
            "-SecurityTypes",
            "None",
            "-localhost=0",
            "-ac",
            "-AlwaysShared",
            "-geometry",
            "1920x1080",
            "-depth",
            "24",
        ])
        .env_remove("WAYLAND_DISPLAY")
        .kill_on_drop(true)
        .spawn()?;

    tracing::info!(
        "WSL Graphics: Xtigervnc launched on display {} port {}",
        x_display,
        port
    );

    wait_for_vnc_ready(port, Duration::from_secs(10)).await?;
    let desktop_child =
        start_desktop_session(distro, &x_display, desktop_cmd, dbus_cmd, extra_env).await;
    Ok((port, vnc_child, desktop_child))
}

#[cfg(not(target_os = "windows"))]
async fn start_session_impl(
    _distro: &str,
    _desktop_cmd: &str,
    _dbus_cmd: &str,
    _extra_env: &str,
) -> Result<(u16, Child, Option<Child>), WslGraphicsError> {
    Err(WslGraphicsError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
async fn start_app_session_impl(
    distro: &str,
    argv: &[String],
    geometry: Option<&str>,
) -> Result<(u16, String, Child, Child), WslGraphicsError> {
    let port = find_free_port().await?;
    let x_display = find_free_display(distro).await;
    let geometry = geometry.unwrap_or("1280x720");

    let vnc_child = wsl_command_async()
        .args([
            "-d",
            distro,
            "--",
            "Xtigervnc",
            &x_display,
            "-rfbport",
            &port.to_string(),
            "-SecurityTypes",
            "None",
            "-localhost=0",
            "-ac",
            "-AlwaysShared",
            "-geometry",
            geometry,
            "-depth",
            "24",
        ])
        .env_remove("WAYLAND_DISPLAY")
        .kill_on_drop(true)
        .spawn()?;

    tracing::info!(
        "WSL Graphics App: Xtigervnc launched on display {} port {} ({})",
        x_display,
        port,
        geometry
    );

    wait_for_vnc_ready(port, Duration::from_secs(10)).await?;
    let app_child = start_app_process(distro, &x_display, argv).await?;
    Ok((port, x_display, vnc_child, app_child))
}

#[cfg(not(target_os = "windows"))]
async fn start_app_session_impl(
    _distro: &str,
    _argv: &[String],
    _geometry: Option<&str>,
) -> Result<(u16, String, Child, Child), WslGraphicsError> {
    Err(WslGraphicsError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
async fn cleanup_wsl_session_impl(distro: &str) {
    let _ = wsl_command_async()
        .args(["-d", distro, "--", "bash", "-c", &cleanup_script()])
        .output()
        .await;
    tracing::info!("WSL Graphics: session cleanup executed for '{}'", distro);
}

#[cfg(not(target_os = "windows"))]
async fn cleanup_wsl_session_impl(_distro: &str) {}

pub fn build_desktop_bootstrap_script(
    x_display: &str,
    desktop_cmd: &str,
    dbus_cmd: &str,
    extra_env: &str,
) -> String {
    let dbus_wrapper = if dbus_cmd == "dbus-run-session" {
        format!("exec dbus-run-session {desktop_cmd}")
    } else {
        format!(
            "eval $(dbus-launch --sh-syntax)\nexport DBUS_SESSION_BUS_ADDRESS\nexec {desktop_cmd}"
        )
    };

    format!(
        r#"#!/bin/bash
# OxideTerm desktop bootstrap script - auto-generated, do not edit
set -e

# Clear WSLg environment to avoid Weston interference
unset WAYLAND_DISPLAY XDG_SESSION_TYPE

export DISPLAY={display}
export XDG_RUNTIME_DIR="/tmp/oxideterm-xdg-$$"
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

{extra_env}

# Write PID file for session cleanup
echo $$ > {pid_file}

# Cleanup on exit
cleanup() {{
    rm -f {pid_file}
    rm -rf "$XDG_RUNTIME_DIR"
}}
trap cleanup EXIT

# Launch D-Bus + desktop session
{dbus_wrapper}
"#,
        display = x_display,
        pid_file = PID_FILE,
        extra_env = extra_env,
        dbus_wrapper = dbus_wrapper,
    )
}

pub fn build_app_bootstrap_script(x_display: &str) -> String {
    format!(
        r#"#!/bin/bash
set -e

# Clear WSLg environment to avoid Weston interference
unset WAYLAND_DISPLAY XDG_SESSION_TYPE

# Reset dangerous environment variables (§11.4 defense)
unset LD_PRELOAD LD_LIBRARY_PATH PYTHONPATH PYTHONSTARTUP NODE_OPTIONS
export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/snap/bin:$HOME/.local/bin"

export DISPLAY={display}
export XDG_RUNTIME_DIR="/tmp/oxideterm-app-xdg-$$"
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

# Optional: start lightweight window manager for window decorations
if command -v openbox-session &>/dev/null; then
    openbox --config-file /dev/null &
    sleep 0.3
fi

echo $$ > /tmp/oxideterm-app-$$.pid

cleanup() {{
    rm -f /tmp/oxideterm-app-$$.pid
    rm -rf "$XDG_RUNTIME_DIR"
}}
trap cleanup EXIT

# Application command passed via positional parameters - no shell parsing
exec "$@"
"#,
        display = x_display,
    )
}

const PID_FILE: &str = "/tmp/oxideterm-desktop.pid";

#[cfg(target_os = "windows")]
async fn wsl_has_command_async(distro: &str, command_name: &str) -> bool {
    wsl_command_async()
        .args(["-d", distro, "--", "which", command_name])
        .output()
        .await
        .is_ok_and(|output| output.status.success())
}

#[cfg(target_os = "windows")]
async fn detect_desktop(distro: &str) -> Option<crate::DesktopCandidate> {
    for candidate in crate::desktop_candidates() {
        if wsl_has_command_async(distro, candidate.probe_cmd).await {
            return Some(*candidate);
        }
    }
    None
}

#[cfg(target_os = "windows")]
async fn find_free_display(distro: &str) -> String {
    for n in 10..100 {
        let check = format!("test -e /tmp/.X11-unix/X{n}");
        let output = wsl_command_async()
            .args(["-d", distro, "--", "bash", "-c", &check])
            .output()
            .await;
        match output {
            Ok(output) if !output.status.success() => return format!(":{n}"),
            Err(_) => return format!(":{n}"),
            _ => {}
        }
    }
    ":99".to_string()
}

#[cfg(target_os = "windows")]
async fn start_desktop_session(
    distro: &str,
    x_display: &str,
    desktop_cmd: &str,
    dbus_cmd: &str,
    extra_env: &str,
) -> Option<Child> {
    let script = build_desktop_bootstrap_script(x_display, desktop_cmd, dbus_cmd, extra_env);
    let child = wsl_command_async()
        .args(["-d", distro, "--", "bash", "-s"])
        .env_remove("WAYLAND_DISPLAY")
        .kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    match child {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                if let Err(error) = stdin.write_all(script.as_bytes()).await {
                    tracing::warn!("WSL Graphics: failed to write bootstrap script: {}", error);
                    return None;
                }
            }
            tracing::info!(
                "WSL Graphics: desktop session '{}' launched via '{}' on display {}",
                desktop_cmd,
                dbus_cmd,
                x_display
            );
            Some(child)
        }
        Err(error) => {
            tracing::warn!("WSL Graphics: failed to start desktop session: {}", error);
            None
        }
    }
}

#[cfg(target_os = "windows")]
async fn start_app_process(
    distro: &str,
    x_display: &str,
    argv: &[String],
) -> Result<Child, WslGraphicsError> {
    let script = build_app_bootstrap_script(x_display);
    let mut child = wsl_command_async()
        .args(["-d", distro, "--", "bash", "-s", "--"])
        .args(argv)
        .env_clear()
        .env(
            "SYSTEMROOT",
            std::env::var("SYSTEMROOT").unwrap_or_default(),
        )
        .env(
            "SYSTEMDRIVE",
            std::env::var("SYSTEMDRIVE").unwrap_or_default(),
        )
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env(
            "USERPROFILE",
            std::env::var("USERPROFILE").unwrap_or_default(),
        )
        .kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        if let Err(error) = stdin.write_all(script.as_bytes()).await {
            tracing::warn!(
                "WSL Graphics App: failed to write bootstrap script: {}",
                error
            );
            let _ = child.kill().await;
            return Err(WslGraphicsError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                format!("Failed to write app bootstrap script: {error}"),
            )));
        }
    }

    tracing::info!(
        "WSL Graphics App: '{}' launched on display {}",
        argv.first().map(String::as_str).unwrap_or("?"),
        x_display
    );
    Ok(child)
}

#[cfg(target_os = "windows")]
async fn find_free_port() -> Result<u16, WslGraphicsError> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

#[cfg(target_os = "windows")]
async fn wait_for_vnc_ready(port: u16, max_wait: Duration) -> Result<(), WslGraphicsError> {
    let addr = format!("127.0.0.1:{port}");
    let deadline = tokio::time::Instant::now() + max_wait;

    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(WslGraphicsError::VncStartTimeout);
        }

        match timeout(Duration::from_millis(500), TcpStream::connect(&addr)).await {
            Ok(Ok(mut stream)) => {
                let mut buf = [0u8; 12];
                match timeout(Duration::from_secs(2), stream.read_exact(&mut buf)).await {
                    Ok(Ok(_)) if buf.starts_with(b"RFB ") => {
                        tracing::info!(
                            "VNC server ready on port {} ({})",
                            port,
                            String::from_utf8_lossy(&buf).trim()
                        );
                        return Ok(());
                    }
                    _ => sleep(Duration::from_millis(200)).await,
                }
            }
            _ => sleep(Duration::from_millis(300)).await,
        }
    }
}

#[cfg(target_os = "windows")]
fn cleanup_script() -> String {
    format!(
        r#"# Recursive process tree killer
kill_tree() {{
    local pid=$1
    local children
    children=$(pgrep -P "$pid" 2>/dev/null) || true
    for child in $children; do
        kill_tree "$child"
    done
    kill -TERM "$pid" 2>/dev/null || true
}}

if [ -f {pid} ]; then
    PID=$(cat {pid})
    if kill -0 "$PID" 2>/dev/null; then
        kill_tree "$PID"
        sleep 0.5
        children=$(pgrep -P "$PID" 2>/dev/null) || true
        for child in $children; do
            kill -KILL "$child" 2>/dev/null || true
        done
        kill -KILL "$PID" 2>/dev/null || true
    fi
    rm -f {pid}
fi
rm -rf /tmp/oxideterm-xdg-* 2>/dev/null || true
rm -rf /tmp/oxideterm-app-xdg-* 2>/dev/null || true

for pidfile in /tmp/oxideterm-app-*.pid; do
    [ -f "$pidfile" ] || continue
    APP_PID=$(cat "$pidfile" 2>/dev/null) || continue
    if kill -0 "$APP_PID" 2>/dev/null; then
        kill_tree "$APP_PID"
        sleep 0.3
        kill -KILL "$APP_PID" 2>/dev/null || true
    fi
    rm -f "$pidfile"
done"#,
        pid = PID_FILE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_wsl_output_handles_utf8() {
        assert_eq!(
            decode_wsl_output(b"NAME STATE\nUbuntu Running\n"),
            "NAME STATE\nUbuntu Running\n"
        );
    }

    #[test]
    fn decode_wsl_output_handles_utf16le_with_bom() {
        let text = "NAME STATE\nUbuntu Running\n";
        let mut raw = vec![0xff, 0xfe];
        raw.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(decode_wsl_output(&raw), text);
    }

    #[test]
    fn decode_wsl_output_handles_utf16le_without_bom() {
        let text = "NAME STATE\nUbuntu Running\n";
        let raw = text
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(decode_wsl_output(&raw), text);
    }

    #[test]
    fn parse_wsl_distro_list_matches_tauri_default_and_running_fields() {
        let distros = parse_wsl_distro_list(
            "  NAME                   STATE           VERSION\n* Ubuntu                 Running         2\n  Debian                 Stopped         2\n",
        );
        assert_eq!(
            distros,
            vec![
                WslDistro {
                    name: "Ubuntu".to_string(),
                    is_default: true,
                    is_running: true,
                },
                WslDistro {
                    name: "Debian".to_string(),
                    is_default: false,
                    is_running: false,
                },
            ]
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn non_windows_returns_tauri_not_available_error() {
        assert_eq!(
            list_distros().unwrap_err().to_string(),
            crate::WSL_GRAPHICS_UNAVAILABLE
        );
    }

    #[test]
    fn desktop_candidate_order_matches_tauri() {
        let names = crate::desktop_candidates()
            .iter()
            .map(|candidate| candidate.display_name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "Xfce",
                "GNOME",
                "KDE Plasma",
                "MATE",
                "LXDE",
                "Cinnamon",
                "Openbox",
                "Fluxbox",
                "IceWM"
            ]
        );
    }

    #[test]
    fn desktop_bootstrap_script_matches_tauri_dbus_run_session_path() {
        let script = build_desktop_bootstrap_script(":10", "xfce4-session", "dbus-run-session", "");
        assert!(script.contains("unset WAYLAND_DISPLAY XDG_SESSION_TYPE"));
        assert!(script.contains("export DISPLAY=:10"));
        assert!(script.contains("echo $$ > /tmp/oxideterm-desktop.pid"));
        assert!(script.contains("exec dbus-run-session xfce4-session"));
    }

    #[test]
    fn desktop_bootstrap_script_matches_tauri_dbus_launch_fallback() {
        let script = build_desktop_bootstrap_script(
            ":11",
            "gnome-session --session=gnome-xorg",
            "dbus-launch",
            "export XDG_SESSION_TYPE=x11\nexport GDK_BACKEND=x11",
        );
        assert!(script.contains("export XDG_SESSION_TYPE=x11"));
        assert!(script.contains("eval $(dbus-launch --sh-syntax)"));
        assert!(script.contains("export DBUS_SESSION_BUS_ADDRESS"));
        assert!(script.contains("exec gnome-session --session=gnome-xorg"));
    }

    #[test]
    fn app_bootstrap_script_keeps_tauri_exec_argv_semantics() {
        let script = build_app_bootstrap_script(":12");
        assert!(script.contains("unset LD_PRELOAD LD_LIBRARY_PATH"));
        assert!(script.contains("export DISPLAY=:12"));
        assert!(script.contains("openbox --config-file /dev/null &"));
        assert!(script.contains("exec \"$@\""));
    }
}
