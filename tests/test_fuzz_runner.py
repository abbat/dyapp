"""Check fuzz build selection and limits without compiling fuzz binaries."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
TARGETS = ["node", "sticker_webp"]
CARGO = """#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
root = Path(__file__).resolve().parents[1]
with (root / "calls.jsonl").open("a") as stream:
    stream.write(json.dumps(["cargo", *sys.argv[1:]]) + "\\n")
if sys.argv[1:3] == ["fuzz", "list"]:
    if os.environ.get("FAIL_LIST"):
        sys.exit(43)
    if os.environ.get("EMPTY_LIST"):
        sys.exit(0)
    print("node\\nsticker_webp")
elif sys.argv[1:3] == ["fuzz", "build"]:
    selected = [arg for arg in sys.argv[3:] if arg in ("node", "sticker_webp")]
    selected = selected or ["node", "sticker_webp"]
    if os.environ.get("FAIL_TARGET") in selected:
        sys.exit(42)
    for target in selected:
        binary = root / "build/fuzz/fixture-host/release" / target
        binary.parent.mkdir(parents=True, exist_ok=True)
        binary.write_text(%s)
        binary.chmod(0o755)
"""
BINARY = """#!/usr/bin/env python3
import json, sys
from pathlib import Path
root = Path(__file__).resolve().parents[4]
with (root / "calls.jsonl").open("a") as stream:
    stream.write(json.dumps(
        ["run", Path(sys.argv[0]).name, *sys.argv[1:]]) + "\\n")
"""


class FuzzRunner(unittest.TestCase):
    def run_stage(self, selected=None, fail=None, fail_list=False,
                  empty_list=False):
        build = ROOT / "build"
        build.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=build) as directory:
            root = Path(directory)
            script = root / "tests/scripts/rust-check.sh"
            script.parent.mkdir(parents=True)
            shutil.copyfile(ROOT / "tests/scripts/rust-check.sh", script)
            (root / "tests/fuzz").mkdir()
            tools = root / "bin"
            tools.mkdir()
            for name, source in {
                "cargo": CARGO % repr(BINARY),
                "rustc": "#!/bin/sh\nprintf 'host: fixture-host\\n'\n",
            }.items():
                path = tools / name
                path.write_text(source)
                path.chmod(0o755)
            env = dict(os.environ, PATH=f"{tools}:{os.environ['PATH']}",
                       FUZZ_SECONDS="1")
            env.pop("FUZZ_TARGETS", None)
            env.pop("FAIL_TARGET", None)
            env.pop("FAIL_LIST", None)
            env.pop("EMPTY_LIST", None)
            if fail_list:
                env["FAIL_LIST"] = "1"
            if empty_list:
                env["EMPTY_LIST"] = "1"
            if selected is not None:
                env["FUZZ_TARGETS"] = selected
            if fail:
                env["FAIL_TARGET"] = fail
            result = subprocess.run(
                ["bash", str(script), "--raw", "fuzz"], env=env,
                capture_output=True, text=True, check=False)
            log = root / "calls.jsonl"
            lines = log.read_text().splitlines() if log.exists() else []
            calls = [json.loads(line) for line in lines]
            return result, calls

    def test_default_and_selected_builds_preserve_limits(self):
        for selected in [None, "sticker_webp", "node sticker_webp",
                         "sticker_webp node"]:
            with self.subTest(selected=selected):
                result, calls = self.run_stage(selected)
                self.assertEqual(result.returncode, 0, result.stderr)
                builds = [row for row in calls
                          if row[1:3] == ["fuzz", "build"]]
                requested = selected.split() if selected else TARGETS
                if selected:
                    self.assertEqual([row[-1] for row in builds], requested)
                else:
                    self.assertEqual(len(builds), 1)
                    self.assertFalse(set(builds[0]) & set(TARGETS))
                runs = [row for row in calls if row[0] == "run"]
                self.assertEqual([row[1] for row in runs], requested)
                for row in runs:
                    self.assertIn("-max_total_time=1", row)
                    self.assertEqual("-max_len=262144" in row,
                                     row[1] == "sticker_webp")
                    self.assertTrue(any(arg.endswith(f"/{row[1]}/")
                                        for arg in row if arg.startswith(
                                            "-artifact_prefix=")))

    def test_discovery_failure_or_empty_selection_cannot_pass(self):
        for selected, fail, empty, status in [
            (None, True, False, 43), (None, False, True, 2),
            ("  ", False, False, 2),
        ]:
            with self.subTest(selected=selected, fail=fail, empty=empty):
                result, calls = self.run_stage(selected, fail_list=fail,
                                               empty_list=empty)
                self.assertEqual(result.returncode, status, result.stderr)
                self.assertFalse(any(row[0] == "run" for row in calls))

    def test_selected_build_failure_stops_before_execution(self):
        result, calls = self.run_stage("sticker_webp", fail="sticker_webp")
        self.assertEqual(result.returncode, 42)
        self.assertFalse(any(row[0] == "run" for row in calls))


if __name__ == "__main__":
    unittest.main()
