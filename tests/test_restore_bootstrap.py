"""Restore swaps a verified snapshot into a stopped node's storage."""

from contextlib import closing
import fcntl
import importlib.util
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import unittest


SCRIPTS = Path(__file__).parents[1] / "tools/scripts"
SCRIPT = SCRIPTS / "restore-bootstrap.py"
SPEC = importlib.util.spec_from_file_location("restore_bootstrap", SCRIPT)
RESTORE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RESTORE)
PREFLIGHT = RESTORE.PREFLIGHT


class RestoreBootstrap(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir="/tmp/ai")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.storage = self.root / "storage"
        self.storage.mkdir(mode=0o750)
        self.snapshot = self.root / "snapshot"
        for name, data in {"node.id": b"peer-id", "network": b"network",
                           "node.key": b"good-key"}.items():
            (self.storage / name).write_bytes(data)
        for name in ("profiles.db", "messages.db"):
            with closing(sqlite3.connect(self.storage / name)) as database, \
                    database:
                database.execute("CREATE TABLE records (id INTEGER)")
                database.execute("INSERT INTO records VALUES (1)")
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
        with PREFLIGHT.stopped(self.storage):
            PREFLIGHT.preflight(self.storage, self.snapshot, "inline", None,
                                self.dyappd, 0, scratch=self.root)
        self.saved = self.live()

    def live(self, path=None):
        path = path or self.storage
        return {p.name: p.read_bytes() for p in path.iterdir()
                if p.name != "dyappd.lock"}

    def migrate(self):
        """What a failed new dyappd leaves behind: a newer, changed store."""
        path = self.storage / "profiles.db"
        with closing(sqlite3.connect(path)) as database, database:
            database.execute("PRAGMA user_version = 99")
            database.execute("INSERT INTO records VALUES (2)")

    def run_restore(self, **options):
        with PREFLIGHT.stopped(self.storage):
            return RESTORE.restore(self.snapshot, self.storage, self.dyappd,
                                   0, scratch=self.root, **options)

    def test_restores_snapshot_and_keeps_failed_store(self):
        self.migrate()
        migrated = self.live()
        failed = self.run_restore()
        self.assertEqual(self.live(), self.saved)
        self.assertEqual(self.live(failed), migrated)
        self.assertEqual(self.storage.stat().st_mode & 0o777, 0o750)
        path = self.storage / "profiles.db"
        with closing(sqlite3.connect(path)) as database:
            self.assertEqual(database.execute(
                "PRAGMA user_version").fetchone(), (0,))
        self.assertEqual(sorted(p.name for p in self.root.iterdir()),
                         sorted(["dyappd", "snapshot", "storage",
                                 failed.name]))
        # The restored directory's lock is free again once restore ends.
        fd = os.open(self.storage / "dyappd.lock", os.O_RDWR)
        self.addCleanup(os.close, fd)
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)

    def test_failed_migrated_update_is_rehearsed_end_to_end(self):
        path = self.storage / "messages.db"
        with closing(sqlite3.connect(path)) as database, database:
            database.execute("INSERT INTO records VALUES (3)")
        # The new dyappd migrates the store, then fails to start.
        new = self.root / "dyappd-new"
        new.write_text(
            f"#!{sys.executable}\n"
            "import pathlib, sqlite3, sys\n"
            "args = dict(a.split('=', 1) for a in sys.argv "
            "if a.startswith('--') and '=' in a)\n"
            "path = pathlib.Path(args['--storage.dir']) / 'profiles.db'\n"
            "database = sqlite3.connect(path)\n"
            "database.execute('PRAGMA user_version = 99')\n"
            "database.execute('INSERT INTO records VALUES (2)')\n"
            "database.commit()\n"
            "database.close()\n"
            "sys.exit('migrated store rejected')\n")
        new.chmod(0o700)
        started = subprocess.run(
            [new, f"--storage.dir={self.storage}"], capture_output=True,
            check=False)
        self.assertNotEqual(started.returncode, 0)
        failed_state = self.live()

        restored = subprocess.run(
            [sys.executable, "-B", SCRIPT, self.snapshot, self.storage,
             f"--dyappd={self.dyappd}", "--id-pow-bits=0",
             f"--scratch={self.root}"],
            capture_output=True, text=True, check=False)
        self.assertEqual(restored.returncode, 0, restored.stderr)
        failed = Path(restored.stdout.rsplit(" ", 1)[1].strip())
        # Data and identity are the snapshot's; the write after it is lost.
        self.assertEqual(self.live(), self.saved)
        self.assertEqual(self.live(failed), failed_state)
        with closing(sqlite3.connect(failed / "messages.db")) as database:
            rows = database.execute("SELECT id FROM records ORDER BY id")
            self.assertEqual(rows.fetchall(), [(1,), (3,)])

    def test_running_node_is_refused(self):
        fd = os.open(self.storage / "dyappd.lock", os.O_RDWR | os.O_CREAT)
        self.addCleanup(os.close, fd)
        fcntl.flock(fd, fcntl.LOCK_EX)
        with self.assertRaisesRegex(ValueError, "node still running"):
            self.run_restore()
        self.assertEqual(self.live(), self.saved)

    def assert_refused_untouched(self, pattern, **options):
        self.migrate()
        migrated = self.live()
        with self.assertRaisesRegex(ValueError, pattern):
            self.run_restore(**options)
        self.assertEqual(self.live(), migrated)
        self.assertEqual(sorted(p.name for p in self.root.iterdir()),
                         ["dyappd", "snapshot", "storage"])

    def test_corrupt_snapshot_is_refused_before_live_data_changes(self):
        (self.snapshot / "storage/messages.db").write_bytes(b"corrupt")
        self.assert_refused_untouched("mismatch")

    def test_other_nodes_snapshot_is_refused(self):
        (self.storage / "node.id").write_bytes(b"other-peer")
        self.assert_refused_untouched("another node")

    def test_insufficient_space_is_refused(self):
        self.assert_refused_untouched("not enough free space",
                                      free=lambda path: 0)


if __name__ == "__main__":
    unittest.main()
