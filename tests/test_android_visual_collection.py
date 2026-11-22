"""Separate completed native archives from remote shell diagnostics."""

import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tests/scripts/android-visual-collect.sh"
ADB = """#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys
args = sys.argv[1:]
with open(os.environ["COMMAND_LOG"], "a") as log:
    log.write(json.dumps(args) + "\\n")
mode = os.environ["EXPORT_MODE"]
if args[2:5] == ["shell", "run-as", "com.dyapp"]:
    assert args[5] == "tar" and args[6] == "-cf"
    if mode == "missing":
        print("capture directory is absent", file=sys.stderr)
        sys.exit(1)
    if mode == "denied":
        print("remote archive creation denied", file=sys.stderr)
        sys.exit(13)
    print("remote shell stdout diagnostic")
    print("remote shell stderr diagnostic", file=sys.stderr)
elif args[2:5] == ["exec-out", "run-as", "com.dyapp"]:
    assert args[5] == "cat"
    if mode == "broken-transfer":
        sys.stdout.buffer.write(b"truncated transfer")
        print("binary transfer failed", file=sys.stderr)
        sys.exit(17)
    sys.stdout.buffer.write(Path(os.environ["PAYLOAD"]).read_bytes())
else:
    raise AssertionError(args)
"""


class AndroidVisualCollection(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(dir=ROOT / "build")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.report = self.root / "reports with spaces"
        self.report.mkdir()
        self.adb = self.root / "adb"
        self.adb.write_text(ADB)
        self.adb.chmod(0o755)
        self.commands = self.root / "commands.jsonl"
        self.payload = self.root / "payload.tar"
        with tarfile.open(self.payload, "w") as archive:
            body = b'{"partial": true}'
            member = tarfile.TarInfo(
                "files/owner-approval-visual/manifest.json"
            )
            member.size = len(body)
            archive.addfile(member, io.BytesIO(body))

    def run_export(
        self,
        mode: str,
        directory: str = "files/owner-approval-visual",
        name: str = "native",
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["bash", str(SCRIPT), str(self.report), directory, name],
            env={
                **os.environ,
                "PATH": str(self.root) + os.pathsep + os.environ["PATH"],
                "COMMAND_LOG": str(self.commands),
                "PAYLOAD": str(self.payload),
                "EXPORT_MODE": mode,
            },
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        )

    def calls(self) -> list[list[str]]:
        return [
            json.loads(line) for line in self.commands.read_text().splitlines()
        ]

    def test_remote_diagnostics_cannot_contaminate_completed_archive(
        self,
    ) -> None:
        result = self.run_export("success")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            (self.report / "native.tar").read_bytes(),
            self.payload.read_bytes(),
        )
        diagnostics = (self.report / "native-collection-error.txt").read_text()
        self.assertIn("stdout diagnostic", diagnostics)
        self.assertIn("stderr diagnostic", diagnostics)
        self.assertEqual(
            [call[2] for call in self.calls()], ["shell", "exec-out"]
        )
        self.assertEqual(self.calls()[0][-2], self.calls()[1][-1])

    def test_unentered_phase_has_diagnostics_instead_of_a_fake_archive(
        self,
    ) -> None:
        result = self.run_export("missing")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.report / "native.tar").exists())
        self.assertIn(
            "capture directory is absent",
            (self.report / "native-collection-error.txt").read_text(),
        )
        self.assertEqual(len(self.calls()), 1)

    def test_creation_failure_does_not_start_transfer(self) -> None:
        result = self.run_export("denied")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.report / "native.tar").exists())
        self.assertEqual(len(self.calls()), 1)

    def test_real_transfer_failure_stays_nonzero_with_partial_evidence(
        self,
    ) -> None:
        result = self.run_export("broken-transfer")
        self.assertEqual(result.returncode, 17)
        archive = self.report / "native.tar"
        self.assertEqual(archive.read_bytes(), b"truncated transfer")
        with self.assertRaises(tarfile.ReadError):
            with tarfile.open(archive):
                self.fail("corrupt transfer unexpectedly opened")
        self.assertIn(
            "binary transfer failed",
            (self.report / "native-collection-error.txt").read_text(),
        )

    def test_invalid_paths_never_reach_adb(self) -> None:
        for directory, name in (
            ("../outside", "native"),
            ("files/capture;command", "native"),
            ("files/owner-approval-visual", "../outside"),
        ):
            with self.subTest(directory=directory, name=name):
                result = self.run_export("success", directory, name)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.commands.exists())


if __name__ == "__main__":
    unittest.main()
