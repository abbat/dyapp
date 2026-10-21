"""Trigger list checks and diff detection of triggers.py."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "triggers", ROOT / "tests/scripts/triggers.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)

TRIGGERS = CHECK.load(CHECK.LIST.read_text())
DIFF = """\
diff --git a/rust/a b/x.rs b/rust/a b/x.rs
--- a/rust/a b/x.rs
+++ b/rust/a b/x.rs
@@ -1 +1 @@
--- unsafe in a removed comment
+let x = 1;
diff --git a/proto/node.proto b/proto/node.proto
deleted file mode 100644
--- a/proto/node.proto
+++ /dev/null
@@ -1 +0,0 @@
-syntax = "proto3";
diff --git a/rust/ffi/Cargo.toml b/rust/ffi/Cargo.toml
--- a/rust/ffi/Cargo.toml
+++ b/rust/ffi/Cargo.toml
@@ -1,0 +2 @@
+sha2 = "0.10"
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1 +1 @@
+unsafe CREATE TABLE
"""


class Triggers(unittest.TestCase):
    def test_diff(self):
        self.assertEqual(CHECK.fired(TRIGGERS, CHECK.changes(DIFF)), [
            "crypto-dependency: rust/ffi/Cargo.toml",
            "protocol: proto/node.proto",
            "unsafe: rust/a b/x.rs",
        ])

    def test_path_and_line_rules(self):
        files = {"rust/bootstrap/src/storage.rs":
                 ["conn.execute(\"CREATE TABLE IF NOT EXISTS `deny`\")"],
                 "tests/scripts/triggers.toml": [],
                 "rust/identity/src/lib.rs": []}
        self.assertEqual(CHECK.fired(TRIGGERS, files), [
            "checks: tests/scripts/triggers.toml",
            "identity: rust/identity/src/lib.rs",
            "storage-migration: rust/bootstrap/src/storage.rs",
        ])

    def test_uncovered_glob(self):
        triggers = CHECK.load('[[trigger]]\nname = "x"\n'
                              'paths = ["clippy.toml", "src/*"]\n')
        self.assertEqual(CHECK.uncovered(triggers, ["src/lib.rs"]),
                         ["x: clippy.toml"])

    def test_list_covers_itself(self):
        self.assertIn("checks: tests/scripts/triggers.toml", CHECK.fired(
            TRIGGERS, {"tests/scripts/triggers.toml": []}))


if __name__ == "__main__":
    unittest.main()
