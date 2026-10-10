#!/usr/bin/env python3
"""Decode an independent fixture using the final Linux package in a clean container."""

import argparse
from pathlib import Path
import subprocess
import tempfile


def verify_frames(output: str) -> None:
    frames = [list(map(int, line.split())) for line in output.splitlines() if line.strip()]
    if len(frames) != 56:
        raise RuntimeError(f"Expected 56 decoded frames, got {len(frames)}")
    for index, frame in enumerate(frames):
        if frame[:5] != [index + 1, index * 40, 40, 64, 48]:
            raise RuntimeError(f"Incorrect decoded frame timing or dimensions: {frame[:5]}")
        expected = [0, 0, 253, 255] if index % 50 < 25 else [254, 0, 0, 255]
        if any(abs(value - color) > 3 for value, color in zip(frame[5:], expected)) or len(frame) != 9:
            raise RuntimeError(f"Incorrect BGRA pixels at frame {index + 1}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--package", required=True, type=Path)
    parser.add_argument("--probe", required=True, type=Path)
    parser.add_argument("--fixture", required=True, type=Path)
    args = parser.parse_args()
    package = args.package.resolve()
    with tempfile.TemporaryDirectory(prefix="oxideterm-media-package-") as directory:
        scratch = Path(directory)
        mounts = ["-v", f"{args.probe.resolve()}:/media/probe:ro", "-v", f"{args.fixture.resolve()}:/media/fixture.mp4:ro"]
        if package.suffix == ".AppImage":
            subprocess.run([str(package), "--appimage-extract"], cwd=scratch, check=True, stdout=subprocess.DEVNULL)
            root = scratch / "squashfs-root"
            command = ["docker", "run", "--rm", "--network", "none", *mounts, "-v", f"{root}:/package:ro", "ubuntu:22.04",
                "env", "LD_LIBRARY_PATH=/package/usr/lib", "GST_PLUGIN_PATH=", "GST_PLUGIN_PATH_1_0=",
                "GST_PLUGIN_SYSTEM_PATH=/package/usr/lib/gstreamer-1.0", "GST_PLUGIN_SYSTEM_PATH_1_0=/package/usr/lib/gstreamer-1.0",
                "GST_PLUGIN_SCANNER=/package/usr/libexec/gst-plugin-scanner", "GST_PLUGIN_SCANNER_1_0=/package/usr/libexec/gst-plugin-scanner",
                "GST_REGISTRY_1_0=/media-registry.bin", "/media/probe", "/media/fixture.mp4"]
        elif package.suffix == ".deb":
            command = ["docker", "run", "--rm", *mounts, "-v", f"{package}:/media/package.deb:ro", "ubuntu:22.04",
                "sh", "-c", "apt-get update >&2 && apt-get install -y /media/package.deb >&2 && /media/probe /media/fixture.mp4"]
        elif package.suffix == ".rpm":
            command = ["docker", "run", "--rm", *mounts, "-v", f"{package}:/media/package.rpm:ro", "fedora:44",
                "sh", "-c", "dnf install -y /media/package.rpm >&2 && /media/probe /media/fixture.mp4"]
        else:
            raise RuntimeError("The clean-environment decoder check supports AppImage, DEB and RPM packages")
        try:
            result = subprocess.run(command, check=True, capture_output=True, text=True, timeout=900)
        except subprocess.CalledProcessError as error:
            raise RuntimeError(f"Packaged background decoder failed:\n{error.stderr}") from error
        verify_frames(result.stdout)
    print(f"Background decoding verified in final {package.suffix} package")


if __name__ == "__main__":
    main()
