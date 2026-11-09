"""The prepared quality image must reject unformatted web sources."""

from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class WebFormatterTest(unittest.TestCase):
    def test_format_check_rejects_drift(self) -> None:
        for source, expected in (("const x=1;\n", 1), ("const x = 1;\n", 0)):
            with self.subTest(source=source):
                with tempfile.TemporaryDirectory(dir=ROOT / "build") as tmp:
                    directory = Path(tmp)
                    (directory / "biome.jsonc").write_text(
                        (ROOT / "biome.jsonc").read_text()
                    )
                    fixture = directory / "clients/desktop/ui/fixture.js"
                    fixture.parent.mkdir(parents=True)
                    fixture.write_text(source)
                    result = subprocess.run(
                        ["biome", "format", "--colors=off", str(fixture)],
                        cwd=directory,
                        text=True,
                        capture_output=True,
                        check=False,
                    )
                self.assertEqual(
                    result.returncode, expected, result.stdout + result.stderr
                )


class WebComplexityTest(unittest.TestCase):
    def test_configured_limits_reject_oversized_functions(self) -> None:
        fixtures = [
            (
                "noExcessiveLinesPerFunction",
                "window.value = 1;\n" * count,
                0 if count == 49 else 1,
            )
            for count in (49, 51)
        ]
        fixtures += [
            (
                "noExcessiveCognitiveComplexity",
                "if (flag) {\n" * depth
                + "window.value = 1;\n"
                + "}\n" * depth,
                0 if depth == 3 else 1,
            )
            for depth in (3, 6)
        ]
        for rule, body, expected in fixtures:
            with self.subTest(rule=rule, expected=expected):
                with tempfile.TemporaryDirectory(dir=ROOT / "build") as tmp:
                    directory = Path(tmp)
                    (directory / "biome.jsonc").write_text(
                        (ROOT / "biome.jsonc").read_text()
                    )
                    fixture = directory / "clients/desktop/ui/contact-notes.js"
                    fixture.parent.mkdir(parents=True)
                    fixture.write_text(
                        "".join(
                            ("window.checked = (flag) => {\n", body, "};\n")
                        )
                    )
                    result = subprocess.run(
                        [
                            "biome",
                            "lint",
                            "--only=complexity/" + rule,
                            str(fixture),
                        ],
                        cwd=directory,
                        text=True,
                        capture_output=True,
                        check=False,
                    )
                self.assertEqual(
                    result.returncode, expected, result.stdout + result.stderr
                )


if __name__ == "__main__":
    unittest.main()
