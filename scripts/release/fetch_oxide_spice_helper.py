#!/usr/bin/env python3
"""Fetch and verify one pinned full-capability OxideSpice helper artifact."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import stat
import subprocess
import tarfile
import tempfile
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath


ROOT_DIR = Path(__file__).resolve().parents[2]
DEFAULT_MANIFEST = Path(__file__).with_name("oxide_spice_helper_release.json")
DEFAULT_OUTPUT = (
    ROOT_DIR / "crates" / "oxideterm-gpui-app" / "resources" / "helpers"
)
DOWNLOAD_CHUNK_BYTES = 1024 * 1024
DOWNLOAD_TIMEOUT_SECONDS = 120


class HelperArtifactError(RuntimeError):
    """Reject an artifact before it can enter an application package."""


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(DOWNLOAD_CHUNK_BYTES):
            digest.update(chunk)
    return digest.hexdigest()


def parse_checksum(contents: str) -> tuple[str, str]:
    # split() deliberately accepts CRLF checksum files produced by Windows runners.
    fields = contents.split()
    if len(fields) != 2 or len(fields[0]) != 64:
        raise HelperArtifactError("OxideSpice checksum file has an invalid format")
    try:
        int(fields[0], 16)
    except ValueError as error:
        raise HelperArtifactError("OxideSpice checksum is not hexadecimal") from error
    return fields[0].lower(), fields[1].lstrip("*")


def download(url: str, destination: Path) -> None:
    request = urllib.request.Request(
        url,
        headers={"User-Agent": "OxideTerm native package builder"},
    )
    partial = destination.with_suffix(destination.suffix + ".partial")
    partial.unlink(missing_ok=True)
    try:
        with urllib.request.urlopen(
            request, timeout=DOWNLOAD_TIMEOUT_SECONDS
        ) as response, partial.open("wb") as output:
            shutil.copyfileobj(response, output, DOWNLOAD_CHUNK_BYTES)
        partial.replace(destination)
    finally:
        partial.unlink(missing_ok=True)


def verify_minisign(checksum: Path, signature: Path, public_key: str) -> None:
    minisign = shutil.which("minisign")
    if minisign is None:
        raise HelperArtifactError(
            "minisign is required to verify the OxideSpice helper release"
        )
    subprocess.run(
        [
            minisign,
            "-Vm",
            str(checksum),
            "-x",
            str(signature),
            "-P",
            public_key,
        ],
        check=True,
    )


def safe_archive_path(name: str) -> PurePosixPath:
    if "\\" in name:
        raise HelperArtifactError(f"archive member uses a backslash path: {name}")
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts:
        raise HelperArtifactError(f"archive member escapes its destination: {name}")
    return path


def validate_tar_members(members: list[tarfile.TarInfo]) -> None:
    for member in members:
        member_path = safe_archive_path(member.name)
        if member.isdev() or member.isfifo():
            raise HelperArtifactError(
                f"archive contains an unsupported special file: {member.name}"
            )
        if member.issym():
            link_path = safe_archive_path(member.linkname)
            resolved = member_path.parent.joinpath(link_path)
            if ".." in resolved.parts:
                raise HelperArtifactError(
                    f"archive symbolic link escapes its destination: {member.name}"
                )
        if member.islnk():
            safe_archive_path(member.linkname)


def validate_zip_members(entries: list[zipfile.ZipInfo]) -> None:
    for entry in entries:
        safe_archive_path(entry.filename)
        unix_mode = entry.external_attr >> 16
        if stat.S_ISLNK(unix_mode):
            raise HelperArtifactError(
                f"zip archive contains an unsupported symbolic link: {entry.filename}"
            )


def extract_archive(archive_path: Path, destination: Path) -> Path:
    if archive_path.name.endswith(".tar.gz"):
        with tarfile.open(archive_path, "r:gz") as archive:
            members = archive.getmembers()
            validate_tar_members(members)
            archive.extractall(destination)
    elif archive_path.suffix == ".zip":
        with zipfile.ZipFile(archive_path) as archive:
            entries = archive.infolist()
            validate_zip_members(entries)
            archive.extractall(destination)
    else:
        raise HelperArtifactError(
            f"unsupported OxideSpice helper archive: {archive_path.name}"
        )

    roots = [entry for entry in destination.iterdir() if entry.is_dir()]
    if len(roots) != 1:
        raise HelperArtifactError("OxideSpice helper archive must contain one root directory")
    return roots[0]


def validate_artifact(
    artifact: Path,
    *,
    target: str,
    version: str,
    ipc_protocol_version: int,
    required_capabilities: list[str],
) -> None:
    metadata_path = artifact / "helper-metadata.json"
    if not metadata_path.is_file():
        raise HelperArtifactError("OxideSpice helper metadata is missing")
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    if metadata.get("helperVersion") != version:
        raise HelperArtifactError("OxideSpice helper version does not match its pin")
    if metadata.get("ipcProtocolVersion") != ipc_protocol_version:
        raise HelperArtifactError("OxideSpice helper IPC version does not match its pin")
    if metadata.get("target") != target:
        raise HelperArtifactError("OxideSpice helper target does not match its pin")
    if sorted(metadata.get("capabilities", [])) != sorted(required_capabilities):
        raise HelperArtifactError("OxideSpice helper lacks the pinned capability contract")

    executable_name = "oxide-spice-helper.exe" if "windows" in target else "oxide-spice-helper"
    required_paths = (
        artifact / "bin" / executable_name,
        artifact / "LICENSE",
        artifact / "THIRD-PARTY-NOTICES.md",
        artifact / "oxide-spice-helper.cdx.json",
    )
    missing = [path.name for path in required_paths if not path.is_file()]
    if missing:
        raise HelperArtifactError(
            f"OxideSpice helper artifact is incomplete: {', '.join(missing)}"
        )
    if not any((artifact / "licenses").rglob("*")):
        raise HelperArtifactError("OxideSpice helper artifact has no license inventory")


def release_url(repository: str, tag: str, asset: str) -> str:
    return f"https://github.com/{repository}/releases/download/{tag}/{asset}"


def fetch_target(manifest: dict, target: str, output: Path) -> Path:
    try:
        target_pin = manifest["targets"][target]
    except KeyError as error:
        raise HelperArtifactError(f"unsupported OxideSpice helper target: {target}") from error

    archive_name = target_pin["archive"]
    checksum_name = f"{archive_name}.sha256"
    signature_name = f"{checksum_name}.minisig"
    with tempfile.TemporaryDirectory(prefix="oxideterm-spice-helper-") as temporary:
        temporary_dir = Path(temporary)
        archive_path = temporary_dir / archive_name
        checksum_path = temporary_dir / checksum_name
        signature_path = temporary_dir / signature_name
        for name, destination in (
            (archive_name, archive_path),
            (checksum_name, checksum_path),
            (signature_name, signature_path),
        ):
            download(
                release_url(manifest["repository"], manifest["tag"], name),
                destination,
            )

        verify_minisign(
            checksum_path,
            signature_path,
            manifest["minisignPublicKey"],
        )
        signed_sha256, signed_archive_name = parse_checksum(
            checksum_path.read_text(encoding="utf-8")
        )
        if signed_archive_name != archive_name:
            raise HelperArtifactError("signed checksum names a different helper archive")
        if signed_sha256 != target_pin["sha256"]:
            raise HelperArtifactError("signed checksum differs from the OxideTerm release pin")
        if file_sha256(archive_path) != signed_sha256:
            raise HelperArtifactError("OxideSpice helper archive checksum mismatch")

        extraction = temporary_dir / "extracted"
        extraction.mkdir()
        artifact = extract_archive(archive_path, extraction)
        if artifact.name != f"oxide-spice-helper-{target}":
            raise HelperArtifactError("OxideSpice helper archive root has an invalid name")
        validate_artifact(
            artifact,
            target=target,
            version=manifest["version"],
            ipc_protocol_version=manifest["ipcProtocolVersion"],
            required_capabilities=manifest["requiredCapabilities"],
        )

        destination = output / target / "oxide-spice-helper"
        destination.parent.mkdir(parents=True, exist_ok=True)
        if destination.exists():
            shutil.rmtree(destination)
        shutil.copytree(artifact, destination, symlinks=True)
        return destination


def validate_staged_target(manifest: dict, target: str, output: Path) -> Path:
    if target not in manifest["targets"]:
        raise HelperArtifactError(f"unsupported OxideSpice helper target: {target}")
    artifact = output / target / "oxide-spice-helper"
    if not artifact.is_dir():
        raise HelperArtifactError(f"staged OxideSpice helper is missing for {target}")
    validate_artifact(
        artifact,
        target=target,
        version=manifest["version"],
        ipc_protocol_version=manifest["ipcProtocolVersion"],
        required_capabilities=manifest["requiredCapabilities"],
    )
    return artifact


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--verify-staged", action="store_true")
    selection = parser.add_mutually_exclusive_group(required=True)
    selection.add_argument("--target")
    selection.add_argument("--all", action="store_true")
    arguments = parser.parse_args()

    manifest = json.loads(arguments.manifest.read_text(encoding="utf-8"))
    targets = sorted(manifest["targets"]) if arguments.all else [arguments.target]
    for target in targets:
        if arguments.verify_staged:
            destination = validate_staged_target(manifest, target, arguments.output)
        else:
            destination = fetch_target(manifest, target, arguments.output)
        print(destination)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
