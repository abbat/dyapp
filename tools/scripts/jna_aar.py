"""Preserve JNA Java resources with protected Android JNI libraries."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
from io import BytesIO
import json
from pathlib import Path, PurePosixPath
import stat
from typing import TypedDict
import zipfile

ABIS = {"arm64-v8a": 183, "x86_64": 62}
LIBRARY = "libjnidispatch.so"


class RepackReport(TypedDict):
    upstream_sha256: str
    classes_sha256: str
    sha256: str
    native_sha256: dict[str, str]


def native_payloads(directory: Path) -> dict[str, bytes]:
    """Validate JNI payloads with the shared Android ELF policy."""
    spec = importlib.util.spec_from_file_location(
        "jna_hardening", Path(__file__).with_name("check-hardening.py")
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("Hardening policy loader unavailable")
    policy = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(policy)
    payloads = {}
    for abi, machine in ABIS.items():
        data = (directory / abi / LIBRARY).read_bytes()
        if len(data) < 64 or int.from_bytes(data[18:20], "little") != machine:
            raise ValueError(f"{abi}: wrong JNI ELF architecture")
        issues = policy.elf_problems(data, shared=True)
        if issues:
            raise ValueError(f"{abi}: {'; '.join(issues)}")
        payloads[f"jni/{abi}/{LIBRARY}"] = data
    return payloads


def rewrite(source: Path, native: Path, output: Path) -> RepackReport:
    """Reject invalid inputs before writing; retain non-JNI bytes."""
    replacements = native_payloads(native)
    upstream = source.read_bytes()
    with zipfile.ZipFile(BytesIO(upstream)) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise ValueError("Duplicate JNA AAR entries")
        for info in archive.infolist():
            path = PurePosixPath(info.filename)
            mode = stat.S_IFMT(info.external_attr >> 16)
            if (
                path.is_absolute()
                or ".." in path.parts
                or "\\" in info.filename
                or mode not in (0, stat.S_IFREG, stat.S_IFDIR)
            ):
                raise ValueError(f"Unsafe JNA AAR entry: {info.filename}")
        for name in ("classes.jar", "AndroidManifest.xml", *replacements):
            if name not in names or archive.getinfo(name).file_size == 0:
                raise ValueError(f"Missing or empty JNA AAR entry: {name}")
        preserved = {
            name: archive.read(name)
            for name in names
            if not name.startswith("jni/")
        }
    with zipfile.ZipFile(output, "x") as archive:
        for name, data in sorted((preserved | replacements).items()):
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            mode = 0o40755 if name.endswith("/") else 0o100644
            info.external_attr = (mode << 16) | (
                0x10 if name.endswith("/") else 0
            )
            archive.writestr(
                info, data, compress_type=zipfile.ZIP_DEFLATED, compresslevel=9
            )
    return {
        "upstream_sha256": hashlib.sha256(upstream).hexdigest(),
        "classes_sha256": hashlib.sha256(preserved["classes.jar"]).hexdigest(),
        "sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
        "native_sha256": {
            name.split("/")[1]: hashlib.sha256(data).hexdigest()
            for name, data in replacements.items()
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--native-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--report", required=True, type=Path)
    args = parser.parse_args()
    report = rewrite(args.source, args.native_dir, args.output)
    with args.report.open("x") as output:
        output.write(json.dumps(report, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
