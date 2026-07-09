#!/usr/bin/env python3
"""Tests for native release packaging helpers."""

from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))

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
        self.assertIn('StrCmp $IsOxideUpdate "1" start_menu_shortcut_done', script)
        self.assertIn('StrCmp $IsOxideUpdate "1" desktop_shortcut_done', script)


class ReleaseDocumentTests(unittest.TestCase):
    def test_release_documents_include_native_and_agent_notices(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory)
            package_native.copy_release_documents(destination)

            self.assertEqual(
                {path.name for path in destination.iterdir()},
                {
                    "LICENSE",
                    "NOTICE",
                    "README.md",
                    "THIRD_PARTY_NOTICES.md",
                    "AGENT_THIRD_PARTY_NOTICES.md",
                },
            )
            self.assertGreater((destination / "THIRD_PARTY_NOTICES.md").stat().st_size, 0)
            self.assertGreater(
                (destination / "AGENT_THIRD_PARTY_NOTICES.md").stat().st_size,
                0,
            )


class ReleaseVersionTests(unittest.TestCase):
    def test_release_version_must_match_compiled_workspace_version(self) -> None:
        workspace_version = package_native.workspace_version()
        package_native.validate_release_version(f"gpui-v{workspace_version}", workspace_version)

        mismatched_version = f"{workspace_version}.mismatch"
        with self.assertRaisesRegex(RuntimeError, "scripts/bump_version.py"):
            package_native.validate_release_version(
                f"v{mismatched_version}", mismatched_version
            )

    def test_windows_numeric_version_uses_semver_core(self) -> None:
        self.assertEqual(
            package_native.windows_numeric_version("2.0.0-gpui-preview.15"),
            "2.0.0.0",
        )


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


class LinuxPackagingTests(unittest.TestCase):
    def test_dpkg_shlibdeps_output_requires_dependency_expression(self) -> None:
        dependencies = package_native.parse_dpkg_shlibdeps_output(
            "shlibs:Depends=libc6 (>= 2.35), libgcc-s1 (>= 3.0)\n"
        )
        self.assertEqual(dependencies, "libc6 (>= 2.35), libgcc-s1 (>= 3.0)")

        with self.assertRaisesRegex(RuntimeError, "shlibs:Depends"):
            package_native.parse_dpkg_shlibdeps_output("shlibs:Recommends=libx11-6")


if __name__ == "__main__":
    unittest.main()
