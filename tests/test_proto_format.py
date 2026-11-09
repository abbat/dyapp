"""Check the pinned Protobuf formatter with canonical and drifting input."""
import shutil
import subprocess
import unittest


@unittest.skipUnless(shutil.which("buf"), "buf runs in the quality container")
class ProtoFormatTests(unittest.TestCase):
    def test_gate_rejects_drift_and_accepts_formatted_input(self) -> None:
        source = ('syntax="proto3";package example;'
                  'message Item{string value=1;}')
        formatted = subprocess.run(
            ["buf", "format", "--stdin-filepath", "fixture.proto"],
            input=source, text=True,
            capture_output=True, check=False)
        self.assertEqual(formatted.returncode, 0, formatted.stderr)
        self.assertNotEqual(formatted.stdout, source)
        for contents, expected in ((source, False), (formatted.stdout, True)):
            with self.subTest(formatted=expected):
                checked = subprocess.run(
                    ["buf", "format", "--stdin-filepath", "fixture.proto",
                     "--diff", "--exit-code"],
                    input=contents, text=True,
                    capture_output=True, check=False)
                self.assertEqual(checked.returncode == 0, expected,
                                 checked.stderr)
                if not expected:
                    self.assertIn("@@", checked.stdout)
