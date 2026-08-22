#!/usr/bin/env python3
"""Tests for native release packaging helpers."""

from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import call, patch

# Import the release helpers from their responsibility-specific directory.
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "release"))

import package_native


class WindowsInstallerScriptTests(unittest.TestCase):
    def identity(self) -> package_native.ReleaseIdentity:
        return package_native.ReleaseIdentity(
            channel="gpui-preview",
            app_name="OxideTerm GPUI Preview",
            app_identifier="com.oxideterm.gpuiPreview",
            windows_install_dir=r"$LOCALAPPDATA\Programs\OxideTerm GPUI Preview",
            windows_registry_key="OxideTerm GPUI Preview",
            windows_uninstall_key="OxideTerm GPUI Preview",
            linux_package_name="oxideterm-gpui-preview",
            linux_install_dir="oxideterm-gpui-preview",
            linux_desktop_id="com.oxideterm.gpuiPreview",
            linux_icon_name="oxideterm-gpui-preview",
        )

    def test_existing_install_is_upgraded_in_place(self) -> None:
        identity = self.identity()
        script = package_native.windows_installer_script(
            binary=Path("oxideterm-native.exe"),
            version="1.2.0-gpui-preview.2",
            identity=identity,
            installer_root=Path(r"C:\dist\nsis-windows_x64"),
            installer_path=Path(r"C:\dist\OxideTerm_setup.exe"),
            icon_path=Path(r"C:\icons\icon.ico"),
        )

        self.assertIn("InstallDirRegKey HKCU", script)
        self.assertIn("SetOverwrite on", script)
        self.assertIn("WriteUninstaller", script)
        self.assertIn("WriteRegStr HKCU", script)
        self.assertIn('VIProductVersion "1.2.0.0"', script)
        self.assertIn('"ProductVersion" "1.2.0-gpui-preview.2"', script)
        self.assertIn("normal_install:", script)
        self.assertIn("!insertmacro MUI_PAGE_COMPONENTS", script)
        self.assertIn('Section "Application Files"', script)
        self.assertIn("SectionIn RO", script)
        self.assertIn('Section "Start Menu Shortcut"', script)
        self.assertIn('Section /o "Desktop Shortcut"', script)
        self.assertNotIn("already installed", script)
        self.assertNotIn("uninstall_existing", script)
        self.assertNotIn("ExecWait", script)

    def test_update_mode_stages_files_and_installs_helper_directly(self) -> None:
        script = package_native.windows_installer_script(
            binary=Path("oxideterm-native.exe"),
            version="1.2.0-gpui-preview.2",
            identity=self.identity(),
            installer_root=Path(r"C:\dist\nsis-windows_x64"),
            installer_path=Path(r"C:\dist\OxideTerm_setup.exe"),
            icon_path=Path(r"C:\icons\icon.ico"),
        )

        self.assertIn('/OXIDETERM_UPDATE=1', script)
        self.assertIn('SetSilent silent', script)
        self.assertIn('RMDir /r "$INSTDIR\\install"', script)
        self.assertIn('SetOutPath "$INSTDIR\\tools"', script)
        self.assertIn('tools/oxideterm-update-helper.exe"', script)
        self.assertIn('SetOutPath "$INSTDIR\\install"', script)
        self.assertIn('Exec \'"$INSTDIR\\tools\\oxideterm-update-helper.exe"', script)
        self.assertIn('--install-dir "$INSTDIR"', script)
        self.assertIn('--app-exe "$INSTDIR\\oxideterm-native.exe" --launch', script)
        self.assertIn('StrCmp $IsOxideUpdate "1" start_menu_shortcut_done', script)
        self.assertIn('StrCmp $IsOxideUpdate "1" desktop_shortcut_done', script)
        self.assertNotIn('$LOCALAPPDATA\\OxideTerm\\oxideterm.exe', script)

    def test_all_install_modes_register_the_application_icon(self) -> None:
        script = package_native.windows_installer_script(
            binary=Path("oxideterm-native.exe"),
            version="1.2.0-gpui-preview.2",
            identity=self.identity(),
            installer_root=Path(r"C:\dist\nsis-windows_x64"),
            installer_path=Path(r"C:\dist\OxideTerm_setup.exe"),
            icon_path=Path(r"C:\icons\icon.ico"),
        )
        display_icon_entry = (
            r'"DisplayIcon" "$\"$INSTDIR\oxideterm-native.exe$\",0"'
        )

        self.assertEqual(script.count(display_icon_entry), 2)

    def test_modern_ui_uses_the_application_icon(self) -> None:
        icon_path = Path(r"C:\icons\icon.ico")
        script = package_native.windows_installer_script(
            binary=Path("oxideterm-native.exe"),
            version="1.2.0-gpui-preview.2",
            identity=self.identity(),
            installer_root=Path(r"C:\dist\nsis-windows_x64"),
            installer_path=Path(r"C:\dist\OxideTerm_setup.exe"),
            icon_path=icon_path,
        )
        resolved_icon = package_native.nsis_path(icon_path)
        installer_icon = f'!define MUI_ICON "{resolved_icon}"'
        uninstaller_icon = f'!define MUI_UNICON "{resolved_icon}"'

        self.assertIn(installer_icon, script)
        self.assertIn(uninstaller_icon, script)
        self.assertLess(script.index(installer_icon), script.index("!include MUI2.nsh"))
        self.assertNotIn('\nIcon "', script)
        self.assertNotIn("\nUninstallIcon ", script)

    def test_stable_installer_detects_tauri_current_user_install(self) -> None:
        identity = package_native.release_identity("v2.0.0", "2.0.0")
        script = package_native.windows_installer_script(
            binary=Path("oxideterm-native.exe"),
            version="2.0.0",
            identity=identity,
            installer_root=Path(r"C:\dist\nsis-windows_x64"),
            installer_path=Path(r"C:\dist\OxideTerm_setup.exe"),
            icon_path=Path(r"C:\icons\icon.ico"),
        )

        self.assertIn('$LOCALAPPDATA\\OxideTerm\\oxideterm.exe', script)
        self.assertIn('StrCpy $INSTDIR "$LOCALAPPDATA\\OxideTerm"', script)
        self.assertIn('StrCpy $IsOxideUpdate "1"', script)
        self.assertIn('StrCpy $IsLegacyUpgrade "1"', script)
        self.assertIn('SetSilent silent', script)
        self.assertIn(
            'CreateShortcut "$SMPROGRAMS\\OxideTerm\\OxideTerm.lnk" '
            '"$INSTDIR\\oxideterm-native.exe"',
            script,
        )
        self.assertIn('IfFileExists "$DESKTOP\\OxideTerm.lnk"', script)

    def test_installer_registers_connection_uri_capabilities_without_embedding_credentials(self) -> None:
        identity = self.identity()
        script = package_native.windows_installer_script(
            binary=Path("oxideterm-native.exe"),
            version="2.0.23",
            identity=identity,
            installer_root=Path(r"C:\dist\nsis-windows_x64"),
            installer_path=Path(r"C:\dist\OxideTerm_setup.exe"),
            icon_path=Path(r"C:\icons\icon.ico"),
        )

        for scheme in package_native.CONNECTION_URI_SCHEMES:
            self.assertIn(f'"{scheme}" "{identity.app_identifier}.{scheme}"', script)
        self.assertIn(r'"$\"$INSTDIR\oxideterm-native.exe$\" $\"%1$\""', script)
        self.assertIn('Software\\RegisteredApplications', script)


