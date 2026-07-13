use super::*;

use sha2::{Digest, Sha256};

pub(in crate::workspace) const CLI_COMPANION_COMMAND_NAME: &str = "oxideterm";
pub(in crate::workspace) const LEGACY_CLI_COMPANION_COMMAND_NAME: &str = "oxt";
pub(in crate::workspace) const CLI_COMPANION_RESOURCE_DIR: &str = "cli-bin";

impl WorkspaceApp {
    pub(in crate::workspace) fn refresh_cli_companion_status(&mut self, cx: &mut Context<Self>) {
        if self.settings_page.cli_companion_loading {
            return;
        }

        self.settings_page.set_cli_companion_loading(true);
        let runtime = self.forwarding_runtime.clone();
        cx.spawn(async move |weak, cx| {
            let result = runtime
                .spawn_blocking(cli_companion_status)
                .await
                .map_err(|error| error.to_string())
                .and_then(|status| status);
            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(status) => this.settings_page.set_cli_companion_status(status),
                    Err(error) => this.settings_page.set_cli_companion_error(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn install_cli_companion(&mut self, cx: &mut Context<Self>) {
        if self.settings_page.cli_companion_loading {
            return;
        }

        self.settings_page.set_cli_companion_loading(true);
        let runtime = self.forwarding_runtime.clone();
        let success_title = self.i18n.t("settings_view.general.cli_installed");
        cx.spawn(async move |weak, cx| {
            let result = runtime
                .spawn_blocking(|| cli_companion_install().and_then(|_| cli_companion_status()))
                .await
                .map_err(|error| error.to_string())
                .and_then(|status| status);
            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(status) => {
                        this.settings_page.set_cli_companion_status(status);
                        this.push_ai_settings_toast(success_title, TerminalNoticeVariant::Success);
                    }
                    Err(error) => {
                        this.settings_page.set_cli_companion_error(error.clone());
                        this.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn uninstall_cli_companion(&mut self, cx: &mut Context<Self>) {
        if self.settings_page.cli_companion_loading {
            return;
        }

        self.settings_page.set_cli_companion_loading(true);
        let runtime = self.forwarding_runtime.clone();
        let success_title = self.i18n.t("settings_view.general.cli_uninstalled");
        cx.spawn(async move |weak, cx| {
            let result = runtime
                .spawn_blocking(|| cli_companion_uninstall().and_then(|_| cli_companion_status()))
                .await
                .map_err(|error| error.to_string())
                .and_then(|status| status);
            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(status) => {
                        this.settings_page.set_cli_companion_status(status);
                        this.push_ai_settings_toast(success_title, TerminalNoticeVariant::Success);
                    }
                    Err(error) => {
                        this.settings_page.set_cli_companion_error(error.clone());
                        this.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn uninstall_legacy_cli_companion(&mut self, cx: &mut Context<Self>) {
        if self.settings_page.cli_companion_loading {
            return;
        }

        self.settings_page.set_cli_companion_loading(true);
        let runtime = self.forwarding_runtime.clone();
        let success_title = self.i18n.t("migration.cli_legacy_uninstalled");
        cx.spawn(async move |weak, cx| {
            let result = runtime
                .spawn_blocking(|| {
                    legacy_cli_companion_uninstall().and_then(|_| cli_companion_status())
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|status| status);
            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(status) => {
                        this.settings_page.set_cli_companion_status(status);
                        this.push_ai_settings_toast(success_title, TerminalNoticeVariant::Success);
                    }
                    Err(error) => {
                        this.settings_page.set_cli_companion_error(error.clone());
                        this.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn migrate_cli_companion(&mut self, cx: &mut Context<Self>) {
        if self.settings_page.cli_companion_loading {
            return;
        }

        self.settings_page.set_cli_companion_loading(true);
        let runtime = self.forwarding_runtime.clone();
        let success_title = self.i18n.t("migration.cli_migrated");
        cx.spawn(async move |weak, cx| {
            let result = runtime
                .spawn_blocking(|| cli_companion_migrate().and_then(|_| cli_companion_status()))
                .await
                .map_err(|error| error.to_string())
                .and_then(|status| status);
            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(status) => {
                        this.settings_page.set_cli_companion_status(status);
                        this.push_ai_settings_toast(success_title, TerminalNoticeVariant::Success);
                    }
                    Err(error) => {
                        this.settings_page.set_cli_companion_error(error.clone());
                        this.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn cli_companion_action_button(
        &self,
        label: String,
        icon: LucideIcon,
        variant: ButtonVariant,
        loading: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(icon, 14.0, rgb(self.tokens.ui.text)).into_any_element()),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                icon_position: ToolbarButtonIconPosition::Leading,
                loading,
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, _window, cx| {
                listener(this, _event, _window, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }
}

pub(in crate::workspace) fn cli_companion_status() -> Result<CliCompanionStatus, String> {
    let bundle_path = find_bundled_cli();
    let install_path = cli_install_path();
    let legacy_install_path = legacy_cli_install_path();
    let installed = cli_path_present(&install_path);
    let legacy_installed = cli_path_present(&legacy_install_path);
    let matches_bundled = match (bundle_path.as_ref(), installed) {
        (Some(bundle_path), true) => {
            Some(installed_cli_matches_bundle(&install_path, bundle_path)?)
        }
        _ => None,
    };

    Ok(CliCompanionStatus {
        bundled: bundle_path.is_some(),
        installed,
        install_path: Some(install_path.display().to_string()),
        legacy_installed,
        legacy_install_path: Some(legacy_install_path.display().to_string()),
        bundle_path: bundle_path.map(|path| path.display().to_string()),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        matches_bundled,
        needs_reinstall: matches_bundled == Some(false),
    })
}

pub(in crate::workspace) fn cli_companion_install() -> Result<(), String> {
    let bundle_path =
        find_bundled_cli().ok_or_else(|| "CLI binary is not included in this build".to_string())?;
    let target = cli_install_path();

    install_cli_at_path(&bundle_path, &target)
}

fn install_cli_at_path(
    bundle_path: &std::path::Path,
    target: &std::path::Path,
) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }

    if target.exists() || target.symlink_metadata().is_ok() {
        std::fs::remove_file(&target)
            .map_err(|error| format!("failed to remove {}: {error}", target.display()))?;
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(bundle_path, target)
        .map_err(|error| format!("failed to link {}: {error}", target.display()))?;

    #[cfg(windows)]
    // Windows follows the Tauri implementation: install a real copied binary,
    // because creating user-visible symlinks can require elevated privileges.
    std::fs::copy(bundle_path, target)
        .map(|_| ())
        .map_err(|error| format!("failed to copy {}: {error}", target.display()))?;

    Ok(())
}

pub(in crate::workspace) fn cli_companion_uninstall() -> Result<(), String> {
    remove_managed_cli(&cli_install_path())
}

pub(in crate::workspace) fn legacy_cli_companion_uninstall() -> Result<(), String> {
    // Only remove the path managed by OxideTerm 1.x. A PATH lookup could point
    // at a package-manager or user-owned command with the same name.
    remove_managed_cli(&legacy_cli_install_path())
}

pub(in crate::workspace) fn cli_companion_migrate() -> Result<(), String> {
    let bundle_path =
        find_bundled_cli().ok_or_else(|| "CLI binary is not included in this build".to_string())?;
    cli_companion_migrate_at_paths(
        &bundle_path,
        &cli_install_path(),
        &legacy_cli_install_path(),
    )
}

fn cli_companion_migrate_at_paths(
    bundle_path: &std::path::Path,
    install_path: &std::path::Path,
    legacy_path: &std::path::Path,
) -> Result<(), String> {
    // Install and verify the replacement before removing the legacy command so
    // a failed bundle operation never leaves the user without either CLI.
    install_cli_at_path(bundle_path, install_path)?;
    if !installed_cli_matches_bundle(install_path, bundle_path)? {
        return Err("installed CLI does not match the bundled OxideTerm CLI".to_string());
    }
    remove_managed_cli(legacy_path)
}

fn remove_managed_cli(target: &std::path::Path) -> Result<(), String> {
    if !target.exists() && target.symlink_metadata().is_err() {
        return Ok(());
    }
    std::fs::remove_file(&target)
        .map_err(|error| format!("failed to remove {}: {error}", target.display()))
}

pub(in crate::workspace) fn cli_path_present(path: &std::path::Path) -> bool {
    path.symlink_metadata().is_ok()
}

pub(in crate::workspace) fn installed_cli_matches_bundle(
    install_path: &std::path::Path,
    bundle_path: &std::path::Path,
) -> Result<bool, String> {
    let metadata = install_path
        .symlink_metadata()
        .map_err(|error| format!("failed to inspect {}: {error}", install_path.display()))?;

    if metadata.file_type().is_symlink() && !install_path.exists() {
        return Ok(false);
    }

    if let (Ok(install), Ok(bundle)) = (install_path.canonicalize(), bundle_path.canonicalize()) {
        if install == bundle {
            return Ok(true);
        }
    }

    Ok(file_sha256(install_path)? == file_sha256(bundle_path)?)
}

pub(in crate::workspace) fn file_sha256(path: &std::path::Path) -> Result<[u8; 32], String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hasher.finalize().into())
}

pub(in crate::workspace) fn find_bundled_cli() -> Option<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("OXIDETERM_CLI_BIN").map(std::path::PathBuf::from) {
        if path.exists() {
            return Some(path);
        }
    }

    let binary_name = cli_binary_name();
    for dir in cli_resource_dirs() {
        let target_path = dir.join(host_target_triple()).join(&binary_name);
        if target_path.exists() {
            return Some(target_path);
        }

        let direct_path = dir.join(&binary_name);
        if direct_path.exists() {
            return Some(direct_path);
        }

        if let Some(path) = find_first_cli_binary_in_dir(&dir, &binary_name) {
            return Some(path);
        }
    }

    None
}

pub(in crate::workspace) fn cli_resource_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        // Native bundles mirror Tauri resources under Contents/Resources on macOS.
        dirs.push(
            exe_dir
                .join("../Resources")
                .join(CLI_COMPANION_RESOURCE_DIR),
        );
        dirs.push(exe_dir.join("resources").join(CLI_COMPANION_RESOURCE_DIR));
        dirs.push(exe_dir.join(CLI_COMPANION_RESOURCE_DIR));
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(
            cwd.join("crates")
                .join("oxideterm-gpui-app")
                .join("resources")
                .join(CLI_COMPANION_RESOURCE_DIR),
        );
    }
    dirs
}

pub(in crate::workspace) fn find_first_cli_binary_in_dir(
    dir: &std::path::Path,
    binary_name: &str,
) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let candidate = path.join(binary_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

pub(in crate::workspace) fn cli_install_path() -> std::path::PathBuf {
    cli_install_path_for_command(CLI_COMPANION_COMMAND_NAME)
}

pub(in crate::workspace) fn legacy_cli_install_path() -> std::path::PathBuf {
    cli_install_path_for_command(LEGACY_CLI_COMPANION_COMMAND_NAME)
}

fn cli_install_path_for_command(command_name: &str) -> std::path::PathBuf {
    #[cfg(unix)]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home)
                .join(".local")
                .join("bin")
                .join(command_name);
        }
        std::path::PathBuf::from("/usr/local/bin").join(command_name)
    }

    #[cfg(windows)]
    {
        let binary_name = format!("{command_name}.exe");
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            return std::path::PathBuf::from(local_app_data)
                .join("OxideTerm")
                .join("bin")
                .join(binary_name);
        }
        std::path::PathBuf::from(binary_name)
    }
}

pub(in crate::workspace) fn cli_binary_name() -> String {
    #[cfg(windows)]
    {
        format!("{CLI_COMPANION_COMMAND_NAME}.exe")
    }
    #[cfg(not(windows))]
    {
        CLI_COMPANION_COMMAND_NAME.to_string()
    }
}

pub(in crate::workspace) fn host_target_triple() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        _ => "unknown",
    }
}

#[cfg(test)]
mod cli_companion_tests {
    use super::{
        CLI_COMPANION_COMMAND_NAME, LEGACY_CLI_COMPANION_COMMAND_NAME,
        cli_companion_migrate_at_paths, cli_install_path, cli_path_present,
        installed_cli_matches_bundle, legacy_cli_install_path, remove_managed_cli,
    };

    pub(in crate::workspace) fn temp_test_dir(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "oxideterm-cli-companion-{name}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    pub(in crate::workspace) fn identical_cli_files_match_bundled_copy() {
        let temp_dir = temp_test_dir("identical");
        let installed_path = temp_dir.join("installed-oxideterm");
        let bundled_path = temp_dir.join("bundled-oxideterm");

        std::fs::write(&installed_path, b"same-cli-binary").unwrap();
        std::fs::write(&bundled_path, b"same-cli-binary").unwrap();

        assert!(installed_cli_matches_bundle(&installed_path, &bundled_path).unwrap());
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    pub(in crate::workspace) fn different_cli_files_require_reinstall() {
        let temp_dir = temp_test_dir("different");
        let installed_path = temp_dir.join("installed-oxideterm");
        let bundled_path = temp_dir.join("bundled-oxideterm");

        std::fs::write(&installed_path, b"old-cli-binary").unwrap();
        std::fs::write(&bundled_path, b"new-cli-binary").unwrap();

        assert!(!installed_cli_matches_bundle(&installed_path, &bundled_path).unwrap());
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[cfg(unix)]
    #[test]
    pub(in crate::workspace) fn broken_symlink_is_installed_but_requires_reinstall() {
        let temp_dir = temp_test_dir("broken-link");
        let bundled_path = temp_dir.join("bundled-oxideterm");
        let broken_target = temp_dir.join("missing-oxideterm");
        let install_path = temp_dir.join("oxideterm");

        std::fs::write(&bundled_path, b"bundled-cli-binary").unwrap();
        std::os::unix::fs::symlink(&broken_target, &install_path).unwrap();

        assert!(cli_path_present(&install_path));
        assert!(!installed_cli_matches_bundle(&install_path, &bundled_path).unwrap());
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    pub(in crate::workspace) fn managed_paths_keep_new_and_legacy_commands_distinct() {
        let expected_new_name = if cfg!(windows) {
            format!("{CLI_COMPANION_COMMAND_NAME}.exe")
        } else {
            CLI_COMPANION_COMMAND_NAME.to_string()
        };
        let expected_legacy_name = if cfg!(windows) {
            format!("{LEGACY_CLI_COMPANION_COMMAND_NAME}.exe")
        } else {
            LEGACY_CLI_COMPANION_COMMAND_NAME.to_string()
        };

        assert_eq!(
            cli_install_path().file_name().unwrap().to_string_lossy(),
            expected_new_name
        );
        assert_eq!(
            legacy_cli_install_path()
                .file_name()
                .unwrap()
                .to_string_lossy(),
            expected_legacy_name
        );
        assert_ne!(cli_install_path(), legacy_cli_install_path());
    }

    #[test]
    pub(in crate::workspace) fn removing_managed_legacy_file_does_not_touch_sibling_command() {
        let temp_dir = temp_test_dir("legacy-remove");
        let legacy_path = temp_dir.join("oxt");
        let new_path = temp_dir.join("oxideterm");
        std::fs::write(&legacy_path, b"legacy-cli").unwrap();
        std::fs::write(&new_path, b"new-cli").unwrap();

        remove_managed_cli(&legacy_path).unwrap();

        assert!(!legacy_path.exists());
        assert!(new_path.exists());
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[cfg(unix)]
    #[test]
    pub(in crate::workspace) fn removing_managed_broken_legacy_symlink_is_idempotent() {
        let temp_dir = temp_test_dir("legacy-broken-link");
        let legacy_path = temp_dir.join("oxt");
        std::os::unix::fs::symlink(temp_dir.join("missing-oxt"), &legacy_path).unwrap();

        remove_managed_cli(&legacy_path).unwrap();
        remove_managed_cli(&legacy_path).unwrap();

        assert!(legacy_path.symlink_metadata().is_err());
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    pub(in crate::workspace) fn cli_migration_installs_new_command_before_removing_legacy() {
        let temp_dir = temp_test_dir("migrate-success");
        let bundle_path = temp_dir.join("bundled-oxideterm");
        let install_path = temp_dir.join("bin/oxideterm");
        let legacy_path = temp_dir.join("bin/oxt");
        std::fs::write(&bundle_path, b"new-cli").unwrap();
        std::fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
        std::fs::write(&legacy_path, b"old-cli").unwrap();

        cli_companion_migrate_at_paths(&bundle_path, &install_path, &legacy_path).unwrap();

        assert!(installed_cli_matches_bundle(&install_path, &bundle_path).unwrap());
        assert!(legacy_path.symlink_metadata().is_err());
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    pub(in crate::workspace) fn failed_new_cli_install_preserves_legacy_command() {
        let temp_dir = temp_test_dir("migrate-failure");
        let bundle_path = temp_dir.join("bundled-oxideterm");
        let blocked_parent = temp_dir.join("blocked");
        let install_path = blocked_parent.join("oxideterm");
        let legacy_path = temp_dir.join("oxt");
        std::fs::write(&bundle_path, b"new-cli").unwrap();
        std::fs::write(&blocked_parent, b"not-a-directory").unwrap();
        std::fs::write(&legacy_path, b"old-cli").unwrap();

        assert!(cli_companion_migrate_at_paths(&bundle_path, &install_path, &legacy_path).is_err());
        assert!(legacy_path.exists());
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
