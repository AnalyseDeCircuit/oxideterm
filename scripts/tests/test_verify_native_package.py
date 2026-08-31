#!/usr/bin/env python3
"""Tests for native package verification helpers."""

from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

# Import the release helpers from their responsibility-specific directory.
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "release"))

import verify_native_package


class ArtifactNameTests(unittest.TestCase):
    def test_release_tag_is_normalized_to_artifact_version(self) -> None:
        self.assertEqual(
            verify_native_package.normalized_version(
                "refs/tags/gpui-v2.0.0-gpui-preview.15"
            ),
            "2.0.0-gpui-preview.15",
        )

    def test_windows_artifacts_include_installer_and_portable(self) -> None:
        self.assertEqual(
            verify_native_package.expected_artifact_names(
                "x86_64-pc-windows-msvc", "2.0.0"
            ),
            {
                "OxideTerm_2.0.0_windows_x64-setup.exe",
                "OxideTerm_2.0.0_windows_x64_portable.zip",
            },
        )

    def test_linux_artifacts_include_all_distribution_shapes(self) -> None:
        names = verify_native_package.expected_artifact_names(
            "aarch64-unknown-linux-gnu", "2.0.0"
        )
        self.assertEqual(len(names), 4)
        self.assertTrue(any(name.endswith(".AppImage") for name in names))
        self.assertTrue(any(name.endswith(".deb") for name in names))
        self.assertTrue(any(name.endswith(".rpm") for name in names))
        self.assertTrue(any(name.endswith(".tar.gz") for name in names))

    def test_stable_macos_requires_tauri_bridge_archive(self) -> None:
        stable = verify_native_package.expected_artifact_names(
            "aarch64-apple-darwin", "2.0.0"
        )
        preview = verify_native_package.expected_artifact_names(
            "aarch64-apple-darwin", "2.0.0-gpui-preview.15"
        )

        self.assertIn("OxideTerm_2.0.0_macos_arm64.app.tar.gz", stable)
        self.assertNotIn(
            "OxideTerm_2.0.0-gpui-preview.15_macos_arm64.app.tar.gz",
            preview,
        )


