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
            frames[0]["pixel_width"] = 2
            (output / "manifest.json").write_text(json.dumps(manifest))
            with self.assertRaises(ValueError):
                ARCHIVE.validate(output, 360, SOURCE)
            frames[0]["pixel_width"] = 1
            manifest["complete"] = False
            (output / "manifest.json").write_text(json.dumps(manifest))
            with self.assertRaises(ValueError):
                ARCHIVE.validate(output, 360, SOURCE)


if __name__ == "__main__":
    main()
