#!/usr/bin/env python3
"""Build native OxideTerm release artifacts without assuming a Unix shell."""

from __future__ import annotations

import base64
import os
import plistlib
import shutil
import stat
import subprocess
import sys
import tarfile
import zipfile
from dataclasses import dataclass
from pathlib import Path


ROOT_DIR = Path(__file__).resolve().parent.parent
APP_MANIFEST = ROOT_DIR / "crates" / "oxideterm-gpui-app" / "Cargo.toml"
RESOURCE_DIR = ROOT_DIR / "crates" / "oxideterm-gpui-app" / "resources"
MACOS_INFO_PLIST_EXTENSION_DIR = RESOURCE_DIR / "info"
MACOS_ENTITLEMENTS = RESOURCE_DIR / "OxideTerm.entitlements"
DIST_DIR = ROOT_DIR / "dist"
BASE_APP_NAME = "OxideTerm"
STABLE_APP_IDENTIFIER = "com.oxideterm.app"
APP_BIN = "oxideterm-native"
CLI_BIN = "oxideterm"
HELPER_BINS = ("oxideterm-rdp-helper", "oxideterm-vnc-helper")
AGENT_RESOURCE_DIR = "agents"
AGENT_BINARY_PREFIX = "oxideterm-agent-"
ENCODED_AGENT_SUFFIX = ".b64"
HELPER_RESOURCE_DIR = "helpers"
PORTABLE_MARKER_FILENAME = "portable"


@dataclass(frozen=True)
class ReleaseIdentity:
    channel: str
    app_name: str
    app_identifier: str
    windows_install_dir: str
    windows_registry_key: str
    windows_uninstall_key: str
    linux_package_name: str
    linux_install_dir: str
    linux_desktop_id: str
    linux_icon_name: str


def run(command: list[str], *, cwd: Path = ROOT_DIR, env: dict[str, str] | None = None) -> None:
    print("+", " ".join(command), flush=True)
    subprocess.run(command, cwd=cwd, env=env, check=True)


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


def raw_release_version() -> str:
    return os.environ.get("OXIDETERM_VERSION") or workspace_version()


def normalized_version(raw: str) -> str:
    for prefix in ("refs/tags/", "native-v", "gpui-v", "v"):
        if raw.startswith(prefix):
            raw = raw[len(prefix) :]
    return raw


def release_channel(raw: str, version: str) -> str:
    raw_lower = raw.lower()
    version_lower = version.lower()
    if raw_lower.startswith("gpui-v") or "gpui-preview" in version_lower:
        return "gpui-preview"
    if raw_lower.startswith("native-v") or "native-preview" in version_lower or "rustnative-preview" in version_lower:
        return "native-preview"
    if "-" in version:
        return "preview"
    return "stable"


def release_identity(raw: str, version: str) -> ReleaseIdentity:
    channel = release_channel(raw, version)
    if channel == "stable":
        return ReleaseIdentity(
            channel=channel,
            app_name=BASE_APP_NAME,
            app_identifier=STABLE_APP_IDENTIFIER,
            windows_install_dir=rf"$LOCALAPPDATA\Programs\{BASE_APP_NAME}",
            windows_registry_key=BASE_APP_NAME,
            windows_uninstall_key=BASE_APP_NAME,
            linux_package_name="oxideterm",
            linux_install_dir="oxideterm",
            linux_desktop_id=STABLE_APP_IDENTIFIER,
            linux_icon_name="oxideterm",
        )

    if channel == "gpui-preview":
        suffix = "GPUI Preview"
        app_name = f"{BASE_APP_NAME} {suffix}"
        return ReleaseIdentity(
            channel=channel,
            app_name=app_name,
            app_identifier="com.oxideterm.gpuiPreview",
            windows_install_dir=rf"$LOCALAPPDATA\Programs\{app_name}",
            windows_registry_key=app_name,
            windows_uninstall_key=app_name,
            linux_package_name="oxideterm-gpui-preview",
            linux_install_dir="oxideterm-gpui-preview",
            linux_desktop_id="com.oxideterm.gpuiPreview",
            linux_icon_name="oxideterm-gpui-preview",
        )

    app_name = f"{BASE_APP_NAME} Preview"
    return ReleaseIdentity(
        channel=channel,
        app_name=app_name,
        app_identifier="com.oxideterm.preview",
        windows_install_dir=rf"$LOCALAPPDATA\Programs\{app_name}",
        windows_registry_key=app_name,
        windows_uninstall_key=app_name,
        linux_package_name="oxideterm-preview",
        linux_install_dir="oxideterm-preview",
        linux_desktop_id="com.oxideterm.preview",
        linux_icon_name="oxideterm-preview",
    )


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


