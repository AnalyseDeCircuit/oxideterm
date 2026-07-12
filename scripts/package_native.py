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
MACOS_UNSIGNED_INSTALL_NOTICE = (
    RESOURCE_DIR / "macos" / "unsigned-install-notice.png"
)
MACOS_UNSIGNED_INSTALL_NOTICE_NAME = "READ FIRST - 未签名安装说明.png"
MACOS_UNSIGNED_DMG_BACKGROUND = (
    RESOURCE_DIR / "macos" / "unsigned-dmg-background.png"
)
MACOS_DMG_BACKGROUND_DIR_NAME = ".background"
MACOS_DMG_BACKGROUND_NAME = "unsigned-dmg-background.png"
DIST_DIR = ROOT_DIR / "dist"
BASE_APP_NAME = "OxideTerm"
STABLE_APP_IDENTIFIER = "com.oxideterm.app"
APP_BIN = "oxideterm-native"
CLI_BIN = "oxideterm"
HELPER_BINS = ("oxideterm-rdp-helper", "oxideterm-vnc-helper")
UPDATE_HELPER_PACKAGE = "oxideterm-update"
UPDATE_HELPER_BIN = "oxideterm-update-helper"
AGENT_RESOURCE_DIR = "agents"
AGENT_BINARY_PREFIX = "oxideterm-agent-"
ENCODED_AGENT_SUFFIX = ".b64"
HELPER_RESOURCE_DIR = "helpers"
WINDOWS_UPDATE_HELPER_DIR = "tools"
WINDOWS_UPDATE_STAGING_DIR = "install"
WINDOWS_UPDATE_FLAG = "OXIDETERM_UPDATE"
PORTABLE_MARKER_FILENAME = "portable"
PACKAGE_VERSION_FILENAME = "VERSION"
LINUX_PACKAGE_KIND_FILENAME = "PACKAGE_KIND"
RELEASE_DOCUMENTS = (
    (ROOT_DIR / "LICENSE", "LICENSE"),
    (ROOT_DIR / "NOTICE", "NOTICE"),
    (ROOT_DIR / "README.md", "README.md"),
    (ROOT_DIR / "THIRD_PARTY_NOTICES.md", "THIRD_PARTY_NOTICES.md"),
    (
        ROOT_DIR / "agent" / "THIRD_PARTY_NOTICES.md",
        "AGENT_THIRD_PARTY_NOTICES.md",
    ),
)


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


def validate_release_version(raw: str, version: str) -> None:
    """Reject artifacts whose filename version differs from the compiled version."""
    compiled_version = workspace_version()
    if version != compiled_version:
        raise RuntimeError(
            f"release version {version!r} from {raw!r} does not match "
            f"workspace version {compiled_version!r}; run scripts/bump_version.py first"
        )


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


def copy_release_documents(dst: Path) -> None:
    """Copy the license and generated dependency notices shipped with binaries."""
    dst.mkdir(parents=True, exist_ok=True)
    for source, destination_name in RELEASE_DOCUMENTS:
        if not source.is_file():
            raise FileNotFoundError(f"release document not found: {source}")
        shutil.copy2(source, dst / destination_name)


def write_package_version(dst: Path, version: str) -> None:
    """Persist the exact workspace version inside every release package."""
    dst.mkdir(parents=True, exist_ok=True)
    (dst / PACKAGE_VERSION_FILENAME).write_text(f"{version}\n", encoding="utf-8")


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


def build_windows_update_helper(target: str, target_was_explicit: bool) -> Path:
    args = [
        "cargo",
        "build",
        "-p",
        UPDATE_HELPER_PACKAGE,
        "--bin",
        UPDATE_HELPER_BIN,
        "--release",
    ]
    if target_was_explicit:
        args.extend(["--target", target])
    run(args)

    source = release_binary(target, target_was_explicit, UPDATE_HELPER_BIN)
    if not source.exists():
        raise FileNotFoundError(f"Windows update helper binary not found: {source}")
    return source


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


def find_7zip() -> str | None:
    for name in ("7z", "7zz", "7za"):
        found = shutil.which(name)
        if found:
            return found
    return None


