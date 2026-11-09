"""Renderer-based focus checks follow W3C contrast and perimeter examples."""

import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    "focus_pixels", Path(__file__).parent / "scripts/focus_pixels.py"
)
assert SPEC is not None and SPEC.loader is not None
PIXELS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PIXELS)


class FocusPixels(unittest.TestCase):
    def test_perimeter_and_device_scale(self) -> None:
        self.assertEqual(PIXELS.required_area(90, 30, 1), 480)
        self.assertEqual(PIXELS.required_area(90, 30, 2), 1920)

    def test_contrast_uses_changed_pixels_not_presence_of_a_shadow(
        self,
    ) -> None:
        white = bytes([255, 255, 255, 255]) * 500
        black = bytes([0, 0, 0, 255]) * 480 + bytes([255, 255, 255, 255]) * 20
        pale = bytes([230, 230, 230, 255]) * 500
        self.assertEqual(PIXELS.contrasting_area(white, white), 0)
        self.assertEqual(PIXELS.contrasting_area(white, pale), 0)
        self.assertEqual(PIXELS.contrasting_area(white, black), 480)
        self.assertEqual(PIXELS.contrasting_area(black, white), 480)

    def test_mismatched_frames_fail(self) -> None:
        with self.assertRaises(ValueError):
            PIXELS.contrasting_area(b"1234", b"12345678")
