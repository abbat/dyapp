"""Inventory existing debug APK bytes against the prepared JNA payload."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
from typing import TypedDict
import zipfile


class Producer(TypedDict):
    coordinate: str
    manifest_sha256: str
    aar_sha256: str
    native_sha256: dict[str, str]
    manifest: dict[str, object]


class Apk(TypedDict):
    filename: str
    sha256: str
    bytes: int
    native_sha256: dict[str, str]


def producer(directory: Path) -> Producer:
    raw = (directory / "manifest.json").read_bytes()
    manifest = json.loads(raw)
    if not isinstance(manifest, dict):
        raise ValueError("Invalid prepared JNA manifest")
    coordinate = manifest.get("coordinate")
    if (
        not isinstance(coordinate, str)
        or re.fullmatch(
            r"net.java.dev.jna:jna:[0-9]+\.[0-9]+\.[0-9]+", coordinate
        )
        is None
    ):
        raise ValueError("Invalid prepared JNA coordinate")
    filename = "jna-" + coordinate.rsplit(":", 1)[1] + "-hardened.aar"
    if manifest.get("filename") != filename:
        raise ValueError("Prepared JNA filename mismatch")
    expected = manifest.get("native_sha256")
    if not isinstance(expected, dict) or set(expected) != {
        "arm64-v8a",
        "x86_64",
    }:
        raise ValueError("Invalid prepared JNI inventory")
    hashes: dict[str, str] = {}
    for abi, value in expected.items():
        if (
            not isinstance(abi, str)
            or not isinstance(value, str)
            or re.fullmatch(r"[0-9a-f]{64}", value) is None
        ):
            raise ValueError("Invalid prepared JNI checksum")
        hashes[abi] = value
    with (directory / filename).open("rb") as stream:
        actual = hashlib.file_digest(stream, "sha256").hexdigest()
    if actual != manifest.get("sha256"):
        raise ValueError("Prepared JNA AAR checksum mismatch")
    return {
        "coordinate": coordinate,
        "manifest_sha256": hashlib.sha256(raw).hexdigest(),
        "aar_sha256": actual,
        "native_sha256": hashes,
        "manifest": manifest,
    }


def require_application_libraries(native: dict[str, str]) -> None:
    abis = {"x86_64"} | {name.split("/")[1] for name in native}
    for abi in sorted(abis):
        required = {
            f"lib/{abi}/libjnidispatch.so",
            f"lib/{abi}/libdyapp_ffi.so",
        }
        if not required <= native.keys():
            raise ValueError(f"Application APK lacks {abi} JNA or FFI")


def inventory(path: Path, prepared: Producer, application: bool) -> Apk:
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise ValueError("Duplicate APK entries")
        native: dict[str, str] = {}
        for name in names:
            if not name.startswith("lib/") or not name.endswith(".so"):
                continue
            parts = name.split("/")
            if len(parts) != 3 or parts[1] not in prepared["native_sha256"]:
                raise ValueError("Unsupported APK native entry: " + name)
            if archive.getinfo(name).file_size == 0:
                raise ValueError("Empty APK native payload: " + name)
            with archive.open(name) as stream:
                native_digest = hashlib.sha256()
                while chunk := stream.read(1024 * 1024):
                    native_digest.update(chunk)
                native[name] = native_digest.hexdigest()
            if (
                parts[2] == "libjnidispatch.so"
                and native[name] != prepared["native_sha256"][parts[1]]
            ):
                raise ValueError("Packaged JNA checksum mismatch")
        if application:
            require_application_libraries(native)
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return {
        "filename": path.name,
        "sha256": digest,
        "bytes": path.stat().st_size,
        "native_sha256": native,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--prepared", type=Path, default=Path("/opt/dyapp/jna-native")
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--application", type=Path, action="append", required=True
    )
    parser.add_argument("--test", type=Path, action="append", required=True)
    args = parser.parse_args()
    prepared = producer(args.prepared)
    applications = [
        inventory(path, prepared, True) for path in args.application
    ]
    tests = [inventory(path, prepared, False) for path in args.test]
    receipt = {
        "schema": 1,
        "producer": prepared,
        "applications": applications,
        "tests": tests,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    # A new instrumentation container must not reuse an earlier receipt.
    with args.output.open("x") as stream:
        stream.write(json.dumps(receipt, indent=2) + "\n")
    print(
        f"OK android-artifacts: {len(applications)} application "
        f"and {len(tests)} test packages"
    )


if __name__ == "__main__":
    main()
