"""Read bounded native visual archives without trusting archive paths."""

import argparse
from itertools import product
import json
from pathlib import Path
import re
import struct
import tarfile

SCREENS = (
    "owner-code",
    "owner-malformed",
    "owner-expired",
    "owner-preparing",
    "owner-prepare-error",
    "owner-confirm",
    "owner-confirm-expired",
    "owner-sending",
    "owner-send-error",
    "owner-send-expired",
    "owner-send-check-error",
    "owner-sent",
)
MAX_BYTES = 512 * 1024 * 1024


def unpack(archive: Path, output: Path, width: int) -> None:
    if archive.stat().st_size > MAX_BYTES:
        raise ValueError("Native visual archive exceeds its limit")
    prefix = f"files/owner-approval-visual/{width}"
    folders = {"files", "files/owner-approval-visual", prefix}
    pattern = re.compile(
        re.escape(prefix)
        + r"/(manifest\.json|owner-[a-z-]+-(en|ru)-"
        + str(width)
        + r"-(light|dark)-(100|200)-(top|bottom)\.png)"
    )
    with tarfile.open(archive, "r:") as source:
        members = source.getmembers()
        names = [member.name.rstrip("/") for member in members]
        if len(members) > 256 or len(names) != len(set(names)):
            raise ValueError("Duplicate or excessive archive members")
        total = 0
        for member, name in zip(members, names, strict=True):
            if member.isdir() and name in folders:
                continue
            if not member.isfile() or pattern.fullmatch(name) is None:
                raise ValueError("Unexpected native archive entry")
            limit = 1024 * 1024 if name.endswith(".json") else 16 * 1024 * 1024
            total += member.size
            if not 0 < member.size <= limit or total > MAX_BYTES:
                raise ValueError("Native archive entry exceeds its limit")
        output.mkdir(parents=True, exist_ok=False)
        for member in members:
            if member.isfile():
                stream = source.extractfile(member)
                if stream is None:
                    raise ValueError("Missing native archive content")
                with (
                    stream,
                    (output / Path(member.name).name).open("xb") as target,
                ):
                    target.write(stream.read())


def validate(output: Path, width: int, source: str) -> int:
    manifest = json.loads((output / "manifest.json").read_text())
    if (
        manifest.get("source_sha") != source
        or manifest.get("requested_width_dp") != width
        or manifest.get("complete") is not True
    ):
        raise ValueError("Incomplete or mismatched native visual manifest")
    expected = {
        f"{screen}-{language}-{width}-{theme}-{font}-{edge}.png"
        for screen, language, theme, font, edge in product(
            SCREENS,
            ("en", "ru"),
            ("light", "dark"),
            (100, 200),
            ("top", "bottom"),
        )
    }
    frames = manifest.get("frames")
    if not isinstance(frames, list):
        raise ValueError("Missing native frame inventory")
    actual = set()
    for frame in frames:
        name = frame.get("file")
        if not isinstance(name, str) or name not in expected or name in actual:
            raise ValueError("Unexpected or duplicate native frame")
        actual.add(name)
        data = (output / name).read_bytes()
        if data[:16] != b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR":
            raise ValueError("Invalid native PNG header")
        dimensions = struct.unpack(">II", data[16:24])
        if dimensions != (frame.get("pixel_width"), frame.get("pixel_height")):
            raise ValueError("Native PNG dimensions disagree with manifest")
        if min(dimensions) <= 0 or frame.get("viewport_width_dp") != width:
            raise ValueError("Native viewport mismatch")
    if actual != expected or {p.name for p in output.iterdir()} != expected | {
        "manifest.json"
    }:
        raise ValueError("Incomplete native visual matrix")
    return len(actual)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--width", type=int, choices=(360, 412), required=True)
    parser.add_argument("--source", required=True)
    args = parser.parse_args()
    if re.fullmatch(r"[0-9a-f]{40}", args.source) is None:
        parser.error("Source must be an exact commit SHA")
    unpack(args.archive, args.output, args.width)
    count = validate(args.output, args.width, args.source)
    print(f"OK native visual matrix: {count} frames")


if __name__ == "__main__":
    main()
