#!/usr/bin/env python3
"""Print a Markdown inventory of the desktop UI (clients/desktop/ui).

Read-only. Per top-level <section> it lists every control with its id,
data-i18n key, English and Russian text and default hidden/disabled state,
then the lines of the page scripts that assign `.hidden` or `.disabled` to that
control. Output is deterministic:

    python3 -B tools/scripts/ui-inventory.py
"""
import argparse
import json
from html.parser import HTMLParser
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
UI = ROOT / "clients/desktop/ui"
VOID = {"input", "meta", "br", "hr", "img", "link"}
CONTROLS = {"button", "input", "select", "textarea", "fieldset", "form",
            "details", "output", "option"}
SCRIPTS = ("app.js", "search.js", "network.js")
SHOWN = 8  # script condition lines listed per control


class Page(HTMLParser):
    def __init__(self):
        super().__init__()
        self.stack = []     # (tag, attrs, label dict or None)
        self.rows = []      # one dict per control, in document order
        self.labels = {}    # `for` id -> i18n key

    def hidden(self, attrs):
        if "hidden" in attrs:
            return "yes"
        for tag, parent, _ in reversed(self.stack):
            if "hidden" in parent:
                return f"via #{parent.get('id', tag)}"
        return "no"

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        label = next((e[2] for e in reversed(self.stack)
                      if e[0] == "label"), None)
        key = attrs.get("data-i18n")
        if tag == "label" and "for" not in attrs:
            label = {}
        if tag == "label" and "for" in attrs and key:
            self.labels[attrs["for"]] = key
        if key and label is not None and "key" not in label \
                and tag != "label":
            label["key"] = key
        screen = next((e[1].get("id") for e in self.stack
                       if e[0] == "section"), None)
        if tag == "section" and screen is None:
            screen = attrs.get("id", "?")
        status = attrs.get("role") in ("status", "alert")
        listed = tag in CONTROLS or status or tag == "section"
        if listed and (attrs.get("id") or tag != "option" or key):
            kind = tag
            if tag == "input":
                kind = f"input[{attrs.get('type', 'text')}]"
            if status and tag not in CONTROLS:
                kind = f"{tag}[role={attrs['role']}]"
            self.rows.append({
                "screen": screen or "(main)", "id": attrs.get("id", "-"),
                "kind": kind, "key": key, "label": label,
                "hidden": self.hidden(attrs),
                "disabled": "yes" if "disabled" in attrs else "no"})
        if tag not in VOID:
            self.stack.append((tag, attrs, label))

    def handle_endtag(self, tag):
        while self.stack and self.stack[-1][0] != tag:
            self.stack.pop()
        if self.stack:
            self.stack.pop()


def cell(text):
    return (text or "").replace("|", "\\|").replace("\n", " ") or "-"


def conditions(name, js):
    """Map element id -> [(file:line, code)] of `.hidden`/`.disabled` sets."""
    names = dict(re.findall(
        r"const (\w+) = document\.getElementById\('([^']+)'\)", js))
    # ponytail: line regex only; loops (`for (const each of ...)
    # each.hidden`) and querySelectorAll groups are not attributed.
    # Upgrade: parse the JS with a real parser.
    assign = re.compile(
        r"(\w+|getElementById\('[^']+'\))\.(?:hidden|disabled)\s*=[^=]")
    found = {}
    for number, line in enumerate(js.splitlines(), 1):
        for target in assign.findall(line):
            match = re.match(r"getElementById\('([^']+)'\)", target)
            ident = match.group(1) if match else names.get(target)
            if ident:
                found.setdefault(ident, []).append(
                    (f"{name}:{number}", line.strip()))
    return found


def main():
    argparse.ArgumentParser(
        description=__doc__.splitlines()[0]).parse_args()
    en = json.loads((UI / "i18n/en.json").read_text(encoding="utf-8"))
    ru = json.loads((UI / "i18n/ru.json").read_text(encoding="utf-8"))
    page = Page()
    page.feed((UI / "index.html").read_text(encoding="utf-8"))
    cond = {}
    for name in SCRIPTS:
        js = (UI / name).read_text(encoding="utf-8")
        for ident, items in conditions(name, js).items():
            cond.setdefault(ident, []).extend(items)
    screens = []
    for row in page.rows:
        if row["screen"] not in screens:
            screens.append(row["screen"])
        row["key"] = row["key"] or page.labels.get(row["id"]) or (
            row["label"] or {}).get("key")
    print("# Desktop UI inventory\n")
    print("Source: clients/desktop/ui/index.html, app.js, search.js, "
          "network.js, i18n/en.json, i18n/ru.json.\n")
    for screen in screens:
        print(f"## {screen if screen == '(main)' else '#' + screen}\n")
        print("| id | element | i18n key | English | Russian | hidden "
              "| disabled |")
        print("|---|---|---|---|---|---|---|")
        rows = [r for r in page.rows if r["screen"] == screen]
        for r in rows:
            k = r["key"]
            print(f"| {cell(r['id'])} | {r['kind']} | {cell(k)} | "
                  f"{cell(en.get(k))} | {cell(ru.get(k))} | "
                  f"{r['hidden']} | {r['disabled']} |")
        lines = [(r["id"], cond[r["id"]]) for r in rows if r["id"] in cond]
        if lines:
            print("\nConditions in the scripts:\n")
        for ident, items in lines:
            for number, code in items[:SHOWN]:
                code = code.replace('`', chr(39))[:90]
                print(f"- `{ident}` {number}: `{code}`")
            if len(items) > SHOWN:
                print(f"- `{ident}` +{len(items) - SHOWN} more")
        print()
    keys = {r["key"] for r in page.rows if r["key"]}
    missing = sorted(keys - set(en) | keys - set(ru))
    if missing:
        listed = ", ".join(f"`{k}`" for k in missing)
        print(f"Keys missing from a catalogue: {listed}")


if __name__ == "__main__":
    main()
