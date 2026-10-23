import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


SCRIPTS = Path(__file__).parents[1] / "tools/scripts"


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


BACKUP = load("backup_bootstrap", "backup-bootstrap.py")
VERIFY = load("verify_bootstrap", "verify-bootstrap-backup.py")


class VerifyBootstrapBackup(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        self.source = root / "source"
        self.source.mkdir()
        self.snapshot = root / "snapshot"
        for name, data in {"node.id": b"peer-id", "network": b"network",
                           "node.key": b"key", "profiles.db": b"profiles",
                           "messages.db": b"messages"}.items():
            (self.source / name).write_bytes(data)

    def create(self, mode="inline"):
        recovery = None
        if mode != "inline":
            (self.source / "node.key").unlink(missing_ok=True)
            recovery = self.source.parent / "portable-key"
            recovery.write_bytes(b"portable")
        BACKUP.snapshot(self.source, self.snapshot, mode, recovery)

    def change_manifest(self, change):
        manifest = self.snapshot / "manifest.jsonl"
        lines = [json.loads(line) for line in
                 manifest.read_text().splitlines()]
        change(lines)
        manifest.write_text("".join(json.dumps(item) + "\n" for item in lines))

    def test_valid_inline_with_wal_and_nested_data(self):
        (self.source / "profiles.db-wal").write_bytes(b"wal")
        (self.source / "media.db").write_bytes(b"media")
        data = self.source / "data/nested"
        data.mkdir(parents=True)
        (data / "blob").write_bytes(b"blob")
        self.create()
        peer, _ = VERIFY.verify(self.snapshot)
        self.assertEqual(peer, "peer-id")

    def test_changed_missing_extra_and_symlink(self):
        self.create()
        file = self.snapshot / "storage/profiles.db"
        file.write_bytes(b"altered!")
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            VERIFY.verify(self.snapshot)
        file.write_bytes(b"profiles")
        (self.snapshot / "storage/extra").write_bytes(b"extra")
        with self.assertRaisesRegex(ValueError, "extra files"):
            VERIFY.verify(self.snapshot)
        (self.snapshot / "storage/extra").unlink()
        file.unlink()
        with self.assertRaises(FileNotFoundError):
            VERIFY.verify(self.snapshot)
        file.symlink_to(self.source / "profiles.db")
        with self.assertRaises(OSError):
            VERIFY.verify(self.snapshot)

    def test_external_and_age_key_modes(self):
        for mode in ("external", "age"):
            with self.subTest(mode=mode):
                self.snapshot = self.source.parent / mode
                self.create(mode)
                self.assertEqual(VERIFY.verify(self.snapshot)[0], "peer-id")
                self.change_manifest(lambda lines: lines.pop(1))
                with self.assertRaisesRegex(ValueError, "recovery key"):
                    VERIFY.verify(self.snapshot)

    def test_missing_empty_directory(self):
        self.create()
        (self.snapshot / "recovery").rmdir()
        with self.assertRaisesRegex(ValueError, "missing directories"):
            VERIFY.verify(self.snapshot)

    def test_oversized_and_wrongly_typed_header(self):
        self.create()
        manifest = self.snapshot / "manifest.jsonl"
        original = manifest.read_text()
        manifest.write_bytes(b"x" * (BACKUP.MAX_RECORD + 1) + b"\n")
        with self.assertRaisesRegex(ValueError, "oversized manifest record"):
            VERIFY.verify(self.snapshot)
        manifest.write_text(original)
        self.change_manifest(lambda lines: lines[0].update(absent=[42]))
        with self.assertRaisesRegex(ValueError, "absent-store list"):
            VERIFY.verify(self.snapshot)

    def test_malformed_duplicate_and_traversal(self):
        self.create()
        manifest = self.snapshot / "manifest.jsonl"
        original = manifest.read_text()
        duplicate = original.replace('"schema":1', '"schema":1,"schema":1', 1)
        manifest.write_text(duplicate)
        with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
            VERIFY.verify(self.snapshot)
        manifest.write_text(original)
        self.change_manifest(lambda lines: lines[1].update(path="../outside"))
        with self.assertRaisesRegex(ValueError, "unsafe manifest path"):
            VERIFY.verify(self.snapshot)


if __name__ == "__main__":
    unittest.main()
