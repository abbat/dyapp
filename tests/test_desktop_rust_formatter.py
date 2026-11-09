"""Check included and unreferenced desktop Rust with the real formatter."""

from pathlib import Path
import shutil
import subprocess
import sys
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]


class DesktopRustFormatterTest(unittest.TestCase):
    def fixture(self):
        root = ROOT / "build/tool-fixtures/desktop-rustfmt"
        root = root / str(time.time_ns())
        script = root / "tests/scripts/desktop-rust-format.py"
        script.parent.mkdir(parents=True)
        shutil.copyfile(ROOT / "tests/scripts/desktop-rust-format.py", script)
        return root, script

    def run_formatter(self, script, mode):
        return subprocess.run(
            [sys.executable, "-B", str(script), mode],
            capture_output=True,
            text=True,
            check=False,
        )

    def test_included_and_unreferenced_files_cannot_escape_formatting(
        self,
    ) -> None:
        root, script = self.fixture()
        source = root / "clients/desktop"
        source.mkdir(parents=True)
        (source / "main.rs").write_text('include!("included.rs");\n')
        (source / "included.rs").write_text("fn included( ){ }\n")
        (source / "orphan.rs").write_text("fn orphan( ){ }\n")
        direct = subprocess.run(
            [
                "rustfmt",
                "--edition",
                "2021",
                "--check",
                str(source / "main.rs"),
            ],
            capture_output=True,
            check=False,
        )
        self.assertEqual(direct.returncode, 0)
        self.assertNotEqual(self.run_formatter(script, "check").returncode, 0)
        written = self.run_formatter(script, "write")
        self.assertEqual(written.returncode, 0, written.stderr)
        checked = self.run_formatter(script, "check")
        self.assertEqual(
            checked.returncode, 0, checked.stdout + checked.stderr
        )
        self.assertEqual(
            (source / "included.rs").read_text(), "fn included() {}\n"
        )
        self.assertEqual(
            (source / "orphan.rs").read_text(), "fn orphan() {}\n"
        )

    def test_empty_source_inventory_fails_closed(self) -> None:
        _, script = self.fixture()
        result = self.run_formatter(script, "check")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("No shared desktop Rust sources", result.stderr)


if __name__ == "__main__":
    unittest.main()
