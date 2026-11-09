"""Install hash-locked Python type-check libraries without system packages."""

from __future__ import annotations

import argparse
from collections.abc import Iterator
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import platform
import re
import stat
import sys
import tarfile
from typing import TypedDict
from urllib.parse import urlparse
from urllib.request import urlopen
import zipfile

LOCK = Path(__file__).resolve().parents[1] / "python-types/requirements.json"
MARKER = ".dyapp-python-types.json"
MAX_DOWNLOAD = 32 * 1024 * 1024
MAX_FILE = 64 * 1024 * 1024
MAX_UNPACKED = 256 * 1024 * 1024
PROJECTS = {
    "mypy",
    "typing_extensions",
    "mypy_extensions",
    "pathspec",
    "librt",
    "ast-serialize",
    "types-PyYAML",
    "Pillow",
    "PyGObject-stubs",
}


class Artifact(TypedDict):
    project: str
    version: str
    abi: str
    kind: str
    url: str
    sha256: str


class Requirements(TypedDict):
    schema: int
    platform: str
    abis: list[str]
    gi_versions: dict[str, str]
    artifacts: list[Artifact]


def load_lock(path: Path) -> Requirements:
    data = path.read_bytes()
    if len(data) > 64 * 1024:
        raise ValueError("Python type-check lock exceeds size limit")
    lock: Requirements = json.loads(data)
    if lock["schema"] != 1 or lock["platform"] != "linux-x86_64":
        raise ValueError("Unsupported Python type-check lock")
    return lock


def select(
    lock: Requirements, abi: str, system: str, machine: str
) -> list[Artifact]:
    if system != "linux" or machine != "x86_64" or abi not in lock["abis"]:
        raise ValueError(
            f"Unsupported Python checker platform: {system}/{machine}/{abi}"
        )
    result = [
        item for item in lock["artifacts"] if item["abi"] in ("any", abi)
    ]
    names = [item["project"] for item in result]
    if set(names) != PROJECTS or len(names) != len(PROJECTS):
        raise ValueError(
            "Incomplete or duplicated Python checker dependencies"
        )
    return result


def download(artifact: Artifact) -> bytes:
    url = urlparse(artifact["url"])
    if url.scheme != "https" or url.netloc != "files.pythonhosted.org":
        raise ValueError(
            "Python checker artifact must use official PyPI files"
        )
    with urlopen(artifact["url"], timeout=60) as response:
        data: bytes = response.read(MAX_DOWNLOAD + 1)
    if len(data) > MAX_DOWNLOAD:
        raise ValueError("Python checker artifact exceeds download limit")
    if hashlib.sha256(data).hexdigest() != artifact["sha256"]:
        raise ValueError("Python checker artifact checksum mismatch")
    return data


def safe_path(name: str) -> str:
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts or "\\" in name:
        raise ValueError("Unsafe Python checker archive path")
    if not path.parts or path.as_posix() != name:
        raise ValueError("Noncanonical Python checker archive path")
    return name


def wheel_files(data: bytes) -> Iterator[tuple[str, bytes]]:
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        for member in archive.infolist():
            if member.is_dir():
                continue
            if stat.S_IFMT(member.external_attr >> 16) not in (
                0,
                stat.S_IFREG,
            ):
                raise ValueError(
                    "Python checker wheel contains a special file"
                )
            if member.file_size > MAX_FILE:
                raise ValueError(
                    "Python checker archive file exceeds size limit"
                )
            yield safe_path(member.filename), archive.read(member)


def gi_files(data: bytes, version: str) -> Iterator[tuple[str, bytes]]:
    prefix = f"pygobject_stubs-{version}/"
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for member in archive:
            if member.name == prefix + "LICENSE":
                relative = "gi-stubs/LICENSE"
            elif member.name.startswith(prefix + "src/gi-stubs/"):
                relative = member.name.removeprefix(prefix + "src/")
            else:
                continue
            if member.isdir():
                continue
            if not member.isfile() or member.size > MAX_FILE:
                raise ValueError("Unsafe GI stub archive member")
            source = archive.extractfile(member)
            assert source is not None, "GI stub member has no content"
            yield safe_path(relative), source.read()


def unpack(
    artifact: Artifact, data: bytes, contents: dict[str, bytes]
) -> None:
    if artifact["kind"] == "wheel":
        files = wheel_files(data)
    elif artifact["kind"] == "gi-stubs":
        files = gi_files(data, artifact["version"])
    else:
        raise ValueError("Unknown Python checker artifact kind")
    total = sum(map(len, contents.values()))
    for name, value in files:
        total += len(value)
        if name in contents or total > MAX_UNPACKED:
            raise ValueError("Duplicate or oversized Python checker archive")
        contents[name] = value


def configure_gi(contents: dict[str, bytes], versions: dict[str, str]) -> None:
    # Reproduce the upstream backend's version wrapper selection, without
    # executing downloaded build code or installing the runtime GI libraries.
    for name, version in versions.items():
        if (
            not re.fullmatch(r"[A-Za-z][A-Za-z0-9]*", name)
            or not version.isdigit()
        ):
            raise ValueError("Invalid GI stub version selection")
        folder = "gi-stubs/repository"
        if f"{folder}/_{name}{version}.pyi" not in contents:
            raise ValueError("Missing configured GI stub version")
        contents[f"{folder}/{name}.pyi"] = (
            f"from ._{name}{version} import *"
        ).encode()


def install(destination: Path, lock_path: Path = LOCK) -> None:
    if destination.exists() or destination.is_symlink():
        raise ValueError("Python checker destination already exists")
    if sys.implementation.name != "cpython":
        raise ValueError("Python type checking requires CPython")
    abi = f"cp{sys.version_info.major}{sys.version_info.minor}"
    lock = load_lock(lock_path)
    artifacts = select(lock, abi, sys.platform, platform.machine())
    contents: dict[str, bytes] = {}
    for artifact in artifacts:
        unpack(artifact, download(artifact), contents)
    configure_gi(contents, lock["gi_versions"])
    destination.mkdir(parents=True)
    for name, value in sorted(contents.items()):
        path = destination / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(value)
        path.chmod(0o644)
    marker = {
        "abi": abi,
        "lock_sha256": hashlib.sha256(lock_path.read_bytes()).hexdigest(),
    }
    (destination / MARKER).write_text(json.dumps(marker) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--lock", type=Path, default=LOCK)
    arguments = parser.parse_args()
    install(arguments.destination, arguments.lock)