def archive_windows_portable(package_root: Path, dest: Path) -> None:
    if dest.exists():
        dest.unlink()
    if seven_zip := find_7zip():
        # Keep the published artifact name as .zip while using 7-Zip's stronger
        # Deflate settings to avoid a much larger portable package than NSIS.
        run(
            [
                seven_zip,
                "a",
                "-tzip",
                "-mx=9",
                "-mfb=258",
                "-mpass=15",
                str(dest),
                package_root.name,
            ],
            cwd=package_root.parent,
        )
        return
    zip_directory(package_root, dest)


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
    signing_identity = os.environ.get("MACOS_CODESIGN_IDENTITY", "-").strip() or "-"
    if signing_identity != "-":
        command.extend(["--options", "runtime", "--timestamp"])
    if MACOS_ENTITLEMENTS.exists():
        # Keep ad-hoc preview builds aligned with the app bundle metadata.
        # Without these entitlements, macOS can block local/private-network
        # traffic even when Info.plist contains the usage description.
        command.extend(["--entitlements", str(MACOS_ENTITLEMENTS)])
    command.extend(["--sign", signing_identity, str(app_dir)])
    return command


def should_include_macos_unsigned_install_notice(identity: ReleaseIdentity) -> bool:
    """Include Gatekeeper help only in an unsigned stable-release DMG."""
    signing_identity = os.environ.get("MACOS_CODESIGN_IDENTITY", "-").strip() or "-"
    return identity.channel == "stable" and signing_identity == "-"


def copy_macos_unsigned_install_notice(
    dmg_root: Path, identity: ReleaseIdentity
) -> bool:
    """Copy the visible Gatekeeper guide into an eligible DMG staging root."""
    if not should_include_macos_unsigned_install_notice(identity):
        return False
    shutil.copy2(
        MACOS_UNSIGNED_INSTALL_NOTICE,
        dmg_root / MACOS_UNSIGNED_INSTALL_NOTICE_NAME,
    )
    background_dir = dmg_root / MACOS_DMG_BACKGROUND_DIR_NAME
    background_dir.mkdir(exist_ok=True)
    shutil.copy2(
        MACOS_UNSIGNED_DMG_BACKGROUND,
        background_dir / MACOS_DMG_BACKGROUND_NAME,
    )
    return True


def macos_dmg_finder_script() -> str:
    """Return the Finder layout script for the unsigned stable DMG."""
    return f'''
on run argv
    set mountPath to item 1 of argv
    set appName to item 2 of argv
    set noticeName to item 3 of argv
    set dmgFolder to POSIX file mountPath as alias
    tell application "Finder"
        open dmgFolder
        set dmgWindow to container window of dmgFolder
        set current view of dmgWindow to icon view
        set toolbar visible of dmgWindow to false
        set statusbar visible of dmgWindow to false
        set pathbar visible of dmgWindow to false
        set bounds of dmgWindow to {{120, 120, 840, 600}}
        set theViewOptions to icon view options of dmgWindow
        set arrangement of theViewOptions to not arranged
        set icon size of theViewOptions to 96
        set text size of theViewOptions to 13
        set background picture of theViewOptions to file "{MACOS_DMG_BACKGROUND_DIR_NAME}:{MACOS_DMG_BACKGROUND_NAME}" of dmgFolder
        set position of item appName of dmgFolder to {{180, 205}}
        set position of item "Applications" of dmgFolder to {{540, 205}}
        set position of item noticeName of dmgFolder to {{620, 390}}
        update dmgFolder without registering applications
        delay 1
        close dmgWindow
    end tell
end run
'''.strip()


