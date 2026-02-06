//! Remote Environment Detector
//!
//! One-shot detection of remote host OS, architecture, shell, and distribution
//! after SSH connection establishment. Results are cached per-connection and
//! pushed to the frontend via Tauri events for AI context injection.
//!
//! # Design
//! - Opens a **temporary** shell channel (closed immediately after detection)
//! - Two-phase detection: Phase A identifies Windows vs Unix; Phase B collects details
//! - Handles "disguised" Windows environments (Git Bash/MinGW, MSYS, Cygwin, WSL)
//! - Total timeout: 8s. Failure → `os_type = "Unknown"`, logged but non-fatal
//!
//! # Invariants
//! - E1: Detection channel is closed after use — no lingering MaxSessions consumption
//! - E2: Runs exactly once per connection (result cached in `ConnectionEntry`)
//! - E3: SSH disconnect during detection → graceful abort, no panic

use std::time::Duration;

use chrono::Utc;
use russh::client::Msg;
use russh::{Channel, ChannelMsg};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;
use tracing::{debug, info, warn};

use crate::ssh::HandleController;

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

const CHANNEL_OPEN_TIMEOUT: Duration = Duration::from_secs(5);
const PHASE_A_TIMEOUT: Duration = Duration::from_secs(3);
const PHASE_B_TIMEOUT: Duration = Duration::from_secs(5);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_OUTPUT_SIZE: usize = 8192;

/// Phase A: Platform discrimination — single-line, busybox-safe command.
/// Uses `$PSModulePath` to detect Windows (always set on Windows, never on Unix).
const PHASE_A_CMD: &str =
    "echo '===DETECT==='; if [ -n \"$PSModulePath\" ]; then echo 'PLATFORM=windows'; else echo \"PLATFORM=$(uname -s 2>/dev/null || echo unknown)\"; fi; echo '===END==='\n";

/// Phase B (Unix): Collect OS, arch, kernel, shell, and distro info.
const PHASE_B_UNIX_CMD: &str =
    "echo '===ENV==='; uname -s 2>/dev/null; echo '===ARCH==='; uname -m 2>/dev/null; echo '===KERNEL==='; uname -r 2>/dev/null; echo '===SHELL==='; echo $SHELL 2>/dev/null; echo '===DISTRO==='; cat /etc/os-release 2>/dev/null | grep -E '^(PRETTY_NAME|ID)=' | head -2; echo '===END==='\n";

/// Phase B (Windows/PowerShell): Collect version, arch, and shell info.
const PHASE_B_WINDOWS_CMD: &str =
    "echo '===ENV==='; [System.Environment]::OSVersion.VersionString; echo '===ARCH==='; $env:PROCESSOR_ARCHITECTURE; echo '===SHELL==='; \"PowerShell $($PSVersionTable.PSVersion)\"; echo '===END==='\n";

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// Detected remote environment information.
///
/// `os_type` special values for Windows variants:
/// - `"Windows"` — native PowerShell/cmd
/// - `"Windows_MinGW"` — Git Bash / MinGW environment
/// - `"Windows_MSYS"` — MSYS2 environment 
/// - `"Windows_Cygwin"` — Cygwin environment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEnvInfo {
    /// OS type: "Linux", "macOS", "Windows", "FreeBSD", "Windows_MinGW", "Unknown", etc.
    pub os_type: String,

    /// Human-readable OS version (e.g., "Ubuntu 22.04.3 LTS", "Microsoft Windows NT 10.0.22631")
    pub os_version: Option<String>,

    /// Kernel version (uname -r)
    pub kernel: Option<String>,

    /// Architecture (uname -m or PROCESSOR_ARCHITECTURE)
    pub arch: Option<String>,

    /// Default shell ($SHELL or "PowerShell 7.x")
    pub shell: Option<String>,

    /// Detection timestamp (Unix seconds)
    pub detected_at: i64,
}

