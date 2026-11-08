"""A5: dependency, crate, wire-format and schema changes name an ADR."""
import importlib.util
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "check_adr", ROOT / "tests/scripts/check-adr.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
CRATE = '[package]\nname = "x"\n\n[dependencies]\nserde = "1"\n'


class Adr(unittest.TestCase):
    def test_triggers(self):
        bumped = CRATE.replace('"1"', '"1.1"')
        added = (CRATE + 'own = { path = "../own" }\n'
                 'prost.workspace = true\n'
                 '[dev-dependencies]\nproptest = "1"\n')
        gradle = "implementation 'a.b:c:1'\n"
        self.assertEqual(CHECK.triggers("x/Cargo.toml", "", CRATE, ""), [
            "new crate x/Cargo.toml", "new dependency serde in x/Cargo.toml"])
        self.assertEqual(CHECK.triggers("x/Cargo.toml", CRATE, bumped, ""),
                         [])
        self.assertEqual(CHECK.triggers("x/Cargo.toml", CRATE, added, ""),
                         ["new dependency proptest in x/Cargo.toml"])
        self.assertEqual(CHECK.triggers(
            "app/build.gradle", gradle,
            gradle.replace(":1", ":2") + "testImplementation 'd:e:1'\n", ""),
            ["new dependency d:e in app/build.gradle"])
        self.assertEqual(CHECK.triggers("proto/node.proto", "a", "b", ""),
                         ["wire format proto/node.proto"])
        schema = "+  `size` INTEGER NOT NULL,\n-// old comment\n"
        self.assertEqual(CHECK.triggers("rust/x/src/db.rs", "", "", schema),
                         ["storage schema rust/x/src/db.rs"])
        self.assertEqual(CHECK.triggers(
            "rust/x/src/db.rs", "", "", "+let size = 1;\n"), [])

    def test_parser_exception_does_not_exempt_application_dependencies(self):
        path = "tools/rust-functions/Cargo.toml"
        parser = ('[package]\nname = "dyapp-function-map"\n'
                  '[dependencies]\nsyn = "2"\nquote = "1"\n'
                  'serde_json = "1"\nproc-macro2 = "1"\n')
        self.assertEqual(CHECK.triggers(path, "", parser, ""), [])
        self.assertTrue(CHECK.triggers("rust/app/Cargo.toml", "", parser, ""))
        self.assertEqual(CHECK.triggers(
            path, parser, parser + 'other = "1"\n', ""), [
                f"new dependency other in {path}"])
        self.assertEqual(CHECK.triggers(
            "tools/rust-functions/src/main.rs", "", "",
            "+ CREATE TABLE items"), [
                "storage schema tools/rust-functions/src/main.rs"])

    def test_trailer(self):
        adr = "docs/decisions/0001-record-architecture-decisions.md"
        for message, error in [
                (f"x\n\nADR: {adr}", None),
                ("x\n\nADR: docs/decisions/9999-missing.md",
                 "names a missing ADR"),
                ("x\n\nADR: none — lint tool, no app architecture", None),
                ("x\n\nADR: none - lint tool, no app architecture", None),
                ("x\n\nADR: none", "has no ADR: trailer"),
                ("x\n\nADR: none — ok", "has no ADR: trailer"),
                ("x", "has no ADR: trailer")]:
            self.assertEqual(CHECK.trailer_error(message, "HEAD"), error,
                             message)

    def test_unpushed_commits_name_their_adr(self):
        shas = subprocess.check_output(
            CHECK.GIT + ["rev-list", "--no-merges", CHECK.base() + "..HEAD"],
            cwd=ROOT, text=True).split()
        self.assertEqual(
            [error for sha in shas for error in CHECK.commit_errors(sha)], [])


if __name__ == "__main__":
    unittest.main()
