"""Verify Clippy expectation attributes cannot hide complexity findings."""
import json
import os
from pathlib import Path
import subprocess
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ComplexityToolTest(unittest.TestCase):
    def test_force_warn_exposes_a_function_with_an_expectation(self):
        directory = ROOT / "build/tool-fixtures/complexity-force-warn"
        directory = directory / str(time.time_ns())
        (directory / "src").mkdir(parents=True)
        (directory / "Cargo.toml").write_text(
            '[workspace]\n[package]\nname = "force-warn-fixture"\n'
            'version = "0.1.0"\nedition = "2021"\n'
            '[lints.rust]\nunfulfilled_lint_expectations = "deny"\n')
        (directory / "clippy.toml").write_text(
            "cognitive-complexity-threshold = 15\n")
        branches = "\n".join(
            f"    if value > {number} {{ value += 1; }}"
            for number in range(16))
        prefix = (
            '#[expect(clippy::cognitive_complexity, reason = "fixture")]\n'
            'pub fn observed(mut value: i32) -> i32 {\n')
        source = prefix + branches + '\n    value\n}\n'
        (directory / "src/lib.rs").write_text(source)
        env = dict(os.environ, CLIPPY_CONF_DIR=str(directory),
                   CARGO_TARGET_DIR=str(directory / "target"))
        run = subprocess.run(
            ["cargo", "clippy", "--manifest-path",
             str(directory / "Cargo.toml"), "--offline",
             "--message-format=json", "--", "-D", "warnings",
             "--force-warn", "clippy::cognitive_complexity"],
            cwd=ROOT, env=env, capture_output=True, text=True,
            check=False, timeout=120)
        (directory / "clippy.jsonl").write_text(run.stdout)
        (directory / "clippy.stderr").write_text(run.stderr)
        messages = [record["message"] for record in
                    map(json.loads, run.stdout.splitlines())
                    if record.get("reason") == "compiler-message"]
        codes = {(message.get("code") or {}).get("code")
                 for message in messages}
        self.assertIn("clippy::cognitive_complexity", codes,
                      f"{directory}: {run.stderr}")
        self.assertEqual(run.returncode, 0, f"{directory}: {run.stderr}")


if __name__ == "__main__":
    unittest.main()
