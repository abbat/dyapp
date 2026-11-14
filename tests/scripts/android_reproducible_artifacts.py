"""Inventory Android archives and native payloads without normalization."""

from __future__ import annotations

import hashlib
import importlib.util
from pathlib import Path
from functools import cache
from typing import TYPE_CHECKING, IO, TypedDict, cast
from types import ModuleType
import zipfile

if TYPE_CHECKING:
    import policy_contracts as pc
    import quality_contracts as qc

ROOT = Path(__file__).resolve().parents[2]
PACKAGES = (("apk", "app-release-unsigned.apk"), ("aab", "app-release.aab"))


@cache
def validator() -> ModuleType:
    spec = importlib.util.spec_from_file_location(
        "reproducible_android_packages",
        ROOT / "tools/scripts/check-android-release.py",
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("Android package validator is unavailable")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def stream_digest(stream: IO[bytes]) -> str:
    digest = hashlib.sha256()
    while chunk := stream.read(1024 * 1024):
        digest.update(chunk)
    return digest.hexdigest()


def inventory(directory: Path) -> dict[str, pc.ReproducedArtifact]:
    result: dict[str, pc.ReproducedArtifact] = {}
    for kind, name in PACKAGES:
        path = directory / name
        verified = cast("qc.AndroidArtifact", validator().verify(path, kind))
        result[name] = {
            "sha256": verified["sha256"],
            "bytes": verified["bytes"],
        }
        with zipfile.ZipFile(path) as archive:
            for info in archive.infolist():
                if not info.filename.endswith(".so"):
                    continue
                with archive.open(info) as stream:
                    sha = stream_digest(stream)
                result[f"{name}!/{info.filename}"] = {
                    "sha256": sha,
                    "bytes": info.file_size,
                }
    return result


class EntryFacts(TypedDict):
    sha256: str
    bytes: int
    compressed_bytes: int
    timestamp: list[int]
    format: list[int]
    attributes: list[int]
    offset: int
    extra: str
    comment: str


class EntryChange(TypedDict):
    name: str
    first: EntryFacts | None
    second: EntryFacts | None


class ArchiveDifference(TypedDict):
    package: str
    order_changed: bool
    comments: list[str]
    entries: list[EntryChange]


def entry_facts(archive: zipfile.ZipFile, info: zipfile.ZipInfo) -> EntryFacts:
    with archive.open(info) as stream:
        sha = stream_digest(stream)
    return {
        "sha256": sha,
        "bytes": info.file_size,
        "compressed_bytes": info.compress_size,
        "timestamp": list(info.date_time),
        "format": [
            info.compress_type,
            info.flag_bits,
            info.create_system,
            info.extract_version,
        ],
        "attributes": [info.internal_attr, info.external_attr],
        "offset": info.header_offset,
        "extra": info.extra.hex(),
        "comment": info.comment.hex(),
    }


def archive_difference(first: Path, second: Path) -> ArchiveDifference:
    with zipfile.ZipFile(first) as a, zipfile.ZipFile(second) as b:
        left = {info.filename: entry_facts(a, info) for info in a.infolist()}
        right = {info.filename: entry_facts(b, info) for info in b.infolist()}
        return {
            "package": first.name,
            "order_changed": a.namelist() != b.namelist(),
            "comments": [a.comment.hex(), b.comment.hex()],
            "entries": [
                {
                    "name": name,
                    "first": left.get(name),
                    "second": right.get(name),
                }
                for name in sorted(left.keys() | right.keys())
                if left.get(name) != right.get(name)
            ],
        }


def changed(
    first: dict[str, pc.ReproducedArtifact],
    second: dict[str, pc.ReproducedArtifact],
) -> list[str]:
    return sorted(
        name
        for name in first.keys() | second.keys()
        if first.get(name) != second.get(name)
    )
