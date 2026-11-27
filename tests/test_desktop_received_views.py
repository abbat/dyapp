"""Check native view artifact completeness, not rendered acceptance."""

from collections.abc import Mapping, Sequence
import hashlib
import json
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
NATIVE = runpy.run_path(str(ROOT / "tests/scripts/desktop_smoke_native.py"))
PANELS = NATIVE["received_views"]
STATES = (
    "loading",
    "populated",
    "empty",
    "initial-error",
    "retained-error",
    "no-query-match",
    "page-transition",
    "profile-return",
)


class ReceivedViewEvidence(unittest.TestCase):
    def test_exposures_require_current_binary_source_and_complete_images(
        self,
    ) -> None:
        for platform, defect in (
            ("linux", ""),
            ("linux", "missing"),
            ("linux", "empty"),
            ("linux", "refused"),
            ("win32", ""),
            ("darwin", ""),
        ):
            with self.subTest(platform=platform, defect=defect):
                self.exercise(platform, defect)

    def test_link_review_capture_contract(self) -> None:
        for platform, defect in (
            ("linux", ""),
            ("linux", "missing"),
            ("linux", "empty"),
            ("linux", "refused"),
            ("win32", ""),
            ("darwin", ""),
        ):
            with self.subTest(platform=platform, defect=defect):
                self.exercise(platform, defect, panel="link-review")

    def test_publication_capture_contract(self) -> None:
        for platform, defect in (
            ("linux", ""),
            ("linux", "missing"),
            ("linux", "empty"),
            ("linux", "refused"),
            ("win32", ""),
            ("darwin", ""),
        ):
            with self.subTest(platform=platform, defect=defect):
                self.exercise(platform, defect, panel="publication")

    def exercise(
        self, platform: str, defect: str, panel: str = "received-views"
    ) -> None:
        states = {
            "received-views": STATES,
            "link-review": ("choices", "photo-error", "preview", "saved"),
            "publication": ("pending",),
        }[panel]
        with tempfile.TemporaryDirectory(dir=ROOT / "build") as folder:
            root = Path(folder)
            binary = root / "app"
            binary.write_bytes(b"controlled executable")
            calls = []
            profiles = []
            outputs = []

            def render(
                name: str,
                flags: Sequence[str],
                profile: str | Path,
                **options: object,
            ) -> str | None:
                raw = options["extra_env"]
                assert isinstance(raw, Mapping)
                env = {str(key): str(value) for key, value in raw.items()}
                calls.append(name)
                self.assertTrue(Path(profile).is_dir())
                profiles.append(str(profile))
                language, theme, text = name.removeprefix(f"{panel}-").split(
                    "-"
                )
                scale = "100" if text == "normal" else "200"
                self.assertIn(f"--ui-test-step={panel}:{theme}:{scale}", flags)
                self.assertEqual("--ui-test-ru" in flags, language == "ru")
                self.assertEqual(env["DYAPP_SMOKE_RU"], "0")
                if defect == "refused":
                    raise RuntimeError("controlled native refusal")
                output = env.get("DYAPP_UI_SHOTS")
                if platform != "linux":
                    self.assertIsNone(output)
                    return None
                assert output is not None
                outputs.append(output)
                for state in states:
                    for width in (440, 1280):
                        if defect == "missing" and state == states[-1]:
                            continue
                        image = Path(output) / (
                            f"{panel}-{state}-{theme}-{text}"
                            f"-{language}-{width}.png"
                        )
                        image.write_bytes(
                            b"" if defect == "empty" else b"controlled PNG"
                        )
                return None

            with (
                patch.object(NATIVE["sys"], "platform", platform),
                patch.dict(NATIVE["os"].environ, GITHUB_SHA="a" * 40),
            ):
                if defect:
                    with self.assertRaises((OSError, RuntimeError)):
                        PANELS(render, root, str(binary), panel=panel)
                    self.assertEqual(
                        list(root.glob(f"{panel}-shots-*/receipt.json")),
                        [],
                    )
                    return
                PANELS(render, root, str(binary), panel=panel)
            self.assertEqual(len(calls), 8)
            self.assertEqual(len(set(calls)), 8)
            self.assertEqual(len(set(profiles)), 8)
            self.assertTrue(
                all(not Path(profile).exists() for profile in profiles)
            )
            receipts = list(root.glob(f"{panel}-shots-*/receipt.json"))
            self.assertEqual(len(receipts), 1)
            receipt = json.loads(receipts[0].read_text())
            self.assertEqual(receipt["source_sha"], "a" * 40)
            self.assertEqual(receipt["cases"], calls)
            self.assertEqual(
                receipt["executable_sha256"],
                hashlib.sha256(binary.read_bytes()).hexdigest(),
            )
            self.assertEqual(
                len(receipt["captures"]),
                len(states) * 16 if platform == "linux" else 0,
            )
            if platform == "linux":
                self.assertEqual(len(set(outputs)), 1)
            for name, digest in receipt["captures"].items():
                self.assertEqual(
                    digest,
                    hashlib.sha256(
                        (receipts[0].parent / name).read_bytes()
                    ).hexdigest(),
                )


if __name__ == "__main__":
    unittest.main()
