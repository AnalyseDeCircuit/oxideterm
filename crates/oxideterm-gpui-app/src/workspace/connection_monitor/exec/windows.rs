use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{Diagnostics::ToolHelp::*, JobObjects::*, Threading::*},
};

pub(super) struct CaptureJob(HANDLE);

// The capture task is the sole owner of this process-wide kernel handle.
unsafe impl Send for CaptureJob {}

impl CaptureJob {
    pub(super) fn attach_and_resume(pid: u32) -> windows::core::Result<Self> {
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

            // Tokio exposes the process but not its primary thread handle.
            // The suspended process has not run user code or created other threads.
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

    pub(super) fn terminate(&self) {
        unsafe {
            let _ = TerminateJobObject(self.0, 1);
        }
    }
}

impl Drop for CaptureJob {
    fn drop(&mut self) {
        // Closing the last handle also ends descendants after a normal shell exit.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
