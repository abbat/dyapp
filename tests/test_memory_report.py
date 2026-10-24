"""Observe the harness saving every node before a real gate failure."""
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests/scripts"))
# This test imports the report helper from the path added above.
from memory_report import write_report  # noqa: E402

SPEC = importlib.util.spec_from_file_location(
    "network_memory", ROOT / "tests/scripts/network-test.py")
NETWORK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NETWORK)


class MemoryReports(unittest.TestCase):
    def test_complete_success_and_failure_keep_all_node_samples(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "dyappd").write_bytes(b"measured release binary")
            expected = hashlib.sha256(b"measured release binary").hexdigest()

            def start(_bin, _storage, peers, processes, _env, flood):
                self.assertFalse(flood)
                peers.extend([("tcp", "quic")] * 3)
                processes.extend([object()] * 3)

            for peaks, failure in [([110, 109, 100], False),
                                   ([110, 111, 112], True)]:
                with patch.object(NETWORK, "start", side_effect=start), \
                        patch.object(NETWORK, "stop") as stop, \
                        patch.object(NETWORK, "mailbox"), \
                        patch.object(NETWORK, "media"), \
                        patch.object(NETWORK, "PLATEAU_SECONDS", 2), \
                        patch.object(NETWORK.time, "monotonic",
                                     side_effect=[0, 0, 1, 1, 2, 2, 3]), \
                        patch.object(NETWORK, "rss",
                                     side_effect=[100, 100, 100] + peaks), \
                        patch.dict(os.environ, {
                            "DYAPP_MEMORY_REPORT_DIR": str(root)}), \
                        patch("sys.stdout", io.StringIO()):
                    if failure:
                        with self.assertRaisesRegex(
                                RuntimeError, r"Nodes \[2, 3\]"):
                            NETWORK.plateau(str(root))
                    else:
                        NETWORK.plateau(str(root))
                    self.assertEqual(stop.call_args.args[1], failure)
            paths = list(root.glob("memory-plateau-*.json"))
            self.assertEqual(len(paths), 2)
            reports = [json.loads(path.read_text()) for path in paths]
            self.assertEqual(sorted(len(r["failed_nodes"]) for r in reports),
                             [0, 2])
            for report in reports:
                self.assertEqual(report["binary_sha256"], expected)
                self.assertEqual(report["growth_limit"], 0.10)
                self.assertEqual(len(report["nodes"]), 3)
                self.assertEqual(report["rounds"], 2)
                self.assertEqual(report["samples"][1]["elapsed_seconds"], 2)
                self.assertEqual(report["measured_seconds"], 3)

    def test_incomplete_observation_cannot_create_success_report(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(RuntimeError, "Incomplete"):
                write_report([], 1, 300, .10, "/missing", directory)
            with self.assertRaisesRegex(RuntimeError, "Too few"):
                write_report([], 300, 300, .10, "/missing", directory)
            self.assertFalse(list(Path(directory).glob("*.json")))


if __name__ == "__main__":
    unittest.main()
