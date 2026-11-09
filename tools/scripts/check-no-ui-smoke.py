"""Reject native release artifacts containing injected UI smoke hooks."""

from pathlib import Path
import sys

MARKERS = (b"ui_test_result", b"UI_SMOKE_PASS:", b"UI_SMOKE_FAIL:")


def check(path: Path) -> None:
    data = path.read_bytes()
    if not data:
        raise ValueError(f"Empty native artifact: {path}")
    for marker in MARKERS:
        if marker in data:
            raise ValueError(f"UI smoke hook in {path}: {marker.decode()}")


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("Usage: check-no-ui-smoke.py <native-binary>")
    try:
        check(Path(sys.argv[1]))
    except (OSError, ValueError) as error:
        raise SystemExit(str(error)) from error
    print("Native artifact contains no injected UI smoke hooks")


if __name__ == "__main__":
    main()
