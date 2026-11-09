"""Exercise real Cargo policies against valid and invalid Rust probes."""

import json
import os
from pathlib import Path
import re
import subprocess
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]


def lint_sections(path):
    source = path.read_text()
    prefix = "workspace." if path == ROOT / "Cargo.toml" else ""
    sections = []
    for kind in ("rust", "clippy"):
        header = f"[{prefix}lints.{kind}]"
        match = re.search(
            re.escape(header) + r"\n(.*?)(?=^\[|\Z)", source, re.M | re.S
        )
        if match is None:
            raise AssertionError(f"Missing lint table: {path}: {header}")
        sections.append(f"[lints.{kind}]\n" + match[1])
    return "\n".join(sections)


class PedanticPolicyTest(unittest.TestCase):
    def test_actual_manifests_reject_new_findings_and_stale_expectations(self):
        directory = ROOT / "build/tool-fixtures/pedantic-policy"
        directory = directory / str(time.time_ns())
        prefix = "//! Standalone Cargo lint probe.\n"
        declaration = "/// Returns an empty fixture string.\n#[must_use]\n"
        valid = "pub fn empty() -> String { String::new() }\n"
        invalid = 'pub fn empty() -> String { "".into() }\n'
        expected = '#[expect(clippy::manual_string_new, reason = "probe")]\n'
        stale_error = "unfulfilled_lint_expectations"
        cases = [
            ("valid", declaration + valid, None),
            ("finding", declaration + invalid, "clippy::manual_string_new"),
            ("retained", declaration + expected + invalid, None),
            ("stale", declaration + expected + valid, stale_error),
        ]
        manifests = [
            "Cargo.toml",
            "rust/bootstrap/Cargo.toml",
            "rust/ffi/Cargo.toml",
        ]
        for number, manifest in enumerate(manifests):
            project = directory / str(number)
            (project / "src").mkdir(parents=True)
            config = lint_sections(ROOT / manifest)
            (project / "Cargo.toml").write_text(
                '[workspace]\n[package]\nname = "pedantic-policy-probe"\n'
                'version = "0.1.0"\nedition = "2021"\n' + config
            )
            env = dict(os.environ, CARGO_TARGET_DIR=str(project / "target"))
            for name, body, error in cases:
                with self.subTest(manifest=manifest, case=name):
                    (project / "src/lib.rs").write_text(prefix + body)
                    run = subprocess.run(
                        [
                            "cargo",
                            "clippy",
                            "--manifest-path",
                            str(project / "Cargo.toml"),
                            "--offline",
                            "--message-format=json",
                            "--",
                            "-D",
                            "warnings",
                        ],
                        cwd=ROOT,
                        env=env,
                        capture_output=True,
                        text=True,
                        check=False,
                        timeout=60,
                    )
                    (project / f"{name}.jsonl").write_text(run.stdout)
                    (project / f"{name}.stderr").write_text(run.stderr)
                    messages = [
                        row["message"]
                        for row in map(json.loads, run.stdout.splitlines())
                        if row.get("reason") == "compiler-message"
                    ]
                    codes = {
                        (row.get("code") or {}).get("code") for row in messages
                    }
                    if error is None:
                        self.assertEqual(
                            run.returncode, 0, f"{project}: {run.stderr}"
                        )
                    else:
                        self.assertNotEqual(run.returncode, 0, str(project))
                        self.assertIn(error, codes, f"{project}: {run.stderr}")


if __name__ == "__main__":
    unittest.main()
