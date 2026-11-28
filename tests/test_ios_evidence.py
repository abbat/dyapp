"""Contract tests for the iOS screenshot export inventory."""

import importlib.util
import json
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import TextIO
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "apple_test_in_ci", ROOT / "tests/scripts/apple-test-in-ci.py"
)
assert SPEC is not None and SPEC.loader is not None, (
    "Python source loader unavailable"
)
APPLE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(APPLE)


class IosEvidence(unittest.TestCase):
    def test_timeout_reports_only_the_captured_tail(self) -> None:
        build = ROOT / "build"
        build.mkdir(exist_ok=True)
        with TemporaryDirectory(dir=build) as temporary:
            log = Path(temporary) / "tests.log"

            def timeout(
                arguments: list[str], *, stdout: TextIO, **_options: object
            ) -> None:
                stdout.write(f"Earlier output\n{'x' * 4096}\nLast test\n")
                raise APPLE.subprocess.TimeoutExpired(arguments, 1)

            with (
                patch.object(APPLE.subprocess, "run", side_effect=timeout),
                self.assertRaisesRegex(
                    RuntimeError, "Command timed out"
                ) as caught,
            ):
                APPLE.run(["xcodebuild"], log, timeout=1)
            self.assertIn("Last test", str(caught.exception))
            self.assertNotIn("Earlier output", str(caught.exception))
            self.assertTrue(log.read_text().startswith("Earlier output"))

    def test_export_rejects_a_different_checkout(self) -> None:
        build = ROOT / "build"
        build.mkdir(exist_ok=True)
        with TemporaryDirectory(dir=build) as temporary:
            reports = Path(temporary)
            app = reports / "Debug-iphonesimulator" / "DYApp.app"
            app.mkdir(parents=True)
            (app / "DYApp").write_bytes(b"test executable")
            (reports / "attachments").mkdir()
            with (
                patch.object(APPLE, "run"),
                patch.object(APPLE, "validate_ios_attachments"),
                patch.object(
                    APPLE.subprocess, "check_output", return_value="checkout\n"
                ),
                patch.dict(APPLE.os.environ, {"GITHUB_SHA": "workflow"}),
                self.assertRaisesRegex(
                    RuntimeError, "differs from the CI source"
                ),
            ):
                APPLE.export_ios_evidence(
                    reports, reports / "test.xcresult", [app], "simulator"
                )
            self.assertFalse(
                (reports / "attachments" / "provenance.json").exists()
            )

    def test_unrelated_png_cannot_replace_the_missing_state(self) -> None:
        build = ROOT / "build"
        build.mkdir(exist_ok=True)
        with TemporaryDirectory(dir=build) as temporary:
            directory = Path(temporary)
            attachments = []
            for index, state in enumerate(("eligible-target", "sent")):
                stem = f"ios-profile-share-en-{state}"
                for name, suffix, content in (
                    (stem, ".png", b"\x89PNG\r\n\x1a\n"),
                    (f"{stem}-geometry", ".txt", b"element=(0, 0, 20, 20)"),
                ):
                    exported = f"{index}{suffix}"
                    readable_name = f"{name}_0_1{suffix}"
                    (directory / exported).write_bytes(content)
                    attachments.append(
                        {
                            "exportedFileName": exported,
                            "suggestedHumanReadableName": readable_name,
                        }
                    )
            manifest = directory / "manifest.json"
            manifest.write_text(json.dumps([{"attachments": attachments}]))
            digests = APPLE.validate_ios_attachments(directory, "en")
            self.assertEqual(
                set(digests), {"0.png", "0.txt", "1.png", "1.txt"}
            )
            with self.assertRaisesRegex(
                RuntimeError, "ios-profile-share-ru-eligible-target"
            ):
                APPLE.validate_ios_attachments(directory, "ru")
            attachments[2]["suggestedHumanReadableName"] = "unrelated.png"
            manifest.write_text(json.dumps([{"attachments": attachments}]))
            with self.assertRaisesRegex(
                RuntimeError, "ios-profile-share-en-sent"
            ):
                APPLE.validate_ios_attachments(directory, "en")
