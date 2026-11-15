"""Prepare pinned regular-file-only JNA native sources."""

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass, fields
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile
from typing import BinaryIO, TypedDict
from urllib.request import urlopen
import zipfile

CONFIG = Path(__file__).resolve().parents[1] / "jna-native.json"
MAX_INPUT = 256 * 1024 * 1024


@dataclass(frozen=True)
class Inputs:
    version: str
    native_version: str
    commit: str
    source_sha256: str
    aar_sha256: str
    classes_sha256: str
    ndk_version: str


class Prepared(TypedDict):
    inputs: dict[str, str]
    files: dict[str, str]


def load(path: Path) -> Inputs:
    raw: dict[str, object] = json.loads(path.read_text())
    names = {field.name for field in fields(Inputs)}
    values = {
        key: value for key, value in raw.items() if isinstance(value, str)
    }
    if set(raw) != names or set(values) != names:
        raise ValueError("Invalid JNA input fields")
    for name, value in values.items():
        pattern = (
            r"[0-9a-f]{64}" if name.endswith("sha256") else r"\d+\.\d+\.\d+"
        )
        if name == "commit":
            pattern = r"[0-9a-f]{40}"
        if re.fullmatch(pattern, value) is None:
            raise ValueError(f"Invalid JNA input: {name}")
    return Inputs(**values)


def digest(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def transfer(source: BinaryIO, target: Path, expected: str) -> None:
    checksum = hashlib.sha256()
    total = 0
    with target.open("xb") as output:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            total += len(chunk)
            if total > MAX_INPUT:
                raise ValueError("JNA input exceeds the download budget")
            checksum.update(chunk)
            output.write(chunk)
    if checksum.hexdigest() != expected:
        raise ValueError(f"JNA input checksum mismatch: {target.name}")


def fetch(inputs: Inputs, destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=False)
    sources = [
        (
            "source.tar.gz",
            inputs.source_sha256,
            "https://codeload.github.com/java-native-access/jna/tar.gz/"
            f"{inputs.commit}",
        ),
        (
            "upstream.aar",
            inputs.aar_sha256,
            "https://repo.maven.apache.org/maven2/net/java/dev/jna/jna/"
            f"{inputs.version}/jna-{inputs.version}.aar",
        ),
    ]
    for name, expected, url in sources:
        with urlopen(url, timeout=60) as source:
            transfer(source, destination / name, expected)


def extract(archive: Path, destination: Path, commit: str) -> None:
    """Extract regular source files without links or special files."""
    destination.mkdir(exist_ok=False)
    prefix = "jna-" + commit
    selected = set()
    with tarfile.open(archive, "r:gz") as source:
        for member in source:
            path = PurePosixPath(member.name)
            if path.is_absolute() or ".." in path.parts or "\\" in member.name:
                raise ValueError("Unsafe JNA source path")
            if not path.parts or path.parts[0] != prefix:
                raise ValueError("Unexpected JNA source prefix")
            relative = PurePosixPath(*path.parts[1:])
            if not relative.parts or relative.parts[0] not in (
                "native",
                "src",
                "AL2.0",
                "LGPL2.1",
                "LICENSE",
            ):
                continue
            if relative in selected or not (member.isdir() or member.isfile()):
                raise ValueError("Duplicate or non-regular JNA source entry")
            selected.add(relative)
            target = destination.joinpath(*relative.parts)
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                content = source.extractfile(member)
                if content is None:
                    raise ValueError("Missing JNA source payload")
                with content, target.open("xb") as output:
                    shutil.copyfileobj(content, output)
                target.chmod(0o755 if member.mode & 0o111 else 0o644)


def snapshot(directory: Path) -> dict[str, str]:
    result = {}
    for path in sorted(directory.rglob("*")):
        if path.is_symlink():
            raise ValueError("Prepared JNA source contains a link")
        if path.is_file():
            result[path.relative_to(directory).as_posix()] = digest(path)
    return result


def prepare(inputs: Inputs, original: Path, work: Path) -> None:
    work.mkdir(parents=True, exist_ok=False)
    for name, expected in [
        ("source.tar.gz", inputs.source_sha256),
        ("upstream.aar", inputs.aar_sha256),
    ]:
        with (original / name).open("rb") as source:
            transfer(source, work / name, expected)
    with zipfile.ZipFile(work / "upstream.aar") as archive:
        if (
            hashlib.sha256(archive.read("classes.jar")).hexdigest()
            != inputs.classes_sha256
        ):
            raise ValueError("Unexpected JNA Java bytecode")
    extract(work / "source.tar.gz", work / "source", inputs.commit)
    subprocess.run(
        ["/bin/sh", "autogen.sh"],
        cwd=work / "source/native/libffi",
        check=True,
    )
    receipt: Prepared = {
        "inputs": asdict(inputs),
        "files": snapshot(work / "source"),
    }
    (work / "prepared.json").write_text(
        json.dumps(receipt, sort_keys=True) + "\n"
    )


def validate(inputs: Inputs, work: Path) -> None:
    receipt: Prepared = json.loads((work / "prepared.json").read_text())
    if receipt["inputs"] != asdict(inputs):
        raise ValueError("Prepared JNA input versions changed")
    if receipt["files"] != snapshot(work / "source"):
        raise ValueError("Prepared JNA source changed")
    if digest(work / "upstream.aar") != inputs.aar_sha256:
        raise ValueError("Prepared JNA AAR changed")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("step", choices=("fetch", "prepare"))
    parser.add_argument("--config", type=Path, default=CONFIG)
    parser.add_argument("--inputs", required=True, type=Path)
    parser.add_argument("--work", type=Path)
    args = parser.parse_args()
    inputs = load(args.config)
    if args.step == "fetch":
        fetch(inputs, args.inputs)
    else:
        if args.work is None:
            parser.error("prepare requires --work")
        prepare(inputs, args.inputs, args.work)


if __name__ == "__main__":
    main()
