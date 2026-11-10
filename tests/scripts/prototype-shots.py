#!/usr/bin/env python3
"""Save every static prototype screen, en and ru, at each width.

Runs under xvfb-run in the prototype image: loads
/<platform>/prototype/index.html?lang=X#screen from a loopback HTTP server
rooted at /assets in a WebKitGTK view and writes
/shots/<screen>-<lang>-<width>.png. The platform is desktop (default) or
android; optional screen names select a subset. Android shots also come in the
dark theme as <screen>-dark-... It also measures the gaps between visible
neighbouring blocks and writes them to /shots/layout.json for
check-layout.py.
"""

from __future__ import annotations

from collections.abc import Callable
import faulthandler
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import sys
import threading

from prototype_progress import Progress

from typing import TYPE_CHECKING

import gi

PHASE_LIMIT_SECONDS = 120
# GTK can block before its event loop can dispatch a failure timer.
faulthandler.dump_traceback_later(PHASE_LIMIT_SECONDS, exit=True)
print("prototype: importing GTK/WebKit", file=sys.stderr, flush=True)

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
gi.require_version("JavaScriptCore", "4.1")
from gi.repository import Gio, GLib, Gtk  # noqa: E402 -- versions set

if TYPE_CHECKING:
    import webkit2_contracts as WebKit2
else:
    from gi.repository import WebKit2

# Neighbouring visible blocks of one parent, stacked or side by side, closer
# than 12 px: the raw input of the layout criteria in check-layout.py.
MEASURE = """JSON.stringify((() => {
  const style = e => getComputedStyle(e);
  const box = e => {
    const s = style(e);
    if (s.display === 'none' || s.visibility === 'hidden'
        || s.position === 'absolute' || s.position === 'fixed') return null;
    const r = e.getBoundingClientRect();
    return r.width > 0 && r.height > 0 ? r : null;
  };
  const block = e => !/^(inline|contents|table-row|table-cell)/
      .test(style(e).display);
  const name = e => e.tagName.toLowerCase() + (e.id ? '#' + e.id
      : e.classList.length ? '.' + [...e.classList].join('.') : '');
  const scope = e => {
    for (let p = e.parentElement; p; p = p.parentElement)
      if (p.id) return '#' + p.id;
    return '';
  };
  const control = e => /^(BUTTON|INPUT|SELECT|TEXTAREA)$/.test(e.tagName);
  const framed = e => {
    const s = style(e);
    return control(e) || parseFloat(s.borderTopWidth) > 0
        || parseFloat(s.borderBottomWidth) > 0
        || !/^(transparent|rgba\\(0, 0, 0, 0\\))$/.test(s.backgroundColor);
  };
  const ident = e => e.classList.contains('full-id')
      || (e.tagName === 'CODE' && style(e).display === 'block');
  const found = [];
  const input = document.getElementById('chat-input');
  const stickers = document.getElementById('chat-stickers');
  if (stickers && box(input) && !input.closest('[inert]')) {
    const attach = document.getElementById('chat-attach');
    const send = document.getElementById('chat-send');
    for (const button of [attach, stickers, send]) {
      if (!button.getAttribute('aria-label') || !button.title)
        throw new Error('composer icon lacks translated name or tooltip');
    }
    if (input.disabled) {
      if (!attach.disabled || !stickers.disabled || !send.disabled)
        throw new Error('unwritable chat enables a composer action');
    } else {
      const previous = document.activeElement;
      const value = input.value;
      input.focus();
      const before = input.getBoundingClientRect();
      for (const draft of ['message', '']) {
        input.value = draft;
        input.dispatchEvent(new Event('input', {bubbles: true}));
        const after = input.getBoundingClientRect();
        if (send.hidden !== !draft || send.disabled !== !draft)
          throw new Error('Send visibility does not follow the draft');
        if (after.width !== before.width || after.left !== before.left
            || document.activeElement !== input)
          throw new Error('Send visibility shifts the input or its focus');
      }
      input.value = value;
      input.dispatchEvent(new Event('input', {bubbles: true}));
      input.blur();
      previous.focus();
    }
  }
  const picker = document.getElementById('sticker-picker');
  if (picker && box(picker)) {
    const bounds = picker.getBoundingClientRect();
    if (bounds.left < 0 || bounds.right > innerWidth)
      throw new Error('sticker picker exceeds viewport');
    for (const button of picker.querySelectorAll('.sticker-choices button')) {
      if (!button.textContent.trim()
          || button.scrollWidth > button.clientWidth)
        throw new Error('sticker pack choice clips its name');
    }
    const paging = picker.querySelector('.sticker-pagination');
    if (box(paging)) {
      const [previous, next] = paging.children;
      const a = previous.getBoundingClientRect();
      const b = next.getBoundingClientRect();
      if (a.top > b.top || (a.top === b.top && a.left >= b.left))
        throw new Error('sticker paging order reversed');
    }
    const grid = document.getElementById('sticker-picker-grid');
    if (box(grid)) {
      if (grid.children.length !== 8)
        throw new Error('sticker picker is not bounded');
      for (const button of grid.children)
        if (!button.getAttribute('aria-label')
            || !button.querySelector('canvas'))
          throw new Error('sticker choice lacks pixels or name');
    }
  }
  const sticker = document.getElementById('sticker-message');
  if (sticker && box(sticker)) {
    if (sticker.classList.contains('msg'))
      throw new Error('sticker has a bubble');
    const image = sticker.querySelector('canvas');
    if (box(image) && (!image.getAttribute('aria-label')
        || image.getBoundingClientRect().width
          > parseFloat(style(image).maxHeight)
        || image.getBoundingClientRect().height
          > parseFloat(style(image).maxHeight)))
      throw new Error('sticker image lacks name or exceeds its size limit');
  }
  // Menu and navigation items touch by platform convention; page regions
  // (landmarks) are separated by their own surfaces.
  const items = 'nav, [role=menu], [role=menubar], [role=tablist]';
  const region = /^(NAV|MAIN|HEADER|ASIDE|FOOTER)$/;
  for (const parent of document.querySelectorAll('body *')) {
    if (parent.matches(items)) continue;
    const kids = [...parent.children].map(e => [e, box(e)])
        .filter(([e, r]) => r && block(e) && !region.test(e.tagName));
    for (let i = 1; i < kids.length; i++) {
      const [a, ra] = kids[i - 1], [b, rb] = kids[i];
      const across = Math.min(ra.right, rb.right) - Math.max(ra.left, rb.left);
      const along = Math.min(ra.bottom, rb.bottom) - Math.max(ra.top, rb.top);
      // Either order: dialogs reverse their buttons on narrow screens.
      const gap = across > 0 ? Math.max(rb.top - ra.bottom, ra.top - rb.bottom)
          : along > 0 ? Math.max(rb.left - ra.right, ra.left - rb.right)
          : null;
      if (gap === null || gap >= 12) continue;
      found.push({scope: scope(a), a: name(a), b: name(b),
                  gap: Math.round(gap), framed: framed(a) || framed(b),
                  identifier: ident(a) || ident(b),
                  label: /^(LABEL|LEGEND)$/.test(a.tagName)});
    }
  }
  return found;
})())"""

