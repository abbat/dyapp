"""Require the complete source-bound received-view matrix."""

from itertools import product
import json
from pathlib import Path
import re
import tarfile
import tempfile
from unittest import TestCase, main

from test_android_visual_archive import ARCHIVE, PNG, SOURCE, add

ROOT = Path(__file__).resolve().parents[1]


class AndroidViewsVisualArchive(TestCase):
    def setUp(self) -> None:
        scratch = ROOT / "build/android-views-archive-tests"
        scratch.mkdir(parents=True, exist_ok=True)
        temporary = tempfile.TemporaryDirectory(dir=scratch)
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def test_native_and_prototype_inventory_agree(self) -> None:
        native = (
            ROOT
            / "clients/android/app/src/androidTest/kotlin/com/dyapp"
            / "ReceivedViewsVisualState.kt"
        ).read_text()
        screens = set(re.findall(r'"(settings-views[a-z-]*)"', native))
        prototype = (ROOT / "clients/android/prototype/index.html").read_text()
        self.assertEqual(screens, set(ARCHIVE.VIEW_SCREENS))
        self.assertEqual(len(screens), 5)
        match = re.search(
            r'<section id="settings-views"[^>]*data-on="([^"]+)"', prototype
        )
        assert match is not None
        self.assertEqual(screens, set(match[1].split()))

    def matrix(self) -> tuple[Path, dict[str, object]]:
        source = self.root / "views.tar"
        frames = []
        with tarfile.open(source, "w") as archive:
            for screen, language, theme, font, edge in product(
                ARCHIVE.VIEW_SCREENS,
                ("en", "ru"),
                ("light", "dark"),
                (100, 200),
                ("top", "bottom"),
            ):
                name = f"{screen}-{language}-360-{theme}-{font}-{edge}.png"
                add(archive, f"files/received-views-visual/360/{name}", PNG)
                frames.append(
                    {
                        "file": name,
                        "viewport_width_dp": 360,
                        "pixel_width": 1,
                        "pixel_height": 1,
                        "font_percent": font,
                        "actual_font_scale": font / 100,
                        "actual_density": 1,
                    }
                )
            manifest: dict[str, object] = {
                "source_sha": SOURCE,
                "requested_width_dp": 360,
                "complete": True,
                "frames": frames,
            }
            add(
                archive,
                "files/received-views-visual/360/manifest.json",
                json.dumps(manifest).encode(),
            )
        return source, manifest

    def test_complete_matrix_requires_source_inventory_and_scale(self) -> None:
        source, manifest = self.matrix()
        output = self.root / "frames"
        ARCHIVE.unpack(source, output, 360, "views")
        self.assertEqual(
            ARCHIVE.validate(output, 360, SOURCE, ARCHIVE.VIEW_SCREENS), 80
        )
        for key, value in (
            ("source_sha", "b" * 40),
            ("complete", False),
            ("frames", []),
        ):
            with self.subTest(key=key):
                (output / "manifest.json").write_text(
                    json.dumps({**manifest, key: value})
                )
                with self.assertRaises(ValueError):
                    ARCHIVE.validate(output, 360, SOURCE, ARCHIVE.VIEW_SCREENS)
        (output / "manifest.json").write_text(json.dumps(manifest))
        doubled = next(output.glob("*-200-*.png"))
        metadata = json.loads((output / "manifest.json").read_text())
        for frame in metadata["frames"]:
            if frame["file"] == doubled.name:
                frame["actual_font_scale"] = 1
        (output / "manifest.json").write_text(json.dumps(metadata))
        with self.assertRaisesRegex(ValueError, "Actual native font scale"):
            ARCHIVE.validate(output, 360, SOURCE, ARCHIVE.VIEW_SCREENS)

    def test_wrong_kind_width_paths_and_links_are_rejected_before_extraction(
        self,
    ) -> None:
        source, _ = self.matrix()
        for kind, width in (("visual", 360), ("receive", 360), ("views", 412)):
            with self.subTest(kind=kind, width=width):
                target = self.root / f"{kind}-{width}"
                with self.assertRaises(ValueError):
                    ARCHIVE.unpack(source, target, width, kind)
                self.assertFalse(target.exists())
        for index, link_type in enumerate((tarfile.SYMTYPE, tarfile.LNKTYPE)):
            unsafe = self.root / f"link-{index}.tar"
            with tarfile.open(unsafe, "w") as archive:
                member = tarfile.TarInfo(
                    "files/received-views-visual/360/manifest.json"
                )
                member.type = link_type
                member.linkname = "../outside"
                archive.addfile(member)
            with self.assertRaises(ValueError):
                ARCHIVE.unpack(
                    unsafe, self.root / f"links-{index}", 360, "views"
                )
        unsafe = self.root / "path.tar"
        with tarfile.open(unsafe, "w") as archive:
            add(
                archive,
                "files/received-views-visual/360/../../outside",
                b"invalid",
            )
        with self.assertRaises(ValueError):
            ARCHIVE.unpack(unsafe, self.root / "paths", 360, "views")
        self.assertFalse((self.root / "paths").exists())


if __name__ == "__main__":
    main()
