"""Stopped-node snapshot writer rejects incomplete or unsafe input."""

import hashlib
import importlib.util
import json
from pathlib import Path
import stat
import tempfile
import unittest


SCRIPT = Path(__file__).parents[1] / "tools/scripts/backup-bootstrap.py"
SPEC = importlib.util.spec_from_file_location("backup_bootstrap", SCRIPT)
BACKUP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BACKUP)


class BackupBootstrap(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "live"
        self.source.mkdir()
        self.destination = self.root / "backup"
        for name, value in {
            "node.id": b"peer-id",
            "network": b"private-network",
            "profiles.db": b"profiles",
            "messages.db": b"messages",
            "node.key": b"private-key",
        }.items():
            (self.source / name).write_bytes(value)

    def records(self):
        return [json.loads(line) for line in
                (self.destination / "manifest.jsonl").read_text().splitlines()]

    def test_inline_snapshot_contains_sorted_hashes_and_wal(self):
        (self.source / "profiles.db-wal").write_bytes(b"committed")
        (self.source / "profiles.db-shm").write_bytes(b"rebuildable")
        (self.source / "profiles-idx.db").write_bytes(b"rebuildable")
        (self.source / "media.db").write_bytes(b"media")
        data = self.source / "data" / "nested"
        data.mkdir(parents=True)
        (data / "blob").write_bytes(b"blob")

        BACKUP.snapshot(self.source, self.destination, "inline", None)
        header, *files = self.records()
        self.assertEqual(header["schema"], 1)
        self.assertEqual(header["peer_id"], "peer-id")
        self.assertEqual(header["network"], "private-network")
        self.assertEqual(header["absent"], ["deny.db"])
        self.assertFalse(header["hot_journal"])
        names = [record["path"] for record in files]
        self.assertEqual(names, sorted(names))
        self.assertIn("storage/profiles.db-wal", names)
        self.assertIn("storage/data/nested/blob", names)
        self.assertNotIn("storage/profiles.db-shm", names)
        self.assertNotIn("storage/profiles-idx.db", names)
        for record in files:
            data = (self.destination / record["path"]).read_bytes()
            self.assertEqual(record["size"], len(data))
            digest = hashlib.sha256(data).hexdigest()
            self.assertEqual(record["sha256"], digest)
        self.assertEqual(stat.S_IMODE(self.destination.stat().st_mode), 0o700)
        key = self.destination / "storage/node.key"
        self.assertEqual(stat.S_IMODE(key.stat().st_mode), 0o600)

    def test_age_mode_requires_portable_key_and_records_hot_journal(self):
        (self.source / "node.key").unlink()
        with self.assertRaisesRegex(ValueError, "portable recovery key"):
            BACKUP.snapshot(self.source, self.destination, "age", None)
        self.assertFalse(self.destination.exists())

        encrypted = self.root / "node.key.age"
        encrypted.write_bytes(b"encrypted-test-key")
        (self.source / "deny.db").write_bytes(b"deny")
        (self.source / "deny.db-journal").write_bytes(b"hot")
        BACKUP.snapshot(self.source, self.destination, "age", encrypted)
        header, *files = self.records()
        self.assertTrue(header["hot_journal"])
        self.assertEqual(header["key_mode"], "age")
        names = [record["path"] for record in files]
        self.assertIn("recovery/node.key.age", names)
        self.assertIn("storage/deny.db-journal", names)
        self.assertFalse((self.destination / "storage/node.key").exists())

    def test_symlink_unknown_and_nested_destination_fail_before_copy(self):
        (self.source / "node.id").unlink()
        (self.source / "node.id").symlink_to(self.root / "outside")
        with self.assertRaises(ValueError):
            BACKUP.snapshot(self.source, self.destination, "inline", None)
        self.assertFalse(self.destination.exists())

        (self.source / "node.id").unlink()
        (self.source / "node.id").write_text("peer-id")
        (self.source / "unknown").write_bytes(b"hidden data")
        with self.assertRaisesRegex(ValueError, "unknown storage entry"):
            BACKUP.snapshot(self.source, self.destination, "inline", None)
        self.assertFalse(self.destination.exists())
        nested = self.source / "nested"
        with self.assertRaisesRegex(ValueError, "outside source"):
            BACKUP.snapshot(self.source, nested, "inline", None)
        self.destination.symlink_to(self.root / "elsewhere")
        with self.assertRaisesRegex(ValueError, "destination must not"):
            BACKUP.snapshot(self.source, self.destination, "inline", None)
        self.assertFalse((self.root / "elsewhere").exists())


if __name__ == "__main__":
    unittest.main()
