//! WSLg availability detection.
//!
//! Detects whether WSLg (Windows Subsystem for Linux GUI) is available
//! in a given WSL distribution by probing system-level mounts and sockets.
//!
//! **Does NOT rely on environment variables** like `WAYLAND_DISPLAY` — those
//! can be overridden in `.bashrc` or cleared by OxideTerm's own VNC bootstrap.
//! Instead, we check:
//!   1. Wayland socket: `/mnt/wslg/runtime-dir/wayland-0`
//!   2. Mount point: `/mnt/wslg/` directory
//!   3. XWayland socket: `/tmp/.X11-unix/X0`
//!   4. Version file: `/mnt/wslg/.wslgversion`

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::graphics::GraphicsError;

/// WSLg availability status for a WSL distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WslgStatus {
    /// Whether WSLg is overall available (Wayland socket alive, or mount + X11 both present).
    pub available: bool,
    /// Whether `/mnt/wslg/runtime-dir/wayland-0` Wayland socket exists.
    pub wayland: bool,
    /// Whether `/tmp/.X11-unix/X0` XWayland socket exists.
    pub x11: bool,
    /// Content of `/mnt/wslg/.wslgversion` if present.
    pub wslg_version: Option<String>,
    /// Whether Openbox window manager is installed (needed for Phase 2 app mode).
    pub has_openbox: bool,
}

/// Detect WSLg availability in the specified WSL distribution.
///
/// Uses a **three-level** detection strategy (descending reliability):
///   1. **Socket alive**: `/mnt/wslg/runtime-dir/wayland-0` (most reliable)
///   2. **Mount point exists**: `/mnt/wslg/` directory (WSLg installed but maybe not running)
///   3. **XWayland socket**: `/tmp/.X11-unix/X0` (X11 compatibility layer)
///
/// Additionally reads `/mnt/wslg/.wslgversion` for the version string (optional).
///
/// The overall `available` flag is true when:
/// - Wayland socket is alive, OR
/// - Both mount point AND X11 socket exist
pub async fn detect_wslg(distro: &str) -> Result<WslgStatus, GraphicsError> {
    // Run all checks in a single `sh -c` invocation to minimize WSL round-trips.
    // Each check echoes a unique marker so we can parse results from one output.
    let check_script = r#"
        echo "--- WAYLAND ---"
        test -S /mnt/wslg/runtime-dir/wayland-0 && echo "READY" || echo "NO"
        echo "--- MOUNT ---"
        test -d /mnt/wslg && echo "READY" || echo "NO"
        echo "--- X11 ---"
        test -S /tmp/.X11-unix/X0 && echo "READY" || echo "NO"
        echo "--- OPENBOX ---"
        which openbox-session >/dev/null 2>&1 && echo "READY" || echo "NO"
        echo "--- VERSION ---"
        cat /mnt/wslg/.wslgversion 2>/dev/null || echo ""
    "#;

    let output = Command::new("wsl.exe")
        .args(["-d", distro, "--", "sh", "-c", check_script])
        .output()
        .await
        .map_err(|e| GraphicsError::Io(e))?;

    if !output.status.success() {
        // Could be an invalid distro name or WSL not available
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not found") || stderr.contains("does not exist") {
            return Err(GraphicsError::WslNotAvailable);
        }
        return Err(GraphicsError::WslNotAvailable);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse sectioned output
    let wayland_ok = parse_section(&stdout, "--- WAYLAND ---", "--- MOUNT ---")
        .map(|s| s.trim() == "READY")
        .unwrap_or(false);

    let mount_ok = parse_section(&stdout, "--- MOUNT ---", "--- X11 ---")
        .map(|s| s.trim() == "READY")
        .unwrap_or(false);

    let x11_ok = parse_section(&stdout, "--- X11 ---", "--- OPENBOX ---")
        .map(|s| s.trim() == "READY")
        .unwrap_or(false);

    let has_openbox = parse_section(&stdout, "--- OPENBOX ---", "--- VERSION ---")
        .map(|s| s.trim() == "READY")
        .unwrap_or(false);

    let wslg_version = parse_section(&stdout, "--- VERSION ---", "")
        .map(|s| s.trim().to_string())
        .filter(|v| !v.is_empty());

    Ok(WslgStatus {
        available: wayland_ok || (mount_ok && x11_ok),
        wayland: wayland_ok,
        x11: x11_ok,
        wslg_version,
        has_openbox,
    })
}

/// Extract text between two section markers in the output.
/// If `end_marker` is empty, extracts to the end of string.
fn parse_section(output: &str, start_marker: &str, end_marker: &str) -> Option<String> {
    let start = output.find(start_marker)?;
    let after_marker = start + start_marker.len();
    let content = if end_marker.is_empty() {
        &output[after_marker..]
    } else {
        let end = output[after_marker..].find(end_marker)?;
        &output[after_marker..after_marker + end]
    };
    Some(content.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_section_basic() {
        let output = "--- A ---\nHELLO\n--- B ---\nWORLD\n";
        assert_eq!(
            parse_section(output, "--- A ---", "--- B ---"),
            Some("\nHELLO\n".to_string())
        );
        assert_eq!(
            parse_section(output, "--- B ---", ""),
            Some("\nWORLD\n".to_string())
        );
    }

    #[test]
    fn test_parse_section_missing() {
        let output = "--- A ---\nHELLO\n";
        assert_eq!(parse_section(output, "--- MISSING ---", ""), None);
    }

    #[test]
    fn test_wslg_status_serialization() {
        let status = WslgStatus {
            available: true,
            wayland: true,
            x11: true,
            wslg_version: Some("1.0.59".to_string()),
            has_openbox: true,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"available\":true"));
        assert!(json.contains("\"wslgVersion\":\"1.0.59\""));
        assert!(json.contains("\"hasOpenbox\":true"));
    }

    #[test]
    fn test_wslg_status_no_version() {
        let status = WslgStatus {
            available: false,
            wayland: false,
            x11: false,
            wslg_version: None,
            has_openbox: false,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"available\":false"));
        assert!(json.contains("\"wslgVersion\":null"));
        assert!(json.contains("\"hasOpenbox\":false"));
    }
}
