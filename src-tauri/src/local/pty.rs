//! PTY (Pseudo-Terminal) abstraction
//!
//! Wraps portable-pty to provide a unified interface for creating
//! and managing pseudo-terminals across platforms.
//!
//! ## Windows Terminal Support
//! 
//! On Windows, this module provides enhanced support for:
//! - **UTF-8 encoding**: Automatic initialization of console code page and PowerShell encoding
//! - **Oh My Posh**: Automatic initialization when enabled in settings
//! - **WSL**: Proper environment variable passing via WSLENV
//!
//! See `generate_powershell_init_script()` for details.

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use crate::local::shell::{get_shell_args, ShellInfo};

#[cfg(unix)]
use nix::sys::signal::{killpg, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

/// Error type for PTY operations
#[derive(Debug, thiserror::Error)]
pub enum PtyError {
    #[error("Failed to create PTY: {0}")]
    CreateFailed(String),

    #[error("Failed to spawn shell: {0}")]
    SpawnFailed(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("PTY system error: {0}")]
    PtySystemError(String),

    #[error("Lock error")]
    LockError,
}

/// Configuration for creating a new PTY
#[derive(Clone, Debug)]
pub struct PtyConfig {
    pub cols: u16,
    pub rows: u16,
    pub shell: ShellInfo,
    pub cwd: Option<std::path::PathBuf>,
    pub env: Vec<(String, String)>,
    /// Whether to load shell profile/startup files
    pub load_profile: bool,
    /// Enable Oh My Posh prompt theme engine (Windows)
    pub oh_my_posh_enabled: bool,
    /// Path to Oh My Posh theme file (.omp.json)
    pub oh_my_posh_theme: Option<String>,
}

// ============================================================================
// Windows PowerShell Initialization Script Generator
// ============================================================================

/// Generate PowerShell initialization script for UTF-8 and Oh My Posh support.
/// 
/// This function generates a PowerShell command string that:
/// 1. Sets console encoding to UTF-8 (fixes CJK, emoji, Nerd Font display)
/// 2. Initializes Oh My Posh if enabled (loads prompt theme)
/// 
/// The script is injected via `-Command` parameter when spawning PowerShell.
/// 
/// # Why this is necessary
/// 
/// - `CHCP=65001` as environment variable doesn't work - must run as command
/// - `[Console]::OutputEncoding` must be set in PowerShell context
/// - Oh My Posh requires explicit `oh-my-posh init pwsh | Invoke-Expression`
#[cfg(target_os = "windows")]
fn generate_powershell_init_script(config: &PtyConfig) -> Option<String> {
    // Only generate for PowerShell shells
    if !matches!(config.shell.id.as_str(), "powershell" | "pwsh") {
        return None;
    }
    
    let mut parts: Vec<String> = Vec::new();
    
    // 1. UTF-8 encoding initialization
    // This fixes display of:
    // - CJK characters (中文, 日本語, 한국어)
    // - Emoji (🎉, 🚀, ✅)
    // - Nerd Font icons (, , )
    parts.push(
        "[Console]::InputEncoding = [Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
         $OutputEncoding = [System.Text.Encoding]::UTF8".to_string()
    );
    
    // 2. Oh My Posh initialization (if enabled)
    if config.oh_my_posh_enabled {
        let omp_init = if let Some(theme) = &config.oh_my_posh_theme {
            if !theme.is_empty() {
                // With custom theme
                format!(
                    "if (Get-Command oh-my-posh -ErrorAction SilentlyContinue) {{ \
                     oh-my-posh init pwsh --config '{}' | Invoke-Expression }}",
                    theme.replace('\'', "''")  // Escape single quotes
                )
            } else {
                // Default theme
                "if (Get-Command oh-my-posh -ErrorAction SilentlyContinue) { \
                 oh-my-posh init pwsh | Invoke-Expression }".to_string()
            }
        } else {
            // Default theme
            "if (Get-Command oh-my-posh -ErrorAction SilentlyContinue) { \
             oh-my-posh init pwsh | Invoke-Expression }".to_string()
        };
        parts.push(omp_init);
    }
    
    // 3. Clear screen for clean start (hide init commands output)
    parts.push("Clear-Host".to_string());
    
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

impl Default for PtyConfig {
    fn default() -> Self {
        Self {
            cols: 80,
            rows: 24,
            shell: crate::local::shell::default_shell(),
            cwd: None,
            env: vec![],
            load_profile: true,
            oh_my_posh_enabled: false,
            oh_my_posh_theme: None,
        }
    }
}

/// Thread-safe PTY handle
/// 
/// Since MasterPty is not Sync, we wrap it in a standard Mutex
/// and handle all operations through this wrapper.
pub struct PtyHandle {
    master: StdMutex<Box<dyn MasterPty + Send>>,
    child: StdMutex<Box<dyn portable_pty::Child + Send + Sync>>,
    reader: Arc<StdMutex<Box<dyn Read + Send>>>,
    writer: Arc<StdMutex<Box<dyn Write + Send>>>,
}

// Safety: We use StdMutex which provides Sync, and all operations
// are properly synchronized through the mutex.
unsafe impl Sync for PtyHandle {}

impl PtyHandle {
    /// Create a new PTY with the given configuration
    pub fn new(config: PtyConfig) -> Result<Self, PtyError> {
        let pty_system = native_pty_system();

        // Create PTY pair
        let pair = pty_system
            .openpty(PtySize {
                rows: config.rows,
                cols: config.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::CreateFailed(e.to_string()))?;

        // Build command
        let mut cmd = CommandBuilder::new(&config.shell.path);

        // =====================================================================
        // Windows PowerShell: Inject initialization script via -Command
        // This enables UTF-8 encoding and Oh My Posh without modifying user profile
        // =====================================================================
        #[cfg(target_os = "windows")]
        let using_powershell_init = {
            if let Some(init_script) = generate_powershell_init_script(&config) {
                tracing::info!("Injecting PowerShell init script for UTF-8 and OMP support");
                
                // Get shell args without -Command (we'll add our own)
                let mut base_args = get_shell_args(&config.shell.id, config.load_profile);
                
                // Add our init script via -Command
                // Note: -Command must come last, and we chain with user's profile if loaded
                base_args.push("-Command".to_string());
                
                // Build the full init command
                let cwd_path = config.cwd.as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "$HOME".to_string());
                
                let full_command = format!(
                    "{}; Set-Location -LiteralPath '{}'",
                    init_script,
                    cwd_path.replace('\'', "''")
                );
                base_args.push(full_command);
                
                for arg in &base_args {
                    cmd.arg(arg);
                }
                true
            } else {
                false
            }
        };
        
        #[cfg(not(target_os = "windows"))]
        let using_powershell_init = false;
        
        // Add shell arguments for non-PowerShell shells (or Windows non-PowerShell)
        if !using_powershell_init {
            let shell_args = if config.shell.id.starts_with("wsl") {
                // WSL uses wsl.exe args, not the shell args
                config.shell.args.clone()
            } else {
                // Use the dynamic args function for profile control
                get_shell_args(&config.shell.id, config.load_profile)
            };
            
            for arg in &shell_args {
                cmd.arg(arg);
            }
        }

        // Set working directory (skip for PowerShell with init script, handled in -Command)
        if !using_powershell_init {
            if let Some(cwd) = &config.cwd {
                cmd.cwd(cwd);
            } else if let Ok(home) = std::env::var("HOME") {
                cmd.cwd(home);
            } else if let Ok(userprofile) = std::env::var("USERPROFILE") {
                cmd.cwd(userprofile);
            }
        }

        // Set environment variables
        // Start with inheriting current environment
        for (key, value) in std::env::vars() {
            cmd.env(key, value);
        }

        // Override TERM for proper terminal emulation
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        // Windows-specific environment variables
        #[cfg(target_os = "windows")]
        {
            // Enable UTF-8 output for Python and other tools
            cmd.env("PYTHONIOENCODING", "utf-8");
            
            // Identify terminal program to all shells
            cmd.env("TERM_PROGRAM", "OxideTerm");
            cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
            
            // WSL-specific: enable UTF-8 mode and pass environment variables
            if config.shell.id.starts_with("wsl") {
                cmd.env("WSL_UTF8", "1");
                
                // WSLENV controls which env vars are passed to WSL
                // Format: VAR1:VAR2/p  (/p = translate Windows path to WSL path)
                let mut wslenv_vars = vec![
                    "TERM",
                    "COLORTERM", 
                    "TERM_PROGRAM",
                    "TERM_PROGRAM_VERSION",
                ];
                
                // Add POSH_THEME with path translation if Oh My Posh is enabled
                if config.oh_my_posh_enabled {
                    if let Some(theme_path) = &config.oh_my_posh_theme {
                        if !theme_path.is_empty() {
                            cmd.env("POSH_THEME", theme_path);
                            wslenv_vars.push("POSH_THEME/p");  // /p = path translation
                        }
                    }
                }
                
                cmd.env("WSLENV", wslenv_vars.join(":"));
                
                tracing::debug!("WSL WSLENV set to: {}", wslenv_vars.join(":"));
            }
            
            // Oh My Posh environment variables (for non-WSL shells)
            // Note: PowerShell init script handles OMP initialization via -Command
            if config.oh_my_posh_enabled && !config.shell.id.starts_with("wsl") {
                // POSH_THEME is still useful as env var for other tools to detect
                if let Some(theme_path) = &config.oh_my_posh_theme {
                    if !theme_path.is_empty() {
                        cmd.env("POSH_THEME", theme_path);
                    }
                }
            }
        }

        // Add custom environment variables
        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        // Ensure PATH includes common directories (especially for macOS Finder launch)
        #[cfg(unix)]
        {
            if let Ok(mut path) = std::env::var("PATH") {
                let additional_paths = ["/usr/local/bin", "/usr/local/sbin", "/opt/homebrew/bin"];
                for p in additional_paths {
                    if !path.contains(p) && Path::new(p).exists() {
                        path.push(':');
                        path.push_str(p);
                    }
                }
                cmd.env("PATH", path);
            }
        }

        // Spawn the shell
        tracing::info!(
            "Spawning PTY shell: {:?} (cwd: {:?}, powershell_init: {})",
            config.shell.path,
            config.cwd,
            using_powershell_init
        );
        
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| {
                tracing::error!("Failed to spawn PTY shell: {}", e);
                PtyError::SpawnFailed(e.to_string())
            })?;
        
        tracing::info!("PTY shell spawned successfully, PID: {:?}", child.process_id());

        // Get reader/writer handles
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| PtyError::PtySystemError(format!("Failed to clone reader: {}", e)))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| PtyError::PtySystemError(format!("Failed to take writer: {}", e)))?;

        Ok(Self {
            master: StdMutex::new(pair.master),
            child: StdMutex::new(child),
            reader: Arc::new(StdMutex::new(reader)),
            writer: Arc::new(StdMutex::new(writer)),
        })
    }

    /// Resize the PTY
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError> {
        let master = self.master.lock().map_err(|_| PtyError::LockError)?;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::PtySystemError(e.to_string()))
    }

    /// Write data to the PTY (input from terminal)
    pub fn write(&self, data: &[u8]) -> Result<usize, PtyError> {
        let mut writer = self.writer.lock().map_err(|_| PtyError::LockError)?;
        let n = writer.write(data)?;
        writer.flush()?;
        Ok(n)
    }

    /// Read data from the PTY (output to terminal)
    /// Returns the number of bytes read, or 0 on EOF
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, PtyError> {
        let mut reader = self.reader.lock().map_err(|_| PtyError::LockError)?;
        Ok(reader.read(buf)?)
    }

    /// Get a clone of the reader Arc for spawning read tasks
    pub fn clone_reader(&self) -> Arc<StdMutex<Box<dyn Read + Send>>> {
        self.reader.clone()
    }

    /// Get a clone of the writer Arc for spawning write tasks
    pub fn clone_writer(&self) -> Arc<StdMutex<Box<dyn Write + Send>>> {
        self.writer.clone()
    }

    /// Check if the child process is still running
    pub fn is_alive(&self) -> bool {
        if let Ok(mut child) = self.child.lock() {
            // try_wait returns Ok(None) if the process is still running
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }

    /// Wait for the child process to exit
    pub fn wait(&self) -> Result<portable_pty::ExitStatus, PtyError> {
        let mut child = self.child.lock().map_err(|_| PtyError::LockError)?;
        child
            .wait()
            .map_err(|e| PtyError::PtySystemError(e.to_string()))
    }

    /// Kill the child process
    pub fn kill(&self) -> Result<(), PtyError> {
        tracing::info!("Killing PTY child process (PID: {:?})", self.pid());
        let mut child = self.child.lock().map_err(|_| PtyError::LockError)?;
        child
            .kill()
            .map_err(|e| PtyError::PtySystemError(e.to_string()))
    }

    /// Kill the entire process group (PGID)
    /// This ensures all child processes (vim, btop, etc.) are cleaned up
    #[cfg(unix)]
    pub fn kill_process_group(&self) -> Result<(), PtyError> {
        if let Some(pid) = self.pid() {
            tracing::debug!("Killing process group for PID {}", pid);
            
            // First try to kill the process group
            // On Unix, the child process becomes a session leader and process group leader
            // So we can use the PID as the PGID
            let pgid = Pid::from_raw(pid as i32);
            
            // Send SIGTERM first to allow graceful shutdown
            if let Err(e) = killpg(pgid, Signal::SIGTERM) {
                tracing::warn!("Failed to send SIGTERM to process group {}: {}", pid, e);
            }
            
            // Give processes a brief moment to handle SIGTERM
            std::thread::sleep(std::time::Duration::from_millis(50));
            
            // Then send SIGKILL to ensure termination
            if let Err(e) = killpg(pgid, Signal::SIGKILL) {
                // This might fail if the process already exited, which is fine
                tracing::debug!("SIGKILL to process group {} (may have already exited): {}", pid, e);
            }
            
            Ok(())
        } else {
            // Fallback to regular kill
            self.kill()
        }
    }

    /// Kill the entire process group (PGID) - Windows version
    #[cfg(windows)]
    pub fn kill_process_group(&self) -> Result<(), PtyError> {
        // On Windows, we use the Windows Job API via portable-pty
        // which should handle child process cleanup
        // For now, we just kill the main process
        // TODO: Implement proper job object handling for Windows
        if let Some(pid) = self.pid() {
            tracing::debug!("Killing process tree for PID {} (Windows)", pid);
            
            // Try to use taskkill with /T flag to kill process tree
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .output();
        }
        
        self.kill()
    }

    /// Get the process ID of the child
    pub fn pid(&self) -> Option<u32> {
        if let Ok(child) = self.child.lock() {
            child.process_id()
        } else {
            None
        }
    }
}

impl Drop for PtyHandle {
    fn drop(&mut self) {
        // Ensure the entire process group is killed when the PTY is dropped
        // This prevents orphan processes (e.g., vim, btop) from lingering
        tracing::debug!("Dropping PTY, killing process group");
        let _ = self.kill_process_group();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pty_config_default() {
        let config = PtyConfig::default();
        assert_eq!(config.cols, 80);
        assert_eq!(config.rows, 24);
    }

    // Note: PTY creation tests require a real terminal environment
    // and may not work in CI. These are better tested manually.
}
