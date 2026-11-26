"""Synthetic coverage for the untrusted-input inventory and guard gate."""

import importlib.util
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests/scripts"))
SPEC = importlib.util.spec_from_file_location(
    "check_untrusted_inputs", ROOT / "tests/scripts/check-untrusted-inputs.py"
)
assert SPEC is not None and SPEC.loader is not None, (
    "Python source loader is unavailable"
)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


def entry(
    name: str,
    start: int,
    end: int,
    test: bool = False,
) -> dict[str, object]:
    prefix = "mod tests # [cfg (test)]::" if test else ""
    return {
        "id": f"{prefix}fn {name} ",
        "kind": "function",
        "start": [start, 0],
        "end": [end, 1],
    }


def commands(text: str = "    identity_state,\n") -> str:
    return (
        "const GENERATED_COMMAND_HANDLER: fn() = "
        f"tauri::generate_handler![\n{text}];\n"
    )


class UntrustedInputs(unittest.TestCase):
    def test_production_families_and_multiline_ffi_are_found(self) -> None:
        source = """#[uniffi::export]
pub fn pasted(
    typed: Option<String>,
    bytes: Vec<u8>,
) -> Option<String> { None }
fn network() {
    Request::decode(bytes);
    image::load_from_memory(bytes);
    sticker_tar::read(bytes);
    quircs::Quirc::default().identify(1, 1, pixels);
}
#[cfg(test)]
mod tests {
    #[test]
    fn ignored() { Request::decode(bytes); }
}
"""
        sources = {
            "rust/ffi/src/input.rs": source,
            CHECK.COMMANDS: commands(
                '    #[cfg(feature = "ui-smoke")]\n'
                "    safety::scan_contact_qr,\n"
            ),
        }
        functions = {
            "rust/ffi/src/input.rs": [
                entry("pasted", 1, 6),
                entry("network", 6, 11),
                entry("ignored", 15, 15, test=True),
            ],
            CHECK.COMMANDS: [],
        }
        self.assertEqual(
            CHECK.scan_sources(sources, functions),
            {
                ("uniffi", "rust/ffi/src/input.rs#pasted"),
                ("prost", "rust/ffi/src/input.rs#network"),
                ("image", "rust/ffi/src/input.rs#network"),
                ("archive", "rust/ffi/src/input.rs#network"),
                ("qr", "rust/ffi/src/input.rs#network"),
                ("tauri", f"{CHECK.COMMANDS}#safety::scan_contact_qr"),
            },
        )

    def test_return_only_string_and_external_tests_are_not_inputs(
        self,
    ) -> None:
        sources = {
            "rust/ffi/src/output.rs": (
                "#[uniffi::export]\n"
                "pub fn output() -> String { String::new() }\n"
                '#[cfg(test)]\n#[path = "output_tests.rs"]\nmod tests;\n'
            ),
            "rust/ffi/src/output_tests.rs": (
                "#[test]\nfn decodes() { Request::decode(bytes); }\n"
            ),
            "rust/client/tests/decoder.rs": (
                "proptest! { fn decodes() { Request::decode(bytes); } }\n"
            ),
            CHECK.COMMANDS: commands(),
        }
        functions = {
            "rust/ffi/src/output.rs": [entry("output", 1, 2)],
            "rust/ffi/src/output_tests.rs": [
                entry("decodes", 1, 2, test=True)
            ],
            "rust/client/tests/decoder.rs": [],
            CHECK.COMMANDS: [],
        }
        self.assertEqual(
            CHECK.scan_sources(sources, functions),
            {("tauri", f"{CHECK.COMMANDS}#identity_state")},
        )

    def test_unknown_method_decode_and_module_decode_fail_closed(self) -> None:
        source = "fn parse() { decoder.decode(); }\n"
        sources = {
            "rust/client/src/input.rs": source,
            CHECK.COMMANDS: commands(),
        }
        functions = {
            "rust/client/src/input.rs": [entry("parse", 1, 1)],
            CHECK.COMMANDS: [],
        }
        self.assertIn(
            ("decode", "rust/client/src/input.rs#parse"),
            CHECK.scan_sources(sources, functions),
        )
        sources["rust/client/src/input.rs"] = (
            "const X: u8 = decoder.decode();\n"
        )
        functions["rust/client/src/input.rs"] = []
        with self.assertRaisesRegex(ValueError, "outside a named function"):
            CHECK.scan_sources(sources, functions)

    def test_malformed_command_inventory_fails(self) -> None:
        self.assertTrue(CHECK.command_hits(commands("    identity_state\n")))
        sources = {
            CHECK.COMMANDS: commands(
                "    identity_state\n    storage_options,\n"
            )
        }
        with self.assertRaisesRegex(ValueError, "malformed command line"):
            CHECK.command_hits(sources[CHECK.COMMANDS])
        with self.assertRaisesRegex(ValueError, "missing generate_handler"):
            CHECK.command_hits("fn main() {}")

    def row(self, kind: str, ref: str) -> dict[str, str]:
        return {
            "input": "prost",
            "location": "rust/client/src/input.rs#parse",
            "guard_kind": kind,
            "guard_ref": ref,
        }

    def fixture(
        self,
    ) -> tuple[dict[str, str], dict[str, list[dict[str, object]]]]:
        sources = {
            "rust/client/src/input.rs": (
                "const MAX_INPUT: usize = 64;\n"
                "fn parse() {}\n"
                "#[cfg(test)]\nmod tests {\n"
                "#[test]\nfn rejects_bad_input() {}\n}\n"
            ),
            "tests/fuzz/fuzz_targets/input.rs": (
                "libfuzzer_sys::fuzz_target!(|data: &[u8]| {});\n"
            ),
        }
        functions = {
            "rust/client/src/input.rs": [
                entry("parse", 2, 2),
                entry("rejects_bad_input", 5, 6, test=True),
            ]
        }
        return sources, functions

    def test_each_guard_kind_is_resolved(self) -> None:
        sources, functions = self.fixture()
        hit = {("prost", "rust/client/src/input.rs#parse")}
        rows = [
            self.row("fuzz", "tests/fuzz/fuzz_targets/input.rs"),
            self.row("test", "rust/client/src/input.rs#rejects_bad_input"),
            self.row("limit", "rust/client/src/input.rs#MAX_INPUT"),
            self.row("allow-list", "rust/client/src/input.rs#parse"),
        ]
        for row in rows:
            with self.subTest(kind=row["guard_kind"]):
                self.assertEqual(
                    CHECK.inventory_errors([row], hit, sources, functions), []
                )

    def test_external_test_guard_uses_parent_module_context(self) -> None:
        sources = {
            "rust/client/src/input.rs": (
                '#[cfg(test)]\n#[path = "input_tests.rs"]\nmod tests;\n'
            ),
            "rust/client/src/input_tests.rs": (
                "#[test]\nfn rejects_bad_input() {}\n"
            ),
        }
        functions = {
            "rust/client/src/input.rs": [],
            "rust/client/src/input_tests.rs": [
                entry("rejects_bad_input", 1, 2)
            ],
        }
        row = self.row(
            "test", "rust/client/src/input_tests.rs#rejects_bad_input"
        )
        hit = {("prost", "rust/client/src/input.rs#parse")}
        self.assertEqual(
            CHECK.inventory_errors([row], hit, sources, functions), []
        )

    def test_missing_stale_duplicate_and_removed_guards_fail(self) -> None:
        sources, functions = self.fixture()
        hit = {("prost", "rust/client/src/input.rs#parse")}
        valid = self.row("fuzz", "tests/fuzz/fuzz_targets/input.rs")
        self.assertTrue(CHECK.inventory_errors([], hit, sources, functions))
        self.assertTrue(
            CHECK.inventory_errors([valid], set(), sources, functions)
        )
        self.assertTrue(
            CHECK.inventory_errors([valid, valid], hit, sources, functions)
        )
        sources["tests/fuzz/fuzz_targets/input.rs"] = (
            "fn no_longer_fuzz() {}\n"
        )
        self.assertTrue(
            CHECK.inventory_errors([valid], hit, sources, functions)
        )
        sources, functions = self.fixture()
        test = self.row("test", "rust/client/src/input.rs#rejects_bad_input")
        functions["rust/client/src/input.rs"] = [entry("parse", 2, 2)]
        self.assertTrue(
            CHECK.inventory_errors([test], hit, sources, functions)
        )
        limit = self.row("limit", "rust/client/src/input.rs#MAX_INPUT")
        sources["rust/client/src/input.rs"] = "fn parse() {}\n"
        self.assertTrue(
            CHECK.inventory_errors([limit], hit, sources, functions)
        )
        allow = self.row("allow-list", "rust/client/src/input.rs#parse")
        sources["rust/client/src/input.rs"] = "fn other() {}\n"
        self.assertTrue(
            CHECK.inventory_errors([allow], hit, sources, functions)
        )


if __name__ == "__main__":
    unittest.main()
