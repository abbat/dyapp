"""Validate every destination before writing reproducible native resources."""

from collections.abc import Sequence
from pathlib import Path, PurePosixPath
from xml.etree import ElementTree

from translation_renderers import ANDROID, APPLE, DESKTOP, plan
from translation_sources import MAX_BYTES, load


def destination(root: Path, relative: PurePosixPath) -> Path:
    if relative.is_absolute() or ".." in relative.parts:
        raise ValueError(f"Unsafe catalogue destination: {relative}")
    path = root.joinpath(*relative.parts)
    for candidate in (path, *path.parents):
        if candidate == root:
            break
        if candidate.is_symlink():
            raise ValueError(
                f"Catalogue destination uses a symlink: {relative}"
            )
        if candidate.exists():
            valid = (
                candidate.is_file()
                if candidate == path
                else candidate.is_dir()
            )
            if not valid:
                raise ValueError(f"Invalid catalogue destination: {relative}")
    return path


def content(path: Path) -> bytes:
    with path.open("rb") as stream:
        data = stream.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise ValueError(f"Native catalogue exceeds {MAX_BYTES} bytes: {path}")
    return data


def existing(root: Path, platforms: Sequence[str]) -> set[Path]:
    result: set[Path] = set()
    if "desktop" in platforms:
        result.update((root / DESKTOP).glob("*.json"))
    if "apple" in platforms:
        result.update((root / APPLE.parent).glob("*.xcstrings"))
    if "android" in platforms:
        for path in (root / ANDROID).glob("values*/*.xml"):
            destination(root, PurePosixPath(path.relative_to(root).as_posix()))
            try:
                tree = ElementTree.fromstring(content(path))
            except ElementTree.ParseError as error:
                raise ValueError(
                    f"Invalid Android resource XML: {path}"
                ) from error
            if any(
                item.tag in ("string", "plurals", "string-array")
                or (item.tag == "item" and item.get("type") == "string")
                for item in tree
            ):
                result.add(path)
    return result


def deliver(
    root: Path, platforms: Sequence[str], *, check: bool
) -> list[PurePosixPath]:
    """Check is read-only; generation never deletes unmanaged native files."""
    root = root.resolve(strict=True)
    outputs = plan(load(root), platforms)
    targets = {
        destination(root, relative): value
        for relative, value in outputs.items()
    }
    unmanaged = existing(root, platforms) - set(targets)
    if unmanaged:
        names = sorted(path.relative_to(root).as_posix() for path in unmanaged)
        raise ValueError(f"Unmanaged native translation files: {names}")
    changed = [
        path
        for path, data in targets.items()
        if not path.exists() or content(path) != data
    ]
    if not check:
        for path in changed:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(targets[path])
    return [
        PurePosixPath(path.relative_to(root).as_posix()) for path in changed
    ]
