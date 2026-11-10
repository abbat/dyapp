"""Check real SARIF conversion across duplicate and failed Cargo scans."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


def diagnostic(
    line: int = 7,
    column: int = 3,
    text: str = "too many lines",
    level: str = "warning",
) -> dict[str, object]:
    return {
        "reason": "compiler-message",
        "message": {
            "level": level,
            "code": {"code": "clippy::too_many_lines"},
            "message": text,
            "spans": [
                {
                    "is_primary": True,
                    "file_name": "rust/example.rs",
                    "line_start": line,
                    "column_start": column,
                }
            ],
        },
    }


class ClippySarifTests(unittest.TestCase):
    def convert(
        self, rows: list[dict[str, object]]
    ) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "cargo.jsonl"
            source.write_text("\n".join(json.dumps(row) for row in rows))
            return subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "tests/scripts/clippy-to-sarif.py"),
                    str(source),
                ],
                capture_output=True,
                text=True,
                check=False,
            )

    def results(
        self, rows: list[dict[str, object]]
    ) -> list[dict[str, object]]:
        process = self.convert(
            rows + [{"reason": "build-finished", "success": True}]
        )
        self.assertEqual(process.returncode, 0, process.stderr)
        report = json.loads(process.stdout)
        self.assertEqual(report["version"], "2.1.0")
        return list(report["runs"][0]["results"])

    def test_library_and_test_target_share_one_source_finding(self) -> None:
        library = diagnostic() | {"target": {"kind": ["lib"]}}
        tests = diagnostic() | {"target": {"kind": ["test"]}}
        self.assertEqual(len(self.results([library, tests])), 1)

    def test_distinct_text_location_and_severity_are_preserved(self) -> None:
        rows = [
            diagnostic(),
            diagnostic(line=8),
            diagnostic(column=4),
            diagnostic(text="different measured finding"),
            diagnostic(level="error"),
        ]
        self.assertEqual(len(self.results(rows)), len(rows))

    def test_incomplete_and_failed_scans_emit_no_partial_sarif(self) -> None:
        for final in ([], [{"reason": "build-finished", "success": False}]):
            with self.subTest(final=final):
                process = self.convert([diagnostic(), *final])
                self.assertNotEqual(process.returncode, 0)
                self.assertEqual(process.stdout, "")
                self.assertIn(
                    "Collector failed or did not finish", process.stderr
                )

    def test_zero_coordinates_are_valid_sarif_positions(self) -> None:
        result = self.results([diagnostic(line=0, column=0)])[0]
        self.assertEqual(
            result["locations"],
            [
                {
                    "physicalLocation": {
                        "artifactLocation": {"uri": "rust/example.rs"},
                        "region": {"startLine": 1, "startColumn": 1},
                    }
                }
            ],
        )


if __name__ == "__main__":
    unittest.main()
