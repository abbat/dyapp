"""Check capture orchestration; real rendering remains native CI acceptance."""

from collections.abc import Mapping, Sequence
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[1]
NATIVE = runpy.run_path(str(ROOT / "tests/scripts/desktop_smoke_native.py"))
PANELS = NATIVE["moderation_panels"]
SMOKE = runpy.run_path(str(ROOT / "tests/scripts/desktop-smoke.py"))


class ModerationEvidence(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(dir=ROOT / "build")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.executable = self.root / "app"
        self.executable.write_bytes(b"controlled native executable")
        self.calls: list[tuple[str, list[str], Path, dict[str, str]]] = []
        self.omit = False
        self.empty = False
        self.failure = ""

    def render(
        self,
        name: str,
        flags: Sequence[str],
        profile: str | Path,
        **options: object,
    ) -> str:
        env = options["extra_env"]
        assert isinstance(env, Mapping)
        values = {str(key): str(value) for key, value in env.items()}
        self.assertTrue(Path(profile).is_dir())
        self.calls.append((name, list(flags), Path(profile), values))
        if name == self.failure:
            raise RuntimeError("controlled native refusal")
        output = values.get("DYAPP_UI_SHOTS")
        if output:
            language = name.removeprefix("moderation-")
            for panel in ("blocked", "passed"):
                for width in (440, 1280):
                    if self.omit and panel == "blocked":
                        continue
                    body = b"" if self.empty else b"controlled PNG bytes"
                    file = (
                        Path(output)
                        / f"settings-{panel}-{language}-{width}.png"
                    )
                    file.write_bytes(body)
        return "peer"

    def run_panels(self, platform: str = "linux") -> None:
        with patch.object(NATIVE["sys"], "platform", platform):
            PANELS(self.render, self.root, str(self.executable))

    def receipts(self) -> list[Path]:
        return list(self.root.glob("moderation-shots-*/receipt.json"))

    def test_linux_requires_both_locales_and_all_eight_captures(self) -> None:
        self.run_panels()
        receipt = json.loads(self.receipts()[0].read_text())
        self.assertEqual(len(receipt["captures"]), 8)
        self.assertEqual(
            receipt["executable_sha256"],
            hashlib.sha256(self.executable.read_bytes()).hexdigest(),
        )
        self.assertEqual(
            [call[0] for call in self.calls],
            ["moderation-en", "moderation-ru"],
        )
        self.assertNotIn("--ui-test-ru", self.calls[0][1])
        self.assertIn("--ui-test-ru", self.calls[1][1])
        self.assertNotEqual(self.calls[0][2], self.calls[1][2])
        for call in self.calls:
            self.assertEqual(call[3]["DYAPP_SMOKE_RU"], "0")
            self.assertIn("--ui-test-step=settings-moderation", call[1])
            self.assertFalse(call[2].exists())
        for name, digest in receipt["captures"].items():
            path = self.receipts()[0].parent / name
            self.assertEqual(
                digest, hashlib.sha256(path.read_bytes()).hexdigest()
            )

    def test_each_exposure_has_a_fresh_capture_directory(self) -> None:
        self.run_panels()
        self.run_panels()
        self.assertEqual(len(self.receipts()), 2)
        self.assertNotEqual(
            self.calls[0][3]["DYAPP_UI_SHOTS"],
            self.calls[2][3]["DYAPP_UI_SHOTS"],
        )

    def test_missing_or_empty_images_cannot_produce_a_receipt(self) -> None:
        for empty in (False, True):
            with self.subTest(empty=empty):
                self.empty, self.omit = empty, not empty
                with self.assertRaises((OSError, RuntimeError)):
                    self.run_panels()
                self.assertEqual(self.receipts(), [])

    def test_native_failure_stops_the_exposure_without_a_success_receipt(
        self,
    ) -> None:
        for failure in ("moderation-en", "moderation-ru"):
            with self.subTest(failure=failure):
                self.failure = failure
                with self.assertRaisesRegex(
                    RuntimeError, "controlled native refusal"
                ):
                    self.run_panels()
                self.assertEqual(self.receipts(), [])

    def test_other_platforms_audit_both_locales_without_linux_capture_tool(
        self,
    ) -> None:
        for platform in ("win32", "darwin"):
            with self.subTest(platform=platform):
                self.run_panels(platform)
                for call in self.calls:
                    self.assertNotIn("DYAPP_UI_SHOTS", call[3])
                self.calls.clear()

    def test_case_locale_override_wins_over_inherited_russian_mode(
        self,
    ) -> None:
        smoke_class = SMOKE["DesktopSmoke"]
        run = Mock(
            return_value=subprocess.CompletedProcess(
                [], 0, "UI_SMOKE_PASS:" + "0" * 64, ""
            )
        )
        with (
            patch.dict(
                os.environ, RUNNER_TEMP=str(self.root), DYAPP_SMOKE_RU="1"
            ),
            patch.dict(
                smoke_class.case.__globals__,
                run=run,
                unlock_keyring=lambda: None,
            ),
        ):
            smoke = smoke_class(str(self.executable))
            smoke.case(
                "english", [], self.root, extra_env={"DYAPP_SMOKE_RU": "0"}
            )
            self.assertNotIn("--ui-test-ru", run.call_args.args[0])
            smoke.case("inherited", [], self.root)
            self.assertIn("--ui-test-ru", run.call_args.args[0])


if __name__ == "__main__":
    unittest.main()
