"""A release must render usable setup; a blank window is insufficient."""

import html
import importlib.util
import os
from pathlib import Path
import sys
import time
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "production_smoke", ROOT / "tests/scripts/production-desktop-smoke.py"
)
assert SPEC is not None and SPEC.loader is not None, (
    "Python source loader is unavailable"
)
DRIVER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DRIVER)


def directory():
    reports = Path(os.environ.get("RUNNER_TEMP", ROOT / "build"))
    path = reports / "production-smoke-fixtures" / str(time.time_ns())
    path.mkdir(parents=True)
    return path


class ProductionSetupTest(unittest.TestCase):
    def test_hidden_disabled_missing_and_mixed_locale_controls_fail(
        self,
    ) -> None:
        expected = [("Setup", "Create"), ("Начало", "Создать")]
        heading = ("Setup", True, True, False)
        button = ("Create", True, True, True)
        self.assertTrue(DRIVER.rendered_setup([heading, button], expected))
        invalid = (
            [],
            [heading],
            [button],
            [("Setup", False, True, False), button],
            [heading, ("Create", False, True, True)],
            [heading, ("Create", True, False, True)],
            [heading, ("Create", True, True, False)],
            [heading, ("Создать", True, True, True)],
        )
        for nodes in invalid:
            self.assertFalse(DRIVER.rendered_setup(nodes, expected), nodes)

    def test_a_dead_process_or_timeout_never_passes(self) -> None:
        process, accessibility = Mock(), Mock()
        process.poll.return_value = 1
        with self.assertRaisesRegex(RuntimeError, "exited before"):
            DRIVER.wait_for_setup(process, accessibility, [], timeout=1)
        accessibility.ready.assert_not_called()
        with self.assertRaisesRegex(RuntimeError, "did not expose"):
            DRIVER.wait_for_setup(process, accessibility, [], timeout=0)

    def test_fresh_profile_and_normal_launch_do_not_use_test_flags(
        self,
    ) -> None:
        target = directory()
        process, accessibility, button = Mock(), Mock(), Mock()
        process.poll.return_value = 0
        process.wait.return_value = 0
        accessibility.ready.return_value = button
        with (
            patch.object(
                DRIVER.subprocess, "Popen", return_value=process
            ) as start,
            patch.object(DRIVER, "wait_for_setup", return_value=button),
            patch.object(DRIVER.subprocess, "run"),
        ):
            DRIVER.launch(Path("/fixture/app"), target, accessibility, [])
        args, kwargs = start.call_args
        self.assertEqual(args[0], ["/fixture/app"])
        for key in ("CONFIG", "CACHE", "DATA"):
            self.assertEqual(
                kwargs["env"]["XDG_" + key + "_HOME"],
                str(target / key.lower()),
            )
        self.assertNotIn("NO_AT_BRIDGE", kwargs["env"])


@unittest.skipUnless(
    os.environ.get("DYAPP_NATIVE_ACCESSIBILITY") == "1",
    "Real AT-SPI/WebKit fixtures run in the package-test session",
)
class NativeProductionSetupTest(unittest.TestCase):
    def test_real_webkit_rejects_broken_pages_and_closes_a_valid_page(
        self,
    ) -> None:
        accessibility = DRIVER.Accessibility()
        expected = DRIVER.labels()
        heading, button = expected[0]
        for mode in ("valid", "missing", "hidden", "disabled", "mixed"):
            target = directory()
            fixture = target / "page"
            title = "Wrong heading" if mode == "missing" else heading
            caption = expected[1][1] if mode == "mixed" else button
            hidden = ' style="display:none"' if mode == "hidden" else ""
            disabled = " disabled" if mode == "disabled" else ""
            page = (
                f"<html><body><h1{hidden}>{html.escape(title)}</h1>"
                f"<button{disabled}>{html.escape(caption)}</button>"
                "</body></html>"
            )
            fixture.write_text(
                "#!" + sys.executable + "\nimport gi\n"
                "gi.require_version('Gtk', '3.0')\n"
                "gi.require_version('WebKit2', '4.1')\n"
                "from gi.repository import Gdk, Gtk, WebKit2\n"
                "window = Gtk.Window()\nwindow.set_default_size(640, 480)\n"
                "view = WebKit2.WebView()\nwindow.add(view)\n"
                "window.connect('key-press-event', lambda w, e: "
                "(Gtk.main_quit() or True) if e.keyval == Gdk.KEY_q "
                "and e.state & Gdk.ModifierType.CONTROL_MASK else False)\n"
                "view.load_html(" + repr(page) + ", None)\n"
                "window.show_all()\nwindow.present()\nGtk.main()\n"
            )
            fixture.chmod(0o755)
            if mode == "valid":
                DRIVER.launch(fixture, target, accessibility, expected)
            else:
                with self.assertRaisesRegex(RuntimeError, "did not expose"):
                    DRIVER.launch(
                        fixture, target, accessibility, expected, timeout=3
                    )


if __name__ == "__main__":
    unittest.main()