def create_macos_dmg(
    dmg_root: Path, dmg_path: Path, identity: ReleaseIdentity
) -> None:
    """Create a compressed DMG, applying Finder chrome when it is available."""
    if not should_include_macos_unsigned_install_notice(identity):
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
                str(dmg_path),
            ]
        )
        return

    writable_dmg = dmg_path.with_name(f".{dmg_path.stem}.writable.dmg")
    mount_point = dmg_path.with_name(f".{dmg_path.stem}.mount")
    writable_dmg.unlink(missing_ok=True)
    if mount_point.exists():
        shutil.rmtree(mount_point)
    mount_point.mkdir()

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
            "UDRW",
            str(writable_dmg),
        ]
    )
    attached = False
    try:
        run(
            [
                "hdiutil",
                "attach",
                "-readwrite",
                "-noverify",
                "-noautoopen",
                "-mountpoint",
                str(mount_point),
                str(writable_dmg),
            ]
        )
        attached = True
        layout = subprocess.run(
            [
                "osascript",
                "-e",
                macos_dmg_finder_script(),
                str(mount_point),
                f"{identity.app_name}.app",
                MACOS_UNSIGNED_INSTALL_NOTICE_NAME,
            ],
            cwd=ROOT_DIR,
            text=True,
            capture_output=True,
            check=False,
        )
        if layout.returncode != 0:
            print(
                "warning: Finder DMG layout was unavailable; using default icon positions: "
                f"{layout.stderr.strip()}",
                file=sys.stderr,
            )
        subprocess.run(["sync"], check=True)
    finally:
        if attached:
            run(["hdiutil", "detach", str(mount_point)])
        if mount_point.exists():
            shutil.rmtree(mount_point)

    run(
        [
            "hdiutil",
            "convert",
            str(writable_dmg),
            "-ov",
            "-format",
            "UDZO",
            "-imagekey",
            "zlib-level=9",
            "-o",
            str(dmg_path),
        ]
    )
    writable_dmg.unlink(missing_ok=True)


def sign_macos_path(path: Path) -> None:
    codesign = require_tool("codesign")
    # Ad-hoc signing does not notarize the app, but it keeps stripped preview
    # bundles launchable on Apple Silicon instead of producing a damaged-app error.
    run(macos_codesign_command(codesign, path))
    run([codesign, "--verify", "--verbose", str(path)])


def is_macos_binary(path: Path) -> bool:
    """Return whether `path` contains Mach-O code that needs its own signature."""
    if not path.is_file():
        return False
    file_tool = require_tool("file")
    description = subprocess.check_output(
        [file_tool, "-b", str(path)], text=True, stderr=subprocess.DEVNULL
    )
    return "Mach-O" in description


def sign_macos_portable_tree(package_root: Path) -> None:
    """Sign every Mach-O payload because a portable directory is not a bundle."""
    for candidate in sorted(package_root.rglob("*")):
        if is_macos_binary(candidate):
            sign_macos_path(candidate)


def notarize_macos_portable_tree(package_root: Path) -> None:
    """Submit signed command-line payloads in Apple's supported ZIP container."""
    submission = DIST_DIR / f".{package_root.name}-notarization.zip"
    zip_macos_app_bundle(package_root, submission)
    try:
        notarize_macos_artifact(submission, staple=False)
    finally:
        submission.unlink(missing_ok=True)


def notarize_macos_artifact(path: Path, *, staple: bool) -> bool:
    """Submit an artifact when App Store Connect API credentials are configured."""
    key_path = os.environ.get("MACOS_NOTARY_KEY_PATH", "").strip()
    key_id = os.environ.get("MACOS_NOTARY_KEY_ID", "").strip()
    issuer_id = os.environ.get("MACOS_NOTARY_ISSUER_ID", "").strip()
    configured = [bool(key_path), bool(key_id), bool(issuer_id)]
    if not any(configured):
        return False
    if not all(configured):
        raise RuntimeError("macOS notarization requires key path, key id, and issuer id")
    if os.environ.get("MACOS_CODESIGN_IDENTITY", "-").strip() in ("", "-"):
        raise RuntimeError("macOS notarization requires a Developer ID signing identity")

    run(
        [
            "xcrun",
            "notarytool",
            "submit",
            str(path),
            "--key",
            key_path,
            "--key-id",
            key_id,
            "--issuer",
            issuer_id,
            "--wait",
        ]
    )
    if staple:
        run(["xcrun", "stapler", "staple", str(path)])
    return True


def find_signtool() -> str | None:
    found = shutil.which("signtool")
    if found:
        return found
    kits_root = os.environ.get("ProgramFiles(x86)")
    if not kits_root:
        return None
    candidates = sorted(
        Path(kits_root).glob("Windows Kits/10/bin/*/x64/signtool.exe"),
        reverse=True,
    )
    return str(candidates[0]) if candidates else None


