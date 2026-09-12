use std::{io, process::Stdio};
use tokio::io::AsyncReadExt;

/// The task owns this process tree; shared terminal shells never enter this type.
pub struct AgentProcess {
    child: tokio::process::Child,
    record: Option<super::AgentResourceRecord>,
    #[cfg(unix)]
    group: i32,
    #[cfg(windows)]
    job: ProcessJob,
}

impl AgentProcess {
    pub fn spawn(command: &mut tokio::process::Command) -> io::Result<Self> {
        command
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        command.process_group(0);
        #[cfg(windows)]
        command.creation_flags(0x08000000 | 0x00000004);
        let child = command.spawn()?;
        let pid = child
            .id()
            .ok_or_else(|| io::Error::other("Child process has no identity"))?;
        #[cfg(windows)]
        let job = ProcessJob::attach_and_resume(pid).map_err(io::Error::other)?;
        Ok(Self {
            child,
            record: None,
            #[cfg(unix)]
            group: pid as i32,
            #[cfg(windows)]
            job,
        })
    }

    pub fn track(mut self, record: Option<super::AgentResourceRecord>) -> Self {
        self.record = record;
        self
    }

    pub async fn output(mut self) -> io::Result<std::process::Output> {
        let mut stdout = self.child.stdout.take().unwrap();
        let mut stderr = self.child.stderr.take().unwrap();
        let mut out = zeroize::Zeroizing::new(Vec::new());
        let mut err = zeroize::Zeroizing::new(Vec::new());
        #[cfg(unix)]
        {
            let group = self.group;
            let exited = async {
                loop {
                    if process_exited(group)? {
                        // Keep the shell unreaped until descendants are terminated; its PID cannot be reused.
                        unsafe {
                            libc::kill(-group, libc::SIGKILL);
                        }
                        return Ok::<(), io::Error>(());
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            };
            tokio::try_join!(
                exited,
                capture_output(&mut stdout, &mut out),
                capture_output(&mut stderr, &mut err)
            )?;
            self.group = 0;
            let status = self.child.wait().await?;
            if let Some(record) = self.record.take() {
                record.finish(super::OwnedResourceState::Completed);
            }
            Ok(std::process::Output {
                status,
                stdout: std::mem::take(&mut *out),
                stderr: std::mem::take(&mut *err),
            })
        }
        #[cfg(not(unix))]
        {
            let (status, _, _) = tokio::try_join!(
                self.child.wait(),
                capture_output(&mut stdout, &mut out),
                capture_output(&mut stderr, &mut err)
            )?;
            if let Some(record) = self.record.take() {
                record.finish(super::OwnedResourceState::Completed);
            }
            Ok(std::process::Output {
                status,
                stdout: std::mem::take(&mut *out),
                stderr: std::mem::take(&mut *err),
            })
        }
    }
}

async fn capture_output(
    reader: &mut (impl tokio::io::AsyncRead + Unpin),
    output: &mut Vec<u8>,
) -> io::Result<()> {
    const MAX_CAPTURE_BYTES: usize = 1024 * 1024;
    let mut chunk = zeroize::Zeroizing::new(vec![0u8; 16 * 1024]);
    loop {
        let count = reader.read(&mut chunk).await?;
        if count == 0 {
            return Ok(());
        }
        if output.len() + count > MAX_CAPTURE_BYTES {
            let discard = output.len() + count - MAX_CAPTURE_BYTES;
            output[..discard].fill(0);
            output.drain(..discard);
        }
        output.extend_from_slice(&chunk[..count]);
    }
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        // The independent group/job belongs to this command, never to the application's shell.
        #[cfg(unix)]
        unsafe {
            if self.group > 0 {
                let sent = libc::kill(-self.group, libc::SIGKILL) == 0;
                if sent {
                    // Cleanup runs on the command worker, with a bounded wait for reaping.
                    for _ in 0..25 {
                        if self.child.try_wait().ok().flatten().is_some() {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
                if libc::kill(-self.group, 0) < 0
                    && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
                {
                    if let Some(record) = self.record.take() {
                        record.finish(super::OwnedResourceState::Stopped);
                    }
                }
            }
        }
        #[cfg(windows)]
        if self.job.terminate() {
            if let Some(record) = self.record.take() {
                record.finish(super::OwnedResourceState::Stopped);
            }
        }
    }
}

#[cfg(windows)]
struct ProcessJob(windows::Win32::Foundation::HANDLE);
// Job handles may move between workers; their final owner closes the handle.
#[cfg(windows)]
unsafe impl Send for ProcessJob {}

#[cfg(windows)]
impl ProcessJob {
    fn attach_and_resume(pid: u32) -> windows::core::Result<Self> {
        use windows::Win32::{
            Foundation::CloseHandle,
            System::{Diagnostics::ToolHelp::*, JobObjects::*, Threading::*},
        };
        unsafe {
            let job = Self(CreateJobObjectW(None, None)?);
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )?;
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid)?;
            let assigned = AssignProcessToJobObject(job.0, process);
            let _ = CloseHandle(process);
            assigned?;
            // Assign the suspended child before it can create descendants outside the job.
            let threads = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)?;
            let mut entry = THREADENTRY32 {
                dwSize: size_of::<THREADENTRY32>() as u32,
                ..Default::default()
            };
            let mut next = Thread32First(threads, &mut entry);
            let mut resumed = false;
            while next.is_ok() {
                if entry.th32OwnerProcessID == pid {
                    if let Ok(thread) = OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID)
                    {
                        resumed = ResumeThread(thread) != u32::MAX;
                        let _ = CloseHandle(thread);
                    }
                    break;
                }
                next = Thread32Next(threads, &mut entry);
            }
            let _ = CloseHandle(threads);
            if !resumed {
                return Err(windows::core::Error::from_win32());
            }
            Ok(job)
        }
    }

