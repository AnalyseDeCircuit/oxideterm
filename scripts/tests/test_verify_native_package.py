#!/usr/bin/env python3
"""Tests for native package verification helpers."""

from pathlib import Path
import hashlib
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

# Import the release helpers from their responsibility-specific directory.
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "release"))

import verify_native_package
import conpty_runtime


class ArtifactNameTests(unittest.TestCase):
    def test_release_tag_is_normalized_to_artifact_version(self) -> None:
        self.assertEqual(
            verify_native_package.normalized_version(
                "refs/tags/gpui-v2.0.0-gpui-preview.15"
            ),
            "2.0.0-gpui-preview.15",
        )

    def test_artifact_names_cover_platforms_and_stable_update_compatibility(self) -> None:
        for target, version, expected in [
            ("x86_64-pc-windows-msvc", "2.0.0", {
                "OxideTerm_2.0.0_windows_x64-setup.exe",
                "OxideTerm_2.0.0_windows_x64_portable.zip",
            }),
            ("aarch64-unknown-linux-gnu", "2.0.0", {
                "OxideTerm_2.0.0_linux_arm64.AppImage",
                "OxideTerm_2.0.0_linux_arm64.deb",
                "OxideTerm_2.0.0_linux_arm64.rpm",
                "OxideTerm_2.0.0_linux_arm64_portable.tar.gz",
            }),
            ("aarch64-apple-darwin", "2.0.0", {
                "OxideTerm_2.0.0_macos_arm64.app.zip",
                "OxideTerm_2.0.0_macos_arm64.app.tar.gz",
                "OxideTerm_2.0.0_macos_arm64.dmg",
                "OxideTerm_2.0.0_macos_arm64_portable.tar.gz",
            }),
            ("aarch64-apple-darwin", "2.0.0-gpui-preview.15", {
                "OxideTerm_2.0.0-gpui-preview.15_macos_arm64.app.zip",
                "OxideTerm_2.0.0-gpui-preview.15_macos_arm64.dmg",
                "OxideTerm_2.0.0-gpui-preview.15_macos_arm64_portable.tar.gz",
            }),
        ]:
            with self.subTest(target=target, version=version):
                self.assertEqual(
                    verify_native_package.expected_artifact_names(target, version), expected
                )


class WindowsInstallerTests(unittest.TestCase):
    def test_installer_checks_runtime_in_normal_and_update_payloads(self) -> None:
        digest = hashlib.sha256(b"runtime fixture").hexdigest()
        files = ("conpty.dll", "arm64/OpenConsole.exe")
        cases = [("valid", None, None)]
        for root in ("", "install"):
            for name in files:
                relative = str(Path(root) / "resources/conpty" / name)
                cases.extend([
                    (f"missing {relative}", relative, None),
                    (f"corrupt {relative}", None, relative),
                ])
        for label, missing, corrupt in cases:
            with self.subTest(case=label):
                def extract(args):
                    if args[1] == "l":
                        return "\n".join(verify_native_package.REQUIRED_DOCUMENTS | {
                            "VERSION", "oxideterm-native.exe", "oxideterm-update-helper.exe",
                        })
                    directory = Path(next(arg[2:] for arg in args if arg.startswith("-o")))
                    # 7-Zip exposes both NSIS File /r destinations, not just the install branch.
                    for root in ("", "install"):
                        (directory / root).mkdir(parents=True, exist_ok=True)
                        (directory / root / "VERSION").write_text("2.2.1\n")
                        for name in files:
                            relative = str(Path(root) / "resources/conpty" / name)
                            if relative == missing:
                                continue
                            file = directory / relative
                            file.parent.mkdir(parents=True, exist_ok=True)
                            file.write_bytes(b"corrupt" if relative == corrupt else b"runtime fixture")
                    return ""

                with (
                    patch.object(verify_native_package.shutil, "which", return_value="7z"),
                    patch.object(verify_native_package, "run_checked", side_effect=extract),
                    patch.object(conpty_runtime, "RUNTIMES", {
                        "aarch64-pc-windows-msvc": ("arm64", digest, digest),
                    }),
                ):
                    if missing or corrupt:
                        with self.assertRaisesRegex(
                            RuntimeError, "must contain" if missing else "SHA-256 mismatch",
                        ) as error:
                            verify_native_package.verify_windows_installer(
                                Path("setup.exe"), "2.2.1", "aarch64-pc-windows-msvc",
                            )
                        self.assertIn(Path(missing or corrupt).as_posix(), str(error.exception))
                    else:
                        verify_native_package.verify_windows_installer(
                            Path("setup.exe"), "2.2.1", "aarch64-pc-windows-msvc",
                        )


class PortableArchiveTests(unittest.TestCase):
    def setUp(self) -> None:
        digest = hashlib.sha256(b"data").hexdigest()
        runtime = patch.object(conpty_runtime, "RUNTIMES", {
            "x86_64-pc-windows-msvc": ("x64", digest, digest),
            "aarch64-pc-windows-msvc": ("arm64", digest, digest),
        })
        runtime.start()
        self.addCleanup(runtime.stop)

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
            *([
                f"{root}/resources/conpty/conpty.dll",
                f"{root}/resources/conpty/x64/OpenConsole.exe",
            ] if executable.endswith(".exe") else []),
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

    def test_portable_archive_validates_contents_version_runtime_and_update_ownership(self) -> None:
        cases = [
            ("valid", None, None, None),
            ("version", None, "VERSION", "contains version"),
            ("plugins", "/data/plugins/", None, "data/plugins"),
            ("manifest", None, "portable-update.json", "includes user data"),
            ("wrong architecture", None, None, "conpty"),
        ]
        for entry in ("conpty.dll", "OpenConsole.exe"):
            cases.extend([
                (f"missing {entry}", entry, None, "conpty"),
                (f"corrupt {entry}", None, entry, "conpty"),
            ])
        for name, omitted, replaced, expected_error in cases:
            with self.subTest(case=name), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "portable.zip"
                with zipfile.ZipFile(path, "w") as archive:
                    for entry in self.required_entries("OxideTerm", "oxideterm-native.exe"):
                        if omitted and entry.endswith(omitted):
                            continue
                        content = self.entry_bytes(entry, "oxideterm-native.exe")
                        if replaced and entry.endswith(replaced):
                            if replaced == "VERSION":
                                content = b"1.9.0\n"
                            elif replaced == "portable-update.json":
                                content = content.replace(
                                    b'"portable-update.json"]',
                                    b'"portable-update.json","data"]',
                                )
                            else:
                                content = b"corrupt runtime"
                        archive.writestr(entry, content)
                target = (
                    "aarch64-pc-windows-msvc" if name == "wrong architecture"
                    else "x86_64-pc-windows-msvc"
                )
                if expected_error:
                    with self.assertRaisesRegex(RuntimeError, expected_error):
                        verify_native_package.verify_portable_archive(path, target, "2.0.0")
                else:
                    verify_native_package.verify_portable_archive(path, target, "2.0.0")

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