def sign_windows_file(path: Path) -> None:
    """Authenticode-sign a release file using a pre-imported certificate."""
    thumbprint = os.environ.get("WINDOWS_SIGN_CERT_SHA1", "").strip().replace(" ", "")
    if not thumbprint:
        return
    signtool = find_signtool()
    if not signtool:
        raise RuntimeError("WINDOWS_SIGN_CERT_SHA1 is set but signtool was not found")
    timestamp_url = os.environ.get(
        "WINDOWS_SIGN_TIMESTAMP_URL", "http://timestamp.digicert.com"
    )
    run(
        [
            signtool,
            "sign",
            "/fd",
            "SHA256",
            "/td",
            "SHA256",
            "/tr",
            timestamp_url,
            "/sha1",
            thumbprint,
            str(path),
        ]
    )
    run([signtool, "verify", "/pa", "/v", str(path)])


def zip_macos_app_bundle(app_dir: Path, dest: Path) -> None:
    ditto = require_tool("ditto")
    if dest.exists():
        dest.unlink()
    # ditto preserves macOS bundle metadata and code-signing state; Python zip
    # is fine for generic archives but can break .app signature metadata.
    run([ditto, "-c", "-k", "--keepParent", app_dir.name, str(dest)], cwd=app_dir.parent)


def archive_macos_tauri_bundle(app_dir: Path, dest: Path) -> None:
    """Create the app.tar.gz shape consumed by the OxideTerm 1.x updater."""
    if dest.exists():
        dest.unlink()
    with tarfile.open(dest, "w:gz", format=tarfile.PAX_FORMAT) as archive:
        archive.add(app_dir, arcname=app_dir.name)


def create_portable_package(binary: Path, target: str, version: str, label: str) -> None:
    package_root = DIST_DIR / f"OxideTerm_{version}_{label}_portable"
    if package_root.exists():
        shutil.rmtree(package_root)
    (package_root / "resources").mkdir(parents=True)

    binary_dest = package_root / binary.name
    shutil.copy2(binary, binary_dest)
    make_executable(binary_dest)
    copy_runtime_resources(package_root / "resources", target)
    copy_release_documents(package_root)
    write_package_version(package_root, version)
    (package_root / PORTABLE_MARKER_FILENAME).touch()

    if "apple-darwin" in target:
        # Standalone helpers are outside an app bundle, so each Mach-O file must
        # be signed and included in its own notarization submission.
        sign_macos_portable_tree(package_root)
        notarize_macos_portable_tree(package_root)

    if "windows" in target:
        archive_windows_portable(
            package_root,
            DIST_DIR / f"OxideTerm_{version}_{label}_portable.zip",
        )
    else:
        archive_path = DIST_DIR / f"OxideTerm_{version}_{label}_portable.tar.gz"
        with tarfile.open(archive_path, "w:gz") as archive:
            archive.add(package_root, arcname=package_root.name)
    shutil.rmtree(package_root)


def stage_windows_installer_root(
    binary: Path, target: str, version: str, label: str, update_helper: Path
) -> Path:
    installer_root = DIST_DIR / f"nsis-{label}"
    if installer_root.exists():
        shutil.rmtree(installer_root)
    (installer_root / "resources").mkdir(parents=True)
    (installer_root / WINDOWS_UPDATE_HELPER_DIR).mkdir(parents=True)

    shutil.copy2(binary, installer_root / binary.name)
    shutil.copy2(update_helper, installer_root / WINDOWS_UPDATE_HELPER_DIR / update_helper.name)
    copy_runtime_resources(installer_root / "resources", target)
    copy_release_documents(installer_root)
    write_package_version(installer_root, version)
    return installer_root


def create_windows_installer(
    binary: Path,
    target: str,
    target_was_explicit: bool,
    version: str,
    label: str,
    identity: ReleaseIdentity,
) -> None:
    makensis = find_makensis()
    if not makensis:
        raise RuntimeError("makensis not found; install NSIS before packaging Windows installers")

    update_helper = build_windows_update_helper(target, target_was_explicit)
    sign_windows_file(update_helper)
    installer_root = stage_windows_installer_root(binary, target, version, label, update_helper)
    installer_path = DIST_DIR / f"OxideTerm_{version}_{label}-setup.exe"
    script_path = DIST_DIR / f"OxideTerm_{version}_{label}.nsi"
    icon_path = RESOURCE_DIR / "icons" / "icon.ico"

    script = windows_installer_script(
        binary=binary,
        version=version,
        identity=identity,
        installer_root=installer_root,
        installer_path=installer_path,
        icon_path=icon_path,
    )
    script_path.write_text(script + "\n", encoding="utf-8")
    run([makensis, str(script_path)])
    sign_windows_file(installer_path)
    shutil.rmtree(installer_root)
    script_path.unlink(missing_ok=True)


