"""Classify retained public visibility without removing implementations."""
import json
import os
from pathlib import Path
import subprocess
import time
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]


class VisibilityPolicyTest(unittest.TestCase):
    def test_new_findings_and_stale_markers_fail_the_real_compiler(self):
        policy = tomllib.loads((ROOT / "Cargo.toml").read_text())
        lints = policy["workspace"]["lints"]["rust"]
        keys = ("unreachable_pub", "unfulfilled_lint_expectations")
        root = ROOT / "build/tool-fixtures/visibility" / str(time.time_ns())
        (root / "src").mkdir(parents=True)
        settings = "".join(f"{key} = {json.dumps(lints[key])}\n"
                           for key in keys)
        manifest = (
            '[workspace]\n[package]\nname = "visibility-fixture"\n'
            'version = "0.1.0"\nedition = "2021"\n[lints.rust]\n')
        (root / "Cargo.toml").write_text(manifest + settings)
        env = dict(os.environ, CARGO_TARGET_DIR=str(root / "target"))
        base = (
            'mod internal {\n'
            '    pub fn retained() {}\n'
            '}\n'
            'pub fn call_internal() { internal::retained(); }\n')
        self.run_case(root, env, "unmarked", base, "unreachable_pub")
        marker = (
            '    #[expect(unreachable_pub, '
            'reason = "Retained module API visibility")]\n')
        declaration = marker + "    pub fn retained()"
        marked = base.replace("    pub fn retained()", declaration)
        self.run_case(root, env, "retained", marked)
        added = marked.replace(
            "    pub fn retained() {}",
            "    pub fn retained() {}\n    pub fn new_unclassified() {}")
        self.run_case(root, env, "new-finding", added, "unreachable_pub")
        exported = marked.replace("mod internal", "pub mod internal")
        self.run_case(root, env, "now-exported", exported,
                      "unfulfilled_lint_expectations")

    def run_case(self, root, env, name, source, expected=None):
        (root / "src/lib.rs").write_text(source)
        result = subprocess.run(
            ["cargo", "check", "--manifest-path", str(root / "Cargo.toml"),
             "--offline", "--message-format=json"],
            cwd=root, env=env, capture_output=True, text=True,
            check=False, timeout=120)
        (root / f"{name}.jsonl").write_text(result.stdout)
        (root / f"{name}.stderr").write_text(result.stderr)
        if expected is None:
            self.assertEqual(result.returncode, 0, result.stderr)
            return
        codes = set()
        for item in map(json.loads, result.stdout.splitlines()):
            if item.get("reason") != "compiler-message":
                continue
            message = item["message"]
            if message["level"] == "error":
                codes.add((message.get("code") or {}).get("code"))
        self.assertNotEqual(result.returncode, 0, f"{name}: {root}")
        self.assertIn(expected, codes, f"{name}: {codes}; reports: {root}")


if __name__ == "__main__":
    unittest.main()
