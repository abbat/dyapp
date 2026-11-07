"""Exercise the pinned API checker against a real two-crate workspace."""
import json
import os
from pathlib import Path
import subprocess
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]


class UnusedPublicToolTest(unittest.TestCase):
    def test_cross_crate_uses_dead_items_and_exact_external_exports(self):
        root = ROOT / "build/tool-fixtures/unused-public" / str(time.time_ns())
        for directory in ("api/src", "app/src"):
            (root / directory).mkdir(parents=True)
        (root / "Cargo.toml").write_text(
            '[workspace]\nmembers = ["api", "app"]\nresolver = "2"\n')
        (root / "api/Cargo.toml").write_text(
            '[package]\nname = "fixture-api"\nversion = "0.1.0"\n'
            'edition = "2024"\n')
        (root / "app/Cargo.toml").write_text(
            '[package]\nname = "fixture-app"\nversion = "0.1.0"\n'
            'edition = "2024"\n'
            '[dependencies]\nfixture-api = { path = "../api" }\n')
        main = root / "app/src/main.rs"
        main.write_text(
            'fn main() { println!("{}", fixture_api::used() + '
            'fixture_api::USED); }\n')
        api = root / "api/src/lib.rs"
        base = "pub fn used() -> u8 { 1 }\npub const USED: u8 = 1;\n"
        api.write_text(base)
        environment = dict(os.environ, CARGO_TARGET_DIR=str(root / "target"),
                           CARGO_NET_OFFLINE="true")
        lock = subprocess.run(
            ["cargo", "generate-lockfile", "--offline",
             "--manifest-path", str(root / "Cargo.toml")],
            cwd=root, env=environment, capture_output=True,
            text=True, check=False, timeout=120)
        self.assertEqual(lock.returncode, 0, lock.stderr)
        self.check(root, environment, "cross-crate-use", True)
        api.write_text(
            base + "pub fn orphan() -> u8 { 2 }\npub const ORPHAN: u8 = 2;\n")
        self.check(root, environment, "unused-function-and-constant", False,
                   ("hawk::dead_public", "orphan", "ORPHAN"))
        production = (
            '[[production]]\npackage = "fixture-app"\nbin = "fixture-app"\n'
            'reason = "fixture shipped entry point"\n')
        overrides = "".join(
            '[[override]]\nlint = "hawk::dead_public"\ncrate = "fixture_api"\n'
            f'item = "{item}"\nlevel = "expect"\n'
            'reason = "fixture bridge export invisible to Rust callers"\n'
            for item in ("orphan", "ORPHAN"))
        (root / "hawk.toml").write_text(production + overrides)
        self.check(root, environment, "external-exports", True)
        main.write_text(
            'fn main() { println!("{}", fixture_api::used()'
            ' + fixture_api::USED'
            ' + fixture_api::orphan() + fixture_api::ORPHAN); }\n')
        self.check(root, environment, "stale-expectation", False,
                   ("hawk::unfulfilled_expectation",))
        main.write_text(
            'fn main() { println!("{}", fixture_api::used() + '
            'fixture_api::USED); }\n')
        api.write_text(base)
        self.check(root, environment, "missing-marked-api", False,
                   ("hawk::unknown_item",))

    def check(self, root, environment, name, success, expected=()):
        result = subprocess.run(
            ["cargo", "hawk", "check", "--manifest-path",
             str(root / "Cargo.toml"),
             "--only", "dead-public", "--output-format=json",
             "-D", "warnings"],
            cwd=root, env=environment, capture_output=True,
            text=True, check=False, timeout=120)
        (root / f"{name}.json").write_text(result.stdout)
        (root / f"{name}.stderr").write_text(result.stderr)
        self.assertEqual(result.returncode == 0, success,
                         f"{name}: {result.stderr[-4096:]}; reports: {root}")
        report = json.loads(result.stdout)
        diagnostics = json.dumps(report["diagnostics"])
        for item in expected:
            self.assertIn(item, diagnostics, f"{name}; report: {root}")


if __name__ == "__main__":
    unittest.main()
