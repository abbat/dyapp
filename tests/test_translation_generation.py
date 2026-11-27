"""Exercise the real generator CLI, including read-only and refusal paths."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from test_translation_sources import ROOT, prepare, write

SCRIPT = ROOT / "tools/scripts/generate-translations.py"
DESKTOP = Path("clients/desktop/ui/i18n/ru.json")


def invoke(root: Path, *flags: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, "-B", str(SCRIPT), "--root", str(root), *flags],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )


class TranslationGeneration(unittest.TestCase):
    def test_generation_drift_and_platform_filtering_use_real_files(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            prepare(root)
            absent = invoke(root, "--check")
            self.assertEqual(absent.returncode, 1, absent.stderr)
            self.assertFalse((root / "clients").exists())
            created = invoke(root)
            self.assertEqual(created.returncode, 0, created.stderr)
            self.assertEqual(invoke(root, "--check").returncode, 0)
            target = root / DESKTOP
            target.write_bytes(b"{}\n")
            failed = invoke(root, "--check", "--platform", "desktop")
            self.assertEqual(failed.returncode, 1)
            self.assertIn(DESKTOP.as_posix(), failed.stderr)
            self.assertEqual(target.read_bytes(), b"{}\n")
            fixed = invoke(root, "--platform", "desktop")
            self.assertEqual(fixed.returncode, 0, fixed.stderr)
            self.assertEqual(
                json.loads(target.read_bytes()), {"total": "{number} entries"}
            )
            self.assertEqual(invoke(root, "--check").returncode, 0)

    def test_invalid_sources_and_late_destination_errors_create_no_outputs(
        self,
    ) -> None:
        for defect in ("source", "destination"):
            with (
                self.subTest(defect=defect),
                tempfile.TemporaryDirectory() as folder,
            ):
                root = Path(folder)
                directory = prepare(root)
                if defect == "source":
                    write(
                        directory / "ru.json",
                        {"common.total": "Lost argument"},
                    )
                else:
                    blocked = root / "clients/ios/DYApp/Localizable.xcstrings"
                    blocked.mkdir(parents=True)
                result = invoke(root)
                self.assertEqual(result.returncode, 2)
                self.assertFalse((root / "clients/android").exists())
                self.assertFalse((root / "clients/desktop").exists())

    def test_output_symlink_cannot_write_outside_the_selected_project(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as folder:
            outer = Path(folder)
            root = outer / "project"
            root.mkdir()
            prepare(root)
            outside = outer / "outside"
            outside.mkdir()
            marker = outside / "keep"
            marker.write_bytes(b"unchanged")
            (root / "clients").symlink_to(outside, target_is_directory=True)
            result = invoke(root)
            self.assertEqual(result.returncode, 2)
            self.assertIn("symlink", result.stderr)
            self.assertEqual(list(outside.iterdir()), [marker])
            self.assertEqual(marker.read_bytes(), b"unchanged")

    def test_unmanaged_android_resources_are_refused_without_deletion(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            prepare(root)
            legacy = root / "clients/android/app/src/main/res/values/old.xml"
            legacy.parent.mkdir(parents=True)
            data = (
                b'<resources><string name="old">Retained</string></resources>'
            )
            legacy.write_bytes(data)
            result = invoke(root, "--platform", "android")
            self.assertEqual(result.returncode, 2)
            self.assertIn("Unmanaged", result.stderr)
            self.assertEqual(legacy.read_bytes(), data)
            self.assertFalse((legacy.parent / "strings.xml").exists())
            desktop = invoke(root, "--platform", "desktop")
            self.assertEqual(desktop.returncode, 0, desktop.stderr)
            self.assertEqual(legacy.read_bytes(), data)


if __name__ == "__main__":
    unittest.main()
