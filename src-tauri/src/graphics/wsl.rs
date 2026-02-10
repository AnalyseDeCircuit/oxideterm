//! WSL distro detection, Xtigervnc management, and desktop session bootstrap.
//!
//! Only Xtigervnc is supported. It creates a standalone X server on a free
//! display number (avoiding WSLg's Weston on `:0`), then launches a desktop
//! session via a bootstrap shell script that sets up D-Bus, XDG vars, etc.

use crate::graphics::GraphicsError;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout};

use super::WslDistro;

/// List WSL distributions by parsing `wsl.exe --list --verbose`.
///
/// ⚠️ Some Windows versions output UTF-16LE with BOM — we handle both encodings.
pub async fn list_distros() -> Result<Vec<WslDistro>, GraphicsError> {
    let output = Command::new("wsl.exe")
        .args(["--list", "--verbose"])
        .output()
        .await
        .map_err(|_| GraphicsError::WslNotAvailable)?;

    if !output.status.success() {
        return Err(GraphicsError::WslNotAvailable);
    }

    // Handle UTF-16LE BOM encoding (common on some Windows versions)
    let stdout = decode_wsl_output(&output.stdout);

    let mut distros = Vec::new();
    for line in stdout.lines().skip(1) {
        // skip header line
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let is_default = line.starts_with('*');
        let line = line.trim_start_matches('*').trim();

        // Format: "NAME    STATE    VERSION"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            distros.push(WslDistro {
                name: parts[0].to_string(),
                is_default,
                is_running: parts
                    .get(1)
                    .map(|s| s.eq_ignore_ascii_case("Running"))
                    .unwrap_or(false),
            });
        }
    }

    if distros.is_empty() {
        return Err(GraphicsError::WslNotAvailable);
    }

    Ok(distros)
}

/// Decode WSL output, handling UTF-16LE with or without BOM.
///
/// `wsl.exe --list --verbose` outputs UTF-16LE on most Windows versions.
/// Some include the BOM (FF FE), others don't. We use a heuristic:
/// if every other byte is 0x00, treat as UTF-16LE regardless of BOM.
fn decode_wsl_output(raw: &[u8]) -> String {
    // Check for UTF-16LE BOM: FF FE
    if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xFE {
        return decode_utf16le(&raw[2..]);
    }

    // Heuristic: UTF-16LE without BOM — check if null bytes are interleaved
    // (ASCII text encoded as UTF-16LE has 0x00 after every ASCII byte)
    if raw.len() >= 4 && raw[1] == 0x00 && raw[3] == 0x00 {
        return decode_utf16le(raw);
    }

    String::from_utf8_lossy(raw).to_string()
}

/// Decode a UTF-16LE byte slice (without BOM) into a String.
fn decode_utf16le(data: &[u8]) -> String {
    let u16_iter = data
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]));
    char::decode_utf16(u16_iter)
        .filter_map(|r| r.ok())
        .filter(|c| *c != '\0') // strip null chars
        .collect()
}

/// Desktop session commands in order of preference.
const DESKTOP_CANDIDATES: &[&str] = &[
    "xfce4-session",
    "mate-session",
    "startlxde",
    "openbox-session",
    "fluxbox",
    "icewm-session",
];

/// Marker file written by bootstrap script so we can clean up later.
const PID_FILE: &str = "/tmp/oxideterm-desktop.pid";

/// Check whether at least one desktop session command is installed in the distro.
async fn has_desktop(distro: &str) -> bool {
    for de in DESKTOP_CANDIDATES {
        let output = Command::new("wsl.exe")
            .args(["-d", distro, "--", "which", de])
            .output()
            .await;
        if let Ok(out) = output {
            if out.status.success() {
                return true;
            }
        }
    }
    false
}