def copy_agent_resources(dst: Path, *, encode_binaries: bool) -> None:
    source_dir = RESOURCE_DIR / AGENT_RESOURCE_DIR
    if dst.exists():
        shutil.rmtree(dst)
    dst.mkdir(parents=True)
    for source in sorted(source_dir.iterdir()):
        if source.is_dir():
            copy_tree(source, dst / source.name)
            continue
        if encode_binaries and source.name.startswith(AGENT_BINARY_PREFIX):
            # AppImage tooling scans nested ELF files by architecture; encode
            # remote-agent payloads as data so both Linux agent targets remain bundled.
            encoded = base64.b64encode(source.read_bytes()).decode("ascii")
            (dst / f"{source.name}{ENCODED_AGENT_SUFFIX}").write_text(encoded, encoding="ascii")
        else:
            shutil.copy2(source, dst / source.name)


def copy_runtime_resources(dst: Path, target: str, *, encode_agent_binaries: bool = False) -> None:
    dst.mkdir(parents=True, exist_ok=True)
    # Keep the app bundle layout aligned with Tauri's resource contract: agents
    # the target-specific CLI, and protocol helpers live under resources instead
    # of relying on PATH.
    copy_agent_resources(dst / AGENT_RESOURCE_DIR, encode_binaries=encode_agent_binaries)
    copy_tree(RESOURCE_DIR / "icons", dst / "icons")

    # Do not copy stale CLI binaries for other platforms. The app resolves the
    # host-specific subdirectory first and only falls back to scanning when it is
    # missing, so release packages must contain exactly the current target CLI.
    cli_source = RESOURCE_DIR / "cli-bin" / target
    if not cli_source.exists():
        raise FileNotFoundError(f"target CLI resource directory not found: {cli_source}")
    copy_tree(cli_source, dst / "cli-bin" / target)

    helper_source = RESOURCE_DIR / HELPER_RESOURCE_DIR / target
    if not helper_source.exists():
        raise FileNotFoundError(f"target helper resource directory not found: {helper_source}")
    copy_tree(helper_source, dst / HELPER_RESOURCE_DIR / target)


def nsis_path(path: Path) -> str:
    return str(path.resolve()).replace("\\", "\\\\")


def nsis_string(value: str) -> str:
    return value.replace("$", "$$").replace('"', "$\\\"")


def find_makensis() -> str | None:
    found = shutil.which("makensis")
    if found:
        return found
    for env_name in ("ProgramFiles(x86)", "ProgramFiles"):
        root = os.environ.get(env_name)
        if not root:
            continue
        candidate = Path(root) / "NSIS" / "makensis.exe"
        if candidate.exists():
            return str(candidate)
    return None


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


def build_helper(package: str, target: str, target_was_explicit: bool) -> Path:
    args = ["cargo", "build", "-p", package, "--release"]
    if target_was_explicit:
        args.extend(["--target", target])
    run(args)

    source = release_binary(target, target_was_explicit, package)
    if not source.exists():
        raise FileNotFoundError(f"helper binary not found: {source}")

    out_dir = RESOURCE_DIR / HELPER_RESOURCE_DIR / target
    out_dir.mkdir(parents=True, exist_ok=True)
    dest = out_dir / source.name
    shutil.copy2(source, dest)
    make_executable(dest)
    print(f"Remote desktop helper artifact written to {dest}")
    return dest


def build_remote_desktop_helpers(target: str, target_was_explicit: bool) -> None:
    for package in HELPER_BINS:
        build_helper(package, target, target_was_explicit)


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


def require_tool(name: str) -> str:
    tool = shutil.which(name)
    if tool:
        return tool
    raise RuntimeError(f"required packaging tool not found: {name}")


def read_macos_info_plist_extension(path: Path) -> dict:
    text = path.read_text(encoding="utf-8")
    try:
        loaded = plistlib.loads(text.encode("utf-8"))
    except plistlib.InvalidFileException:
        # cargo-bundle accepts plist fragments containing only <key>/<value>
        # pairs. The native packager writes Info.plist itself, so wrap those
        # fragments before merging them into the generated bundle plist.
        wrapped = (
            '<?xml version="1.0" encoding="UTF-8"?>'
            '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" '
            '"https://www.apple.com/DTDs/PropertyList-1.0.dtd">'
            '<plist version="1.0"><dict>'
            f"{text}"
            "</dict></plist>"
        )
        loaded = plistlib.loads(wrapped.encode("utf-8"))
    if not isinstance(loaded, dict):
        raise RuntimeError(f"macOS Info.plist extension must be a dict: {path}")
    return loaded


