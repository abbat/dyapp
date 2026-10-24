"""Preflight passes the verified recovery key to dyappd on scratch data."""

from contextlib import closing
import importlib.util
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import unittest


SCRIPTS = Path(__file__).parents[1] / "tools/scripts"


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


BACKUP = load("backup_bootstrap", "backup-bootstrap.py")
PROBE = load("probe_bootstrap_backup", "probe-bootstrap-backup.py")


class ProbeBootstrapBackup(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir="/tmp/ai")
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        self.source = root / "source"
        self.source.mkdir()
        self.snapshot = root / "snapshot"
        for name, data in {"node.id": b"peer-id", "network": b"network",
                           "node.key": b"good-key"}.items():
            (self.source / name).write_bytes(data)
        for name in ("profiles.db", "messages.db"):
            with closing(sqlite3.connect(self.source / name)) as database, \
                    database:
                database.execute("CREATE TABLE records (id INTEGER)")
        self.dyappd = root / "dyappd"
        self.dyappd.write_text(
            f"#!{sys.executable}\n"
            "import pathlib, sys\n"
            "args = dict(a.split('=', 1) for a in sys.argv "
            "if a.startswith('--') "
            "and '=' in a)\n"
            "storage = pathlib.Path(args['--storage.dir'])\n"
            "assert args['--network.id-pow-bits'] == '0'\n"
            "key = pathlib.Path(args['--storage.key']).read_bytes()\n"
            "if key != b'good-key': sys.exit(1)\n"
            "print((storage / 'node.id').read_text())\n")
        self.dyappd.chmod(0o700)
        self.age = root / "age"
        self.age.write_text(
            f"#!{sys.executable}\n"
            "import pathlib, sys\n"
            "identity = pathlib.Path(sys.argv[3]).read_bytes()\n"
            "encrypted = pathlib.Path(sys.argv[4]).read_bytes()\n"
            "if identity != b'age-secret' or encrypted != b'ciphertext': "
            "sys.exit(1)\n"
            "sys.stdout.buffer.write(b'good-key')\n")
        self.age.chmod(0o700)
        self.age_identity = root / "age-identity"
        self.age_identity.write_bytes(b"age-secret")
        self.age_identity.chmod(0o600)

    def create(self, mode):
        recovery = None
        if mode != "inline":
            (self.source / "node.key").unlink()
            recovery = self.source.parent / "portable-key"
            recovery.write_bytes(
                b"ciphertext" if mode == "age" else b"good-key")
        BACKUP.snapshot(self.source, self.snapshot, mode, recovery)

    def probe(self, **options):
        return PROBE.probe(self.snapshot, self.dyappd, 0,
                           scratch=Path(self.temp.name), **options)

    def test_inline_and_external_use_snapshot_key(self):
        for mode in ("inline", "external"):
            with self.subTest(mode=mode):
                self.snapshot = self.source.parent / mode
                if mode == "inline":
                    (self.source / "node.key").write_bytes(b"good-key")
                self.create(mode)
                before = (self.snapshot / "manifest.jsonl").read_bytes()
                self.assertEqual(self.probe(), "peer-id")
                manifest = self.snapshot / "manifest.jsonl"
                self.assertEqual(manifest.read_bytes(), before)

    def test_age_decrypts_snapshot_copy_and_requires_identity(self):
        self.create("age")
        with self.assertRaisesRegex(ValueError, "age identity and binary"):
            self.probe()
        self.assertEqual(self.probe(age_identity=self.age_identity,
                                    age_binary=self.age), "peer-id")
        self.age_identity.write_bytes(b"wrong")
        with self.assertRaisesRegex(ValueError, "age decryption failed"):
            self.probe(age_identity=self.age_identity, age_binary=self.age)

    def test_real_age_cli(self):
        # Pinned age release from tools/docker/Dockerfile.dev and ci-setup.sh;
        # no skip, so a missing binary fails the check instead of hiding it.
        age = Path(shutil.which("age") or self.fail("age not on PATH"))
        root = self.source.parent
        identity, wrong = root / "real-identity", root / "wrong-identity"
        for path in (identity, wrong):
            subprocess.run(["age-keygen", "-o", str(path)], check=True,
                           stderr=subprocess.DEVNULL)
            path.chmod(0o600)
        recipient = subprocess.run(
            ["age-keygen", "-y", str(identity)], check=True,
            capture_output=True, text=True).stdout.strip()
        plain = root / "plain-key"
        plain.write_bytes(b"good-key")

        def snapshot(name, tamper=False):
            encrypted = root / f"{name}.age"
            subprocess.run([str(age), "--encrypt", "--recipient", recipient,
                            "--output", str(encrypted), str(plain)],
                           check=True)
            if tamper:
                data = bytearray(encrypted.read_bytes())
                data[-1] ^= 1
                encrypted.write_bytes(data)
            (self.source / "node.key").unlink(missing_ok=True)
            self.snapshot = root / name
            BACKUP.snapshot(self.source, self.snapshot, "age", encrypted)

        snapshot("real")
        self.assertEqual(self.probe(age_identity=identity, age_binary=age),
                         "peer-id")
        with self.assertRaisesRegex(ValueError, "age decryption failed"):
            self.probe(age_identity=wrong, age_binary=age)
        with self.assertRaises(FileNotFoundError):
            self.probe(age_identity=identity, age_binary=root / "no-age")
        # Tampered before the snapshot, so only age's own MAC can catch it.
        snapshot("tampered", tamper=True)
        with self.assertRaisesRegex(ValueError, "age decryption failed"):
            self.probe(age_identity=identity, age_binary=age)

    def test_wrong_key_and_corrupt_sqlite_fail(self):
        self.source.joinpath("node.key").write_bytes(b"wrong")
        self.create("inline")
        with self.assertRaisesRegex(ValueError, "restored node key"):
            self.probe()
        self.snapshot = self.source.parent / "corrupt"
        (self.source / "node.key").write_bytes(b"good-key")
        (self.source / "profiles.db").write_bytes(b"corrupt")
        self.create("inline")
        with self.assertRaises(sqlite3.DatabaseError):
            self.probe()

    def test_wal_restore_preserves_data_and_source(self):
        db = sqlite3.connect(self.source / "profiles.db")
        self.addCleanup(db.close)
        db.execute("PRAGMA journal_mode=WAL")
        db.execute("INSERT INTO records VALUES (42)")
        db.commit()
        self.create("inline")
        wal = self.snapshot / "storage/profiles.db-wal"
        self.assertTrue(wal.exists())
        source_bytes = {p.name: p.read_bytes() for p in self.source.iterdir()
                        if p.is_file()}
        store = self.snapshot / "storage"
        snapshot_bytes = {p.name: p.read_bytes()
                          for p in store.iterdir()
                          if p.is_file()}
        self.assertEqual(self.probe(), "peer-id")
        restored = self.source.parent / "restored"
        restored.mkdir()
        for name in ("profiles.db", "profiles.db-wal"):
            shutil.copy2(self.snapshot / "storage" / name, restored / name)
        with closing(sqlite3.connect(restored / "profiles.db")) as copy, \
                copy:
            value = copy.execute("SELECT id FROM records").fetchone()
        self.assertEqual(value, (42,))
        self.assertEqual(source_bytes, {p.name: p.read_bytes()
                                        for p in self.source.iterdir()
                                        if p.is_file()})
        self.assertEqual(snapshot_bytes, {p.name: p.read_bytes()
                                          for p in store.iterdir()
                                          if p.is_file()})

    def test_missing_recovery_evidence_and_changed_byte_fail(self):
        with self.assertRaisesRegex(ValueError, "portable recovery key"):
            BACKUP.snapshot(self.source, self.snapshot, "age", None)
        self.assertFalse(self.snapshot.exists())
        self.create("age")
        (self.snapshot / "recovery/node.key.age").unlink()
        with self.assertRaises(FileNotFoundError):
            self.probe(age_identity=self.age_identity, age_binary=self.age)
        self.snapshot = self.source.parent / "changed"
        (self.source / "node.key").write_bytes(b"good-key")
        self.create("inline")
        (self.snapshot / "storage/messages.db").write_bytes(b"damaged")
        with self.assertRaisesRegex(ValueError, "size mismatch"):
            self.probe()


if __name__ == "__main__":
    unittest.main()