DESKTOP = [
    "wizard-identity",
    "wizard-network",
    "picker",
    "recovery",
    "profile",
    "profile-hidden",
    "profile-dark",
    "publish-dialog",
    "photo-remove-dialog",
    "publish-progress",
    "network",
    "network-checking",
    "network-error",
    "search",
    "search-loading",
    "search-empty",
    "search-error",
    "profile-view",
    "profile-view-safety",
    "verify-dialog",
    "profile-view-verified",
    "profile-view-match",
    "unlike-dialog",
    "chats",
    "chat",
    "chat-draft",
    "chat-closed",
    "chat-first",
    "chat-request",
    "chat-new-device",
    "chat-menu",
    "chat-rename-dialog",
    "chat-clear-dialog",
    "chat-cleared",
    "delete-dialog",
    "settings",
    "settings-appearance",
    "settings-privacy",
    "settings-notifications",
    "settings-photos",
    "settings-node",
    "settings-account",
    "settings-about",
    "settings-identities-screen",
    "settings-devices",
    "settings-blocked",
    "settings-passed",
    "sticker-picker-empty",
    "sticker-picker-ready",
    "sticker-picker-recent",
    "sticker-picker-loading",
    "sticker-picker-error",
    "sticker-picker-sending",
    "chat-sticker-image",
    "chat-sticker-fallback",
    "chat-sticker-pending",
    "chat-sticker-downloading",
    "chat-sticker-failed",
    "chat-sticker-quota",
    "chat-sticker-saved",
    "chat-sticker-checking",
    "chat-sticker-fetch",
    "chat-sticker-storage",
    "chat-sticker-storage-idle",
    "chat-sticker-missing",
    "chat-sticker-read-error",
    "chat-sticker-request-error",
    "stickers-empty",
    "stickers-ready",
    "stickers-loading",
    "stickers-error",
    "stickers-importing",
    "stickers-quota",
    "stickers-remove-dialog",
    "stickers-remove-unreadable",
    "stickers-removing",
    "stickers-removed",
    "stickers-remove-error",
    "switcher",
    "menu",
    "remove-dialog",
    "remove-offline",
    "revoked-dialog",
    "wizard-network-link",
    "link-show",
    "link-offline",
    "link-expired",
    "link-finishing",
    "link-done",
    "link-done-pending",
    "chat-linked",
    "devices-sent",
    "device-code-dialog",
    "device-code-error",
    "device-code-bad",
    "device-confirm-dialog",
    "disconnect-dialog",
    "disconnect-offline",
]
ANDROID = [
    "lock-setup",
    "lock-unlock",
    "wizard-identity",
    "wizard-network",
    "picker",
    "recovery",
    "profile",
    "profile-hidden",
    "publish-dialog",
    "publish-progress",
    "network",
    "network-checking",
    "network-error",
    "search",
    "search-loading",
    "search-empty",
    "search-error",
    "profile-view",
    "profile-view-match",
    "unlike-dialog",
    "chats",
    "chat",
    "chat-closed",
    "chat-first",
    "chat-request",
    "chat-menu",
    "chat-clear-dialog",
    "chat-cleared",
    "delete-dialog",
    "settings",
    "lock-settings",
    "switcher",
    "remove-dialog",
    "remove-offline",
]
# Await actual font/layout readiness; the timer is a failure deadline.
READY_MEASURE = (
    """
let timer;
try {
  await Promise.race([
    document.fonts.ready.then(() => new Promise(resolve =>
      requestAnimationFrame(() => requestAnimationFrame(resolve)))),
    new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error('layout readiness deadline')),
                         30000);
    })
  ]);
  return """
    + MEASURE
    + """;
} finally { clearTimeout(timer); }
"""
)

