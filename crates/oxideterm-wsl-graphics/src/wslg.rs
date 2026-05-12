// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{WslGraphicsError, WslgStatus};

pub fn detect_wslg(distro: &str) -> Result<WslgStatus, WslGraphicsError> {
    detect_wslg_impl(distro)
}

pub async fn detect_wslg_async(distro: &str) -> Result<WslgStatus, WslGraphicsError> {
    detect_wslg_async_impl(distro).await
}

pub fn parse_wslg_status_output(stdout: &str) -> WslgStatus {
    let wayland = parse_section(stdout, "WAYLAND").is_some_and(|value| value.trim() == "READY");
    let mount = parse_section(stdout, "MOUNT").is_some_and(|value| value.trim() == "READY");
    let x11 = parse_section(stdout, "X11").is_some_and(|value| value.trim() == "READY");
    let has_openbox = parse_section(stdout, "OPENBOX").is_some_and(|value| value.trim() == "READY");
    let wslg_version = parse_section(stdout, "VERSION").and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    });

    WslgStatus {
        available: wayland || (mount && x11),
        wayland,
        x11,
        wslg_version,
        has_openbox,
    }
}

pub fn parse_section(output: &str, name: &str) -> Option<String> {
    let marker = format!("--- {name} ---");
    let start = output.find(&marker)? + marker.len();
    let rest = &output[start..];
    let end = rest.find("--- ").unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

#[cfg(target_os = "windows")]
fn detect_wslg_impl(distro: &str) -> Result<WslgStatus, WslGraphicsError> {
    use std::process::Command;

    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut command = Command::new("wsl.exe");
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .args(["-d", distro, "--", "sh", "-c", detect_script()])
        .output()
        .map_err(WslGraphicsError::Io)?;
    if !output.status.success() {
        return Err(WslGraphicsError::WslNotAvailable);
    }
    Ok(parse_wslg_status_output(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

#[cfg(not(target_os = "windows"))]
fn detect_wslg_impl(_distro: &str) -> Result<WslgStatus, WslGraphicsError> {
    Err(WslGraphicsError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
async fn detect_wslg_async_impl(distro: &str) -> Result<WslgStatus, WslGraphicsError> {
    use tokio::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut command = Command::new("wsl.exe");
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .args(["-d", distro, "--", "sh", "-c", detect_script()])
        .output()
        .await
        .map_err(WslGraphicsError::Io)?;
    if !output.status.success() {
        return Err(WslGraphicsError::WslNotAvailable);
    }
    Ok(parse_wslg_status_output(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

#[cfg(not(target_os = "windows"))]
async fn detect_wslg_async_impl(_distro: &str) -> Result<WslgStatus, WslGraphicsError> {
    Err(WslGraphicsError::UnsupportedPlatform)
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn detect_script() -> &'static str {
    r#"echo "--- WAYLAND ---"
test -S /mnt/wslg/runtime-dir/wayland-0 && echo "READY" || echo "NO"
echo "--- MOUNT ---"
test -d /mnt/wslg && echo "READY" || echo "NO"
echo "--- X11 ---"
test -S /tmp/.X11-unix/X0 && echo "READY" || echo "NO"
echo "--- OPENBOX ---"
which openbox-session >/dev/null 2>&1 && echo "READY" || echo "NO"
echo "--- VERSION ---"
cat /mnt/wslg/.wslgversion 2>/dev/null || echo """#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_section_reads_between_tauri_markers() {
        let output = "--- WAYLAND ---\nREADY\n--- MOUNT ---\nNO\n";
        assert_eq!(parse_section(output, "WAYLAND").as_deref(), Some("READY"));
        assert_eq!(parse_section(output, "MOUNT").as_deref(), Some("NO"));
    }

    #[test]
    fn parse_wslg_status_uses_tauri_availability_rule() {
        let output = "--- WAYLAND ---\nNO\n--- MOUNT ---\nREADY\n--- X11 ---\nREADY\n--- OPENBOX ---\nREADY\n--- VERSION ---\n1.0.65\n";
        assert_eq!(
            parse_wslg_status_output(output),
            WslgStatus {
                available: true,
                wayland: false,
                x11: true,
                wslg_version: Some("1.0.65".to_string()),
                has_openbox: true,
            }
        );
    }

    #[test]
    fn parse_wslg_status_keeps_empty_version_none() {
        let output = "--- WAYLAND ---\nREADY\n--- MOUNT ---\nREADY\n--- X11 ---\nNO\n--- OPENBOX ---\nNO\n--- VERSION ---\n\n";
        assert_eq!(parse_wslg_status_output(output).wslg_version, None);
    }
}
