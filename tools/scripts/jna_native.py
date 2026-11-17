"""Build the pinned protected JNA AAR with unchanged Java classes."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import zipfile

from jna_aar import rewrite
from jna_source import CONFIG, Inputs, load, validate


@dataclass(frozen=True)
class Target:
    abi: str
    arch: str
    triple: str


TARGETS = (
    Target("arm64-v8a", "aarch64", "aarch64-linux-android"),
    Target("x86_64", "x86-64", "x86_64-linux-android"),
)
LINK_FLAGS = (
    "-shared -o $@ -Wl,-Bsymbolic,--build-id=sha1,-z,relro,-z,now,"
    "-z,noexecstack,-z,max-page-size=16384,-z,common-page-size=16384,"
    "--no-undefined"
)


def headers(inputs: Inputs, work: Path) -> None:
    with zipfile.ZipFile(work / "upstream.aar") as archive:
        (work / "classes.jar").write_bytes(archive.read("classes.jar"))
    version = subprocess.check_output(
        [
            "javap",
            "-constants",
            "-classpath",
            str(work / "classes.jar"),
            "com.sun.jna.Version",
        ],
        text=True,
    )
    for name, value in [
        ("VERSION", inputs.version),
        ("VERSION_NATIVE", inputs.native_version),
    ]:
        if f'{name} = "{value}"' not in version:
            raise ValueError(f"JNA Java/native version mismatch: {name}")
    for directory in ("headers", "classes", "empty-sourcepath"):
        (work / directory).mkdir()
    source = work / "source/src/com/sun/jna"
    subprocess.run(
        [
            "javac",
            "--release",
            "8",
            "-h",
            str(work / "headers"),
            "-d",
            str(work / "classes"),
            "-classpath",
            str(work / "classes.jar"),
            "-sourcepath",
            str(work / "empty-sourcepath"),
            str(source / "Native.java"),
            str(source / "Function.java"),
        ],
        check=True,
    )
    for name in ("Native", "Function"):
        if not (work / f"headers/com_sun_jna_{name}.h").is_file():
            raise ValueError(f"Missing JNI header: {name}")


def exports(toolchain: Path, library: Path) -> set[str]:
    text = subprocess.check_output(
        [str(toolchain / "llvm-nm"), "-D", "--defined-only", str(library)],
        text=True,
    )
    return {
        line.split()[-1]
        for line in text.splitlines()
        if line.split()
        and (
            line.split()[-1].startswith("Java_")
            or line.split()[-1] == "JNI_OnLoad"
        )
    }


def compile_target(
    inputs: Inputs, work: Path, toolchain: Path, target: Target
) -> None:
    native = work / "native" / target.abi
    native.mkdir(parents=True)
    ffi = work / "source/native/libffi"
    cc = toolchain / f"{target.triple}31-clang"
    environment = dict(
        os.environ,
        AR=str(toolchain / "llvm-ar"),
        RANLIB=str(toolchain / "llvm-ranlib"),
        CXX=str(toolchain / f"{target.triple}31-clang++"),
        READELF=str(toolchain / "llvm-readelf"),
        SOURCE_DATE_EPOCH="0",
    )
    flags = (
        "-O2 -D_FORTIFY_SOURCE=2 -fno-omit-frame-pointer -fno-strict-aliasing "
        "-fstack-protector-strong -fpic -ffunction-sections "
        "-funwind-tables -fno-short-enums -Wno-implicit-function-declaration "
        f"-ffile-prefix-map={ffi}=/usr/src/jna/native/libffi "
        f"-ffile-prefix-map={native}=/usr/src/jna/build/{target.abi}"
    )
    # JNA's API fingerprint uses MD5; it does not authenticate inputs.
    checksum = hashlib.md5(
        b"".join(
            path.read_bytes()
            for path in sorted((work / "headers").glob("*.h"))
        ),
        usedforsecurity=False,
    ).hexdigest()
    command = [
        "make",
        "-j1",
        "-f",
        str(work / "source/native/Makefile"),
        "-f",
        str(work / "native-hardening.mk"),
        "OS=android",
        f"ARCH={target.arch}",
        f"BUILD={native}",
        f"FFI_SRC={ffi}",
        f"CC={cc}",
        f"CPP={cc} -E",
        f"RANLIB={toolchain / 'llvm-ranlib'}",
        f"STRIP={toolchain / 'llvm-strip'} -x",
        f"JAVAH={work / 'headers'}",
        f"CINCLUDES=-I{work / 'headers'} -I{native / 'libffi/include'}",
        f"COPT={flags}",
        "LIBS=-ldl -lm",
        f"JNA_JNI_VERSION={inputs.native_version}",
        f"CHECKSUM={checksum}",
        "FFI_CONFIG=--enable-static --disable-shared --with-pic=yes "
        f"--disable-docs --host={target.triple}",
        "HOST_CONFIG=--build=x86_64-pc-linux-gnu",
        str(native / "libjnidispatch.so"),
    ]
    subprocess.run(
        command, cwd=work / "source/native", env=environment, check=True
    )


def verify_exports(work: Path, toolchain: Path, target: Target) -> list[str]:
    native = work / "native" / target.abi
    original = native / "upstream.so"
    with zipfile.ZipFile(work / "upstream.aar") as archive:
        original.write_bytes(
            archive.read(f"jni/{target.abi}/libjnidispatch.so")
        )
    expected = exports(toolchain, original)
    actual = exports(toolchain, native / "libjnidispatch.so")
    if not expected or actual != expected:
        raise ValueError(f"JNA JNI export mismatch: {target.abi}")
    return sorted(actual)


def build(inputs: Inputs, work: Path, ndk: Path, output: Path) -> None:
    work = work.resolve()
    ndk = ndk.resolve()
    if any(
        re.fullmatch(r"[A-Za-z0-9_./-]+", str(path)) is None
        for path in (work, ndk)
    ):
        raise ValueError(
            "JNA make paths cannot contain spaces or metacharacters"
        )
    validate(inputs, work)
    expected = f"Pkg.Revision = {inputs.ndk_version}"
    if expected not in (ndk / "source.properties").read_text().splitlines():
        raise ValueError("Unexpected JNA NDK version")
    output.mkdir(parents=True, exist_ok=False)
    toolchain = ndk / "toolchains/llvm/prebuilt/linux-x86_64/bin"
    headers(inputs, work)
    # Shared-library link flags must not leak into libffi's compiler probes.
    (work / "native-hardening.mk").write_text(
        "unexport LDFLAGS\nLDFLAGS = " + LINK_FLAGS + "\n"
    )
    symbols = {}
    for target in TARGETS:
        compile_target(inputs, work, toolchain, target)
        symbols[target.abi] = verify_exports(work, toolchain, target)
    filename = f"jna-{inputs.version}-hardened.aar"
    report = rewrite(work / "upstream.aar", work / "native", output / filename)
    manifest = {
        "schema": 1,
        "coordinate": f"net.java.dev.jna:jna:{inputs.version}",
        "upstream_filename": f"jna-{inputs.version}.aar",
        "filename": filename,
        "native_version": inputs.native_version,
        "source_commit": inputs.commit,
        "source_sha256": inputs.source_sha256,
        "ndk_version": inputs.ndk_version,
        "jni_exports": symbols,
        **report,
    }
    (output / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, default=CONFIG)
    parser.add_argument("--work", required=True, type=Path)
    parser.add_argument("--ndk", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    build(load(args.config), args.work, args.ndk, args.output)


if __name__ == "__main__":
    main()
