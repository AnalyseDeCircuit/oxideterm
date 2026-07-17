#!/usr/bin/env python3
"""Verify native release artifacts before they are uploaded."""

from __future__ import annotations

import argparse
import json
import plistlib
import re
import shutil
import subprocess
import tarfile
import tempfile
import zipfile
from pathlib import Path


REQUIRED_DOCUMENTS = {
    "GPUI-CE-LICENSE-APACHE",
    "LICENSE",
    "MICROSOFT-TERMINAL-LICENSE-MIT",
    "NOTICE",
    "THIRD_PARTY_NOTICES.md",
    "AGENT_THIRD_PARTY_NOTICES.md",
}
LINUX_DEB_GRAPHICS_RECOMMENDS = {"libegl1", "libvulkan1"}
LINUX_RPM_GRAPHICS_RECOMMENDS = {"libglvnd-egl", "vulkan-loader"}
PACKAGE_VERSION_FILENAME = "VERSION"


def normalized_version(raw: str) -> str:
    """Match the package script's tag-to-artifact version normalization."""
    for prefix in ("refs/tags/", "native-v", "gpui-v", "v"):
        if raw.startswith(prefix):
            raw = raw[len(prefix) :]
    return raw


def target_label(target: str) -> str:
    labels = {
        "x86_64-apple-darwin": "macos_x64",
        "aarch64-apple-darwin": "macos_arm64",
        "x86_64-pc-windows-msvc": "windows_x64",
        "aarch64-pc-windows-msvc": "windows_arm64",
        "x86_64-unknown-linux-gnu": "linux_x64",
        "aarch64-unknown-linux-gnu": "linux_arm64",
    }
    if target not in labels:
        raise ValueError(f"unsupported release target: {target}")
    return labels[target]


def expected_artifact_names(target: str, version: str) -> set[str]:
    label = target_label(target)
    names = {f"OxideTerm_{version}_{label}_portable"}
    if "windows" in target:
        return {
            f"OxideTerm_{version}_{label}-setup.exe",
            f"OxideTerm_{version}_{label}_portable.zip",
        }
    names = {f"OxideTerm_{version}_{label}_portable.tar.gz"}
    if "apple-darwin" in target:
        names.update(
            {
                f"OxideTerm_{version}_{label}.app.zip",
                f"OxideTerm_{version}_{label}.dmg",
            }
        )
        if "-" not in version:
            names.add(f"OxideTerm_{version}_{label}.app.tar.gz")
    if "linux" in target:
        names.update(
            {
                f"OxideTerm_{version}_{label}.AppImage",
                f"OxideTerm_{version}_{label}.deb",
                f"OxideTerm_{version}_{label}.rpm",
            }
        )
    return names


def archive_names(path: Path) -> set[str]:
    if path.suffix == ".zip":
        with zipfile.ZipFile(path) as archive:
            return set(archive.namelist())
    if path.name.endswith(".tar.gz"):
        with tarfile.open(path, "r:gz") as archive:
            return set(archive.getnames())
    raise ValueError(f"unsupported archive: {path}")


def require_archive_suffixes(names: set[str], suffixes: set[str], artifact: Path) -> None:
    missing = [suffix for suffix in suffixes if not any(name.endswith(suffix) for name in names)]
    if missing:
        raise RuntimeError(f"{artifact.name} is missing archive entries: {', '.join(missing)}")


def archive_entry_bytes(path: Path, suffix: str) -> bytes:
    """Read one archive entry selected by a stable package-relative suffix."""
    if path.suffix == ".zip":
        with zipfile.ZipFile(path) as archive:
            matches = [name for name in archive.namelist() if name.endswith(suffix)]
            if len(matches) != 1:
                raise RuntimeError(f"expected one {suffix} in {path.name}, found {len(matches)}")
            return archive.read(matches[0])
    with tarfile.open(path, "r:gz") as archive:
        matches = [member for member in archive.getmembers() if member.name.endswith(suffix)]
        if len(matches) != 1:
            raise RuntimeError(f"expected one {suffix} in {path.name}, found {len(matches)}")
        extracted = archive.extractfile(matches[0])
        if extracted is None:
            raise RuntimeError(f"{suffix} in {path.name} is not a regular file")
        return extracted.read()


def verify_embedded_version(path: Path, suffix: str, expected_version: str) -> None:
    actual = archive_entry_bytes(path, suffix).decode("utf-8").strip()
    if actual != expected_version:
        raise RuntimeError(
            f"{path.name} contains version {actual!r}, expected {expected_version!r}"
        )


def verify_portable_archive(path: Path, target: str, expected_version: str) -> None:
    executable = "oxideterm-native.exe" if "windows" in target else "oxideterm-native"
    require_archive_suffixes(
        archive_names(path),
        REQUIRED_DOCUMENTS | {PACKAGE_VERSION_FILENAME, "portable", executable},
        path,
    )
    verify_embedded_version(path, PACKAGE_VERSION_FILENAME, expected_version)


