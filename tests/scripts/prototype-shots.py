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
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import sys
import threading

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
gi.require_version("JavaScriptCore", "4.1")
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402 -- versions set

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
  if (stickers && box(input)) {
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
      if (previous.getBoundingClientRect().left
          >= next.getBoundingClientRect().left)
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
    "wizard-identity", "wizard-network", "picker", "recovery",
    "profile", "profile-hidden", "profile-dark",
    "publish-dialog", "publish-progress",
    "network", "network-checking", "network-error",
    "search", "search-loading", "search-empty", "search-error",
    "profile-view", "profile-view-safety", "verify-dialog",
    "profile-view-verified", "profile-view-match", "unlike-dialog",
    "chats", "chat", "chat-draft", "chat-closed", "chat-first", "chat-request",
    "chat-new-device",
    "chat-menu", "chat-rename-dialog", "chat-clear-dialog", "chat-cleared",
    "delete-dialog",
    "settings", "settings-appearance", "settings-privacy",
    "settings-notifications",
    "settings-photos", "settings-node", "settings-account", "settings-about",
    "settings-identities-screen", "settings-devices", "settings-blocked",
    "settings-passed",
    "sticker-picker-empty", "sticker-picker-ready", "sticker-picker-recent",
    "sticker-picker-loading", "sticker-picker-error", "sticker-picker-sending",
    "chat-sticker-image", "chat-sticker-fallback", "chat-sticker-pending",
    "chat-sticker-downloading", "chat-sticker-failed",
    "chat-sticker-quota", "chat-sticker-saved",
    "chat-sticker-checking", "chat-sticker-fetch", "chat-sticker-storage",
    "chat-sticker-storage-idle", "chat-sticker-missing",
    "chat-sticker-read-error", "chat-sticker-request-error",
    "stickers-empty", "stickers-ready", "stickers-loading",
    "stickers-error", "stickers-importing", "stickers-quota",
    "stickers-remove-dialog", "stickers-remove-unreadable",
    "stickers-removing",
    "stickers-removed", "stickers-remove-error",
    "switcher", "menu", "remove-dialog", "remove-offline",
    "revoked-dialog", "wizard-network-link", "link-show", "link-offline",
    "link-expired", "link-finishing", "link-done", "link-done-pending",
    "chat-linked", "devices-sent", "device-code-dialog", "device-code-error",
    "device-code-bad",
    "device-confirm-dialog", "disconnect-dialog", "disconnect-offline",
]
ANDROID = [
    "lock-setup", "lock-unlock",
    "wizard-identity", "wizard-network", "picker", "recovery",
    "profile", "profile-hidden", "publish-dialog", "publish-progress",
    "network", "network-checking", "network-error",
    "search", "search-loading", "search-empty", "search-error",
    "profile-view", "profile-view-match", "unlike-dialog",
    "chats", "chat", "chat-closed", "chat-first", "chat-request",
    "chat-menu", "chat-clear-dialog", "chat-cleared", "delete-dialog",
    "settings", "lock-settings", "switcher", "remove-dialog", "remove-offline",
]
# Await actual font/layout readiness; the timer is a failure deadline.
READY_MEASURE = """
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
  return """ + MEASURE + """;
} finally { clearTimeout(timer); }
"""

PLATFORM = sys.argv[1] if len(sys.argv) > 1 else "desktop"
ALL_SCREENS = {"desktop": DESKTOP, "android": ANDROID}[PLATFORM]
requested = sys.argv[2:]
if len(requested) != len(set(requested)) or any(
        s not in ALL_SCREENS for s in requested):
    raise SystemExit("screen names must be unique and available "
                     "on this platform")
SCREENS = requested or ALL_SCREENS
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
layout = {}
# A loopback origin lets the shared identifier ES module load normally.
server = ThreadingHTTPServer(
    ("127.0.0.1", 0), partial(SimpleHTTPRequestHandler, directory="/assets"))
threading.Thread(target=server.serve_forever, daemon=True).start()
origin = f"http://127.0.0.1:{server.server_port}"
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
    view.load_uri(f"{origin}{SOURCE}?lang={lang}&theme={theme}"
                  f"&shot={len(jobs)}#{screen}")


def shot_name():
    screen, lang, width, theme = jobs[0]
    dark = "-dark" if theme == "dark" else ""
    return f"{screen}{dark}-{lang}-{width}"


def snapshot():
    view.get_snapshot(WebKit2.SnapshotRegion.FULL_DOCUMENT,
                      WebKit2.SnapshotOptions.NONE, None, saved)


def measured(_view, result):
    try:
        layout[shot_name()] = json.loads(
            view.call_async_javascript_function_finish(result).to_string())
    except GLib.Error as error:
        failed.append(f"{shot_name()} layout: {error.message}")
    snapshot()


def saved(_view, result):
    name = f"{OUT}/{shot_name()}.png"
    jobs.pop(0)
    try:
        view.get_snapshot_finish(result).write_to_png(name)
    except GLib.Error as error:
        failed.append(f"{name}: {error.message}")
    next_job()


def loaded(_view, event):
    if event == WebKit2.LoadEvent.FINISHED:
        view.call_async_javascript_function(
            READY_MEASURE, -1, None, None, None, None, measured)


def load_failed(_view, _event, uri, error):
    failed.append(f"{uri}: {error.message}")
    jobs.pop(0)
    GLib.idle_add(lambda: next_job() and False)
    return True


view.connect("load-changed", loaded)
view.connect("load-failed", load_failed)
GLib.idle_add(lambda: next_job() and False)
Gtk.main()
server.shutdown()
server.server_close()
with open(f"{OUT}/layout.json", "w", encoding="utf-8") as out:
    json.dump(layout, out, indent=1, sort_keys=True)
for line in failed:
    print(line, file=sys.stderr)
print(f"{total - len(failed)} screenshots in {OUT}")
sys.exit(1 if failed else 0)
