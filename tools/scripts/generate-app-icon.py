#!/usr/bin/env python3
"""Rasterize the small, fixed SVG icon with Pillow for desktop packages."""

from pathlib import Path
import re
import xml.etree.ElementTree as ET

from PIL import Image, ImageColor, ImageDraw


ROOT = Path(__file__).resolve().parents[2]
SVG = ROOT / "assets/icon.svg"
SCALE = 4


def box(element: ET.Element) -> tuple[int, int, int, int]:
    keys = ("x", "y", "width", "height")
    return tuple(int(element.attrib[key]) * SCALE for key in keys)


def render(size: int = 256) -> Image.Image:
    root = ET.parse(SVG).getroot()
    canvas = Image.new("RGBA", (256 * SCALE, 256 * SCALE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(canvas)
    for element in root:
        kind = element.tag.rsplit("}", 1)[-1]
        if kind == "rect":
            x, y, width, height = box(element)
            draw.rounded_rectangle(
                (x, y, x + width, y + height),
                radius=int(element.attrib["rx"]) * SCALE,
                fill=ImageColor.getrgb(element.attrib["fill"]),
            )
        elif kind == "path":
            coordinates = [
                int(n) * SCALE
                for n in re.findall(r"\d+", element.attrib["d"])
            ]
            if len(coordinates) != 4:
                raise ValueError("icon path must be a single line")
            width = int(element.attrib["stroke-width"]) * SCALE
            color = ImageColor.getrgb(element.attrib["stroke"])
            draw.line(coordinates, fill=color, width=width)
            ends = (
                (coordinates[0], coordinates[1]),
                (coordinates[2], coordinates[3]),
            )
            for x, y in ends:
                radius = width // 2
                draw.ellipse(
                    (x - radius, y - radius, x + radius, y + radius),
                    fill=color,
                )
        elif kind == "circle":
            x = int(element.attrib["cx"]) * SCALE
            y = int(element.attrib["cy"]) * SCALE
            radius = int(element.attrib["r"]) * SCALE
            draw.ellipse(
                (x - radius, y - radius, x + radius, y + radius),
                fill=ImageColor.getrgb(element.attrib["fill"]),
            )
        else:
            raise ValueError(f"unsupported SVG element: {kind}")
    return canvas.resize((size, size), Image.Resampling.LANCZOS)


def main() -> None:
    image = render()
    image.resize((32, 32), Image.Resampling.LANCZOS).save(
        ROOT / "clients/linux/icons/icon.png"
    )
    image.save(ROOT / "clients/windows/icons/icon.png")
    image.save(
        ROOT / "clients/windows/icons/icon.ico",
        format="ICO",
        sizes=[
            (16, 16), (24, 24), (32, 32), (48, 48),
            (64, 64), (128, 128), (256, 256),
        ],
    )
    ios_icon = render(1024)
    opaque = Image.new("RGBA", ios_icon.size, "#392348")
    opaque.alpha_composite(ios_icon)
    ios_icons = ROOT / "clients/ios/DYApp/Assets.xcassets/AppIcon.appiconset"
    opaque.convert("RGB").save(ios_icons / "icon-1024.png")
    mac_icons = ROOT / "clients/macos/Assets.xcassets/AppIcon.appiconset"
    for size in (16, 32, 64, 128, 256, 512, 1024):
        render(size).save(mac_icons / f"icon-{size}.png")


if __name__ == "__main__":
    main()
