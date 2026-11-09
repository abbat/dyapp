"""Run actual window geometry tests without linking Tauri."""

import json
from pathlib import Path
import re
import subprocess
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]


class WindowGeometryTest(unittest.TestCase):
    def test_production_geometry_at_integer_boundaries(self):
        directory = ROOT / "build/tool-fixtures/window-geometry"
        directory = directory / str(time.time_ns())
        directory.mkdir(parents=True)
        source = (ROOT / "clients/desktop/window.rs").read_text()
        rectangle = re.search(
            r"pub struct Rect \{.*?^\}", source, flags=re.DOTALL | re.MULTILINE
        )
        self.assertIsNotNone(rectangle)
        module = ROOT / "clients/desktop/window/geometry.rs"
        harness = directory / "harness.rs"
        header = "#[derive(Clone, Copy, Debug, PartialEq)]\n"
        module_path = json.dumps(module.as_posix())
        program = header + rectangle.group(0)
        program += "\n#[path = " + module_path + "]\nmod geometry;\n"
        harness.write_text(program)
        binary = directory / "geometry-tests"
        compiled = subprocess.run(
            [
                "rustc",
                "--edition",
                "2021",
                "--test",
                "-D",
                "warnings",
                str(harness),
                "-o",
                str(binary),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            compiled.returncode, 0, compiled.stdout + compiled.stderr
        )
        result = subprocess.run(
            [str(binary), "--quiet"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
