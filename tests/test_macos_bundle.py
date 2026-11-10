"""Exercise macOS application assembly without native compilation."""

import importlib.util
import json
from pathlib import Path
import plistlib
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "macos_bundle", ROOT / "tools/scripts/build-macos-client.py"
)
assert SPEC and SPEC.loader
BUILD = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = BUILD
SPEC.loader.exec_module(BUILD)


class MacBundleTests(unittest.TestCase):
    def test_bundle_contains_executable_identity_and_camera_locales(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "binary"
            binary.write_bytes(b"native fixture")
            binary.chmod(0o755)
            destination = root / "DYApp.app"
            config = BUILD.settings()
            BUILD.bundle(binary, destination, config)
            contents = destination / "Contents"
            metadata = plistlib.loads((contents / "Info.plist").read_bytes())
            self.assertEqual(metadata["CFBundleIdentifier"], config.identifier)
            self.assertEqual(metadata["LSMinimumSystemVersion"], "12.0")
            self.assertEqual(metadata["CFBundleExecutable"], "dyapp-macos")
            executable = contents / "MacOS/dyapp-macos"
            self.assertEqual(executable.read_bytes(), binary.read_bytes())
            self.assertTrue(executable.stat().st_mode & 0o111)
            self.assertTrue((contents / "Resources/icon.icns").is_file())
            for locale in ("en", "ru"):
                expected = json.dumps(
                    BUILD.camera_description(locale), ensure_ascii=False
                )
                strings = (
                    contents / f"Resources/{locale}.lproj/InfoPlist.strings"
                )
                self.assertIn(expected, strings.read_text())
            with self.assertRaisesRegex(ValueError, "output exists"):
                BUILD.bundle(binary, destination, config)

    def test_build_rejects_incomplete_universal_binary(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with (
                patch.object(BUILD.platform, "system", return_value="Darwin"),
                patch.object(
                    BUILD, "compile_app", return_value=root / "binary"
                ),
                patch.object(
                    BUILD.subprocess, "check_output", return_value="arm64"
                ),
            ):
                with self.assertRaisesRegex(ValueError, "architectures"):
                    BUILD.build("release", True, "", root / "DYApp.app", root)
            self.assertFalse((root / "DYApp.app").exists())


if __name__ == "__main__":
    unittest.main()
