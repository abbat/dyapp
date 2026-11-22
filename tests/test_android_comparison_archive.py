"""Require native comparison value lines between the saved edge frames."""

import importlib.util
import json
import re
import tarfile
import tempfile
from copy import deepcopy
from itertools import product
from pathlib import Path
from typing import Any
from unittest import TestCase, main
from unittest.mock import patch

from test_android_visual_archive import ARCHIVE as VISUAL
from test_android_visual_archive import PNG, SOURCE, add

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "android_comparison_archive",
    ROOT / "tests/scripts/android_comparison_archive.py",
)
assert SPEC is not None and SPEC.loader is not None
ARCHIVE = importlib.util.module_from_spec(SPEC)
with patch.dict("sys.modules", {"android_visual_archive": VISUAL}):
    SPEC.loader.exec_module(ARCHIVE)


class AndroidComparisonArchive(TestCase):
    def setUp(self) -> None:
        scratch = ROOT / "build/android-comparison-archive-tests"
        scratch.mkdir(parents=True, exist_ok=True)
        temporary = tempfile.TemporaryDirectory(dir=scratch)
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def matrix(self) -> tuple[Path, dict[str, Any]]:
        source = self.root / "comparison.tar"
        manifests = {}
        with tarfile.open(source, "w") as archive:
            for language in ("en", "ru"):
                scenes = []
                for state, theme, font in product(
                    ARCHIVE.STATES, ("light", "dark"), (100, 200)
                ):
                    name = f"{state}-{language}-360-{theme}-{font}"
                    sources = (
                        ("REMOTE",)
                        if state == "preview"
                        else ("LOCAL", "REMOTE")
                    )
                    required = {
                        f"link-review-{field}-{side}-value": [0, 1, 2]
                        for field, side in product(ARCHIVE.FIELDS, sources)
                    }
                    frames = []
                    for index in range(3):
                        file = f"{name}-{index}.png"
                        side = sources[min(index, len(sources) - 1)]
                        frames.append(
                            {
                                "file": file,
                                "visible": {tag: [index] for tag in required},
                                "font_percent": font,
                                "actual_font_scale": font / 100,
                                "actual_density": 1,
                                "viewport_width_dp": 360,
                                "pixel_width": 1,
                                "pixel_height": 1,
                                "photo": f"link-review-photo-{side}-0"
                                + (
                                    "-retry"
                                    if state == "photo_unavailable"
                                    else ""
                                ),
                            }
                        )
                        add(
                            archive,
                            f"files/comparison-visual/360/{language}/{file}",
                            PNG,
                        )
                    scenes.append(
                        {
                            "scene": name,
                            "complete": True,
                            "required": required,
                            "frames": frames,
                        }
                    )
                manifest = {
                    "source_sha": SOURCE,
                    "requested_width_dp": 360,
                    "language": language,
                    "complete": True,
                    "scenes": scenes,
                }
                manifests[language] = manifest
                add(
                    archive,
                    f"files/comparison-visual/360/{language}/manifest.json",
                    json.dumps(manifest).encode(),
                )
        return source, manifests

    def test_complete_native_matrix_and_shared_field_inventory(self) -> None:
        source, _ = self.matrix()
        output = self.root / "frames"
        ARCHIVE.unpack(source, output, 360)
        self.assertEqual(ARCHIVE.validate(output, 360, SOURCE), 120)
        native = (ROOT / "rust/ffi/src/link_review/fields.rs").read_text()
        match = re.search(r"fields!\s*\{([^}]+)\}", native)
        assert match is not None
        fields = {
            re.sub(r"(?<!^)(?=[A-Z])", "_", name).upper()
            for name in re.findall(r"[A-Z][a-zA-Z]+", match[1])
        }
        self.assertEqual(set(ARCHIVE.FIELDS), fields)
        self.assertEqual(len(fields), 31)

    def test_edge_frames_do_not_prove_middle_lines_or_missing_fields(
        self,
    ) -> None:
        source, manifests = self.matrix()
        output = self.root / "frames"
        ARCHIVE.unpack(source, output, 360)
        for mutation in (
            "middle",
            "field",
            "scale",
            "source",
            "scene",
            "photo",
        ):
            changed = deepcopy(manifests["en"])
            scene = changed["scenes"][0]
            if mutation == "middle":
                scene["frames"][1]["visible"] = {}
            elif mutation == "field":
                scene["required"].pop(next(iter(scene["required"])))
            elif mutation == "scale":
                changed["scenes"][1]["frames"][0]["actual_font_scale"] = 1
            elif mutation == "photo":
                for entry in scene["frames"]:
                    entry["photo"] = None
            elif mutation == "source":
                changed["source_sha"] = "b" * 40
            else:
                changed["scenes"].pop()
            (output / "en/manifest.json").write_text(json.dumps(changed))
            with (
                self.subTest(mutation=mutation),
                self.assertRaises(ValueError),
            ):
                ARCHIVE.validate(output, 360, SOURCE)

    def test_archive_paths_links_width_and_duplicate_members_are_rejected(
        self,
    ) -> None:
        source, _ = self.matrix()
        with self.assertRaises(ValueError):
            ARCHIVE.unpack(source, self.root / "wrong-width", 412)
        for index, (name, kind, duplicate) in enumerate(
            (
                ("../escape.png", tarfile.REGTYPE, False),
                (
                    "files/comparison-visual/360/en/manifest.json",
                    tarfile.SYMTYPE,
                    False,
                ),
                (
                    "files/comparison-visual/360/en/manifest.json",
                    tarfile.REGTYPE,
                    True,
                ),
                (
                    "files/receive-visual/360/manifest.json",
                    tarfile.REGTYPE,
                    False,
                ),
            )
        ):
            path = self.root / f"invalid-{index}.tar"
            with tarfile.open(path, "w") as archive:
                if kind == tarfile.SYMTYPE:
                    member = tarfile.TarInfo(name)
                    member.type = kind
                    member.linkname = "/outside"
                    archive.addfile(member)
                else:
                    add(archive, name, b"{}")
                    if duplicate:
                        add(archive, name, b"{}")
            with (
                self.subTest(name=name, duplicate=duplicate),
                self.assertRaises(ValueError),
            ):
                ARCHIVE.unpack(path, self.root / f"invalid-{index}", 360)
            self.assertFalse((self.root / f"invalid-{index}").exists())


if __name__ == "__main__":
    main()