def merge_plist_value(existing: object, incoming: object) -> object:
    if isinstance(existing, dict) and isinstance(incoming, dict):
        merged = dict(existing)
        merged.update(incoming)
        return merged
    return incoming


def merge_macos_info_plist_extensions(plist: dict) -> dict:
    merged = dict(plist)
    if not MACOS_INFO_PLIST_EXTENSION_DIR.exists():
        return merged
    for extension in sorted(MACOS_INFO_PLIST_EXTENSION_DIR.glob("*.plist")):
        for key, value in read_macos_info_plist_extension(extension).items():
            merged[key] = merge_plist_value(merged[key], value) if key in merged else value
    return merged


def build_macos_info_plist(version: str, identity: ReleaseIdentity) -> dict:
    plist = {
        "CFBundleDevelopmentRegion": "en",
        "CFBundleDisplayName": identity.app_name,
        "CFBundleExecutable": APP_BIN,
        "CFBundleIconFile": "icon.icns",
        "CFBundleIdentifier": identity.app_identifier,
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": identity.app_name,
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": version,
        "CFBundleVersion": version,
        "LSMinimumSystemVersion": "13.0",
        "NSHighResolutionCapable": True,
    }
    return merge_macos_info_plist_extensions(plist)


def macos_codesign_command(codesign: str, app_dir: Path) -> list[str]:
    command = [codesign, "--force", "--deep"]
    if MACOS_ENTITLEMENTS.exists():
        # Keep ad-hoc preview builds aligned with the app bundle metadata.
        # Without these entitlements, macOS can block local/private-network
        # traffic even when Info.plist contains the usage description.
        command.extend(["--entitlements", str(MACOS_ENTITLEMENTS)])
    command.extend(["--sign", "-", str(app_dir)])
    return command


def sign_macos_app_bundle(app_dir: Path) -> None:
    codesign = require_tool("codesign")
    # Ad-hoc signing does not notarize the app, but it keeps stripped preview
    # bundles launchable on Apple Silicon instead of producing a damaged-app error.
    run(macos_codesign_command(codesign, app_dir))
    run([codesign, "--verify", "--verbose", str(app_dir)])


def zip_macos_app_bundle(app_dir: Path, dest: Path) -> None:
    ditto = require_tool("ditto")
    if dest.exists():
        dest.unlink()
    # ditto preserves macOS bundle metadata and code-signing state; Python zip
    # is fine for generic archives but can break .app signature metadata.
    run([ditto, "-c", "-k", "--keepParent", app_dir.name, str(dest)], cwd=app_dir.parent)


def create_portable_package(binary: Path, target: str, version: str, label: str) -> None:
    package_root = DIST_DIR / f"OxideTerm_{version}_{label}_portable"
    if package_root.exists():
        shutil.rmtree(package_root)
    (package_root / "resources").mkdir(parents=True)

    binary_dest = package_root / binary.name
    shutil.copy2(binary, binary_dest)
    make_executable(binary_dest)
    copy_runtime_resources(package_root / "resources", target)
    for name in ("LICENSE", "NOTICE", "README.md"):
        shutil.copy2(ROOT_DIR / name, package_root / name)
    (package_root / PORTABLE_MARKER_FILENAME).touch()

    # Portable artifacts are produced with stdlib archive writers so Windows runners
    # do not depend on a Unix shell, tar, cp, or symlink behavior.
    if "windows" in target:
        zip_directory(package_root, DIST_DIR / f"OxideTerm_{version}_{label}_portable.zip")
    else:
        archive_path = DIST_DIR / f"OxideTerm_{version}_{label}_portable.tar.gz"
        with tarfile.open(archive_path, "w:gz") as archive:
            archive.add(package_root, arcname=package_root.name)
    shutil.rmtree(package_root)


