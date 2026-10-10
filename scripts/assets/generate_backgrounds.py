#!/usr/bin/env python3
"""Generate OxideTerm's original looping artwork with NumPy, Pillow and FFmpeg.

Run with --preview-only to inspect the composition before encoding both videos.
No external images, models or source footage are used.
"""

import argparse
import hashlib
from pathlib import Path
import subprocess

import numpy as np
from PIL import Image


ROOT = Path(__file__).resolve().parents[2]
ARTWORK = ROOT / "crates/oxideterm-gpui-app/resources/backgrounds"
WIDTH, HEIGHT = 1000, 625
FPS, SECONDS = 24, 16
Y, X = np.mgrid[-1:1:complex(HEIGHT), -1.6:1.6:complex(WIDTH)].astype(np.float32)
RADIUS = np.sqrt((X / 1.6) ** 2 + Y**2)
EDGE = np.clip((RADIUS - 0.18) / 0.85, 0, 1)
EDGE = EDGE * EDGE * (3 - 2 * EDGE)
RNG = np.random.default_rng(20261010)
DITHER = RNG.uniform(-0.65, 0.65, (HEIGHT, WIDTH, 1)).astype(np.float32)
STARS = RNG.uniform([0, 0, 0, 0.25], [WIDTH, HEIGHT, np.pi * 2, 0.8], (42, 4))


def cloud(x: np.ndarray, y: np.ndarray) -> np.ndarray:
    result = np.zeros_like(x)
    for frequency, weight, angle, offset in [
        (1.7, 0.52, 0.4, 1.3),
        (3.1, 0.27, 2.1, 3.7),
        (6.3, 0.13, 1.2, 0.8),
        (12.9, 0.06, 2.7, 2.5),
        (26.1, 0.02, 0.7, 4.1),
    ]:
        u = x * np.cos(angle) + y * np.sin(angle)
        v = y * np.cos(angle) - x * np.sin(angle)
        result += weight * np.sin(u * frequency + offset) * np.cos(v * frequency * 0.83 - offset)
    return result


def frame(kind: str, phase: float) -> np.ndarray:
    # Every time-dependent term is periodic over one turn, including the warp and stars.
    u = X + 0.22 * np.cos(phase) + 0.24 * np.sin(Y * 2.1 + 0.45 * np.sin(phase))
    v = Y + 0.18 * np.sin(phase) + 0.20 * np.sin(X * 1.8 + 0.35 * np.cos(phase))
    field = cloud(u * 1.7, v * 1.5)
    detail = cloud(u * 2.4 + 7.1, v * 2.2 - 3.2)
    folds = np.exp(-((field + 0.23 * detail) / 0.13) ** 2)
    ribbon = np.exp(-((Y + 0.39 * np.sin(X * 1.6 + field * 1.8)) / 0.55) ** 2)
    hue = np.clip(0.5 + 0.48 * np.sin(X * 1.35 - Y * 1.8 + detail * 2.1), 0, 1)
    if kind == "dawn-mist":
        base = np.array([239, 235, 227], dtype=np.float32)
        teal = np.array([106, 166, 161], dtype=np.float32)
        gold = np.array([210, 169, 104], dtype=np.float32)
        tint = teal * (1 - hue[..., None]) + gold * hue[..., None]
        density = (0.15 + 0.48 * ribbon + 0.24 * field) * (0.15 + 0.85 * EDGE)
        pixels = base + (tint - base) * density[..., None]
        pixels += (folds * EDGE * 16)[..., None]
    else:
        base = np.array([8, 14, 29], dtype=np.float32)
        blue = np.array([38, 106, 151], dtype=np.float32)
        violet = np.array([122, 62, 156], dtype=np.float32)
        tint = blue * (1 - hue[..., None]) + violet * hue[..., None]
        density = (0.16 + 0.30 * ribbon + 0.27 * field + 0.28 * folds) * (0.12 + 0.88 * EDGE)
        pixels = base + (tint - base) * density[..., None]
        # Small edge lights leave the central reading area calm.
        for sx, sy, offset, strength in STARS:
            if ((sx / WIDTH - 0.5) / 0.35) ** 2 + ((sy / HEIGHT - 0.5) / 0.35) ** 2 < 1:
                continue
            cx = sx + 2.5 * np.cos(phase + offset)
            cy = sy + 1.8 * np.sin(phase + offset)
            left, top = max(0, int(cx) - 4), max(0, int(cy) - 4)
            right, bottom = min(WIDTH, int(cx) + 5), min(HEIGHT, int(cy) + 5)
            yy, xx = np.mgrid[top:bottom, left:right]
            glow = np.exp(-((xx - cx) ** 2 + (yy - cy) ** 2) / 1.3)
            light = strength * (0.7 + 0.3 * np.sin(phase + offset))
            pixels[top:bottom, left:right] += (glow * light)[..., None] * np.array([110, 146, 175])
    return np.clip(pixels + DITHER, 0, 255).astype(np.uint8)


def encode(kind: str, destination: Path) -> None:
    command = [
        "ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
        "-f", "rawvideo", "-pixel_format", "rgb24", "-video_size", f"{WIDTH}x{HEIGHT}",
        "-framerate", str(FPS), "-i", "pipe:0", "-an",
        "-vf", "scale=1600:1000:flags=lanczos:out_color_matrix=bt709:out_range=tv",
        "-c:v", "libx264", "-preset", "slow", "-crf", "18", "-pix_fmt", "yuv420p",
        "-g", str(FPS * 2), "-movflags", "+faststart",
        "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709",
        "-color_range", "tv", str(destination),
    ]
    with subprocess.Popen(command, stdin=subprocess.PIPE) as encoder:
        try:
            for index in range(FPS * SECONDS):
                encoder.stdin.write(frame(kind, index * np.pi * 2 / (FPS * SECONDS)).tobytes())
                if index % (FPS * 4) == 0:
                    print(f"{kind}: {index // FPS}/{SECONDS} seconds", flush=True)
        finally:
            encoder.stdin.close()
        if encoder.wait() != 0:
            raise RuntimeError(f"FFmpeg failed to encode {destination}")
    digest = hashlib.sha256(destination.read_bytes()).hexdigest()
    print(f"{destination.name}: {destination.stat().st_size / 1024**2:.2f} MiB; SHA-256 {digest}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, default=ARTWORK)
    parser.add_argument("--preview-only", action="store_true")
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    for kind in ["dawn-mist", "night-flow"]:
        if args.preview_only:
            for quarter in range(4):
                Image.fromarray(frame(kind, quarter * np.pi / 2)).save(
                    args.output_dir / f"oxide-{kind}-{quarter}.png"
                )
        else:
            encode(kind, args.output_dir / f"oxide-{kind}-v1.mp4")


if __name__ == "__main__":
    main()
