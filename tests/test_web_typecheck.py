"""The web type gate rejects invalid IPC calls and nullable values."""

from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class WebTypecheckTest(unittest.TestCase):
    def test_command_and_null_contracts(self) -> None:
        bridge = (ROOT / "clients/desktop/ui/ipc.js").as_posix()
        cases = (
            (
                "invoke('quit'); invoke('remove_profile_photo', "
                "{index: 0, revision: []});",
                0,
            ),
            ("invoke('unknown_command');", 1),
            (
                "invoke('remove_profile_photo', "
                "{index: 'zero', revision: []});",
                1,
            ),
            ("invoke('remove_profile_photo');", 1),
            (
                "/** @type {string | null} */ let value = null; "
                "value.toUpperCase();",
                1,
            ),
        )
        for source, expected in cases:
            with self.subTest(source=source):
                with tempfile.TemporaryDirectory(dir=ROOT / "build") as tmp:
                    fixture = Path(tmp) / "contract.js"
                    fixture.write_text(
                        f"import {{ invoke }} from '{bridge}';\n{source}\n"
                    )
                    result = subprocess.run(
                        [
                            "tsc",
                            "--allowJs",
                            "--checkJs",
                            "--noEmit",
                            "--strict",
                            "--target",
                            "es2022",
                            "--module",
                            "esnext",
                            "--lib",
                            "es2022,dom",
                            "--pretty",
                            "false",
                            str(fixture),
                        ],
                        cwd=ROOT,
                        text=True,
                        capture_output=True,
                        check=False,
                    )
                self.assertEqual(
                    result.returncode, expected, result.stdout + result.stderr
                )
                if expected:
                    self.assertIn("contract.js(", result.stdout)
                    self.assertIn("error TS", result.stdout)


if __name__ == "__main__":
    unittest.main()
