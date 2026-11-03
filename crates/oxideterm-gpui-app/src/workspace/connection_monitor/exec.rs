//! Host Tools execution targets. A capture runs either on an SSH node or on
//! the machine running the application; both return the same output shape so
//! the per-tool parsers and delivery paths stay unchanged.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use oxideterm_connection_monitor::{
    ResourceSampleError, ResourceSampleShell, ResourceSampler, ResourceSamplerFuture,
};
use oxideterm_ssh::{SshCommandOutput, SshConnectionHandle};
use tokio::io::AsyncReadExt;

/// Synthetic Host Tools connection id for the local machine.
pub(in crate::workspace) const LOCAL_HOST_CONNECTION_ID: &str = "oxide-local-host";

/// Local OS in the same vocabulary as remote environment detection, so the
/// existing per-OS command builders select the local dialect unchanged.
pub(in crate::workspace) fn local_host_os_type() -> &'static str {
    match std::env::consts::OS {
        "windows" => "Windows",
        "macos" => "macOS",
        "freebsd" => "FreeBSD",
        "openbsd" => "OpenBSD",
        "netbsd" => "NetBSD",
        "solaris" => "SunOS",
        _ => "Linux",
    }
}

/// Where one Host Tools capture or action runs.
pub(in crate::workspace) enum HostCommandTarget {
    Ssh(SshConnectionHandle),
    Local,
}

impl HostCommandTarget {
    pub(super) async fn run_command_capture(
        &self,
        command: &str,
        timeout: Duration,
        max_output_size: usize,
    ) -> Result<SshCommandOutput, ()> {
        match self {
            Self::Ssh(handle) => handle
                .run_command_capture(command, timeout, max_output_size)
                .await
                .map_err(|_| ()),
            Self::Local => run_local_command_capture(command, timeout, max_output_size).await,
        }
    }
}

/// Executes a generated command line through the local shell the way an SSH
/// exec would on that platform: `cmd.exe` on Windows (the line already carries
/// the PowerShell invocation) and `sh` elsewhere.
async fn run_local_command_capture(
    command: &str,
    timeout: Duration,
    max_output_size: usize,
) -> Result<SshCommandOutput, ()> {
    let line = command.trim_end_matches(['\r', '\n']);
    let mut process = local_shell_command(line);
    process
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child = process.spawn().map_err(|_| ())?;
    // Dropping the capture future is a cancellation, not a timeout: `kill_on_drop`
    // only ends the shell, so the guard ends the tree the same way the timeout
    // path does.
    let mut child = LocalCaptureChild::new(child);
    let stdout = child.child.stdout.take().ok_or(())?;
    let stderr = child.child.stderr.take().ok_or(())?;
    // Both streams share one budget, matching the SSH capture limit.
    let budget = Arc::new(AtomicUsize::new(max_output_size));
    // The child stays owned here: both streams closing is what ends a normal
    // capture, and a hung command leaves them open forever.
    let reads = tokio::time::timeout(timeout, async {
        tokio::join!(
            read_capped(stdout, budget.clone()),
            read_capped(stderr, budget.clone()),
        )
    })
    .await;
    let (stdout, stderr) = match reads {
        Ok(read) => read,
        Err(_) => {
            // A Windows capture is `cmd /C powershell …`; killing only the
            // shell leaves the PowerShell running its hung CIM call.
            child.kill_tree().await;
            return Err(());
        }
    };
    let status = child.child.wait().await.map_err(|_| ())?;
    child.disarm();
    let (stdout, stdout_truncated) = stdout;
    let (stderr, stderr_truncated) = stderr;
    Ok(SshCommandOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code: status.code(),
        truncated: stdout_truncated || stderr_truncated,
    })
}

/// Runs profiler samples as one-shot local captures. SSH profilers keep a
/// dedicated shell per connection; the local machine instead pays one process
/// spawn per sample, which the 10 s sampling interval makes acceptable.
pub(in crate::workspace) struct LocalResourceSampler;

impl ResourceSampler for LocalResourceSampler {
    fn open_shell<'a>(
        &'a self,
        init_command: &'a str,
        _timeout: Duration,
    ) -> ResourceSamplerFuture<'a, Result<Box<dyn ResourceSampleShell>, String>> {
        // The SSH profile types the init line into its persistent shell. A
        // local sample runs one command per process, so interactive setup is
        // pointless: the Windows `set PROMPT=` would even swallow the whole
        // `cmd /C` line as its assignment value and the PowerShell command
        // would never run. Only the POSIX locale exports affect the numeric
        // parsers, so the init line survives on Unix alone.
        let init = if cfg!(windows) {
            String::new()
        } else {
            init_command
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join("; ")
        };
        Box::pin(async move {
            let pwsh = local_pwsh_available();
            Ok(Box::new(LocalResourceSampleShell { init, pwsh }) as Box<dyn ResourceSampleShell>)
        })
    }
}