def windows_installer_script(
    *,
    binary: Path,
    version: str,
    identity: ReleaseIdentity,
    installer_root: Path,
    installer_path: Path,
    icon_path: Path,
) -> str:
    # The NSIS package mirrors Tauri's current-user install mode while keeping
    # each native release channel isolated in its own registry/install scope.
    # Automatic updates stage payloads first; the helper performs the final
    # replacement after the running app has exited.
    legacy_upgrade_init = ""
    if identity.channel == "stable":
        legacy_upgrade_init = rf"""
  ${{If}} $IsOxideUpdate == "0"
  ${{AndIf}} ${{FileExists}} "$LOCALAPPDATA\OxideTerm\oxideterm.exe"
    StrCpy $INSTDIR "$LOCALAPPDATA\OxideTerm"
    StrCpy $IsOxideUpdate "1"
    StrCpy $IsLegacyUpgrade "1"
    SetSilent silent
  ${{EndIf}}"""

    return f"""
Unicode true
RequestExecutionLevel user
!include MUI2.nsh
!include FileFunc.nsh
!include LogicLib.nsh

Name "{identity.app_name}"
OutFile "{nsis_path(installer_path)}"
InstallDir "{identity.windows_install_dir}"
InstallDirRegKey HKCU "Software\\{identity.windows_registry_key}" "InstallDir"
Icon "{nsis_path(icon_path)}"
UninstallIcon "{nsis_path(icon_path)}"
BrandingText "{identity.app_name}"
VIProductVersion "{windows_numeric_version(version)}"
VIAddVersionKey /LANG=1033 "ProductVersion" "{nsis_string(version)}"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Var IsOxideUpdate
Var IsLegacyUpgrade

Function .onInit
  StrCpy $IsLegacyUpgrade "0"
  ${{GetOptions}} "$CMDLINE" "/{WINDOWS_UPDATE_FLAG}=1" $IsOxideUpdate
  IfErrors check_legacy_install oxide_update_mode
check_legacy_install:
  StrCpy $IsOxideUpdate "0"
{legacy_upgrade_init}
  Return
oxide_update_mode:
  StrCpy $IsOxideUpdate "1"
  SetSilent silent
FunctionEnd

Section "Application Files"
  SectionIn RO
  StrCmp $IsOxideUpdate "1" update_install normal_install

normal_install:
  SetOutPath "$INSTDIR"
  SetOverwrite on
  File /r "{nsis_path(installer_root)}\\*"
  WriteUninstaller "$INSTDIR\\Uninstall.exe"
  WriteRegStr HKCU "Software\\{identity.windows_registry_key}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{identity.windows_uninstall_key}" "DisplayName" "{identity.app_name}"
  WriteRegStr HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{identity.windows_uninstall_key}" "DisplayVersion" "{nsis_string(version)}"
  WriteRegStr HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{identity.windows_uninstall_key}" "Publisher" "AnalyseDeCircuit"
  WriteRegStr HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{identity.windows_uninstall_key}" "UninstallString" "$\\"$INSTDIR\\Uninstall.exe$\\""
  Goto install_done

update_install:
  RMDir /r "$INSTDIR\\{WINDOWS_UPDATE_STAGING_DIR}"
  CreateDirectory "$INSTDIR\\{WINDOWS_UPDATE_HELPER_DIR}"
  SetOutPath "$INSTDIR\\{WINDOWS_UPDATE_HELPER_DIR}"
  SetOverwrite on
  File "{nsis_path(installer_root / WINDOWS_UPDATE_HELPER_DIR / (UPDATE_HELPER_BIN + '.exe'))}"
  SetOutPath "$INSTDIR\\{WINDOWS_UPDATE_STAGING_DIR}"
  File /r "{nsis_path(installer_root)}\\*"
  WriteRegStr HKCU "Software\\{identity.windows_registry_key}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{identity.windows_uninstall_key}" "DisplayName" "{identity.app_name}"
  WriteRegStr HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{identity.windows_uninstall_key}" "DisplayVersion" "{nsis_string(version)}"
  WriteRegStr HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{identity.windows_uninstall_key}" "Publisher" "AnalyseDeCircuit"
  WriteRegStr HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{identity.windows_uninstall_key}" "UninstallString" "$\\"$INSTDIR\\Uninstall.exe$\\""
  StrCmp $IsLegacyUpgrade "1" 0 legacy_shortcuts_done
  CreateDirectory "$SMPROGRAMS\\{identity.app_name}"
  CreateShortcut "$SMPROGRAMS\\{identity.app_name}\\{identity.app_name}.lnk" "$INSTDIR\\{binary.name}" "" "$INSTDIR\\resources\\icons\\icon.ico"
  IfFileExists "$DESKTOP\\{identity.app_name}.lnk" 0 legacy_shortcuts_done
  CreateShortcut "$DESKTOP\\{identity.app_name}.lnk" "$INSTDIR\\{binary.name}" "" "$INSTDIR\\resources\\icons\\icon.ico"
legacy_shortcuts_done:
  Exec '"$INSTDIR\\{WINDOWS_UPDATE_HELPER_DIR}\\{UPDATE_HELPER_BIN}.exe" --install-dir "$INSTDIR" --app-exe "$INSTDIR\\{binary.name}" --launch'

install_done:
SectionEnd

Section "Start Menu Shortcut"
  StrCmp $IsOxideUpdate "1" start_menu_shortcut_done
  CreateDirectory "$SMPROGRAMS\\{identity.app_name}"
  CreateShortcut "$SMPROGRAMS\\{identity.app_name}\\{identity.app_name}.lnk" "$INSTDIR\\{binary.name}" "" "$INSTDIR\\resources\\icons\\icon.ico"
start_menu_shortcut_done:
SectionEnd

Section /o "Desktop Shortcut"
  StrCmp $IsOxideUpdate "1" desktop_shortcut_done
  CreateShortcut "$DESKTOP\\{identity.app_name}.lnk" "$INSTDIR\\{binary.name}" "" "$INSTDIR\\resources\\icons\\icon.ico"
desktop_shortcut_done:
SectionEnd

Section "Uninstall"
  Delete "$DESKTOP\\{identity.app_name}.lnk"
  Delete "$SMPROGRAMS\\{identity.app_name}\\{identity.app_name}.lnk"
  RMDir "$SMPROGRAMS\\{identity.app_name}"
  DeleteRegKey HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{identity.windows_uninstall_key}"
  DeleteRegKey HKCU "Software\\{identity.windows_registry_key}"
  RMDir /r "$INSTDIR"
SectionEnd
""".strip()


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
    copy_release_documents(resources / "licenses")
    write_package_version(resources / "licenses", version)

    plist = build_macos_info_plist(version, identity)
    with (contents / "Info.plist").open("wb") as file:
        plistlib.dump(plist, file)

    sign_macos_path(app_dir)
    app_zip = DIST_DIR / f"OxideTerm_{version}_{label}.app.zip"
    zip_macos_app_bundle(app_dir, app_zip)
    if notarize_macos_artifact(app_zip, staple=False):
        # The ZIP is only the submission container. Staple the accepted ticket
        # to the application itself, then rebuild the distributable archive.
        run(["xcrun", "stapler", "staple", str(app_dir)])
        zip_macos_app_bundle(app_dir, app_zip)

    if identity.channel == "stable":
        # OxideTerm 1.x uses Tauri's app.tar.gz installer contract. Keep this
        # bridge asset beside the native ZIP until the 1.x population retires.
        archive_macos_tauri_bundle(
            app_dir,
            DIST_DIR / f"OxideTerm_{version}_{label}.app.tar.gz",
        )

    if shutil.which("hdiutil"):
        dmg_root = DIST_DIR / f"dmg-{label}"
        if dmg_root.exists():
            shutil.rmtree(dmg_root)
        dmg_root.mkdir()
        shutil.copytree(app_dir, dmg_root / f"{identity.app_name}.app")
        (dmg_root / "Applications").symlink_to("/Applications")
        copy_macos_unsigned_install_notice(dmg_root, identity)
        dmg_path = DIST_DIR / f"OxideTerm_{version}_{label}.dmg"
        create_macos_dmg(dmg_root, dmg_path, identity)
        notarize_macos_artifact(dmg_path, staple=True)
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


