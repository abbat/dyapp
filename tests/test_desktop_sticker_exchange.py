"""The native sender must stay alive until the saved-pack reply completes."""

from pathlib import Path
import runpy
from threading import Event
import unittest

ROOT = Path(__file__).resolve().parents[1]
EXCHANGE = runpy.run_path(str(ROOT / "tests/scripts/desktop_smoke_native.py"))[
    "sticker_exchange"
]


class DesktopStickerExchangeTest(unittest.TestCase):
    def exchange(self, failing: str = "") -> None:
        sender_started, recipient_done = Event(), Event()
        calls: dict[str, tuple[Path, dict[str, object]]] = {}

        def case(
            name: str, _extra: list[str], root: Path, **kwargs: object
        ) -> str:
            calls[name] = (root, kwargs)
            if name == "peers-sticker-answer":
                sender_started.set()
                self.assertTrue(recipient_done.wait(5), "recipient never ran")
            else:
                self.assertTrue(sender_started.wait(5), "sender never ran")
                recipient_done.set()
            if name == failing:
                raise RuntimeError(name)
            return "peer"

        EXCHANGE(
            case,
            (Path("alice"), "bob"),
            (Path("bob"), "alice"),
            ["--ui-test-file"],
        )
        self.assertEqual(calls["peers-sticker-answer"][0], Path("alice"))
        self.assertEqual(calls["peers-sticker-save-reply"][0], Path("bob"))
        self.assertEqual(
            calls["peers-sticker-answer"][1]["launcher"],
            ["dbus-run-session", "--"],
        )
        self.assertNotIn("launcher", calls["peers-sticker-save-reply"][1])

    def test_sender_and_recipient_overlap_until_completion(self) -> None:
        self.exchange()

    def test_failure_in_either_client_is_preserved(self) -> None:
        for name in ("peers-sticker-answer", "peers-sticker-save-reply"):
            with (
                self.subTest(name=name),
                self.assertRaisesRegex(RuntimeError, name),
            ):
                self.exchange(name)


if __name__ == "__main__":
    unittest.main()
