"""The native network fixture has the same wire bytes on every platform."""

import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


class DesktopNetworkFixtureTests(unittest.TestCase):
    def test_mixed_setup_ignores_windows_text_newline_translation(
        self,
    ) -> None:
        spec = importlib.util.spec_from_file_location(
            "desktop_fixture", ROOT / "tests/scripts/desktop-smoke.py"
        )
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        driver = object.__new__(module.DesktopSmoke)
        write_text = Path.write_text

        def windows_text(path: Path, text: str) -> int:
            return write_text(path, text, newline="\r\n")

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with (
                patch.object(Path, "write_text", windows_text),
                patch.object(
                    driver,
                    "case",
                    side_effect=RuntimeError("native launch intercepted"),
                ),
                self.assertRaisesRegex(
                    RuntimeError, "native launch intercepted"
                ),
            ):
                driver.mixed(root, "first")
            self.assertEqual(
                (root / "node-config/network").read_bytes(),
                b"dyapp-network 1\nname \n",
            )


if __name__ == "__main__":
    unittest.main()