    fn terminate(&self) -> bool {
        use windows::Win32::System::JobObjects::*;
        unsafe {
            if TerminateJobObject(self.0, 1).is_err() {
                return false;
            }
            for _ in 0..25 {
                let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
                if QueryInformationJobObject(
                    Some(self.0),
                    JobObjectBasicAccountingInformation,
                    &mut info as *mut _ as _,
                    size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    None,
                )
                .is_err()
                {
                    return false;
                }
                if info.ActiveProcesses == 0 {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            false
        }
    }
}

#[cfg(windows)]
impl Drop for ProcessJob {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(unix)]
fn process_exited(pid: i32) -> io::Result<bool> {
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            pid as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { info.si_pid() } != 0)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn command_output_keeps_exit_status_and_cancel_kills_its_descendants() {
        let mut command = tokio::process::Command::new("sh");
        command.args(["-c", "printf output; printf error >&2; exit 7"]);
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            AgentProcess::spawn(&mut command).unwrap().output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            (output.status.code(), output.stdout, output.stderr),
            (Some(7), b"output".to_vec(), b"error".to_vec())
        );
        let temp = tempfile::tempdir().unwrap();
        let pid_file = temp.path().join("pid");
        let mut command = tokio::process::Command::new("sh");
        command
            .args(["-c", r#"sleep 60 & printf '%s' $! > "$1"; wait"#, "test"])
            .arg(&pid_file);
        let process = AgentProcess::spawn(&mut command).unwrap();
        let root = process.group;
        let task = tokio::spawn(process.output());
        let child = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if let Ok(value) = std::fs::read_to_string(&pid_file) {
                    if let Ok(pid) = value.parse::<i32>() {
                        break pid;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(unsafe { libc::getpgid(child) }, root);
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while unsafe { libc::kill(child, 0) } == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("descendant must exit after cancellation");
        assert_ne!(unsafe { libc::kill(root, 0) }, 0);
    }
}
