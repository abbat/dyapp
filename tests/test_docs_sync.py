"""Interface detection of docs-sync.py."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "docs_sync", ROOT / "scripts/docs-sync.py")
SYNC = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SYNC)


class DocsSync(unittest.TestCase):
    def test_errors(self):
        files = {"rust/bootstrap/src/config.rs": ["    pub max_peers: u32,"],
                 "Makefile": ["ui-test-web:"],
                 "rust/ffi/src/lib.rs": ["    let x = 1;"]}
        self.assertEqual(SYNC.errors(files, "Add x"), [
            "Make target: Makefile",
            "config key or CLI flag: rust/bootstrap/src/config.rs"])
        self.assertEqual(SYNC.errors(
            dict(files, **{"docs/a.md": ["x"]}), "Add x"), [])
        self.assertEqual(SYNC.errors(
            files, "Add x\n\nDocs-Unchanged: test-only field\n"), [])
        self.assertEqual(SYNC.errors(
            {"proto/node.proto": ["  string name = 3;"],
             "rust/ffi/src/lib.rs": ["// pub fn in a comment"]}, "x"),
            ["public API: proto/node.proto"])


if __name__ == "__main__":
    unittest.main()
