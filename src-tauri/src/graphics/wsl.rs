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

/// A desktop session candidate with detection binary and launch command.
struct DesktopCandidate {
    /// Binary to check with `which` (must exist in $PATH)
    detect_bin: &'static str,
    /// Actual command to exec in the bootstrap script (may include args)
    launch_cmd: &'static str,
    /// Extra environment exports injected into the bootstrap script.
    /// E.g. GNOME needs `XDG_SESSION_TYPE=x11`, KDE needs `QT_QPA_PLATFORM=xcb`.
    /// Empty string if no extra env is needed.
    extra_env: &'static str,
    /// Human-readable name shown in the UI
    display_name: &'static str,
}

/// Desktop session candidates in order of preference.
const DESKTOP_CANDIDATES: &[DesktopCandidate] = &[
    DesktopCandidate {
        detect_bin: "xfce4-session",
        launch_cmd: "xfce4-session",
        extra_env: "",
        display_name: "Xfce",
    },
    DesktopCandidate {
        detect_bin: "gnome-session",
        launch_cmd: "gnome-session --session=gnome-xorg",
        extra_env: "export XDG_SESSION_TYPE=x11\nexport GDK_BACKEND=x11",
        display_name: "GNOME",
    },
    DesktopCandidate {
        detect_bin: "startplasma-x11",
        launch_cmd: "startplasma-x11",
        extra_env: "export QT_QPA_PLATFORM=xcb\nexport DESKTOP_SESSION=plasma\nexport KWIN_COMPOSE=N",
        display_name: "KDE Plasma",
    },
    DesktopCandidate {
        detect_bin: "mate-session",
        launch_cmd: "mate-session",
        extra_env: "",
        display_name: "MATE",
    },
    DesktopCandidate {
        detect_bin: "startlxde",
        launch_cmd: "startlxde",
        extra_env: "",
        display_name: "LXDE",
    },
    DesktopCandidate {
        detect_bin: "cinnamon-session",
        launch_cmd: "cinnamon-session",
        extra_env: "",
        display_name: "Cinnamon",
    },
    DesktopCandidate {
        detect_bin: "openbox-session",
        launch_cmd: "openbox-session",
        extra_env: "",
        display_name: "Openbox",
    },
    DesktopCandidate {
        detect_bin: "fluxbox",
        launch_cmd: "fluxbox",
        extra_env: "",
        display_name: "Fluxbox",
    },
    DesktopCandidate {
        detect_bin: "icewm-session",
        launch_cmd: "icewm-session",
        extra_env: "",
        display_name: "IceWM",
    },
];

/// Marker file written by bootstrap script so we can clean up later.
const PID_FILE: &str = "/tmp/oxideterm-desktop.pid";