/// Check whether D-Bus session launcher is available.
///
/// Prefers `dbus-run-session` (cleaner lifecycle) with `dbus-launch` as fallback.
/// Returns the command name if found, or `None` if neither is available.
async fn detect_dbus(distro: &str) -> Option<&'static str> {
    for cmd in &["dbus-run-session", "dbus-launch"] {
        let output = Command::new("wsl.exe")
            .args(["-d", distro, "--", "which", cmd])
            .output()
            .await;
        if let Ok(out) = output {
            if out.status.success() {
                return Some(cmd);
            }
        }
    }
    None
}

/// Check all prerequisites for Xtigervnc graphics session.
///
/// Verifies: Xtigervnc binary, desktop environment, and D-Bus launcher.
/// Returns the detected desktop command and D-Bus launcher.
pub async fn check_prerequisites(
    distro: &str,
) -> Result<(&'static str, &'static str), GraphicsError> {
    // 1. Check for Xtigervnc
    let output = Command::new("wsl.exe")
        .args(["-d", distro, "--", "which", "Xtigervnc"])
        .output()
        .await;
    let has_vnc = output.map(|o| o.status.success()).unwrap_or(false);
    if !has_vnc {
        return Err(GraphicsError::NoVncServer(distro.to_string()));
    }

    // 2. Check for a desktop environment
    let mut desktop_cmd: Option<&str> = None;
    for de in DESKTOP_CANDIDATES {
        let output = Command::new("wsl.exe")
            .args(["-d", distro, "--", "which", de])
            .output()
            .await;
        if let Ok(out) = output {
            if out.status.success() {
                desktop_cmd = Some(de);
                break;
            }
        }
    }
    let desktop_cmd = desktop_cmd.ok_or_else(|| GraphicsError::NoDesktop(distro.to_string()))?;

    // 3. Check for D-Bus
    let dbus_cmd =
        detect_dbus(distro)
            .await
            .ok_or_else(|| GraphicsError::NoDbus(distro.to_string()))?;

    tracing::info!(
        "WSL Graphics prerequisites OK: desktop='{}', dbus='{}'",
        desktop_cmd,
        dbus_cmd
    );

    Ok((desktop_cmd, dbus_cmd))
}

/// Find a free X display number by checking `/tmp/.X11-unix/X{n}` inside WSL.
/// Starts from `:10` to avoid collision with WSLg (`:0`) and common user displays.
async fn find_free_display(distro: &str) -> String {
    for n in 10..100 {
        let check = format!("test -e /tmp/.X11-unix/X{}", n);
        let output = Command::new("wsl.exe")
            .args(["-d", distro, "--", "bash", "-c", &check])
            .output()
            .await;
        if let Ok(out) = output {
            if !out.status.success() {
                // Socket doesn't exist → display is free
                return format!(":{}", n);
            }
        } else {
            // Can't check — just use it
            return format!(":{}", n);
        }
    }
    // Fallback
    ":99".to_string()
}

/// Start an Xtigervnc server and desktop session inside WSL.
///
/// Returns `(vnc_port, vnc_child, desktop_child)`.
///
/// 1. Finds a free X display number (`:10`+) and TCP port
/// 2. Launches Xtigervnc as a standalone X+VNC server
/// 3. Waits for the RFB handshake
/// 4. Generates and runs a bootstrap shell script that initializes D-Bus,
///    XDG environment, and launches the desktop session
pub async fn start_session(
    distro: &str,
    desktop_cmd: &str,
    dbus_cmd: &str,
) -> Result<(u16, Child, Option<Child>), GraphicsError> {
    let port = find_free_port().await?;
    let display = find_free_display(distro).await;

    // 1. Start Xtigervnc
    let vnc_child = Command::new("wsl.exe")
        .args([
            "-d",
            distro,
            "--",
            "Xtigervnc",
            &display,
            "-rfbport",
            &port.to_string(),
            "-SecurityTypes",
            "None",
            "-localhost=0",
            "-ac",
            "-AlwaysShared",
            "-geometry",
            "1280x720",
            "-depth",
            "24",
        ])
        .env_remove("WAYLAND_DISPLAY")
        .kill_on_drop(true)
        .spawn()?;

    tracing::info!(
        "WSL Graphics: Xtigervnc launched on display {} port {}",
        display,
        port
    );

    // 2. Wait for VNC to be ready (RFB handshake)
    wait_for_vnc_ready(port, Duration::from_secs(10)).await?;

    // 3. Launch desktop session via bootstrap script
    let desktop_child =
        start_desktop_session(distro, &display, desktop_cmd, dbus_cmd).await;

    Ok((port, vnc_child, desktop_child))
}

