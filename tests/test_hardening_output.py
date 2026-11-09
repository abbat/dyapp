"""Exercise the release hardening checker with synthetic ELF headers."""

from collections.abc import Iterable, Mapping
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from typing import TypedDict, Unpack

ROOT = Path(__file__).resolve().parents[1]


class ElfOptions(TypedDict, total=False):
    e_type: int
    interp: bool
    stack: int | None
    relro: bool
    dynamic: Mapping[int, int] | Iterable[tuple[int, int]]
    names: Iterable[bytes]


GOOD: ElfOptions = {
    "e_type": 3,
    "interp": True,
    "stack": 6,
    "relro": True,
    "dynamic": {30: 0x8},
    "names": [b"__stack_chk_fail"],
}


def elf(
    e_type: int = 3,
    interp: bool = True,
    stack: int | None = 6,
    relro: bool = True,
    dynamic: Mapping[int, int] | Iterable[tuple[int, int]] = (),
    names: Iterable[bytes] = (),
) -> bytes:
    """ELF64 headers, string table and dynamic section in one PT_LOAD."""
    strtab = b"\x00" + b"".join(name + b"\x00" for name in names)
    phnum = 3 + interp + (stack is not None) + relro
    strtab_at = 64 + 56 * phnum
    dynamic_at = strtab_at + len(strtab) + (-len(strtab) % 8)
    tags = {5: strtab_at, 10: len(strtab), **dict(dynamic)}
    dynamic_bytes = b"".join(struct.pack("<qQ", t, v) for t, v in tags.items())
    dynamic_bytes += struct.pack("<qQ", 0, 0)
    size = dynamic_at + len(dynamic_bytes)
    header = (
        b"\x7fELF\x02\x01\x01"
        + b"\x00" * 9
        + struct.pack(
            "<HHIQQQIHHHHHH",
            e_type,
            62,
            1,
            0,
            64,
            0,
            0,
            64,
            56,
            phnum,
            64,
            0,
            0,
        )
    )

    def segment(
        kind: int, flags: int = 4, offset: int = 0, filesz: int = 0
    ) -> bytes:
        return struct.pack(
            "<IIQQQQQQ", kind, flags, offset, offset, offset, filesz, filesz, 8
        )

    segments = segment(1, 5, 0, size)
    segments += segment(2, 6, dynamic_at, len(dynamic_bytes))
    segments += segment(6, 4, 64, 56 * phnum)
    if interp:
        segments += segment(3)
    if stack is not None:
        segments += segment(0x6474E551, stack)
    if relro:
        segments += segment(0x6474E552)
    body = header + segments + strtab
    return body + b"\x00" * (dynamic_at - len(body)) + dynamic_bytes


class Hardening(unittest.TestCase):
    def test_standalone_profile_and_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = root / "check.py"
            script.write_bytes(
                (ROOT / "tools/scripts/check-hardening.py").read_bytes()
            )
            env = dict(os.environ, DYAPP_CHECK_LOG_DIR=str(root / "logs"))

            def run(
                binary: Path, raw: bool = False
            ) -> subprocess.CompletedProcess[str]:
                return subprocess.run(
                    [
                        sys.executable,
                        str(script),
                        *(["--raw"] if raw else []),
                        str(binary),
                    ],
                    capture_output=True,
                    text=True,
                    env=env,
                    check=False,
                )

            def fixture(name: str, **change: Unpack[ElfOptions]) -> Path:
                path = root / name
                options: ElfOptions = {**GOOD, **change}
                path.write_bytes(elf(**options))
                return path

            variants: list[tuple[str, ElfOptions]] = [
                ("flags", {}),
                ("now-flag-1", {"dynamic": {0x6FFFFFFB: 1}}),
                ("bind-now", {"dynamic": {24: 0}}),
            ]
            for name, change in variants:
                result = run(fixture(name, **change))
                self.assertEqual(result.stdout, "OK hardening\n", name)
                self.assertEqual(result.returncode, 0, name)
            policy = json.loads(
                (ROOT / "tools/scripts/check-output.json").read_text()
            )
            bad: dict[str, tuple[ElfOptions, str]] = {
                "exec": ({"e_type": 2}, "not a PIE"),
                "no-interp": ({"interp": False}, "not a PIE"),
                "exec-stack": ({"stack": 7}, "executable stack"),
                "no-stack": ({"stack": None}, "executable stack"),
                "no-relro": ({"relro": False}, "no RELRO segment"),
                "lazy": ({"dynamic": {}}, "RELRO is partial"),
                "rpath": ({"dynamic": {30: 0x8, 15: 1}}, "RPATH or RUNPATH"),
                "runpath": ({"dynamic": {30: 0x8, 29: 1}}, "RPATH or RUNPATH"),
                "no-canary": (
                    {"names": [b"__stack_chk_fail_local"]},
                    "no stack protector",
                ),
            }
            for name, (change, cause) in bad.items():
                result = run(fixture(name, **change))
                self.assertEqual(result.returncode, 1, name)
                self.assertTrue(
                    result.stdout.startswith("FAIL hardening:"), name
                )
                self.assertIn(cause, result.stdout, name)
                self.assertIn(
                    f"FIX: {policy['hardening']['fix']}\n", result.stdout
                )
                self.assertIn(": raw captured output", result.stdout)
                legacy = run(root / name, raw=True)
                self.assertNotEqual(legacy.returncode, 0, name)
                self.assertIn(cause, legacy.stderr, name)
            not_elf = root / "text"
            not_elf.write_text("#!/bin/sh\n")
            self.assertIn("not a 64-bit", run(not_elf).stdout)
            logs = list((root / "logs").glob("hardening-*.log"))
            self.assertEqual(len(logs), len(bad) + 1)


if __name__ == "__main__":
    unittest.main()
