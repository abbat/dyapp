"""Calibrate the real keyboard auditor against broken DOM behavior."""

from collections import deque
from collections.abc import Callable
import json
from typing import TYPE_CHECKING

from gi.repository import Gio, GLib
from prototype_keyboard import KeyboardAudit
from prototype_progress import Progress

if TYPE_CHECKING:
    import webkit2_contracts as WebKit2
else:
    from gi.repository import WebKit2


class KeyboardFixtures:
    def __init__(
        self,
        view: WebKit2.WebView,
        done: Callable[[list[str]], None],
        advanced: Callable[[str], None] | None = None,
    ) -> None:
        self.view, self.done = view, done
        self.cases = deque(
            [
                ("trap", "expected #keyboard-fixture-second"),
                ("focus", "focus indicator #keyboard-fixture-first"),
                ("escape", "Escape did not close/restore focus"),
            ]
        )
        self.pending: Callable[[], None] = lambda: None
        self.runner: KeyboardAudit | None = None
        self.expected = ""
        self.problem: list[str] = []
        self.closed = False
        self.advanced = advanced
        self.deadline = Progress(
            lambda callback: GLib.timeout_add_seconds(120, callback),
            GLib.source_remove,
            self.expired,
        )

    def start(self) -> None:
        if not self.cases:
            self.finish([])
            return
        mode, self.expected = self.cases.popleft()
        self.evaluate(f"await keys.beginFixture({json.dumps(mode)})", self.run)

    def waiting(self, phase: str) -> None:
        self.deadline.begin(phase)
        if self.advanced is not None:
            self.advanced(phase)

    def evaluate(self, expression: str, callback: Callable[[], None]) -> None:
        if self.closed:
            return
        self.pending = callback
        self.waiting(f"counterexample {expression}")
        self.view.call_async_javascript_function(
            "const keys = await import("
            "'/desktop/ui-tests/prototype-keyboard.js');"
            f"{expression}; return 'done';",
            -1,
            None,
            None,
            None,
            None,
            self.evaluated,
        )

    def evaluated(
        self, _view: WebKit2.WebView, result: Gio.AsyncResult
    ) -> None:
        if self.closed:
            return
        try:
            self.view.call_async_javascript_function_finish(result)
            self.pending()
        except GLib.Error as error:
            self.finish([f"keyboard counterexample: {error.message}"])

    def run(self) -> None:
        self.runner = KeyboardAudit(self.view, self.checked, self.waiting)
        self.runner.start()

    def checked(self, errors: list[str]) -> None:
        if self.closed:
            return
        self.problem = (
            []
            if any(self.expected in error for error in errors)
            else [
                f"Keyboard counterexample not detected ({self.expected}): "
                f"{errors}"
            ]
        )
        self.evaluate("keys.endFixture()", self.cleaned)

    def cleaned(self) -> None:
        if self.problem:
            self.finish(self.problem)
        else:
            self.start()

    def finish(self, errors: list[str]) -> None:
        if self.closed:
            return
        self.closed = True
        self.deadline.close()
        if self.runner is not None:
            self.runner.finish([])
        self.done(errors)

    def expired(self, phase: str) -> None:
        self.finish([f"Keyboard counterexample calibration deadline: {phase}"])
