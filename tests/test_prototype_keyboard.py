"""Exercise the actual auditor callbacks with a manually driven scheduler."""

from collections import deque
from collections.abc import Callable
import importlib.util
from pathlib import Path
import sys
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tests/scripts"))


class Scheduler:
    def __init__(self) -> None:
        self.callbacks: list[Callable[[], bool]] = []
        self.cancelled: list[int] = []

    def schedule(self, _seconds: int, callback: Callable[[], bool]) -> int:
        self.callbacks.append(callback)
        return len(self.callbacks)

    def cancel(self, timer: int) -> bool:
        self.cancelled.append(timer)
        return True


class View:
    def __init__(self) -> None:
        self.scripts: list[str] = []

    def call_async_javascript_function(
        self, script: str, *_args: object
    ) -> None:
        self.scripts.append(script)


class CloseDeadlines(unittest.TestCase):
    def setUp(self) -> None:
        self.scheduler = Scheduler()
        repository = ModuleType("gi.repository")
        repository.__dict__["GLib"] = SimpleNamespace(
            timeout_add_seconds=self.scheduler.schedule,
            source_remove=self.scheduler.cancel,
            Error=type("GLibError", (Exception,), {}),
        )
        repository.__dict__["Gdk"] = SimpleNamespace(
            KEY_Escape=65307, KEY_Tab=65289
        )
        repository.__dict__["Gio"] = SimpleNamespace(AsyncResult=object)
        repository.__dict__["WebKit2"] = SimpleNamespace(WebView=object)
        gi = ModuleType("gi")
        gi.__dict__["repository"] = repository
        spec = importlib.util.spec_from_file_location(
            "_keyboard_audit_test",
            ROOT / "tests/scripts/prototype_keyboard.py",
        )
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, {"gi": gi, "gi.repository": repository}):
            spec.loader.exec_module(module)
        self.view = View()
        self.results: list[list[str]] = []
        self.progress: list[str] = []
        self.keys: list[tuple[object, ...]] = []
        self.audit = module.KeyboardAudit(
            self.view, self.results.append, self.progress.append
        )
        self.audit.key = lambda *args: self.keys.append(args)
        self.audit.plan = {
            "dialog": {
                "dialog": "#dialog",
                "opener": "#opener",
                "state": "profile",
                "heading": False,
            },
            "forward": [],
            "reverse": [],
            "active": "",
            "radios": [],
            "problems": [],
            "boundary": "",
        }

    def test_close_listener_is_ready_before_escape_and_completion(
        self,
    ) -> None:
        self.audit.escape()
        self.assertIn("keys.prepareEscape", self.view.scripts[-1])
        self.assertEqual(self.keys, [])
        self.assertEqual(self.results, [])
        self.audit.pending(None)  # Browser acknowledged listener registration.
        self.assertEqual(self.keys, [(65307, 9)])
        self.assertIn("await keys.restored", self.view.scripts[-1])
        self.assertEqual(self.results, [])
        self.assertEqual(self.scheduler.cancelled, [1])
        self.scheduler.callbacks[0]()  # A cancelled callback already queued.
        self.assertEqual(self.results, [])
        self.audit.pending(
            True
        )  # Native close and focus restoration completed.
        self.assertEqual(self.results, [[]])
        self.assertEqual(self.scheduler.cancelled, [1, 2])
        self.scheduler.callbacks[-1]()
        self.assertEqual(self.results, [[]])

    def test_missing_close_expires_once_without_sleep(self) -> None:
        self.audit.escape()
        self.audit.pending(None)
        pending = self.audit.pending
        self.scheduler.callbacks[-1]()
        self.assertEqual(len(self.results), 1)
        self.assertIn("await keys.restored", self.results[0][0])
        pending(True)  # Late success cannot turn a failed transition green.
        self.scheduler.callbacks[-1]()
        self.assertEqual(len(self.results), 1)

    def test_missing_listener_ack_does_not_deliver_escape(self) -> None:
        self.audit.escape()
        self.scheduler.callbacks[-1]()
        self.assertEqual(self.keys, [])
        self.assertEqual(len(self.results), 1)
        self.assertIn("prepareEscape", self.results[0][0])

    def boundary_plan(self, phase: str, current: str, expected: str) -> None:
        self.audit.plan.update(
            {
                "forward": ["#first", "#last"],
                "reverse": ["#first", "#last"],
                "boundary": "#dialog",
            }
        )
        self.audit.phase = phase
        self.audit.current = current
        self.audit.steps = deque([expected])

    def test_native_boundary_is_allowed_once_then_must_reach_next_control(
        self,
    ) -> None:
        self.boundary_plan("forward", "#last", "#first")
        self.audit.key_finished("#dialog")
        self.assertEqual(self.results, [])
        self.assertIn("afterKey", self.view.scripts[-1])
        self.audit.key_finished("#dialog")
        self.assertIn("expected #first", self.results[0][0])

    def test_reverse_boundary_is_allowed_once(self) -> None:
        self.boundary_plan("reverse", "#first", "#last")
        self.audit.key_finished("#dialog")
        self.assertEqual(self.results, [])
        self.assertTrue(self.audit.native_skipped)

    def test_native_root_between_controls_is_rejected(self) -> None:
        self.boundary_plan("forward", "#first", "#last")
        self.audit.key_finished("#dialog")
        self.assertIn("expected #last", self.results[0][0])

    def test_body_then_native_boundary_has_separate_bounded_transitions(
        self,
    ) -> None:
        self.boundary_plan("forward", "#last", "#first")
        self.audit.key_finished("")
        self.audit.key_finished("#dialog")
        self.assertEqual(self.results, [])
        self.assertTrue(self.audit.body_skipped)
        self.assertTrue(self.audit.native_skipped)
        self.audit.key_finished("#dialog")
        self.assertIn("expected #first", self.results[0][0])

    def test_non_native_root_is_rejected(self) -> None:
        self.boundary_plan("forward", "#last", "#first")
        self.audit.plan["boundary"] = ""
        self.audit.key_finished("#dialog")
        self.assertIn("expected #first", self.results[0][0])


if __name__ == "__main__":
    unittest.main()
