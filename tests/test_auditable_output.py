"""Exercise the standalone release checker with real ELF sections."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zlib

ROOT = Path(__file__).resolve().parents[1]


class Auditable(unittest.TestCase):
    def test_standalone_elf_inventory_and_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = root / "check.py"
            script.write_bytes(
                (ROOT / "tools/scripts/check-auditable.py").read_bytes())
            lock = root / "packages.toml"
            lock.write_text(
                '[[package]]\nname="dyapp-bootstrap"\nversion="0.0.1"\n')
            env = dict(os.environ, DYAPP_CHECK_LOG_DIR=str(root / "logs"))

            def run(binary, raw=False):
                return subprocess.run(
                    [sys.executable, str(script),
                     *(["--raw"] if raw else []), str(binary), str(lock)],
                    capture_output=True, text=True, env=env, check=False)

            def binary(name, packages):
                section = root / "section"
                section.write_bytes(zlib.compress(json.dumps(
                    {"packages": packages}).encode()))
                result = root / name
                subprocess.run(
                    ["objcopy", "--add-section", f".dep-v0={section}",
                     "/bin/true", str(result)], check=True,
                    capture_output=True)
                return result

            package = {"name": "dyapp-bootstrap", "version": "0.0.1"}
            good = binary("valid", [package])
            result = run(good)
            self.assertEqual(result.returncode, 0, result.stdout)
            self.assertEqual(result.stdout, "OK auditable\n")
            legacy = run(good, raw=True)
            self.assertEqual(legacy.returncode, 0, legacy.stderr)
            self.assertIn("1 packages in the embedded", legacy.stdout)
            policy = json.loads(
                (ROOT / "tools/scripts/check-output.json").read_text())
            for bad in [Path("/bin/true"),
                        binary("unknown", [dict(package, version="9")]),
                        binary("missing-bootstrap", [])]:
                result = run(bad)
                self.assertEqual(result.returncode, 1)
                self.assertTrue(result.stdout.startswith("FAIL auditable:"))
                self.assertIn(f'FIX: {policy["auditable"]["fix"]}\n',
                              result.stdout)
                self.assertIn("DETAILS:\n", result.stdout)
                self.assertIn(": raw captured output", result.stdout)
                legacy = run(bad, raw=True)
                self.assertNotEqual(legacy.returncode, 0)
                self.assertNotIn("FIX:", legacy.stderr)
            logs = list((root / "logs").glob("auditable-*.log"))
            self.assertEqual(len(logs), 3)
            self.assertTrue(all(path.stat().st_size for path in logs))


if __name__ == "__main__":
    unittest.main()
