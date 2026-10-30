#!/usr/bin/env python3
"""Save every static prototype screen, en and ru, at each width.

Runs under xvfb-run in the prototype image: loads
/<platform>/prototype/index.html?lang=X#screen in a WebKitGTK view and writes
/shots/<screen>-<lang>-<width>.png. The platform is desktop (default) or
android; Android shots also come in the dark theme as <screen>-dark-...
"""
import sys

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402 -- versions set

DESKTOP = [
    "wizard-identity", "wizard-network", "picker", "recovery",
    "profile", "profile-hidden", "profile-dark",
    "publish-dialog", "publish-progress",
    "network", "network-checking", "network-error",
    "search", "search-loading", "search-empty", "search-error",
    "profile-view", "profile-view-match", "unlike-dialog",
    "chats", "chat", "chat-first", "chat-request",
    "chat-menu", "chat-clear-dialog", "chat-cleared", "delete-dialog",
    "settings", "switcher", "menu", "remove-dialog", "remove-offline",
]
ANDROID = [
    "lock-setup", "lock-unlock",
    "wizard-identity", "wizard-network", "picker", "recovery",
    "profile", "profile-hidden", "publish-dialog", "publish-progress",
    "network", "network-checking", "network-error",
    "search", "search-loading", "search-empty", "search-error",
    "profile-view", "profile-view-match", "unlike-dialog",
    "chats", "chat", "chat-first", "chat-request",
    "chat-menu", "chat-clear-dialog", "chat-cleared", "delete-dialog",
    "settings", "lock-settings", "switcher", "remove-dialog", "remove-offline",
]
PLATFORM = sys.argv[1] if len(sys.argv) > 1 else "desktop"
SCREENS = {"desktop": DESKTOP, "android": ANDROID}[PLATFORM]
# Phone widths for Android: a small and a common one.
WIDTHS = [360, 412] if PLATFORM == "android" else [440, 1280]
THEMES = ["light", "dark"] if PLATFORM == "android" else ["light"]
LANGS = ["en", "ru"]
HEIGHT = 800
SOURCE, OUT = f"/{PLATFORM}/prototype/index.html", "/shots"

jobs = [(s, lang, w, theme) for theme in THEMES for w in WIDTHS
        for lang in LANGS for s in SCREENS]
total = len(jobs)
failed = []
window = Gtk.OffscreenWindow()
view = WebKit2.WebView()
window.add(view)
window.show_all()


def next_job():
    if not jobs:
        Gtk.main_quit()
        return
    screen, lang, width, theme = jobs[0]
    window.resize(width, HEIGHT)
    view.set_size_request(width, HEIGHT)
    # A changed hash alone is a same-page jump with no load event,
    # so the query differs too.
    view.load_uri(f"file://{SOURCE}?lang={lang}&theme={theme}"
                  f"&shot={len(jobs)}#{screen}")


def saved(_view, result):
    screen, lang, width, theme = jobs.pop(0)
    dark = "-dark" if theme == "dark" else ""
    name = f"{OUT}/{screen}{dark}-{lang}-{width}.png"
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
print(f"{total - len(failed)} screenshots in {OUT}")
sys.exit(1 if failed else 0)
