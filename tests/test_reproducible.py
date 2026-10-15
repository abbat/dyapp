"""Reproducibility must reject changed bytes and cached or missing outputs."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "reproducible", ROOT / "scripts/check-reproducible.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class Reproducibility(unittest.TestCase):
    def test_different_empty_and_missing_outputs_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            first, second = [Path(directory) / x for x in ("a", "b")]
            first.write_bytes(b"executable")
            for contents in (None, b"", b"changed"):
                if contents is not None:
                    second.write_bytes(contents)
                with self.assertRaises(ValueError):
                    CHECK.compare([first, second])
            second.write_bytes(first.read_bytes())
            self.assertEqual(len(set(CHECK.compare([first, second]))), 1)

    def test_no_git_host_metadata_and_failed_build_report(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(CHECK, "output", return_value="rustc") as query:
                with patch.object(CHECK.subprocess, "run", return_value=(
                        SimpleNamespace(returncode=7))):
                    report = CHECK.build_pair(
                        "linux", Path(directory), "a" * 40, "123")
            query.assert_called_once_with("rustc", "-Vv")
            self.assertEqual(report["revision"], "a" * 40)
            self.assertEqual(report["source_date_epoch"], "123")
            self.assertEqual(report["builds"][0]["exit_code"], 7)
            self.assertIn("Build 1 failed", report["errors"][0])

    def test_second_build_uses_a_fresh_directory_and_must_produce_output(self):
        for produce_second in (False, True):
            with self.subTest(produce_second=produce_second):
                with tempfile.TemporaryDirectory() as directory:
                    seen = []

                    def cargo(command, **kwargs):
                        env = kwargs["env"]
                        destination = Path(env["CARGO_TARGET_DIR"])
                        self.assertFalse(destination.exists())
                        self.assertEqual(env["CARGO_INCREMENTAL"], "0")
                        self.assertEqual(env["RUSTC_WRAPPER"], "")
                        self.assertEqual(env["RUSTC_WORKSPACE_WRAPPER"], "")
                        seen.append(destination)
                        if len(seen) == 1 or produce_second:
                            binary = destination / "release" / "dyappd"
                            binary.parent.mkdir(parents=True)
                            binary.write_bytes(b"same executable")
                        return SimpleNamespace(returncode=0)

                    with patch.object(CHECK, "output", return_value="1"):
                        with patch.object(CHECK.subprocess, "run", cargo):
                            report = CHECK.build_pair(
                                "dyappd", Path(directory))
                    self.assertEqual(len(set(seen)), 2)
                    self.assertEqual(bool(report["errors"]),
                                     not produce_second)
                    for entry in report["builds"]:
                        if "sha256" in entry:
                            self.assertEqual(entry["bytes"],
                                             len(b"same executable"))


if __name__ == "__main__":
    unittest.main()
