"""The embedded-UI gate rejects plain UI bytes and accepts compressed ones."""

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zlib

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tests/scripts/check-embedded-ui.py"


def run(binary: bytes, ui: Path) -> subprocess.CompletedProcess[str]:
    with tempfile.NamedTemporaryFile() as file:
        file.write(binary)
        file.flush()
        return subprocess.run(
            [sys.executable, "-B", str(SCRIPT), file.name, str(ui)],
            capture_output=True,
            text=True,
            check=False,
        )


class EmbeddedUi(unittest.TestCase):
    def test_plain_ui_fails_and_compressed_ui_passes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            ui = Path(directory)
            (ui / "i18n").mkdir()
            app = "".join(f"const value{i} = {i};\n" for i in range(200))
            (ui / "app.js").write_text(app)
            (ui / "i18n/en.json").write_text('{"k": "v"}')
            body = b"ELF" + app.encode() + b"tail"
            plain = run(body, ui)
            self.assertEqual(plain.returncode, 1, plain.stdout)
            self.assertIn("app.js", plain.stdout)
            packed = run(b"ELF" + zlib.compress(app.encode(), 9), ui)
            self.assertEqual(packed.returncode, 0, packed.stdout)
            self.assertIn("1 files checked", packed.stdout)

    def test_a_ui_dir_without_large_files_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            result = run(b"ELF", Path(directory))
            self.assertEqual(result.returncode, 1)
            self.assertIn("no UI files", result.stdout)


if __name__ == "__main__":
    unittest.main()