PLATFORM = sys.argv[1] if len(sys.argv) > 1 else "desktop"
ALL_SCREENS = {"desktop": DESKTOP, "android": ANDROID}[PLATFORM]
requested = sys.argv[2:]
if len(requested) != len(set(requested)) or any(
    s not in ALL_SCREENS for s in requested
):
    raise SystemExit(
        "screen names must be unique and available on this platform"
    )
SCREENS = requested or ALL_SCREENS
# Phone widths for Android: a small and a common one.
WIDTHS = [360, 412] if PLATFORM == "android" else [320, 440, 1280]
THEMES = ["light", "dark"]
TEXT_SCALES = [100] if PLATFORM == "android" else [100, 200]
LANGS = ["en", "ru"]
HEIGHT = 800
SOURCE, OUT = f"/{PLATFORM}/prototype/index.html", "/shots"
CRITERIA = (
    Path(os.environ.get("DYAPP_AXE", "/opt/axe-core/axe.min.js")).read_text()
    + "\nconst smoke = {};\n"
    + Path("/assets/desktop/ui-tests/criteria.js").read_text()
    + Path("/assets/desktop/ui-tests/criteria-axe.js").read_text()
    + Path("/assets/desktop/ui-tests/criteria-text.js").read_text()
    if PLATFORM == "desktop"
    else ""
)
SELF_CHECK = (
    "smoke.criteria.selfCheck(); smoke.textClippingSelfCheck(); "
    "await smoke.accessibilitySelfCheck(); "
)
AUDIT = (
    "return JSON.stringify([...smoke.criteria.dialogs({titles: "
    "innerWidth >= 440 && "
    "document.documentElement.dataset.textScale !== '200' "
    "&& document.documentElement.lang !== 'en-XA'}), "
    "...smoke.textClipping(), ...await smoke.accessibility()]);"
    if PLATFORM == "desktop"
    else ""
)