class MacosConnectionUriTests(unittest.TestCase):
    def test_bundle_declares_all_connection_uri_schemes(self) -> None:
        identity = package_native.release_identity("v2.0.23", "2.0.23")
        plist = package_native.build_macos_info_plist("2.0.23", identity)

        self.assertEqual(
            plist["CFBundleURLTypes"][0]["CFBundleURLSchemes"],
            list(package_native.CONNECTION_URI_SCHEMES),
        )


class MacosBridgeArchiveTests(unittest.TestCase):
    def test_tauri_bridge_archive_keeps_app_bundle_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            app = root / "OxideTerm.app"
            executable = app / "Contents" / "MacOS" / "oxideterm-native"
            executable.parent.mkdir(parents=True)
            executable.write_bytes(b"native")
            executable.chmod(0o755)
            archive_path = root / "OxideTerm.app.tar.gz"

            package_native.archive_macos_tauri_bundle(app, archive_path)

            with package_native.tarfile.open(archive_path, "r:gz") as archive:
                member = archive.getmember(
                    "OxideTerm.app/Contents/MacOS/oxideterm-native"
                )
                self.assertEqual(member.mode & 0o111, 0o111)


class MacosDmgDetachTests(unittest.TestCase):
    def test_selects_partition_scheme_root_device_from_attach_plist(self) -> None:
        attach_plist = plistlib.dumps(
            {
                "system-entities": [
                    {
                        "dev-entry": "/dev/disk4",
                        "content-hint": "GUID_partition_scheme",
                    },
                    {
                        "dev-entry": "/dev/disk4s1",
                        "content-hint": "Apple_APFS",
                    },
                    {
                        "dev-entry": "/dev/disk5s1",
                        "mount-point": "/tmp/oxideterm-test.mount",
                    },
                ]
            }
        )

        self.assertEqual(
            package_native.macos_dmg_root_device(attach_plist),
            "/dev/disk4",
        )

    def test_collects_devices_from_hdiutil_info_plist(self) -> None:
        info_plist = plistlib.dumps(
            {
                "images": [
                    {
                        "system-entities": [
                            {"dev-entry": "/dev/disk4"},
                            {"dev-entry": "/dev/disk4s1"},
                        ]
                    },
                    {"system-entities": [{"dev-entry": "/dev/disk8"}]},
                ]
            }
        )

        self.assertEqual(
            package_native.macos_dmg_devices_from_info_plist(info_plist),
            {"/dev/disk4", "/dev/disk4s1", "/dev/disk8"},
        )

    def test_retries_busy_dmg_before_succeeding(self) -> None:
        device = "/dev/disk9"
        detach_command = ["hdiutil", "detach", device]
        busy_error = subprocess.CalledProcessError(
            package_native.MACOS_RESOURCE_BUSY_EXIT_CODE, detach_command
        )

        with (
            patch.object(
                package_native, "run", side_effect=[busy_error, None]
            ) as run_mock,
            patch.object(
                package_native, "macos_dmg_device_is_attached", return_value=True
            ),
            patch.object(package_native.time, "sleep") as sleep,
        ):
            package_native.detach_macos_dmg(device)

        self.assertEqual(
            run_mock.call_args_list, [call(detach_command), call(detach_command)]
        )
        sleep.assert_called_once_with(
            package_native.MACOS_DMG_DETACH_RETRY_DELAY_SECONDS
        )

    def test_force_detaches_after_retry_limit(self) -> None:
        device = "/dev/disk9"
        detach_command = ["hdiutil", "detach", device]
        busy_error = subprocess.CalledProcessError(
            package_native.MACOS_RESOURCE_BUSY_EXIT_CODE, detach_command
        )
        failed_attempts = [
            busy_error for _ in range(package_native.MACOS_DMG_DETACH_MAX_ATTEMPTS)
        ]

        with (
            patch.object(
                package_native, "run", side_effect=[*failed_attempts, None]
            ) as run_mock,
            patch.object(
                package_native, "macos_dmg_device_is_attached", return_value=True
            ),
            patch.object(package_native.time, "sleep") as sleep,
        ):
            package_native.detach_macos_dmg(device)

        self.assertEqual(
            run_mock.call_args_list,
            [call(detach_command)] * package_native.MACOS_DMG_DETACH_MAX_ATTEMPTS
            + [call(["hdiutil", "detach", "-force", device])],
        )
        self.assertEqual(
            sleep.call_count, package_native.MACOS_DMG_DETACH_MAX_ATTEMPTS - 1
        )

    def test_retries_busy_force_detach_before_succeeding(self) -> None:
        device = "/dev/disk9"
        detach_command = ["hdiutil", "detach", device]
        force_detach_command = ["hdiutil", "detach", "-force", device]
        busy_error = subprocess.CalledProcessError(
            package_native.MACOS_RESOURCE_BUSY_EXIT_CODE, detach_command
        )
        failed_attempts = [
            busy_error for _ in range(package_native.MACOS_DMG_DETACH_MAX_ATTEMPTS)
        ]

        with (
            patch.object(
                package_native,
                "run",
                side_effect=[*failed_attempts, busy_error, None],
            ) as run_mock,
            patch.object(
                package_native, "macos_dmg_device_is_attached", return_value=True
            ),
            patch.object(package_native.time, "sleep") as sleep,
        ):
            package_native.detach_macos_dmg(device)

        self.assertEqual(
            run_mock.call_args_list,
            [call(detach_command)] * package_native.MACOS_DMG_DETACH_MAX_ATTEMPTS
            + [call(force_detach_command), call(force_detach_command)],
        )
        self.assertEqual(
            sleep.call_count, package_native.MACOS_DMG_DETACH_MAX_ATTEMPTS
        )

    def test_reports_busy_after_force_detach_retry_limit(self) -> None:
        device = "/dev/disk9"
        detach_command = ["hdiutil", "detach", device]
        force_detach_command = ["hdiutil", "detach", "-force", device]
        busy_error = subprocess.CalledProcessError(
            package_native.MACOS_RESOURCE_BUSY_EXIT_CODE, detach_command
        )
        failed_attempt_count = (
            package_native.MACOS_DMG_DETACH_MAX_ATTEMPTS
            + package_native.MACOS_DMG_FORCE_DETACH_MAX_ATTEMPTS
        )

        with (
            patch.object(
                package_native,
                "run",
                side_effect=[busy_error for _ in range(failed_attempt_count)],
            ) as run_mock,
            patch.object(
                package_native, "macos_dmg_device_is_attached", return_value=True
            ),
            patch.object(package_native.time, "sleep") as sleep,
            self.assertRaises(subprocess.CalledProcessError),
        ):
            package_native.detach_macos_dmg(device)

        self.assertEqual(
            run_mock.call_args_list,
            [call(detach_command)] * package_native.MACOS_DMG_DETACH_MAX_ATTEMPTS
            + [call(force_detach_command)]
            * package_native.MACOS_DMG_FORCE_DETACH_MAX_ATTEMPTS,
        )
        self.assertEqual(sleep.call_count, failed_attempt_count - 2)

    def test_does_not_retry_non_busy_detach_error(self) -> None:
        device = "/dev/disk9"
        detach_command = ["hdiutil", "detach", device]
        permission_error = subprocess.CalledProcessError(1, detach_command)

        with (
            patch.object(
                package_native, "run", side_effect=permission_error
            ) as run_mock,
            patch.object(package_native.time, "sleep") as sleep,
            self.assertRaises(subprocess.CalledProcessError),
        ):
            package_native.detach_macos_dmg(device)

        run_mock.assert_called_once_with(detach_command)
        sleep.assert_not_called()

    def test_accepts_async_detach_after_busy_result(self) -> None:
        device = "/dev/disk9"
        detach_command = ["hdiutil", "detach", device]
        busy_error = subprocess.CalledProcessError(
            package_native.MACOS_RESOURCE_BUSY_EXIT_CODE, detach_command
        )

        with (
            patch.object(package_native, "run", side_effect=busy_error) as run_mock,
            patch.object(
                package_native, "macos_dmg_device_is_attached", return_value=False
            ) as device_is_attached,
            patch.object(package_native.time, "sleep") as sleep,
        ):
            package_native.detach_macos_dmg(device)

        run_mock.assert_called_once_with(detach_command)
        device_is_attached.assert_called_once_with(device)
        sleep.assert_not_called()


