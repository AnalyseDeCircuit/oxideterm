// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::Command,
};

pub const WINDOWS_UPDATE_STAGING_DIR: &str = "install";
pub const WINDOWS_UPDATE_OLD_DIR: &str = "old";
pub const WINDOWS_UPDATE_HELPER_RELATIVE: &str = "tools/oxideterm-update-helper.exe";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsUpdateHelperOptions {
    pub install_dir: PathBuf,
    pub app_exe: PathBuf,
    pub wait_pid: Option<u32>,
    pub launch_after_apply: bool,
}

pub fn windows_update_helper_path(current_exe: &Path) -> Option<PathBuf> {
    current_exe
        .parent()
        .map(|install_dir| install_dir.join(WINDOWS_UPDATE_HELPER_RELATIVE))
}

pub fn windows_update_helper_arguments(options: &WindowsUpdateHelperOptions) -> Vec<String> {
    let mut args = vec![
        "--install-dir".to_string(),
        options.install_dir.to_string_lossy().into_owned(),
        "--app-exe".to_string(),
        options.app_exe.to_string_lossy().into_owned(),
    ];
    if let Some(wait_pid) = options.wait_pid {
        args.push("--wait-pid".to_string());
        args.push(wait_pid.to_string());
    }
    if options.launch_after_apply {
        args.push("--launch".to_string());
    }
    args
}

pub fn parse_windows_update_helper_options<I, S>(
    args: I,
) -> Result<WindowsUpdateHelperOptions, String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut install_dir = None;
    let mut app_exe = None;
    let mut wait_pid = None;
    let mut launch_after_apply = false;
    let mut iter = args.into_iter().map(Into::into).skip(1);

    while let Some(arg) = iter.next() {
        let arg = arg.to_string_lossy();
        match arg.as_ref() {
            "--install-dir" => {
                install_dir = Some(PathBuf::from(required_arg(&mut iter, "--install-dir")?));
            }
            "--app-exe" => {
                app_exe = Some(PathBuf::from(required_arg(&mut iter, "--app-exe")?));
            }
            "--wait-pid" => {
                let raw = required_arg(&mut iter, "--wait-pid")?;
                wait_pid = Some(
                    raw.to_string_lossy()
                        .parse::<u32>()
                        .map_err(|error| format!("invalid --wait-pid value: {error}"))?,
                );
            }
            "--launch" => launch_after_apply = true,
            unknown => return Err(format!("unknown update helper argument: {unknown}")),
        }
    }

    Ok(WindowsUpdateHelperOptions {
        install_dir: install_dir.ok_or_else(|| "missing --install-dir".to_string())?,
        app_exe: app_exe.ok_or_else(|| "missing --app-exe".to_string())?,
        wait_pid,
        launch_after_apply,
    })
}

fn required_arg<I>(iter: &mut I, name: &str) -> Result<OsString, String>
where
    I: Iterator<Item = OsString>,
{
    iter.next()
        .ok_or_else(|| format!("missing value for {name}"))
}

pub fn run_windows_update_helper(options: WindowsUpdateHelperOptions) -> Result<(), String> {
    if let Some(wait_pid) = options.wait_pid {
        wait_for_process_exit(wait_pid);
    }

    #[cfg(windows)]
    release_windows_restart_manager_handles(&options.app_exe);

    apply_staged_windows_update(&options.install_dir).map_err(|error| {
        format!(
            "apply staged Windows update in {} failed: {error}",
            options.install_dir.display()
        )
    })?;

    if options.launch_after_apply {
        launch_updated_app(&options.app_exe).map_err(|error| {
            format!(
                "launch updated Windows app {} failed: {error}",
                options.app_exe.display()
            )
        })?;
    }

    Ok(())
}

pub fn apply_staged_windows_update(install_dir: &Path) -> io::Result<()> {
    let staging_dir = install_dir.join(WINDOWS_UPDATE_STAGING_DIR);
    let old_dir = install_dir.join(WINDOWS_UPDATE_OLD_DIR);
    if !staging_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("staging directory not found: {}", staging_dir.display()),
        ));
    }

    if old_dir.exists() {
        remove_path(&old_dir)?;
    }
    fs::create_dir_all(&old_dir)?;

    let jobs = collect_replacement_jobs(install_dir, &staging_dir, &old_dir)?;
    let mut applied = Vec::new();
    for mut job in jobs {
        if let Err(error) = apply_replacement_job(&mut job) {
            let _ = rollback_replacement_jobs(applied);
            let _ = rollback_replacement_jobs(vec![job]);
            return Err(error);
        }
        applied.push(job);
    }

    remove_path(&staging_dir)?;
    remove_path(&old_dir)?;
    Ok(())
}

#[derive(Debug)]
struct ReplacementJob {
    source: PathBuf,
    target: PathBuf,
    backup: PathBuf,
    target_existed: bool,
    installed: bool,
}

