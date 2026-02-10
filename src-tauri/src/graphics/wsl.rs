//! WSL distro detection and VNC server management.

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

/// Detect available VNC server in a WSL distro.
///
/// Priority: Xtigervnc (standalone X+VNC, avoids Weston/WSLg conflict)
///         > x0vncserver (screen scraper, only useful if a separate X server is running)
///         > wayvnc (Wayland-native, needs a running Wayland compositor)
///
/// For Xtigervnc, also validates that a desktop environment is installed,
/// since a standalone X server shows a black screen without one.
pub async fn detect_vnc(distro: &str) -> Result<String, GraphicsError> {
    let candidates = ["Xtigervnc", "x0vncserver", "wayvnc"];
    let mut found_xtigervnc = false;

    for binary in &candidates {
        let output = Command::new("wsl.exe")
            .args(["-d", distro, "--", "which", binary])
            .output()
            .await;
        if let Ok(out) = output {
            if out.status.success() {
                if *binary == "Xtigervnc" {
                    // Xtigervnc requires a desktop session to avoid black screen
                    if has_desktop(distro).await {
                        return Ok(binary.to_string());
                    }
                    found_xtigervnc = true;
                    tracing::warn!(
                        "WSL Graphics: Xtigervnc found in '{}' but no desktop environment installed, skipping",
                        distro
                    );
                    continue;
                }
                return Ok(binary.to_string());
            }
        }
    }

    // Xtigervnc exists but no desktop → give specific error
    if found_xtigervnc {
        return Err(GraphicsError::NoDesktop(distro.to_string()));
    }

    Err(GraphicsError::NoVncServer(distro.to_string()))
}

/// The X display number used by our standalone Xtigervnc server.
/// `:99` avoids collision with WSLg's Weston on `:0`.
const TIGERVNC_DISPLAY: &str = ":99";

/// Start a VNC server inside WSL.
///
/// Returns (vnc_port, child_process).
///
/// For **Xtigervnc** (preferred): creates a standalone X server on `:99` with its own
/// framebuffer, completely independent of WSLg's Weston on `:0`.
/// After VNC is ready, a desktop session (xfce4 etc.) is auto-launched on that display.
///
/// For **x0vncserver**: scrapes an existing X display — only useful when a separate
/// X server is already running (not WSLg's Weston-controlled `:0`).
pub async fn start_vnc(distro: &str, vnc_binary: &str) -> Result<(u16, Child), GraphicsError> {
    let port = find_free_port().await?;

    let child = match vnc_binary {
        "Xtigervnc" => Command::new("wsl.exe")
            .args([
                "-d",
                distro,
                "--",
                "Xtigervnc",
                TIGERVNC_DISPLAY,
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
            .spawn()?,
        "x0vncserver" => Command::new("wsl.exe")
            .args([
                "-d",
                distro,
                "--",
                "x0vncserver",
                "-display",
                ":0",
                "-rfbport",
                &port.to_string(),
                "-SecurityTypes",
                "None",
                "-localhost=0",
                "--I-KNOW-THIS-IS-INSECURE",
            ])
            .env_remove("WAYLAND_DISPLAY")
            .kill_on_drop(true)
            .spawn()?,
        "wayvnc" => Command::new("wsl.exe")
            .args([
                "-d",
                distro,
                "--",
                "wayvnc",
                "--output=HEADLESS-1",
                "0.0.0.0",
                &port.to_string(),
            ])
            .env_remove("WAYLAND_DISPLAY")
            .kill_on_drop(true)
            .spawn()?,
        _ => return Err(GraphicsError::UnsupportedVnc(vnc_binary.to_string())),
    };

    // Wait for VNC to be ready (poll for RFB handshake)
    wait_for_vnc_ready(port, Duration::from_secs(10)).await?;

    // For standalone X server (Xtigervnc), launch a desktop session on the new display
    if vnc_binary == "Xtigervnc" {
        start_desktop_session(distro, TIGERVNC_DISPLAY).await;
    }

    Ok((port, child))
}

/// Detect and start a desktop session on the given X display.
///
/// Tries common lightweight desktops in order of preference.
/// Fire-and-forget: runs in the background inside WSL.
async fn start_desktop_session(distro: &str, x_display: &str) {
    // Find which desktop is installed
    let mut desktop: Option<&str> = None;
    for de in DESKTOP_CANDIDATES {
        let output = Command::new("wsl.exe")
            .args(["-d", distro, "--", "which", de])
            .output()
            .await;
        if let Ok(out) = output {
            if out.status.success() {
                desktop = Some(de);
                break;
            }
        }
    }

    match desktop {
        Some(de) => {
            let cmd = format!(
                "export DISPLAY={} && {} >/dev/null 2>&1 &",
                x_display, de
            );
            let _ = Command::new("wsl.exe")
                .args(["-d", distro, "--", "bash", "-c", &cmd])
                .env_remove("WAYLAND_DISPLAY")
                .spawn();
            tracing::info!(
                "WSL Graphics: launched desktop '{}' on display {}",
                de,
                x_display
            );
        }
        None => {
            tracing::warn!(
                "WSL Graphics: no desktop environment found in '{}'. \
                 VNC will show an empty screen. Install one: \
                 sudo apt install xfce4",
                distro
            );
        }
    }
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
}
