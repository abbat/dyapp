"""Install pinned test bytes; native code must validate proof."""

from hashlib import sha256
import os
from pathlib import Path
import re


def install(source: Path, destination: Path, expected_sha256: str) -> None:
    """Refuse changed bytes or existing destinations; never generate a key."""
    if re.fullmatch(r"[0-9a-f]{64}", expected_sha256) is None:
        raise ValueError("Invalid pinned fixture digest")
    if source.is_symlink() or not source.is_file():
        raise ValueError("Fixture must be a regular file")
    with source.open("rb") as stream:
        data = stream.read(4097)
    if (
        not data
        or len(data) > 4096
        or sha256(data).hexdigest() != expected_sha256
    ):
        raise ValueError(
            "Public node fixture bytes do not match the pinned digest"
        )
    descriptor = os.open(
        destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600
    )
    with os.fdopen(descriptor, "wb") as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
