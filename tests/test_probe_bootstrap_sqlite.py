"""SQLite backup probe opens scratch copies, not the source snapshot."""

import importlib.util
from pathlib import Path
import sqlite3
import tempfile
import unittest


SCRIPTS = Path(__file__).parents[1] / "tools/scripts"


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


BACKUP = load("backup_bootstrap", "backup-bootstrap.py")
PROBE = load("probe_bootstrap_sqlite", "probe-bootstrap-sqlite.py")


class ProbeBootstrapSqlite(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir="/tmp/ai")
        self.addCleanup(self.temp.cleanup)
        root = self.scratch = Path(self.temp.name)
        self.source = root / "source"
        self.source.mkdir()
        self.snapshot = root / "snapshot"
        for name, value in {"node.id": b"peer-id", "network": b"network",
                            "node.key": b"key"}.items():
            (self.source / name).write_bytes(value)
        for name in ("profiles.db", "messages.db"):
            with sqlite3.connect(self.source / name) as db:
                db.execute("CREATE TABLE records (id INTEGER PRIMARY KEY)")
                db.execute("INSERT INTO records DEFAULT VALUES")

    def snapshot_now(self):
        BACKUP.snapshot(self.source, self.snapshot, "inline", None)

    def test_valid_databases_leave_snapshot_unchanged(self):
        self.snapshot_now()
        before = {p.relative_to(self.snapshot): p.read_bytes()
                  for p in self.snapshot.rglob("*") if p.is_file()}
        header, keys, count = PROBE.probe(self.snapshot, self.scratch)
        self.assertEqual((header["peer_id"], count), ("peer-id", 2))
        self.assertIn("storage/node.key", keys)
        after = {p.relative_to(self.snapshot): p.read_bytes()
                 for p in self.snapshot.rglob("*") if p.is_file()}
        self.assertEqual(after, before)

    def test_valid_wal_restores_from_copy(self):
        with sqlite3.connect(self.source / "profiles.db") as db:
            db.execute("PRAGMA journal_mode=WAL")
            db.execute("INSERT INTO records DEFAULT VALUES")
            self.snapshot_now()
            wal = self.snapshot / "storage/profiles.db-wal"
            self.assertTrue(wal.exists())
            self.assertEqual(PROBE.probe(self.snapshot, self.scratch)[2], 2)

    def test_corrupt_database_and_hot_journal_fail(self):
        (self.source / "profiles.db").write_bytes(b"corrupt")
        self.snapshot_now()
        with self.assertRaises(sqlite3.DatabaseError):
            PROBE.probe(self.snapshot, self.scratch)

        other = self.source.parent / "hot"
        (self.source / "profiles.db-journal").write_bytes(b"hot")
        BACKUP.snapshot(self.source, other, "inline", None)
        with self.assertRaisesRegex(ValueError, "hot rollback journal"):
            PROBE.probe(other, self.scratch)


if __name__ == "__main__":
    unittest.main()
