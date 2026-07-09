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

VENDORED_WORKSPACE_PACKAGES = {
    "russh",
}


@dataclass(frozen=True)
class CrateNotice:
    name: str
    version: str
    source: str
    licenses: tuple[str, ...]


@dataclass(frozen=True)
class BundledAssetNotice:
    name: str
    license_name: str
    license_file: str
    file_count: int


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate THIRD_PARTY_NOTICES.md from cargo deny license data.",
    )
    parser.add_argument("--cwd", default=".", help="Cargo project directory to inspect.")
    parser.add_argument("--output", default="THIRD_PARTY_NOTICES.md", help="Output file path relative to --cwd.")
    parser.add_argument("--title", default="Third-Party Notices", help="Markdown title.")
    parser.add_argument("--exclude-name", action="append", default=[], help="Crate name to exclude.")
    parser.add_argument("--exclude-prefix", action="append", default=[], help="Crate name prefix to exclude.")
    parser.add_argument(
        "--check",
        action="store_true",
        help="Verify the existing output is current without rewriting it.",
    )
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


def workspace_package_names(cwd: Path) -> set[str]:
    # `cargo deny list` includes local workspace packages. Third-party notices
    # should describe external/vendor obligations, not OxideTerm's own GPL
    # crates. Keep vendored workspace packages such as our patched russh fork:
    # those are local paths, but still third-party attribution obligations.
    completed = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=cwd,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    metadata = json.loads(completed.stdout)
    workspace_members = set(metadata.get("workspace_members") or [])
    return {
        package.get("name", "")
        for package in metadata.get("packages", [])
        if package.get("id") in workspace_members and package.get("name")
        and package.get("name") not in VENDORED_WORKSPACE_PACKAGES
    }


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


def bundled_asset_notices(cwd: Path) -> list[BundledAssetNotice]:
    # The native app embeds decompressed TTF terminal font subsets for
    # GPUI/font-kit. Keep asset notices here so binary distributions do not
    # silently omit font attribution.
    fonts_dir = cwd / "crates" / "oxideterm-gpui-app" / "resources" / "fonts"
    candidates = [
        ("JetBrains Mono Subset", "SIL Open Font License 1.1", fonts_dir / "JetBrainsMono" / "OFL.txt", fonts_dir / "JetBrainsMono"),
        ("Meslo Nerd Font Subset", "Apache License 2.0", fonts_dir / "Meslo" / "LICENSE.txt", fonts_dir / "Meslo"),
        ("Maple Mono NF CN Subset", "SIL Open Font License 1.1", fonts_dir / "MapleMono" / "LICENSE.txt", fonts_dir / "MapleMono"),
    ]
    notices = []
    for name, license_name, license_file, asset_dir in candidates:
        if not license_file.exists() or not asset_dir.exists():
            continue
        notices.append(
            BundledAssetNotice(
                name=name,
                license_name=license_name,
                license_file=license_file.relative_to(cwd).as_posix(),
                file_count=len(list(asset_dir.glob("*.ttf"))),
            )
        )
    return notices


def bundled_asset_table(assets: list[BundledAssetNotice]) -> str:
    lines = ["| Asset | Files | License | License File |", "|---|---:|---|---|"]
    for asset in assets:
        lines.append(
            f"| {table_cell(asset.name)} | {asset.file_count} | "
            f"{table_cell(asset.license_name)} | {table_cell(asset.license_file)} |"
        )
    return "\n".join(lines) + "\n\n"


def build_notices(args: argparse.Namespace) -> tuple[str, int, int]:
    cwd = Path(args.cwd).resolve()
    data = cargo_deny_license_data(cwd)
    bundled_assets = bundled_asset_notices(cwd)
    exclude_names = set(args.exclude_name) | workspace_package_names(cwd)
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

    generated_at = getattr(
        args,
        "generated_at",
        datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
    )
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
    if bundled_assets:
        output += "## Bundled Fonts / Assets\n\n"
        output += bundled_asset_table(bundled_assets)
    output += "## Notes\n\n"
    output += "- Multi-license policy: where a crate offers multiple licenses, OxideTerm uses the most permissive compatible option available.\n"
    output += "- License data is generated from crate metadata through cargo-deny and may include multiple licenses per crate.\n"
    output += "- This notice list is for attribution and compliance tracking. It does not replace upstream license texts.\n"

    return output, len(crates), len(copyleft_strict) + len(copyleft_with_permissive)


def main() -> None:
    args = parse_args()
    cwd = Path(args.cwd).resolve()
    output_path = (cwd / args.output).resolve()
    if args.check and output_path.is_file():
        # Preserve the recorded timestamp during verification so dependency
        # changes, rather than the current clock, determine freshness.
        for line in output_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("Generated: "):
                args.generated_at = line.removeprefix("Generated: ")
                break
    output, crate_count, copyleft_count = build_notices(args)
    if args.check:
        if not output_path.is_file() or output_path.read_text(encoding="utf-8") != output:
            raise SystemExit(f"{output_path} is stale; regenerate third-party notices")
        print(f"Verified {output_path.relative_to(Path.cwd())} with {crate_count} crate entries.")
        return
    output_path.write_text(output, encoding="utf-8")
    print(f"Wrote {output_path.relative_to(Path.cwd())} with {crate_count} crate entries ({copyleft_count} copyleft-flagged).")


if __name__ == "__main__":
    main()
