"""Small-batch parser and real Git-history regressions."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tests/scripts/check-batches.py"
SPEC = importlib.util.spec_from_file_location("batches", SCRIPT)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class Batches(unittest.TestCase):
    def test_numstat_exclusions_and_unusual_names(self):
        names = ["Cargo.lock", "clients/linux/Cargo.lock", "yarn.lock",
                 "clients/android/gradle.lockfile", "package-lock.json",
                 "npm-shrinkwrap.json", "tests/a.snap",
                 "tests/snapshots/a.txt", "tests/__snapshots__/a.js",
                 "generated/a.rs", "src/generated/a.rs", "dyapp.uniffi.rs"]
        stats = b"".join(f"999\t999\t{name}\0".encode() for name in names)
        stats += (b"200\t200\tsrc/ordinary.rs\0"
                  b"1\t2\tweird\tname\n.rs\0-\t-\timage.png\0")
        self.assertEqual(CHECK.changed_lines(stats), 403)
        self.assertFalse(CHECK.excluded("src/lock.rs"))
        self.assertFalse(CHECK.excluded("docs/snapshots.md"))

    def test_large_change_reason_in_final_metadata(self):
        message = ("Change code\n\nDetails.\n\n"
                   "Large-Change: Keep platform changes together\n\n"
                   "Co-Authored-By: Codex <noreply@openai.com>\n")
        self.assertEqual(CHECK.large_change_reason(message),
                         "Keep platform changes together")
        message = ("Change code\n\nLarge-Change: \n\n"
                   "Co-Authored-By: Codex <noreply@openai.com>")
        self.assertEqual(CHECK.large_change_reason(message), "")
        self.assertEqual(CHECK.large_change_reason(
            "Change code\n\nLarge-Change: Apparent reason\n\nMore prose."), "")

    def test_shallow_history_fails(self):
        with patch.object(CHECK, "git", return_value=b"true\n"):
            with self.assertRaisesRegex(ValueError, "full-history"):
                CHECK.inspect("base", "HEAD", 400)

    def test_real_history_and_cli_failure(self):
        # Keep fixtures under the approved temp root; do not delete user files.
        Path("/tmp/ai").mkdir(exist_ok=True)
        repo = Path(tempfile.mkdtemp(prefix="batches-", dir="/tmp/ai"))

        def git(*args):
            return subprocess.check_output(
                ["git", "-c", "core.hooksPath=/dev/null",
                 "-c", "commit.gpgsign=false", *args], cwd=repo)

        def commit(text, message):
            (repo / "code.txt").write_text(text)
            git("add", "code.txt")
            git("commit", "-qm", message)
            return git("rev-parse", "HEAD").decode().strip()

        git("init", "-q")
        git("config", "user.name", "Batch test")
        git("config", "user.email", "batch@example.invalid")
        base = commit("", "Start")
        exact = commit("line\n" * 400, "Add exactly 400 lines")

        def run(base, head="HEAD"):
            result = subprocess.run(
                [sys.executable, "-B", str(SCRIPT), "--base", base,
                 "--head", head, "--report", str(repo / "report.json")],
                cwd=repo, capture_output=True, text=True)
            return result, json.loads((repo / "report.json").read_text())

        result, report = run(base)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "OK batches\n")
        self.assertEqual(report["commits"][0]["lines"], 400)
        self.assertEqual(report["large_percent"], 0)
        # Removing 400 lines and adding one is 401, not a net change of 399.
        bad = commit("replacement\n", "Replace code")
        result, report = run(base)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertTrue(result.stdout.startswith("FAIL batches:"))
        self.assertIn("401 > 400", report["errors"][0])
        self.assertEqual(report["large_percent"], 50)
        # Old violations are outside a newer baseline, with no SHA allow-list.
        result, report = run(bad, bad)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(report["total_commits"], 0)
        commit("new\n" * 400, "Replace code\n\nLarge-Change: \n")
        result, report = run(bad)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertTrue(report["errors"])
        blank = git("rev-parse", "HEAD").decode().strip()
        commit("replacement\n", "Replace code\n\n"
               "Large-Change: Keep the atomic fixture update together\n")
        result, report = run(blank)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(report["large_commits"], 1)
        self.assertIn("atomic fixture", report["commits"][0]["reason"])
        # Commit-msg preflight checks staged lines before a commit exists.
        (repo / "code.txt").write_text("new\n" * 401)
        git("add", "code.txt")
        message_file = repo / "message.txt"
        message_file.write_text("Change code\n")
        preflight = subprocess.run(
            [sys.executable, "-B", str(SCRIPT), "--staged",
             "--message-file", str(message_file)],
            cwd=repo, capture_output=True, text=True)
        self.assertEqual(preflight.returncode, 1)
        self.assertIn("402 > 400", preflight.stdout)
        message_file.write_text("Change code\n\n"
                                "Large-Change: Keep fixture together\n\n"
                                "Co-Authored-By: Codex <noreply@openai.com>\n")
        preflight = subprocess.run(
            [sys.executable, "-B", str(SCRIPT), "--staged",
             "--message-file", str(message_file)],
            cwd=repo, capture_output=True, text=True)
        self.assertEqual(preflight.returncode, 0, preflight.stdout)
        result, report = run(exact, exact)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(report["total_commits"], 0)
        self.assertEqual(report["large_percent"], 0)


if __name__ == "__main__":
    unittest.main()
