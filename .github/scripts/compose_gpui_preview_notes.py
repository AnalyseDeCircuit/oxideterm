#!/usr/bin/env python3
"""Compose GPUI preview release notes from evergreen text and one changelog entry."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


CHANGELOG_MARKER = "<!-- GPUI_PREVIEW_CHANGELOG -->"


def extract_version_section(changelog: str, version: str) -> str:
    """Return the changelog section whose heading exactly matches the release version."""
    pattern = re.compile(
        rf"^## {re.escape(version)}\n(?P<section>.*?)(?=^## |\Z)",
        re.MULTILINE | re.DOTALL,
    )
    match = pattern.search(changelog)
    if match is None:
        raise ValueError(f"no changelog section found for {version}")
    return f"## {version}\n{match.group('section').rstrip()}"


def compose_notes(version: str, base_path: Path, changelog_path: Path) -> str:
    base = base_path.read_text(encoding="utf-8")
    if CHANGELOG_MARKER not in base:
        raise ValueError(f"{base_path} is missing {CHANGELOG_MARKER}")

    changelog = changelog_path.read_text(encoding="utf-8")
    section = extract_version_section(changelog, version)
    return base.replace(CHANGELOG_MARKER, section).rstrip() + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--base", required=True, type=Path)
    parser.add_argument("--changelog", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    try:
        notes = compose_notes(args.version, args.base, args.changelog)
    except Exception as error:
        print(f"failed to compose GPUI preview release notes: {error}", file=sys.stderr)
        return 1

    args.output.write_text(notes, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