impl RemoteEnvInfo {
    /// Create an "Unknown" result for detection failures.
    pub fn unknown() -> Self {
        Self {
            os_type: "Unknown".to_string(),
            os_version: None,
            kernel: None,
            arch: None,
            shell: None,
            detected_at: Utc::now().timestamp(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Detection Logic
// ═══════════════════════════════════════════════════════════════════════════

/// Run remote environment detection on the given SSH connection.
///
/// Opens a temporary shell channel, runs platform-agnostic detection commands,
/// parses the output, and returns the result. The channel is closed before returning.
pub async fn detect_remote_env(
    controller: &HandleController,
    connection_id: &str,
) -> RemoteEnvInfo {
    let result = timeout(TOTAL_TIMEOUT, detect_inner(controller, connection_id)).await;

    match result {
        Ok(info) => info,
        Err(_) => {
            warn!(
                "[EnvDetector] Total timeout ({}s) exceeded for {}",
                TOTAL_TIMEOUT.as_secs(),
                connection_id
            );
            RemoteEnvInfo::unknown()
        }
    }
}

async fn detect_inner(controller: &HandleController, connection_id: &str) -> RemoteEnvInfo {
    // 1. Open temporary shell channel
    let mut channel = match open_detect_channel(controller).await {
        Ok(ch) => ch,
        Err(e) => {
            warn!("[EnvDetector] Failed to open channel for {}: {}", connection_id, e);
            return RemoteEnvInfo::unknown();
        }
    };

    // Ensure channel is closed on all exit paths
    let result = run_detection(&mut channel, connection_id).await;

    if let Err(e) = channel.close().await {
        debug!("[EnvDetector] Channel close error (non-fatal) for {}: {}", connection_id, e);
    }

    result
}

async fn run_detection(channel: &mut Channel<Msg>, connection_id: &str) -> RemoteEnvInfo {
    // 2. Phase A: Platform discrimination
    let phase_a_output = match send_and_read(channel, PHASE_A_CMD, "===END===", PHASE_A_TIMEOUT).await {
        Ok(output) => output,
        Err(e) => {
            warn!("[EnvDetector] Phase A failed for {}: {}", connection_id, e);
            return RemoteEnvInfo::unknown();
        }
    };

    let is_windows = phase_a_output.contains("PLATFORM=windows");
    let raw_platform = extract_between(&phase_a_output, "PLATFORM=", "\n")
        .unwrap_or_default()
        .trim()
        .to_string();

    debug!(
        "[EnvDetector] Phase A result for {}: is_windows={}, raw_platform='{}'",
        connection_id, is_windows, raw_platform
    );

    // 3. Phase B: Platform-specific detail collection
    let phase_b_cmd = if is_windows {
        PHASE_B_WINDOWS_CMD
    } else {
        PHASE_B_UNIX_CMD
    };

    let phase_b_output = match send_and_read(channel, phase_b_cmd, "===END===", PHASE_B_TIMEOUT).await {
        Ok(output) => output,
        Err(e) => {
            warn!("[EnvDetector] Phase B failed for {}: {}", connection_id, e);
            // We at least know the platform from Phase A
            let os_type = if is_windows {
                "Windows".to_string()
            } else {
                classify_unix_os(&raw_platform)
            };
            return RemoteEnvInfo {
                os_type,
                os_version: None,
                kernel: None,
                arch: None,
                shell: None,
                detected_at: Utc::now().timestamp(),
            };
        }
    };

    // 4. Parse Phase B output
    if is_windows {
        parse_windows_env(&phase_b_output)
    } else {
        parse_unix_env(&phase_b_output, &raw_platform)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Channel Management  
// ═══════════════════════════════════════════════════════════════════════════

/// Open a temporary shell channel with minimal initialization.
async fn open_detect_channel(controller: &HandleController) -> Result<Channel<Msg>, String> {
    let channel = timeout(CHANNEL_OPEN_TIMEOUT, controller.open_session_channel())
        .await
        .map_err(|_| "Timeout opening detection channel".to_string())?
        .map_err(|e| format!("Failed to open detection channel: {}", e))?;

    // Request shell (not exec — more compatible with restricted environments)
    channel
        .request_shell(false)
        .await
        .map_err(|e| format!("Failed to request shell: {}", e))?;

    // Minimal init: disable prompt/echo for clean output parsing
    let init_cmd = "export PS1=''; export PS2=''; stty -echo 2>/dev/null; export LANG=C\n";
    channel
        .data(init_cmd.as_bytes())
        .await
        .map_err(|e| format!("Failed to init detection shell: {}", e))?;

    // Brief settle time
    tokio::time::sleep(Duration::from_millis(150)).await;

    Ok(channel)
}

/// Send a command and read stdout until the end marker appears.
async fn send_and_read(
    channel: &mut Channel<Msg>,
    cmd: &str,
    end_marker: &str,
    read_timeout: Duration,
) -> Result<String, String> {
    channel
        .data(cmd.as_bytes())
        .await
        .map_err(|e| format!("Failed to write command: {}", e))?;

    let mut stdout = Vec::new();

    let result = timeout(read_timeout, async {
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) => {
                    stdout.extend_from_slice(&data);
                    if stdout.len() > MAX_OUTPUT_SIZE {
                        stdout.truncate(MAX_OUTPUT_SIZE);
                        break;
                    }
                    if let Ok(s) = std::str::from_utf8(&stdout) {
                        if s.contains(end_marker) {
                            break;
                        }
                    }
                }
                Some(ChannelMsg::ExtendedData { .. }) => {}
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => {
                    return Err("Detection channel closed unexpectedly".to_string());
                }
                Some(_) => {}
                None => {
                    return Err("Detection channel returned None".to_string());
                }
            }
        }
        Ok(())
    })
    .await;

    match result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err(format!("Read timeout ({}ms)", read_timeout.as_millis())),
    }

    String::from_utf8(stdout).map_err(|e| format!("Invalid UTF-8: {}", e))
}

// ═══════════════════════════════════════════════════════════════════════════
// Parsers
// ═══════════════════════════════════════════════════════════════════════════

/// Classify a Unix `uname -s` output into an OS type, handling Windows-like environments.
fn classify_unix_os(uname_s: &str) -> String {
    let s = uname_s.trim();
    let upper = s.to_uppercase();

    // Windows variant environments (Git Bash, MSYS, Cygwin)
    if upper.starts_with("MINGW32") || upper.starts_with("MINGW64") {
        return "Windows_MinGW".to_string();
    }
    if upper.starts_with("MSYS") {
        return "Windows_MSYS".to_string();
    }
    if upper.starts_with("CYGWIN") {
        return "Windows_Cygwin".to_string();
    }

    // Standard Unix variants
    match s {
        "Linux" => "Linux".to_string(),
        "Darwin" => "macOS".to_string(),
        "FreeBSD" => "FreeBSD".to_string(),
        "OpenBSD" => "OpenBSD".to_string(),
        "NetBSD" => "NetBSD".to_string(),
        "SunOS" => "SunOS".to_string(),
        _ => {
            if s.is_empty() || s == "unknown" {
                "Unknown".to_string()
            } else {
                s.to_string() // Preserve unrecognized but valid uname output
            }
        }
    }
}

/// Parse Phase B output for Unix hosts.
fn parse_unix_env(output: &str, raw_platform: &str) -> RemoteEnvInfo {
    let os_type = classify_unix_os(raw_platform);

    let env_val = extract_section(output, "===ENV===", "===ARCH===")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let arch = extract_section(output, "===ARCH===", "===KERNEL===")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let kernel = extract_section(output, "===KERNEL===", "===SHELL===")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let shell = extract_section(output, "===SHELL===", "===DISTRO===")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let distro_block = extract_section(output, "===DISTRO===", "===END===")
        .unwrap_or_default();

    // Extract PRETTY_NAME from /etc/os-release
    let os_version = extract_os_release_field(&distro_block, "PRETTY_NAME")
        .or_else(|| extract_os_release_field(&distro_block, "ID"))
        .or(env_val); // Fallback to raw uname -s if no os-release

    info!(
        "[EnvDetector] Unix result: os_type={}, version={:?}, arch={:?}, kernel={:?}, shell={:?}",
        os_type, os_version, arch, kernel, shell
    );

    RemoteEnvInfo {
        os_type,
        os_version,
        kernel,
        arch,
        shell,
        detected_at: Utc::now().timestamp(),
    }
}

/// Parse Phase B output for Windows hosts.
fn parse_windows_env(output: &str) -> RemoteEnvInfo {
    let os_version = extract_section(output, "===ENV===", "===ARCH===")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let arch = extract_section(output, "===ARCH===", "===SHELL===")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let shell = extract_section(output, "===SHELL===", "===END===")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    info!(
        "[EnvDetector] Windows result: version={:?}, arch={:?}, shell={:?}",
        os_version, arch, shell
    );

    RemoteEnvInfo {
        os_type: "Windows".to_string(),
        os_version,
        kernel: None,
        arch,
        shell,
        detected_at: Utc::now().timestamp(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// String Utilities
// ═══════════════════════════════════════════════════════════════════════════

/// Extract text between two markers.
fn extract_section(text: &str, start_marker: &str, end_marker: &str) -> Option<String> {
    let start = text.find(start_marker)?;
    let after_start = start + start_marker.len();
    let end = text[after_start..].find(end_marker)?;
    Some(text[after_start..after_start + end].to_string())
}

/// Extract text after a prefix up to a delimiter.
fn extract_between(text: &str, prefix: &str, delimiter: &str) -> Option<String> {
    let start = text.find(prefix)?;
    let after = start + prefix.len();
    let end = text[after..].find(delimiter).map(|i| after + i).unwrap_or(text.len());
    Some(text[after..end].to_string())
}

/// Extract a field from /etc/os-release content (KEY="value" or KEY=value format).
fn extract_os_release_field(content: &str, field: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(field) {
            if let Some(value) = rest.strip_prefix('=') {
                // Remove surrounding quotes if present
                let cleaned = value.trim().trim_matches('"').trim().to_string();
                if !cleaned.is_empty() {
                    return Some(cleaned);
                }
            }
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_unix_os() {
        assert_eq!(classify_unix_os("Linux"), "Linux");
        assert_eq!(classify_unix_os("Darwin"), "macOS");
        assert_eq!(classify_unix_os("FreeBSD"), "FreeBSD");
        assert_eq!(classify_unix_os("MINGW64_NT-10.0-19045"), "Windows_MinGW");
        assert_eq!(classify_unix_os("MSYS_NT-10.0-19045"), "Windows_MSYS");
        assert_eq!(classify_unix_os("CYGWIN_NT-10.0"), "Windows_Cygwin");
        assert_eq!(classify_unix_os(""), "Unknown");
        assert_eq!(classify_unix_os("unknown"), "Unknown");
        assert_eq!(classify_unix_os("SunOS"), "SunOS");
    }

    #[test]
    fn test_parse_unix_env() {
        let output = r#"===ENV===
Linux
===ARCH===
x86_64
===KERNEL===
5.15.0-91-generic
===SHELL===
/bin/bash
===DISTRO===
PRETTY_NAME="Ubuntu 22.04.3 LTS"
ID=ubuntu
===END===
"#;
        let result = parse_unix_env(output, "Linux");
        assert_eq!(result.os_type, "Linux");
        assert_eq!(result.os_version.as_deref(), Some("Ubuntu 22.04.3 LTS"));
        assert_eq!(result.arch.as_deref(), Some("x86_64"));
        assert_eq!(result.kernel.as_deref(), Some("5.15.0-91-generic"));
        assert_eq!(result.shell.as_deref(), Some("/bin/bash"));
    }

    #[test]
    fn test_parse_unix_env_macos() {
        let output = r#"===ENV===
Darwin
===ARCH===
arm64
===KERNEL===
23.4.0
===SHELL===
/bin/zsh
===DISTRO===
===END===
"#;
        let result = parse_unix_env(output, "Darwin");
        assert_eq!(result.os_type, "macOS");
        assert_eq!(result.arch.as_deref(), Some("arm64"));
        assert_eq!(result.shell.as_deref(), Some("/bin/zsh"));
        // No /etc/os-release on macOS, falls back to uname -s value
        assert_eq!(result.os_version.as_deref(), Some("Darwin"));
    }

    #[test]
    fn test_parse_unix_env_mingw() {
        let output = r#"===ENV===
MINGW64_NT-10.0-19045
===ARCH===
x86_64
===KERNEL===
3.4.10-87d57229.x86_64
===SHELL===
/usr/bin/bash
===DISTRO===
===END===
"#;
        let result = parse_unix_env(output, "MINGW64_NT-10.0-19045");
        assert_eq!(result.os_type, "Windows_MinGW");
        assert_eq!(result.arch.as_deref(), Some("x86_64"));
    }

    #[test]
    fn test_parse_windows_env() {
        let output = r#"===ENV===
Microsoft Windows NT 10.0.22631.0
===ARCH===
AMD64
===SHELL===
PowerShell 7.4.1
===END===
"#;
        let result = parse_windows_env(output);
        assert_eq!(result.os_type, "Windows");
        assert_eq!(
            result.os_version.as_deref(),
            Some("Microsoft Windows NT 10.0.22631.0")
        );
        assert_eq!(result.arch.as_deref(), Some("AMD64"));
        assert_eq!(result.shell.as_deref(), Some("PowerShell 7.4.1"));
    }

    #[test]
    fn test_extract_os_release_field() {
        let content = r#"PRETTY_NAME="Ubuntu 22.04.3 LTS"
ID=ubuntu"#;
        assert_eq!(
            extract_os_release_field(content, "PRETTY_NAME"),
            Some("Ubuntu 22.04.3 LTS".to_string())
        );
        assert_eq!(
            extract_os_release_field(content, "ID"),
            Some("ubuntu".to_string())
        );
        assert_eq!(extract_os_release_field(content, "MISSING"), None);
    }

    #[test]
    fn test_extract_section() {
        let text = "===A===hello===B===world===C===";
        assert_eq!(extract_section(text, "===A===", "===B==="), Some("hello".to_string()));
        assert_eq!(extract_section(text, "===B===", "===C==="), Some("world".to_string()));
        assert_eq!(extract_section(text, "===X===", "===Y==="), None);
    }

    #[test]
    fn test_unknown() {
        let info = RemoteEnvInfo::unknown();
        assert_eq!(info.os_type, "Unknown");
        assert!(info.os_version.is_none());
    }
}