fn collect_replacement_jobs(
    install_dir: &Path,
    staging_dir: &Path,
    old_dir: &Path,
) -> io::Result<Vec<ReplacementJob>> {
    let mut jobs = Vec::new();
    for entry in sorted_read_dir(staging_dir)? {
        let source = entry.path();
        let relative = entry.file_name();
        if relative == OsString::from("tools") {
            collect_tool_replacement_jobs(install_dir, staging_dir, old_dir, &source, &mut jobs)?;
            continue;
        }
        push_replacement_job(install_dir, staging_dir, old_dir, &source, &mut jobs)?;
    }
    Ok(jobs)
}

fn collect_tool_replacement_jobs(
    install_dir: &Path,
    staging_dir: &Path,
    old_dir: &Path,
    tools_dir: &Path,
    jobs: &mut Vec<ReplacementJob>,
) -> io::Result<()> {
    let mut pending = vec![tools_dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in sorted_read_dir(&dir)? {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let relative = relative_path(staging_dir, &path)?;
            if relative.as_path() == Path::new(WINDOWS_UPDATE_HELPER_RELATIVE) {
                continue;
            }
            push_replacement_job(install_dir, staging_dir, old_dir, &path, jobs)?;
        }
    }
    Ok(())
}

fn push_replacement_job(
    install_dir: &Path,
    staging_dir: &Path,
    old_dir: &Path,
    source: &Path,
    jobs: &mut Vec<ReplacementJob>,
) -> io::Result<()> {
    let relative = relative_path(staging_dir, source)?;
    jobs.push(ReplacementJob {
        source: source.to_path_buf(),
        target: install_dir.join(&relative),
        backup: old_dir.join(relative),
        target_existed: false,
        installed: false,
    });
    Ok(())
}

fn apply_replacement_job(job: &mut ReplacementJob) -> io::Result<()> {
    if let Some(parent) = job.target.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = job.backup.parent() {
        fs::create_dir_all(parent)?;
    }

    if job.backup.exists() {
        remove_path(&job.backup)?;
    }
    if job.target.exists() {
        fs::rename(&job.target, &job.backup)?;
        job.target_existed = true;
    }
    fs::rename(&job.source, &job.target)?;
    job.installed = true;
    Ok(())
}

