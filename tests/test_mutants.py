"""B2 mutation score over cargo-mutants shards."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tests/scripts/check-mutants.py"
SPEC = importlib.util.spec_from_file_location("check_mutants", SCRIPT)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
SPEC = importlib.util.spec_from_file_location(
    "repository_skips", ROOT / "tests/scripts/check-repository.py")
REPOSITORY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REPOSITORY)


def shard(caught, timeout, missed, unviable=0):
    return {"caught": caught, "timeout": timeout, "missed": missed,
            "unviable": unviable}


class Mutants(unittest.TestCase):
    def test_score_sums_shards_and_counts_timeouts_as_caught(self):
        percentage, counts = CHECK.score([shard(6, 1, 2, 5), shard(1, 0, 0)])
        self.assertEqual(percentage, 80.0)
        self.assertEqual(counts, {"caught": 7, "timeout": 1, "missed": 2})
        with self.assertRaises(ValueError):
            CHECK.score([shard(0, 0, 0, 3)])

    def test_score_below_the_baseline_fails(self):
        with self.assertRaisesRegex(ValueError, "80.0% < baseline 80.5%"):
            CHECK.check([shard(8, 0, 2)], 80.5)
        self.assertIn("Raise [mutants] score in "
                      "tests/scripts/baselines.toml to "
                      "80.0", CHECK.check([shard(8, 0, 2)], 79.5))

    def test_cli_reads_shards_and_the_repository_baseline(self):
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "outcomes.json"
            report.write_text(json.dumps(shard(1, 0, 0)))
            result = subprocess.run(
                [sys.executable, "-B", str(SCRIPT), "--raw", str(report)],
                capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Mutation score 100.0%", result.stdout)

    def test_mutants_attributes_need_a_reason(self):
        text = ("#[mutants::skip]\nfn a() {}\n"
                "// Returning false would hang the loop.\n"
                "#[mutants::skip]\nfn b() {}\n"
                "#[cfg_attr(test, mutants::skip)] // hangs\nfn c() {}\n"
                "    #[mutants::exclude_re(\"with 0\")]\nfn d() {}\n")
        self.assertEqual(REPOSITORY.skip_errors(
            {"rust/x/src/lib.rs": text, "notes.md": "#[mutants::skip]\n"}), [
            "rust/x/src/lib.rs:1: mutants attribute without a reason",
            "rust/x/src/lib.rs:8: mutants attribute without a reason",
        ])


if __name__ == "__main__":
    unittest.main()
