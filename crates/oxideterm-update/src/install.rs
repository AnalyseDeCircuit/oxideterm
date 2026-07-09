// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::{InstallFlavor, NativeUpdateError, PlatformTarget, current_platform_target};
#[cfg(target_os = "windows")]
use crate::{
    WindowsUpdateHelperOptions, windows_update_helper_arguments, windows_update_helper_path,
};

#[cfg(windows)]
const WINDOWS_BACKGROUND_PROCESS_CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallPackageKind {
    MacAppBundle,
    MacDmg,
    MacArchive,
    WindowsMsi,
    WindowsExe,
    WindowsInstallerArchive,
    LinuxAppImage,
    LinuxAppImageArchive,
    LinuxPackage,
    PortableArchive,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallStrategy {
    PortableManualReplacement,
    MacOpenDmg,
    MacReplaceAppBundle,
    MacReplaceAppArchive,
    WindowsRunInstaller,
    WindowsExtractAndRunInstaller,
    LinuxReplaceAppImage,
    LinuxReplaceAppImageArchive,
    LinuxOpenPackage,
    OpenPackage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallActionKind {
    Manual,
    OpenPackage,
    LaunchInstaller,
    LaunchReplacementScript,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeInstallContext {
    pub target: PlatformTarget,
    pub current_exe: PathBuf,
    pub process_id: u32,
    pub install_flavor: InstallFlavor,
}

impl NativeInstallContext {
    pub fn current(portable: bool) -> Result<Self, NativeUpdateError> {
        let target = current_platform_target();
        let current_exe = current_install_executable()?;
        Ok(Self {
            install_flavor: InstallFlavor::infer(&target, &current_exe, portable),
            target,
            current_exe,
            process_id: std::process::id(),
        })
    }
}

fn current_install_executable() -> Result<PathBuf, NativeUpdateError> {
    // AppImage processes usually run from a mounted inner path. APPIMAGE is the
    // user-visible file that must be replaced during an update.
    if cfg!(target_os = "linux") {
        if let Some(appimage) = std::env::var_os("APPIMAGE") {
            return Ok(PathBuf::from(appimage));
        }
    }
    std::env::current_exe().map_err(|error| {
        NativeUpdateError::State(format!("resolve current executable failed: {error}"))
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeInstallPlan {
    pub strategy: InstallStrategy,
    pub action: InstallActionKind,
    pub package_kind: InstallPackageKind,
    pub package_path: PathBuf,
    pub current_exe: PathBuf,
    pub process_id: u32,
    pub requires_app_exit: bool,
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeInstallStatus {
    ManualActionRequired,
    InstallerLaunched,
    ReplacementScheduled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeInstallOutcome {
    pub status: NativeInstallStatus,
    pub message: String,
    pub should_quit_app: bool,
}

pub fn plan_native_install(
    package_path: impl AsRef<Path>,
    context: &NativeInstallContext,
) -> NativeInstallPlan {
    let package_path = package_path.as_ref().to_path_buf();
    let package_kind = classify_package(&package_path);

    if context.install_flavor == InstallFlavor::Portable {
        return NativeInstallPlan {
            strategy: InstallStrategy::PortableManualReplacement,
            action: InstallActionKind::Manual,
            package_kind,
            package_path,
            current_exe: context.current_exe.clone(),
            process_id: context.process_id,
            requires_app_exit: true,
            summary: "Portable mode updates require replacing the whole portable folder."
                .to_string(),
        };
    }

    let (strategy, action, requires_app_exit, summary) =
        match (context.target.os(), context.install_flavor, package_kind) {
            ("macos", InstallFlavor::MacApp, InstallPackageKind::MacDmg) => (
                InstallStrategy::MacOpenDmg,
                InstallActionKind::OpenPackage,
                false,
                "Open the DMG and let Finder complete installation.",
            ),
            ("macos", InstallFlavor::MacApp, InstallPackageKind::MacAppBundle) => (
                InstallStrategy::MacReplaceAppBundle,
                InstallActionKind::LaunchReplacementScript,
                true,
                "Schedule .app bundle replacement after OxideTerm exits.",
            ),
            ("macos", InstallFlavor::MacApp, InstallPackageKind::MacArchive) => (
                InstallStrategy::MacReplaceAppArchive,
                InstallActionKind::LaunchReplacementScript,
                true,
                "Schedule archived .app bundle replacement after OxideTerm exits.",
            ),
            ("windows", InstallFlavor::WindowsNsis, InstallPackageKind::WindowsMsi) => (
                InstallStrategy::WindowsRunInstaller,
                InstallActionKind::LaunchInstaller,
                false,
                "Launch the Windows installer with elevation.",
            ),
            ("windows", InstallFlavor::WindowsNsis, InstallPackageKind::WindowsExe) => (
                InstallStrategy::WindowsRunInstaller,
                InstallActionKind::LaunchInstaller,
                true,
                "Stage the Windows installer and apply it after OxideTerm exits.",
            ),
            (
                "windows",
                InstallFlavor::WindowsNsis,
                InstallPackageKind::WindowsInstallerArchive,
            ) => (
                InstallStrategy::WindowsExtractAndRunInstaller,
                InstallActionKind::LaunchInstaller,
                false,
                "Extract the Windows update archive and launch its installer.",
            ),
            ("linux", InstallFlavor::LinuxAppImage, InstallPackageKind::LinuxAppImage) => (
                InstallStrategy::LinuxReplaceAppImage,
                InstallActionKind::LaunchReplacementScript,
                true,
                "Schedule AppImage replacement after OxideTerm exits.",
            ),
            ("linux", InstallFlavor::LinuxAppImage, InstallPackageKind::LinuxAppImageArchive) => (
                InstallStrategy::LinuxReplaceAppImageArchive,
                InstallActionKind::LaunchReplacementScript,
                true,
                "Extract and schedule AppImage replacement after OxideTerm exits.",
            ),
            ("linux", InstallFlavor::LinuxDeb, InstallPackageKind::LinuxPackage) => (
                InstallStrategy::LinuxOpenPackage,
                InstallActionKind::OpenPackage,
                false,
                "Open the Linux package with the desktop package installer.",
            ),
            _ => (
                InstallStrategy::OpenPackage,
                InstallActionKind::OpenPackage,
                false,
                "Open the update package for manual installation.",
            ),
        };

    NativeInstallPlan {
        strategy,
        action,
        package_kind,
        package_path,
        current_exe: context.current_exe.clone(),
        process_id: context.process_id,
        requires_app_exit,
        summary: summary.to_string(),
    }
}

pub fn execute_install_plan(
    plan: &NativeInstallPlan,
) -> Result<NativeInstallOutcome, NativeUpdateError> {
    match plan.strategy {
        InstallStrategy::PortableManualReplacement => Ok(NativeInstallOutcome {
            status: NativeInstallStatus::ManualActionRequired,
            message: plan.summary.clone(),
            should_quit_app: false,
        }),
        InstallStrategy::MacReplaceAppBundle | InstallStrategy::MacReplaceAppArchive => {
            execute_macos_app_replacement(plan)
        }
        InstallStrategy::LinuxReplaceAppImage | InstallStrategy::LinuxReplaceAppImageArchive => {
            execute_linux_appimage_replacement(plan)
        }
        InstallStrategy::WindowsRunInstaller => execute_windows_installer(plan),
        InstallStrategy::WindowsExtractAndRunInstaller => execute_windows_archive_installer(plan),
        InstallStrategy::MacOpenDmg
        | InstallStrategy::LinuxOpenPackage
        | InstallStrategy::OpenPackage => {
            open_package(&plan.package_path)?;
            Ok(NativeInstallOutcome {
                status: NativeInstallStatus::InstallerLaunched,
                message: plan.summary.clone(),
                should_quit_app: false,
            })
        }
    }
}

fn classify_package(path: &Path) -> InstallPackageKind {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if (file_name.contains("_portable.") || file_name.contains("-portable."))
        && (extension == "zip" || file_name.ends_with(".tar.gz") || extension == "tgz")
    {
        InstallPackageKind::PortableArchive
    } else if extension == "app" {
        InstallPackageKind::MacAppBundle
    } else if extension == "dmg" {
        InstallPackageKind::MacDmg
    } else if file_name.ends_with(".msi.zip") || file_name.ends_with(".exe.zip") {
        InstallPackageKind::WindowsInstallerArchive
    } else if file_name.ends_with(".appimage.tar.gz") || file_name.ends_with(".appimage.tgz") {
        InstallPackageKind::LinuxAppImageArchive
    } else if file_name.ends_with(".app.zip")
        || file_name.ends_with(".app.tar.gz")
        || file_name.ends_with(".tgz")
        || extension == "zip"
    {
        InstallPackageKind::MacArchive
    } else if extension == "msi" {
        InstallPackageKind::WindowsMsi
    } else if extension == "exe" {
        InstallPackageKind::WindowsExe
    } else if extension == "appimage" {
        InstallPackageKind::LinuxAppImage
    } else if matches!(extension.as_str(), "deb" | "rpm") || file_name.ends_with(".pkg.tar.zst") {
        InstallPackageKind::LinuxPackage
    } else {
        InstallPackageKind::Unknown
    }
}

fn open_package(path: &Path) -> Result<(), NativeUpdateError> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn().map_err(|error| {
            NativeUpdateError::State(format!("open update package failed: {error}"))
        })?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("cmd");
        configure_windows_background_process(&mut command);
        command.arg("/C").arg("start").arg("").arg(path);
        command.spawn().map_err(|error| {
            NativeUpdateError::State(format!("open update package failed: {error}"))
        })?;
        return Ok(());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|error| {
                NativeUpdateError::State(format!("open update package failed: {error}"))
            })?;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn execute_windows_installer(
    plan: &NativeInstallPlan,
) -> Result<NativeInstallOutcome, NativeUpdateError> {
    match plan.package_kind {
        InstallPackageKind::WindowsMsi => launch_windows_installer_via_powershell(
            "msiexec.exe",
            &[
                "/i".to_string(),
                plan.package_path.to_string_lossy().into_owned(),
                "/promptrestart".to_string(),
            ],
            &plan.package_path,
            false,
            true,
        )?,
        InstallPackageKind::WindowsExe => {
            launch_windows_installer_via_powershell(
                &plan.package_path.to_string_lossy(),
                &["/S".to_string(), "/OXIDETERM_UPDATE=1".to_string()],
                &plan.package_path,
                true,
                false,
            )?;
            launch_windows_update_helper(plan)?;
        }
        _ => open_package(&plan.package_path)?,
    }
    Ok(NativeInstallOutcome {
        status: match plan.package_kind {
            InstallPackageKind::WindowsExe => NativeInstallStatus::ReplacementScheduled,
            _ => NativeInstallStatus::InstallerLaunched,
        },
        message: windows_installer_launched_message(&plan.package_path, plan.package_kind),
        should_quit_app: true,
    })
}

#[cfg(any(target_os = "windows", test))]
fn windows_installer_launched_message(
    package_path: &Path,
    package_kind: InstallPackageKind,
) -> String {
    if package_kind == InstallPackageKind::WindowsExe {
        return "Windows update staged. OxideTerm will quit and the update helper will finish replacement."
            .to_string();
    }
    format!(
        "Windows installer launched with elevation. If setup is cancelled or fails, rerun the retained update package from: {}",
        package_path.display()
    )
}

#[cfg(any(target_os = "windows", test))]
fn powershell_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(any(target_os = "windows", test))]
fn windows_start_process_script(
    file_path: &str,
    arguments: &[String],
    wait: bool,
    elevate: bool,
) -> String {
    let mut script = format!(
        "Start-Process -FilePath {}",
        powershell_single_quoted(file_path)
    );
    if !arguments.is_empty() {
        let argument_list = arguments
            .iter()
            .map(|argument| powershell_single_quoted(argument))
            .collect::<Vec<_>>()
            .join(", ");
        script.push_str(&format!(" -ArgumentList @({argument_list})"));
    }
    if elevate {
        script.push_str(" -Verb RunAs");
    }
    if wait {
        script.push_str(" -Wait");
    }
    script
}

#[cfg(target_os = "windows")]
fn launch_windows_update_helper(plan: &NativeInstallPlan) -> Result<(), NativeUpdateError> {
    let helper_path = windows_update_helper_path(&plan.current_exe).ok_or_else(|| {
        NativeUpdateError::State("resolve Windows update helper path failed".to_string())
    })?;
    if !helper_path.is_file() {
        reveal_windows_update_package(&plan.package_path);
        return Err(NativeUpdateError::State(format!(
            "Windows update helper not found at {}; update package retained at {}",
            helper_path.display(),
            plan.package_path.display()
        )));
    }
    let install_dir = plan.current_exe.parent().ok_or_else(|| {
        NativeUpdateError::State("resolve Windows install directory failed".to_string())
    })?;
    let options = WindowsUpdateHelperOptions {
        install_dir: install_dir.to_path_buf(),
        app_exe: plan.current_exe.clone(),
        wait_pid: Some(plan.process_id),
        launch_after_apply: true,
    };
    let mut command = Command::new(helper_path);
    configure_windows_background_process(&mut command);
    command
        .args(windows_update_helper_arguments(&options))
        .spawn()
        .map_err(|error| {
            NativeUpdateError::State(format!("launch Windows update helper failed: {error}"))
        })?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn launch_windows_installer_via_powershell(
    file_path: &str,
    arguments: &[String],
    retained_package_path: &Path,
    wait: bool,
    elevate: bool,
) -> Result<(), NativeUpdateError> {
    // Per-user NSIS updates run without UAC. MSI fallback packages may still
    // request elevation, while the PowerShell bridge remains console-free.
    let script = windows_start_process_script(file_path, arguments, wait, elevate);
    let mut command = Command::new("powershell");
    configure_windows_background_process(&mut command);
    let status = command
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(script)
        .status()
        .map_err(|error| {
            reveal_windows_update_package(retained_package_path);
            NativeUpdateError::State(format!(
                "launch Windows installer failed: {error}; update package retained at {}",
                retained_package_path.display()
            ))
        })?;
    if !status.success() {
        reveal_windows_update_package(retained_package_path);
        return Err(NativeUpdateError::State(format!(
            "launch Windows installer was cancelled or failed; update package retained at {}",
            retained_package_path.display()
        )));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn configure_windows_background_process(command: &mut Command) {
    // PowerShell/cmd are launch bridges for hidden updater work. The installer
    // or Explorer can still show UI, but the bridge console must not flash.
    command.creation_flags(WINDOWS_BACKGROUND_PROCESS_CREATE_NO_WINDOW);
}

#[cfg(target_os = "windows")]
fn reveal_windows_update_package(path: &Path) {
    // Opening Explorer is best-effort guidance for manual retry. The update
    // result should still report the original launch failure if this also fails.
    let select_arg = format!("/select,{}", path.display());
    let _ = Command::new("explorer").arg(select_arg).spawn();
}

#[cfg(not(target_os = "windows"))]
fn execute_windows_installer(
    plan: &NativeInstallPlan,
) -> Result<NativeInstallOutcome, NativeUpdateError> {
    open_package(&plan.package_path)?;
    Ok(NativeInstallOutcome {
        status: NativeInstallStatus::InstallerLaunched,
        message: plan.summary.clone(),
        should_quit_app: false,
    })
}

#[cfg(target_os = "windows")]
fn execute_windows_archive_installer(
    plan: &NativeInstallPlan,
) -> Result<NativeInstallOutcome, NativeUpdateError> {
    let extract_dir = plan.package_path.with_extension("installer");
    std::fs::create_dir_all(&extract_dir).map_err(|error| {
        NativeUpdateError::State(format!(
            "create Windows installer directory failed: {error}"
        ))
    })?;
    let mut command = Command::new("powershell");
    configure_windows_background_process(&mut command);
    let status = command
        .arg("-NoProfile")
        .arg("-Command")
        .arg("Expand-Archive")
        .arg("-LiteralPath")
        .arg(&plan.package_path)
        .arg("-DestinationPath")
        .arg(&extract_dir)
        .arg("-Force")
        .status()
        .map_err(|error| {
            NativeUpdateError::State(format!("extract Windows installer archive failed: {error}"))
        })?;
    if !status.success() {
        return Err(NativeUpdateError::State(
            "extract Windows installer archive failed".to_string(),
        ));
    }

    let installer = find_windows_installer(&extract_dir).ok_or_else(|| {
        NativeUpdateError::State(
            "Windows installer archive did not contain .msi or .exe".to_string(),
        )
    })?;
    let package_kind = classify_package(&installer);
    let nested_plan = NativeInstallPlan {
        strategy: InstallStrategy::WindowsRunInstaller,
        action: InstallActionKind::LaunchInstaller,
        package_kind,
        package_path: installer,
        current_exe: plan.current_exe.clone(),
        process_id: plan.process_id,
        requires_app_exit: matches!(package_kind, InstallPackageKind::WindowsExe),
        summary: plan.summary.clone(),
    };
    execute_windows_installer(&nested_plan)
}

#[cfg(not(target_os = "windows"))]
fn execute_windows_archive_installer(
    plan: &NativeInstallPlan,
) -> Result<NativeInstallOutcome, NativeUpdateError> {
    open_package(&plan.package_path)?;
    Ok(NativeInstallOutcome {
        status: NativeInstallStatus::InstallerLaunched,
        message: plan.summary.clone(),
        should_quit_app: false,
    })
}

#[cfg(target_os = "windows")]
fn find_windows_installer(root: &Path) -> Option<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).ok()? {
            let path = entry.ok()?.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let extension = path
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or_default();
            if extension.eq_ignore_ascii_case("msi") || extension.eq_ignore_ascii_case("exe") {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn execute_linux_appimage_replacement(
    plan: &NativeInstallPlan,
) -> Result<NativeInstallOutcome, NativeUpdateError> {
    use std::os::unix::fs::PermissionsExt as _;

    let script_path = plan.package_path.with_extension("install.sh");
    let backup_path = plan.current_exe.with_extension("old");
    let install_source = match plan.package_kind {
        InstallPackageKind::LinuxAppImage => format!(
            r#"new_app="{package}""#,
            package = shell_escape_path(&plan.package_path)
        ),
        InstallPackageKind::LinuxAppImageArchive => format!(
            r#"tmp_dir="$(mktemp -d "${{TMPDIR:-/tmp}}/oxideterm-update.XXXXXX")"
tar -xzf "{package}" -C "$tmp_dir"
new_app="$(find "$tmp_dir" -maxdepth 3 -iname '*.AppImage' -type f | head -n 1)"
if [ -z "$new_app" ]; then
  exit 3
fi"#,
            package = shell_escape_path(&plan.package_path)
        ),
        _ => {
            open_package(&plan.package_path)?;
            return Ok(NativeInstallOutcome {
                status: NativeInstallStatus::InstallerLaunched,
                message: plan.summary.clone(),
                should_quit_app: false,
            });
        }
    };
    let script = format!(
        r#"#!/bin/sh
set -eu
while kill -0 {pid} 2>/dev/null; do
  sleep 0.2
done
{install_source}
chmod +x "$new_app"
rm -f "{backup}"
if [ -f "{current}" ]; then
  mv "{current}" "{backup}"
fi
if ! mv "$new_app" "{current}"; then
  if [ -f "{backup}" ]; then
    mv "{backup}" "{current}"
  fi
  exit 4
fi
rm -f "{backup}"
if [ -n "${{tmp_dir:-}}" ]; then
  rm -rf "$tmp_dir"
fi
"{current}" >/dev/null 2>&1 &
"#,
        pid = plan.process_id,
        install_source = install_source,
        current = shell_escape_path(&plan.current_exe),
        backup = shell_escape_path(&backup_path),
    );
    std::fs::write(&script_path, script).map_err(|error| {
        NativeUpdateError::State(format!("write AppImage installer failed: {error}"))
    })?;
    let mut permissions = std::fs::metadata(&script_path)
        .map_err(|error| {
            NativeUpdateError::State(format!("read installer permissions failed: {error}"))
        })?
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&script_path, permissions).map_err(|error| {
        NativeUpdateError::State(format!("mark installer executable failed: {error}"))
    })?;
    Command::new("sh")
        .arg(&script_path)
        .spawn()
        .map_err(|error| {
            NativeUpdateError::State(format!("launch AppImage replacement failed: {error}"))
        })?;
    Ok(NativeInstallOutcome {
        status: NativeInstallStatus::ReplacementScheduled,
        message: plan.summary.clone(),
        should_quit_app: true,
    })
}

#[cfg(not(target_os = "linux"))]
fn execute_linux_appimage_replacement(
    plan: &NativeInstallPlan,
) -> Result<NativeInstallOutcome, NativeUpdateError> {
    open_package(&plan.package_path)?;
    Ok(NativeInstallOutcome {
        status: NativeInstallStatus::InstallerLaunched,
        message: plan.summary.clone(),
        should_quit_app: false,
    })
}

#[cfg(target_os = "macos")]
fn execute_macos_app_replacement(
    plan: &NativeInstallPlan,
) -> Result<NativeInstallOutcome, NativeUpdateError> {
    let Some(current_bundle) = current_macos_app_bundle(&plan.current_exe) else {
        open_package(&plan.package_path)?;
        return Ok(NativeInstallOutcome {
            status: NativeInstallStatus::ManualActionRequired,
            message: "Current executable is not inside an .app bundle.".to_string(),
            should_quit_app: false,
        });
    };

    let script_path = plan.package_path.with_extension("install.sh");
    let backup_bundle = current_bundle.with_extension("old");
    let install_source = match plan.package_kind {
        InstallPackageKind::MacAppBundle => format!(
            r#"new_app="{package}""#,
            package = shell_escape_path(&plan.package_path)
        ),
        InstallPackageKind::MacArchive => format!(
            r#"tmp_dir="$(/usr/bin/mktemp -d "${{TMPDIR:-/tmp}}/oxideterm-update.XXXXXX")"
case "{package}" in
  *.zip|*.app.zip)
    /usr/bin/ditto -x -k "{package}" "$tmp_dir"
    ;;
  *.tar.gz|*.tgz|*.app.tar.gz)
    /usr/bin/tar -xzf "{package}" -C "$tmp_dir"
    ;;
  *)
    exit 2
    ;;
esac
new_app="$(/usr/bin/find "$tmp_dir" -maxdepth 3 -name '*.app' -type d | /usr/bin/head -n 1)"
if [ -z "$new_app" ]; then
  exit 3
fi"#,
            package = shell_escape_path(&plan.package_path)
        ),
        _ => {
            open_package(&plan.package_path)?;
            return Ok(NativeInstallOutcome {
                status: NativeInstallStatus::InstallerLaunched,
                message: plan.summary.clone(),
                should_quit_app: false,
            });
        }
    };
    let script = format!(
        r#"#!/bin/sh
set -eu
while kill -0 {pid} 2>/dev/null; do
  sleep 0.2
done
{install_source}
rm -rf "{backup}"
if [ -d "{current}" ]; then
  mv "{current}" "{backup}"
fi
if ! /usr/bin/ditto "$new_app" "{current}"; then
  if [ -d "{backup}" ]; then
    mv "{backup}" "{current}"
  fi
  exit 4
fi
rm -rf "{backup}"
if [ -n "${{tmp_dir:-}}" ]; then
  rm -rf "$tmp_dir"
fi
/usr/bin/open "{current}"
"#,
        pid = plan.process_id,
        install_source = install_source,
        current = shell_escape_path(&current_bundle),
        backup = shell_escape_path(&backup_bundle),
    );
    std::fs::write(&script_path, script).map_err(|error| {
        NativeUpdateError::State(format!("write macOS installer failed: {error}"))
    })?;
    Command::new("sh")
        .arg(&script_path)
        .spawn()
        .map_err(|error| {
            NativeUpdateError::State(format!("launch macOS replacement failed: {error}"))
        })?;
    Ok(NativeInstallOutcome {
        status: NativeInstallStatus::ReplacementScheduled,
        message: plan.summary.clone(),
        should_quit_app: true,
    })
}

#[cfg(not(target_os = "macos"))]
fn execute_macos_app_replacement(
    plan: &NativeInstallPlan,
) -> Result<NativeInstallOutcome, NativeUpdateError> {
    open_package(&plan.package_path)?;
    Ok(NativeInstallOutcome {
        status: NativeInstallStatus::InstallerLaunched,
        message: plan.summary.clone(),
        should_quit_app: false,
    })
}

#[cfg(target_os = "macos")]
fn current_macos_app_bundle(current_exe: &Path) -> Option<PathBuf> {
    current_exe
        .ancestors()
        .find(|path| path.extension().and_then(|extension| extension.to_str()) == Some("app"))
        .map(Path::to_path_buf)
}

fn shell_escape_path(path: &Path) -> String {
    // Scripts quote the returned path, so escape characters that still carry
    // meaning inside double quotes.
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(os: &'static str, install_flavor: InstallFlavor, exe: &str) -> NativeInstallContext {
        NativeInstallContext {
            target: PlatformTarget::new(os, "x86_64"),
            current_exe: PathBuf::from(exe),
            process_id: 42,
            install_flavor,
        }
    }

    #[test]
    fn portable_updates_require_manual_folder_replacement() {
        let plan = plan_native_install(
            "/tmp/OxideTerm_linux_x64_portable.tar.gz",
            &context("linux", InstallFlavor::Portable, "/apps/oxideterm-native"),
        );
        assert_eq!(plan.strategy, InstallStrategy::PortableManualReplacement);
        assert_eq!(plan.action, InstallActionKind::Manual);
        assert_eq!(plan.package_kind, InstallPackageKind::PortableArchive);
    }

    #[test]
    fn windows_installer_is_launched_directly() {
        let plan = plan_native_install(
            "C:/Temp/OxideTerm.msi",
            &context(
                "windows",
                InstallFlavor::WindowsNsis,
                "C:/Program Files/OxideTerm/OxideTerm.exe",
            ),
        );
        assert_eq!(plan.strategy, InstallStrategy::WindowsRunInstaller);
        assert_eq!(plan.package_kind, InstallPackageKind::WindowsMsi);
        assert_eq!(plan.summary, "Launch the Windows installer with elevation.");
    }

    #[test]
    fn windows_exe_installer_uses_helper_replacement_strategy() {
        let plan = plan_native_install(
            "C:/Temp/OxideTerm_setup.exe",
            &context(
                "windows",
                InstallFlavor::WindowsNsis,
                "C:/Users/me/AppData/Local/Programs/OxideTerm/oxideterm-native.exe",
            ),
        );
        assert_eq!(plan.strategy, InstallStrategy::WindowsRunInstaller);
        assert_eq!(plan.package_kind, InstallPackageKind::WindowsExe);
        assert!(plan.requires_app_exit);
        assert_eq!(
            plan.summary,
            "Stage the Windows installer and apply it after OxideTerm exits."
        );
    }

    #[test]
    fn windows_installer_archive_is_extracted_before_launch() {
        let plan = plan_native_install(
            "C:/Temp/OxideTerm_1.0.0_x64.msi.zip",
            &context(
                "windows",
                InstallFlavor::WindowsNsis,
                "C:/Program Files/OxideTerm/OxideTerm.exe",
            ),
        );
        assert_eq!(
            plan.strategy,
            InstallStrategy::WindowsExtractAndRunInstaller
        );
        assert_eq!(
            plan.package_kind,
            InstallPackageKind::WindowsInstallerArchive
        );
        assert_eq!(
            plan.summary,
            "Extract the Windows update archive and launch its installer."
        );
    }

    #[test]
    fn powershell_single_quotes_escape_embedded_quotes() {
        assert_eq!(
            powershell_single_quoted("C:/Temp/Oxide'Term Setup.exe"),
            "'C:/Temp/Oxide''Term Setup.exe'"
        );
    }

    #[test]
    fn windows_start_process_script_uses_runas_and_argument_list() {
        assert_eq!(
            windows_start_process_script(
                "msiexec.exe",
                &[
                    "/i".to_string(),
                    "C:/Temp/OxideTerm Setup.msi".to_string(),
                    "/promptrestart".to_string(),
                ],
                false,
                true,
            ),
            "Start-Process -FilePath 'msiexec.exe' -ArgumentList @('/i', 'C:/Temp/OxideTerm Setup.msi', '/promptrestart') -Verb RunAs"
        );
    }

    #[test]
    fn per_user_nsis_update_waits_without_runas() {
        let script = windows_start_process_script(
            "C:/Temp/OxideTerm Setup.exe",
            &["/S".to_string(), "/OXIDETERM_UPDATE=1".to_string()],
            true,
            false,
        );

        assert_eq!(
            script,
            "Start-Process -FilePath 'C:/Temp/OxideTerm Setup.exe' -ArgumentList @('/S', '/OXIDETERM_UPDATE=1') -Wait"
        );
        assert!(!script.contains("RunAs"));
    }

    #[test]
    fn windows_installer_message_includes_retained_package_path() {
        assert!(
            windows_installer_launched_message(
                Path::new("C:/Temp/OxideTerm Setup.msi"),
                InstallPackageKind::WindowsMsi,
            )
            .contains("C:/Temp/OxideTerm Setup.msi")
        );
    }

    #[test]
    fn linux_appimage_replacement_requires_appimage_runtime() {
        let plan = plan_native_install(
            "/tmp/OxideTerm.AppImage",
            &context(
                "linux",
                InstallFlavor::LinuxAppImage,
                "/opt/OxideTerm.AppImage",
            ),
        );
        assert_eq!(plan.strategy, InstallStrategy::LinuxReplaceAppImage);
        assert!(plan.requires_app_exit);
    }

    #[test]
    fn linux_appimage_archive_replacement_requires_appimage_runtime() {
        let plan = plan_native_install(
            "/tmp/OxideTerm.AppImage.tar.gz",
            &context(
                "linux",
                InstallFlavor::LinuxAppImage,
                "/opt/OxideTerm.AppImage",
            ),
        );
        assert_eq!(plan.strategy, InstallStrategy::LinuxReplaceAppImageArchive);
        assert_eq!(plan.package_kind, InstallPackageKind::LinuxAppImageArchive);
        assert!(plan.requires_app_exit);
    }

    #[test]
    fn macos_dmg_opens_for_finder_installation() {
        let plan = plan_native_install(
            "/tmp/OxideTerm.dmg",
            &context(
                "macos",
                InstallFlavor::MacApp,
                "/Applications/OxideTerm.app/Contents/MacOS/OxideTerm",
            ),
        );
        assert_eq!(plan.strategy, InstallStrategy::MacOpenDmg);
        assert_eq!(plan.action, InstallActionKind::OpenPackage);
    }

    #[test]
    fn macos_archived_app_uses_replacement_strategy() {
        let plan = plan_native_install(
            "/tmp/OxideTerm.app.zip",
            &context(
                "macos",
                InstallFlavor::MacApp,
                "/Applications/OxideTerm.app/Contents/MacOS/OxideTerm",
            ),
        );
        assert_eq!(plan.strategy, InstallStrategy::MacReplaceAppArchive);
        assert_eq!(plan.process_id, 42);
        assert!(plan.requires_app_exit);
    }

    #[test]
    fn linux_deb_opens_with_the_package_installer() {
        let plan = plan_native_install(
            "/tmp/OxideTerm_linux_x64.deb",
            &context(
                "linux",
                InstallFlavor::LinuxDeb,
                "/opt/oxideterm/oxideterm-native",
            ),
        );

        assert_eq!(plan.strategy, InstallStrategy::LinuxOpenPackage);
        assert_eq!(plan.action, InstallActionKind::OpenPackage);
        assert_eq!(plan.package_kind, InstallPackageKind::LinuxPackage);
        assert!(!plan.requires_app_exit);
    }

    #[test]
    fn every_platform_portable_package_uses_manual_folder_replacement() {
        // Portable archives differ by platform, but all require replacing the
        // self-contained folder instead of invoking an installed-package flow.
        let cases = [
            ("macos", "/tmp/OxideTerm_macos_x64_portable.tar.gz"),
            ("windows", "C:/Temp/OxideTerm_windows_x64_portable.zip"),
            ("linux", "/tmp/OxideTerm_linux_x64_portable.tar.gz"),
        ];

        for (os, package_path) in cases {
            let plan = plan_native_install(
                package_path,
                &context(os, InstallFlavor::Portable, "/apps/oxideterm-native"),
            );
            assert_eq!(plan.strategy, InstallStrategy::PortableManualReplacement);
            assert_eq!(plan.package_kind, InstallPackageKind::PortableArchive);
            assert!(plan.requires_app_exit);
        }
    }

    #[test]
    fn mismatched_install_flavor_falls_back_to_manual_open() {
        let plan = plan_native_install(
            "/tmp/OxideTerm_linux_x64.AppImage",
            &context(
                "linux",
                InstallFlavor::LinuxDeb,
                "/opt/oxideterm/oxideterm-native",
            ),
        );

        assert_eq!(plan.strategy, InstallStrategy::OpenPackage);
        assert_eq!(plan.action, InstallActionKind::OpenPackage);
    }

    #[test]
    fn linux_pkg_tar_zst_is_a_linux_package() {
        assert_eq!(
            classify_package(Path::new("/tmp/oxideterm.pkg.tar.zst")),
            InstallPackageKind::LinuxPackage
        );
    }

    #[test]
    fn escapes_paths_for_double_quoted_shell_scripts() {
        assert_eq!(
            shell_escape_path(Path::new("/tmp/Oxide $Term`\".AppImage")),
            "/tmp/Oxide \\$Term\\`\\\".AppImage"
        );
    }
}
