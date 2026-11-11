"""Checks that a desktop release binary embeds the web UI compressed: a
64-byte slice from the middle of every UI file of at least 1 KiB must not
appear in the binary as plain text. Tauri brotli-compresses the embedded
assets by default; the UI has no minifying build step because of it.

Usage: check-embedded-ui.py BINARY [UI_DIR]."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
UI = ROOT / "clients/desktop/ui"
SLICE = 64
MIN_FILE = 1024


def samples(ui: Path) -> dict[str, bytes]:
    """{relative path: middle slice} of every UI file of at least 1 KiB."""
    found = {}
    for path in sorted(ui.rglob("*")):
        data = path.read_bytes() if path.is_file() else b""
        if len(data) >= MIN_FILE:
            middle = len(data) // 2
            found[str(path.relative_to(ui))] = data[middle : middle + SLICE]
    return found


def plain(binary: bytes, ui: Path) -> list[str]:
    """UI files whose slice appears verbatim in the binary."""
    return [name for name, chunk in samples(ui).items() if chunk in binary]


def main(argv: list[str]) -> int:
    if len(argv) not in (2, 3):
        print(__doc__, file=sys.stderr)
        return 2
    ui = Path(argv[2]) if len(argv) == 3 else UI
    checked = samples(ui)
    if not checked:
        print(f"FAIL no UI files of at least {MIN_FILE} bytes in {ui}")
        return 1
    found = plain(Path(argv[1]).read_bytes(), ui)
    if found:
        print("FAIL UI embedded uncompressed: " + ", ".join(found[:10]))
        return 1
    print(f"OK embedded UI compressed: {len(checked)} files checked")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