def linux_rpm_arch(target: str) -> str:
    """Map Rust Linux targets to RPM architecture names."""
    mapping = {
        "x86_64-unknown-linux-gnu": "x86_64",
        "aarch64-unknown-linux-gnu": "aarch64",
    }
    if target not in mapping:
        raise RuntimeError(f"unsupported Linux rpm target: {target}")
    return mapping[target]


def linux_rpm_version_release(version: str) -> tuple[str, str]:
    """Convert SemVer prereleases into RPM's Version and Release fields."""
    if "-" not in version:
        return version, "1"
    upstream, prerelease = version.split("-", 1)
    normalized_prerelease = "".join(
        character if character.isalnum() else "." for character in prerelease
    ).strip(".")
    return upstream, f"0.{normalized_prerelease or 'preview'}"


def parse_dpkg_shlibdeps_output(output: str) -> str:
    """Extract the dependency expression emitted by dpkg-shlibdeps."""
    prefix = "shlibs:Depends="
    for line in output.splitlines():
        if line.startswith(prefix):
            dependencies = line[len(prefix) :].strip()
            if dependencies:
                return dependencies
    raise RuntimeError("dpkg-shlibdeps did not report shlibs:Depends")


def linux_deb_dependencies(binary: Path, scratch_dir: Path) -> str:
    """Resolve Debian runtime dependencies from the final linked executable."""
    dpkg_shlibdeps = require_tool("dpkg-shlibdeps")
    debian_dir = scratch_dir / "debian"
    debian_dir.mkdir(parents=True, exist_ok=True)
    (debian_dir / "control").write_text(
        "Source: oxideterm\nPackage: oxideterm\nArchitecture: any\nDescription: OxideTerm\n",
        encoding="utf-8",
    )
    output = subprocess.check_output(
        [dpkg_shlibdeps, "-O", "-e", str(binary)],
        cwd=scratch_dir,
        text=True,
    )
    return parse_dpkg_shlibdeps_output(output)


