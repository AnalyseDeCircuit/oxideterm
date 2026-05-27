#!/usr/bin/env python3
"""Generate third-party Rust dependency notices from cargo-deny output."""

from __future__ import annotations

import argparse
import json
import subprocess
from collections import Counter
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path


PERMISSIVE_LICENSES = {
    "MIT",
    "Apache-2.0",
    "BSD-1-Clause",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "0BSD",
    "Zlib",
    "Unlicense",
    "CC0-1.0",
    "MIT-0",
}


@dataclass(frozen=True)
class CrateNotice:
    name: str
    version: str
    source: str
    licenses: tuple[str, ...]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate THIRD_PARTY_NOTICES.md from cargo deny license data.",
    )
    parser.add_argument("--cwd", default=".", help="Cargo project directory to inspect.")
    parser.add_argument("--output", default="THIRD_PARTY_NOTICES.md", help="Output file path relative to --cwd.")
    parser.add_argument("--title", default="Third-Party Notices", help="Markdown title.")
    parser.add_argument("--exclude-name", action="append", default=[], help="Crate name to exclude.")
    parser.add_argument("--exclude-prefix", action="append", default=[], help="Crate name prefix to exclude.")
    return parser.parse_args()


def cargo_deny_license_data(cwd: Path) -> dict[str, dict[str, list[str]]]:
    # cargo-deny already understands SPDX metadata and workspace resolution, so
    # keep this script as formatting glue instead of reimplementing license logic.
    completed = subprocess.run(
        ["cargo", "deny", "list", "-f", "json", "-l", "crate"],
        cwd=cwd,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return json.loads(completed.stdout)


def parse_cargo_deny_key(key: str) -> tuple[str, str, str]:
    parts = key.split(" ")
    name = parts[0] if parts else key
    version = parts[1] if len(parts) > 1 else ""
    source = " ".join(parts[2:]) if len(parts) > 2 else ""
    return name, version, source


def is_excluded(crate: CrateNotice, exclude_names: set[str], exclude_prefixes: list[str]) -> bool:
    return crate.name in exclude_names or any(crate.name.startswith(prefix) for prefix in exclude_prefixes)


def is_copyleft(license_name: str) -> bool:
    normalized = license_name.upper()
    return normalized.startswith("GPL-") or normalized.startswith("AGPL-") or normalized.startswith("LGPL-")


def has_permissive_option(crate: CrateNotice) -> bool:
    return any(license_name in PERMISSIVE_LICENSES for license_name in crate.licenses)


def table_cell(value: object) -> str:
    return str(value or "").replace("|", "\\|").replace("\n", " ")


def crate_table(crates: list[CrateNotice]) -> str:
    lines = ["| Crate | Version | Licenses | Source |", "|---|---:|---|---|"]
    for crate in crates:
        lines.append(
            f"| {table_cell(crate.name)} | {table_cell(crate.version)} | "
            f"{table_cell(', '.join(crate.licenses))} | {table_cell(crate.source)} |"
        )
    return "\n".join(lines) + "\n\n"


def build_notices(args: argparse.Namespace) -> tuple[str, int, int]:
    cwd = Path(args.cwd).resolve()
    data = cargo_deny_license_data(cwd)
    exclude_names = set(args.exclude_name)
    exclude_prefixes = list(args.exclude_prefix)

    crates: list[CrateNotice] = []
    for key, value in data.items():
        name, version, source = parse_cargo_deny_key(key)
        crate = CrateNotice(
            name=name,
            version=version,
            source=source,
            licenses=tuple(value.get("licenses") or []),
        )
        if not is_excluded(crate, exclude_names, exclude_prefixes):
            crates.append(crate)

    crates.sort(key=lambda crate: (crate.name, crate.version, crate.source))

    license_counts = Counter(license_name for crate in crates for license_name in crate.licenses)
    copyleft_strict = [
        crate for crate in crates if any(is_copyleft(license_name) for license_name in crate.licenses) and not has_permissive_option(crate)
    ]
    copyleft_with_permissive = [
        crate for crate in crates if any(is_copyleft(license_name) for license_name in crate.licenses) and has_permissive_option(crate)
    ]

    generated_at = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    lines = [
        f"# {args.title}",
        "",
        "This file lists third-party Rust crates and detected licenses, including transitive dependencies.",
        "It is generated from `cargo deny list -f json -l crate`.",
        f"Generated: {generated_at}",
        "",
        "## Summary",
        "",
    ]

    if license_counts:
        for license_name, count in sorted(license_counts.items(), key=lambda item: (-item[1], item[0])):
            lines.append(f"- {license_name}: {count}")
    else:
        lines.append("No third-party crates detected.")
    lines.append("")

    if copyleft_strict or copyleft_with_permissive:
        lines.extend(
            [
                "## Copyleft Notes",
                "",
                "Crates can be multi-licensed. When a crate lists both copyleft and permissive licenses, OxideTerm uses the most permissive compatible option available.",
                "This section is a review prompt for binary distribution; it does not replace legal review.",
                "",
            ]
        )

    output = "\n".join(lines)
    if copyleft_strict:
        output += "### Copyleft (no permissive option detected)\n\n"
        output += crate_table(copyleft_strict)
    if copyleft_with_permissive:
        output += "### Copyleft present, but permissive options also listed\n\n"
        output += crate_table(copyleft_with_permissive)

    output += "## Crates\n\n"
    output += crate_table(crates)
    output += "## Notes\n\n"
    output += "- Multi-license policy: where a crate offers multiple licenses, OxideTerm uses the most permissive compatible option available.\n"
    output += "- License data is generated from crate metadata through cargo-deny and may include multiple licenses per crate.\n"
    output += "- This notice list is for attribution and compliance tracking. It does not replace upstream license texts.\n"

    return output, len(crates), len(copyleft_strict) + len(copyleft_with_permissive)


def main() -> None:
    args = parse_args()
    cwd = Path(args.cwd).resolve()
    output_path = (cwd / args.output).resolve()
    output, crate_count, copyleft_count = build_notices(args)
    output_path.write_text(output, encoding="utf-8")
    print(f"Wrote {output_path.relative_to(Path.cwd())} with {crate_count} crate entries ({copyleft_count} copyleft-flagged).")


if __name__ == "__main__":
    main()