def stage_windows_installer_root(binary: Path, target: str, version: str, label: str) -> Path:
    installer_root = DIST_DIR / f"nsis-{label}"
    if installer_root.exists():
        shutil.rmtree(installer_root)
    (installer_root / "resources").mkdir(parents=True)

    shutil.copy2(binary, installer_root / binary.name)
    copy_runtime_resources(installer_root / "resources", target)
    for name in ("LICENSE", "NOTICE", "README.md"):
        shutil.copy2(ROOT_DIR / name, installer_root / name)
    return installer_root


def create_windows_installer(
    binary: Path, target: str, version: str, label: str, identity: ReleaseIdentity
) -> None:
    makensis = find_makensis()
    if not makensis:
        raise RuntimeError("makensis not found; install NSIS before packaging Windows installers")

    installer_root = stage_windows_installer_root(binary, target, version, label)
    installer_path = DIST_DIR / f"OxideTerm_{version}_{label}-setup.exe"
    script_path = DIST_DIR / f"OxideTerm_{version}_{label}.nsi"
    icon_path = RESOURCE_DIR / "icons" / "icon.ico"

    # The NSIS package mirrors Tauri's current-user install mode while keeping
    # each native release channel isolated in its own registry/install scope.
    script = f"""
Unicode true
RequestExecutionLevel user
!include MUI2.nsh

Name "{identity.app_name}"
OutFile "{nsis_path(installer_path)}"
InstallDir "{identity.windows_install_dir}"
InstallDirRegKey HKCU "Software\\{identity.windows_registry_key}" "InstallDir"
Icon "{nsis_path(icon_path)}"
UninstallIcon "{nsis_path(icon_path)}"
BrandingText "{identity.app_name}"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ReadRegStr $0 HKCU "Software\\{identity.windows_registry_key}" "InstallDir"
  StrCmp $0 "" install_ready
  IfFileExists "$0\\Uninstall.exe" 0 install_ready
  MessageBox MB_ICONQUESTION|MB_YESNO "{nsis_string(identity.app_name)} is already installed.$\\r$\\n$\\r$\\nUninstall the existing installation before continuing?" IDYES uninstall_existing
  Abort

uninstall_existing:
  ClearErrors
  ExecWait '"$0\\Uninstall.exe" _?=$0' $1
  IfErrors uninstall_failed check_uninstall_exit

check_uninstall_exit:
  IntCmp $1 0 verify_uninstalled uninstall_failed uninstall_failed

verify_uninstalled:
  IfFileExists "$0\\Uninstall.exe" uninstall_failed install_ready

uninstall_failed:
  MessageBox MB_ICONEXCLAMATION "The existing {nsis_string(identity.app_name)} installation was not uninstalled. Setup will exit."
  Abort

install_ready:
FunctionEnd

Section "Install"
  SetOutPath "$INSTDIR"
  File /r "{nsis_path(installer_root)}\\*"
  WriteUninstaller "$INSTDIR\\Uninstall.exe"
  WriteRegStr HKCU "Software\\{identity.windows_registry_key}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{identity.windows_uninstall_key}" "DisplayName" "{identity.app_name}"
  WriteRegStr HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{identity.windows_uninstall_key}" "DisplayVersion" "{nsis_string(version)}"
  WriteRegStr HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{identity.windows_uninstall_key}" "Publisher" "AnalyseDeCircuit"
  WriteRegStr HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{identity.windows_uninstall_key}" "UninstallString" "$\\"$INSTDIR\\Uninstall.exe$\\""
  CreateDirectory "$SMPROGRAMS\\{identity.app_name}"
  CreateShortcut "$SMPROGRAMS\\{identity.app_name}\\{identity.app_name}.lnk" "$INSTDIR\\{binary.name}" "" "$INSTDIR\\resources\\icons\\icon.ico"
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\\{identity.app_name}\\{identity.app_name}.lnk"
  RMDir "$SMPROGRAMS\\{identity.app_name}"
  DeleteRegKey HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{identity.windows_uninstall_key}"
  DeleteRegKey HKCU "Software\\{identity.windows_registry_key}"
  RMDir /r "$INSTDIR"
SectionEnd
""".strip()
    script_path.write_text(script + "\n", encoding="utf-8")
    run([makensis, str(script_path)])
    shutil.rmtree(installer_root)
    script_path.unlink(missing_ok=True)


