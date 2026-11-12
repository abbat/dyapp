#!/usr/bin/env python3
"""Checks the hardening profile of a release ELF or Windows PE binary.

Usage: check-hardening.py <binary>. Fails unless the 64-bit little-endian ELF
is a position-independent executable (ET_DYN with an interpreter), its stack
is not executable, it has a RELRO segment and binds all symbols at start (full
RELRO), carries no RPATH or RUNPATH, and imports __stack_chk_fail. Rust code
gets the first four from its toolchain; the canary import shows that the
bundled C code (AWS-LC, SQLite) was built with Debian's dpkg-buildflags, as
Debian 12's gcc does not add -fstack-protector-strong by default.

A PE must be a 64-bit (PE32+) image that keeps its relocations and asks for
ASLR (DYNAMIC_BASE), 64-bit ASLR (HIGH_ENTROPY_VA) and DEP (NX_COMPAT); the
Rust MSVC toolchain sets all of them. Control Flow Guard is not required: Rust
leaves it off by default and the bundled std is not built with it.
"""

from collections.abc import Iterable
import os
import struct
import sys
import tempfile

PT_LOAD, PT_DYNAMIC, PT_INTERP = 1, 2, 3
PT_GNU_STACK, PT_GNU_RELRO = 0x6474E551, 0x6474E552
DT_STRTAB, DT_STRSZ, DT_RPATH = 5, 10, 15
DT_BIND_NOW, DT_RUNPATH, DT_FLAGS = 24, 29, 30
DT_FLAGS_1 = 0x6FFFFFFB
DF_BIND_NOW, DF_1_NOW = 0x8, 0x1
Segment = tuple[int, int, int, int, int]


def dynamic_tags(data: bytes, segments: Iterable[Segment]) -> dict[int, int]:
    tags: dict[int, int] = {}
    for kind, _, offset, _, size in segments:
        if kind != PT_DYNAMIC:
            continue
        for at in range(offset, offset + size - 15, 16):
            tag, value = struct.unpack_from("<qQ", data, at)
            if tag == 0:
                break
            tags.setdefault(tag, value)
    return tags


# IMAGE_FILE_RELOCS_STRIPPED, then the DllCharacteristics bits by name.
PE_RELOCS_STRIPPED = 0x1
PE_FLAGS = {0x40: "DYNAMIC_BASE", 0x20: "HIGH_ENTROPY_VA", 0x100: "NX_COMPAT"}


def pe_problems(data: bytes) -> list[str]:
    pe = struct.unpack_from("<I", data, 0x3C)[0] if len(data) >= 0x40 else 0
    if pe == 0 or data[pe : pe + 4] != b"PE\x00\x00" or len(data) < pe + 96:
        return ["not a PE image"]
    characteristics = struct.unpack_from("<H", data, pe + 22)[0]
    (magic,) = struct.unpack_from("<H", data, pe + 24)
    if magic != 0x20B:
        return ["not a 64-bit PE32+ image"]
    flags = struct.unpack_from("<H", data, pe + 24 + 70)[0]
    found = [f"no {name}" for bit, name in PE_FLAGS.items() if not flags & bit]
    if characteristics & PE_RELOCS_STRIPPED:
        found.append("relocations stripped, ASLR cannot move the image")
    return found


def problems(binary: str | os.PathLike[str]) -> list[str]:
    with open(binary, "rb") as f:
        data = f.read()
    return pe_problems(data) if data[:2] == b"MZ" else elf_problems(data)


def elf_problems(data: bytes) -> list[str]:
    if data[:6] != b"\x7fELF\x02\x01":
        return ["not a 64-bit little-endian ELF"]
    e_type = struct.unpack_from("<H", data, 16)[0]
    phoff = struct.unpack_from("<Q", data, 32)[0]
    phentsize, phnum = struct.unpack_from("<HH", data, 54)
    # (type, flags, offset, vaddr, filesz) of each program header.
    segments: list[Segment] = []
    for i in range(phnum):
        kind, flags, offset, vaddr, _, filesz = struct.unpack_from(
            "<IIQQQQ", data, phoff + i * phentsize
        )
        segments.append((kind, flags, offset, vaddr, filesz))
    kinds = {s[0] for s in segments}
    found = []
    if e_type != 3 or PT_INTERP not in kinds:
        found.append("not a PIE")
    stack = [s for s in segments if s[0] == PT_GNU_STACK]
    if not stack or stack[0][1] & 1:
        found.append("executable stack")
    if PT_GNU_RELRO not in kinds:
        found.append("no RELRO segment")
    tags = dynamic_tags(data, segments)
    bind_now = DT_BIND_NOW in tags or tags.get(DT_FLAGS, 0) & DF_BIND_NOW
    if not (bind_now or tags.get(DT_FLAGS_1, 0) & DF_1_NOW):
        found.append("lazy binding, RELRO is partial")
    if DT_RPATH in tags or DT_RUNPATH in tags:
        found.append("RPATH or RUNPATH set")
    strtab = next(
        (
            offset + tags[DT_STRTAB] - vaddr
            for kind, _, offset, vaddr, size in segments
            if kind == PT_LOAD and DT_STRTAB in tags
            if vaddr <= tags[DT_STRTAB] < vaddr + size
        ),
        None,
    )
    names = (
        data[strtab : strtab + tags.get(DT_STRSZ, 0)]
        if strtab is not None
        else b""
    )
    if b"__stack_chk_fail" not in names.split(b"\x00"):
        found.append(
            "no stack protector in the C code (__stack_chk_fail not imported)"
        )
    return found


FIX = "docs/manuals/bootstrap.md#getting-dyappd"


def main(raw: bool = False) -> None:
    (binary,) = sys.argv[1:]
    found = problems(binary)
    if found:
        sys.exit(f"{binary}: {'; '.join(found)}")
    with open(binary, "rb") as f:
        pe = f.read(2) == b"MZ"
    profile = (
        "PE32+, ASLR, high-entropy ASLR, DEP, relocations kept"
        if pe
        else "PIE, NX, full RELRO, no RPATH, stack protector"
    )
    print(f"{binary}: {profile}" if raw else "OK hardening")


if __name__ == "__main__":
    raw = "--raw" in sys.argv[1:]
    if raw:
        sys.argv.remove("--raw")
        main(raw=True)
    else:
        try:
            main()
        except (Exception, SystemExit) as error:
            # Copied alone into the release image; tests bind FIX to
            # check-output.json.
            message = str(error)
            directory = os.environ.get(
                "DYAPP_CHECK_LOG_DIR", "/tmp/ai/check-output"
            )
            log = None
            try:
                os.makedirs(directory, exist_ok=True)
                with tempfile.NamedTemporaryFile(
                    mode="w",
                    prefix="hardening-",
                    suffix=".log",
                    dir=directory,
                    delete=False,
                ) as output:
                    output.write(message + "\n")
                    log = output.name
            except OSError as log_error:
                message += f"\nCannot retain diagnostic: {log_error}"
            cause = message.splitlines()[0][:512] if message else "exit 1"
            print(f"FAIL hardening: {cause}\nFIX: {FIX}\nDETAILS:")
            print(message[:65536])
            if log:
                print(f"{log}: raw captured output")
            sys.exit(1)
