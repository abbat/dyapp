#!/usr/bin/env python3
"""Save every static desktop prototype screen, en and ru, narrow and wide.

Runs under xvfb-run in the prototype image: loads
/prototype/index.html?lang=X#screen in a WebKitGTK view and writes
/shots/<screen>-<lang>-<width>.png.
"""
import sys

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402 -- versions set

SCREENS = [
    "wizard-identity", "wizard-network", "picker", "recovery",
    "profile", "profile-hidden", "profile-dark",
    "publish-dialog", "publish-progress",
    "network", "network-checking", "network-error",
    "search", "search-loading", "search-empty", "search-error",
    "profile-view", "profile-view-match",
    "chats", "chat", "chat-first", "chat-request",
    "chat-menu", "chat-clear-dialog", "chat-cleared",
    "settings", "switcher", "menu", "remove-dialog", "remove-offline",
]
LANGS = ["en", "ru"]
WIDTHS = [440, 1280]
HEIGHT = 800
SOURCE, OUT = "/prototype/index.html", "/shots"

jobs = [(s, lang, w) for w in WIDTHS for lang in LANGS for s in SCREENS]
failed = []
window = Gtk.OffscreenWindow()
view = WebKit2.WebView()
window.add(view)
window.show_all()


def next_job():
    if not jobs:
        Gtk.main_quit()
        return
    screen, lang, width = jobs[0]
    window.resize(width, HEIGHT)
    view.set_size_request(width, HEIGHT)
    # A changed hash alone is a same-page jump with no load event,
    # so the query differs too.
    view.load_uri(f"file://{SOURCE}?lang={lang}&shot={len(jobs)}#{screen}")


def saved(_view, result):
    screen, lang, width = jobs.pop(0)
    name = f"{OUT}/{screen}-{lang}-{width}.png"
    try:
        view.get_snapshot_finish(result).write_to_png(name)
    except GLib.Error as error:
        failed.append(f"{name}: {error.message}")
    next_job()


def loaded(_view, event):
    if event == WebKit2.LoadEvent.FINISHED:
        # Let layout settle after the inline script hides the other screens.
        GLib.timeout_add(150, lambda: view.get_snapshot(
            WebKit2.SnapshotRegion.FULL_DOCUMENT,
            WebKit2.SnapshotOptions.NONE, None, saved,
        ) and False)


def load_failed(_view, _event, uri, error):
    failed.append(f"{uri}: {error.message}")
    jobs.pop(0)
    GLib.idle_add(lambda: next_job() and False)
    return True


view.connect("load-changed", loaded)
view.connect("load-failed", load_failed)
GLib.idle_add(lambda: next_job() and False)
Gtk.main()
for line in failed:
    print(line, file=sys.stderr)
total = len(SCREENS) * len(LANGS) * len(WIDTHS)
print(f"{total - len(failed)} screenshots in {OUT}")
sys.exit(1 if failed else 0)
