"""WCAG 2.4.13: compare the same rendered pixels before and after focus."""

import math
import sys
from typing import Protocol


class Surface(Protocol):
    def get_width(self) -> int:
        """Image width in device pixels."""
        ...

    def get_height(self) -> int:
        """Image height in device pixels."""
        ...

    def get_stride(self) -> int:
        """Byte offset between adjacent image rows."""
        ...

    def get_data(self) -> memoryview:
        """Native Cairo ARGB32 bytes."""
        ...


Rect = tuple[float, float, float, float]


def crop(surface: Surface, rect: Rect, scale: float, padding: float) -> bytes:
    """Read a Cairo ARGB32 component plus its outline/shadow extent."""
    x, y, width, height = rect
    left = max(0, math.floor((x - padding) * scale))
    top = max(0, math.floor((y - padding) * scale))
    right = min(surface.get_width(), math.ceil((x + width + padding) * scale))
    bottom = min(
        surface.get_height(), math.ceil((y + height + padding) * scale)
    )
    data, stride = surface.get_data(), surface.get_stride()
    return b"".join(
        bytes(data[row * stride + left * 4 : row * stride + right * 4])
        for row in range(top, bottom)
    )


LINEAR = tuple(
    value / 255 / 12.92
    if value / 255 <= 0.04045
    else ((value / 255 + 0.055) / 1.055) ** 2.4
    for value in range(256)
)


def contrasting_area(before: bytes, after: bytes) -> int:
    """Count focused/unfocused pixels with at least 3:1 contrast."""
    if len(before) != len(after) or len(before) % 4:
        raise ValueError("Focus images must have equal ARGB32 dimensions")
    red, green, blue = (2, 1, 0) if sys.byteorder == "little" else (1, 2, 3)
    count = 0
    for offset in range(0, len(before), 4):
        if before[offset : offset + 4] == after[offset : offset + 4]:
            continue
        a = (
            0.2126 * LINEAR[before[offset + red]]
            + 0.7152 * LINEAR[before[offset + green]]
            + 0.0722 * LINEAR[before[offset + blue]]
        )
        b = (
            0.2126 * LINEAR[after[offset + red]]
            + 0.7152 * LINEAR[after[offset + green]]
            + 0.0722 * LINEAR[after[offset + blue]]
        )
        if max(a, b) + 0.05 >= 3 * (min(a, b) + 0.05):
            count += 1
    return count


def required_area(width: float, height: float, scale: float) -> float:
    """W3C: 4h + 4w, scaled from CSS pixels to physical pixel area."""
    return 4 * (width + height) * scale * scale
