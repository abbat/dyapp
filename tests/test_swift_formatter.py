"""The quality image's Swift formatter must reject layout drift."""

from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class SwiftFormatterTest(unittest.TestCase):
    def test_strict_lint_rejects_unformatted_source(self):
        for source, expected in (("let value=1\n", 1), ("let value = 1\n", 0)):
            with self.subTest(source=source):
                with tempfile.TemporaryDirectory(dir=ROOT / "build") as tmp:
                    fixture = Path(tmp) / "Fixture.swift"
                    fixture.write_text(source)
                    result = subprocess.run(
                        [
                            "bash",
                            "tests/scripts/swift-format.sh",
                            "lint",
                            str(fixture.relative_to(ROOT)),
                        ],
                        cwd=ROOT,
                        capture_output=True,
                        text=True,
                        check=False,
                    )
                self.assertEqual(
                    result.returncode, expected, result.stdout + result.stderr
                )


if __name__ == "__main__":
    unittest.main()
