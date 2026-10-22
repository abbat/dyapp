"""No crates.io version younger than 14 days reaches a Cargo.lock."""
from datetime import datetime, timedelta, timezone
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "check_dependency_age", ROOT / "tests/scripts/check-dependency-age.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
NOW = datetime(2026, 10, 23, tzinfo=timezone.utc)


class DependencyAge(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.lock = self.root / "Cargo.lock"
        self.index = self.root / "index.crates.io-x"

    def publish(self, name, version, days=None):
        entry = {"name": name, "vers": version}
        if days is not None:
            entry["pubtime"] = (NOW - timedelta(days=days)).strftime(
                "%Y-%m-%dT%H:%M:%SZ")
        cache = self.index / ".cache" / CHECK.index_path(name)
        cache.parent.mkdir(parents=True, exist_ok=True)
        cache.write_bytes(b"\0".join([
            b"\x03\0\0\0etag", version.encode(), json.dumps(entry).encode(),
            b""]))
        with self.lock.open("a") as lock:
            lock.write(f'[[package]]\nname = "{name}"\nversion = "{version}"'
                       f'\nsource = "{CHECK.CRATES_IO}"\n\n')

    def errors(self, exceptions=None):
        return CHECK.errors([self.lock], [self.index], NOW, exceptions or {})

    def test_age(self):
        self.publish("serde", "1.0.0", days=20)
        self.publish("ab", "0.1.0")
        self.assertEqual(self.errors(), [])
        self.publish("new", "0.2.0", days=3)
        [error] = self.errors()
        self.assertIn("new 0.2.0", error)
        self.assertIn("younger than 14 days", error)

    def test_exception(self):
        self.publish("new", "0.2.0", days=3)
        self.assertEqual(self.errors(
            {("new", "0.2.0"): ("2026-10-20", "fixes RUSTSEC-0000-0000")}),
            [])
        stale = self.errors({("new", "0.2.0"): ("2026-10-01", "")})
        self.assertEqual(len(stale), 2)

    def test_missing_from_index_fails(self):
        self.lock.write_text(f'[[package]]\nname = "gone"\nversion = "1.0.0"'
                             f'\nsource = "{CHECK.CRATES_IO}"\n\n'
                             '[[package]]\nname = "local"\n'
                             'version = "0.1.0"\n')
        [error] = self.errors()
        self.assertIn("gone 1.0.0", error)

    def test_index_path(self):
        self.assertEqual(
            [CHECK.index_path(name) for name in ("a", "ab", "abc", "Serde")],
            ["1/a", "2/ab", "3/a/abc", "se/rd/serde"])


if __name__ == "__main__":
    unittest.main()
