"""B3 round-trip inventory: every protobuf and serde type has a property."""
import importlib.util
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "repository_round_trips", ROOT / "tests/scripts/check-repository.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
SERDE = "#[derive(Debug, Serialize, Deserialize)]\npub struct Stored {}\n"


class RoundTrips(unittest.TestCase):
    def test_untested_protobuf_and_serde_types_fail(self):
        texts = {
            "proto/a.proto": "message Tested {}\n  enum Missing { A = 0; }\n",
            "rust/x/src/deep/mod.rs": SERDE,
            "rust/x/src/only_out.rs": "#[derive(Serialize)]\nstruct Out;\n",
            "rust/x/tests/t.rs": SERDE.replace("Stored", "TestOnly"),
            "rust/bootstrap/src/config.rs": SERDE.replace("Stored", "Config"),
            CHECK.PROPERTIES: "protobuf_round_trip! {\n    t: Tested,\n}\n",
        }
        self.assertEqual(CHECK.round_trip_errors(texts), [
            "proto/a.proto: Missing has no round-trip property",
            "rust/x/src/deep/mod.rs: Stored has no round-trip property",
        ])
        texts[CHECK.PROPERTIES] += (
            "json_round_trip! {\n    m: Missing,\n"
            "    s: Stored = any::<Stored>(),\n}\n")
        self.assertEqual(CHECK.round_trip_errors(texts), [])

    def test_repository_types_all_have_properties(self):
        names = subprocess.check_output(
            ["git", "-c", "safe.directory=*", "ls-files", "proto", "rust"],
            cwd=ROOT, text=True).split()
        texts = {name: (ROOT / name).read_text() for name in names
                 if name.endswith((".proto", ".rs"))}
        self.assertEqual(CHECK.round_trip_errors(texts), [])
        tested = CHECK.PROPERTY.findall(texts[CHECK.PROPERTIES])
        self.assertGreater(len(tested), 50)


if __name__ == "__main__":
    unittest.main()