def job_name(job: tuple[str, str, int, str, int]) -> str:
    screen, lang, width, theme, scale = job
    dark = (
        ("-dark" if PLATFORM == "android" else "--dark")
        if theme == "dark"
        else ""
    )
    text = "--text200" if scale == 200 else ""
    return f"{screen}{dark}{text}-{lang}-{width}"


all_jobs = [
    (s, lang, w, theme, scale)
    for scale in TEXT_SCALES
    for theme in THEMES
    for w in WIDTHS
    for lang in LANGS
    for s in ALL_SCREENS
]
if PLATFORM == "desktop":
    all_jobs.extend(
        (screen, "en-XA", 440, "light", 100) for screen in ALL_SCREENS
    )
selectors = {
    "DYAPP_PROTOTYPE_THEME": ["light", "dark"],
    "DYAPP_PROTOTYPE_TEXT_SCALE": ["100", "200"],
    "DYAPP_PROTOTYPE_LANGUAGE": ["en", "ru"],
}
selected = {key: os.environ.get(key, "all") for key in selectors}
for key, choices in selectors.items():
    if selected[key] not in ["all", *choices]:
        raise SystemExit(f"{key} must be all or one of {choices}")
jobs = [
    job
    for job in all_jobs
    if job[0] in SCREENS
    and (
        PLATFORM != "desktop"
        or (
            selected["DYAPP_PROTOTYPE_THEME"] in ("all", job[3])
            and selected["DYAPP_PROTOTYPE_TEXT_SCALE"] in ("all", str(job[4]))
            and selected["DYAPP_PROTOTYPE_LANGUAGE"]
            in ("all", "en" if job[1] == "en-XA" else job[1])
        )
    )
]
if not jobs:
    raise SystemExit("No prototype states selected")
expected_shots = [job_name(job) for job in jobs]
if len(all_jobs) != len({job_name(job) for job in all_jobs}):
    raise SystemExit("Prototype screenshot names collide")
total = len(jobs)
failed: list[str] = []
saved_count = 0
self_checked = False
keyboard_checked = False
layout: dict[str, object] = {}
criteria: dict[str, object] = {}
# A loopback origin lets the shared identifier ES module load normally.
print("prototype: starting loopback server", file=sys.stderr, flush=True)
server = ThreadingHTTPServer(
    ("127.0.0.1", 0), partial(SimpleHTTPRequestHandler, directory="/assets")
)
threading.Thread(target=server.serve_forever, daemon=True).start()
origin = f"http://127.0.0.1:{server.server_port}"


def stalled(phase: str) -> None:
    failed.append(f"{phase}: completion deadline")
    Gtk.main_quit()


def schedule(callback: Callable[[], bool]) -> int:
    faulthandler.dump_traceback_later(PHASE_LIMIT_SECONDS, exit=True)
    return GLib.timeout_add_seconds(PHASE_LIMIT_SECONDS, callback)


progress = Progress(
    schedule,
    GLib.source_remove,
    stalled,
)


def phase(name: str) -> None:
    label = f"{shot_name()}: {name}"
    print(label, file=sys.stderr, flush=True)
    progress.begin(label)


print("prototype: constructing GTK window", file=sys.stderr, flush=True)
window = Gtk.Window()
print("prototype: constructing WebKit view", file=sys.stderr, flush=True)
view = WebKit2.WebView()
window.add(view)
window.show_all()


def next_job() -> None:
    if not jobs:
        Gtk.main_quit()
        return
    screen, lang, width, theme, scale = jobs[0]
    phase("load")
    window.resize(width, HEIGHT)
    view.set_size_request(width, HEIGHT)
    # A changed hash alone is a same-page jump with no load event,
    # so the query differs too.
    view.load_uri(
        f"{origin}{SOURCE}?lang={lang}&theme={theme}&textScale={scale}"
        f"&shot={len(jobs)}#{screen}"
    )


def shot_name() -> "str":
    return job_name(jobs[0])


def snapshot() -> None:
    phase("snapshot")
    view.get_snapshot(
        WebKit2.SnapshotRegion.FULL_DOCUMENT,
        WebKit2.SnapshotOptions.NONE,
        None,
        saved,
    )