def create_macos_app(
    binary: Path, target: str, version: str, label: str, identity: ReleaseIdentity
) -> None:
    app_dir = DIST_DIR / f"{identity.app_name}.app"
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
    copy_runtime_resources(resources, target)

    plist = build_macos_info_plist(version, identity)
    with (contents / "Info.plist").open("wb") as file:
        plistlib.dump(plist, file)

    sign_macos_app_bundle(app_dir)
    zip_macos_app_bundle(app_dir, DIST_DIR / f"OxideTerm_{version}_{label}.app.zip")

    if shutil.which("hdiutil"):
        dmg_root = DIST_DIR / f"dmg-{label}"
        if dmg_root.exists():
            shutil.rmtree(dmg_root)
        dmg_root.mkdir()
        shutil.copytree(app_dir, dmg_root / f"{identity.app_name}.app")
        (dmg_root / "Applications").symlink_to("/Applications")
        run(
            [
                "hdiutil",
                "create",
                "-volname",
                identity.app_name,
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


def linux_deb_arch(target: str) -> str:
    mapping = {
        "x86_64-unknown-linux-gnu": "amd64",
        "aarch64-unknown-linux-gnu": "arm64",
    }
    if target not in mapping:
        raise RuntimeError(f"unsupported Linux deb target: {target}")
    return mapping[target]


def linux_deb_dependencies(target: str) -> str:
    dependencies = ["libc6 (>= 2.31)", "libgcc-s1"]
    return ", ".join(dependencies)


def linux_deb_version(version: str) -> str:
    # Debian treats '-' as the Debian revision separator. Preview semver tags
    # use '-' inside the upstream version, so store them as '~' for valid
    # package metadata while keeping release asset filenames unchanged.
    return version.replace("-", "~")


def linux_appimage_arch(target: str) -> str:
    mapping = {
        "x86_64-unknown-linux-gnu": "x86_64",
        "aarch64-unknown-linux-gnu": "aarch64",
    }
    if target not in mapping:
        raise RuntimeError(f"unsupported Linux AppImage target: {target}")
    return mapping[target]


def write_linux_desktop_file(path: Path, identity: ReleaseIdentity, exec_value: str) -> None:
    # The desktop id and icon name are channel-specific so preview installs do
    # not overwrite the future stable Linux desktop entry.
    path.write_text(
        "\n".join(
            [
                "[Desktop Entry]",
                "Type=Application",
                f"Name={identity.app_name}",
                f"Exec={exec_value} %U",
                f"Icon={identity.linux_icon_name}",
                "Terminal=false",
                "Categories=Development;TerminalEmulator;Network;",
                "StartupNotify=true",
                "",
            ]
        ),
        encoding="utf-8",
    )


def copy_linux_icons(root: Path, identity: ReleaseIdentity) -> None:
    icon_sources = {
        "32x32": "32x32.png",
        "64x64": "64x64.png",
        "128x128": "128x128.png",
        "256x256": "128x128@2x.png",
    }
    for size, source_name in icon_sources.items():
        icon_dir = root / "usr" / "share" / "icons" / "hicolor" / size / "apps"
        icon_dir.mkdir(parents=True, exist_ok=True)
        shutil.copy2(
            RESOURCE_DIR / "icons" / source_name,
            icon_dir / f"{identity.linux_icon_name}.png",
        )


def create_linux_appimage(
    binary: Path, target: str, version: str, label: str, identity: ReleaseIdentity
) -> None:
    appimagetool = shutil.which("appimagetool")
    if not appimagetool:
        raise RuntimeError("appimagetool not found; install AppImage tooling before packaging Linux")

    appdir = DIST_DIR / f"appimage-{label}"
    if appdir.exists():
        shutil.rmtree(appdir)

    usr_bin = appdir / "usr" / "bin"
    usr_bin.mkdir(parents=True)
    app_binary = usr_bin / APP_BIN
    shutil.copy2(binary, app_binary)
    make_executable(app_binary)
    copy_runtime_resources(usr_bin / "resources", target, encode_agent_binaries=True)

    applications_dir = appdir / "usr" / "share" / "applications"
    applications_dir.mkdir(parents=True)
    desktop_name = f"{identity.linux_desktop_id}.desktop"
    write_linux_desktop_file(applications_dir / desktop_name, identity, APP_BIN)
    shutil.copy2(applications_dir / desktop_name, appdir / desktop_name)

    shutil.copy2(
        RESOURCE_DIR / "icons" / "128x128.png",
        appdir / f"{identity.linux_icon_name}.png",
    )
    copy_linux_icons(appdir, identity)

    apprun = appdir / "AppRun"
    # AppImage launches from a mounted runtime, so resources must stay adjacent
    # to the binary under usr/bin for the native resource resolvers.
    apprun.write_text(
        "\n".join(
            [
                "#!/bin/sh",
                'APPDIR="${APPDIR:-$(dirname "$(readlink -f "$0")")}"',
                f'exec "$APPDIR/usr/bin/{APP_BIN}" "$@"',
                "",
            ]
        ),
        encoding="utf-8",
    )
    make_executable(apprun)

    output = DIST_DIR / f"OxideTerm_{version}_{label}.AppImage"
    env = os.environ.copy()
    env["ARCH"] = linux_appimage_arch(target)
    env.setdefault("APPIMAGE_EXTRACT_AND_RUN", "1")
    run([appimagetool, str(appdir), str(output)], env=env)
    shutil.rmtree(appdir)


def create_linux_deb(
    binary: Path, target: str, version: str, label: str, identity: ReleaseIdentity
) -> None:
    if not shutil.which("dpkg-deb"):
        raise RuntimeError("dpkg-deb not found; install dpkg before packaging Linux deb")

    deb_root = DIST_DIR / f"deb-{label}"
    if deb_root.exists():
        shutil.rmtree(deb_root)

    app_root = deb_root / "opt" / identity.linux_install_dir
    app_root.mkdir(parents=True)
    app_binary = app_root / APP_BIN
    shutil.copy2(binary, app_binary)
    make_executable(app_binary)
    copy_runtime_resources(app_root / "resources", target)
    for name in ("LICENSE", "NOTICE", "README.md"):
        shutil.copy2(ROOT_DIR / name, app_root / name)

    applications_dir = deb_root / "usr" / "share" / "applications"
    applications_dir.mkdir(parents=True)
    write_linux_desktop_file(
        applications_dir / f"{identity.linux_desktop_id}.desktop",
        identity,
        f"/opt/{identity.linux_install_dir}/{APP_BIN}",
    )
    copy_linux_icons(deb_root, identity)

    control_dir = deb_root / "DEBIAN"
    control_dir.mkdir(parents=True)
    # Keep dependencies intentionally small; GPUI links the native libraries
    # through the CI image while the application resources stay self-contained.
    control = f"""Package: {identity.linux_package_name}
Version: {linux_deb_version(version)}
Section: utils
Priority: optional
Architecture: {linux_deb_arch(target)}
Maintainer: AnalyseDeCircuit <noreply@oxideterm.app>
Depends: {linux_deb_dependencies(target)}
Description: OxideTerm native SSH workspace
 Local-first SSH workspace with terminal, SFTP, port forwarding, and AI context.
"""
    (control_dir / "control").write_text(control, encoding="utf-8")

    output = DIST_DIR / f"OxideTerm_{version}_{label}.deb"
    run(["dpkg-deb", "--build", "--root-owner-group", str(deb_root), str(output)])
    shutil.rmtree(deb_root)


def main() -> None:
    target_was_explicit = len(sys.argv) > 1 and sys.argv[1] != ""
    target = sys.argv[1] if target_was_explicit else host_triple()
    raw_version = raw_release_version()
    version = normalized_version(raw_version)
    identity = release_identity(raw_version, version)
    label = target_label(target)

    os.environ.setdefault("CLANG_MODULE_CACHE_PATH", str(ROOT_DIR / "target" / "clang-module-cache"))
    Path(os.environ["CLANG_MODULE_CACHE_PATH"]).mkdir(parents=True, exist_ok=True)

    if DIST_DIR.exists():
        shutil.rmtree(DIST_DIR)
    DIST_DIR.mkdir()

    print(
        f"==> Packaging {identity.app_name} {version} ({identity.channel}) for {target}",
        flush=True,
    )
    build_cli(target, target_was_explicit)
    build_remote_desktop_helpers(target, target_was_explicit)
    app_binary = build_app(target, target_was_explicit)
    if "windows" in target:
        create_windows_installer(app_binary, target, version, label, identity)
    # Every target should publish a self-contained portable artifact; Windows
    # additionally ships an NSIS installer for users who prefer installation.
    create_portable_package(app_binary, target, version, label)
    if "apple-darwin" in target:
        create_macos_app(app_binary, target, version, label, identity)
    if "linux" in target:
        create_linux_appimage(app_binary, target, version, label, identity)
        create_linux_deb(app_binary, target, version, label, identity)

    print("==> Package artifacts", flush=True)
    for path in sorted(DIST_DIR.iterdir()):
        if path.is_file():
            print(path)


if __name__ == "__main__":
    main()
