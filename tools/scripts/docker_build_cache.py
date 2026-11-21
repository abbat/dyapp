"""Bounded evidence and recovery commands for optional cache failures."""

from collections.abc import Sequence
from pathlib import Path
import re

from capture import HEAD, TAIL

MISSING_BLOB = re.compile(
    r"^#\d+ ERROR: blob sha256:[0-9a-f]{64}: not found\r?$", re.MULTILINE
)


def read_build_log(path: Path) -> str:
    """Use the same bounded head/tail policy as normal stage diagnostics."""
    with path.open("rb") as stream:
        head = stream.read(HEAD)
        stream.seek(0, 2)
        size = stream.tell()
        tail_at = max(len(head), size - TAIL)
        stream.seek(tail_at)
        tail = stream.read(TAIL)
    gap = tail_at - len(head)
    marker = f"\n[{gap} bytes omitted]\n".encode() if gap else b""
    return (head + marker + tail).decode(errors="replace")


def cache_recovery_command(
    command: Sequence[str], status: int, output: str
) -> list[str] | None:
    if (
        not 0 < status < 128
        or list(command[:3]) != ["docker", "buildx", "build"]
        or "--cache-from" not in command
        or not MISSING_BLOB.search(output)
    ):
        return None
    fresh = list(command)
    while "--cache-from" in fresh:
        index = fresh.index("--cache-from")
        if index + 1 >= len(fresh):
            return None
        if not fresh[index + 1].startswith("type="):
            return None
        del fresh[index : index + 2]
    if "--no-cache" not in fresh:
        fresh.insert(3, "--no-cache")
    return fresh