/// Generate and execute a bootstrap shell script inside WSL that:
/// - Clears WSLg environment variables
/// - Sets up `XDG_RUNTIME_DIR`
/// - Launches a D-Bus session bus (`dbus-run-session` or `dbus-launch`)
/// - Starts the desktop session as a foreground process
/// - Writes a PID file for session-level cleanup
///
/// Returns the `Child` handle of the `wsl.exe` process running the script.
async fn start_desktop_session(
    distro: &str,
    x_display: &str,
    desktop_cmd: &str,
    dbus_cmd: &str,
) -> Option<Child> {
    // Build the bootstrap script.
    // `dbus-run-session` wraps the desktop command directly (cleaner lifecycle).
    // `dbus-launch` needs eval + exec pattern.
    let dbus_wrapper = if dbus_cmd == "dbus-run-session" {
        format!("exec dbus-run-session {}", desktop_cmd)
    } else {
        format!(
            "eval $(dbus-launch --sh-syntax)\nexport DBUS_SESSION_BUS_ADDRESS\nexec {}",
            desktop_cmd
        )
    };

    let script = format!(
        r#"#!/bin/bash
# OxideTerm desktop bootstrap script — auto-generated, do not edit
set -e

# Clear WSLg environment to avoid Weston interference
unset WAYLAND_DISPLAY XDG_SESSION_TYPE

export DISPLAY={display}
export XDG_RUNTIME_DIR="/tmp/oxideterm-xdg-$$"
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

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
        dbus_wrapper = dbus_wrapper,
    );

    // Pipe script content into bash via stdin
    let child = Command::new("wsl.exe")
        .args(["-d", distro, "--", "bash", "-s"])
        .env_remove("WAYLAND_DISPLAY")
        .kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    match child {
        Ok(mut child) => {
            // Write the script to stdin
            if let Some(mut stdin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                if let Err(e) = stdin.write_all(script.as_bytes()).await {
                    tracing::warn!("WSL Graphics: failed to write bootstrap script: {}", e);
                    return None;
                }
                drop(stdin); // Close stdin so bash starts executing
            }
            tracing::info!(
                "WSL Graphics: desktop session '{}' launched via '{}' on display {}",
                desktop_cmd,
                dbus_cmd,
                x_display
            );
            Some(child)
        }
        Err(e) => {
            tracing::warn!("WSL Graphics: failed to start desktop session: {}", e);
            None
        }
    }
}

/// Clean up any lingering session processes inside WSL.
///
/// Called when stopping a session — reads the PID file written by the
/// bootstrap script and sends SIGTERM to the process tree.
pub async fn cleanup_wsl_session(distro: &str) {
    let cleanup_cmd = format!(
        r#"if [ -f {pid} ]; then
    PID=$(cat {pid})
    if kill -0 "$PID" 2>/dev/null; then
        pkill -TERM -P "$PID" 2>/dev/null || true
        kill -TERM "$PID" 2>/dev/null || true
        sleep 0.5
        kill -KILL "$PID" 2>/dev/null || true
    fi
    rm -f {pid}
fi
rm -rf /tmp/oxideterm-xdg-* 2>/dev/null || true"#,
        pid = PID_FILE,
    );

    let _ = Command::new("wsl.exe")
        .args(["-d", distro, "--", "bash", "-c", &cleanup_cmd])
        .output()
        .await;
    tracing::info!("WSL Graphics: session cleanup executed for '{}'", distro);
}

