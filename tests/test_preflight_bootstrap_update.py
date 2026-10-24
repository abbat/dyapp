"""Update preflight snapshots and proves only a stopped node's storage."""

from contextlib import closing
import fcntl
import importlib.util
import os
from pathlib import Path
import sqlite3
import sys
import tempfile
import unittest


SCRIPTS = Path(__file__).parents[1] / "tools/scripts"
SCRIPT = SCRIPTS / "preflight-bootstrap-update.py"
SPEC = importlib.util.spec_from_file_location("preflight_update", SCRIPT)
PREFLIGHT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PREFLIGHT)


class PreflightBootstrapUpdate(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir="/tmp/ai")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "source"
        self.source.mkdir()
        self.snapshot = self.root / "snapshot"
        for name, data in {"node.id": b"peer-id", "network": b"network",
                           "node.key": b"good-key"}.items():
            (self.source / name).write_bytes(data)
        for name in ("profiles.db", "messages.db"):
            with closing(sqlite3.connect(self.source / name)) as database, \
                    database:
                database.execute("CREATE TABLE records (id INTEGER)")
        self.dyappd = self.root / "dyappd"
        self.dyappd.write_text(
            f"#!{sys.executable}\n"
            "import pathlib, sys\n"
            "args = dict(a.split('=', 1) for a in sys.argv "
            "if a.startswith('--') and '=' in a)\n"
            "storage = pathlib.Path(args['--storage.dir'])\n"
            "key = pathlib.Path(args['--storage.key']).read_bytes()\n"
            "if key != b'good-key': sys.exit(1)\n"
            "print((storage / 'node.id').read_text())\n")
        self.dyappd.chmod(0o700)

    def run_preflight(self, mode="inline", recovery=None, **options):
        with PREFLIGHT.stopped(self.source):
            return PREFLIGHT.preflight(self.source, self.snapshot, mode,
                                       recovery, self.dyappd, 0,
                                       scratch=self.root, **options)

    def live(self):
        return {p.name: p.read_bytes() for p in self.source.iterdir()}

    def test_stopped_node_passes_and_releases_lock(self):
        before = self.live()
        self.assertEqual(self.run_preflight(), "peer-id")
        self.assertTrue((self.snapshot / "manifest.jsonl").exists())
        after = self.live()
        self.assertEqual(after.pop("dyappd.lock"), b"")
        self.assertEqual(after, before)
        fd = os.open(self.source / "dyappd.lock", os.O_RDWR)
        self.addCleanup(os.close, fd)
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)

    def test_running_node_fails_before_snapshot(self):
        fd = os.open(self.source / "dyappd.lock", os.O_RDWR | os.O_CREAT)
        self.addCleanup(os.close, fd)
        fcntl.flock(fd, fcntl.LOCK_EX)
        with self.assertRaisesRegex(ValueError, "node still running"):
            self.run_preflight()
        self.assertFalse(self.snapshot.exists())

    def test_insufficient_space_fails_before_snapshot(self):
        sizes = []

        def free(path):
            sizes.append(path)
            return PREFLIGHT.MARGIN

        with self.assertRaisesRegex(ValueError, "not enough free space"):
            self.run_preflight(free=free)
        self.assertFalse(self.snapshot.exists())
        # Destination, scratch and storage share one filesystem here.
        self.assertEqual(sizes, [self.root])

    def test_missing_recovery_key_and_corrupt_store_fail(self):
        (self.source / "node.key").unlink()
        with self.assertRaisesRegex(ValueError, "portable recovery key"):
            self.run_preflight("external")
        self.assertFalse(self.snapshot.exists())
        (self.source / "node.key").write_bytes(b"good-key")
        (self.source / "profiles.db").write_bytes(b"corrupt")
        with self.assertRaises(sqlite3.DatabaseError):
            self.run_preflight()

    def test_identity_mismatch_fails(self):
        (self.source / "node.key").write_bytes(b"wrong")
        with self.assertRaisesRegex(ValueError, "restored node key"):
            self.run_preflight()


if __name__ == "__main__":
    unittest.main()
