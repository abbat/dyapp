#!/usr/bin/env python3
"""Rasterize the SVG icon masters for desktop and Apple packages.

Needs rsvg-convert (Debian: librsvg2-bin) and Pillow. Sizes up to 24 px come
from the tray variant: the pentagon outline and dots of the full icon blur
there."""

from io import BytesIO
from pathlib import Path
import shutil
import subprocess

from PIL import Image


ROOT = Path(__file__).resolve().parents[2]
APP = ROOT / "assets/icon.svg"
SMALL = ROOT / "assets/icon-tray.svg"
SMALL_MAX = 24
BACKGROUND = "#12141F"  # iOS needs an opaque square; the system rounds it


def render(size: int) -> Image.Image:
    svg = SMALL if size <= SMALL_MAX else APP
    png = subprocess.run(
        ["rsvg-convert", "-w", str(size), "-h", str(size), str(svg)],
        check=True,
        capture_output=True,
    ).stdout
    image = Image.open(BytesIO(png)).convert("RGBA")
    if image.size != (size, size):
        raise ValueError(f"{svg.name} rendered as {image.size}, not {size}")
    return image


def main() -> None:
    if not shutil.which("rsvg-convert"):
        raise SystemExit("rsvg-convert not found (Debian: librsvg2-bin)")
    render(32).save(ROOT / "clients/linux/icons/icon.png")
    windows = ROOT / "clients/windows/icons"
    big = render(256)
    big.save(windows / "icon.png")
    sizes = (16, 24, 32, 48, 64, 128, 256)
    big.save(
        windows / "icon.ico",
        format="ICO",
        sizes=[(n, n) for n in sizes],
        append_images=[render(n) for n in sizes[:-1]],
    )
    ios = Image.new("RGBA", (1024, 1024), BACKGROUND)
    ios.alpha_composite(render(1024))
    ios_icons = ROOT / "clients/ios/DYApp/Assets.xcassets/AppIcon.appiconset"
    ios.convert("RGB").save(ios_icons / "icon-1024.png")
    mac = ROOT / "clients/macos/Assets.xcassets/AppIcon.appiconset"
    for size in (16, 32, 64, 128, 256, 512, 1024):
        render(size).save(mac / f"icon-{size}.png")


if __name__ == "__main__":
    main()
