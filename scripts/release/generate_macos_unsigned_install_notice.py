#!/usr/bin/env python3
"""Generate the bilingual unsigned macOS installation notice for stable DMGs."""

from __future__ import annotations

import argparse
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


ROOT_DIR = Path(__file__).resolve().parents[2]
DEFAULT_OUTPUT = (
    ROOT_DIR
    / "crates"
    / "oxideterm-gpui-app"
    / "resources"
    / "macos"
    / "unsigned-install-notice.png"
)
DEFAULT_DMG_BACKGROUND = (
    ROOT_DIR
    / "crates"
    / "oxideterm-gpui-app"
    / "resources"
    / "macos"
    / "unsigned-dmg-background.png"
)
APP_ICON = (
    ROOT_DIR
    / "crates"
    / "oxideterm-gpui-app"
    / "resources"
    / "icons"
    / "128x128@2x.png"
)
UI_FONT = Path("/System/Library/Fonts/PingFang.ttc")
MONO_FONT = Path("/System/Library/Fonts/SFNSMono.ttf")

CANVAS_SIZE = (1200, 720)
DMG_BACKGROUND_SIZE = (720, 452)
BACKGROUND = "#0a1725"
PANEL = "#10263a"
BORDER = "#29465e"
PRIMARY = "#e5edf5"
SECONDARY = "#9db0c3"
ACCENT = "#54a8e8"
WARNING = "#f4c95d"
COMMAND_BACKGROUND = "#07111c"


def load_font(path: Path, size: int, index: int = 0) -> ImageFont.FreeTypeFont:
    """Load a bundled macOS font with a deterministic size."""
    return ImageFont.truetype(str(path), size=size, index=index)


def draw_centered_text(
    draw: ImageDraw.ImageDraw,
    text: str,
    center_x: int,
    y: int,
    font: ImageFont.FreeTypeFont,
    fill: str,
) -> None:
    """Draw one line centered around the requested horizontal coordinate."""
    bounds = draw.textbbox((0, 0), text, font=font)
    width = bounds[2] - bounds[0]
    draw.text((center_x - width // 2, y), text, font=font, fill=fill)


def generate_notice(output: Path) -> None:
    """Render the stable unsigned-build installation notice as a PNG."""
    output.parent.mkdir(parents=True, exist_ok=True)
    image = Image.new("RGB", CANVAS_SIZE, BACKGROUND)
    draw = ImageDraw.Draw(image)

    title_font = load_font(UI_FONT, 48)
    subtitle_font = load_font(UI_FONT, 26)
    body_font = load_font(UI_FONT, 28)
    detail_font = load_font(UI_FONT, 23)
    mono_font = load_font(MONO_FONT, 26)

    draw.rounded_rectangle((48, 42, 1152, 678), radius=20, fill=PANEL, outline=BORDER, width=2)

    icon = Image.open(APP_ICON).convert("RGBA")
    icon.thumbnail((112, 112), Image.Resampling.LANCZOS)
    image.paste(icon, (82, 76), icon)

    draw.polygon([(1080, 78), (1130, 166), (1030, 166)], fill=WARNING)
    draw.rounded_rectangle((1076, 106, 1084, 142), radius=4, fill=BACKGROUND)
    draw.ellipse((1076, 150, 1084, 158), fill=BACKGROUND)

    draw.text((222, 78), "Unsigned macOS build", font=title_font, fill=PRIMARY)
    draw.text((224, 137), "未签名 macOS 版本", font=subtitle_font, fill=WARNING)

    steps = [
        ("1", "Drag OxideTerm to Applications", "将 OxideTerm 拖入“应用程序”文件夹"),
        ("2", "If macOS blocks the first launch, open Terminal", "若首次启动被 macOS 阻止，请打开“终端”"),
        ("3", "Run the command below, then launch OxideTerm again", "执行下方命令，然后重新启动 OxideTerm"),
    ]
    start_y = 220
    for index, (number, english, chinese) in enumerate(steps):
        y = start_y + index * 90
        draw.ellipse((82, y, 126, y + 44), fill=ACCENT)
        draw_centered_text(draw, number, 104, y + 3, body_font, BACKGROUND)
        draw.text((154, y - 2), english, font=body_font, fill=PRIMARY)
        draw.text((154, y + 38), chinese, font=detail_font, fill=SECONDARY)

    draw.rounded_rectangle(
        (82, 496, 1118, 568), radius=8, fill=COMMAND_BACKGROUND, outline=BORDER, width=2
    )
    command = 'xattr -cr "/Applications/OxideTerm.app"'
    draw_centered_text(draw, command, 600, 513, mono_font, PRIMARY)

    draw_centered_text(
        draw,
        "Verify the SHA-256 checksum from the GitHub Release before opening.",
        600,
        600,
        detail_font,
        SECONDARY,
    )
    draw_centered_text(
        draw,
        "打开前请核对 GitHub Release 提供的 SHA-256 校验和。",
        600,
        636,
        detail_font,
        SECONDARY,
    )

    image.save(output, format="PNG", optimize=True)


def generate_dmg_background(output: Path) -> None:
    """Render the compact Finder background for an unsigned stable DMG."""
    output.parent.mkdir(parents=True, exist_ok=True)
    # Finder draws icon labels in black, so the installer background must stay light.
    dmg_background = "#F2F3F5"
    dmg_panel = "#FFFFFF"
    dmg_primary = "#20242A"
    dmg_secondary = "#626B76"
    dmg_border = "#D7DBE0"
    image = Image.new("RGB", DMG_BACKGROUND_SIZE, dmg_background)
    draw = ImageDraw.Draw(image)

    title_font = load_font(UI_FONT, 34)
    subtitle_font = load_font(UI_FONT, 21)
    detail_font = load_font(UI_FONT, 17)

    draw_centered_text(draw, "Install OxideTerm", 360, 24, title_font, dmg_primary)
    draw_centered_text(
        draw, "拖入“应用程序”即可安装", 360, 68, subtitle_font, dmg_secondary
    )

    arrow_y = 196
    draw.rounded_rectangle((278, arrow_y - 4, 428, arrow_y + 4), radius=4, fill="#F97316")
    draw.polygon(
        [(428, arrow_y - 18), (456, arrow_y), (428, arrow_y + 18)], fill="#F97316"
    )

    draw.rounded_rectangle(
        (76, 300, 644, 388),
        radius=10,
        fill=dmg_panel,
        outline=dmg_border,
        width=1,
    )
    draw.polygon([(112, 320), (128, 350), (96, 350)], fill=WARNING)
    draw.rounded_rectangle((110, 330, 114, 342), radius=2, fill=dmg_primary)
    draw.ellipse((110, 346, 114, 350), fill=dmg_primary)
    draw.text(
        (150, 315),
        "First launch blocked? Open the guide at lower right.",
        font=detail_font,
        fill=dmg_primary,
    )
    draw.text(
        (150, 348),
        "首次启动被阻止？请打开右下角安装说明。",
        font=detail_font,
        fill=dmg_secondary,
    )

    image.save(output, format="PNG", optimize=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument(
        "--dmg-background-output", type=Path, default=DEFAULT_DMG_BACKGROUND
    )
    args = parser.parse_args()
    generate_notice(args.output.resolve())
    generate_dmg_background(args.dmg_background_output.resolve())


if __name__ == "__main__":
    main()
