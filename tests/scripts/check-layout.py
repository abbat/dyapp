#!/usr/bin/env python3
"""Check prototype gaps against layout criteria L2 and I1.

Reads the layout.json that prototype-shots.py writes next to the screenshots
(docs/design/review-criteria.md):
L2: a framed block (a control, a border or a background) is at least 8 px
    from its visible neighbour; a label or legend may sit closer to its field.
I1: an identifier block (.full-id, a block code) is at least 12 px from its
    neighbours.
A finding is "<rule> <scope> <first> | <second>", whatever the screen, language
or width. Known ones are listed under [layout] in baselines.toml; the check
fails on a new finding and names listed ones that no longer occur.

    check-layout.py <desktop|android> <layout.json>
"""

from collections.abc import Collection, Mapping, Sequence
import json
from pathlib import Path
import sys
import tomllib
from typing import TypedDict


class Gap(TypedDict):
    identifier: bool
    framed: bool
    label: bool
    gap: float
    scope: str
    a: str
    b: str


BASELINES = Path(__file__).with_name("baselines.toml")


def findings(layout: Mapping[str, Sequence[Gap]]) -> dict[str, list[str]]:
    """Each finding with the shots it occurs in."""
    found: dict[str, list[str]] = {}
    for shot, gaps in sorted(layout.items()):
        for gap in gaps:
            if gap["identifier"] and gap["gap"] < 12:
                rule = "I1"
            elif gap["framed"] and not gap["label"] and gap["gap"] < 8:
                rule = "L2"
            else:
                continue
            key = f"{rule} {gap['scope']} {gap['a']} | {gap['b']}".strip()
            found.setdefault(key, []).append(f"{shot} ({gap['gap']} px)")
    return found


def errors(
    found: Mapping[str, Sequence[str]],
    known: Collection[str],
) -> tuple[list[str], list[str]]:
    """New findings fail; known ones that are gone are reported as stale."""
    new = [
        f"{key}: {', '.join(shots[:3])}"
        for key, shots in found.items()
        if key not in known
    ]
    stale = [key for key in known if key not in found]
    return new, stale


def main(platform: str, path: str | Path) -> int:
    known = tomllib.loads(BASELINES.read_text())["layout"][platform]
    found = findings(json.loads(Path(path).read_text()))
    new, stale = errors(found, known)
    for key in stale:
        print(f"fixed, remove from [layout] {platform}: {key}")
    if new:
        print(
            "Layout (L2/I1, docs/design/review-criteria.md):",
            "\n".join(new),
            sep="\n",
            file=sys.stderr,
        )
        return 1
    print(f"layout: {len(found)} known findings, no new ones")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    sys.exit(main(*sys.argv[1:]))
