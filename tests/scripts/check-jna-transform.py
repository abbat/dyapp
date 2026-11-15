"""Check real Gradle JNA transforms and cache invalidation offline."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
from typing import TypedDict
import zipfile

ROOT = Path(__file__).resolve().parents[2]


class Case(TypedDict):
    name: str
    directory: str
    message: str


def digest(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def gradle_binary() -> Path:
    properties = (
        ROOT / "clients/android/gradle/wrapper/gradle-wrapper.properties"
    ).read_text()
    version = re.search(r"gradle-([0-9.]+)-bin\.zip", properties)
    if version is None:
        raise ValueError("Missing pinned Gradle distribution")
    cache = Path(os.environ["GRADLE_USER_HOME"])
    pinned = version[1]
    candidates = list(
        cache.glob(
            f"wrapper/dists/gradle-{pinned}-bin/*/gradle-{pinned}/bin/gradle"
        )
    )
    if len(candidates) != 1:
        raise ValueError("Pinned Gradle executable is absent or ambiguous")
    return candidates[0]


def text(value: object) -> str:
    if not isinstance(value, str):
        raise ValueError("Expected JNA provenance text")
    return value


def variant(work: Path, name: str, changes: dict[str, object]) -> Path:
    destination = work / "cases" / name
    shutil.copytree(work / "prepared", destination)
    manifest: dict[str, object] = json.loads(
        (destination / "manifest.json").read_text()
    )
    manifest.update(changes)
    (destination / "manifest.json").write_text(json.dumps(manifest))
    return destination


def mutate(path: Path, name: str, body: bytes) -> None:
    with zipfile.ZipFile(path) as archive:
        payloads = {entry: archive.read(entry) for entry in archive.namelist()}
    payloads[name] = body
    with zipfile.ZipFile(path, "w") as archive:
        for entry, data in sorted(payloads.items()):
            archive.writestr(
                zipfile.ZipInfo(entry, (1980, 1, 1, 0, 0, 0)), data
            )


def cases(work: Path, filename: str) -> list[Case]:
    result: list[Case] = []
    settings: list[tuple[str, dict[str, object], str]] = [
        ("schema", {"schema": 2}, "Unsupported provenance"),
        (
            "version",
            {"coordinate": "net.java.dev.jna:jna:invalid"},
            "version mismatch",
        ),
        ("filename", {"filename": "../escape.aar"}, "filename mismatch"),
        (
            "upstream",
            {"upstream_sha256": "0" * 64},
            "Upstream JNA AAR checksum",
        ),
        ("checksum", {"sha256": "0" * 64}, "Prepared JNA AAR checksum"),
        ("native-map", {"native_sha256": {}}, "Invalid JNI hashes"),
    ]
    for name, changes, message in settings:
        directory = variant(work, name, changes)
        result.append(
            {"name": name, "directory": str(directory), "message": message}
        )
    for name, entry, body, message in [
        (
            "java",
            "classes.jar",
            b"changed Java",
            "Java or resource bytes changed",
        ),
        (
            "native",
            "jni/x86_64/libjnidispatch.so",
            b"changed JNI",
            "JNI checksum mismatch",
        ),
        (
            "inventory",
            "jni/x86/libjnidispatch.so",
            b"unsupported",
            "JNI payload inventory",
        ),
    ]:
        directory = variant(work, name, {})
        mutate(directory / filename, entry, body)
        path = directory / "manifest.json"
        manifest: dict[str, object] = json.loads(path.read_text())
        manifest["sha256"] = digest(directory / filename)
        path.write_text(json.dumps(manifest))
        result.append(
            {"name": name, "directory": str(directory), "message": message}
        )
    missing = work / "cases/missing"
    missing.mkdir()
    result.append(
        {
            "name": "missing",
            "directory": str(missing),
            "message": "manifest.json",
        }
    )
    return result


def seed(work: Path, prepared: Path) -> tuple[str, str, int]:
    shutil.copytree(prepared, work / "prepared")
    manifest: dict[str, object] = json.loads(
        (prepared / "manifest.json").read_text()
    )
    coordinate = text(manifest["coordinate"])
    version = coordinate.removeprefix("net.java.dev.jna:jna:")
    if re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version) is None:
        raise ValueError("Unexpected JNA coordinate")
    filename = f"jna-{version}.aar"
    if manifest["upstream_filename"] != filename:
        raise ValueError("Unexpected JNA filename")
    cache = Path(os.environ["GRADLE_USER_HOME"]) / "caches/modules-2/files-2.1"
    originals = sorted(
        path
        for path in (cache / "net.java.dev.jna/jna" / version).glob(
            f"*/{filename}"
        )
        if digest(path) == manifest["upstream_sha256"]
    )
    if not originals:
        raise ValueError("Pinned original JNA AAR is not cached")
    repository = work / "repo/net/java/dev/jna/jna" / version
    repository.mkdir(parents=True)
    shutil.copyfile(originals[0], repository / filename)
    (repository / f"jna-{version}.pom").write_text(
        "<project><modelVersion>4.0.0</modelVersion>"
        "<groupId>net.java.dev.jna</groupId><artifactId>jna</artifactId>"
        f"<version>{version}</version><packaging>aar</packaging></project>"
    )
    source = (ROOT / "clients/android/harden-jna.gradle").read_text()
    fixture = (ROOT / "tests/fixtures/jna-transform.gradle").read_text()
    (work / "build.gradle").write_text(source + "\n" + fixture)
    prepared_name = text(manifest["filename"])
    samples = cases(work, prepared_name)
    (work / "cases.json").write_text(json.dumps(samples))
    return coordinate, prepared_name, len(samples)


def run(gradle: Path, work: Path, label: str) -> int:
    with (work / f"{label}.log").open("w") as output:
        result = subprocess.run(
            [
                str(gradle),
                "--offline",
                "--no-daemon",
                "--max-workers=1",
                "-Pdyapp.jnaNativeDir=" + str(work / "prepared"),
                "verifyJnaTransform",
            ],
            cwd=work,
            stdout=output,
            stderr=subprocess.STDOUT,
            timeout=180,
            check=False,
        )
    return result.returncode


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gradle", type=Path)
    parser.add_argument(
        "--prepared", type=Path, default=Path("/opt/dyapp/jna-native")
    )
    parser.add_argument(
        "--work", type=Path, default=ROOT / "build/jna-transform"
    )
    args = parser.parse_args()
    base = args.work.resolve()
    prepared = args.prepared.resolve()
    if base.is_relative_to(prepared):
        raise ValueError(
            "JNA test workspace must be outside its prepared input"
        )
    base.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(dir=base))
    coordinate, filename, count = seed(work, prepared)
    gradle = (args.gradle or gradle_binary()).resolve()
    if run(gradle, work, "positive") != 0:
        raise RuntimeError(
            f"JNA Gradle fixture failed: {work / 'positive.log'}"
        )
    report = json.loads((work / "report.json").read_text())
    if report["cases"] != count or report["coordinate"] != coordinate:
        raise ValueError("Incomplete JNA transform fixture inventory")
    with (work / "prepared" / filename).open("ab") as output:
        output.write(b"cache-invalidating change")
    if run(gradle, work, "changed-cache") == 0:
        raise ValueError("Changed JNA input reused a successful transform")
    if (
        "Prepared JNA AAR checksum mismatch"
        not in (work / "changed-cache.log").read_text()
    ):
        raise RuntimeError(
            f"Unexpected JNA cache failure: {work / 'changed-cache.log'}"
        )
    (work / "passed.json").write_text(
        json.dumps(
            {
                "negative_cases": count,
                "coordinate": coordinate,
                "cache_change_rejected": True,
            },
            indent=2,
        )
        + "\n"
    )
    print(f"OK jna-transform: {count} rejection cases and cache invalidation")


if __name__ == "__main__":
    main()
