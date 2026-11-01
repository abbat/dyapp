#!/usr/bin/env python3
"""Print bounded compiler errors from cargo's JSON stream.

Used after a failed test build.
"""

import json
import sys
from pathlib import Path


def main(path: Path) -> None:
    shown = 0
    for line in path.open():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        message = event.get("message", {})
        if event.get("reason") != "compiler-message":
            continue
        if message.get("level") != "error":
            continue
        rendered = message.get("rendered") or message.get("message")
        print((rendered or "compiler error")[:8192], file=sys.stderr)
        shown += 1
        if shown == 8:
            break
    if not shown:
        print("cargo test failed without compiler-message errors",
              file=sys.stderr)


if __name__ == "__main__":
    main(Path(sys.argv[1]))
