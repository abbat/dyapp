"""Check the Linux release UI through AT-SPI without injected commands."""

import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]


def labels():
    directory = ROOT / "clients/desktop/ui/i18n"
    result = []
    for path in sorted(directory.glob("*.json")):
        catalogue = json.loads(path.read_text())
        result.append((catalogue["setup.title"], catalogue["setup.create"]))
    if not result:
        raise RuntimeError("Missing production smoke locale catalogues")
    return result


def rendered_setup(nodes, expected):
    """Require visible setup text and an enabled button in the same locale."""
    headings = {name for name, showing, _, _ in nodes if showing}
    buttons = {
        name
        for name, showing, enabled, button in nodes
        if showing and enabled and button
    }
    return any(
        heading in headings and button in buttons
        for heading, button in expected
    )


class Accessibility:
    def __init__(self) -> None:
        import gi

        gi.require_version("Atspi", "2.0")
        from gi.repository import Atspi, GLib

        self.api = Atspi
        self.transient = GLib.Error
        self.context = GLib.MainContext.default()
        if Atspi.init() not in (0, 1):
            raise RuntimeError("Could not connect to AT-SPI")
        self.desktop = Atspi.get_desktop(0)
        if self.desktop is None:
            raise RuntimeError("AT-SPI desktop is unavailable")

    def nodes(self, application):
        application.clear_cache()
        pending, result, count = [application], [], 0
        while pending:
            node = pending.pop()
            if node is None:
                continue
            count += 1
            if count > 4096:
                raise RuntimeError(
                    "Production accessibility tree exceeds 4096 nodes"
                )
            states = node.get_state_set()
            result.append(
                (
                    node.get_name(),
                    states.contains(self.api.StateType.SHOWING),
                    states.contains(self.api.StateType.ENABLED),
                    node.get_role() == self.api.Role.PUSH_BUTTON,
                    node,
                )
            )
            pending.extend(
                node.get_child_at_index(index)
                for index in range(node.get_child_count())
            )
        return result

    def setup_button(self, application, expected):
        nodes = self.nodes(application)
        if not rendered_setup([row[:4] for row in nodes], expected):
            return None
        names = {button for _, button in expected}
        return next(
            node
            for name, showing, enabled, button, node in nodes
            if name in names and showing and enabled and button
        )

    def ready(self, process_id, expected):
        for _ in range(100):
            if not self.context.pending():
                break
            self.context.iteration(False)
        self.desktop.clear_cache()
        for index in range(self.desktop.get_child_count()):
            application = self.desktop.get_child_at_index(index)
            if application is None:
                continue
            try:
                if application.get_process_id() != process_id:
                    continue
                return self.setup_button(application, expected)
            except self.transient:
                # The app may replace its accessible tree during launch.
                continue
        return None


def wait_for_setup(process, accessibility, expected, timeout: int = 45):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("Production app exited before rendering setup")
        button = accessibility.ready(process.pid, expected)
        if button is not None:
            return button
        time.sleep(0.1)
    raise RuntimeError(
        "Production app did not expose a rendered setup heading and button"
    )


def launch(
    executable, directory, accessibility, expected, timeout: int = 45
) -> None:
    env = dict(os.environ)
    for key in ("CONFIG", "CACHE", "DATA"):
        path = directory / key.lower()
        path.mkdir()
        env["XDG_" + key + "_HOME"] = str(path)
    env.pop("NO_AT_BRIDGE", None)
    with (directory / "app.log").open("w") as log:
        process = subprocess.Popen(
            [str(executable)],
            env=env,
            stdout=log,
            stderr=log,
            start_new_session=True,
        )
        try:
            button = wait_for_setup(process, accessibility, expected, timeout)
            if not button.get_component_iface().grab_focus():
                raise RuntimeError(
                    "Could not focus the production setup button"
                )
            subprocess.run(
                ["xdotool", "key", "--clearmodifiers", "ctrl+q"],
                check=True,
                timeout=10,
            )
            if process.wait(timeout=15) != 0:
                raise RuntimeError(
                    "Production app did not exit cleanly after Ctrl+Q"
                )
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()


def smoke(executable: "str | Path") -> None:
    executable = Path(executable).resolve(strict=True)
    checker = ROOT / "tools/scripts/check-no-ui-smoke.py"
    subprocess.run([sys.executable, str(checker), str(executable)], check=True)
    expected = labels()
    # Register the accessibility client before launching WebKit.
    accessibility = Accessibility()
    reports = Path(os.environ.get("RUNNER_TEMP", ROOT / "build"))
    directory = reports / "production-desktop" / str(time.time_ns())
    directory.mkdir(parents=True)
    launch(executable, directory, accessibility, expected)
    print(
        "Production desktop rendered setup and closed without injected hooks"
    )


if __name__ == "__main__":
    from check_wrapper import wrap

    wrap("production-desktop-smoke")
    if len(sys.argv) != 2:
        raise SystemExit("Usage: production-desktop-smoke.py <native-binary>")
    smoke(sys.argv[1])