def measured(_view: WebKit2.WebView, result: Gio.AsyncResult) -> None:
    try:
        measured_layout = json.loads(
            view.call_async_javascript_function_finish(result).to_string()
        )
        _, lang, width, theme, scale = jobs[0]
        if PLATFORM == "android" or (
            lang in LANGS
            and width in (440, 1280)
            and theme == "light"
            and scale == 100
        ):
            layout[shot_name()] = measured_layout
    except GLib.Error as error:
        failed.append(f"{shot_name()} layout: {error.message}")
    if CRITERIA:
        phase("criteria")
        view.call_async_javascript_function(
            CRITERIA + (SELF_CHECK if not self_checked else "") + AUDIT,
            -1,
            None,
            None,
            None,
            None,
            audited,
        )
    else:
        snapshot()


def audited(_view: WebKit2.WebView, result: Gio.AsyncResult) -> None:
    global self_checked
    try:
        findings = json.loads(
            view.call_async_javascript_function_finish(result).to_string()
        )
        self_checked = True
        criteria[shot_name()] = findings
        for finding in findings:
            if finding["rule"].startswith(("axe", "T1:")):
                continue
            failed.append(
                f"{shot_name()} {finding['rule']} {finding['element']}: "
                f"{finding['detail']}"
            )
    except GLib.Error as error:
        failed.append(f"{shot_name()} criteria: {error.message}")
    snapshot()


def saved(_view: WebKit2.WebView, result: Gio.AsyncResult) -> None:
    global saved_count
    name = f"{OUT}/{shot_name()}.png"
    try:
        view.get_snapshot_finish(result).write_to_png(name)
        saved_count += 1
    except GLib.Error as error:
        failed.append(f"{name}: {error.message}")
    _, lang, width, theme, scale = jobs[0]
    if (
        PLATFORM == "desktop"
        and lang in LANGS
        and width in (440, 1280)
        and theme == "light"
        and scale == 100
    ):
        phase("keyboard")
        from prototype_keyboard import KeyboardAudit
        from prototype_keyboard_fixtures import KeyboardFixtures

        if keyboard_checked:
            KeyboardAudit(view, keyboard_done).start()
        else:
            KeyboardFixtures(view, keyboard_calibrated).start()
    else:
        finish_job()


def finish_job() -> None:
    progress.close()
    jobs.pop(0)
    next_job()


def keyboard_calibrated(errors: list[str]) -> None:
    global keyboard_checked
    if errors:
        keyboard_done(errors)
        return
    from prototype_keyboard import KeyboardAudit

    keyboard_checked = True
    KeyboardAudit(view, keyboard_done).start()


def keyboard_done(errors: list[str]) -> None:
    failed.extend(f"{shot_name()} keyboard: {error}" for error in errors)
    finish_job()


def loaded(_view: WebKit2.WebView, event: WebKit2.LoadEvent) -> None:
    if event == WebKit2.LoadEvent.FINISHED:
        phase("layout")
        window.present()
        view.grab_focus()
        view.call_async_javascript_function(
            READY_MEASURE, -1, None, None, None, None, measured
        )


def advance() -> bool:
    next_job()
    return False


def load_failed(
    _view: WebKit2.WebView,
    _event: WebKit2.LoadEvent,
    uri: str,
    error: GLib.Error,
) -> bool:
    failed.append(f"{uri}: {error.message}")
    progress.close()
    jobs.pop(0)
    GLib.idle_add(advance)
    return True


def terminated(_view: WebKit2.WebView, reason: object) -> None:
    progress.close()
    failed.append(f"{shot_name()}: web process terminated: {reason}")
    Gtk.main_quit()


view.connect("web-process-terminated", terminated)
view.connect("load-changed", loaded)
view.connect("load-failed", load_failed)
GLib.idle_add(advance)
Gtk.main()
progress.close()
server.shutdown()
server.server_close()
with open(f"{OUT}/layout.json", "w", encoding="utf-8") as out:
    json.dump(layout, out, indent=1, sort_keys=True)
if CRITERIA:
    with open(f"{OUT}/criteria.json", "w", encoding="utf-8") as out:
        json.dump(
            {
                "shots": criteria,
                "expected": expected_shots,
                "all_shots": [job_name(job) for job in all_jobs],
            },
            out,
            indent=1,
            sort_keys=True,
        )
for line in failed:
    print(line, file=sys.stderr)
print(f"{saved_count}/{total} screenshots in {OUT}")
faulthandler.cancel_dump_traceback_later()
sys.exit(1 if failed else 0)
