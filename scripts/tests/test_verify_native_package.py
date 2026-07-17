#!/usr/bin/env python3
"""Tests for native package verification helpers."""

from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
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
            *(f"{root}/{name}" for name in verify_native_package.REQUIRED_DOCUMENTS),
        ]

    def test_windows_portable_archive_has_required_entries(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "portable.zip"
            with zipfile.ZipFile(path, "w") as archive:
                for name in self.required_entries("OxideTerm", "oxideterm-native.exe"):
                    archive.writestr(name, b"2.0.0\n" if name.endswith("VERSION") else b"data")
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
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"data")
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
                    archive.writestr(name, b"1.9.0\n" if name.endswith("VERSION") else b"data")

            with self.assertRaisesRegex(RuntimeError, "contains version"):
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

if __name__ == "__main__":
    unittest.main()
