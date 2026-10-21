"""Measurements must not turn absent artifacts into zero-byte successes."""
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "artifact_measurement", ROOT / "tools/scripts/report-artifact-size.py")
REPORT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REPORT)


class ArtifactMeasurements(unittest.TestCase):
    def test_hash_covers_data_beyond_the_first_chunk(self):
        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory) / "large.exe"
            content = b"x" * (1024 * 1024) + b"tail"
            artifact.write_bytes(content)
            result = REPORT.file_record("app", artifact, "fixture")
            self.assertEqual(result["sha256"],
                             hashlib.sha256(content).hexdigest())

    def test_missing_empty_and_real_file(self):
        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory) / "app.exe"
            with self.assertRaises(ValueError):
                REPORT.file_record("app", artifact, "fixture")
            artifact.write_bytes(b"")
            with self.assertRaises(ValueError):
                REPORT.file_record("app", artifact, "fixture")
            artifact.write_bytes(b"release executable fixture")
            result = REPORT.file_record("app", artifact, "fixture")
            self.assertEqual(result["bytes"], 26)
            self.assertEqual(result["environment"], "fixture")
            self.assertEqual(len(result["sha256"]), 64)
            self.assertNotIn("growth_percent", result)


if __name__ == "__main__":
    unittest.main()
