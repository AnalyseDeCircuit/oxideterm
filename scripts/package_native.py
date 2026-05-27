#!/usr/bin/env python3
"""Build native OxideTerm release artifacts without assuming a Unix shell."""

from __future__ import annotations

import os
import plistlib
import shutil
import stat
import subprocess
import sys
import tarfile
import zipfile
from pathlib import Path


ROOT_DIR = Path(__file__).resolve().parent.parent
APP_MANIFEST = ROOT_DIR / "crates" / "oxideterm-gpui-app" / "Cargo.toml"
RESOURCE_DIR = ROOT_DIR / "crates" / "oxideterm-gpui-app" / "resources"
DIST_DIR = ROOT_DIR / "dist"
APP_NAME = "OxideTerm"
APP_IDENTIFIER = "com.analysecircuit.OxideTerm"
APP_BIN = "oxideterm-native"
CLI_BIN = "oxideterm"


def run(command: list[str], *, cwd: Path = ROOT_DIR) -> None:
    print("+", " ".join(command), flush=True)
    subprocess.run(command, cwd=cwd, check=True)


def host_triple() -> str:
    output = subprocess.check_output(["rustc", "-vV"], text=True)
    for line in output.splitlines():
        if line.startswith("host:"):
            return line.split(":", 1)[1].strip()
    raise RuntimeError("rustc -vV did not report a host triple")


def workspace_version() -> str:
    in_workspace_package = False
    for line in (ROOT_DIR / "Cargo.toml").read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if stripped == "[workspace.package]":
            in_workspace_package = True
            continue
        if in_workspace_package and stripped.startswith("["):
            break
        if in_workspace_package and stripped.startswith("version"):
            return stripped.split('"', 2)[1]
    raise RuntimeError("workspace.package version not found")


def normalized_version() -> str:
    raw = os.environ.get("OXIDETERM_VERSION") or workspace_version()
    for prefix in ("refs/tags/", "native-v", "gpui-v", "v"):
        if raw.startswith(prefix):
            raw = raw[len(prefix) :]
    return raw


def target_label(triple: str) -> str:
    labels = {
        "x86_64-apple-darwin": "macos_x64",
        "aarch64-apple-darwin": "macos_arm64",
        "x86_64-pc-windows-msvc": "windows_x64",
        "aarch64-pc-windows-msvc": "windows_arm64",
        "x86_64-unknown-linux-gnu": "linux_x64",
        "aarch64-unknown-linux-gnu": "linux_arm64",
    }
    return labels.get(triple, triple.replace("-", "_"))


def release_binary(target: str, target_was_explicit: bool, name: str) -> Path:
    binary_name = f"{name}.exe" if "windows" in target else name
    if target_was_explicit:
        return ROOT_DIR / "target" / target / "release" / binary_name
    return ROOT_DIR / "target" / "release" / binary_name