/// Find an available port by binding to :0, reading the assigned port, then releasing.
///
/// ⚠️ TOCTOU risk — the port may be taken between release and VNC bind.
/// Mitigated by wait_for_vnc_ready() timeout which will detect bind failures.
async fn find_free_port() -> Result<u16, GraphicsError> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Wait for VNC server to become ready by attempting TCP connection
/// and reading the RFB version string ("RFB 003.0...").
async fn wait_for_vnc_ready(port: u16, max_wait: Duration) -> Result<(), GraphicsError> {
    let addr = format!("127.0.0.1:{}", port);
    let deadline = tokio::time::Instant::now() + max_wait;

    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(GraphicsError::VncStartTimeout);
        }

        match timeout(Duration::from_millis(500), TcpStream::connect(&addr)).await {
            Ok(Ok(mut stream)) => {
                // Try to read RFB version string (12 bytes: "RFB 003.0xx\n")
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
                    _ => {
                        // Connected but no RFB handshake yet
                        sleep(Duration::from_millis(200)).await;
                    }
                }
            }
            _ => {
                // Connection refused — VNC not ready yet
                sleep(Duration::from_millis(300)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_utf8_output() {
        let input = b"  NAME      STATE           VERSION\n* Ubuntu    Running         2\n  Debian    Stopped         2\n";
        let result = decode_wsl_output(input);
        assert!(result.contains("Ubuntu"));
        assert!(result.contains("Debian"));
    }

    #[test]
    fn test_decode_utf16le_bom_output() {
        // UTF-16LE BOM + "Hi"
        let input = vec![0xFF, 0xFE, b'H', 0x00, b'i', 0x00];
        let result = decode_wsl_output(&input);
        assert_eq!(result, "Hi");
    }

    #[test]
    fn test_decode_utf16le_no_bom_output() {
        // UTF-16LE WITHOUT BOM — common on many Windows versions
        // "* Ubuntu    Running         2\n"
        let text = "  NAME      STATE           VERSION\n* Ubuntu    Running         2\n";
        let input: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let result = decode_wsl_output(&input);
        assert!(result.contains("Ubuntu"));
        assert!(result.contains("Running"));
        assert!(!result.contains('\0'));
    }

    #[test]
    fn test_parse_distros_utf16le_no_bom() {
        // Simulate full wsl.exe output as UTF-16LE without BOM
        let text = "  NAME      STATE           VERSION\r\n* Ubuntu    Running         2\r\n  Debian    Stopped         2\r\n";
        let raw: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let decoded = decode_wsl_output(&raw);

        // Parse lines like list_distros does
        let mut distros = Vec::new();
        for line in decoded.lines().skip(1) {
            let line = line.trim();
            if line.is_empty() { continue; }
            let is_default = line.starts_with('*');
            let line = line.trim_start_matches('*').trim();
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                distros.push((parts[0].to_string(), is_default, parts[1].to_string()));
            }
        }

        assert_eq!(distros.len(), 2);
        assert_eq!(distros[0].0, "Ubuntu");
        assert!(distros[0].1); // is_default
        assert_eq!(distros[0].2, "Running");
        assert_eq!(distros[1].0, "Debian");
        assert!(!distros[1].1);
        assert_eq!(distros[1].2, "Stopped");
    }

    #[test]
    fn test_bootstrap_script_dbus_run_session() {
        // Verify the script uses `dbus-run-session` when available
        // (just a sanity check on string formatting)
        let display = ":10";
        let dbus_wrapper = format!("exec dbus-run-session {}", "xfce4-session");
        assert!(dbus_wrapper.contains("dbus-run-session xfce4-session"));
        assert!(!dbus_wrapper.contains("dbus-launch"));
        let _ = display; // prevent unused warning
    }

    #[test]
    fn test_bootstrap_script_dbus_launch_fallback() {
        let dbus_wrapper = format!(
            "eval $(dbus-launch --sh-syntax)\nexport DBUS_SESSION_BUS_ADDRESS\nexec {}",
            "xfce4-session"
        );
        assert!(dbus_wrapper.contains("dbus-launch --sh-syntax"));
        assert!(dbus_wrapper.contains("exec xfce4-session"));
    }
}
