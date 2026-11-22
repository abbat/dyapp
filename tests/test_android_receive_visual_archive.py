"""Require complete, source-bound receiving evidence in its own namespace."""

from itertools import product
import json
from pathlib import Path
import re
import tarfile
import tempfile
from unittest import TestCase, main

from test_android_visual_archive import ARCHIVE, PNG, SOURCE, add

ROOT = Path(__file__).resolve().parents[1]


class AndroidReceiveVisualArchive(TestCase):
    def setUp(self) -> None:
        scratch = ROOT / "build/android-receive-archive-tests"
        scratch.mkdir(parents=True, exist_ok=True)
        temporary = tempfile.TemporaryDirectory(dir=scratch)
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def test_inventory_matches_native_and_prototype_states(self) -> None:
        native = (
            ROOT
            / "clients/android/app/src/androidTest/kotlin/com/dyapp"
            / "ReceiveVisualState.kt"
        ).read_text()
        screens = set(re.findall(r'"(link-[a-z-]+)"', native))
        prototype = (ROOT / "clients/android/prototype/index.html").read_text()
        match = re.search(r'data-on="(link-receive-intro [^"]+)"', prototype)
        assert match is not None
        self.assertEqual(screens, set(ARCHIVE.RECEIVE_SCREENS))
        self.assertEqual(screens, set(match[1].split()))
        self.assertEqual(len(screens), 25)

    def matrix(self) -> tuple[Path, dict[str, object]]:
        source = self.root / "receive.tar"
        frames = []
        with tarfile.open(source, "w") as archive:
            for screen, language, theme, font, edge in product(
                ARCHIVE.RECEIVE_SCREENS,
                ("en", "ru"),
                ("light", "dark"),
                (100, 200),
                ("top", "bottom"),
            ):
                name = f"{screen}-{language}-412-{theme}-{font}-{edge}.png"
                add(archive, f"files/receive-visual/412/{name}", PNG)
                frames.append(
                    {
                        "file": name,
                        "viewport_width_dp": 412,
                        "pixel_width": 1,
                        "pixel_height": 1,
                    }
                )
            manifest: dict[str, object] = {
                "source_sha": SOURCE,
                "requested_width_dp": 412,
                "complete": True,
                "frames": frames,
            }
            add(
                archive,
                "files/receive-visual/412/manifest.json",
                json.dumps(manifest).encode(),
            )
        return source, manifest

    def test_complete_receiving_matrix_has_four_hundred_frames(self) -> None:
        source, manifest = self.matrix()
        output = self.root / "frames"
        ARCHIVE.unpack(source, output, 412, "receive")
        self.assertEqual(
            ARCHIVE.validate(output, 412, SOURCE, ARCHIVE.RECEIVE_SCREENS),
            400,
        )
        for key, value in (
            ("source_sha", "b" * 40),
            ("complete", False),
            ("frames", []),
        ):
            changed = {**manifest, key: value}
            (output / "manifest.json").write_text(json.dumps(changed))
            with self.assertRaises(ValueError):
                ARCHIVE.validate(output, 412, SOURCE, ARCHIVE.RECEIVE_SCREENS)

    def test_namespaces_widths_and_links_cannot_be_mixed(self) -> None:
        source, _ = self.matrix()
        for kind, width in (("visual", 412), ("receive", 360)):
            with self.subTest(kind=kind, width=width):
                with self.assertRaises(ValueError):
                    ARCHIVE.unpack(source, self.root / kind, width, kind)
                self.assertFalse((self.root / kind).exists())
        link = self.root / "link.tar"
        with tarfile.open(link, "w") as archive:
            member = tarfile.TarInfo("files/receive-visual/412/manifest.json")
            member.type = tarfile.LNKTYPE
            member.linkname = "../outside"
            archive.addfile(member)
        with self.assertRaises(ValueError):
            ARCHIVE.unpack(link, self.root / "links", 412, "receive")
        self.assertFalse((self.root / "links").exists())


if __name__ == "__main__":
    main()
