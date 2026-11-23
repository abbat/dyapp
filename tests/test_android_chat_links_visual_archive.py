"""Require complete source-bound native chat destination evidence."""

from itertools import product
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
from unittest import TestCase, main

from test_android_visual_archive import ARCHIVE, PNG, SOURCE, add

ROOT = Path(__file__).resolve().parents[1]
SCREENS = ("chat-links-live", "chat-links-edited", "chat-links-deleted")


def matrix(root: Path, width: int) -> Path:
    source = root / f"links-{width}.tar"
    prefix = f"files/chat-links-visual/{width}"
    frames = []
    with tarfile.open(source, "w") as archive:
        for screen, language, theme, font, edge in product(
            SCREENS,
            ("en", "ru"),
            ("light", "dark"),
            (100, 200),
            ("top", "bottom"),
        ):
            name = f"{screen}-{language}-{width}-{theme}-{font}-{edge}.png"
            add(archive, f"{prefix}/{name}", PNG)
            frames.append(
                {
                    "file": name,
                    "viewport_width_dp": width,
                    "pixel_width": 1,
                    "pixel_height": 1,
                    "font_percent": font,
                    "actual_font_scale": font / 100,
                    "actual_density": 1,
                }
            )
        add(
            archive,
            f"{prefix}/manifest.json",
            json.dumps(
                {
                    "source_sha": SOURCE,
                    "requested_width_dp": width,
                    "complete": True,
                    "frames": frames,
                }
            ).encode(),
        )
    return source


class AndroidChatLinksVisualArchive(TestCase):
    def test_cli_checks_both_widths_and_refuses_foreign_evidence(
        self,
    ) -> None:
        self.assertEqual(tuple(ARCHIVE.LINK_SCREENS), SCREENS)
        with tempfile.TemporaryDirectory(dir=ROOT / "build") as temporary:
            root = Path(temporary)
            for width in (360, 412):
                source = matrix(root, width)
                output = root / f"frames-{width}"
                result = subprocess.run(
                    [
                        sys.executable,
                        "-B",
                        str(ROOT / "tests/scripts/android_visual_archive.py"),
                        "--kind",
                        "links",
                        "--archive",
                        str(source),
                        "--output",
                        str(output),
                        "--width",
                        str(width),
                        "--source",
                        SOURCE,
                    ],
                    capture_output=True,
                    text=True,
                    check=False,
                    timeout=30,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("48 frames", result.stdout)
                self.check_refusals(source, output, width)

    def check_refusals(self, source: Path, output: Path, width: int) -> None:
        path = output / "manifest.json"
        manifest = json.loads(path.read_text())
        for key, value in (
            ("source_sha", "b" * 40),
            ("complete", False),
            ("frames", manifest["frames"][:-1]),
        ):
            path.write_text(json.dumps({**manifest, key: value}))
            with self.assertRaises(ValueError):
                ARCHIVE.validate(output, width, SOURCE, SCREENS)
        path.write_text(json.dumps(manifest))
        manifest["frames"][0]["actual_font_scale"] = 3
        path.write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, "Actual native font scale"):
            ARCHIVE.validate(output, width, SOURCE, SCREENS)
        with self.assertRaises(ValueError):
            ARCHIVE.unpack(
                source, output.parent / f"wrong-kind-{width}", width, "views"
            )


if __name__ == "__main__":
    main()
