"""The real Python lint gate rejects functions above complexity ten."""

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class PythonComplexity(unittest.TestCase):
    def test_configured_limit_accepts_ten_and_rejects_eleven(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fixture.py"
            for branches, expected in ((9, 0), (10, 1)):
                with self.subTest(branches=branches):
                    code = "def branch(value):\n"
                    for index in range(branches):
                        code += (
                            f"    if value == {index}:\n"
                            f"        return {index}\n"
                        )
                    path.write_text(code + "    return -1\n")
                    result = subprocess.run(
                        [
                            sys.executable,
                            "-m",
                            "flake8",
                            "--config",
                            str(ROOT / ".flake8"),
                            "--select",
                            "C901",
                            str(path),
                        ],
                        capture_output=True,
                        text=True,
                        check=False,
                    )
                    self.assertEqual(
                        result.returncode,
                        expected,
                        result.stdout + result.stderr,
                    )
                    if expected:
                        self.assertIn("C901", result.stdout)
                        self.assertIn("(11)", result.stdout)


if __name__ == "__main__":
    unittest.main()
