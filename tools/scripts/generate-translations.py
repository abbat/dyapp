"""Generate native catalogues, or check output without changing it."""

import argparse
from pathlib import Path
import sys

from translation_delivery import deliver


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root", type=Path, default=Path(__file__).resolve().parents[2]
    )
    parser.add_argument(
        "--platform", action="append", choices=("android", "apple", "desktop")
    )
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    platforms = args.platform or ["android", "apple", "desktop"]
    try:
        changed = deliver(args.root, platforms, check=args.check)
    except (ValueError, OSError) as error:
        print(f"Translation generation failed: {error}", file=sys.stderr)
        return 2
    if args.check and changed:
        for path in changed:
            print(f"Generated translation differs: {path}", file=sys.stderr)
        return 1
    action = "differences" if args.check else "updated"
    print(f"OK translations ({len(changed)} {action})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