class PortableArchiveTests(unittest.TestCase):
    def required_entries(self, root: str, executable: str) -> list[str]:
        return [
            f"{root}/{executable}",
            f"{root}/portable",
            f"{root}/VERSION",
            f"{root}/data/plugins/",
            f"{root}/portable-update.json",
            (
                f"{root}/tools/oxideterm-update-helper.exe"
                if executable.endswith(".exe")
                else f"{root}/tools/oxideterm-update-helper"
            ),
            *(f"{root}/{name}" for name in verify_native_package.REQUIRED_DOCUMENTS),
        ]

    def entry_bytes(self, name: str, executable: str) -> bytes:
        if name.endswith("VERSION"):
            return b"2.0.0\n"
        if name.endswith("portable-update.json"):
            helper = (
                "tools/oxideterm-update-helper.exe"
                if executable.endswith(".exe")
                else "tools/oxideterm-update-helper"
            )
            return (
                "{"
                '"formatVersion":1,'
                f'"appExecutable":"{executable}",'
                f'"updateHelper":"{helper}",'
                '"managedEntries":['
                f'"{executable}","resources","tools","portable","VERSION",'
                '"portable-update.json"]'
                "}"
            ).encode()
        return b"data"

    def test_windows_portable_archive_has_required_entries(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "portable.zip"
            with zipfile.ZipFile(path, "w") as archive:
                for name in self.required_entries("OxideTerm", "oxideterm-native.exe"):
                    archive.writestr(
                        name, self.entry_bytes(name, "oxideterm-native.exe")
                    )
            verify_native_package.verify_portable_archive(
                path, "x86_64-pc-windows-msvc", "2.0.0"
            )

    def test_linux_portable_archive_rejects_missing_agent_notice(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "root"
            root.mkdir()
            for name in self.required_entries("OxideTerm", "oxideterm-native"):
                if name.endswith("AGENT_THIRD_PARTY_NOTICES.md"):
                    continue
                path = root / name
                if name.endswith("/"):
                    path.mkdir(parents=True, exist_ok=True)
                else:
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(self.entry_bytes(name, "oxideterm-native"))
            archive_path = Path(directory) / "portable.tar.gz"
            with tarfile.open(archive_path, "w:gz") as archive:
                archive.add(root / "OxideTerm", arcname="OxideTerm")

            with self.assertRaisesRegex(RuntimeError, "AGENT_THIRD_PARTY_NOTICES"):
                verify_native_package.verify_portable_archive(
                    archive_path, "x86_64-unknown-linux-gnu", "2.0.0"
                )

    def test_portable_archive_rejects_wrong_internal_version(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "portable.zip"
            with zipfile.ZipFile(path, "w") as archive:
                for name in self.required_entries("OxideTerm", "oxideterm-native.exe"):
                    content = self.entry_bytes(name, "oxideterm-native.exe")
                    archive.writestr(
                        name, b"1.9.0\n" if name.endswith("VERSION") else content
                    )

            with self.assertRaisesRegex(RuntimeError, "contains version"):
                verify_native_package.verify_portable_archive(
                    path, "x86_64-pc-windows-msvc", "2.0.0"
                )

    def test_portable_archive_rejects_missing_plugins_directory(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "portable.zip"
            with zipfile.ZipFile(path, "w") as archive:
                for name in self.required_entries("OxideTerm", "oxideterm-native.exe"):
                    if name.endswith("/data/plugins/"):
                        continue
                    archive.writestr(
                        name,
                        self.entry_bytes(name, "oxideterm-native.exe"),
                    )

            with self.assertRaisesRegex(RuntimeError, "data/plugins"):
                verify_native_package.verify_portable_archive(
                    path, "x86_64-pc-windows-msvc", "2.0.0"
                )

    def test_portable_archive_rejects_manifest_owned_user_data(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "portable.zip"
            with zipfile.ZipFile(path, "w") as archive:
                for name in self.required_entries(
                    "OxideTerm", "oxideterm-native.exe"
                ):
                    content = self.entry_bytes(name, "oxideterm-native.exe")
                    if name.endswith("portable-update.json"):
                        content = content.replace(
                            b'"portable-update.json"]',
                            b'"portable-update.json","data"]',
                        )
                    archive.writestr(name, content)

            with self.assertRaisesRegex(RuntimeError, "includes user data"):
                verify_native_package.verify_portable_archive(
                    path, "x86_64-pc-windows-msvc", "2.0.0"
                )


class LinuxMetadataTests(unittest.TestCase):
    def test_dynamic_graphics_loader_recommendations_are_required(self) -> None:
        verify_native_package.require_metadata_values(
            "libegl1, libvulkan1",
            verify_native_package.LINUX_DEB_GRAPHICS_RECOMMENDS,
            Path("OxideTerm.deb"),
            "Recommends field",
        )

        with self.assertRaisesRegex(RuntimeError, "libvulkan1"):
            verify_native_package.require_metadata_values(
                "libegl1, notlibvulkan1",
                verify_native_package.LINUX_DEB_GRAPHICS_RECOMMENDS,
                Path("OxideTerm.deb"),
                "Recommends field",
            )


class LinuxCompatibilityTests(unittest.TestCase):
    def test_glibc_symbol_versions_are_parsed_without_duplicates(self) -> None:
        version_info = """
          0x0010: Name: GLIBC_2.35  Flags: none  Version: 4
          0x0020: Name: GLIBC_2.17  Flags: none  Version: 3
          004:   3 (GLIBC_2.17)    4 (GLIBC_2.35)
        """

        self.assertEqual(
            verify_native_package.parse_glibc_versions(version_info),
            {(2, 17), (2, 35)},
        )

    def test_binary_requiring_newer_glibc_is_rejected(self) -> None:
        with (
            patch.object(
                verify_native_package.shutil,
                "which",
                return_value="/usr/bin/readelf",
            ),
            patch.object(
                verify_native_package,
                "run_checked",
                return_value="Name: GLIBC_2.39",
            ),
            self.assertRaisesRegex(RuntimeError, "exceeding the supported 2.35"),
        ):
            verify_native_package.verify_linux_glibc_compatibility(
                Path("oxideterm-native")
            )

if __name__ == "__main__":
    unittest.main()