def make_executable(path: Path) -> None:
    if os.name == "nt":
        return
    mode = path.stat().st_mode
    path.chmod(mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def copy_tree(src: Path, dst: Path) -> None:
    if dst.exists():
        shutil.rmtree(dst)
    shutil.copytree(src, dst)


def copy_runtime_resources(dst: Path) -> None:
    dst.mkdir(parents=True, exist_ok=True)
    # Keep the app bundle layout aligned with Tauri's resource contract: agents and
    # the target-specific CLI live under resources instead of being discovered by PATH.
    for name in ("agents", "cli-bin", "icons"):
        copy_tree(RESOURCE_DIR / name, dst / name)


def build_cli(target: str, target_was_explicit: bool) -> Path:
    args = ["cargo", "build", "-p", "oxideterm-cli", "--release"]
    if target_was_explicit:
        args.extend(["--target", target])
    run(args)

    source = release_binary(target, target_was_explicit, CLI_BIN)
    if not source.exists():
        raise FileNotFoundError(f"CLI binary not found: {source}")

    out_dir = RESOURCE_DIR / "cli-bin" / target
    out_dir.mkdir(parents=True, exist_ok=True)
    dest = out_dir / source.name
    shutil.copy2(source, dest)
    make_executable(dest)
    print(f"CLI artifact written to {dest}")
    return dest


def build_app(target: str, target_was_explicit: bool) -> Path:
    args = [
        "cargo",
        "build",
        "--manifest-path",
        str(APP_MANIFEST),
        "--bin",
        APP_BIN,
        "--release",
    ]
    if target_was_explicit:
        args.extend(["--target", target])
    run(args)

    source = release_binary(target, target_was_explicit, APP_BIN)
    if not source.exists():
        raise FileNotFoundError(f"app binary not found: {source}")
    return source


def zip_directory(src: Path, dest: Path) -> None:
    with zipfile.ZipFile(dest, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(src.rglob("*")):
            arcname = path.relative_to(src.parent)
            info = zipfile.ZipInfo.from_file(path, arcname.as_posix())
            if path.is_dir():
                archive.writestr(info, b"")
                continue
            if os.name != "nt" and os.access(path, os.X_OK):
                info.external_attr = (0o100755 & 0xFFFF) << 16
            with path.open("rb") as file:
                archive.writestr(info, file.read())


def create_portable_package(binary: Path, target: str, version: str, label: str) -> None:
    package_root = DIST_DIR / f"OxideTerm_{version}_{label}_portable"
    if package_root.exists():
        shutil.rmtree(package_root)
    (package_root / "resources").mkdir(parents=True)

    binary_dest = package_root / binary.name
    shutil.copy2(binary, binary_dest)
    make_executable(binary_dest)
    copy_runtime_resources(package_root / "resources")
    for name in ("LICENSE", "NOTICE", "README.md"):
        shutil.copy2(ROOT_DIR / name, package_root / name)

    # Portable artifacts are produced with stdlib archive writers so Windows runners
    # do not depend on a Unix shell, tar, cp, or symlink behavior.
    if "windows" in target:
        zip_directory(package_root, DIST_DIR / f"OxideTerm_{version}_{label}_portable.zip")
    else:
        archive_path = DIST_DIR / f"OxideTerm_{version}_{label}_portable.tar.gz"
        with tarfile.open(archive_path, "w:gz") as archive:
            archive.add(package_root, arcname=package_root.name)
    shutil.rmtree(package_root)


def create_macos_app(binary: Path, version: str, label: str) -> None:
    app_dir = DIST_DIR / f"{APP_NAME}.app"
    if app_dir.exists():
        shutil.rmtree(app_dir)

    contents = app_dir / "Contents"
    macos = contents / "MacOS"
    resources = contents / "Resources"
    macos.mkdir(parents=True)
    resources.mkdir(parents=True)

    app_binary = macos / APP_BIN
    shutil.copy2(binary, app_binary)
    make_executable(app_binary)
    shutil.copy2(RESOURCE_DIR / "icons" / "icon.icns", resources / "icon.icns")
    copy_runtime_resources(resources)

    plist = {
        "CFBundleDevelopmentRegion": "en",
        "CFBundleDisplayName": APP_NAME,
        "CFBundleExecutable": APP_BIN,
        "CFBundleIconFile": "icon.icns",
        "CFBundleIdentifier": APP_IDENTIFIER,
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": APP_NAME,
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": version,
        "CFBundleVersion": version,
        "LSMinimumSystemVersion": "13.0",
        "NSHighResolutionCapable": True,
    }
    with (contents / "Info.plist").open("wb") as file:
        plistlib.dump(plist, file)

    zip_directory(app_dir, DIST_DIR / f"OxideTerm_{version}_{label}.app.zip")

    if shutil.which("hdiutil"):
        dmg_root = DIST_DIR / f"dmg-{label}"
        if dmg_root.exists():
            shutil.rmtree(dmg_root)
        dmg_root.mkdir()
        shutil.copytree(app_dir, dmg_root / f"{APP_NAME}.app")
        (dmg_root / "Applications").symlink_to("/Applications")
        run(
            [
                "hdiutil",
                "create",
                "-volname",
                APP_NAME,
                "-srcfolder",
                str(dmg_root),
                "-ov",
                "-format",
                "UDZO",
                str(DIST_DIR / f"OxideTerm_{version}_{label}.dmg"),
            ]
        )
        shutil.rmtree(dmg_root)

    shutil.rmtree(app_dir)


def main() -> None:
    target_was_explicit = len(sys.argv) > 1 and sys.argv[1] != ""
    target = sys.argv[1] if target_was_explicit else host_triple()
    version = normalized_version()
    label = target_label(target)

    os.environ.setdefault("CLANG_MODULE_CACHE_PATH", str(ROOT_DIR / "target" / "clang-module-cache"))
    Path(os.environ["CLANG_MODULE_CACHE_PATH"]).mkdir(parents=True, exist_ok=True)

    if DIST_DIR.exists():
        shutil.rmtree(DIST_DIR)
    DIST_DIR.mkdir()

    print(f"==> Packaging {APP_NAME} {version} for {target}", flush=True)
    build_cli(target, target_was_explicit)
    app_binary = build_app(target, target_was_explicit)
    create_portable_package(app_binary, target, version, label)
    if "apple-darwin" in target:
        create_macos_app(app_binary, version, label)

    print("==> Package artifacts", flush=True)
    for path in sorted(DIST_DIR.iterdir()):
        if path.is_file():
            print(path)


if __name__ == "__main__":
    main()
