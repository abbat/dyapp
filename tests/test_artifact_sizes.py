"""Size gates reject overflow, absent artifacts and out-of-range exceptions."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "artifact_sizes", ROOT / "scripts/check-artifact-size.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class ArtifactSizes(unittest.TestCase):
    def test_real_file_boundary_and_explicit_exception(self):
        entries = {"app": {"bytes": 100, "environment": "fixture",
                           "revision": "a" * 40}}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "app"
            for size, overflow in ((50, False), (105, False), (106, True)):
                path.write_bytes(b"x" * size)
                report = CHECK.measure("app", path, entries, [])
                self.assertEqual(bool(report["errors"]), overflow)
                accepted = CHECK.measure("app", path, entries, ["reviewed"])
                self.assertFalse(accepted["errors"])
                self.assertEqual(accepted["actual_bytes"], size)
            path.write_bytes(b"")
            for name, artifact in (("app", path), ("app", path / "missing"),
                                   ("unknown", path)):
                with self.assertRaises(ValueError):
                    CHECK.measure(name, artifact, entries, ["reviewed"])

    def test_inventory_rejects_invalid_or_unmeasured_budgets(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "sizes.json"
            for record in ({}, [], {"app": {"bytes": 0}},
                           {"app": {"bytes": True}},
                           {"app": {"bytes": 5, "environment": ""}},
                           {"app": {"bytes": 5, "environment": "fixture",
                                    "revision": "short"}}):
                path.write_text(json.dumps(record))
                with self.assertRaises(ValueError):
                    CHECK.inventory(path)

    def test_only_nonempty_trailers_inside_ancestor_range_count(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.check_output(
                    ["git", *args], cwd=root, text=True,
                    stderr=subprocess.PIPE).strip()

            git("init")
            git("config", "user.name", "Fixture")
            git("config", "user.email", "fixture@example.invalid")
            git("commit", "--allow-empty", "-m", "Initial",
                "-m", "Size-Increase: stale approval")
            base = git("rev-parse", "HEAD")
            git("commit", "--allow-empty", "-m", "Current change")
            self.assertEqual(CHECK.exceptions(base, root=root), [])
            git("commit", "--allow-empty", "-m", "Empty reason",
                "-m", "Size-Increase:")
            self.assertEqual(CHECK.exceptions(base, root=root), [])
            git("commit", "--allow-empty", "-m", "Reviewed increase",
                "-m", "Size-Increase: required runtime dependency")
            self.assertEqual(CHECK.exceptions(base, root=root),
                             ["required runtime dependency"])
            with self.assertRaises(subprocess.CalledProcessError):
                CHECK.exceptions("missing-revision", root=root)
            with self.assertRaises(subprocess.CalledProcessError):
                CHECK.exceptions("HEAD", base, root=root)


if __name__ == "__main__":
    unittest.main()
