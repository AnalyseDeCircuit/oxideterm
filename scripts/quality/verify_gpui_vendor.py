#!/usr/bin/env python3
"""Verify the pinned GPUI-CE vendor baseline and its required license files."""

from __future__ import annotations

import argparse
import hashlib
import subprocess
import tomllib
from pathlib import Path


ROOT_DIR = Path(__file__).resolve().parents[2]
GPUI_VENDOR_DIR = ROOT_DIR / "crates" / "gpui-ce"
BASELINE_PATH = GPUI_VENDOR_DIR / "gpui" / "UPSTREAM_BASELINE.toml"
CARGO_LOCK_PATH = ROOT_DIR / "Cargo.lock"
THIRD_PARTY_LICENSE_DIR = ROOT_DIR / "licenses" / "third-party"
CANONICAL_LICENSE = THIRD_PARTY_LICENSE_DIR / "GPUI-CE-LICENSE-APACHE"
ROOT_UPSTREAM_LICENSE = GPUI_VENDOR_DIR / "gpui" / "GPUI_CE_ROOT_LICENSE.md"
MICROSOFT_TERMINAL_LICENSE = (
    THIRD_PARTY_LICENSE_DIR / "MICROSOFT-TERMINAL-LICENSE-MIT"
)
MICROSOFT_TERMINAL_LICENSE_SHA256 = (
    "3c181bf8ce0bab0c2e5be1b10132d2fa9450a99d3280ffc7136bd4e27a696e98"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--upstream-checkout",
        type=Path,
        help="Optional GPUI-CE Git checkout used to verify recorded tree hashes.",
    )
    return parser.parse_args()


def load_toml(path: Path) -> dict:
    with path.open("rb") as source:
        return tomllib.load(source)


def verify_workspace_members(crate_paths: set[str]) -> None:
    workspace = load_toml(ROOT_DIR / "Cargo.toml")["workspace"]
    workspace_members = set(workspace["members"])
    missing = sorted(crate_paths - workspace_members)
    if missing:
        raise RuntimeError(f"vendored GPUI crates missing from workspace: {missing}")


def verify_license(crate_path: str) -> None:
    license_path = ROOT_DIR / crate_path / "LICENSE-APACHE"
    if not license_path.is_file():
        raise RuntimeError(f"missing vendored GPUI license: {license_path}")
    if license_path.read_bytes() != CANONICAL_LICENSE.read_bytes():
        raise RuntimeError(f"vendored GPUI license differs from canonical copy: {license_path}")


def verify_recorded_tree(
    checkout: Path,
    revision: str,
    source_path: str,
    expected_tree: str,
) -> None:
    # Git tree object IDs provide a stable source snapshot without pretending
    # that locally patched vendor directories still match upstream byte-for-byte.
    actual_tree = subprocess.check_output(
        ["git", "rev-parse", f"{revision}:{source_path}"],
        cwd=checkout,
        text=True,
    ).strip()
    if actual_tree != expected_tree:
        raise RuntimeError(
            f"upstream tree mismatch for {source_path}: "
            f"expected {expected_tree}, got {actual_tree}"
        )


def upstream_file_bytes(checkout: Path, revision: str, source_path: str) -> bytes:
    """Read one pinned upstream file without depending on the checkout branch."""
    return subprocess.check_output(
        ["git", "show", f"{revision}:{source_path}"],
        cwd=checkout,
    )


def main() -> None:
    args = parse_args()
    baseline = load_toml(BASELINE_PATH)
    crates = baseline.get("crates") or []
    local_crate_paths = {entry["local_path"] for entry in crates}
    upstream_crate_paths = {entry["upstream_path"] for entry in crates}

    # The migration baseline is reviewed against one exact dependency graph.
    cargo_lock_digest = hashlib.sha256(CARGO_LOCK_PATH.read_bytes()).hexdigest()
    if cargo_lock_digest != baseline["oxideterm_cargo_lock_sha256"]:
        raise RuntimeError(
            "Cargo.lock differs from the dependency graph recorded by the GPUI baseline"
        )

    if len(local_crate_paths) != len(crates):
        raise RuntimeError("vendor baseline contains duplicate local crate paths")
    if len(upstream_crate_paths) != len(crates):
        raise RuntimeError("vendor baseline contains duplicate upstream crate paths")
    if not CANONICAL_LICENSE.is_file():
        raise RuntimeError(f"missing canonical GPUI-CE license: {CANONICAL_LICENSE}")
    if not ROOT_UPSTREAM_LICENSE.is_file():
        raise RuntimeError(f"missing GPUI-CE root license: {ROOT_UPSTREAM_LICENSE}")
    if not MICROSOFT_TERMINAL_LICENSE.is_file():
        raise RuntimeError(
            f"missing Microsoft Terminal license: {MICROSOFT_TERMINAL_LICENSE}"
        )
    microsoft_license_digest = hashlib.sha256(
        MICROSOFT_TERMINAL_LICENSE.read_bytes()
    ).hexdigest()
    if microsoft_license_digest != MICROSOFT_TERMINAL_LICENSE_SHA256:
        raise RuntimeError(
            "Microsoft Terminal license does not match the recorded source text"
        )

    verify_workspace_members(local_crate_paths)
    for entry in crates:
        crate_path = entry["local_path"]
        if not (ROOT_DIR / crate_path / "Cargo.toml").is_file():
            raise RuntimeError(f"missing vendored GPUI crate manifest: {crate_path}")
        verify_license(crate_path)

    if args.upstream_checkout is not None:
        revision = baseline["upstream_commit"]
        if CANONICAL_LICENSE.read_bytes() != upstream_file_bytes(
            args.upstream_checkout,
            revision,
            "crates/gpui/LICENSE-APACHE",
        ):
            raise RuntimeError("canonical GPUI-CE license differs from pinned upstream")
        if ROOT_UPSTREAM_LICENSE.read_bytes() != upstream_file_bytes(
            args.upstream_checkout,
            revision,
            "LICENSE.md",
        ):
            raise RuntimeError("GPUI-CE root license differs from pinned upstream")
        for entry in crates:
            verify_recorded_tree(
                args.upstream_checkout,
                revision,
                entry["upstream_path"],
                entry["upstream_tree"],
            )
    print(
        f"verified {len(crates)} GPUI vendor crates at "
        f"{baseline['upstream_commit']}"
    )


if __name__ == "__main__":
    main()
