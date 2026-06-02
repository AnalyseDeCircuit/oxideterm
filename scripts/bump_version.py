#!/usr/bin/env python3
"""Bump the native OxideTerm workspace version and refresh Cargo.lock."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path


ROOT_DIR = Path(__file__).resolve().parent.parent
WORKSPACE_MANIFEST = ROOT_DIR / "Cargo.toml"
README_BADGE_RE = re.compile(r"https://img\.shields\.io/badge/version-[^\"]+-blue")
SEMVER_RE = re.compile(
    r"^(0|[1-9]\d*)\."
    r"(0|[1-9]\d*)\."
    r"(0|[1-9]\d*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Update the native workspace package version and keep Cargo.lock in sync.",
    )
    parser.add_argument("version", help="New SemVer version, for example 2.0.0-gpui-preview.1.")
    parser.add_argument(
        "--no-lock",
        action="store_true",
        help="Edit Cargo.toml and README badges, but do not refresh Cargo.lock.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the planned change without writing files or running cargo.",
    )
    return parser.parse_args()


def validate_version(version: str) -> None:
    if SEMVER_RE.fullmatch(version):
        return
    raise ValueError(
        f"invalid version {version!r}; use SemVer like 2.0.0-gpui-preview.1, not 2.0.0preview1"
    )


def workspace_version(manifest_text: str) -> str:
    in_workspace_package = False
    for line in manifest_text.splitlines():
        stripped = line.strip()
        if stripped == "[workspace.package]":
            in_workspace_package = True
            continue
        if in_workspace_package and stripped.startswith("["):
            break
        if in_workspace_package and stripped.startswith("version"):
            return stripped.split('"', 2)[1]
    raise RuntimeError("[workspace.package] version not found in Cargo.toml")


def updated_manifest(manifest_text: str, new_version: str) -> str:
    lines = manifest_text.splitlines(keepends=True)
    in_workspace_package = False
    for index, line in enumerate(lines):
        stripped = line.strip()
        if stripped == "[workspace.package]":
            in_workspace_package = True
            continue
        if in_workspace_package and stripped.startswith("["):
            break
        if in_workspace_package and stripped.startswith("version"):
            # Preserve the manifest's surrounding layout and only replace the value.
            lines[index] = re.sub(
                r'version\s*=\s*"[^"]+"',
                f'version = "{new_version}"',
                line,
                count=1,
            )
            return "".join(lines)
    raise RuntimeError("[workspace.package] version not found in Cargo.toml")


def readme_paths() -> list[Path]:
    paths = [ROOT_DIR / "README.md"]
    paths.extend(sorted((ROOT_DIR / "docs" / "readme").glob("README*.md")))
    return paths


def shields_badge_version(version: str) -> str:
    return version.replace("-", "--")


def version_badge_url(version: str) -> str:
    return f"https://img.shields.io/badge/version-{shields_badge_version(version)}-blue"


def updated_readme_badge(readme_text: str, new_version: str, path: Path) -> str:
    updated_text, count = README_BADGE_RE.subn(version_badge_url(new_version), readme_text)
    if count == 0:
        raise RuntimeError(f"version badge not found in {path.relative_to(ROOT_DIR)}")
    return updated_text


def update_readme_badges(new_version: str) -> None:
    for path in readme_paths():
        readme_text = path.read_text(encoding="utf-8")
        path.write_text(updated_readme_badge(readme_text, new_version, path), encoding="utf-8")


def run(command: list[str]) -> None:
    print("+", " ".join(command), flush=True)
    subprocess.run(command, cwd=ROOT_DIR, check=True)


def main() -> int:
    args = parse_args()
    try:
        validate_version(args.version)
        manifest_text = WORKSPACE_MANIFEST.read_text(encoding="utf-8")
        old_version = workspace_version(manifest_text)
        new_manifest_text = updated_manifest(manifest_text, args.version)
        for path in readme_paths():
            updated_readme_badge(path.read_text(encoding="utf-8"), args.version, path)
    except Exception as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    print(f"native workspace version: {old_version} -> {args.version}")
    if args.dry_run:
        print("dry run: Cargo.toml, README badges, and Cargo.lock were not changed")
        return 0

    WORKSPACE_MANIFEST.write_text(new_manifest_text, encoding="utf-8")
    update_readme_badges(args.version)

    if not args.no_lock:
        # Cargo owns lockfile package metadata, so use it instead of rewriting Cargo.lock by hand.
        run(["cargo", "update", "--workspace", "--offline"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
