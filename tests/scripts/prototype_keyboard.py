"""Real GTK key events, DOM-order walks and rendered WCAG focus checks."""

from collections import deque
from collections.abc import Callable
import json
from pathlib import Path
from typing import TYPE_CHECKING, TypedDict, cast

from gi.repository import Gdk, Gio, GLib

from focus_pixels import contrasting_area, crop, required_area

if TYPE_CHECKING:
    import webkit2_contracts as WebKit2
else:
    from gi.repository import WebKit2


class Dialog(TypedDict):
    dialog: str
    state: str
    opener: str
    heading: bool


class Radio(TypedDict):
    names: list[str]
    checked: int


class Plan(TypedDict):
    forward: list[str]
    reverse: list[str]
    active: str
    dialog: Dialog | None
    radios: list[Radio]
    problems: list[str]


class Bounds(TypedDict):
    x: float
    y: float
    width: float
    height: float
    scale: float


class Focused(Bounds):
    padding: float
    exposed: bool
    active: str


class KeyboardAudit:
    def __init__(
        self, view: WebKit2.WebView, done: Callable[[list[str]], None]
    ) -> None:
        self.view, self.done = view, done
        self.pending: Callable[[object], None] = lambda _value: None
        self.plan: Plan | None = None
        self.steps: deque[str] = deque()
        self.phase = "forward"
        self.current = ""
        self.body_skipped = False
        self.closed = False
        self.before: WebKit2.SnapshotSurface | None = None
        self.bounds: Bounds | None = None
        self.focused: Focused | None = None
        self.groups: deque[Radio] = deque()
        self.group: Radio | None = None
        self.timer = GLib.timeout_add_seconds(120, self.expired)

    def start(self) -> None:
        self.view.grab_focus()
        self.js("keys.plan()", self.planned)

    def js(self, expression: str, callback: Callable[[object], None]) -> None:
        self.pending = callback
        self.view.call_async_javascript_function(
            "const keys = await import("
            "'/desktop/ui-tests/prototype-keyboard.js');"
            f"return JSON.stringify({expression});",
            -1,
            None,
            None,
            None,
            None,
            self.received,
        )

    def received(
        self, _view: WebKit2.WebView, result: Gio.AsyncResult
    ) -> None:
        if self.closed:
            return
        try:
            value = json.loads(
                self.view.call_async_javascript_function_finish(
                    result
                ).to_string()
            )
            self.pending(value)
        except (GLib.Error, ValueError, KeyError, TypeError) as error:
            self.finish([f"keyboard {self.current}: {error}"])

    def finish(self, errors: list[str]) -> None:
        if self.closed:
            return
        self.closed = True
        if self.timer:
            GLib.source_remove(self.timer)
        self.done(errors)

    def expired(self) -> bool:
        self.timer = 0
        self.finish([f"keyboard deadline at {self.current}"])
        return False

    def planned(self, value: object) -> None:
        self.plan = cast(Plan, value)
        if self.plan["problems"]:
            self.finish(self.plan["problems"])
            return
        self.groups = deque(self.plan["radios"])
        names = self.plan["forward"]
        if names:
            # Removed fixtures can leave a sequential navigation anchor
            # behind in WebKit even when activeElement is BODY.
            self.js(f"await keys.focus({json.dumps(names[0])})", self.anchored)
        else:
            self.current = self.plan["active"]
            self.walk(False)

    def anchored(self, value: object) -> None:
        self.current = cast(str, value)
        self.walk(False)

    def walk(self, reverse: bool) -> None:
        assert self.plan is not None, "Keyboard plan must exist"
        names = self.plan["reverse" if reverse else "forward"]
        ordered = list(reversed(names)) if reverse else names[:]
        current = self.current
        for group in self.plan["radios"]:
            if current in group["names"]:
                current = next(
                    name for name in ordered if name in group["names"]
                )
        index = ordered.index(current) if current in ordered else -1
        self.steps = deque(ordered[index + 1 :] + ordered[: index + 1])
        self.body_skipped = False
        self.advance()

    def key(self, keyval: int, code: int, shift: bool = False) -> None:
        event = Gdk.Event.new(Gdk.EventType.KEY_PRESS)
        key_event = cast(Gdk.EventKey, event)
        window = self.view.get_window()
        assert window is not None, "Keyboard view must be mapped"
        key_event.window = window
        key_event.keyval, key_event.hardware_keycode = keyval, code
        key_event.state = (
            Gdk.ModifierType.SHIFT_MASK if shift else Gdk.ModifierType(0)
        )
        key_event.send_event = True
        seat = self.view.get_display().get_default_seat()
        device = seat.get_keyboard() if seat is not None else None
        if device is not None:
            event.set_device(device)
        self.view.event(event)
        event.type = Gdk.EventType.KEY_RELEASE
        self.view.event(event)

    def advance(self) -> None:
        if not self.steps:
            self.phase_done()
            return
        if self.phase == "radios":
            self.key(Gdk.KEY_Right, 114)
        else:
            self.key(Gdk.KEY_Tab, 23, self.phase == "reverse")
        self.js("await keys.afterKey()", self.key_finished)

    def key_finished(self, value: object) -> None:
        actual = cast(str, value)
        expected = self.steps[0]
        if not actual and not self.body_skipped and self.phase != "radios":
            self.body_skipped = True
            self.advance()
            return
        if actual != expected:
            self.finish(
                [
                    f"{self.phase}: expected {expected}, "
                    f"reached {actual or 'BODY'}"
                ]
            )
            return
        self.current = actual
        self.steps.popleft()
        self.js(f"await keys.unfocused({json.dumps(actual)})", self.unfocused)

    def unfocused(self, value: object) -> None:
        self.bounds = cast(Bounds, value)
        self.view.get_snapshot(
            WebKit2.SnapshotRegion.VISIBLE,
            WebKit2.SnapshotOptions.NONE,
            None,
            self.before_saved,
        )

    def before_saved(
        self, _view: WebKit2.WebView, result: Gio.AsyncResult
    ) -> None:
        if self.closed:
            return
        try:
            self.before = self.view.get_snapshot_finish(result)
            self.js(
                f"await keys.focused({json.dumps(self.current)})",
                self.refocused,
            )
        except GLib.Error as error:
            self.finish([f"keyboard snapshot {self.current}: {error.message}"])

    def refocused(self, value: object) -> None:
        self.focused = cast(Focused, value)
        if (
            not self.focused["exposed"]
            or self.focused["active"] != self.current
        ):
            self.finish(
                [
                    f"focus is obscured or lost: {self.current}; "
                    f"actual={self.focused['active']}; "
                    f"exposed={self.focused['exposed']}; "
                    f"bounds={self.bounds}"
                ]
            )
            return
        self.view.get_snapshot(
            WebKit2.SnapshotRegion.VISIBLE,
            WebKit2.SnapshotOptions.NONE,
            None,
            self.after_saved,
        )

    def after_saved(
        self, _view: WebKit2.WebView, result: Gio.AsyncResult
    ) -> None:
        if self.closed:
            return
        try:
            assert (
                self.before is not None
                and self.bounds is not None
                and self.focused is not None
            ), "Focus frames must exist"
            a, b = self.bounds, self.focused
            x, y = min(a["x"], b["x"]), min(a["y"], b["y"])
            rect = (
                x,
                y,
                max(a["x"] + a["width"], b["x"] + b["width"]) - x,
                max(a["y"] + a["height"], b["y"] + b["height"]) - y,
            )
            after = self.view.get_snapshot_finish(result)
            before_pixels = crop(self.before, rect, b["scale"], b["padding"])
            after_pixels = crop(after, rect, b["scale"], b["padding"])
            area = contrasting_area(before_pixels, after_pixels)
            required = required_area(a["width"], a["height"], a["scale"])
            if area < required:
                if Path("/shots").is_dir():
                    self.before.write_to_png(
                        "/shots/keyboard-focus-before.png"
                    )
                    after.write_to_png("/shots/keyboard-focus-after.png")
                self.finish(
                    [
                        f"focus indicator {self.current}: {area}px at 3:1; "
                        f"requires {required:g}px (WCAG 2.4.13)"
                    ]
                )
                return
            self.advance()
        except (GLib.Error, ValueError) as error:
            self.finish([f"keyboard focus {self.current}: {error}"])

    def phase_done(self) -> None:
        if self.phase == "forward":
            self.phase = "reverse"
            self.walk(True)
        elif self.phase == "reverse":
            self.phase = "radios"
            self.next_radio()
        else:
            assert self.group is not None, "Radio group must exist"
            self.js(
                f"keys.restoreRadios({json.dumps(self.group)}) ?? null",
                lambda _value: self.next_radio(),
            )

    def next_radio(self) -> None:
        if not self.groups:
            self.escape()
            return
        self.group = self.groups.popleft()
        names = self.group["names"]
        self.steps = deque(names[1:])
        self.js(f"await keys.focus({json.dumps(names[0])})", self.radio_ready)

    def radio_ready(self, value: object) -> None:
        self.current = cast(str, value)
        self.advance()

    def escape(self) -> None:
        assert self.plan is not None, "Keyboard plan must exist"
        if self.plan["dialog"] is None:
            self.finish([])
            return
        self.key(Gdk.KEY_Escape, 9)
        self.js(
            "(await keys.afterKey(), "
            f"keys.restored({json.dumps(self.plan['dialog'])}))",
            self.escaped,
        )

    def escaped(self, value: object) -> None:
        assert self.plan is not None, "Keyboard plan must exist"
        self.finish(
            []
            if value
            else [f"Escape did not close/restore focus: {self.plan['dialog']}"]
        )