fn rollback_replacement_jobs(mut jobs: Vec<ReplacementJob>) -> io::Result<()> {
    let mut first_error = None;
    while let Some(job) = jobs.pop() {
        if job.installed && job.target.exists() {
            capture_first_error(&mut first_error, remove_path(&job.target));
        }
        if job.target_existed && job.backup.exists() {
            if let Some(parent) = job.target.parent() {
                capture_first_error(&mut first_error, fs::create_dir_all(parent));
            }
            capture_first_error(&mut first_error, fs::rename(&job.backup, &job.target));
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn capture_first_error(first_error: &mut Option<io::Error>, result: io::Result<()>) {
    if first_error.is_none() {
        if let Err(error) = result {
            *first_error = Some(error);
        }
    }
}

fn sorted_read_dir(dir: &Path) -> io::Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    Ok(entries)
}

fn relative_path(root: &Path, path: &Path) -> io::Result<PathBuf> {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .map_err(|error| io::Error::other(format!("build relative update path failed: {error}")))
}

fn remove_path(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn launch_updated_app(app_exe: &Path) -> io::Result<()> {
    let mut command = Command::new(app_exe);
    configure_helper_child_process(&mut command);
    command.spawn()?;
    Ok(())
}

#[cfg(windows)]
fn configure_helper_child_process(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure_helper_child_process(_command: &mut Command) {}

#[cfg(windows)]
fn wait_for_process_exit(pid: u32) {
    use windows::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };

    // Waiting is best-effort: if the process is already gone or inaccessible,
    // the replacement path still proceeds and relies on filesystem errors.
    let Ok(handle) = (unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }) else {
        return;
    };
    loop {
        let wait_result = unsafe { WaitForSingleObject(handle, 1000) };
        if wait_result == WAIT_OBJECT_0 {
            break;
        }
    }
    let _ = unsafe { CloseHandle(handle) };
}

#[cfg(not(windows))]
fn wait_for_process_exit(_pid: u32) {
    std::thread::sleep(std::time::Duration::from_millis(1));
}

#[cfg(windows)]
fn release_windows_restart_manager_handles(app_exe: &Path) {
    use std::os::windows::ffi::OsStrExt as _;
    use windows::{
        Win32::{
            Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS},
            System::RestartManager::{
                CCH_RM_SESSION_KEY, RM_PROCESS_INFO, RmEndSession, RmForceShutdown, RmGetList,
                RmRegisterResources, RmShutdown, RmShutdownOnlyRegistered, RmStartSession,
            },
        },
        core::{PCWSTR, PWSTR},
    };

    let mut session = 0u32;
    let mut session_key = vec![0u16; CCH_RM_SESSION_KEY as usize + 1];
    let started = unsafe { RmStartSession(&mut session, None, PWSTR(session_key.as_mut_ptr())) };
    if started != ERROR_SUCCESS {
        return;
    }

    let wide_path = app_exe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let resources = [PCWSTR(wide_path.as_ptr())];
    let registered = unsafe { RmRegisterResources(session, Some(&resources), None, None) };
    if registered == ERROR_SUCCESS {
        let mut needed = 0u32;
        let mut count = 0u32;
        let mut reboot_reasons = 0u32;
        let listed =
            unsafe { RmGetList(session, &mut needed, &mut count, None, &mut reboot_reasons) };
        if listed == ERROR_MORE_DATA && needed > 0 {
            let mut affected = vec![RM_PROCESS_INFO::default(); needed as usize];
            count = needed;
            let listed = unsafe {
                RmGetList(
                    session,
                    &mut needed,
                    &mut count,
                    Some(affected.as_mut_ptr()),
                    &mut reboot_reasons,
                )
            };
            if listed == ERROR_SUCCESS && count > 0 {
                let flags = (RmForceShutdown.0 | RmShutdownOnlyRegistered.0) as u32;
                let _ = unsafe { RmShutdown(session, flags, None) };
            }
        }
    }

    let _ = unsafe { RmEndSession(session) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_arguments_round_trip() {
        let options = WindowsUpdateHelperOptions {
            install_dir: PathBuf::from("C:/Users/me/AppData/Local/OxideTerm"),
            app_exe: PathBuf::from("C:/Users/me/AppData/Local/OxideTerm/oxideterm-native.exe"),
            wait_pid: Some(42),
            launch_after_apply: true,
        };
        let mut args = vec![OsString::from("oxideterm-update-helper.exe")];
        args.extend(
            windows_update_helper_arguments(&options)
                .into_iter()
                .map(OsString::from),
        );

        assert_eq!(parse_windows_update_helper_options(args).unwrap(), options);
    }

    #[test]
    fn staged_update_replaces_files_and_skips_running_helper() {
        let temp = tempfile::tempdir().unwrap();
        let install_dir = temp.path();
        fs::create_dir_all(install_dir.join("resources")).unwrap();
        fs::create_dir_all(install_dir.join("tools")).unwrap();
        fs::write(install_dir.join("oxideterm-native.exe"), "old app").unwrap();
        fs::write(install_dir.join("resources/config.json"), "old config").unwrap();
        fs::write(
            install_dir.join(WINDOWS_UPDATE_HELPER_RELATIVE),
            "old helper",
        )
        .unwrap();

        let staging = install_dir.join(WINDOWS_UPDATE_STAGING_DIR);
        fs::create_dir_all(staging.join("resources")).unwrap();
        fs::create_dir_all(staging.join("tools")).unwrap();
        fs::write(staging.join("oxideterm-native.exe"), "new app").unwrap();
        fs::write(staging.join("resources/config.json"), "new config").unwrap();
        fs::write(staging.join(WINDOWS_UPDATE_HELPER_RELATIVE), "new helper").unwrap();

        apply_staged_windows_update(install_dir).unwrap();

        assert_eq!(
            fs::read_to_string(install_dir.join("oxideterm-native.exe")).unwrap(),
            "new app"
        );
        assert_eq!(
            fs::read_to_string(install_dir.join("resources/config.json")).unwrap(),
            "new config"
        );
        assert_eq!(
            fs::read_to_string(install_dir.join(WINDOWS_UPDATE_HELPER_RELATIVE)).unwrap(),
            "old helper"
        );
        assert!(!staging.exists());
        assert!(!install_dir.join(WINDOWS_UPDATE_OLD_DIR).exists());
    }

    #[test]
    fn staged_update_rolls_back_when_a_move_fails() {
        let temp = tempfile::tempdir().unwrap();
        let install_dir = temp.path();
        let old_dir = install_dir.join(WINDOWS_UPDATE_OLD_DIR);
        let staging = install_dir.join(WINDOWS_UPDATE_STAGING_DIR);
        fs::create_dir_all(&old_dir).unwrap();
        fs::create_dir_all(&staging).unwrap();
        let app_target = install_dir.join("oxideterm-native.exe");
        let app_source = staging.join("oxideterm-native.exe");
        fs::write(&app_target, "old app").unwrap();
        fs::write(&app_source, "new app").unwrap();

        let mut applied = ReplacementJob {
            source: app_source,
            target: app_target.clone(),
            backup: old_dir.join("oxideterm-native.exe"),
            target_existed: false,
            installed: false,
        };
        apply_replacement_job(&mut applied).unwrap();

        let mut failing = ReplacementJob {
            source: staging.join("missing-resource.json"),
            target: install_dir.join("resources/config.json"),
            backup: old_dir.join("resources/config.json"),
            target_existed: false,
            installed: false,
        };
        let error = apply_replacement_job(&mut failing).unwrap_err();
        rollback_replacement_jobs(vec![applied]).unwrap();

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(fs::read_to_string(app_target).unwrap(), "old app");
    }
}