/// Check whether at least one desktop session command is installed in the distro.
async fn has_desktop(distro: &str) -> bool {
    for de in DESKTOP_CANDIDATES {
        let output = Command::new("wsl.exe")
            .args(["-d", distro, "--", "which", de.detect_bin])
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
) -> Result<(&'static str, &'static str, &'static str, &'static str), GraphicsError> {
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
    let mut matched: Option<&DesktopCandidate> = None;
    for de in DESKTOP_CANDIDATES {
        let output = Command::new("wsl.exe")
            .args(["-d", distro, "--", "which", de.detect_bin])
            .output()
            .await;
        if let Ok(out) = output {
            if out.status.success() {
                matched = Some(de);
                break;
            }
        }
    }
    let candidate = matched.ok_or_else(|| GraphicsError::NoDesktop(distro.to_string()))?;

    // 3. Check for D-Bus
    let dbus_cmd =
        detect_dbus(distro)
            .await
            .ok_or_else(|| GraphicsError::NoDbus(distro.to_string()))?;

    tracing::info!(
        "WSL Graphics prerequisites OK: desktop='{}' ({}), dbus='{}', extra_env={}",
        candidate.launch_cmd,
        candidate.display_name,
        dbus_cmd,
        if candidate.extra_env.is_empty() { "(none)" } else { "yes" }
    );

    Ok((candidate.launch_cmd, dbus_cmd, candidate.extra_env, candidate.display_name))
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
    extra_env: &str,
) -> Result<(u16, Child, Option<Child>), GraphicsError> {
    let port = find_free_port().await?;
    let disp = find_free_display(distro).await;

    // 1. Start Xtigervnc
    let vnc_child = Command::new("wsl.exe")
        .args([
            "-d",
            distro,
            "--",
            "Xtigervnc",
            &disp,
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
        disp,
        port
    );

    // 2. Wait for VNC to be ready (RFB handshake)
    wait_for_vnc_ready(port, Duration::from_secs(10)).await?;

    // 3. Launch desktop session via bootstrap script
    let desktop_child =
        start_desktop_session(distro, &disp, desktop_cmd, dbus_cmd, extra_env).await;

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
    extra_env: &str,
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

// ─── App Mode (Phase 2) ─────────────────────────────────────────────

/// Check that Xtigervnc is available in the distro (without checking for desktop env).
///
/// Used by app mode which doesn't need a desktop environment.
pub async fn check_vnc_available(distro: &str) -> Result<(), GraphicsError> {
    let output = Command::new("wsl.exe")
        .args(["-d", distro, "--", "which", "Xtigervnc"])
        .output()
        .await;
    let has_vnc = output.map(|o| o.status.success()).unwrap_or(false);
    if !has_vnc {
        return Err(GraphicsError::NoVncServer(distro.to_string()));
    }
    Ok(())
}

/// Start a single-app graphics session (no desktop environment).
///
/// Similar to `start_session()` but:
/// - Uses a smaller default resolution
/// - Launches an optional lightweight WM (Openbox) + the target application
/// - No D-Bus required (though some apps may need it)
///
/// Returns `(vnc_port, x_display, vnc_child, app_child)`.
pub async fn start_app_session(
    distro: &str,
    argv: &[String],
    geometry: Option<&str>,
) -> Result<(u16, String, Child, Child), GraphicsError> {
    let port = find_free_port().await?;
    let disp = find_free_display(distro).await;
    let geo = geometry.unwrap_or("1280x720");

    // 1. Start Xtigervnc with smaller resolution (app mode)
    let vnc_child = Command::new("wsl.exe")
        .args([
            "-d",
            distro,
            "--",
            "Xtigervnc",
            &disp,
            "-rfbport",
            &port.to_string(),
            "-SecurityTypes",
            "None",
            "-localhost=0",
            "-ac",
            "-AlwaysShared",
            "-geometry",
            geo,
            "-depth",
            "24",
        ])
        .env_remove("WAYLAND_DISPLAY")
        .kill_on_drop(true)
        .spawn()?;

    tracing::info!(
        "WSL Graphics App: Xtigervnc launched on display {} port {} ({})",
        disp,
        port,
        geo
    );

    // 2. Wait for VNC to be ready
    wait_for_vnc_ready(port, Duration::from_secs(10)).await?;

    // 3. Launch app via bootstrap script
    let app_child = start_app_process(distro, &disp, argv).await?;

    Ok((port, disp, vnc_child, app_child))
}

/// Generate the app-mode bootstrap script.
///
/// Sets up DISPLAY, XDG_RUNTIME_DIR, optionally starts Openbox WM,
/// then executes the target application via `exec "$@"` (no shell injection).
fn build_app_bootstrap_script(x_display: &str) -> String {
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

# Application command passed via positional parameters — no shell parsing
exec "$@"
"#,
        display = x_display,
    )
}

/// Start the application process inside WSL.
///
/// Uses `env_clear()` + minimal whitelist (§11.4) and pipes the bootstrap
/// script via stdin. argv elements become positional parameters via `bash -s --`.
async fn start_app_process(
    distro: &str,
    x_display: &str,
    argv: &[String],
) -> Result<Child, GraphicsError> {
    let script = build_app_bootstrap_script(x_display);

    // Build wsl.exe args:
    // wsl.exe -d Ubuntu -- bash -s -- gedit /home/user/file.txt
    //                                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    //                                 These become $1, $2, ... executed by `exec "$@"`
    let mut child = Command::new("wsl.exe")
        .args(["-d", distro, "--", "bash", "-s", "--"])
        .args(argv)
        // §11.4: Clear all inherited environment, inject only safe minimum
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

    // Write the bootstrap script to stdin
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        if let Err(e) = stdin.write_all(script.as_bytes()).await {
            tracing::warn!("WSL Graphics App: failed to write bootstrap script: {}", e);
            let _ = child.kill().await;
            return Err(GraphicsError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                format!("Failed to write app bootstrap script: {}", e),
            )));
        }
        drop(stdin); // Close stdin so bash starts executing
    }

    tracing::info!(
        "WSL Graphics App: '{}' launched on display {}",
        argv.first().map(|s| s.as_str()).unwrap_or("?"),
        x_display
    );

    Ok(child)
}

