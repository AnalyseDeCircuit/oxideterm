#!/usr/bin/env python3
"""Estimate repository code, comment and blank lines without external packages."""

from __future__ import annotations

import argparse
from collections import defaultdict
from dataclasses import dataclass
import os
from pathlib import Path
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
VENDOR_ROOTS = (
    Path("crates/gpui-ce"),
    Path("crates/alacritty-terminal"),
    Path("crates/russh-sftp"),
    Path("crates/vte"),
)
GENERATED_FILES = {
    Path("crates/oxideterm-theme/src/generated.rs"),
    Path("crates/oxideterm-theme/src/generated_ui.rs"),
    Path("crates/oxideterm-gpui-ui/src/file_icons/generated.rs"),
}
EXCLUDED_DIRS = {"target", "node_modules", "dist", "__pycache__", "resources", "assets"}
LANGUAGES = {
    ".rs": "Rust", ".py": "Python", ".sh": "Shell", ".bash": "Shell",
    ".c": "C", ".h": "C", ".cc": "C++", ".cpp": "C++", ".hpp": "C++",
    ".m": "Objective-C", ".mm": "Objective-C++",
    ".js": "JavaScript", ".jsx": "JavaScript", ".mjs": "JavaScript", ".cjs": "JavaScript",
    ".ts": "TypeScript", ".tsx": "TypeScript", ".css": "CSS", ".scss": "CSS",
    ".metal": "Metal", ".wgsl": "WGSL",
    ".md": "Markdown", ".toml": "TOML", ".json": "JSON",
    ".yml": "YAML", ".yaml": "YAML", ".html": "HTML", ".htm": "HTML",
}
DOCUMENT_LANGUAGES = {"Markdown", "TOML", "JSON", "YAML", "HTML"}
HASH_COMMENTS = {"Python", "Shell", "TOML", "YAML", "Just"}
BLOCK_COMMENTS = {
    "Rust", "C", "C++", "Objective-C", "Objective-C++",
    "JavaScript", "TypeScript", "CSS", "Metal", "WGSL",
}


@dataclass
class Counts:
    files: int = 0
    blank: int = 0
    comment: int = 0
    code: int = 0

    def add(self, other: Counts) -> None:
        self.files += other.files
        self.blank += other.blank
        self.comment += other.comment
        self.code += other.code


def language_for(path: Path) -> str | None:
    if path.name == "Justfile":
        return "Just"
    return LANGUAGES.get(path.suffix.lower())


def count_lines(text: str, language: str) -> Counts:
    # This retains the original line-based estimate, not a language parser.
    # Inline comments count as code; multiline strings can resemble comments.
    counts = Counts(files=1)
    depth = 0
    for raw in text.splitlines():
        line = raw.strip()
        if not line:
            counts.blank += 1
            continue
        while line and (depth or (language in BLOCK_COMMENTS and line.startswith("/*"))):
            if not depth:
                depth = 1
                line = line[2:]
            end = line.find("*/")
            nested = line.find("/*") if language == "Rust" else -1
            if nested >= 0 and (end < 0 or nested < end):
                depth += 1
                line = line[nested + 2:]
            elif end >= 0:
                depth -= 1
                line = line[end + 2:].lstrip()
            else:
                line = ""
        if not line or (language in HASH_COMMENTS and line.startswith("#")) or (
            language in BLOCK_COMMENTS - {"CSS"} and line.startswith("//")
        ):
            counts.comment += 1
        else:
            counts.code += 1
    return counts


def collect_files(root: Path, include_vendor: bool = False) -> list[Path]:
    output = subprocess.check_output(
        ["git", "-C", str(root), "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
    )
    files = []
    for name in sorted(set(output.split(b"\0")) - {b""}):
        relative = Path(os.fsdecode(name))
        path = root / relative
        if not language_for(relative) or relative in GENERATED_FILES:
            continue
        if EXCLUDED_DIRS.intersection(relative.parts[:-1]):
            continue
        if not include_vendor and is_vendor(relative):
            continue
        # Deleted tracked files and links outside the repository are not source inputs.
        if path.is_symlink() or not path.is_file():
            continue
        files.append(relative)
    return files


def is_vendor(relative: Path) -> bool:
    return any(relative.is_relative_to(directory) for directory in VENDOR_ROOTS)


def crate_name(root: Path, relative: Path, cache: dict[Path, str]) -> str:
    directory = relative.parent
    if directory in cache:
        return cache[directory]
    manifest = root / directory / "Cargo.toml"
    if manifest.is_file():
        package = tomllib.loads(manifest.read_text(encoding="utf-8")).get("package", {})
        if "name" in package:
            cache[directory] = package["name"]
            return cache[directory]
    result = "(repository)" if directory == Path(".") else crate_name(root, directory, cache)
    cache[directory] = result
    return result


def print_table(title: str, rows: dict[str, Counts]) -> None:
    print(f"\n{title}")
    width = max(24, *(len(name) for name in rows)) if rows else 24
    print(f"{'Name':<{width}} {'Files':>7} {'Blank':>10} {'Comment':>10} {'Code':>12}")
    print("-" * (width + 43))
    total = Counts()
    for name, counts in sorted(rows.items(), key=lambda item: (-item[1].code, item[0])):
        print(f"{name:<{width}} {counts.files:>7,} {counts.blank:>10,} {counts.comment:>10,} {counts.code:>12,}")
        total.add(counts)
    print("-" * (width + 43))
    print(f"{'TOTAL':<{width}} {total.files:>7,} {total.blank:>10,} {total.comment:>10,} {total.code:>12,}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", nargs="?", type=Path, default=ROOT, help="Git repository root")
    parser.add_argument("--by-crate", action="store_true", help="Group source code by Cargo package")
    parser.add_argument("--by-dir", action="store_true", help="Group source code by top-level directory")
    parser.add_argument("--include-vendor", action="store_true", help="Also report bundled third-party code separately")
    args = parser.parse_args()
    root = args.root.resolve()
    try:
        files = collect_files(root, args.include_vendor)
        languages: dict[str, Counts] = defaultdict(Counts)
        documents: dict[str, Counts] = defaultdict(Counts)
        vendors: dict[str, Counts] = defaultdict(Counts)
        crates: dict[str, Counts] = defaultdict(Counts)
        directories: dict[str, Counts] = defaultdict(Counts)
        crate_cache: dict[Path, str] = {}
        for relative in files:
            language = language_for(relative)
            counts = count_lines((root / relative).read_text(encoding="utf-8"), language)
            vendor = is_vendor(relative)
            if language in DOCUMENT_LANGUAGES:
                documents[f"{'Vendor' if vendor else 'Project'} / {language}"].add(counts)
                continue
            (vendors if vendor else languages)[language].add(counts)
            prefix = "Vendor / " if vendor else ""
            if args.by_crate:
                crates[prefix + crate_name(root, relative, crate_cache)].add(counts)
            if args.by_dir:
                top = relative.parts[0] if len(relative.parts) > 1 else "(root)"
                directories[prefix + top].add(counts)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"project-stats: {error}", file=sys.stderr)
        return 1

    print(f"OxideTerm project statistics: {root}")
    print("Scope: tracked and non-ignored local files; generated files and asset directories excluded.")
    print("Line-based estimates; inline comments count as code. Test code is included, not estimated separately.")
    print_table("Project source", languages)
    if args.include_vendor:
        print_table("Bundled third-party source (including local patches)", vendors)
    print_table("Documentation and configuration (Code = non-comment content)", documents)
    if args.by_crate:
        print_table("Source by Cargo package", crates)
    if args.by_dir:
        print_table("Source by top-level directory", directories)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
