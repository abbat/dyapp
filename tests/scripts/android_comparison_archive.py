"""Validate bounded native frames and complete comparison line coverage."""

import argparse
import json
import re
import struct
import tarfile
from itertools import product
from pathlib import Path
from typing import Any

from android_visual_archive import MAX_BYTES, allowed_size, native_scale

STATES = ("choices", "preview", "saving", "refused", "photo_unavailable")
FIELDS = [
    "NAME",
    "LOCATION",
    "INCOME",
    "HAS_KIDS",
    "WANTS_MORE_KIDS",
    "INTERESTS",
    "PHOTOS",
    "EDUCATION",
    "PROFESSION",
    "COMPANY",
    "INDUSTRY",
    "CAREER_STAGE",
    "WORK_LIFE_BALANCE",
    "BIRTH",
    "GENDER",
    "ORIENTATION",
    "RELATIONSHIP_GOAL",
    "ROLE_PREFERENCE",
    "CAREER_AMBITION",
    "FINANCIAL_PHILOSOPHY",
    "EMPLOYMENT_STATUS",
    "FITNESS_LEVEL",
    "ZODIAC",
    "HEIGHT",
    "ABOUT",
    "SMOKING",
    "DRINKING",
    "TATTOOS",
    "MARITAL_STATUS",
    "PETS",
    "SEEKING",
]


def unpack(archive: Path, output: Path, width: int) -> None:
    """Derive every output path from a bounded allow-listed archive member."""
    if archive.stat().st_size > MAX_BYTES:
        raise ValueError("Comparison archive exceeds its limit")
    prefix = f"files/comparison-visual/{width}"
    folders = {"files", "files/comparison-visual", prefix}
    folders.update(f"{prefix}/{language}" for language in ("en", "ru"))
    pattern = re.compile(
        re.escape(prefix)
        + r"/(en|ru)/(manifest\.json|[a-z_]+-(en|ru)-"
        + str(width)
        + r"-(light|dark)-(100|200)-[0-9]{1,4}\.png)"
    )
    with tarfile.open(archive, "r:") as source:
        members = source.getmembers()
        names = [member.name.rstrip("/") for member in members]
        if len(members) > 4096 or len(names) != len(set(names)):
            raise ValueError("Duplicate or excessive comparison members")
        if (
            sum(allowed_size(item, folders, pattern) for item in members)
            > MAX_BYTES
        ):
            raise ValueError("Comparison content exceeds its limit")
        output.mkdir(parents=True, exist_ok=False)
        for member in members:
            if member.isfile():
                match = pattern.fullmatch(member.name)
                if match is None:
                    raise ValueError("Unexpected comparison file")
                target = output / match[1] / match[2]
                target.parent.mkdir(exist_ok=True)
                stream = source.extractfile(member)
                if stream is None:
                    raise ValueError("Missing comparison content")
                with stream, target.open("xb") as destination:
                    destination.write(stream.read())


def line_set(value: object) -> set[int]:
    """Zero-based actual line numbers are finite, unique and bounded."""
    if not isinstance(value, list) or not value or len(value) > 4096:
        raise ValueError("Missing or excessive comparison lines")
    if any(type(line) is not int or not 0 <= line < 4096 for line in value):
        raise ValueError("Invalid comparison line index")
    if len(set(value)) != len(value):
        raise ValueError("Duplicate comparison line index")
    return set(value)


def frame(
    directory: Path, value: dict[str, Any], name: str, width: int
) -> None:
    if value.get("file") != name or value.get("viewport_width_dp") != width:
        raise ValueError("Comparison frame identity disagrees with scene")
    native_scale(value, name)
    data = (directory / name).read_bytes()
    if len(data) < 24 or data[:16] != b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR":
        raise ValueError("Invalid comparison PNG header")
    dimensions = struct.unpack(">II", data[16:24])
    if min(dimensions) <= 0 or dimensions != (
        value.get("pixel_width"),
        value.get("pixel_height"),
    ):
        raise ValueError("Comparison PNG dimensions disagree with manifest")