/// PowerShell 5.1 rejects `Get-Process -IncludeUserName` without elevation,
/// which blanks the process user column, while pwsh 7 fills user names for the
/// caller's own processes. Running pwsh also avoids paying the 5.1 startup
/// before the sample's own interpreter. The swap stays inside the sampling
/// shell; one-shot capture and action commands keep their generated 5.1 line.
fn local_sample_line(command: &str, pwsh: bool) -> String {
    if pwsh && cfg!(windows) {
        if let Some(rest) = command.strip_prefix("powershell ") {
            return format!("pwsh {rest}");
        }
    }
    command.to_string()
}

/// Probes `pwsh` on PATH once per process. `where.exe` costs a few
/// milliseconds; spawning pwsh itself would cost a full interpreter start.
fn local_pwsh_available() -> bool {
    static CACHE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHE.get_or_init(|| {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            std::process::Command::new("where")
                .arg("pwsh")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .creation_flags(LOCAL_HOST_COMMAND_CREATE_NO_WINDOW)
                .output()
                .map(|output| output.status.success() && !output.stdout.is_empty())
                .unwrap_or(false)
        }
        #[cfg(not(windows))]
        {
            false
        }
    })
}

struct LocalResourceSampleShell {
    /// Shell setup statements the sample command must run first.
    init: String,
    /// Whether the sample interpreter may be swapped to pwsh (see
    /// `local_sample_line`).
    pwsh: bool,
}

impl ResourceSampleShell for LocalResourceSampleShell {
    fn sample_until<'a>(
        &'a mut self,
        command: &'a str,
        end_marker: &'a str,
        timeout: Duration,
        max_output_size: usize,
    ) -> ResourceSamplerFuture<'a, Result<String, ResourceSampleError>> {
        Box::pin(async move {
            let line = if self.init.is_empty() {
                local_sample_line(command, self.pwsh)
            } else {
                format!("{}; {command}", self.init)
            };
            let output = run_local_command_capture(&line, timeout, max_output_size)
                .await
                .map_err(|()| ResourceSampleError::Failed("Local sample capture failed.".into()))?;
            // A local sample owns a fresh process, so a truncated read never
            // leaves the channel unusable; only the sample itself is dropped.
            if output.truncated {
                return Err(ResourceSampleError::OutputLimit {
                    channel_reusable: true,
                });
            }
            if output.exit_code.is_some_and(|code| code != 0) {
                return Err(ResourceSampleError::Failed(
                    "Local sample command failed.".into(),
                ));
            }
            if !output
                .stdout
                .lines()
                .any(|line| line.trim_end_matches('\r') == end_marker)
            {
                return Err(ResourceSampleError::Failed(
                    "Local sample ended without its completion marker.".into(),
                ));
            }
            Ok(output.stdout)
        })
    }

    fn close<'a>(&'a mut self) -> ResourceSamplerFuture<'a, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
}

#[cfg(windows)]
const LOCAL_HOST_COMMAND_CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn local_shell_command(line: &str) -> tokio::process::Command {
    #[cfg(windows)]
    {
        // Captures run from the GUI process, which has no console. Without
        // this flag every command flashes a console window for cmd.exe,
        // powershell.exe, and their children.
        // `COMSPEC` is the shell the session already owns; resolving `cmd`
        // through the CreateProcess search order would let an executable
        // beside the app shadow it.
        let shell = std::env::var_os("COMSPEC")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| std::ffi::OsString::from(r"C:\Windows\System32\cmd.exe"));
        let mut process = tokio::process::Command::new(shell);
        // The generated line already carries the quoting its own shell expects
        // (`-Command "..."`). `arg` would escape those quotes for the C
        // runtime and cmd would hand PowerShell a literal quoted string, so the
        // line is appended verbatim instead.
        process.arg("/C");
        process.raw_arg(line);
        process.creation_flags(LOCAL_HOST_COMMAND_CREATE_NO_WINDOW);
        process
    }
    #[cfg(not(windows))]
    {
        let mut process = tokio::process::Command::new("sh");
        process.arg("-c").arg(line);
        // A process-group leader lets the timeout signal every descendant.
        process.process_group(0);
        process
    }
}

/// Owns one capture's process and ends its tree on cancellation.
///
/// The shell is the only process this runner knows, so the tree has to be
/// addressed through the platform: a job object on Windows would need the child
/// spawned suspended, and the process group is enough on Unix. `kill_on_drop`
/// alone leaves a `powershell` CIM call running after `cmd.exe` dies, so this
/// guard repeats the timeout path when the future is dropped.
struct LocalCaptureChild {
    child: tokio::process::Child,
    /// Cleared once the capture ends normally, so a finished child is not
    /// killed again on drop.
    armed: bool,
}