def linux_deb_version(version: str) -> str:
    # Debian treats '-' as the Debian revision separator. Preview semver tags
    # use '-' inside the upstream version, so store them as '~' for valid
    # package metadata while keeping release asset filenames unchanged.
    return version.replace("-", "~")


def windows_numeric_version(version: str) -> str:
    """Convert SemVer to the four numeric fields required by NSIS version info."""
    core = version.split("-", 1)[0]
    components = core.split(".")
    if len(components) != 3 or any(not part.isdigit() for part in components):
        raise RuntimeError(f"Windows package version is not SemVer: {version}")
    numeric = [int(part) for part in components] + [0]
    if any(part > 65535 for part in numeric):
        raise RuntimeError(f"Windows package version component exceeds 65535: {version}")
    return ".".join(str(part) for part in numeric)


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
    document_root = appdir / "usr" / "share" / "doc" / identity.linux_package_name
    copy_release_documents(document_root)
    write_package_version(document_root, version)

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
    copy_release_documents(app_root)
    write_package_version(app_root, version)
    (app_root / LINUX_PACKAGE_KIND_FILENAME).write_text("deb\n", encoding="utf-8")
    document_root = deb_root / "usr" / "share" / "doc" / identity.linux_package_name
    copy_release_documents(document_root)
    write_package_version(document_root, version)

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
    shlibdeps_dir = deb_root / "shlibdeps"
    dependencies = linux_deb_dependencies(app_binary, shlibdeps_dir)
    shutil.rmtree(shlibdeps_dir)
    control = f"""Package: {identity.linux_package_name}
Version: {linux_deb_version(version)}
Section: utils
Priority: optional
Architecture: {linux_deb_arch(target)}
Maintainer: AnalyseDeCircuit <noreply@oxideterm.app>
Depends: {dependencies}
Description: OxideTerm native SSH workspace
 Local-first SSH workspace with terminal, SFTP, port forwarding, and AI context.
"""
    (control_dir / "control").write_text(control, encoding="utf-8")

    output = DIST_DIR / f"OxideTerm_{version}_{label}.deb"
    run(["dpkg-deb", "--build", "--root-owner-group", str(deb_root), str(output)])
    shutil.rmtree(deb_root)