def required_lines(value: dict[str, Any], name: str) -> dict[str, set[int]]:
    required = value.get("required")
    sources = (
        ("REMOTE",) if name.startswith("preview-") else ("LOCAL", "REMOTE")
    )
    expected = {
        f"link-review-{field}-{source}-value"
        for field, source in product(FIELDS, sources)
    }
    if (
        value.get("complete") is not True
        or not isinstance(required, dict)
        or set(required) != expected
    ):
        raise ValueError("Incomplete comparison value inventory")
    required = {tag: line_set(lines) for tag, lines in required.items()}
    if any(lines != set(range(len(lines))) for lines in required.values()):
        raise ValueError(
            "Comparison required lines must be contiguous from zero"
        )
    return required


def scene(
    directory: Path, value: dict[str, Any], name: str, width: int
) -> set[str]:
    required = required_lines(value, name)
    frames = value.get("frames")
    if not isinstance(frames, list) or not 2 <= len(frames) <= 1024:
        raise ValueError("Missing or excessive comparison viewports")
    covered: dict[str, set[int]] = {tag: set() for tag in required}
    files = set()
    for index, entry in enumerate(frames):
        file = f"{name}-{index}.png"
        frame(directory, entry, file, width)
        files.add(file)
        visible = entry.get("visible")
        if (
            not isinstance(visible, dict)
            or not set(visible) <= required.keys()
        ):
            raise ValueError("Unexpected visible comparison values")
        for tag, lines in visible.items():
            actual = line_set(lines)
            if not actual <= required[tag]:
                raise ValueError(
                    "Visible comparison line is not in the actual layout"
                )
            covered[tag].update(actual)
    if covered != required:
        raise ValueError("Saved viewports miss required comparison lines")
    photo_coverage(frames, name)
    return files


def photo_coverage(frames: list[dict[str, Any]], name: str) -> None:
    sources = (
        ("REMOTE",) if name.startswith("preview-") else ("LOCAL", "REMOTE")
    )
    suffix = "-retry" if name.startswith("photo_unavailable-") else ""
    expected = {f"link-review-photo-{side}-0{suffix}" for side in sources}
    actual = {
        entry["photo"] for entry in frames if entry.get("photo") is not None
    }
    if actual != expected:
        raise ValueError(
            "Missing comparison photo or unavailable-photo control"
        )


def validate(output: Path, width: int, source: str) -> int:
    count = 0
    if {path.name for path in output.iterdir()} != {"en", "ru"}:
        raise ValueError("Comparison language inventory mismatch")
    for language in ("en", "ru"):
        directory = output / language
        manifest = json.loads((directory / "manifest.json").read_text())
        identity = {
            "source_sha": source,
            "requested_width_dp": width,
            "language": language,
        }
        if manifest.get("complete") is not True or any(
            manifest.get(key) != expected for key, expected in identity.items()
        ):
            raise ValueError("Incomplete or mismatched comparison manifest")
        expected = {
            f"{state}-{language}-{width}-{theme}-{font}"
            for state, theme, font in product(
                STATES, ("light", "dark"), (100, 200)
            )
        }
        scenes = manifest.get("scenes")
        if not isinstance(scenes, list) or len(scenes) != len(expected):
            raise ValueError("Missing comparison scenes")
        seen, files = set(), {"manifest.json"}
        for value in scenes:
            name = value.get("scene")
            if (
                not isinstance(name, str)
                or name not in expected
                or name in seen
            ):
                raise ValueError("Unexpected or duplicate comparison scene")
            seen.add(name)
            files.update(scene(directory, value, name, width))
        if {path.name for path in directory.iterdir()} != files:
            raise ValueError("Missing or unlisted comparison frames")
        count += len(files) - 1
    return count


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--width", type=int, choices=(360, 412), required=True)
    parser.add_argument("--source", required=True)
    args = parser.parse_args()
    if re.fullmatch(r"[0-9a-f]{40}", args.source) is None:
        parser.error("source must be an exact lowercase commit SHA")
    unpack(args.archive, args.output, args.width)
    count = validate(args.output, args.width, args.source)
    print(
        f"OK comparison matrix: {count} frames; all actual value lines covered"
    )


if __name__ == "__main__":
    main()
