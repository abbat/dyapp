#!/usr/bin/env python3
"""Checks the hardening profile of a release ELF, Windows PE or Mach-O binary.

Usage: check-hardening.py [--android-apk] <binary>. A desktop ELF must be
64-bit little-endian and a position-independent executable (ET_DYN with an
interpreter), its stack is not executable, it has a RELRO segment and binds
all symbols at start (full
RELRO), carries no RPATH or RUNPATH, and imports __stack_chk_fail. Rust code
gets the first four from its toolchain; the canary import shows that the
bundled C code (AWS-LC, SQLite) was built with Debian's dpkg-buildflags, as
Debian 12's gcc does not add -fstack-protector-strong by default.

A PE must be a 64-bit (PE32+) image that keeps its relocations and asks for
ASLR (DYNAMIC_BASE), 64-bit ASLR (HIGH_ENTROPY_VA) and DEP (NX_COMPAT); the
Rust MSVC toolchain sets all of them. Control Flow Guard is not required: Rust
leaves it off by default and the bundled std is not built with it.

A Mach-O must be a universal binary with arm64 and x86_64 slices, and each
slice a 64-bit PIE executable that does not allow an executable stack, has no
LC_RPATH and imports ___stack_chk_fail. Apple clang adds the stack protector
to the bundled C code by default. 64-bit Mach-O heaps are never executable,
and the hardened runtime is a code-signing flag the unsigned ZIP cannot carry.

The Android APK profile requires both core ABIs and checks every packaged JNI
library as ET_DYN without an interpreter, NX stack, full RELRO, no RPATH or
text relocations, a canary import, no writable executable load, and 16 KiB
PT_LOAD alignment. APK ZIP alignment is checked separately with SDK zipalign.
"""

from collections.abc import Iterable
import os
import struct
import sys
import tempfile
import zipfile

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


MH_EXECUTE, MH_PIE, MH_ALLOW_STACK_EXECUTION = 2, 0x200000, 0x20000
LC_SYMTAB, LC_RPATH = 0x2, 0x8000001C
MACHO_CPUS = {0x0100000C: "arm64", 0x01000007: "x86_64"}


def macho_slice_problems(data: bytes) -> list[str]:
    if data[:4] != b"\xcf\xfa\xed\xfe" or len(data) < 32:
        return ["not a 64-bit Mach-O"]
    _, _, _, filetype, ncmds, _, flags, _ = struct.unpack_from(
        "<IiiIIIII", data
    )
    found = []
    if filetype != MH_EXECUTE or not flags & MH_PIE:
        found.append("not a PIE")
    if flags & MH_ALLOW_STACK_EXECUTION:
        found.append("executable stack")
    names: list[bytes] = []
    at = 32
    for _ in range(ncmds):
        cmd, size = struct.unpack_from("<II", data, at)
        if cmd == LC_RPATH:
            found.append("LC_RPATH set")
        if cmd == LC_SYMTAB:
            stroff, strsize = struct.unpack_from("<II", data, at + 16)
            names = data[stroff : stroff + strsize].split(b"\x00")
        at += max(size, 8)
    if b"___stack_chk_fail" not in names:
        found.append(
            "no stack protector in the C code (___stack_chk_fail not imported)"
        )
    return found


def macho_problems(data: bytes) -> list[str]:
    (count,) = struct.unpack_from(">I", data, 4)
    slices = {}
    for i in range(count):
        cpu, _, offset, size, _ = struct.unpack_from(
            ">IIIII", data, 8 + 20 * i
        )
        slices[MACHO_CPUS.get(cpu, hex(cpu))] = data[offset : offset + size]
    if sorted(slices) != sorted(MACHO_CPUS.values()):
        return [f"slices {', '.join(sorted(slices))}, not arm64 and x86_64"]
    return [
        f"{cpu}: {problem}"
        for cpu, body in sorted(slices.items())
        for problem in macho_slice_problems(body)
    ]


def problems(
    binary: str | os.PathLike[str], android_apk: bool = False
) -> list[str]:
    if android_apk:
        return apk_problems(binary)
    with open(binary, "rb") as f:
        data = f.read()
    if data[:2] == b"MZ":
        return pe_problems(data)
    if data[:4] == b"\xca\xfe\xba\xbe":
        return macho_problems(data)
    return elf_problems(data)


