"""Reject path tricks and incomplete native rendering evidence."""

import base64
import importlib.util
import io
from itertools import product
import json
from pathlib import Path
import tarfile
import tempfile
from unittest import TestCase, main

SPEC = importlib.util.spec_from_file_location(
    "android_visual_archive",
    Path(__file__).parent / "scripts/android_visual_archive.py",
)
assert SPEC is not None and SPEC.loader is not None
ARCHIVE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ARCHIVE)
SOURCE = "a" * 40
PNG = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk"
    "+A8AAQUBAScY42YAAAAASUVORK5CYII="
)


def add(archive: tarfile.TarFile, name: str, content: bytes) -> None:
    member = tarfile.TarInfo(name)
    member.size = len(content)
    archive.addfile(member, io.BytesIO(content))


class AndroidVisualArchive(TestCase):
    def test_rejects_unexpected_paths_links_and_duplicate_entries(
        self,
    ) -> None:
        for name, kind, duplicate in (
            ("../escape.png", tarfile.REGTYPE, False),
            ("/absolute.png", tarfile.REGTYPE, False),
            (
                "files/owner-approval-visual/360/manifest.json",
                tarfile.SYMTYPE,
                False,
            ),
            (
                "files/owner-approval-visual/360/manifest.json",
                tarfile.REGTYPE,
                True,
            ),
            (
                "files/owner-approval-visual/412/manifest.json",
                tarfile.REGTYPE,
                False,
            ),
        ):
            with (
                self.subTest(name=name, kind=kind),
                tempfile.TemporaryDirectory() as temporary,
            ):
                root = Path(temporary)
                source = root / "native.tar"
                with tarfile.open(source, "w") as archive:
                    member = tarfile.TarInfo(name)
                    member.type = kind
                    member.linkname = "/outside"
                    member.size = 1 if kind == tarfile.REGTYPE else 0
                    archive.addfile(member, io.BytesIO(b"x"))
                    if duplicate:
                        archive.addfile(member, io.BytesIO(b"x"))
                with self.assertRaises(ValueError):
                    ARCHIVE.unpack(source, root / "output", 360)
                self.assertFalse((root / "output").exists())

    def test_complete_matrix_and_source_dimensions_are_required(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "native.tar"
            frames = []
            with tarfile.open(source, "w") as archive:
                for screen, language, theme, font, edge in product(
                    ARCHIVE.SCREENS,
                    ("en", "ru"),
                    ("light", "dark"),
                    (100, 200),
                    ("top", "bottom"),
                ):
                    name = f"{screen}-{language}-360-{theme}-{font}-{edge}.png"
                    add(
                        archive, f"files/owner-approval-visual/360/{name}", PNG
                    )
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
                manifest = {
                    "source_sha": SOURCE,
                    "requested_width_dp": 360,
                    "complete": True,
                    "frames": frames,
                }
                add(
                    archive,
                    "files/owner-approval-visual/360/manifest.json",
                    json.dumps(manifest).encode(),
                )
            output = root / "output"
            ARCHIVE.unpack(source, output, 360)
            self.assertEqual(ARCHIVE.validate(output, 360, SOURCE), 192)
            with self.assertRaises(ValueError):
                ARCHIVE.validate(output, 360, "b" * 40)
            for key, value in (
                ("actual_font_scale", 2),
                ("actual_font_scale", True),
                ("actual_font_scale", None),
                ("actual_density", float("nan")),
                ("actual_density", 0),
                ("font_percent", 200),
            ):
                saved = frames[0][key]
                frames[0][key] = value
                (output / "manifest.json").write_text(json.dumps(manifest))
                with self.assertRaises(ValueError):
                    ARCHIVE.validate(output, 360, SOURCE)
                frames[0][key] = saved
            frames[0]["pixel_width"] = 2
            (output / "manifest.json").write_text(json.dumps(manifest))
            with self.assertRaises(ValueError):
                ARCHIVE.validate(output, 360, SOURCE)
            frames[0]["pixel_width"] = 1
            manifest["complete"] = False
            (output / "manifest.json").write_text(json.dumps(manifest))
            with self.assertRaises(ValueError):
                ARCHIVE.validate(output, 360, SOURCE)

    def test_activity_matrix_requires_every_mark_and_audience_outcome(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory(
            dir=Path(__file__).resolve().parents[1] / "build"
        ) as temporary:
            root = Path(temporary)
            source = root / "activity.tar"
            frames: list[dict[str, object]] = []
            with tarfile.open(source, "w") as archive:
                for screen, language, theme, font, edge in product(
                    ARCHIVE.ACTIVITY_SCREENS,
                    ("en", "ru"),
                    ("light", "dark"),
                    (100, 200),
                    ("top", "bottom"),
                ):
                    name = f"{screen}-{language}-360-{theme}-{font}-{edge}.png"
                    add(archive, f"files/activity-visual/360/{name}", PNG)
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
                manifest = {
                    "source_sha": SOURCE,
                    "requested_width_dp": 360,
                    "complete": True,
                    "frames": frames,
                }
                add(
                    archive,
                    "files/activity-visual/360/manifest.json",
                    json.dumps(manifest).encode(),
                )
            output = root / "output"
            ARCHIVE.unpack(source, output, 360, "activity")
            self.assertEqual(
                ARCHIVE.validate(
                    output, 360, SOURCE, ARCHIVE.ACTIVITY_SCREENS
                ),
                128,
            )
            frames.pop()
            (output / "manifest.json").write_text(json.dumps(manifest))
            with self.assertRaises(ValueError):
                ARCHIVE.validate(output, 360, SOURCE, ARCHIVE.ACTIVITY_SCREENS)

    def test_real_ime_matrix_rejects_hidden_or_invalid_geometry(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "ime.tar"
            frames: list[dict[str, object]] = []
            with tarfile.open(source, "w") as archive:
                folder = tarfile.TarInfo("files/owner-approval-ime")
                folder.type = tarfile.DIRTYPE
                archive.addfile(folder)
                for language, theme, font, index in product(
                    ("en", "ru"), ("light", "dark"), (100, 200), (0, 4, 8)
                ):
                    name = (
                        f"owner-ime-{language}-360-{theme}-{font}-{index}.png"
                    )
                    add(archive, f"files/owner-approval-ime/360/{name}", PNG)
                    frames.append(
                        {
                            "file": name,
                            "selection": index,
                            "font_percent": font,
                            "actual_font_scale": font / 100,
                            "actual_density": 1,
                            "visible_screen_px": [0, 0, 360, 500],
                            "glyph_screen_px": [10, 30, 20, 45],
                            "cancel_screen_px": [10, 450, 90, 490],
                            "next_screen_px": [100, 450, 220, 490],
                        }
                    )
                manifest = {
                    "source_sha": SOURCE,
                    "width_dp": 360,
                    "code_length": 8,
                    "complete": True,
                    "frames": frames,
                }
                add(
                    archive,
                    "files/owner-approval-ime/360/manifest.json",
                    json.dumps(manifest).encode(),
                )
            output = root / "output"
            ARCHIVE.unpack(source, output, 360, "ime")
            self.assertEqual(ARCHIVE.validate_ime(output, 360, SOURCE), 24)
            frames[0]["actual_font_scale"] = 2
            (output / "manifest.json").write_text(json.dumps(manifest))
            with self.assertRaises(ValueError):
                ARCHIVE.validate_ime(output, 360, SOURCE)
            frames[0]["actual_font_scale"] = 1
            for geometry in (
                [10, 501, 20, 510],
                [0, 0, 0, 0],
                [0, 0, float("nan"), 20],
            ):
                frames[0]["glyph_screen_px"] = geometry
                (output / "manifest.json").write_text(json.dumps(manifest))
                with self.assertRaises(ValueError):
                    ARCHIVE.validate_ime(output, 360, SOURCE)
            frames[0]["glyph_screen_px"] = [10, 30, 20, 45]
            frames.append(frames[0])
            (output / "manifest.json").write_text(json.dumps(manifest))
            with self.assertRaises(ValueError):
                ARCHIVE.validate_ime(output, 360, SOURCE)


if __name__ == "__main__":
    main()