class ReleaseDocumentTests(unittest.TestCase):
    def test_release_documents_include_native_and_agent_notices(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory)
            package_native.copy_release_documents(destination)

            self.assertEqual(
                {path.name for path in destination.iterdir()},
                {
                    "BACKGROUND-ASSETS-LICENSE.md",
                    "GPUI-CE-LICENSE-APACHE",
                    "LICENSE",
                    "MICROSOFT-TERMINAL-LICENSE-MIT",
                    "NOTICE",
                    "README.md",
                    "THIRD_PARTY_NOTICES.md",
                    "AGENT_THIRD_PARTY_NOTICES.md",
                },
            )
            self.assertGreater((destination / "THIRD_PARTY_NOTICES.md").stat().st_size, 0)
            background_license = (
                destination / "BACKGROUND-ASSETS-LICENSE.md"
            ).read_text(encoding="utf-8")
            self.assertIn("oxide-nocturne-v1.webp", background_license)
            self.assertIn("CC-BY-4.0", background_license)
            self.assertIn("does not grant rights", background_license)
            self.assertGreater(
                (destination / "AGENT_THIRD_PARTY_NOTICES.md").stat().st_size,
                0,
            )

    def test_portable_update_manifest_owns_only_release_entries(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            package_root = Path(directory)
            binary = package_root / "oxideterm-native"
            update_helper = package_root / "oxideterm-update-helper"

            package_native.write_portable_update_manifest(
                package_root, binary, update_helper
            )

            manifest = package_native.json.loads(
                (package_root / "portable-update.json").read_text(encoding="utf-8")
            )
            self.assertEqual(manifest["appExecutable"], binary.name)
            self.assertEqual(
                manifest["updateHelper"],
                f"tools/{update_helper.name}",
            )
            self.assertIn("resources", manifest["managedEntries"])
            self.assertIn("tools", manifest["managedEntries"])
            self.assertNotIn("data", manifest["managedEntries"])
            self.assertNotIn("portable.json", manifest["managedEntries"])


class ReleaseVersionTests(unittest.TestCase):
    def test_release_version_must_match_compiled_workspace_version(self) -> None:
        workspace_version = package_native.workspace_version()
        package_native.validate_release_version(f"gpui-v{workspace_version}", workspace_version)

        mismatched_version = f"{workspace_version}.mismatch"
        with self.assertRaisesRegex(RuntimeError, "scripts/release/bump_version.py"):
            package_native.validate_release_version(
                f"v{mismatched_version}", mismatched_version
            )

    def test_windows_numeric_version_uses_semver_core(self) -> None:
        self.assertEqual(
            package_native.windows_numeric_version("2.0.0-gpui-preview.15"),
            "2.0.0.0",
        )


class LinuxDesktopEntryTests(unittest.TestCase):
    def test_desktop_entry_exposes_matching_startup_window_class(self) -> None:
        identity = package_native.release_identity("v2.0.0", "2.0.0")
        with tempfile.TemporaryDirectory() as directory:
            desktop_file = Path(directory) / "com.oxideterm.app.desktop"

            package_native.write_linux_desktop_file(
                desktop_file, identity, "/opt/oxideterm/oxideterm-native"
            )

            desktop_entry = desktop_file.read_text(encoding="utf-8")
            self.assertIn("StartupWMClass=com.oxideterm.app", desktop_entry)
            for scheme in package_native.CONNECTION_URI_SCHEMES:
                self.assertIn(f"x-scheme-handler/{scheme}", desktop_entry)


class PlatformSigningTests(unittest.TestCase):
    def test_macos_developer_id_enables_hardened_runtime_and_timestamp(self) -> None:
        with patch.dict(
            package_native.os.environ,
            {"MACOS_CODESIGN_IDENTITY": "Developer ID Application: OxideTerm"},
            clear=False,
        ):
            command = package_native.macos_codesign_command(
                "codesign", Path("OxideTerm.app")
            )

        self.assertIn("--options", command)
        self.assertIn("runtime", command)
        self.assertIn("--timestamp", command)
        self.assertIn("Developer ID Application: OxideTerm", command)

    def test_macos_development_build_uses_ad_hoc_identity(self) -> None:
        with patch.dict(package_native.os.environ, {}, clear=True):
            command = package_native.macos_codesign_command(
                "codesign", Path("OxideTerm.app")
            )

        self.assertNotIn("--timestamp", command)
        self.assertEqual(command[-2:], ["-", "OxideTerm.app"])

    def test_macos_notarization_is_optional_for_development_builds(self) -> None:
        with patch.dict(package_native.os.environ, {}, clear=True):
            submitted = package_native.notarize_macos_artifact(
                Path("OxideTerm.app.zip"), staple=False
            )

        self.assertFalse(submitted)

    def test_unsigned_stable_dmg_includes_gatekeeper_notice(self) -> None:
        identity = package_native.release_identity("v2.0.0", "2.0.0")
        with patch.dict(package_native.os.environ, {}, clear=True):
            included = package_native.should_include_macos_unsigned_install_notice(
                identity
            )

        self.assertTrue(included)

    def test_signed_or_preview_dmg_omits_stable_gatekeeper_notice(self) -> None:
        stable = package_native.release_identity("v2.0.0", "2.0.0")
        preview = package_native.release_identity(
            "gpui-v2.0.0-gpui-preview.16", "2.0.0-gpui-preview.16"
        )

        with patch.dict(
            package_native.os.environ,
            {"MACOS_CODESIGN_IDENTITY": "Developer ID Application: OxideTerm"},
            clear=True,
        ):
            self.assertFalse(
                package_native.should_include_macos_unsigned_install_notice(stable)
            )
        with patch.dict(package_native.os.environ, {}, clear=True):
            self.assertFalse(
                package_native.should_include_macos_unsigned_install_notice(preview)
            )

    def test_unsigned_notice_is_copied_into_stable_dmg_root(self) -> None:
        identity = package_native.release_identity("v2.0.0", "2.0.0")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "notice-source.png"
            source.write_bytes(b"png notice")
            background = root / "background-source.png"
            background.write_bytes(b"png background")
            dmg_root = root / "dmg"
            dmg_root.mkdir()

            with (
                patch.object(
                    package_native, "MACOS_UNSIGNED_INSTALL_NOTICE", source
                ),
                patch.object(
                    package_native, "MACOS_UNSIGNED_DMG_BACKGROUND", background
                ),
                patch.dict(package_native.os.environ, {}, clear=True),
            ):
                copied = package_native.copy_macos_unsigned_install_notice(
                    dmg_root, identity
                )

            self.assertTrue(copied)
            self.assertEqual(
                (dmg_root / package_native.MACOS_UNSIGNED_INSTALL_NOTICE_NAME).read_bytes(),
                b"png notice",
            )
            self.assertEqual(
                (
                    dmg_root
                    / package_native.MACOS_DMG_BACKGROUND_DIR_NAME
                    / package_native.MACOS_DMG_BACKGROUND_NAME
                ).read_bytes(),
                b"png background",
            )

    def test_unsigned_finder_layout_keeps_primary_icons_and_notice_separate(self) -> None:
        script = package_native.macos_dmg_finder_script()

        self.assertIn("set dmgFolder to POSIX file mountPath as alias", script)
        self.assertNotIn("tell disk volumeName", script)
        self.assertIn("set bounds of dmgWindow to {120, 120, 840, 600}", script)
        self.assertIn("set position of item appName of dmgFolder to {180, 205}", script)
        self.assertIn(
            'set position of item "Applications" of dmgFolder to {540, 205}', script
        )
        self.assertIn(
            "set position of item noticeName of dmgFolder to {620, 390}", script
        )
        self.assertIn(package_native.MACOS_DMG_BACKGROUND_NAME, script)


class LinuxPackagingTests(unittest.TestCase):
    def test_dynamic_graphics_loaders_are_declared_as_recommendations(self) -> None:
        # Vulkan and EGL are loaded with dlopen, so ELF dependency discovery
        # cannot add these runtime packages automatically.
        self.assertEqual(
            package_native.linux_deb_graphics_recommends(),
            "libegl1, libvulkan1",
        )
        self.assertEqual(
            package_native.linux_rpm_graphics_recommends(),
            "Recommends: libglvnd-egl\nRecommends: vulkan-loader",
        )

    def test_rpm_arch_and_prerelease_version_are_normalized(self) -> None:
        self.assertEqual(
            package_native.linux_rpm_arch("aarch64-unknown-linux-gnu"),
            "aarch64",
        )
        self.assertEqual(
            package_native.linux_rpm_version_release("2.0.0-gpui-preview.16"),
            ("2.0.0", "0.gpui.preview.16"),
        )
        self.assertEqual(
            package_native.linux_rpm_version_release("2.0.0"),
            ("2.0.0", "1"),
        )

    @unittest.skipUnless(
        shutil.which("rpmbuild") and shutil.which("rpm"),
        "RPM build tools are not installed",
    )
    def test_rpm_package_contains_runtime_layout_and_metadata(self) -> None:
        # Exercise the actual RPM spec with synthetic resources so Linux CI
        # catches rpmbuild syntax and payload regressions before a release.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            dist_dir = root / "dist"
            resource_dir = root / "resources"
            dist_dir.mkdir()

            for icon_name in (
                "32x32.png",
                "64x64.png",
                "128x128.png",
                "128x128@2x.png",
            ):
                icon_path = resource_dir / "icons" / icon_name
                icon_path.parent.mkdir(parents=True, exist_ok=True)
                icon_path.write_bytes(b"synthetic icon")

            target = "x86_64-unknown-linux-gnu"
            for relative_path in (
                Path("cli-bin") / target / "oxideterm",
                Path("helpers") / target / "oxideterm-rdp-helper",
                Path("helpers") / target / "oxideterm-vnc-helper",
            ):
                destination = resource_dir / relative_path
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2("/bin/true", destination)

            for agent_target in (
                "x86_64-unknown-linux-musl",
                "aarch64-unknown-linux-musl",
            ):
                # Follow the production resource contract so a directory rename
                # cannot leave the RPM fixture silently testing a stale layout.
                agent_path = (
                    resource_dir
                    / package_native.AGENT_RESOURCE_DIR
                    / agent_target
                    / "oxideterm-agent"
                )
                agent_path.parent.mkdir(parents=True, exist_ok=True)
                agent_path.write_bytes(b"synthetic agent")

            release_documents = []
            for name in (
                "GPUI-CE-LICENSE-APACHE",
                "LICENSE",
                "MICROSOFT-TERMINAL-LICENSE-MIT",
                "NOTICE",
                "README.md",
                "THIRD_PARTY_NOTICES.md",
                "AGENT_THIRD_PARTY_NOTICES.md",
            ):
                document = root / name
                document.write_text(f"{name}\n", encoding="utf-8")
                # Production documents carry an explicit package filename so
                # sources outside the repository root can be renamed safely.
                release_documents.append((document, name))

            binary = root / "oxideterm-native"
            shutil.copy2("/bin/true", binary)
            identity = package_native.release_identity("v2.0.0", "2.0.0")
            with (
                patch.object(package_native, "DIST_DIR", dist_dir),
                patch.object(package_native, "RESOURCE_DIR", resource_dir),
                patch.object(package_native, "RELEASE_DOCUMENTS", release_documents),
            ):
                package_native.create_linux_rpm(
                    binary,
                    target,
                    "2.0.0",
                    "linux_x64",
                    identity,
                )

            artifact = dist_dir / "OxideTerm_2.0.0_linux_x64.rpm"
            self.assertTrue(artifact.is_file())
            package_listing = subprocess.check_output(
                ["rpm", "-qpl", str(artifact)],
                text=True,
            )
            self.assertIn("/opt/oxideterm/PACKAGE_KIND", package_listing)
            self.assertIn("/opt/oxideterm/oxideterm-native", package_listing)
            self.assertIn(
                "/opt/oxideterm/resources/agents/x86_64-unknown-linux-musl/oxideterm-agent",
                package_listing,
            )
            self.assertIn(
                "/opt/oxideterm/resources/agents/aarch64-unknown-linux-musl/oxideterm-agent",
                package_listing,
            )
            recommendations = subprocess.check_output(
                ["rpm", "-qp", "--recommends", str(artifact)],
                text=True,
            )
            for package_name in package_native.LINUX_RPM_GRAPHICS_RECOMMENDS:
                self.assertIn(package_name, recommendations)

    def test_dpkg_shlibdeps_output_requires_dependency_expression(self) -> None:
        dependencies = package_native.parse_dpkg_shlibdeps_output(
            "shlibs:Depends=libc6 (>= 2.35), libgcc-s1 (>= 3.0)\n"
        )
        self.assertEqual(dependencies, "libc6 (>= 2.35), libgcc-s1 (>= 3.0)")

        with self.assertRaisesRegex(RuntimeError, "shlibs:Depends"):
            package_native.parse_dpkg_shlibdeps_output("shlibs:Recommends=libx11-6")


if __name__ == "__main__":
    unittest.main()
