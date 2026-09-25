#!/usr/bin/env python3
"""Compose release notes from channel-specific evergreen text and one changelog entry."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from urllib.parse import quote


CHANGELOG_MARKER = "<!-- RELEASE_CHANGELOG -->"
DOWNLOADS_MARKER = "<!-- RELEASE_DOWNLOADS -->"
REPOSITORY_RELEASE_URL = "https://github.com/AnalyseDeCircuit/oxideterm/releases/download"


def normalize_leading_summary(section: str) -> str:
    """Join soft-wrapped lines in the opening prose paragraph."""
    lines = section.splitlines()
    if not lines:
        return section

    paragraph_end = next(
        (index for index, line in enumerate(lines) if not line.strip()),
        len(lines),
    )
    paragraph = lines[:paragraph_end]
    if not paragraph:
        return section

    # Preserve Markdown blocks when a channel intentionally starts with one.
    if any(
        re.match(r"^(?:#{1,6}\s|[-*+]\s|>\s?|```|~~~|\d+[.)]\s|\||<)", line.lstrip())
        for line in paragraph
    ):
        return section

    summary = " ".join(line.strip() for line in paragraph)
    remainder = lines[paragraph_end:]
    return "\n".join([summary, *remainder])


def release_asset_url(tag: str, filename: str) -> str:
    """Build a URL for an asset attached to the release being composed."""
    return f"{REPOSITORY_RELEASE_URL}/{quote(tag, safe='')}/{quote(filename, safe='')}"


def stable_download_table(version: str, tag: str) -> str:
    """Render the recommended installer matrix without listing portable variants."""
    filenames = {
        "windows_x64": f"OxideTerm_{version}_windows_x64-setup.exe",
        "windows_arm64": f"OxideTerm_{version}_windows_arm64-setup.exe",
        "macos_x64": f"OxideTerm_{version}_macos_x64.dmg",
        "macos_arm64": f"OxideTerm_{version}_macos_arm64.dmg",
        "linux_x64_appimage": f"OxideTerm_{version}_linux_x64.AppImage",
        "linux_x64_deb": f"OxideTerm_{version}_linux_x64.deb",
        "linux_x64_rpm": f"OxideTerm_{version}_linux_x64.rpm",
        "linux_arm64_appimage": f"OxideTerm_{version}_linux_arm64.AppImage",
        "linux_arm64_deb": f"OxideTerm_{version}_linux_arm64.deb",
        "linux_arm64_rpm": f"OxideTerm_{version}_linux_arm64.rpm",
    }

    def link(label: str, key: str) -> str:
        return f"[{label}]({release_asset_url(tag, filenames[key])})"

    return "\n".join(
        [
            "## 📥 Download for your system",
            "",
            "| Operating system | x64 | ARM64 |",
            "|---|---|---|",
            f"| **Windows** | {link('Setup (.exe)', 'windows_x64')} | {link('Setup (.exe)', 'windows_arm64')} |",
            f"| **macOS** | {link('DMG (Intel)', 'macos_x64')} | {link('DMG (Apple Silicon)', 'macos_arm64')} |",
            f"| **Linux** | {link('AppImage', 'linux_x64_appimage')} · {link('DEB', 'linux_x64_deb')} · {link('RPM', 'linux_x64_rpm')} | {link('AppImage', 'linux_arm64_appimage')} · {link('DEB', 'linux_arm64_deb')} · {link('RPM', 'linux_arm64_rpm')} |",
            "",
            "Portable archives, signatures, and `sha256sums.txt` remain available in the release assets below.",
        ]
    )


def extract_version_section(
    changelog: str, version: str, *, include_heading: bool = True
) -> str:
    """Return one version entry, optionally retaining its extraction heading."""
    pattern = re.compile(
        rf"^## {re.escape(version)}\n(?P<section>.*?)(?=^## |\Z)",
        re.MULTILINE | re.DOTALL,
    )
    match = pattern.search(changelog)
    if match is None:
        raise ValueError(f"no changelog section found for {version}")
    section = normalize_leading_summary(match.group("section").strip())
    if include_heading:
        return f"## {version}\n\n{section}"
    return section


def compose_notes(version: str, tag: str, base_path: Path, changelog_path: Path) -> str:
    base = base_path.read_text(encoding="utf-8")
    if CHANGELOG_MARKER not in base:
        raise ValueError(f"{base_path} is missing {CHANGELOG_MARKER}")

    changelog = changelog_path.read_text(encoding="utf-8")
    # Stable releases already show the version in GitHub's release title, so their
    # body omits the version heading while other channels keep it.
    is_stable_release = DOWNLOADS_MARKER in base
    section = extract_version_section(
        changelog, version, include_heading=not is_stable_release
    )
    language_blocks = re.split(r"^### (中文|English)\n", section, flags=re.MULTILINE)
    bilingual = len(language_blocks) == 5 and set(language_blocks[1::2]) == {
        "中文", "English"
    }
    if bilingual:
        blocks = dict(zip(language_blocks[1::2], language_blocks[2::2]))
        section = language_blocks[0] + "\n\n".join(
            f"### {language}\n\n{blocks[language].strip()}"
            for language in ("中文", "English")
        )
    notes = base.replace(CHANGELOG_MARKER, section)
    if DOWNLOADS_MARKER in notes:
        notes = notes.replace(DOWNLOADS_MARKER, stable_download_table(version, tag))
    if bilingual:
        notes = "[中文](#中文) | [English](#english)\n\n" + notes
    return notes.rstrip() + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--base", required=True, type=Path)
    parser.add_argument("--changelog", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    try:
        notes = compose_notes(args.version, args.tag, args.base, args.changelog)
    except Exception as error:
        print(f"failed to compose release notes: {error}", file=sys.stderr)
        return 1

    args.output.write_text(notes, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