def verify_macos_app_zip(path: Path, expected_version: str) -> None:
    suffixes = {
        ".app/Contents/MacOS/oxideterm-native",
        f".app/Contents/Resources/licenses/{PACKAGE_VERSION_FILENAME}",
        *(f".app/Contents/Resources/licenses/{name}" for name in REQUIRED_DOCUMENTS),
    }
    require_archive_suffixes(archive_names(path), suffixes, path)
    verify_embedded_version(path, PACKAGE_VERSION_FILENAME, expected_version)
    plist = plistlib.loads(archive_entry_bytes(path, ".app/Contents/Info.plist"))
    actual_version = plist.get("CFBundleShortVersionString")
    if actual_version != expected_version:
        raise RuntimeError(
            f"{path.name} Info.plist contains version {actual_version!r}, expected {expected_version!r}"
        )


def verify_macos_tauri_archive(path: Path, expected_version: str) -> None:
    """Validate the legacy Tauri updater archive using the app bundle contract."""
    verify_macos_app_zip(path, expected_version)


def run_checked(command: list[str], *, cwd: Path | None = None) -> str:
    result = subprocess.run(
        command,
        cwd=cwd,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(f"command failed ({result.returncode}): {' '.join(command)}\n{result.stdout}")
    return result.stdout


def verify_binary_architecture(path: Path, target: str) -> None:
    file_tool = shutil.which("file")
    if not file_tool:
        raise RuntimeError("file is required for release architecture verification")
    description = run_checked([file_tool, str(path)]).lower()
    expected = ("x86-64", "x86_64") if target.startswith("x86_64") else ("arm64", "aarch64")
    if not any(marker in description for marker in expected):
        raise RuntimeError(f"{path.name} does not match {target}: {description.strip()}")


def extract_portable_binary(path: Path, target: str, destination: Path) -> Path:
    if path.suffix == ".zip":
        with zipfile.ZipFile(path) as archive:
            archive.extractall(destination)
    else:
        with tarfile.open(path, "r:gz") as archive:
            archive.extractall(destination, filter="data")
    executable = "oxideterm-native.exe" if "windows" in target else "oxideterm-native"
    matches = list(destination.rglob(executable))
    if len(matches) != 1:
        raise RuntimeError(f"expected one {executable} in {path.name}, found {len(matches)}")
    return matches[0]


def verify_linux_dynamic_libraries(binary: Path) -> None:
    ldd = shutil.which("ldd")
    if not ldd:
        raise RuntimeError("ldd is required for Linux release verification")
    output = run_checked([ldd, str(binary)])
    missing = [line.strip() for line in output.splitlines() if "not found" in line]
    if missing:
        raise RuntimeError(f"Linux binary has unresolved libraries: {'; '.join(missing)}")
    # WGPU loads Vulkan and EGL with dlopen, so ldd cannot prove that either
    # graphics path is installed. Package metadata and VM smoke tests cover it.


def require_metadata_values(
    metadata: str,
    expected_values: set[str],
    artifact: Path,
    field_name: str,
) -> None:
    """Require semantic package names in one metadata field."""
    metadata_values = set(re.findall(r"[A-Za-z0-9][A-Za-z0-9+.-]*", metadata))
    missing = sorted(expected_values - metadata_values)
    if missing:
        raise RuntimeError(
            f"{artifact.name} {field_name} is missing: {', '.join(missing)}"
        )


def verify_deb(path: Path, expected_version: str) -> None:
    dpkg_deb = shutil.which("dpkg-deb")
    if not dpkg_deb:
        raise RuntimeError("dpkg-deb is required for Debian package verification")
    listing = run_checked([dpkg_deb, "--contents", str(path)])
    for name in REQUIRED_DOCUMENTS:
        if name not in listing:
            raise RuntimeError(f"{path.name} does not contain {name}")
    info = run_checked([dpkg_deb, "--field", str(path), "Depends"])
    if not info.strip():
        raise RuntimeError(f"{path.name} has an empty Depends field")
    recommendations = run_checked([dpkg_deb, "--field", str(path), "Recommends"])
    require_metadata_values(
        recommendations,
        LINUX_DEB_GRAPHICS_RECOMMENDS,
        path,
        "Recommends field",
    )
    actual_version = run_checked([dpkg_deb, "--field", str(path), "Version"]).strip()
    debian_version = expected_version.replace("-", "~")
    if actual_version != debian_version:
        raise RuntimeError(
            f"{path.name} contains Debian version {actual_version!r}, expected {debian_version!r}"
        )


def verify_rpm(path: Path, target: str, expected_version: str) -> None:
    rpm = shutil.which("rpm")
    if not rpm:
        raise RuntimeError("rpm is required for RPM package verification")
    listing = run_checked([rpm, "-qpl", str(path)])
    for name in REQUIRED_DOCUMENTS | {PACKAGE_VERSION_FILENAME, "PACKAGE_KIND"}:
        if name not in listing:
            raise RuntimeError(f"{path.name} does not contain {name}")
    requirements = run_checked([rpm, "-qpR", str(path)])
    if not any(".so" in requirement for requirement in requirements.splitlines()):
        raise RuntimeError(f"{path.name} has no shared-library runtime requirements")
    recommendations = run_checked([rpm, "-qp", "--recommends", str(path)])
    require_metadata_values(
        recommendations,
        LINUX_RPM_GRAPHICS_RECOMMENDS,
        path,
        "recommended packages",
    )
    actual_version = run_checked([rpm, "-qp", "--qf", "%{VERSION}-%{RELEASE}", str(path)]).strip()
    if "-" in expected_version:
        upstream, prerelease = expected_version.split("-", 1)
        normalized = "".join(character if character.isalnum() else "." for character in prerelease).strip(".")
        expected_rpm_version = f"{upstream}-0.{normalized or 'preview'}"
    else:
        expected_rpm_version = f"{expected_version}-1"
    if actual_version != expected_rpm_version:
        raise RuntimeError(
            f"{path.name} contains RPM version {actual_version!r}, expected {expected_rpm_version!r}"
        )
    expected_arch = "x86_64" if target.startswith("x86_64") else "aarch64"
    actual_arch = run_checked([rpm, "-qp", "--qf", "%{ARCH}", str(path)]).strip()
    if actual_arch != expected_arch:
        raise RuntimeError(f"{path.name} contains RPM arch {actual_arch!r}, expected {expected_arch!r}")


def verify_appimage(path: Path, expected_version: str) -> None:
    path.chmod(path.stat().st_mode | 0o111)
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        run_checked([str(path.resolve()), "--appimage-extract"], cwd=root)
        extracted = root / "squashfs-root"
        for name in REQUIRED_DOCUMENTS:
            if not list(extracted.rglob(name)):
                raise RuntimeError(f"{path.name} does not contain {name}")
        versions = list(extracted.rglob(PACKAGE_VERSION_FILENAME))
        if not versions or all(item.read_text(encoding="utf-8").strip() != expected_version for item in versions):
            raise RuntimeError(f"{path.name} does not contain version {expected_version}")


def verify_windows_installer(path: Path, expected_version: str) -> None:
    seven_zip = next((shutil.which(name) for name in ("7z", "7zz", "7za") if shutil.which(name)), None)
    if not seven_zip:
        raise RuntimeError("7-Zip is required for NSIS content verification")
    listing = run_checked([seven_zip, "l", "-slt", str(path)])
    for name in REQUIRED_DOCUMENTS | {
        PACKAGE_VERSION_FILENAME,
        "oxideterm-native.exe",
        "oxideterm-update-helper.exe",
    }:
        if name not in listing:
            raise RuntimeError(f"{path.name} does not contain {name}")
    with tempfile.TemporaryDirectory() as directory:
        run_checked([seven_zip, "x", "-y", f"-o{directory}", str(path)])
        versions = list(Path(directory).rglob(PACKAGE_VERSION_FILENAME))
        if not versions or all(item.read_text(encoding="utf-8").strip() != expected_version for item in versions):
            raise RuntimeError(f"{path.name} does not contain version {expected_version}")


def verify_release(dist: Path, target: str, version: str) -> dict[str, object]:
    version = normalized_version(version)
    expected = expected_artifact_names(target, version)
    missing = sorted(name for name in expected if not (dist / name).is_file())
    if missing:
        raise RuntimeError(f"missing release artifacts: {', '.join(missing)}")
    empty = sorted(name for name in expected if (dist / name).stat().st_size == 0)
    if empty:
        raise RuntimeError(f"empty release artifacts: {', '.join(empty)}")

    label = target_label(target)
    portable_name = (
        f"OxideTerm_{version}_{label}_portable.zip"
        if "windows" in target
        else f"OxideTerm_{version}_{label}_portable.tar.gz"
    )
    portable_path = dist / portable_name
    verify_portable_archive(portable_path, target, version)
    with tempfile.TemporaryDirectory() as directory:
        binary = extract_portable_binary(portable_path, target, Path(directory))
        verify_binary_architecture(binary, target)
        if "linux" in target:
            verify_linux_dynamic_libraries(binary)

    if "windows" in target:
        verify_windows_installer(dist / f"OxideTerm_{version}_{label}-setup.exe", version)
    elif "apple-darwin" in target:
        verify_macos_app_zip(dist / f"OxideTerm_{version}_{label}.app.zip", version)
        legacy_archive = dist / f"OxideTerm_{version}_{label}.app.tar.gz"
        if legacy_archive.exists():
            verify_macos_tauri_archive(legacy_archive, version)
    elif "linux" in target:
        verify_appimage(dist / f"OxideTerm_{version}_{label}.AppImage", version)
        verify_deb(dist / f"OxideTerm_{version}_{label}.deb", version)
        verify_rpm(dist / f"OxideTerm_{version}_{label}.rpm", target, version)

    return {"target": target, "version": version, "artifacts": sorted(expected), "status": "ok"}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--dist", type=Path, default=Path("dist"))
    args = parser.parse_args()
    try:
        summary = verify_release(args.dist, args.target, args.version)
    except Exception as error:
        print(json.dumps({"status": "error", "error": str(error)}, ensure_ascii=True))
        return 1
    print(json.dumps(summary, ensure_ascii=True, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
