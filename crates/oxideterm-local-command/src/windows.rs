// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr,
};

use windows::{
    Win32::{
        Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
        Security::SECURITY_ATTRIBUTES,
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::{
            JobObjects::{CreateJobObjectW, TerminateJobObject},
            SystemInformation::GetSystemDirectoryW,
            Threading::{
                CREATE_NO_WINDOW, CreateProcessW, DeleteProcThreadAttributeList,
                EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
                InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST,
                PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW,
                UpdateProcThreadAttribute, WaitForSingleObject,
            },
        },
    },
    core::{PCWSTR, PWSTR, w},
};
use zeroize::Zeroizing;

pub(super) struct RunningCommand {
    child: OwnedHandle,
    job: OwnedHandle,
    succeeded: bool,
}

fn raw(handle: &OwnedHandle) -> HANDLE {
    HANDLE(handle.as_raw_handle())
}

struct Attributes(Vec<usize>);

impl Attributes {
    fn new() -> windows::core::Result<Self> {
        let mut bytes = 0;
        // The first call returns the required storage size.
        let _ = unsafe { InitializeProcThreadAttributeList(None, 2, None, &mut bytes) };
        let mut words = vec![0; bytes.div_ceil(size_of::<usize>())];
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(words.as_mut_ptr().cast());
        unsafe { InitializeProcThreadAttributeList(Some(list), 2, None, &mut bytes)? };
        Ok(Self(words))
    }

    fn list(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        LPPROC_THREAD_ATTRIBUTE_LIST(self.0.as_mut_ptr().cast())
    }

    fn set_handle(&mut self, attribute: u32, handle: &HANDLE) -> windows::core::Result<()> {
        unsafe {
            UpdateProcThreadAttribute(
                self.list(),
                0,
                attribute as usize,
                Some(ptr::from_ref(handle).cast()),
                size_of::<HANDLE>(),
                None,
                None,
            )
        }
    }
}

impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.list()) };
    }
}

impl RunningCommand {
    pub(super) fn spawn(command: &str) -> io::Result<Self> {
        if command.contains('\0') {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Self::spawn_windows(command).map_err(|_| io::ErrorKind::Other.into())
    }

    fn spawn_windows(command: &str) -> windows::core::Result<Self> {
        let mut system_dir = [0; 260];
        let len = unsafe { GetSystemDirectoryW(Some(&mut system_dir)) } as usize;
        if len == 0 || len >= system_dir.len() {
            return Err(windows::core::Error::from_win32());
        }
        let mut application = system_dir[..len].to_vec();
        application.extend("\\cmd.exe\0".encode_utf16());
        let mut command_line = Zeroizing::new(Vec::new());
        command_line.extend("cmd.exe /D /S /C \"".encode_utf16());
        command_line.extend(command.encode_utf16());
        command_line.extend(['"' as u16, 0]);

        // Attach the job at creation time; the shell cannot start a child
        // before the job owns it. Only the NUL handle can be inherited.
        let inherit = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            bInheritHandle: true.into(),
            ..Default::default()
        };
        let nul = unsafe {
            let handle = CreateFileW(
                w!("NUL"),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                Some(&inherit),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )?;
            OwnedHandle::from_raw_handle(handle.0)
        };
        let job =
            unsafe { OwnedHandle::from_raw_handle(CreateJobObjectW(None, PCWSTR::null())?.0) };
        let nul_handle = raw(&nul);
        let job_handle = raw(&job);
        let mut attributes = Attributes::new()?;
        attributes.set_handle(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &nul_handle)?;
        attributes.set_handle(PROC_THREAD_ATTRIBUTE_JOB_LIST, &job_handle)?;

        let mut info = STARTUPINFOEXW::default();
        info.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        info.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        info.StartupInfo.hStdInput = nul_handle;
        info.StartupInfo.hStdOutput = nul_handle;
        info.StartupInfo.hStdError = nul_handle;
        info.lpAttributeList = attributes.list();
        let mut result = PROCESS_INFORMATION::default();
        let child = unsafe {
            CreateProcessW(
                PCWSTR(application.as_ptr()),
                Some(PWSTR(command_line.as_mut_ptr())),
                None,
                None,
                true,
                CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT,
                None,
                PCWSTR::null(),
                &info.StartupInfo,
                &mut result,
            )?;
            let _thread = OwnedHandle::from_raw_handle(result.hThread.0);
            OwnedHandle::from_raw_handle(result.hProcess.0)
        };
        Ok(Self {
            child,
            job,
            succeeded: false,
        })
    }

    pub(super) fn try_wait(&mut self) -> io::Result<Option<i32>> {
        match unsafe { WaitForSingleObject(raw(&self.child), 0) } {
            WAIT_OBJECT_0 => {
                let mut code = 0;
                unsafe { GetExitCodeProcess(raw(&self.child), &mut code) }
                    .map_err(|_| io::Error::last_os_error())?;
                self.succeeded = code == 0;
                Ok(Some(code as i32))
            }
            WAIT_TIMEOUT => Ok(None),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

impl Drop for RunningCommand {
    fn drop(&mut self) {
        if !self.succeeded {
            let _ = unsafe { TerminateJobObject(raw(&self.job), 1) };
        }
    }
}
