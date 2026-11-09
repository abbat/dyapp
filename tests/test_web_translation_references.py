"""Exercise real-parser translation gates with positive and negative inputs."""
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]


class WebTranslationReferences(unittest.TestCase):
    def test_real_parser_and_gate_fixtures(self) -> None:
        result = subprocess.run(
            ["node", str(ROOT / "tools/web-lint/translation-fixtures.mjs")],
            capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("translation parser fixtures", result.stdout)


if __name__ == "__main__":
    unittest.main()
