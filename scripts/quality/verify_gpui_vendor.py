#!/usr/bin/env python3
"""Verify OxideTerm's six local gpui-pre patches and their source licenses."""

from __future__ import annotations

import argparse
import hashlib
import subprocess
import tarfile
import tomllib
from pathlib import Path


ROOT_DIR = Path(__file__).resolve().parents[2]
GPUI_VENDOR_DIR = ROOT_DIR / "crates" / "gpui-ce"
BASELINE_PATH = GPUI_VENDOR_DIR / "gpui" / "UPSTREAM_BASELINE.toml"
CANONICAL_LICENSE = ROOT_DIR / "licenses/third-party/GPUI-CE-LICENSE-APACHE"
ROOT_UPSTREAM_LICENSE = GPUI_VENDOR_DIR / "gpui/GPUI_CE_ROOT_LICENSE.md"
MICROSOFT_TERMINAL_LICENSE = (
    ROOT_DIR / "licenses/third-party/MICROSOFT-TERMINAL-LICENSE-MIT"
)
MICROSOFT_TERMINAL_LICENSE_SHA256 = (
    "3c181bf8ce0bab0c2e5be1b10132d2fa9450a99d3280ffc7136bd4e27a696e98"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--registry-archives",
        type=Path,
        help="Optional directory containing the six pristine <package-name>.crate archives.",
    )
    parser.add_argument(
        "--gpui-ce-checkout",
        type=Path,
        help="Optional GPUI-CE Git checkout for the retained historical license.",
    )
    return parser.parse_args()


def load_toml(path: Path) -> dict:
    with path.open("rb") as source:
        return tomllib.load(source)


def verify_package(manifest: dict, entry: dict, baseline: dict) -> None:
    package = manifest["package"]
    if (
        package["name"] != entry["name"]
        or package["version"] != baseline["gpui_pre_version"]
    ):
        raise RuntimeError(f"package name/version differs from baseline: {entry['name']}")
    if package.get("license") != "Apache-2.0":
        raise RuntimeError(f"unexpected GPUI package license: {entry['name']}")
    metadata = package.get("metadata", {}).get("gpui-pre", {})
    expected = {
        "zed-crate": entry["zed_crate"],
        "zed-version": entry["zed_version"],
        "zed-rev": baseline["zed_commit"],
    }
    if metadata != expected:
        raise RuntimeError(f"Zed source metadata differs from baseline: {entry['name']}")


def upstream_file_bytes(checkout: Path, revision: str, source_path: str) -> bytes:
    return subprocess.check_output(
        ["git", "show", f"{revision}:{source_path}"], cwd=checkout
    )


def normalized_markdown_lines(content: bytes) -> list[bytes]:
    return [line.rstrip(b" \t") for line in content.splitlines()]


def main() -> None:
    args = parse_args()
    baseline = load_toml(BASELINE_PATH)
    crates = baseline["crates"]
    names = {entry["name"] for entry in crates}
    paths = {entry["local_path"] for entry in crates}
    if len(crates) != 6 or len(names) != 6 or len(paths) != 6:
        raise RuntimeError("baseline must describe six distinct local GPUI patch packages")
    workspace = load_toml(ROOT_DIR / "Cargo.toml")
    missing = paths - set(workspace["workspace"]["members"])
    if missing:
        raise RuntimeError(f"GPUI patch crates missing from workspace: {sorted(missing)}")
    patches = {
        name: value
        for name, value in workspace["patch"]["crates-io"].items()
        if name.startswith("gpui-pre")
    }
    expected_patches = {entry["name"]: {"path": entry["local_path"]} for entry in crates}
    if patches != expected_patches:
        raise RuntimeError("root crates.io patches differ from the six recorded GPUI paths")

    canonical_license = CANONICAL_LICENSE.read_bytes()
    ROOT_UPSTREAM_LICENSE.read_bytes()
    microsoft_license_digest = hashlib.sha256(
        MICROSOFT_TERMINAL_LICENSE.read_bytes()
    ).hexdigest()
    if microsoft_license_digest != MICROSOFT_TERMINAL_LICENSE_SHA256:
        raise RuntimeError("Microsoft Terminal license differs from recorded source text")
    locked = load_toml(ROOT_DIR / "Cargo.lock")["package"]
    for entry in crates:
        directory = ROOT_DIR / entry["local_path"]
        verify_package(load_toml(directory / "Cargo.toml"), entry, baseline)
        if (directory / "LICENSE-APACHE").read_bytes() != canonical_license:
            raise RuntimeError(f"GPUI patch license differs from canonical copy: {directory}")
        packages = [package for package in locked if package["name"] == entry["name"]]
        if (
            len(packages) != 1
            or packages[0]["version"] != baseline["gpui_pre_version"]
            or "source" in packages[0]
        ):
            raise RuntimeError(f"Cargo.lock does not resolve the recorded local patch: {entry['name']}")
        if args.registry_archives is not None:
            archive_path = args.registry_archives / f"{entry['name']}.crate"
            archive_digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
            if archive_digest != entry["registry_checksum"]:
                raise RuntimeError(f"registry archive checksum differs from baseline: {archive_path}")
            prefix = f"{entry['name']}-{baseline['gpui_pre_version']}"
            with tarfile.open(archive_path, "r:gz") as archive:
                # Read members directly; verification never extracts untrusted archive paths.
                manifest_file = archive.extractfile(f"{prefix}/Cargo.toml")
                license_file = archive.extractfile(f"{prefix}/LICENSE-APACHE")
                if manifest_file is None or license_file is None:
                    raise RuntimeError(f"missing registry manifest/license: {archive_path}")
                verify_package(
                    tomllib.loads(manifest_file.read().decode("utf-8")), entry, baseline
                )
                if license_file.read() != canonical_license:
                    raise RuntimeError(f"registry license differs from canonical copy: {archive_path}")

    if args.gpui_ce_checkout is not None:
        revision = baseline["gpui_ce_commit"]
        if canonical_license != upstream_file_bytes(
            args.gpui_ce_checkout, revision, "crates/gpui/LICENSE-APACHE"
        ):
            raise RuntimeError("canonical GPUI license differs from historical GPUI-CE")
        if normalized_markdown_lines(
            ROOT_UPSTREAM_LICENSE.read_bytes()
        ) != normalized_markdown_lines(
            upstream_file_bytes(args.gpui_ce_checkout, revision, "LICENSE.md")
        ):
            raise RuntimeError("retained GPUI-CE root license differs from historical source")
    print(
        f"verified six gpui-pre {baseline['gpui_pre_version']} local patches "
        f"with Zed source metadata {baseline['zed_commit']}"
    )


if __name__ == "__main__":
    main()
