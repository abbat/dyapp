"""Require source and font witnesses for native search recovery evidence."""

from itertools import product
import json
from pathlib import Path
import re
import tarfile
import tempfile
from unittest import TestCase, main

from test_android_visual_archive import ARCHIVE, PNG, SOURCE, add

ROOT = Path(__file__).resolve().parents[1]


class AndroidRecoveryVisualArchive(TestCase):
    def test_complete_native_inventory_rejects_missing_and_wrong_scale_frames(
        self,
    ) -> None:
        native = (
            ROOT
            / "clients/android/app/src/androidTest/kotlin/com/dyapp"
            / "SearchRecoveryVisualHost.kt"
        ).read_text()
        screens = set(re.findall(r'"(search-recovery-[a-z-]+)"', native))
        screens.remove("search-recovery-root")
        screens.remove("search-recovery-visual")
        self.assertEqual(screens, set(ARCHIVE.RECOVERY_SCREENS))
        self.assertEqual(len(screens), 7)
        with tempfile.TemporaryDirectory(dir=ROOT / "build") as temporary:
            root = Path(temporary)
            for width in (360, 412):
                self.check_matrix(root, width)

    def matrix(self, root: Path, width: int) -> Path:
        source = root / f"recovery-{width}.tar"
        frames = []
        prefix = f"files/search-recovery-visual/{width}"
        with tarfile.open(source, "w") as archive:
            for screen, language, theme, font, edge in product(
                ARCHIVE.RECOVERY_SCREENS,
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
            manifest = {
                "source_sha": SOURCE,
                "requested_width_dp": width,
                "complete": True,
                "frames": frames,
            }
            add(
                archive,
                f"{prefix}/manifest.json",
                json.dumps(manifest).encode(),
            )
        return source

    def check_matrix(self, root: Path, width: int) -> None:
        source = self.matrix(root, width)
        output = root / f"frames-{width}"
        ARCHIVE.unpack(source, output, width, "recovery")
        manifest = json.loads((output / "manifest.json").read_text())
        frames = manifest["frames"]
        self.assertEqual(
            ARCHIVE.validate(output, width, SOURCE, ARCHIVE.RECOVERY_SCREENS),
            112,
        )
        for key, value in (
            ("source_sha", "b" * 40),
            ("complete", False),
            ("frames", frames[:-1]),
        ):
            (output / "manifest.json").write_text(
                json.dumps({**manifest, key: value})
            )
            with self.assertRaises(ValueError):
                ARCHIVE.validate(
                    output, width, SOURCE, ARCHIVE.RECOVERY_SCREENS
                )
        frames[0]["actual_font_scale"] = 3
        (output / "manifest.json").write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, "Actual native font scale"):
            ARCHIVE.validate(output, width, SOURCE, ARCHIVE.RECOVERY_SCREENS)
        with self.assertRaises(ValueError):
            ARCHIVE.unpack(
                source, root / f"wrong-kind-{width}", width, "views"
            )


if __name__ == "__main__":
    main()