impl LocalCaptureChild {
    fn new(child: tokio::process::Child) -> Self {
        Self { child, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }

    async fn kill_tree(&mut self) {
        self.armed = false;
        kill_local_command_tree(&mut self.child).await;
    }
}

impl Drop for LocalCaptureChild {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        // Drop cannot await, so the platform kill is issued without waiting.
        #[cfg(windows)]
        if let Some(pid) = self.child.id() {
            use std::os::windows::process::CommandExt;
            let mut command = std::process::Command::new("taskkill");
            command
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .creation_flags(LOCAL_HOST_COMMAND_CREATE_NO_WINDOW);
            let _ = command.spawn();
        }
        #[cfg(not(windows))]
        if let Some(pid) = self.child.id() {
            let _ = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
        }
        let _ = self.child.start_kill();
    }
}

/// Ends the capture and everything it started.
#[cfg(windows)]
async fn kill_local_command_tree(child: &mut tokio::process::Child) {
    if let Some(pid) = child.id() {
        let mut command = tokio::process::Command::new("taskkill");
        command
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(LOCAL_HOST_COMMAND_CREATE_NO_WINDOW);
        let _ = command.status().await;
    }
    let _ = child.start_kill();
}

#[cfg(not(windows))]
async fn kill_local_command_tree(child: &mut tokio::process::Child) {
    if let Some(pid) = child.id() {
        // `local_shell_command` made the shell a process-group leader.
        let _ = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
    }
    let _ = child.start_kill();
}

/// Drains a stream to EOF while retaining at most the shared budget, so a
/// large capture cannot block the child on a full pipe.
async fn read_capped<R>(mut reader: R, budget: Arc<AtomicUsize>) -> (Vec<u8>, bool)
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut output = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut truncated = false;
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                let keep = match budget.try_update(Ordering::Relaxed, Ordering::Relaxed, |left| {
                    Some(left.saturating_sub(read))
                }) {
                    Ok(previous) => previous.min(read),
                    Err(_) => 0,
                };
                if keep > 0 {
                    output.extend_from_slice(&chunk[..keep]);
                }
                if keep < read {
                    truncated = true;
                }
            }
        }
    }
    (output, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_sample_interpreter_swap_only_touches_powershell_commands() {
        let sample = "powershell -NoProfile -ExecutionPolicy Bypass -Command \"& ([scriptblock]::Create([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('W0NvbnNvbGVd'))))\"\r\n";
        // The pwsh swap exists to avoid Windows PowerShell 5.1, so on other
        // platforms the line must pass through unchanged.
        if cfg!(windows) {
            assert_eq!(
                local_sample_line(sample, true),
                sample.replacen("powershell", "pwsh", 1)
            );
        } else {
            assert_eq!(local_sample_line(sample, true), sample);
        }
        // Without pwsh installed the generated line must stay byte-identical.
        assert_eq!(local_sample_line(sample, false), sample);
        // Non-PowerShell samples (e.g. the GPU vendor probes) stay unchanged.
        let vendor = "nvidia-smi --query-gpu=name --format=csv,noheader\n";
        assert_eq!(local_sample_line(vendor, true), vendor);
    }

    #[tokio::test]
    async fn local_capture_reports_output_and_exit_code() {
        let command = "echo oxide-local";
        let output = run_local_command_capture(command, Duration::from_secs(10), 4096)
            .await
            .expect("local capture");
        assert_eq!(output.stdout.trim(), "oxide-local");
        assert_eq!(output.exit_code, Some(0));
        assert!(!output.truncated);
    }

    #[tokio::test]
    async fn local_capture_truncates_at_the_shared_budget() {
        let command = if cfg!(windows) {
            "for /L %i in (1,1,2000) do @echo 0123456789"
        } else {
            "i=0; while [ $i -lt 2000 ]; do echo 0123456789; i=$((i+1)); done"
        };
        let output = run_local_command_capture(command, Duration::from_secs(30), 64)
            .await
            .expect("local capture");
        assert!(output.truncated);
        assert!(output.stdout.len() + output.stderr.len() <= 64);
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn local_capture_preserves_embedded_double_quotes() {
        // Generated Windows commands carry `-Command "..."` scripts. Escaping
        // the embedded quotes a second time made cmd hand PowerShell a literal
        // string, which PowerShell printed instead of running.
        let output = run_local_command_capture(
            "powershell -NoProfile -NonInteractive -Command \"Write-Output ('oxide'+'-quoted')\"",
            Duration::from_secs(30),
            4096,
        )
        .await
        .expect("local capture");
        assert_eq!(output.stdout.trim(), "oxide-quoted");
    }
}
