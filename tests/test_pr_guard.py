"""Fixture diffs for the pull request review guard."""
import contextlib
import datetime as dt
import importlib.util
import io
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

SCRIPT = Path(__file__).parent / "scripts/pr-guard.py"
spec = importlib.util.spec_from_file_location("pr_guard", SCRIPT)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)
SOURCE = "registry+https://github.com/rust-lang/crates.io-index"
NOW = dt.datetime(2026, 10, 23, tzinfo=dt.timezone.utc)


def lock(source=SOURCE):
    return ("version = 4\n\n[[package]]\nname = \"example\"\n"
            "version = \"1.0.0\"\n"
            f"source = \"{source}\"\nchecksum = \"{'a' * 64}\"\n")


class PullRequestGuardTests(unittest.TestCase):
    def setUp(self):
        Path("/tmp/ai").mkdir(exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(dir="/tmp/ai")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.previous = Path.cwd()
        os.chdir(self.root)
        self.addCleanup(os.chdir, self.previous)
        self.git("init", "-q")
        self.write("README.md", "fixture\n")
        self.base = self.commit()

    def git(self, *args):
        result = subprocess.run(["git", "-c", "user.name=Test",
                                 "-c", "user.email=test@example.com",
                                 "-c", "commit.gpgsign=false", *args],
                                capture_output=True, text=True, check=True)
        return result.stdout.strip()

    def write(self, path, content="changed\n"):
        file = self.root / path
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(content)

    def commit(self):
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "fixture")
        return self.git("rev-parse", "HEAD")

    def check(self, files, published=None):
        for path, content in files.items():
            self.write(path, content)
        head = self.commit()
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            with mock.patch.object(guard, "published", side_effect=published):
                ok = guard.check(self.base, head, now=NOW)
        return ok, output.getvalue()

    def test_diverged_base_does_not_count_base_changes(self):
        self.git("checkout", "-b", "base-branch")
        self.write(".github/workflows/ci.yml", "on: push\n")
        current_base = self.commit()
        self.git("checkout", "--detach", self.base)
        self.write("rust/example.rs", "fn main() {}\n")
        head = self.commit()
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            self.assertTrue(guard.check(current_base, head, now=NOW))
        self.assertIn("Changed paths: 1", output.getvalue())

    def test_script_with_code_fails(self):
        ok, _ = self.check({"tests/scripts/entry.sh": "exit 0\n",
                            "rust/example.rs": "fn main() {}\n"})
        self.assertFalse(ok)

    def test_infra_with_code_fails(self):
        ok, text = self.check({".github/workflows/ci.yml": "on: push\n",
                               "rust/example.rs": "fn main() {}\n"})
        self.assertFalse(ok)
        self.assertIn("separate pull request", text)

    def test_deps_with_code_passes(self):
        ok, _ = self.check({"Cargo.toml": "[workspace]\n",
                            "rust/example.rs": "fn main() {}\n"})
        self.assertTrue(ok)

    def test_infra_with_deps_fails(self):
        ok, _ = self.check({".github/dependabot.yml": "version: 2\n",
                            "Cargo.toml": "[workspace]\n"})
        self.assertFalse(ok)

    def test_git_source_fails(self):
        ok, text = self.check(
            {"Cargo.lock": lock("git+https://example.com/repo")})
        self.assertFalse(ok)
        self.assertIn("non-crates.io source", text)

    def test_old_lock_version_passes(self):
        old = NOW - dt.timedelta(days=30)
        ok, text = self.check({"Cargo.lock": lock()}, lambda *_: old)
        self.assertTrue(ok)
        self.assertIn("example 1.0.0", text)

    def test_infra_only_passes(self):
        ok, _ = self.check({".github/workflows/ci.yml": "on: push\n",
                            "tests/test_pr_guard.py": "fixture\n"})
        self.assertTrue(ok)

    def test_young_version_fails(self):
        young = NOW - dt.timedelta(days=1)
        ok, text = self.check({"Cargo.lock": lock()}, lambda *_: young)
        self.assertFalse(ok)
        self.assertIn("younger than 14 days", text)

    def test_unreachable_registry_fails(self):
        ok, text = self.check({"Cargo.lock": lock()}, OSError("unreachable"))
        self.assertFalse(ok)
        self.assertIn("cannot verify age", text)

    def test_guard_change_warns(self):
        ok, text = self.check({"tests/scripts/pr-guard.py": "changed\n"})
        self.assertTrue(ok)
        self.assertIn("changes the guard itself", text)


if __name__ == "__main__":
    unittest.main()