def create_linux_rpm(
    binary: Path, target: str, version: str, label: str, identity: ReleaseIdentity
) -> None:
    """Build an installable RPM with the same /opt layout as the Debian package."""
    rpmbuild = require_tool("rpmbuild")
    rpm_root = DIST_DIR / f"rpm-{label}"
    top_dir = rpm_root / "rpmbuild"
    payload_root = rpm_root / "payload"
    for directory in ("BUILD", "BUILDROOT", "RPMS", "SOURCES", "SPECS", "SRPMS"):
        (top_dir / directory).mkdir(parents=True, exist_ok=True)

    app_root = payload_root / "opt" / identity.linux_install_dir
    app_root.mkdir(parents=True)
    app_binary = app_root / APP_BIN
    shutil.copy2(binary, app_binary)
    make_executable(app_binary)
    # Encode the two remote-agent ELFs so RPM dependency discovery only scans
    # executables built for the package's own architecture.
    copy_runtime_resources(app_root / "resources", target, encode_agent_binaries=True)
    copy_release_documents(app_root)
    write_package_version(app_root, version)
    (app_root / LINUX_PACKAGE_KIND_FILENAME).write_text("rpm\n", encoding="utf-8")

    document_root = payload_root / "usr" / "share" / "doc" / identity.linux_package_name
    copy_release_documents(document_root)
    write_package_version(document_root, version)
    applications_dir = payload_root / "usr" / "share" / "applications"
    applications_dir.mkdir(parents=True)
    write_linux_desktop_file(
        applications_dir / f"{identity.linux_desktop_id}.desktop",
        identity,
        f"/opt/{identity.linux_install_dir}/{APP_BIN}",
    )
    copy_linux_icons(payload_root, identity)

    rpm_version, rpm_release = linux_rpm_version_release(version)
    payload_path = str(payload_root.resolve()).replace("%", "%%")
    spec = f"""Name: {identity.linux_package_name}
Version: {rpm_version}
Release: {rpm_release}
Summary: OxideTerm SSH workspace
License: GPL-3.0-only
URL: https://oxideterm.app
BuildArch: {linux_rpm_arch(target)}

%description
Local-first SSH workspace with terminal, SFTP, port forwarding, and AI context.

%install
rm -rf %{{buildroot}}
mkdir -p %{{buildroot}}
cp -a \"{payload_path}/.\" %{{buildroot}}/

%files
/opt/{identity.linux_install_dir}
/usr/share/applications/{identity.linux_desktop_id}.desktop
/usr/share/icons/hicolor/*/apps/{identity.linux_icon_name}.png
/usr/share/doc/{identity.linux_package_name}
"""
    spec_path = top_dir / "SPECS" / f"{identity.linux_package_name}.spec"
    spec_path.write_text(spec, encoding="utf-8")
    run(
        [
            rpmbuild,
            "--define",
            f"_topdir {top_dir.resolve()}",
            "--define",
            "_build_id_links none",
            "--define",
            "debug_package %{nil}",
            "-bb",
            str(spec_path),
        ]
    )

    built = list((top_dir / "RPMS" / linux_rpm_arch(target)).glob("*.rpm"))
    if len(built) != 1:
        raise RuntimeError(f"expected one RPM artifact, found {len(built)}")
    shutil.copy2(built[0], DIST_DIR / f"OxideTerm_{version}_{label}.rpm")
    shutil.rmtree(rpm_root)


def main() -> None:
    target_was_explicit = len(sys.argv) > 1 and sys.argv[1] != ""
    target = sys.argv[1] if target_was_explicit else host_triple()
    raw_version = raw_release_version()
    version = normalized_version(raw_version)
    validate_release_version(raw_version, version)
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
        sign_windows_file(app_binary)
        create_windows_installer(app_binary, target, target_was_explicit, version, label, identity)
    if "apple-darwin" in target:
        sign_macos_path(app_binary)
    # Every target should publish a self-contained portable artifact; Windows
    # additionally ships an NSIS installer for users who prefer installation.
    create_portable_package(app_binary, target, version, label)
    if "apple-darwin" in target:
        create_macos_app(app_binary, target, version, label, identity)
    if "linux" in target:
        create_linux_appimage(app_binary, target, version, label, identity)
        create_linux_deb(app_binary, target, version, label, identity)
        create_linux_rpm(app_binary, target, version, label, identity)

    print("==> Package artifacts", flush=True)
    for path in sorted(DIST_DIR.iterdir()):
        if path.is_file():
            print(path)


if __name__ == "__main__":
    main()