def elf_problems(data: bytes, shared: bool = False) -> list[str]:
    if len(data) < 64 or data[:6] != b"\x7fELF\x02\x01":
        return ["not a 64-bit little-endian ELF"]
    e_type = struct.unpack_from("<H", data, 16)[0]
    phoff = struct.unpack_from("<Q", data, 32)[0]
    phentsize, phnum = struct.unpack_from("<HH", data, 54)
    # (type, flags, offset, vaddr, filesz) of each program header.
    segments: list[Segment] = []
    if phentsize < 56 or not phnum or phoff + phnum * phentsize > len(data):
        return ["invalid ELF program headers"]
    for i in range(phnum):
        kind, flags, offset, vaddr, _, filesz = struct.unpack_from(
            "<IIQQQQ", data, phoff + i * phentsize
        )
        if offset + filesz > len(data):
            return ["ELF segment extends beyond payload"]
        segments.append((kind, flags, offset, vaddr, filesz))
    kinds = {s[0] for s in segments}
    found = []
    if shared:
        if e_type != 3 or PT_INTERP in kinds:
            found.append("not an ET_DYN shared library")
        found.extend(android_load_problems(data, phoff, phentsize, phnum))
    elif e_type != 3 or PT_INTERP not in kinds:
        found.append("not a PIE")
    stack = [s for s in segments if s[0] == PT_GNU_STACK]
    if not stack or stack[0][1] & 1:
        found.append("executable stack")
    if PT_GNU_RELRO not in kinds:
        found.append("no RELRO segment")
    found.extend(elf_dynamic_problems(data, segments, shared))
    return found


def elf_dynamic_problems(
    data: bytes, segments: list[Segment], shared: bool
) -> list[str]:
    found = []
    tags = dynamic_tags(data, segments)
    if shared and (22 in tags or tags.get(DT_FLAGS, 0) & 4):
        found.append("text relocations")
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


def android_load_problems(
    data: bytes, phoff: int, stride: int, count: int
) -> list[str]:
    found = []
    loads = 0
    for index in range(count):
        kind, flags, offset, vaddr, _, _, _, align = struct.unpack_from(
            "<IIQQQQQQ", data, phoff + index * stride
        )
        if kind != PT_LOAD:
            continue
        loads += 1
        if align < 16384 or align & (align - 1) or (vaddr - offset) % 16384:
            found.append("PT_LOAD not 16 KiB aligned")
        if flags & 3 == 3:
            found.append("writable executable PT_LOAD")
    if not loads:
        found.append("no PT_LOAD")
    return found


def apk_problems(binary: str | os.PathLike[str]) -> list[str]:
    expected = {"arm64-v8a": 183, "x86_64": 62}
    found = []
    with zipfile.ZipFile(binary) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)):
            return ["duplicate APK entries"]
        for abi in expected:
            if f"lib/{abi}/libdyapp_ffi.so" not in names:
                found.append(f"missing core library for {abi}")
        for name in names:
            if not (name.startswith("lib/") and name.endswith(".so")):
                continue
            parts = name.split("/")
            if len(parts) != 3 or parts[1] not in expected:
                found.append(f"{name}: unsupported native ABI")
                continue
            data = archive.read(name)
            if len(data) < 64:
                found.append(f"{name}: truncated ELF")
                continue
            machine = struct.unpack_from("<H", data, 18)[0]
            if machine != expected[parts[1]]:
                found.append(f"{name}: wrong ELF architecture")
            found.extend(
                f"{name}: {issue}" for issue in elf_problems(data, shared=True)
            )
    return found


FIX = "docs/manuals/bootstrap.md#getting-dyappd"


def main(raw: bool = False) -> None:
    arguments = sys.argv[1:]
    android_apk = "--android-apk" in arguments
    if android_apk:
        arguments.remove("--android-apk")
    (binary,) = arguments
    found = problems(binary, android_apk=android_apk)
    if found:
        sys.exit(f"{binary}: {'; '.join(found)}")
    with open(binary, "rb") as f:
        magic = f.read(4)
    if android_apk:
        profile = (
            "Android shared ELF, NX, full RELRO, no RPATH/textrel, "
            "canary, 16 KiB"
        )
    elif magic[:2] == b"MZ":
        profile = "PE32+, ASLR, high-entropy ASLR, DEP, relocations kept"
    elif magic == b"\xca\xfe\xba\xbe":
        profile = "arm64+x86_64, PIE, NX stack, no RPATH, stack protector"
    else:
        profile = "PIE, NX, full RELRO, no RPATH, stack protector"
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
            fix = (
                "docs/manuals/android.md#release-hardening"
                if "--android-apk" in sys.argv
                else FIX
            )
            print(f"FAIL hardening: {cause}\nFIX: {fix}\nDETAILS:")
            print(message[:65536])
            if log:
                print(f"{log}: raw captured output")
            sys.exit(1)
