"""The desktop UI inventory lists known controls and is deterministic."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tools/scripts/ui-inventory.py"


def run():
    return subprocess.run([sys.executable, "-B", str(SCRIPT)], check=True,
                          capture_output=True, text=True).stdout


class Inventory(unittest.TestCase):
    def test_lists_controls_and_is_stable(self):
        out = run()
        self.assertEqual(out, run())
        self.assertIn("| create-identity | button | setup.create | "
                      "Create identity |", out)
        self.assertIn("| check-connection | button |", out)
        self.assertIn("## #settings", out)

    def test_condition_heuristic(self):
        spec = importlib.util.spec_from_file_location("ui_inventory",
                                                      SCRIPT)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        js = "const a = document.getElementById('x-y');\na.hidden = true;\n"
        self.assertEqual(module.conditions("a.js", js),
                         {"x-y": [("a.js:2", "a.hidden = true;")]})


if __name__ == "__main__":
    unittest.main()