/// Clean up any lingering session processes inside WSL.
///
/// Called when stopping a session — reads the PID file written by the
/// bootstrap script and recursively kills the entire process tree.
/// This is critical for GNOME which spawns deep process trees
/// (gnome-session → gnome-shell → gnome-settings-daemon → ...).
///
/// Also cleans up app-mode PID files (oxideterm-app-*.pid).
pub async fn cleanup_wsl_session(distro: &str) {
    let cleanup_cmd = format!(
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
        # Force-kill anything still alive
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

# Kill app process trees before removing PID files
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
        let dbus_wrapper = format!("exec dbus-run-session {}", "xfce4-session");
        assert!(dbus_wrapper.contains("dbus-run-session xfce4-session"));
        assert!(!dbus_wrapper.contains("dbus-launch"));
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

    #[test]
    fn test_bootstrap_script_gnome_extra_env() {
        // GNOME needs extra_env for X11 session type + GDK backend
        let gnome = DESKTOP_CANDIDATES
            .iter()
            .find(|c| c.detect_bin == "gnome-session")
            .expect("gnome-session should be in candidates");
        assert!(gnome.extra_env.contains("XDG_SESSION_TYPE=x11"));
        assert!(gnome.extra_env.contains("GDK_BACKEND=x11"));
        assert_eq!(gnome.display_name, "GNOME");
        let dbus_wrapper = format!("exec dbus-run-session {}", gnome.launch_cmd);
        assert!(dbus_wrapper.contains("gnome-session --session=gnome-xorg"));
    }

    #[test]
    fn test_desktop_candidates_kde_has_extra_env() {
        // KDE Plasma needs QT_QPA_PLATFORM=xcb + DESKTOP_SESSION=plasma + KWIN_COMPOSE=N
        let kde = DESKTOP_CANDIDATES
            .iter()
            .find(|c| c.detect_bin == "startplasma-x11")
            .expect("startplasma-x11 should be in candidates");
        assert!(kde.extra_env.contains("QT_QPA_PLATFORM=xcb"));
        assert!(kde.extra_env.contains("DESKTOP_SESSION=plasma"));
        assert!(kde.extra_env.contains("KWIN_COMPOSE=N"));
        assert_eq!(kde.display_name, "KDE Plasma");
        assert_eq!(kde.launch_cmd, "startplasma-x11");
    }

    #[test]
    fn test_desktop_candidates_extra_env_consistency() {
        // GNOME and KDE should have non-empty extra_env, others should be empty
        for c in DESKTOP_CANDIDATES {
            match c.detect_bin {
                "gnome-session" | "startplasma-x11" => {
                    assert!(!c.extra_env.is_empty(), "{} should have extra_env", c.display_name);
                }
                _ => {
                    assert!(c.extra_env.is_empty(), "{} should NOT have extra_env", c.display_name);
                }
            }
        }
    }
}
