#!/usr/bin/env python3
"""Stage the pinned Microsoft ConPTY runtime used by Windows builds."""

from __future__ import annotations

import argparse
import hashlib
import tempfile
import urllib.request
import zipfile
from pathlib import Path


PACKAGE_VERSION = "1.24.260710001"
PACKAGE_SHA256 = "175640566a3b59c4b132070ee96c2c77e5ab7edd2e92732a5eb3610bbf63d90e"
PACKAGE_NAME = f"microsoft.windows.console.conpty.{PACKAGE_VERSION}.nupkg"
PACKAGE_URL = (
    "https://api.nuget.org/v3-flatcontainer/microsoft.windows.console.conpty/"
    f"{PACKAGE_VERSION}/{PACKAGE_NAME}"
)
CACHE_DIR = Path(__file__).resolve().parents[2] / "target" / "conpty-runtime"
RUNTIMES = {
    "x86_64-pc-windows-msvc": (
        "x64",
        "39fba2713e2495117b1591ae8c32a3b904bea7aa66069cf7815e2844c76d75d8",
        "b7fd936c2668b87b9ecf7b3366dc6568afc1c6f981874cba3e955a1c35cf8160",
    ),
    "aarch64-pc-windows-msvc": (
        "arm64",
        "db3d173640b172bafd42d5b541b638a9aeec1c7d0e40dd636bf02822a32c912c",
        "ed7622fd0d3bedc9ab9f122f5e58edf0def9e7999224f52dd395ba9f54edbe09",
    ),
}


def runtime_files(target: str) -> dict[str, tuple[str, str]]:
    """Map bundle-relative paths to package entries and their pinned digests."""
    arch, dll_digest, host_digest = RUNTIMES[target]
    return {
        "conpty.dll": (f"runtimes/win-{arch}/native/conpty.dll", dll_digest),
        f"{arch}/OpenConsole.exe": (
            f"build/native/runtimes/{arch}/OpenConsole.exe", host_digest
        ),
    }


def verify_digest(content: bytes, expected: str, name: str) -> None:
    if hashlib.sha256(content).hexdigest() != expected:
        raise RuntimeError(f"ConPTY SHA-256 mismatch: {name}")


def cached_package() -> Path:
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    package = CACHE_DIR / PACKAGE_NAME
    if not package.exists():
        # An interrupted download must never become a reusable cache entry.
        with urllib.request.urlopen(PACKAGE_URL, timeout=60) as response:
            content = response.read()
        verify_digest(content, PACKAGE_SHA256, PACKAGE_NAME)
        with tempfile.NamedTemporaryFile(dir=CACHE_DIR, delete=False) as temporary:
            temporary.write(content)
            temporary_path = Path(temporary.name)
        try:
            temporary_path.replace(package)
        finally:
            temporary_path.unlink(missing_ok=True)
    return package


def stage_runtime(resources: Path, target: str, package: Path | None = None) -> None:
    files = runtime_files(target)
    package = package if package is not None else cached_package()
    verify_digest(package.read_bytes(), PACKAGE_SHA256, package.name)
    with zipfile.ZipFile(package) as archive:
        # Validate both files before changing a development or package directory.
        payloads = {name: archive.read(entry) for name, (entry, _) in files.items()}
    for name, payload in payloads.items():
        verify_digest(payload, files[name][1], name)
    for name, payload in payloads.items():
        destination = resources / "conpty" / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(payload)
    print(f"Bundled Microsoft ConPTY {PACKAGE_VERSION} for {target}", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=RUNTIMES, required=True)
    parser.add_argument("--destination", type=Path, required=True, help="Directory containing the application executable")
    args = parser.parse_args()
    stage_runtime(args.destination / "resources", args.target)


if __name__ == "__main__":
    main()
