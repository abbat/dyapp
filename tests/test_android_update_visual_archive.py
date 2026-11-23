"""Require source-bound update notices at every native visual configuration."""

import json
import re
import tarfile
import tempfile
from itertools import product
from pathlib import Path
from unittest import TestCase, main

from test_android_visual_archive import ARCHIVE, PNG, SOURCE, add

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_SCREENS = (
    "update-notice-package",
    "update-notice-source",
    "update-notice-saving",
    "update-notice-write-error",
    "update-notice-open-error",
    "update-notice-permission-allowed",
    "update-notice-permission-blocked",
    "update-notice-permission-pending",
    "update-notice-permission-failed",
    "update-notice-permission-unknown",
)


class AndroidUpdateVisualArchive(TestCase):
    def setUp(self) -> None:
        scratch = ROOT / "build/android-update-archive-tests"
        scratch.mkdir(parents=True, exist_ok=True)
        temporary = tempfile.TemporaryDirectory(dir=scratch)
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def matrix(self, width: int) -> Path:
        source = self.root / f"updates-{width}.tar"
        frames = []
        prefix = f"files/update-notice-visual/{width}"
        with tarfile.open(source, "w") as archive:
            for screen, language, theme, font, edge in product(
                EXPECTED_SCREENS,
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

    def test_both_widths_require_the_complete_native_state_inventory(
        self,
    ) -> None:
        native = (
            ROOT
            / "clients/android/app/src/androidTest/kotlin/com/dyapp"
            / "UpdateNoticeVisualHost.kt"
        ).read_text()
        screens = set(re.findall(r'"(update-notice-[a-z-]+)"', native))
        self.assertEqual(screens, set(EXPECTED_SCREENS))
        self.assertEqual(set(ARCHIVE.UPDATE_SCREENS), set(EXPECTED_SCREENS))
        for width in (360, 412):
            with self.subTest(width=width):
                source = self.matrix(width)
                output = self.root / f"frames-{width}"
                ARCHIVE.unpack(source, output, width, "updates")
                self.assertEqual(
                    ARCHIVE.validate(
                        output, width, SOURCE, ARCHIVE.UPDATE_SCREENS
                    ),
                    len(EXPECTED_SCREENS) * 2 * 2 * 2 * 2,
                )
                manifest = output / "manifest.json"
                original = json.loads(manifest.read_text())
                for key, value in (
                    ("source_sha", "b" * 40),
                    ("complete", False),
                    ("frames", original["frames"][:-1]),
                ):
                    manifest.write_text(json.dumps({**original, key: value}))
                    with self.assertRaises(ValueError):
                        ARCHIVE.validate(
                            output, width, SOURCE, ARCHIVE.UPDATE_SCREENS
                        )
                original["frames"][0]["actual_font_scale"] = 3
                manifest.write_text(json.dumps(original))
                with self.assertRaisesRegex(
                    ValueError, "Actual native font scale"
                ):
                    ARCHIVE.validate(
                        output, width, SOURCE, ARCHIVE.UPDATE_SCREENS
                    )

    def test_wrong_kind_width_and_unsafe_entries_cannot_be_extracted(
        self,
    ) -> None:
        source = self.matrix(360)
        for kind, width in (("views", 360), ("visual", 360), ("updates", 412)):
            with self.subTest(kind=kind, width=width):
                output = self.root / f"wrong-{kind}-{width}"
                with self.assertRaises(ValueError):
                    ARCHIVE.unpack(source, output, width, kind)
                self.assertFalse(output.exists())
        unsafe = self.root / "unsafe.tar"
        with tarfile.open(unsafe, "w") as archive:
            member = tarfile.TarInfo(
                "files/update-notice-visual/360/manifest.json"
            )
            member.type = tarfile.SYMTYPE
            member.linkname = "../outside"
            archive.addfile(member)
        with self.assertRaises(ValueError):
            ARCHIVE.unpack(unsafe, self.root / "unsafe", 360, "updates")
        self.assertFalse((self.root / "unsafe").exists())


if __name__ == "__main__":
    main()
