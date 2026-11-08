"""Keep complexity budgets attached to the actual offending functions."""
import copy
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "rust_complexity", ROOT / "tests/scripts/rust_complexity.py")
GATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GATE)


def fixture():
    source = b"fn old() { if true {} }"
    entry = {"id": "fn old", "kind": "function",
             "start": [1, 0], "end": [1, len(source)], "nesting": 6}
    finding = {
        "code": "clippy::too_many_lines",
        "span": {"file_name": "src/lib.rs", "byte_start": 3,
                 "byte_end": 6, "is_primary": True},
        "message": "this function has too many lines (80/50)"}
    return source, entry, finding


def complete(finding):
    return [
        {"reason": "compiler-artifact", "package_id": "fixture",
         "target": {"kind": ["lib"]}},
        {"reason": "compiler-message", "package_id": "fixture",
         "message": {"level": "warning", "code": {"code": finding["code"]},
                     "message": finding["message"],
                     "spans": [finding["span"]]}},
        {"reason": "build-finished", "success": True}]


class RustComplexityTest(unittest.TestCase):
    def measure(self, source, entries, finding):
        return GATE.measure([finding], {"src/lib.rs": source},
                            {"src/lib.rs": entries})

    def test_new_function_cannot_replace_old_with_equal_total(self):
        old = {("src/lib.rs", "fn old"): {"lines": 80}}
        new = {("src/lib.rs", "fn new"): {"lines": 80}}
        errors = GATE.errors(new, old)
        self.assertEqual(len(errors), 2)
        self.assertTrue(any("80 > 0" in error for error in errors))

    def test_improvements_require_a_smaller_recorded_budget(self):
        key = ("src/lib.rs", "fn old")
        actual = {key: {"lines": 65}}
        self.assertTrue(GATE.errors(actual, {key: {"lines": 80}}))
        self.assertEqual(GATE.errors(actual, {key: {"lines": 65}}), [])
        self.assertTrue(GATE.errors(actual, {key: {"lines": 60}}))

    def test_budget_edits_cannot_authorize_a_new_offender(self):
        key = ("src/lib.rs", "fn old")
        old = {key: {"lines": 80}}
        self.assertEqual(GATE.growth_errors({key: {"lines": 65}}, old), [])
        self.assertTrue(GATE.growth_errors({key: {"lines": 81}}, old))
        new = {("src/lib.rs", "fn new"): {"lines": 80}}
        self.assertTrue(GATE.growth_errors(new, old))

    def test_nested_regions_and_depth_are_both_bounded(self):
        key = ("src/lib.rs", "fn old")
        budget = {key: {"nesting": 6, "nested_regions": 1}}
        for metric in ("nesting", "nested_regions"):
            current = copy.deepcopy(budget)
            current[key][metric] += 1
            self.assertTrue(GATE.errors(current, budget))

    def test_anonymous_closure_cannot_borrow_parent_cognitive_budget(self):
        source, entry, finding = fixture()
        closure = dict(entry, id="closure", kind="closure",
                       start=[1, 3], end=[1, 6])
        finding["code"] = "clippy::cognitive_complexity"
        finding["message"] = (
            "the function has a cognitive complexity of (16/15)")
        with self.assertRaisesRegex(ValueError, "anonymous closure"):
            self.measure(source, [entry, closure], finding)
        finding["code"] = "clippy::excessive_nesting"
        self.assertEqual(self.measure(source, [entry, closure], finding),
                         {("src/lib.rs", "fn old"):
                          {"nesting": 6, "nested_regions": 1}})

    def test_missing_ambiguous_and_outdated_spans_fail(self):
        source, entry, finding = fixture()
        for entries in ([], [entry, entry]):
            with self.assertRaises(ValueError):
                self.measure(source, entries, finding)
        finding["span"]["byte_end"] = len(source) + 1
        with self.assertRaisesRegex(ValueError, "outside"):
            self.measure(source, [entry], finding)

    def test_repeated_canonical_span_does_not_inflate_nesting_regions(self):
        source, entry, finding = fixture()
        finding["code"] = "clippy::excessive_nesting"
        result = GATE.measure(
            [finding, copy.deepcopy(finding)], {"src/lib.rs": source},
            {"src/lib.rs": [entry]})
        self.assertEqual(result[("src/lib.rs", "fn old")]["nested_regions"], 1)

    def test_threshold_overrides_are_rejected(self):
        source, entry, finding = fixture()
        finding["message"] = "this function has too many lines (80/100)"
        with self.assertRaisesRegex(ValueError, "overridden"):
            self.measure(source, [entry], finding)

    def test_complete_build_and_duplicate_messages(self):
        _, _, finding = fixture()
        records = complete(finding)
        records.insert(2, copy.deepcopy(records[1]))
        self.assertEqual(len(GATE.diagnostics(
            {"workspace_members": ["fixture"]}, records)), 1)

    def test_incomplete_failed_and_error_reports_fail(self):
        _, _, finding = fixture()
        records = complete(finding)
        cases = [records[:-1], records[1:]]
        failed = copy.deepcopy(records)
        failed[-1]["success"] = False
        cases.append(failed)
        errored = copy.deepcopy(records)
        errored[1]["message"]["level"] = "error"
        cases.append(errored)
        for case in cases:
            with self.assertRaises(ValueError):
                GATE.diagnostics({"workspace_members": ["fixture"]}, case)


if __name__ == "__main__":
    unittest.main()
